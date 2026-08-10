# Fork notes (effort router)

Compact ops sheet. Full quick-start + detailed guide: **[README.md](README.md)**.  
Upstream product README: [README.upstream.md](README.upstream.md).

## One-liner

```sh
cargo build -p xai-grok-pager-bin --release && ./scripts/install-grok-local.sh
# ~/.local/bin/grok-local  — set [cli] auto_update=false
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

- `upstream` / read-only `origin` → `xai-org/grok-build` (never push; no PR)
- Your fork remote → e.g. `git@github.com:YOU/grok-build.git`

```sh
git fetch upstream
git rebase upstream/main
cargo test -p xai-grok-shell --lib effort_router
cargo build -p xai-grok-pager-bin --release
./scripts/install-grok-local.sh
```

## License

Apache-2.0 — personal thin patch, not an xAI release.
