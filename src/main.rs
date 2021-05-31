use futures::future::try_join_all;

use hyper::{
    header::CONTENT_TYPE,
    service::{make_service_fn, service_fn},
    Body, Request, Response, Server,
};

use prometheus::{Encoder, GaugeVec, TextEncoder};

use prometheus::register_gauge_vec;

use std::collections::HashMap;

use chrono::DateTime;
use chrono::prelude::*;

use std::path::Path;
use std::path::PathBuf;
use structopt::StructOpt;
use std::fs::File;
use config::*;

const USER_AGENT : &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 12_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/12.0 Mobile/15E148 Safari/604.1";

use lazy_static::lazy_static;

use serde::Deserialize;

#[derive(Debug, StructOpt)]
#[structopt(name = "rpromhub", about = "prometheus hub written in rust")]
struct Opt {
    #[structopt(parse(from_os_str))]
    input: PathBuf
}

lazy_static! {
    static ref GITHUB_BRANCH_AGE_GAUGE: GaugeVec = register_gauge_vec!(
        "github_repo_branch_age_days",
        "how long has the branch not been updated",
        &["owner", "repo", "branch"]
    )
        .unwrap();

    static ref SETTINGS: PromHubConfig = {
        let mut settings= config::Config::default();
        
        settings
            .merge(config::File::with_name("/etc/rpromhub/Settings")).unwrap();
        
        let c = settings.try_into::<PromHubConfig>().unwrap();

        c
    };
}

#[derive(Debug, Deserialize)]
struct BranchInfo {
    commit: CommitInfo
}

#[derive(Debug, Deserialize)]
struct CommitInfo {
    commit: CommitRecord
}

#[derive(Debug, Deserialize)]
struct CommitRecord {
    author: HashMap<String, String>
}

#[derive(Debug, Deserialize)]
struct RepoConfig {
    owner: String,
    repo: String,
    branch: Vec<String>
}

#[derive(Debug, Deserialize)]
struct PromHubConfig {
    repo: Vec<RepoConfig>,
    addr: String
}


async fn update_branch_age(owner: &str, repo: &str, branch: &str) -> Result<i64, Box<dyn std::error::Error>> {
    let url = format!("https://api.github.com/repos/{}/{}/branches/{}", owner, repo, branch);
    
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .build()?;
    
    let resp = client.get(url)
        .send()
        .await?
        .json::<BranchInfo>()
        .await?;

    let date_str = resp.commit.commit.author.get("date").unwrap();

    let date = DateTime::parse_from_rfc3339(date_str).unwrap();
    let utc_date : DateTime<Utc> = date.with_timezone(&Utc);

    let now: DateTime<Utc> = Utc::now();

    let diff = now - utc_date;

    let num_days = diff.num_days();

    GITHUB_BRANCH_AGE_GAUGE
        .with_label_values(&[owner, repo, branch])
        .set(num_days as f64);

    Ok(diff.num_days())
}

async fn job() -> Result<i32, Box<dyn std::error::Error>> {

    let mut futures = Vec::new();
    
    for repo in SETTINGS.repo.iter() {
        for branch in repo.branch.iter() {
            futures.push(update_branch_age(&repo.owner, &repo.repo, &branch));
        }
    }

    if let Err(e) = try_join_all(futures).await {
        eprintln!("Got error {}", e);
    }

    Ok(0)
}

async fn serve_req(_req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    let encoder = TextEncoder::new();

    let _ = job().await;

    let metric_families = prometheus::gather();
    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer).unwrap();
    
    let response = Response::builder()
        .status(200)
        .header(CONTENT_TYPE, encoder.format_type())
        .body(Body::from(buffer))
        .unwrap();

    Ok(response)
}

#[tokio::main]
async fn main() {

    let addr = &SETTINGS.addr;
        
    if let Ok(addr) = addr.parse() {
        println!("Listening on http://{}", addr);

        let serve_future = Server::bind(&addr).serve(make_service_fn(|_| async {
            Ok::<_, hyper::Error>(service_fn(serve_req))
        }));

        if let Err(err) = serve_future.await {
            eprintln!("server error: {}", err);
        }        
    } else {
        eprintln!("addr invalid: {}", addr);
    }
        
}
