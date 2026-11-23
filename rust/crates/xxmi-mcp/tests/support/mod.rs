//! A fixture data volume and an in-process server over it.

#![allow(dead_code)]

use std::fs;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;
use xxmi_mcp::app::{AppState, router};
use xxmi_mcp::catalog::Catalog;
use xxmi_mcp::config::Config;
use xxmi_mcp::indexer::{Indexer, PassResult};
use xxmi_mcp::logger::Logger;
use xxmi_mcp::packages::PackageCache;
use xxmi_mcp::specs::{NoVersionSpecs, SpecRegistry};
use xxmi_mcp::store::Store;
use xxmi_mcp::tools::{BlockingGate, ToolContext};

const SHA_HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SHA_TAG: &str = "cccccccccccccccccccccccccccccccccccccccc";

/// Owner, name, alias, fixture directory, default branch, tag.
type Repo = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    Option<&'static str>,
);

const REPOS: [Repo; 4] = [
    (
        "SpectrumQT",
        "XXMI-Libs-Package",
        "dll",
        "dll",
        "master",
        None,
    ),
    (
        "leotorrez",
        "ZZMI-Package",
        "zzmi",
        "zzmi-package",
        "main",
        Some("v1.0.0"),
    ),
    (
        "leotorrez",
        "XXMITools",
        "xxmitools",
        "xxmitools",
        "main",
        None,
    ),
    ("bo3b", "3Dmigoto", "migoto", "dll", "master", None),
];

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("create target dir");
    for entry in fs::read_dir(from).expect("read fixture dir") {
        let entry = entry.expect("fixture entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).expect("copy fixture file");
        }
    }
}

fn version(name: &str, sha: &str, kind: &str) -> Value {
    json!({ "name": name, "sha": sha, "kind": kind })
}

/// Lays out the volume the indexer leaves behind: checkouts of the test fixtures and state.json.
pub fn make_volume(dir: &Path) {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut repos = serde_json::Map::new();
    for (owner, name, alias, fixture, branch, tag) in REPOS {
        let shas = if tag.is_some() {
            vec![SHA_HEAD, SHA_TAG]
        } else {
            vec![SHA_HEAD]
        };
        for sha in shas {
            copy_dir(
                &fixtures.join(fixture),
                &dir.join("repos").join(owner).join(name).join(sha),
            );
        }
        let tags: Vec<Value> = tag
            .map(|t| version(t, SHA_TAG, "tag"))
            .into_iter()
            .collect();
        repos.insert(
            alias.into(),
            json!({ "branch": version(branch, SHA_HEAD, "head"), "tags": tags, "indexedAt": "2026-01-01T00:00:00.000Z" }),
        );
    }
    let state = json!({ "version": 1, "repos": repos, "releases": {} });
    fs::write(dir.join("state.json"), state.to_string()).expect("write state");
}

pub fn config(dir: &Path, env: &[(&str, &str)]) -> Config {
    let data_dir = dir.to_string_lossy().to_string();
    Config::from_lookup(&|key| match key {
        "DATA_DIR" => Some(data_dir.clone()),
        "INDEXER" => Some("off".into()),
        other => env
            .iter()
            .find(|(k, _)| *k == other)
            .map(|(_, v)| v.to_string())
            .or_else(|| (other == "RATE_LIMIT_PER_MIN").then(|| "0".into())),
    })
    .expect("valid test config")
}

/// The tests serve a fixed volume: nothing is fetched.
struct NoIndexer;

impl Indexer for NoIndexer {
    fn refresh(&self) -> Pin<Box<dyn Future<Output = PassResult> + Send + '_>> {
        Box::pin(async { PassResult::default() })
    }
}

pub async fn app(dir: &Path, env: &[(&str, &str)]) -> Router {
    app_with_state(dir, env).await.0
}

/// The router and the state behind it, for tests that reach into the limits.
pub async fn app_with_state(dir: &Path, env: &[(&str, &str)]) -> (Router, Arc<AppState>) {
    make_volume(dir);
    let config = config(dir, env);
    let ctx = ToolContext {
        catalog: Arc::new(Catalog::load(Store::new(&config.data_dir)).await),
        config,
        logger: Logger::default(),
        specs: SpecRegistry::new(Box::new(NoVersionSpecs)),
        packages: PackageCache::default(),
        blocking: BlockingGate::default(),
    };
    let state = AppState::new(ctx, Arc::new(NoIndexer));
    (router(state.clone()), state)
}

pub struct Reply {
    pub status: StatusCode,
    pub body: Value,
}

pub async fn send(app: &Router, request: Request<Body>) -> Reply {
    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("infallible service");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let body = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into()));
    Reply { status, body }
}

pub fn mcp_request(body: &Value) -> Request<Body> {
    Request::post("/mcp")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .body(Body::from(body.to_string()))
        .expect("request")
}

pub async fn call(app: &Router, tool: &str, arguments: Value) -> Value {
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": tool, "arguments": arguments } });
    send(app, mcp_request(&body)).await.body["result"].clone()
}

/// The JSON document a `format: "json"` call returns as its text.
pub fn data_of(result: &Value) -> Value {
    serde_json::from_str(&text_of(result)).unwrap_or(Value::Null)
}

pub fn text_of(result: &Value) -> String {
    result["content"][0]["text"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}
