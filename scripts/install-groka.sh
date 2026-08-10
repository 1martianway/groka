#!/usr/bin/env bash
# Build (optional) + install the fork binary as groka on PATH.
#
# Version string is baked at compile time from `git rev-parse --short HEAD`
# (see crates/codegen/xai-grok-pager-bin/build.rs). This script rebuilds by
# default and force-reruns that build.rs so `groka --version` matches HEAD —
# not a stale prior release binary.
#
# Usage:
#   ./scripts/install-groka.sh              # release build + install
#   ./scripts/install-groka.sh --skip-build  # install existing release only
#   GROK_LOCAL_ALIAS_KGROK=1 ./scripts/install-groka.sh
#
# Env:
#   GROK_LOCAL_BIN_DIR   default: ~/.local/bin
#   GROK_LOCAL_NAME      default: groka
#   GROK_LOCAL_ALIAS_KGROK  set to 1 to also symlink kgrok
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN_DIR="${GROK_LOCAL_BIN_DIR:-$HOME/.local/bin}"
NAME="${GROK_LOCAL_NAME:-groka}"
SRC="${ROOT}/target/release/xai-grok-pager"
BUILD_RS="${ROOT}/crates/codegen/xai-grok-pager-bin/build.rs"
DO_BUILD=1

while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-build) DO_BUILD=0 ;;
    -h|--help)
      sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "error: unknown option: $1" >&2
      exit 1
      ;;
  esac
  shift
done

cd "$ROOT"

HEAD_SHORT="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
echo "git HEAD: ${HEAD_SHORT}"

if [[ "$DO_BUILD" == 1 ]]; then
  # Force build.rs to re-run even when only the git tip moved (no source edits).
  if [[ -f "$BUILD_RS" ]]; then
    touch "$BUILD_RS"
  fi
  echo "==> cargo build -p xai-grok-pager-bin --release"
  cargo build -p xai-grok-pager-bin --release
else
  echo "==> --skip-build: using existing ${SRC}"
fi

if [[ ! -x "$SRC" ]]; then
  echo "error: no release binary at:" >&2
  echo "  $SRC" >&2
  echo "Build first (omit --skip-build) from this repo:" >&2
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

VERSION_LINE="$("$NAME" --version 2>&1 || true)"
echo "$VERSION_LINE"

if [[ "$HEAD_SHORT" != "unknown" ]] && ! grep -q "$HEAD_SHORT" <<<"$VERSION_LINE"; then
  echo
  echo "warning: installed version does not contain git HEAD ${HEAD_SHORT}" >&2
  echo "  got: ${VERSION_LINE}" >&2
  if [[ "$DO_BUILD" == 0 ]]; then
    echo "  hint: re-run without --skip-build so build.rs stamps the current commit" >&2
  else
    echo "  hint: cargo may have used a stale incremental stamp; try:" >&2
    echo "    touch \"$BUILD_RS\" && cargo build -p xai-grok-pager-bin --release" >&2
  fi
  exit 1
fi

if [[ "$HEAD_SHORT" != "unknown" ]]; then
  echo "ok: version stamp matches HEAD (${HEAD_SHORT})"
fi

echo
echo "Reminder: disable auto-update in ~/.grok/config.toml while using the fork:"
echo "  [cli]"
echo "  auto_update = false"
