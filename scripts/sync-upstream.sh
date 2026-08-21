#!/usr/bin/env bash
# Sync this fork (1martianway/groka) with xai-org/grok-build.
#
# Default: fetch upstream + merge into the current branch (preserves local
# history: effort-router, groka branding, prompt usage bar, etc.).
#
# Usage:
#   ./scripts/sync-upstream.sh              # merge upstream/main
#   ./scripts/sync-upstream.sh --rebase     # rebase current branch on upstream
#   ./scripts/sync-upstream.sh --push       # push to origin after success
#   ./scripts/sync-upstream.sh --stash      # stash dirty work, sync, pop
#   ./scripts/sync-upstream.sh --dry-run    # fetch + show what would land
#   ./scripts/sync-upstream.sh --ours       # on conflict prefer our side
#   ./scripts/sync-upstream.sh --theirs     # on conflict prefer upstream
#
# Env:
#   UPSTREAM_REMOTE   default: upstream
#   UPSTREAM_URL      default: https://github.com/xai-org/grok-build.git
#   UPSTREAM_BRANCH   default: main
#   ORIGIN_REMOTE     default: origin
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

UPSTREAM_REMOTE="${UPSTREAM_REMOTE:-upstream}"
UPSTREAM_URL="${UPSTREAM_URL:-https://github.com/xai-org/grok-build.git}"
UPSTREAM_BRANCH="${UPSTREAM_BRANCH:-main}"
ORIGIN_REMOTE="${ORIGIN_REMOTE:-origin}"

MODE=merge          # merge | rebase
PUSH=0
STASH=0
DRY_RUN=0
STRATEGY=           # empty | ours | theirs
STASHED=0

die() { echo "error: $*" >&2; exit 1; }
info() { echo "==> $*"; }
ok() { echo "ok: $*"; }

usage() {
  sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'
  exit 0
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage ;;
    --rebase) MODE=rebase ;;
    --merge) MODE=merge ;;
    --push) PUSH=1 ;;
    --stash) STASH=1 ;;
    --dry-run) DRY_RUN=1 ;;
    --ours) STRATEGY=ours ;;
    --theirs) STRATEGY=theirs ;;
    *) die "unknown option: $1 (try --help)" ;;
  esac
  shift
done

command -v git >/dev/null || die "git not found"
git rev-parse --is-inside-work-tree >/dev/null 2>&1 || die "not a git repo: $ROOT"

BRANCH="$(git branch --show-current)"
[[ -n "$BRANCH" ]] || die "detached HEAD — check out a branch first"

# --- remotes -----------------------------------------------------------------

ensure_upstream() {
  if git remote get-url "$UPSTREAM_REMOTE" >/dev/null 2>&1; then
    local url
    url="$(git remote get-url "$UPSTREAM_REMOTE")"
    if [[ "$url" != *xai-org/grok-build* ]]; then
      info "resetting $UPSTREAM_REMOTE → $UPSTREAM_URL (was: $url)"
      git remote set-url "$UPSTREAM_REMOTE" "$UPSTREAM_URL"
    fi
  else
    info "adding remote $UPSTREAM_REMOTE → $UPSTREAM_URL"
    git remote add "$UPSTREAM_REMOTE" "$UPSTREAM_URL"
  fi
  # Never push to xAI.
  git remote set-url --push "$UPSTREAM_REMOTE" no_push 2>/dev/null || true
}

ensure_upstream

# --- dirty tree --------------------------------------------------------------

dirty() {
  ! git diff --quiet || ! git diff --cached --quiet || [[ -n "$(git ls-files --others --exclude-standard)" ]]
}

if dirty; then
  if [[ "$DRY_RUN" == 1 ]]; then
    info "working tree is dirty (ok for --dry-run)"
  elif [[ "$STASH" == 1 ]]; then
    info "stashing local changes (including untracked)"
    git stash push -u -m "sync-upstream: auto-stash $(date -u +%Y-%m-%dT%H:%MZ)"
    STASHED=1
  else
    die "working tree is dirty. Commit, or re-run with --stash.
$(git status -sb | head -40)"
  fi
fi

restore_stash() {
  if [[ "$STASHED" == 1 ]]; then
    info "restoring stashed changes"
    if ! git stash pop; then
      echo "warning: stash pop had conflicts — resolve, then git stash drop if needed" >&2
      return 1
    fi
  fi
  return 0
}

# --- fetch -------------------------------------------------------------------

info "fetching $UPSTREAM_REMOTE/$UPSTREAM_BRANCH"
git fetch --prune "$UPSTREAM_REMOTE" "$UPSTREAM_BRANCH"

UP_REF="${UPSTREAM_REMOTE}/${UPSTREAM_BRANCH}"
git rev-parse --verify "$UP_REF" >/dev/null 2>&1 || die "missing ref $UP_REF after fetch"

HEAD_SHA="$(git rev-parse HEAD)"
UP_SHA="$(git rev-parse "$UP_REF")"
BASE_SHA="$(git merge-base HEAD "$UP_REF")"

AHEAD="$(git rev-list --count "${UP_REF}..HEAD")"
BEHIND="$(git rev-list --count "HEAD..${UP_REF}")"

echo
echo "branch:   $BRANCH"
echo "HEAD:     ${HEAD_SHA:0:10}"
echo "upstream: ${UP_SHA:0:10}  ($UP_REF)"
echo "base:     ${BASE_SHA:0:10}"
echo "local commits not in upstream: $AHEAD"
echo "upstream commits not in local: $BEHIND"
echo

