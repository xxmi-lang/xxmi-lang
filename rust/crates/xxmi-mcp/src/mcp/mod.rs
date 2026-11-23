//! The `/mcp` endpoint:
//! stateless JSON-RPC over HTTP.

mod initialize;
mod methods;
mod params;
mod post;
mod resources;

pub use post::{allow_post, handle, json_response};
