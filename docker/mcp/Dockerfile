# Hosted MCP server, Rust build (docs/06-mcp-server.md). Build context is the repository root:
#   docker build -f docker/mcp/Dockerfile -t xxmi-mcp-rs .
# The image Railway deploys.

# Keep the tag equal to rust/rust-toolchain.toml; the C compiler in this image builds tree-sitter.
FROM rust:1.100-slim-trixie AS build
WORKDIR /repo
COPY . .
RUN cd rust && cargo build --release --locked -p xxmi-mcp

FROM debian:trixie-slim
# ripgrep backs search_source; git fetches the tracked repos; gosu drops root after fixing the
# volume's ownership; curl serves the health check.
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl git gosu ripgrep \
 && rm -rf /var/lib/apt/lists/* \
 && groupadd -g 1000 node && useradd -u 1000 -g node -m -s /bin/sh node \
 && ln -s /usr/sbin/gosu /usr/local/bin/su-exec
COPY --from=build /repo/rust/target/release/xxmi-mcp /usr/local/bin/xxmi-mcp
# The entrypoint calls su-exec, which gosu stands in for.
COPY docker/mcp/docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
ENV DATA_DIR=/data \
    PORT=3000 \
    TRUSTED_PROXY_HOPS=1
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s \
  CMD curl -fsS "http://127.0.0.1:${PORT}/healthz" >/dev/null || exit 1
ENTRYPOINT ["docker-entrypoint.sh"]
CMD ["xxmi-mcp"]