if [[ "$BEHIND" == 0 ]]; then
  ok "already up to date with $UP_REF"
  restore_stash || true
  exit 0
fi

info "incoming commits:"
git log --oneline --no-decorate "HEAD..${UP_REF}" | head -30
if [[ "$BEHIND" -gt 30 ]]; then
  echo "  … $((BEHIND - 30)) more"
fi
echo

if [[ "$DRY_RUN" == 1 ]]; then
  ok "dry-run only — no merge/rebase performed"
  restore_stash || true
  exit 0
fi

# --- integrate ---------------------------------------------------------------

# Trivial auto-resolve: SOURCE_REV is always upstream's monorepo stamp.
# Prefer their side so the fork tracks the real source revision.
auto_resolve_trivial() {
  local f
  for f in SOURCE_REV; do
    if git ls-files -u -- "$f" | grep -q .; then
      info "auto-resolving $f → upstream"
      git checkout --theirs -- "$f"
      git add -- "$f"
    fi
  done
}

if [[ "$MODE" == merge ]]; then
  info "merging $UP_REF into $BRANCH"
  MERGE_ARGS=(-m "Merge ${UP_REF} into ${BRANCH}

Integrate latest xai-org/grok-build while keeping fork customizations.")
  if [[ -n "$STRATEGY" ]]; then
    MERGE_ARGS+=(-X "$STRATEGY")
  fi

  set +e
  git merge --no-ff "${MERGE_ARGS[@]}" "$UP_REF"
  MERGE_RC=$?
  set -e

  if [[ $MERGE_RC -ne 0 ]]; then
    auto_resolve_trivial
    if git diff --name-only --diff-filter=U | grep -q .; then
      echo
      echo "error: merge conflicts remain:" >&2
      git diff --name-only --diff-filter=U | sed 's/^/  /' >&2
      echo
      echo "Resolve, then:" >&2
      echo "  git add <files> && git commit" >&2
      echo "Or abort:" >&2
      echo "  git merge --abort" >&2
      if [[ -n "$STRATEGY" ]]; then
        echo "(you already used --$STRATEGY; remaining conflicts need a human)" >&2
      else
        echo "Retry with automatic preference if appropriate:" >&2
        echo "  git merge --abort && $0 --ours     # keep fork on conflicts" >&2
        echo "  git merge --abort && $0 --theirs   # take upstream on conflicts" >&2
      fi
      exit 1
    fi
    # Only trivial files were conflicted — finish the merge.
    if ! git diff --cached --quiet || ! git diff --quiet; then
      git commit --no-edit
    fi
  fi
else
  info "rebasing $BRANCH onto $UP_REF"
  REBASE_ARGS=()
  if [[ -n "$STRATEGY" ]]; then
    REBASE_ARGS+=(-X "$STRATEGY")
  fi

  set +e
  git rebase "${REBASE_ARGS[@]}" "$UP_REF"
  REBASE_RC=$?
  set -e

  while [[ $REBASE_RC -ne 0 ]]; do
    auto_resolve_trivial
    if git diff --name-only --diff-filter=U | grep -q .; then
      echo
      echo "error: rebase conflicts remain:" >&2
      git diff --name-only --diff-filter=U | sed 's/^/  /' >&2
      echo
      echo "Resolve, then:  git add <files> && git rebase --continue" >&2
      echo "Or abort:       git rebase --abort" >&2
      exit 1
    fi
    # Trivial only — continue.
    set +e
    GIT_EDITOR=true git rebase --continue
    REBASE_RC=$?
    set -e
    # If continue failed with nothing left to do, break.
    if [[ $REBASE_RC -ne 0 ]] && [[ ! -d .git/rebase-merge && ! -d .git/rebase-apply ]]; then
      break
    fi
  done
fi

echo
ok "synced $BRANCH with $UP_REF"
git log --oneline --no-decorate -5
echo

# Smoke-check custom fork surfaces still exist after the merge.
MISSING=0
for path in \
  crates/codegen/xai-grok-shell/src/agent/effort_router \
  crates/codegen/xai-grok-shell/src/agent/effort_router/classifier.rs \
  crates/codegen/xai-grok-pager/src/views/credit_bar.rs \
  crates/codegen/xai-grok-pager/src/slash/commands/usage.rs \
  scripts/install-groka.sh \
  FORK.md
do
  if [[ ! -e "$path" ]]; then
    echo "warning: expected fork file missing after sync: $path" >&2
    MISSING=1
  fi
done
if [[ $MISSING -eq 0 ]]; then
  ok "fork markers present (effort_router, credit bar, /usage, install script, FORK.md)"
fi

if [[ "$PUSH" == 1 ]]; then
  info "pushing $BRANCH → $ORIGIN_REMOTE"
  if [[ "$MODE" == rebase ]]; then
    git push --force-with-lease "$ORIGIN_REMOTE" "$BRANCH"
  else
    git push "$ORIGIN_REMOTE" "$BRANCH"
  fi
  ok "pushed to $ORIGIN_REMOTE/$BRANCH"
else
  echo "tip: push when ready:"
  if [[ "$MODE" == rebase ]]; then
    echo "  git push --force-with-lease $ORIGIN_REMOTE $BRANCH"
  else
    echo "  git push $ORIGIN_REMOTE $BRANCH"
  fi
  echo "  or: $0 --push"
fi

restore_stash || exit 1
ok "done"
