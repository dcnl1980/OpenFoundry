#!/usr/bin/env bash
# Start every OpenFoundry service against per-service databases as openfoundry_app.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LOGDIR="${OF_E2E_LOGDIR:-/tmp/of-e2e/logs}"
BIN="${ROOT}/target/debug"
ADMIN_URL="${MIGRATION_DATABASE_URL:-postgres://openfoundry:openfoundry@127.0.0.1:5432/openfoundry}"
JWT_SECRET="${JWT_SECRET:-change-me-in-production-use-a-256-bit-key}"
SESSION="${OF_TMUX_SESSION:-of-platform}"

mkdir -p "$LOGDIR" /tmp/of-e2e/datasets /tmp/of-e2e/notebooks /tmp/of-e2e/pipelines

psql "$ADMIN_URL" -v ON_ERROR_STOP=1 <<'SQL'
DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'openfoundry_app') THEN
    CREATE ROLE openfoundry_app LOGIN PASSWORD 'openfoundry' NOSUPERUSER NOBYPASSRLS;
  END IF;
END
$$;
SQL

prepare_db() {
  local db="$1"
  if ! psql "$ADMIN_URL" -tAc "SELECT 1 FROM pg_database WHERE datname = '${db}'" | grep -q 1; then
    psql "$ADMIN_URL" -v ON_ERROR_STOP=1 -c "CREATE DATABASE ${db}"
  fi
  psql "$ADMIN_URL" -v ON_ERROR_STOP=1 -c "GRANT CONNECT ON DATABASE ${db} TO openfoundry_app;"
  local db_url
  db_url="$(echo "$ADMIN_URL" | sed "s#/[^/]*\$#/${db}#")"
  psql "$db_url" -v ON_ERROR_STOP=1 <<SQL
GRANT USAGE, CREATE ON SCHEMA public TO openfoundry_app;
ALTER DEFAULT PRIVILEGES FOR ROLE openfoundry IN SCHEMA public GRANT ALL ON TABLES TO openfoundry_app;
ALTER DEFAULT PRIVILEGES FOR ROLE openfoundry IN SCHEMA public GRANT ALL ON SEQUENCES TO openfoundry_app;
ALTER DEFAULT PRIVILEGES FOR ROLE openfoundry IN SCHEMA public GRANT EXECUTE ON FUNCTIONS TO openfoundry_app;
SQL
}

# name:port:dbname
SERVICES=(
  "auth-service:50051:ofe2e_auth"
  "data-connector:50152:ofe2e_connector"
  "dataset-service:50053:ofe2e_dataset"
  "streaming-service:50054:ofe2e_streaming"
  "query-service:50055:ofe2e_query"
  "pipeline-service:50056:ofe2e_pipeline"
  "ontology-service:50057:ofe2e_ontology"
  "fusion-service:50058:ofe2e_fusion"
  "ml-service:50059:ofe2e_ml"
  "ai-service:50060:ofe2e_ai"
  "workflow-service:50061:ofe2e_workflow"
  "notebook-service:50062:ofe2e_notebook"
  "app-builder-service:50063:ofe2e_apps"
  "report-service:50064:ofe2e_report"
  "code-repo-service:50065:ofe2e_code"
  "marketplace-service:50066:ofe2e_market"
  "nexus-service:50067:ofe2e_nexus"
  "geospatial-service:50068:ofe2e_geo"
  "notification-service:50069:ofe2e_notify"
  "audit-service:50070:ofe2e_audit"
)

for spec in "${SERVICES[@]}"; do
  prepare_db "${spec##*:}"
done

# Drop leftover listeners from earlier agent sessions so health checks hit this stack.
for leftover in auth-service gateway; do
  if tmux -f /exec-daemon/tmux.portal.conf has-session -t "=$leftover" 2>/dev/null; then
    tmux -f /exec-daemon/tmux.portal.conf kill-session -t "$leftover"
  fi
done
if tmux -f /exec-daemon/tmux.portal.conf has-session -t "=$SESSION" 2>/dev/null; then
  tmux -f /exec-daemon/tmux.portal.conf kill-session -t "$SESSION"
