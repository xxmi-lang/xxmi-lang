//! The `/mcp` endpoint: stateless JSON-RPC over HTTP, answering the way the SDK transport of the
//! TypeScript server does.

mod initialize;
mod methods;
mod params;
mod post;
mod resources;

pub use post::{handle, json_response};
