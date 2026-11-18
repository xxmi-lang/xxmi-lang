//! HTTP plumbing around the endpoints: routes, client identity, rate limiting, admin, logging.

pub mod admin;
pub mod client_ip;
pub mod log;
pub mod rate_limit;
pub mod routes;
