#!/bin/sh
# Railway mounts volumes owned by root. Make the data directory writable for the unprivileged
# `node` user, then run the server as that user. Started as a non-root user (other platforms),
# it just runs the command.
set -e
if [ "$(id -u)" = "0" ]; then
  mkdir -p "$DATA_DIR"
  chown node:node "$DATA_DIR"
  exec su-exec node "$@"
fi
exec "$@"
