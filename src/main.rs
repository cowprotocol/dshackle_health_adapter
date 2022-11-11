use anyhow::{Context, Result};
use clap::Parser;
use regex::Regex;
use reqwest::StatusCode;
use std::{
    convert::Infallible,
    fmt::{self, Display, Formatter},
    net::SocketAddr,
};
use url::Url;
use warp::{reply::with_status, Filter};

#[derive(clap::Parser, Debug)]
pub struct Arguments {
    /// On which address the server should listen.
    #[clap(long, env, default_value = "0.0.0.0:8080")]
    pub bind_address: SocketAddr,

    /// Where to read the dshackle detailed health info from.
    #[clap(long, env, default_value = "http://127.0.0.1:8082/health?detailed")]
    pub health_url: Url,

    /// Name of the node this adapter monitors. This has to match with what is configured
    /// in `dshackle.yaml` as the node `id`.
    #[clap(long, env, default_value = "cow-nethermind")]
    pub node_id: String,

    /// How many blocks a node may lag behind before being considered unhealthy.
    /// If this value is unset we simply use whatever dshackle reports.
    #[clap(long, env)]
    pub unhealthy_lag: Option<u64>,
}

struct HealthReply(Result<(Status, u64)>);

/// Turns the return value of `is_healthy()` into a `warp::Reply`.
impl warp::Reply for HealthReply {
    fn into_response(self) -> warp::reply::Response {
        let result = match self.0 {
            Ok((status, lag)) => with_status(format!("{status}({lag})"), status.http_status_code()),
            Err(err) => with_status(err.to_string(), StatusCode::INTERNAL_SERVER_ERROR),
        };
        result.into_response()
    }
}

#[derive(Debug)]
enum Status {
    Ok,
    Lagging,
    Unavailable,
    Other(String),
}

impl Status {
    fn http_status_code(&self) -> StatusCode {
        match self {
            Self::Ok => StatusCode::OK,
            _ => StatusCode::SERVICE_UNAVAILABLE,
        }
    }
}

impl Display for Status {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.write_str(match self {
            Self::Ok => "OK",
            Self::Lagging => "LAGGING",
            Self::Unavailable => "UNAVAILABLE",
            Self::Other(message) => message,
        })
    }
}

/// Requests the metrics from dshackle and parses the node lag from such a line:
/// dshackle_upstreams_lag{chain="ETH",upstream="cow-nethermind",} 0.0
/// Where "ETH"=`chain_id` and "cow-nethermind"=`node_id.
async fn is_healthy(args: &Arguments) -> Result<(Status, u64)> {
    let response = reqwest::get(args.health_url.clone()).await?.text().await?;
    println!("dshackle responded:\n{response}");
    let regex_string = format!("{} (.*) with lag=(.*)", args.node_id);
    let re = Regex::new(&regex_string).unwrap();
    for line in response.split('\n') {
        let capture_groups = match re.captures(line) {
            None => continue,
            Some(groups) => groups,
        };

        let status = match capture_groups
            .get(1)
            .context("missing status capture group")?
            .as_str()
        {
            "OK" => Status::Ok,
            "LAGGING" => Status::Lagging,
            "UNAVAILABLE" => Status::Unavailable,
            other => Status::Other(other.to_owned()),
        };
        let lag: u64 = capture_groups
            .get(2)
            .context("missing lag capture group")?
            .as_str()
            .parse()?;

        let result = match (status, args.unhealthy_lag) {
            (Status::Ok | Status::Lagging, Some(max_lag)) => {
                // Adjust the status based on our internal lagging parameter.
                if lag > max_lag {
                    Ok((Status::Lagging, lag))
                } else {
                    Ok((Status::Ok, lag))
                }
            }
            // use whatever `dshackle` reports
            (status, _) => Ok((status, lag)),
        };
        return result;
    }

    Err(anyhow::anyhow!(
        "regex could not find the node lag within dshackle's detailed health endpoint"
    ))
}

async fn get_health(args: &Arguments) -> Result<impl warp::Reply, Infallible> {
    let response = is_healthy(args).await;
    println!("returning /health response: {response:?}");
    Ok(HealthReply(response))
}

#[tokio::main]
async fn main() {
    let args = Box::new(Arguments::parse());
    let args = Box::leak(args);
    let health = warp::path("health").and_then(|| get_health(args));
    println!("starting dshackle_health_adapter with validated arguments: {args:#?}");
    warp::serve(health).run(args.bind_address).await;
}
