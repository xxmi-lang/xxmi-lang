//! The accept loop: axum's own `serve` cannot set connection timeouts.

use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::pin::pin;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::{TokioIo, TokioTimer};
use hyper_util::server::graceful::GracefulShutdown;
use serde_json::json;
use tokio::net::{TcpListener, TcpStream};
use tower::ServiceExt;

use crate::logger::Logger;

/// How long a client may take to send a request head, which also bounds a keep-alive connection
/// that stays silent; Node's `headersTimeout` of the TypeScript server.
pub const HEADER_READ_TIMEOUT: Duration = Duration::from_secs(20);

/// Connections still busy this long after a shutdown signal are dropped.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(30);

/// Pause after a failed accept (usually out of file descriptors) so the loop does not spin.
const ACCEPT_BACKOFF: Duration = Duration::from_millis(50);

/// Serves `app` until `shutdown` resolves, then lets in-flight requests finish.
pub async fn serve(
    listener: TcpListener,
    app: Router,
    header_read_timeout: Duration,
    logger: Logger,
    shutdown: impl Future<Output = ()>,
) -> io::Result<()> {
    let graceful = GracefulShutdown::new();
    let mut shutdown = pin!(shutdown);

    loop {
        let accepted = tokio::select! {
            accepted = listener.accept() => accepted,
            () = &mut shutdown => break,
        };
        match accepted {
            Ok((stream, peer)) => {
                spawn_connection(&graceful, stream, peer, app.clone(), header_read_timeout);
            }
            Err(error) => {
                logger.warn("accept failed", json!({ "error": error.to_string() }));
                tokio::time::sleep(ACCEPT_BACKOFF).await;
            }
        }
    }

    let _ = tokio::time::timeout(SHUTDOWN_GRACE, graceful.shutdown()).await;
    Ok(())
}

fn spawn_connection(
    graceful: &GracefulShutdown,
    stream: TcpStream,
    peer: SocketAddr,
    app: Router,
    header_read_timeout: Duration,
) {
    let service = service_fn(move |mut request: hyper::Request<Incoming>| {
        let app = app.clone();
        async move {
            request.extensions_mut().insert(ConnectInfo(peer));
            app.oneshot(request.map(Body::new)).await
        }
    });
    let connection = http1::Builder::new()
        .timer(TokioTimer::new())
        .header_read_timeout(header_read_timeout)
        .serve_connection(TokioIo::new(stream), service);
    let watched = graceful.watch(connection);

    tokio::spawn(async move {
        let _ = watched.await;
    });
}
