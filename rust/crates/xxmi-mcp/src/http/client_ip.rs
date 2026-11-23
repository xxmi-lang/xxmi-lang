//! The address a request is attributed to, for rate limiting.

use std::net::SocketAddr;

use axum::extract::ConnectInfo;
use axum::extract::Request;
use axum::http::HeaderMap;

/// Longer than any address; a key this long is a header stuffed by the client.
const MAX_KEY_CHARS: usize = 64;

fn forwarded_chain(headers: &HeaderMap) -> Vec<String> {
    let joined: Vec<&str> = headers
        .get_all("x-forwarded-for")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .collect();
    joined
        .join(", ")
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(String::from)
        .collect()
}

/// The entry `hops` from the right of `X-Forwarded-For`:
/// each trusted proxy appends the address it saw,
/// so the entries to the left of that are client-controlled and never picked.
pub fn client_ip(headers: &HeaderMap, socket: Option<&str>, hops: usize) -> String {
    let fallback = socket.unwrap_or("unknown").to_string();
    if hops == 0 {
        return fallback;
    }

    let chain = forwarded_chain(headers);
    chain
        .len()
        .checked_sub(hops)
        .and_then(|at| chain.get(at).cloned())
        .filter(|entry| entry.len() <= MAX_KEY_CHARS)
        .unwrap_or(fallback)
}

pub fn socket_ip(request: &Request) -> Option<String> {
    let ConnectInfo(addr) = request.extensions().get::<ConnectInfo<SocketAddr>>()?;
    Some(addr.ip().to_string())
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    fn forwarded(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_str(value).unwrap());
        headers
    }

    #[test]
    fn picks_the_nth_entry_from_the_right() {
        let headers = forwarded("6.6.6.6, 1.1.1.1, 2.2.2.2");
        assert_eq!(client_ip(&headers, Some("9.9.9.9"), 1), "2.2.2.2");
        assert_eq!(client_ip(&headers, Some("9.9.9.9"), 2), "1.1.1.1");
    }

    #[test]
    fn a_spoofed_leftmost_entry_changes_nothing() {
        let honest = forwarded("1.1.1.1");
        let spoofed = forwarded("6.6.6.6, 1.1.1.1");
        assert_eq!(client_ip(&honest, None, 1), client_ip(&spoofed, None, 1));
    }

    #[test]
    fn an_oversized_entry_is_not_a_key() {
        let headers = forwarded(&"9".repeat(65));

        assert_eq!(client_ip(&headers, Some("9.9.9.9"), 1), "9.9.9.9");
    }

    #[test]
    fn falls_back_to_the_socket() {
        assert_eq!(
            client_ip(&forwarded("1.1.1.1"), Some("9.9.9.9"), 0),
            "9.9.9.9"
        );
        assert_eq!(
            client_ip(&forwarded("1.1.1.1"), Some("9.9.9.9"), 2),
            "9.9.9.9"
        );
        assert_eq!(client_ip(&HeaderMap::new(), None, 1), "unknown");
    }
}
