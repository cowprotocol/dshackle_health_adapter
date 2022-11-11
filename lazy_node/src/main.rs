mod rpc;

use self::rpc::{eth, Id, Method, Request, Response};
use anyhow::{ensure, Result};
use clap::Parser;
use reqwest::{header, Client, StatusCode};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::{
    convert::Infallible,
    net::SocketAddr,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
use url::Url;
use warp::Filter;

#[derive(Debug, Parser)]
struct Arguments {
    /// Log filter to use
    #[clap(long, env, default_value = "warn,lazy_node=debug")]
    log_filter: String,

    /// On which address the server should listen.
    #[clap(long, env, default_value = "0.0.0.0:9545")]
    bind_address: SocketAddr,

    /// Upstream JSON RPC node to proxy.
    #[clap(long, env, default_value = "http://127.0.0.1:8545")]
    node_url: Url,

    /// Chance to update a new block.
    #[clap(long, env, default_value = "0.25")]
    update_chance: f64,
}

struct Context {
    client: Client,
    url: Url,
    update_chance: f64,
    block: AtomicU64,
}

impl Context {
    async fn block_number(&self, request: Request<eth::BlockNumber>) -> Response<eth::BlockNumber> {
        tracing::trace!(?request, "block number");

        let roll = rand::random::<f64>();
        if roll < self.update_chance || self.block.load(Ordering::SeqCst) == 0 {
            match self.call(eth::BlockNumber, []).await {
                Ok(block) => {
                    tracing::info!(%block, "updated block number");
                    self.block.store(block, Ordering::SeqCst)
                }
                Err(err) => tracing::warn!(?err, "error updating latest block"),
            }
        }

        Response::new(request, self.block.load(Ordering::SeqCst))
    }

    async fn proxy_request(&self, request: Value) -> Result<Value> {
        self.exec(&request).await
    }

    async fn exec<R, S>(&self, request: &R) -> Result<S>
    where
        R: Serialize,
        S: DeserializeOwned,
    {
        static UID: AtomicU64 = AtomicU64::new(0);

        let uid = UID.fetch_add(1, Ordering::SeqCst);
        let body = serde_json::to_string(request)?;
        tracing::trace!(%uid, %body, ">");

        let response = self
            .client
            .post(self.url.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await?;

        let status = response.status();
        let body = response.text().await?;
        tracing::trace!(%uid, %body, ?status, "<");
        ensure!(status.is_success(), "{body}");

        let response = serde_json::from_str(&body)?;
        Ok(response)
    }

    async fn call<M>(&self, method: M, params: M::Params) -> Result<M::Result>
    where
        M: Method + Serialize,
        M::Params: Serialize,
    {
        let response: Response<M> = self
            .exec(&Request {
                jsonrpc: rpc::JsonRpc::V2,
                method,
                params,
                id: Id::Number(1337),
            })
            .await?;
        Ok(response.result)
    }
}

#[tokio::main]
async fn main() {
    let args = Arguments::parse();
    tracing_subscriber::fmt()
        .with_ansi(atty::is(atty::Stream::Stdout))
        .with_env_filter(&args.log_filter)
        .init();

    tracing::info!("starting with {args:#?}");

    let context = Arc::new(Context {
        client: Client::new(),
        url: args.node_url,
        update_chance: args.update_chance,
        block: AtomicU64::new(0),
    });

    let proxy = warp::post()
        .and(warp::body::json())
        .and_then(move |request: Value| {
            let context = context.clone();
            async move {
                let json = match Request::<eth::BlockNumber>::deserialize(request.clone()) {
                    Ok(request) => Ok(warp::reply::json(&context.block_number(request).await)),
                    Err(_) => context
                        .proxy_request(request)
                        .await
                        .map(|response| warp::reply::json(&response)),
                };

                let reply = match json {
                    Ok(response) => warp::reply::with_status(response, StatusCode::OK),
                    Err(err) => {
                        tracing::warn!(?err, "error proxying request");
                        warp::reply::with_status(
                            warp::reply::json(&err.to_string()),
                            StatusCode::INTERNAL_SERVER_ERROR,
                        )
                    }
                };
                Result::<_, Infallible>::Ok(reply)
            }
        });

    warp::serve(proxy).run(args.bind_address).await;
}
