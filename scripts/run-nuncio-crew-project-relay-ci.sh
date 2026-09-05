#!/usr/bin/env bash
set -euo pipefail

TEST_PATH="${1:?provide the live Project test path}"
REPO_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$REPO_ROOT"

cleanup() {
  if [[ -f /tmp/nuncio-crew-relay.pid ]]; then
    kill "$(cat /tmp/nuncio-crew-relay.pid)" 2>/dev/null || true
  fi
  docker compose down -v >/dev/null 2>&1 || true
}
trap cleanup EXIT

for attempt in 1 2 3; do
  if docker compose up -d postgres redis minio minio-init; then
    break
  fi
  if [[ "$attempt" -eq 3 ]]; then
    echo "Integration services failed to start." >&2
    exit 1
  fi
  sleep "$((attempt * 5))"
done

wait_healthy() {
  local container="$1"
  for _ in $(seq 1 60); do
    if [[ "$(docker inspect --format='{{.State.Health.Status}}' "$container" \
      2>/dev/null || true)" == "healthy" ]]; then
      return 0
    fi
    sleep 2
  done
  docker logs "$container" || true
  return 1
}

wait_healthy buzz-postgres
wait_healthy buzz-redis
wait_healthy buzz-minio

export PGHOST=localhost
export PGPORT=5432
export PGUSER=buzz
export PGPASSWORD=buzz_dev
export PGDATABASE=buzz
export PGSCHEMA_PLAN_HOST=localhost
export PGSCHEMA_PLAN_PORT=5432
export PGSCHEMA_PLAN_DB=buzz
export PGSCHEMA_PLAN_USER=buzz
export PGSCHEMA_PLAN_PASSWORD=buzz_dev

./bin/pgschema apply --file schema/schema.sql --auto-approve
docker exec -i -e PGPASSWORD=buzz_dev buzz-postgres \
  psql -U buzz -d buzz -v ON_ERROR_STOP=1 \
  < scripts/reconcile-schema-after-pgschema.sql
docker exec -e PGPASSWORD=buzz_dev buzz-postgres \
  psql -U buzz -d buzz -v ON_ERROR_STOP=1 -c "
INSERT INTO communities (id, host)
VALUES ('00000000-0000-4000-8000-00000000c0de', 'localhost:3000')
ON CONFLICT (lower(host)) DO NOTHING;"

cargo build --profile ci -p buzz-relay
nohup env \
  DATABASE_URL=postgres://buzz:buzz_dev@localhost:5432/buzz \
  REDIS_URL=redis://localhost:6379 \
  RELAY_URL=ws://localhost:3000 \
  BUZZ_BIND_ADDR=0.0.0.0:3000 \
  BUZZ_S3_ENDPOINT=http://localhost:9000 \
  BUZZ_S3_ACCESS_KEY=buzz_dev \
  BUZZ_S3_SECRET_KEY=buzz_dev_secret \
  BUZZ_S3_BUCKET=buzz-media \
  BUZZ_S3_REGION=us-east-1 \
  BUZZ_S3_ADDRESSING_STYLE=path \
  BUZZ_REQUIRE_AUTH_TOKEN=false \
  BUZZ_RECONCILE_CHANNELS=false \
  BUZZ_RATE_LIMIT_HUMAN_MESSAGES_PER_MIN=100000 \
  BUZZ_RATE_LIMIT_HUMAN_API_CALLS_PER_MIN=100000 \
  BUZZ_RATE_LIMIT_HUMAN_WS_EVENTS_PER_SEC=10000 \
  ./target/ci/buzz-relay > /tmp/nuncio-crew-relay.log 2>&1 &
echo $! > /tmp/nuncio-crew-relay.pid

for _ in $(seq 1 60); do
  if ! kill -0 "$(cat /tmp/nuncio-crew-relay.pid)" 2>/dev/null; then
    cat /tmp/nuncio-crew-relay.log
    exit 1
  fi
  if [[ "$(curl -s -o /dev/null -w '%{http_code}' \
    http://127.0.0.1:3000/_readiness || true)" == "200" ]]; then
    # The relay resolves its community from the WebSocket Host header.
    CREW_LIVE_RELAY_URL=ws://localhost:3000 \
      node --import ./desktop/test-loader.mjs \
      --experimental-strip-types --test "$TEST_PATH"
    exit 0
  fi
  sleep 1
done

cat /tmp/nuncio-crew-relay.log
exit 1