fi
for port in 8080 50051 50053 50054 50055 50056 50057 50058 50059 50060 50061 50062 50063 50064 50065 50066 50067 50068 50069 50070 50152; do
  pids="$(netstat -lntp 2>/dev/null | awk -v p=":$port" '$4 ~ p"$" {print $7}' | cut -d/ -f1 | sort -u)"
  for pid in $pids; do
    [[ "$pid" =~ ^[0-9]+$ ]] && kill "$pid" 2>/dev/null || true
  done
done
sleep 1

TMUX_CONF="/exec-daemon/tmux.portal.conf"
if [[ ! -f "$TMUX_CONF" ]]; then
  TMUX_CONF=""
fi
tmux_cmd() {
  if [[ -n "$TMUX_CONF" ]]; then
    tmux -f "$TMUX_CONF" "$@"
  else
    tmux "$@"
  fi
}

tmux_cmd new-session -d -s "$SESSION" -n bootstrap -- "${SHELL:-bash}" -l
tmux_cmd send-keys -t "$SESSION:bootstrap" "echo OpenFoundry platform session" C-m

start_bin() {
  local name="$1"
  local port="$2"
  local db="$3"
  local extra="${4:-}"
  local admin_db
  admin_db="$(echo "$ADMIN_URL" | sed "s#/[^/]*\$#/${db}#")"
  local runtime_db
  runtime_db="$(echo "$admin_db" | sed 's#://openfoundry:#://openfoundry_app:#')"
  local cmd
  cmd="cd '$ROOT' && HOST=127.0.0.1 PORT=$port JWT_SECRET='$JWT_SECRET' DATABASE_URL='$runtime_db' MIGRATION_DATABASE_URL='$admin_db' RUST_LOG=info,sqlx=warn $extra '$BIN/$name' >'$LOGDIR/$name.log' 2>&1"
  tmux_cmd new-window -t "$SESSION" -n "$name" -- "${SHELL:-bash}" -l
  tmux_cmd send-keys -t "$SESSION:$name" "$cmd" C-m
}

for spec in "${SERVICES[@]}"; do
  name="${spec%%:*}"
  rest="${spec#*:}"
  port="${rest%%:*}"
  db="${rest##*:}"
  extra=""
  case "$name" in
    dataset-service)
      extra="STORAGE_BACKEND=local LOCAL_STORAGE_ROOT=/tmp/of-e2e/datasets"
      ;;
    notebook-service)
      extra="DATA_DIR=/tmp/of-e2e/notebooks"
      ;;
    pipeline-service)
      extra="DATA_DIR=/tmp/of-e2e/pipelines"
      ;;
  esac
  start_bin "$name" "$port" "$db" "$extra"
done

tmux_cmd new-window -t "$SESSION" -n gateway -- "${SHELL:-bash}" -l
tmux_cmd send-keys -t "$SESSION:gateway" \
  "cd '$ROOT' && HOST=127.0.0.1 PORT=8080 JWT_SECRET='$JWT_SECRET' DATA_CONNECTOR_URL=http://127.0.0.1:50152 RUST_LOG=info '$BIN/gateway' >'$LOGDIR/gateway.log' 2>&1" C-m

echo "Started services in tmux session '$SESSION'. Logs: $LOGDIR"
echo "Waiting for health endpoints..."

fail=0
for spec in "${SERVICES[@]}"; do
  name="${spec%%:*}"
  rest="${spec#*:}"
  port="${rest%%:*}"
  ok=0
  for _ in $(seq 1 60); do
    if curl -sf "http://127.0.0.1:${port}/health" >/dev/null; then
      echo "  ok  $name :$port"
      ok=1
      break
    fi
    sleep 1
  done
  if [[ "$ok" -ne 1 ]]; then
    echo "  FAIL $name :$port"
    tail -n 40 "$LOGDIR/$name.log" || true
    fail=1
  fi
done

ok=0
for _ in $(seq 1 30); do
  if curl -sf "http://127.0.0.1:8080/health" >/dev/null; then
    echo "  ok  gateway :8080"
    ok=1
    break
  fi
  sleep 1
done
if [[ "$ok" -ne 1 ]]; then
  echo "  FAIL gateway :8080"
  tail -n 40 "$LOGDIR/gateway.log" || true
  fail=1
fi

exit "$fail"
