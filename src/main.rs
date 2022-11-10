use anyhow::{Context, Result};
use clap::Parser;
use regex::Regex;
use reqwest::StatusCode;
use std::{convert::Infallible, net::SocketAddr};
use url::Url;
use warp::{reply::with_status, Filter};

#[derive(clap::Parser)]
pub struct Arguments {
    /// On which address the server should listen.
    #[clap(long, env, default_value = "0.0.0.0:8080")]
    pub bind_address: SocketAddr,

    /// Where to read the dshackle metrics from.
    #[clap(long, env, default_value = "http://127.0.0.1:8081/metrics")]
    pub metrics_url: Url,

    /// How many blocks does a node have to lag behind to be considered unhealthy.
    #[clap(long, env, default_value = "5.0")]
    pub unhealthy_block_lag: f64,

    /// Name of the node this adapter monitors. This has to match with what is configured
    /// in `dshackle.yaml` as the node `id`.
    #[clap(long, env, default_value = "cow-nethermind")]
    pub node_id: String,

    /// Name of the block chain the monitored belongs to. This does not get configured in
    /// `dshackle.yaml`. Possible values: "ETH" and "GOERLI". Gnosis chain or xdai are not
    /// supported by `dshackle` and the other supported options we don't care about.
    #[clap(long, env, default_value = "ETH")]
    pub chain_id: String,
}

struct HealthReply(Result<bool>);

/// Turns the return value of `is_healthy()` into a `warp::Reply`.
impl warp::Reply for HealthReply {
    fn into_response(self) -> warp::reply::Response {
        let result = match self.0 {
            Ok(true) => with_status("OK".to_string(), StatusCode::OK),
            Ok(false) => with_status("lagging".to_string(), StatusCode::INTERNAL_SERVER_ERROR),
            Err(err) => with_status(err.to_string(), StatusCode::INTERNAL_SERVER_ERROR),
        };
        result.into_response()
    }
}

/// Requests the metrics from dshackle and parses the node lag from such a line:
/// dshackle_upstreams_lag{chain="ETH",upstream="cow-nethermind",} 0.0
/// Where "ETH"=`chain_id` and "cow-nethermind"=`node_id.
async fn is_healthy(args: &Arguments) -> Result<bool> {
    let metrics = reqwest::get(args.metrics_url.clone()).await?.text().await?;
    let regex_string = format!(
        "dshackle_upstreams_lag\\{{chain=\"{}\",upstream=\"{}\",\\}} (.*)",
        args.chain_id, args.node_id
    );
    let re = Regex::new(&regex_string).unwrap();
    for line in metrics.split('\n') {
        let capture_groups = match re.captures(line) {
            None => continue,
            Some(groups) => groups,
        };

        let lag: f64 = capture_groups
            .get(1)
            .context("regex contained no capture group")?
            .as_str()
            .parse()?;

        anyhow::ensure!(
            lag.is_finite() && lag.is_sign_positive(),
            "received a non-sensical lag value"
        );

        return Ok(lag <= args.unhealthy_block_lag);
    }

    Err(anyhow::anyhow!(
        "regex could not find the node lag within dshackle's metrics"
    ))
}

async fn get_health(args: &Arguments) -> Result<impl warp::Reply, Infallible> {
    Ok(HealthReply(is_healthy(args).await))
}

#[tokio::main]
async fn main() {
    let args = Box::new(Arguments::parse());
    let args = Box::leak(args);
    let health = warp::path("health").and_then(|| get_health(args));
    warp::serve(health).run(args.bind_address).await;
}
