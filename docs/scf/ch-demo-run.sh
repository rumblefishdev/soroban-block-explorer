#!/usr/bin/env bash
#
# Open clickhouse-client inside the production CH container on
# sorban-prod. Use during the SCF Milestone 1 video recording for
# Scene 6 queries (see ch-demo-queries.sql).
#
# RUN THIS SCRIPT *ON* THE HETZNER HOST after `ssh sorban-prod`.
# From within the container the default user has no password
# (mTLS auth applies only to traffic via Caddy, not to local
# docker-exec), so the call is short.
#
# Usage:
#   sudo /path/to/ch-demo-run.sh                  # interactive REPL
#   sudo /path/to/ch-demo-run.sh --file <sql>     # run a script

set -euo pipefail

readonly CONTAINER="app-clickhouse-1"

# Sanity checks — fail loud if anything is missing.
command -v docker >/dev/null || { echo "❌ docker not in PATH"; exit 1; }
if ! docker ps --format '{{.Names}}' | grep -qx "$CONTAINER"; then
    echo "❌ container '$CONTAINER' is not running."
    echo "   docker ps   (then check the compose stack at /srv/app)"
    exit 1
fi

if [[ "${1:-}" == "--file" ]]; then
    [[ -n "${2:-}" ]] || { echo "usage: $0 --file <path-to-sql>"; exit 2; }
    SQL_FILE="$2"
    [[ -f "$SQL_FILE" ]] || { echo "❌ SQL file not found: $SQL_FILE"; exit 1; }
    echo "→ Running $SQL_FILE inside '$CONTAINER'…"
    # -i (stdin, no TTY) + --multiquery for batch run.
    docker exec -i "$CONTAINER" \
        clickhouse-client --multiquery < "$SQL_FILE"
else
    echo "→ Opening clickhouse-client REPL inside '$CONTAINER'."
    echo "   Type \\q (or Ctrl-D) to exit."
    echo ""
    docker exec -it "$CONTAINER" clickhouse-client
fi
