# Fork notes (effort router)

Compact ops sheet. Full quick-start + detailed guide: **[README.md](README.md)**.  
Upstream product README: [README.upstream.md](README.upstream.md).

## One-liner

```sh
cargo build -p xai-grok-pager-bin --release && ./scripts/install-grok-local.sh
# ~/.local/bin/groka  — set [cli] auto_update=false
```

## Config

```toml
[cli]
auto_update = false

[effort_router]
enabled = true
preference = 3
floor = "low"
ceiling = "high"
```

**Default:** router on → status starts at **`low (auto)`**; each turn may bump to
medium/high. Pin (`/effort`, `--effort`, persona) wins; `/effort auto` re-enables.

**Precedence:** pin → router → `default_reasoning_effort` → catalog high.

## Remotes

- `upstream` (fetch-only) → `xai-org/grok-build`
- `origin` → `1martianway/groka` (your fork; push here)

### Sync from xAI

```sh
./scripts/sync-upstream.sh              # fetch + merge upstream/main
./scripts/sync-upstream.sh --dry-run    # show incoming only
./scripts/sync-upstream.sh --stash      # stash dirty tree, sync, pop
./scripts/sync-upstream.sh --push       # merge then push origin
./scripts/sync-upstream.sh --rebase     # rebase instead of merge
./scripts/sync-upstream.sh --ours       # prefer fork on conflicts
./scripts/sync-upstream.sh --theirs     # prefer upstream on conflicts
```

`SOURCE_REV` conflicts auto-take upstream. Real conflicts exit non-zero with
paths and fix-up commands. Then:

```sh
cargo test -p xai-grok-shell --lib effort_router
cargo build -p xai-grok-pager-bin --release
./scripts/install-grok-local.sh
```

## License

Apache-2.0 — personal thin patch, not an xAI release.
