# Groka — Grok Build with Auto Router

Open-source fork of [xAI’s Grok Build](https://github.com/xai-org/grok-build) with auto effort router, usage limit bar, and more to come. Installs as `groka` beside official `grok`. Full upstream TUI and agent stack — practical upgrades you feel every day in the terminal.

| Feature | What it does |
|--------|----------------|
| **Auto effort router** | Picks `low` / `medium` / `high` reasoning effort **per turn** from your prompt — so trivial messages don’t burn high effort by default |
| **Usage limit bar** | Shows your weekly/period Grok coding limit as a compact bar on the prompt chrome, so you can see allowance before you run out |
| **More to come** | Thin, rebase-friendly fork — more session ergonomics and control on the way |

Official `grok` stays on your PATH. This tree installs **beside** it as **`groka`** (optional alias `kgrok`). Same auth, same config dir (`~/.grok`), same models — more control.

Upstream product docs: [README.upstream.md](README.upstream.md) · [x.ai/cli](https://x.ai/cli)

> **Not an xAI product.** Do not open PRs against `xai-org/grok-build`. Push only to your own remote (this project’s origin is the fork remote).

---

## Why groka?

Upstream Grok Build is excellent — but two frictions show up quickly in long coding days:

1. **Effort defaults high.** Sticky session defaults and personas don’t adapt per prompt. Hooks and plugins can’t set wire `reasoning_effort`. Easy turns cost like hard ones.
2. **Limit visibility is buried.** Allowance lives in billing UIs; the prompt chrome didn’t show period usage at a glance.

**groka** is a thin, maintainable fork: rebase-friendly patches on top of upstream, not a rewrite.

---

## Features

### 1. Auto effort router

Each user turn scores the prompt (length + simple / hard / coding keywords) and stamps `sampling_config.reasoning_effort` to `low` | `medium` | `high`. **No model swap** — only reasoning effort.

- Default on; status shows e.g. `medium (auto)` when the router chose the level  
- Pin anytime: `/effort low|medium|high` or `groka --effort high`  
- Clear the pin: `/effort auto`  
- Config: `[effort_router]` in `~/.grok/config.toml`

**Precedence:** explicit pin (`/effort`, `--effort`, persona) → router → `[models].default_reasoning_effort` → catalog default (usually high).

### 2. Usage limit bar

A fixed-width progress bar on the **prompt chrome** (left of the model chip) shows period coding-limit usage from billing (`usage_pct` — typically weekly). Color shifts at 80% / 100%.

- **Default on**  
- Toggle: `/usage bar` · `/usage bar on` · `/usage bar off`  
- Also under **Settings → Usage limit bar** (`[ui].show_limit_bar`)  
- Hidden for gateway/chat sessions and when billing usage isn’t visible

`/usage` / `/usage show` still opens the full usage summary; `/usage manage` opens billing (consumer accounts).

---

## Quick start

```sh
# 1) Build + install (stamps git HEAD into groka --version)
git clone https://github.com/1martianway/groka.git
cd groka
./scripts/install-groka.sh
# → ~/.local/bin/groka

# Or from an existing Grok Build session: /groka
```

Keep the fork from self-replacing with the official channel — `~/.grok/config.toml`:

```toml
[cli]
auto_update = false

[effort_router]
enabled = true
preference = 3   # 1..=5; 3 neutral, higher → more high effort
floor = "low"
ceiling = "high"

[ui]
# Optional — limit bar defaults on when omitted
# show_limit_bar = true
```

```sh
export PATH="$HOME/.local/bin:$PATH"
groka

# Effort
#   /effort low | medium | high | auto

# Limit bar
#   /usage bar        # toggle
#   /usage bar off    # hide
#   /usage bar on     # show
```

---

## Detailed guide

### Motivation

Upstream Grok Build often defaults reasoning effort to **high**. Hooks, plugins, skills, and MCP cannot set wire effort (`UserPromptSubmit` is observe-only; hook decisions are allow/deny; `additionalContext` is text only). Personas and `[models].default_reasoning_effort` are **session-sticky**, not per-prompt.

This fork adds a small turn-start router that chooses effort from the user prompt so trivial turns cost less and hard coding/debug turns still get high effort.

Separately, the **usage limit bar** surfaces period allowance on every prompt so you don’t discover limits only when a request fails.

### Architecture (thin patch)

| Piece | Role |
|-------|------|
| `xai-grok-shell` `effort_router.rs` | Pure heuristics + `[effort_router]` config |
| `turn.rs` `maybe_stamp_effort_router` | At conversation-turn start, stamp `sampling_config.reasoning_effort` when unpinned |
| `ModelsManager` / `model_switch` | Pin flag; `/effort auto` clears pin |
| pager `/effort` + status | UI: pin levels + `effort: medium (auto)` when routed |
| pager `credit_bar` + prompt chrome | Weekly/period limit bar spans |
| `[ui].show_limit_bar` + `/usage bar` | Persistable toggle (default on) |

Wire path remains the normal chat sampling config (`reasoning_effort` on the request). Subagent turns skip the main-turn router stamp (persona/subagent overrides stay sticky).

### Effort heuristics (v0 — no LLM)

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

[ui]
show_limit_bar = true   # false hides the prompt chrome limit bar
```

Section omitted → same defaults (`enabled = true`, limit bar on).

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
# /usage bar off
```

### Precedence (effort, full chain)

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
git clone https://github.com/1martianway/groka.git
cd groka
```

This repo’s `origin` is the **fork**. Upstream xAI is fetch-only:

```sh
git remote -v
# origin    → your fork (push here)
# upstream  → xai-org/grok-build (fetch only; no_push)
```

**Never** `git push` to `xai-org/grok-build`. No PRs to xAI.

Sync upstream:

```sh
./scripts/sync-upstream.sh              # fetch + merge upstream/main
./scripts/sync-upstream.sh --dry-run    # show incoming only
```

### Build

```sh
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
./scripts/install-groka.sh
# release-builds first (touches build.rs so the version stamp = git HEAD),
# then copies to ~/.local/bin/groka. Fails if --version does not contain HEAD.
# ./scripts/install-groka.sh --skip-build   # install existing release only
```

From Grok Build: `/groka` (or `/workflow groka`) runs the same script and checks that `groka --version` matches the short SHA.

Installs to `~/.local/bin/groka` (override with `GROK_LOCAL_NAME`). Optional:

```sh
GROK_LOCAL_ALIAS_KGROK=1 ./scripts/install-groka.sh   # also ~/.local/bin/kgrok
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
cargo test -p xai-grok-pager --lib limit_usage
# broader:
cargo test -p xai-grok-shell --lib effort
cargo test -p xai-grok-pager --lib effort
```

Fixtures cover prompt→low/medium/high, floor/ceiling, preference bias, pin vs router, limit-bar visibility gates, and the full effort precedence chain.

### Rebase onto upstream

Keep the patch thin; rebase often:

```sh
./scripts/sync-upstream.sh
cargo test -p xai-grok-shell --lib effort_router
./scripts/install-groka.sh   # or /groka
```

### Layout

| Path | Role |
|------|------|
| This repo | Fork source + effort router + limit bar |
| `target/release/xai-grok-pager` | Release binary name (package `xai-grok-pager-bin`) |
| `~/.local/bin/groka` | Installed fork CLI |
| Official `grok` install | Unchanged managed binary |
| `~/.grok/config.toml` | Shared config (`[cli]`, `[effort_router]`, `[ui]`, models, …) |
| `FORK.md` | Compact operational notes |
| `README.upstream.md` | Upstream README snapshot |

### Out of scope (v0)

- LLM-based difficulty classifier  
- Model routing / model swap  
- PR or push to `xai-org`  

### Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Prefer small, rebase-friendly patches that stay easy to re-apply on upstream.

### License

Apache-2.0 — see [LICENSE](LICENSE). Community fork; not an xAI release.
