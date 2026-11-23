//! Hosted MCP server: the Rust port of packages/mcp-server (docs/06-mcp-server.md).

pub mod app;
pub mod bundled;
pub mod cancel;
pub mod catalog;
pub mod config;
pub mod exec;
pub mod http;
pub mod indexer;
pub mod jsonrpc;
pub mod logger;
pub mod mcp;
pub mod packages;
pub mod ratelimit;
pub mod repos;
pub mod schema;
pub mod selection;
pub mod serve;
pub mod sources;
pub mod spec_build;
pub mod spec_ensure;
pub mod spec_store;
pub mod specs;
pub mod store;
pub mod tools;
pub mod upstream;
pub mod zod;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
