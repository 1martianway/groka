# Grok Build fork — per-turn effort router (`groka`)

Thin personal fork of [xai-org/grok-build](https://github.com/xai-org/grok-build) (Apache-2.0).  
Adds a **heuristic effort router** so sessions stop always shipping **grok-4.5 high** on every turn.

Official `grok` stays on PATH. This tree installs beside it as **`groka`** (optional alias `kgrok`).

**Do not open a PR against `xai-org/grok-build`.** Push only to your own remote.

Upstream product docs: [README.upstream.md](README.upstream.md) · [x.ai/cli](https://x.ai/cli)

---

## Quick use

```sh
# 1) Build (Rust + DotSlash required — see Prerequisites)
cd ~/repos/grok-build   # or this worktree
cargo build -p xai-grok-pager-bin --release

# 2) Install beside official grok
./scripts/install-grok-local.sh
# → ~/.local/bin/groka

# 3) Keep the fork from self-replacing with the official channel
#    ~/.grok/config.toml
```

```toml
[cli]
auto_update = false

[effort_router]
enabled = true
preference = 3   # 1..=5; 3 neutral, higher → more high effort
floor = "low"
ceiling = "high"
```

```sh
# 4) Run
export PATH="$HOME/.local/bin:$PATH"
groka

# Pin when you want a fixed effort; auto re-enables the router
#   /effort low | medium | high | auto
# Status may show: medium (auto)
```

**What it does:** each user turn scores the prompt (length + simple/hard/coding keywords) and stamps `sampling_config.reasoning_effort` to `low` | `medium` | `high`. **No model swap.**

**Precedence:** explicit pin (`/effort`, `--effort`, persona) → router → `[models].default_reasoning_effort` → catalog default (usually high).

---

## Detailed guide

### Motivation

Upstream Grok Build often defaults reasoning effort to **high**. Hooks, plugins, skills, and MCP cannot set wire effort (`UserPromptSubmit` is observe-only; hook decisions are allow/deny; `additionalContext` is text only). Personas and `[models].default_reasoning_effort` are **session-sticky**, not per-prompt.

This fork adds a small turn-start router that chooses effort from the user prompt so trivial turns cost less and hard coding/debug turns still get high effort.

### Architecture (thin patch)

| Piece | Role |
|-------|------|
| `xai-grok-shell` `effort_router.rs` | Pure heuristics + `[effort_router]` config |
| `turn.rs` `maybe_stamp_effort_router` | At conversation-turn start, stamp `sampling_config.reasoning_effort` when unpinned |
| `ModelsManager` / `model_switch` | Pin flag; `/effort auto` clears pin |
| pager `/effort` + status | UI: pin levels + `effort: medium (auto)` when routed |

Wire path remains the normal chat sampling config (`reasoning_effort` on the request). Subagent turns skip the main-turn router stamp (persona/subagent overrides stay sticky).

### Heuristics (v0 — no LLM)

Base rank = medium, then:

| Signal | Effect |
|--------|--------|
| Very short prompt (&lt; ~40 chars) | −1 rank |
| Long prompt (&gt; ~400 chars) | +1 rank |
| Simple chat (`hi`, `thanks`, …) if short | −1 rank |
| Hard keywords (debug, race, architecture, …) | +1 rank |
| Coding keywords (implement, unit test, stack trace, …) | +1 rank |
| `preference` 1..=5 | bias `preference − 3` (−2..=+2) |
| `floor` / `ceiling` | clamp within low\|medium\|high |

Examples (default preference=3):

| Prompt | Typical effort |
|--------|----------------|
| `hi` / `thanks!` | low |
| Neutral ~100-char summary request | medium |
| Debug race + implement + unit tests | high |

### Configuration

Shared with official `grok`: `~/.grok/config.toml`.

```toml
[effort_router]
enabled = true      # false → never mutates sampling effort
preference = 3      # 1 cheap-biased … 5 expensive-biased
floor = "low"       # min among low|medium|high
ceiling = "high"    # max among low|medium|high
```

Section omitted → same defaults (`enabled = true`).

```toml
[models]
# Sticky seed only — does not pin; router may still override when unpinned.
default_reasoning_effort = "medium"
```

```toml
[cli]
# Required for a local build so official auto-update does not overwrite it.
auto_update = false
```

CLI pin (sticky until changed):

```sh
groka --effort low
# or inside the TUI:
# /effort high
# /effort auto
```

### Precedence (full chain)

1. **Explicit pin** — `/effort low|medium|high`, `--effort`, or persona/role pin  
2. **Router** — when enabled and not pinned  
3. **`[models].default_reasoning_effort`** — sticky seed, not a pin  
4. **Catalog default** — typically high  

`/effort auto` clears the pin and re-enables routing. UI/log can show `medium (auto)` when the current level came from the router.

### Prerequisites

- **Rust** via [`rust-toolchain.toml`](rust-toolchain.toml) / rustup  
- **[DotSlash](https://dotslash-cli.com)** on `PATH` (for hermetic `bin/protoc`)  
- macOS or Linux  

```sh
cargo install dotslash
/usr/bin/env dotslash --help
```

### Clone, remotes, and push policy

```sh
git clone https://github.com/xai-org/grok-build.git ~/repos/grok-build
cd ~/repos/grok-build

# xAI = upstream only (never push here)
git remote rename origin upstream

# your private fork (create once, then:)
# git remote add origin git@github.com:YOU/grok-build.git
```

If `origin` still points at `xai-org/grok-build`, treat it as **read-only**. Never `git push origin` to that org. No PRs to xAI.

### Build

```sh
cd ~/repos/grok-build   # or a linked worktree on your patch branch
cargo build -p xai-grok-pager-bin --release
# → target/release/xai-grok-pager
```

Cold release builds are large (~10–20+ minutes; `target/` can grow multi-GB). Prefer backgrounded builds in agent shells that time out at ~5 minutes.

Fast compile check (no binary):

```sh
cargo check -p xai-grok-pager-bin
```

### Install as `groka`

```sh
./scripts/install-grok-local.sh
```

Installs to `~/.local/bin/groka` (override with `GROK_LOCAL_NAME`). Optional:

```sh
GROK_LOCAL_ALIAS_KGROK=1 ./scripts/install-grok-local.sh   # also ~/.local/bin/kgrok
```

Manual:

```sh
mkdir -p "$HOME/.local/bin"
install -m 755 target/release/xai-grok-pager "$HOME/.local/bin/groka"
export PATH="$HOME/.local/bin:$PATH"
which groka
groka --version
which grok   # still official
```

Re-run install after every rebuild.

### Tests

```sh
cargo test -p xai-grok-shell --lib effort_router
# broader effort-related:
cargo test -p xai-grok-shell --lib effort
cargo test -p xai-grok-pager --lib effort
```

Fixtures cover prompt→low/medium/high, floor/ceiling, preference bias, pin vs router, and the full precedence chain.

### Rebase onto upstream

Keep the patch thin; rebase often:

```sh
git fetch upstream   # or: git fetch origin  if origin is still xai-org
git checkout your-fork-branch
git rebase upstream/main
cargo test -p xai-grok-shell --lib effort_router
cargo build -p xai-grok-pager-bin --release
./scripts/install-grok-local.sh
```

### Layout

| Path | Role |
|------|------|
| This repo / worktree | Fork source + effort_router patch |
| `target/release/xai-grok-pager` | Release binary name (package `xai-grok-pager-bin`) |
| `~/.local/bin/groka` | Installed fork CLI |
| Official `grok` install | Unchanged managed binary |
| `~/.grok/config.toml` | Shared config (`[cli]`, `[effort_router]`, models, …) |
| `FORK.md` | Compact operational notes (same fork) |
| `README.upstream.md` | Upstream README snapshot |

### Out of scope (v0)

- LLM-based difficulty classifier  
- Model routing / model swap  
- Magy or other repos  
- PR or push to `xai-org`  

### License

Apache-2.0 — see [LICENSE](LICENSE). Personal thin fork; not an xAI release.
