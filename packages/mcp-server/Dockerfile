# Hosted MCP server (docs/06-mcp-server.md). Build context is the repository root:
#   docker build -f packages/mcp-server/Dockerfile -t xxmi-mcp .

FROM node:22-alpine AS build-node

FROM build-node AS build
WORKDIR /repo
RUN corepack enable
COPY . .
# Only the server and the packages it depends on (core); the rest of the workspace is skipped.
RUN pnpm install --frozen-lockfile --filter "@xxmi-lang/mcp-server..." \
 && pnpm --filter "@xxmi-lang/mcp-server..." build \
 && pnpm --filter @xxmi-lang/mcp-server deploy --prod --legacy /out

# Runtime: plain Alpine plus the Node binary from the official image. The Node image's
# own layers (npm, yarn, headers) would add ~90 MB that a server doesn't use. Keep the Alpine
# version equal to the Node image's, so libstdc++ matches what the binary was built with.
FROM alpine:3.24
# ripgrep backs search_source; su-exec drops root after fixing the volume's ownership.
RUN apk add --no-cache libstdc++ ripgrep su-exec \
 && addgroup -g 1000 node && adduser -u 1000 -G node -s /bin/sh -D node
COPY --from=build-node /usr/local/bin/node /usr/local/bin/node
WORKDIR /app
COPY --from=build /out/ ./
COPY packages/mcp-server/docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
# The data volume is mounted at DATA_DIR by the host (Railway volume or `docker run -v`).
ENV NODE_ENV=production \
    DATA_DIR=/data \
    PORT=3000 \
    TRUSTED_PROXY_HOPS=1
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s \
  CMD node -e "fetch('http://127.0.0.1:'+(process.env.PORT||3000)+'/healthz').then(r=>process.exit(r.ok?0:1),()=>process.exit(1))"
ENTRYPOINT ["docker-entrypoint.sh"]
# --liftoff-only: web-tree-sitter leaks native memory under V8's optimizing compiler
# (packages/core/src/runtime.ts). Passing it here avoids the self-relaunch in main.ts.
CMD ["node", "--liftoff-only", "dist/main.js"]
