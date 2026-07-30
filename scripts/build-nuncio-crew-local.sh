#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$REPO_ROOT"

unset \
  ANTHROPIC_API_KEY \
  BUZZ_ACP_API_TOKEN \
  BUZZ_ACP_PRIVATE_KEY \
  BUZZ_API_TOKEN \
  BUZZ_PRIVATE_KEY \
  BUZZ_RELAY_PRIVATE_KEY \
  BUZZ_UPDATER_ENDPOINT \
  BUZZ_UPDATER_PUBLIC_KEY \
  DATABRICKS_TOKEN \
  NOSTR_PRIVATE_KEY \
  OPENAI_COMPAT_API_KEY \
  OPENAI_API_KEY \
  OPENROUTER_API_KEY \
  TAURI_SIGNING_PRIVATE_KEY \
  TAURI_SIGNING_PRIVATE_KEY_PASSWORD

. ./bin/activate-hermit

unset CARGO_BUILD_TARGET CARGO_ENCODED_RUSTFLAGS CARGO_TARGET_DIR RUSTFLAGS
export CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS=false
export VITE_NUNCIO_CREW_CHANNEL=local
HOST_TARGET=$(rustc -vV | sed -n 's|host: ||p')

cargo build --release --target "$HOST_TARGET" \
  -p buzz-acp \
  -p buzz-agent \
  -p buzz-dev-mcp \
  -p git-credential-nostr \
  -p buzz-cli
./scripts/bundle-sidecars.sh "$HOST_TARGET"

if [[ ! -d desktop/node_modules ]]; then
  pnpm --dir desktop install
fi

cd desktop
pnpm exec tauri build \
  --target "$HOST_TARGET" \
  --bundles app \
  --config src-tauri/tauri.nuncio-crew.conf.json \
  --no-sign

echo "Built desktop/src-tauri/target/$HOST_TARGET/release/bundle/macos/NuncioCrew Local.app"
