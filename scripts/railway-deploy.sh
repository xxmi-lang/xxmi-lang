#!/usr/bin/env bash
# Deploys one commit of the hosted MCP server to Railway.
# Waits for the deployment to succeed and for /healthz to report ready.
# Called from .github/workflows/deploy.yml;
# the token comes from the environment only and is never printed.
#   usage: scripts/railway-deploy.sh <40-hex commit sha>
set -euo pipefail

readonly RAILWAY_API=https://backboard.railway.com/graphql/v2
readonly POLL_SECONDS=10
readonly POLL_ATTEMPTS=90
readonly HEALTH_SECONDS=5
readonly HEALTH_ATTEMPTS=12

die() {
  echo "error: $*" >&2
  exit 1
}

require_env() {
  : "${RAILWAY_PROJECT_TOKEN:?RAILWAY_PROJECT_TOKEN is not set}"
  : "${RAILWAY_SERVICE_ID:?RAILWAY_SERVICE_ID is not set}"
  : "${RAILWAY_ENVIRONMENT_ID:?RAILWAY_ENVIRONMENT_ID is not set}"
  : "${MCP_URL:?MCP_URL is not set}"
}

check_sha() {
  [[ "$1" =~ ^[0-9a-f]{40}$ ]] || die "not a full commit sha: '$1'"
}

# Sends one GraphQL document to Railway and prints the JSON response.
graphql() {
  local body
  body=$(jq -n --arg query "$1" '{query: $query}')
  curl -fsS --max-time 30 "$RAILWAY_API" \
    -H 'content-type: application/json' \
    -H "Project-Access-Token: $RAILWAY_PROJECT_TOKEN" \
    -d "$body"
}

# Stops on a GraphQL error; the messages come from Railway, the token is not part of them.
check_graphql() {
  if jq -e '.errors' >/dev/null <<<"$1"; then
    die "Railway API: $(jq -r '[.errors[].message] | join("; ")' <<<"$1")"
  fi
}

request_deploy() {
  local response id
  response=$(graphql "mutation { serviceInstanceDeployV2(serviceId: \"$RAILWAY_SERVICE_ID\", environmentId: \"$RAILWAY_ENVIRONMENT_ID\", commitSha: \"$1\") }")
  check_graphql "$response"

  id=$(jq -r '.data.serviceInstanceDeployV2 // empty' <<<"$response")
  [[ -n "$id" ]] || die "Railway returned no deployment id"
  echo "$id"
}

deployment_status() {
  local response
  response=$(graphql "query { deployment(id: \"$1\") { status } }")
  check_graphql "$response"
  jq -r '.data.deployment.status // "UNKNOWN"' <<<"$response"
}

wait_for_deployment() {
  local id=$1 status="" attempt
  for ((attempt = 1; attempt <= POLL_ATTEMPTS; attempt++)); do
    status=$(deployment_status "$id")
    echo "deployment $id: $status"

    case "$status" in
      SUCCESS) return 0 ;;
      FAILED | CRASHED | REMOVED | SKIPPED) die "deployment $id ended as $status" ;;
    esac
    sleep "$POLL_SECONDS"
  done
  die "deployment $id still $status after $((POLL_ATTEMPTS * POLL_SECONDS)) s"
}

wait_for_health() {
  local attempt
  for ((attempt = 1; attempt <= HEALTH_ATTEMPTS; attempt++)); do
    if curl -fsS --max-time 30 "$MCP_URL/healthz" | jq -e '.ready == true' >/dev/null; then
      echo "healthy: $MCP_URL/healthz reports ready"
      return 0
    fi
    sleep "$HEALTH_SECONDS"
  done
  die "$MCP_URL/healthz is not ready after $((HEALTH_ATTEMPTS * HEALTH_SECONDS)) s"
}

main() {
  local sha=${1:-} id

  require_env
  check_sha "$sha"

  id=$(request_deploy "$sha")
  echo "deploying $sha as $id"

  wait_for_deployment "$id"
  wait_for_health
}

main "$@"
