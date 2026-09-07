#!/usr/bin/env bash
# ──────────────────────────────────────────────────────────────────
# rds-iam-launch.sh — Launch Agent Commons with RDS IAM auth
#
# Generates a short-lived IAM auth token, URL-encodes it into
# AC_DATABASE_URL, and starts the agent-commons binary.
#
# Usage:
#   RDSHOST=database-1.cluster-xxx.us-east-1.rds.amazonaws.com ./rds-iam-launch.sh
#
#   # Long session — restarts binary every 12 min with a fresh token
#   RDSHOST=... ./rds-iam-launch.sh --loop
#
# Environment variables (override defaults):
#   RDSHOST     — (required) RDS endpoint hostname
#   RDSPORT     — default 5432
#   RDSUSER     — default postgres
#   RDSDB       — default aishelter
#   AWS_REGION  — default us-east-1
#   AC_BINARY   — path to agent-commons binary
# ──────────────────────────────────────────────────────────────────
set -euo pipefail

RDSHOST="${RDSHOST:?'RDSHOST environment variable is required'}"
RDSPORT="${RDSPORT:-5432}"
RDSUSER="${RDSUSER:-postgres}"
RDSDB="${RDSDB:-aishelter}"
REGION="${AWS_REGION:-us-east-1}"
TOKEN_LIFETIME_MINS="${TOKEN_LIFETIME_MINS:-12}"
LOOP=false

for arg in "$@"; do
  case "$arg" in
    --loop) LOOP=true ;;
  esac
done

# ── Locate binary ──────────────────────────────────────────────────
if [ -z "${AC_BINARY:-}" ]; then
  SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
  for candidate in \
    "$SCRIPT_DIR/../target/release/agent-commons" \
    "$SCRIPT_DIR/../target/debug/agent-commons"; do
    if [ -x "$candidate" ]; then
      AC_BINARY="$(realpath "$candidate")"
      break
    fi
  done
  if [ -z "${AC_BINARY:-}" ]; then
    echo "ERROR: Cannot find agent-commons binary. Build or set AC_BINARY." >&2
    exit 1
  fi
fi
echo "Binary: $AC_BINARY"

# ── Helpers ────────────────────────────────────────────────────────
generate_token() {
  echo "Generating RDS IAM auth token..." >&2
  aws rds generate-db-auth-token \
    --hostname "$RDSHOST" \
    --port "$RDSPORT" \
    --username "$RDSUSER" \
    --region "$REGION"
}

url_encode() {
  # The IAM token contains /, =, + which break postgresql:// URIs.
  python3 -c "import urllib.parse, sys; print(urllib.parse.quote(sys.stdin.read().strip(), safe=''))"
}

# ── Main loop ──────────────────────────────────────────────────────
while true; do
  TOKEN="$(generate_token)"
  ENCODED="$(echo "$TOKEN" | url_encode)"

  export AC_DATABASE_URL="postgresql://${RDSUSER}:${ENCODED}@${RDSHOST}:${RDSPORT}/${RDSDB}?sslmode=require"

  # Mask token in log output
  MASKED="$(echo "$AC_DATABASE_URL" | sed "s/:${ENCODED}@/:***@/")"
  echo "AC_DATABASE_URL = $MASKED"

  echo "Starting agent-commons..."
  "$AC_BINARY" || true

  if [ "$LOOP" = false ]; then
    break
  fi

  echo "Binary exited. Refreshing token in ${TOKEN_LIFETIME_MINS} minutes..."
  sleep $((TOKEN_LIFETIME_MINS * 60))
done
