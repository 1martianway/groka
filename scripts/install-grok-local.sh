#!/usr/bin/env bash
# Install the fork binary as groka (and optional kgrok) on PATH.
# Uses only this repo's release build so an upstream tree cannot shadow the patch.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN_DIR="${GROK_LOCAL_BIN_DIR:-$HOME/.local/bin}"
NAME="${GROK_LOCAL_NAME:-groka}"
SRC="${ROOT}/target/release/xai-grok-pager"

if [[ ! -x "$SRC" ]]; then
  echo "error: no release binary at:" >&2
  echo "  $SRC" >&2
  echo "Build first from this repo (the tree with effort_router):" >&2
  echo "  cd \"$ROOT\" && cargo build -p xai-grok-pager-bin --release" >&2
  exit 1
fi

mkdir -p "$BIN_DIR"
install -m 755 "$SRC" "${BIN_DIR}/${NAME}"
echo "installed: ${BIN_DIR}/${NAME}"
echo "  from:    ${SRC}"

if [[ "${GROK_LOCAL_ALIAS_KGROK:-0}" == "1" ]]; then
  ln -sfn "${BIN_DIR}/${NAME}" "${BIN_DIR}/kgrok"
  echo "alias:    ${BIN_DIR}/kgrok -> ${NAME}"
fi

export PATH="${BIN_DIR}:${PATH}"
hash -r 2>/dev/null || true
command -v "$NAME"
"$NAME" --version || true

echo
echo "Reminder: disable auto-update in ~/.grok/config.toml while using the fork:"
echo "  [cli]"
echo "  auto_update = false"
