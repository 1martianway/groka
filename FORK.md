# Fork notes (groka)

Compact ops sheet. Full product story + guide: **[README.md](README.md)**.  
Upstream product README: [README.upstream.md](README.upstream.md).

**Groka — Grok Build with Auto Router**

Open-source fork with auto effort router, usage limit bar, and more to come. Installs as `groka` beside official `grok`.

## One-liner

```sh
./scripts/install-groka.sh
# builds release, stamps git HEAD into --version, installs ~/.local/bin/groka
# [cli] auto_update=false
```

Grok Build workflow (local, not GitHub Actions):

```text
/groka
# or: /workflow groka
```

Runs `./scripts/install-groka.sh` and requires `groka --version` to contain the
current short `git HEAD`. Optional: `/workflow groka` with
`{"skip_build": true}` to copy an existing release only.

CI (optional artifacts): `.github/workflows/build-groka.yml` on `main` / PRs.

## Config

```toml
[cli]
auto_update = false

[effort_router]
enabled = true
preference = 3
floor = "low"
ceiling = "high"
mode = "hybrid"
confidence_threshold = 50
escalation_strikes = 2
classifier_timeout_ms = 500

[ui]
show_limit_bar = true   # default on; /usage bar toggles
```

**Effort:** router on → status starts at **`low (auto)`**; each turn may bump to
medium/high via the hybrid cascade (stage / meaning / heuristic). Pin
(`/effort`, `--effort`, persona) wins; `/effort auto` re-enables and clears
escalation. Model is always grok-4.6.

**Limit bar:** on by default on the prompt chrome; `/usage bar` / `/usage bar on|off`
or Settings → Usage limit bar.

**Precedence (effort):** pin → router → `default_reasoning_effort` → catalog high.

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
./scripts/install-groka.sh          # or /groka in Grok Build
```

## License

Apache-2.0 — personal thin patch, not an xAI release.
