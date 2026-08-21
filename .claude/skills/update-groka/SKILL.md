---
name: update-groka
description: Sync the groka fork with xai-org/grok-build upstream, resolve merge conflicts against the fork's local customizations, then build and install the groka binary. Use when the user asks to update groka, pull upstream Grok changes, sync the fork, rebuild groka, or install the latest groka.
---

# Update & install groka

Bring `1martianway/groka` up to date with `xai-org/grok-build`, keeping every
fork customization intact, then rebuild and install `~/.local/bin/groka`.

Run every step from the repo root (`/home/karan/repos/groka`).

## Step 0 — Preflight

```sh
git branch --show-current      # expect: main
git status -sb
git stash list
```

- Not on `main`? Stop and ask which branch to sync.
- A leftover stash from a previous aborted sync? Surface it before creating another.
- An in-progress merge/rebase (`.git/MERGE_HEAD`, `.git/rebase-merge`)? Finish or abort it first.

## Step 1 — Preview what is landing

```sh
./scripts/sync-upstream.sh --dry-run
```

Then map the collision surface *before* merging — this is what tells you where
conflicts will occur:

```sh
BASE=$(git merge-base HEAD upstream/main)
comm -12 \
  <(git diff --name-only "$BASE"..HEAD | sort) \
  <(git diff --name-only "$BASE"..upstream/main | sort)
```

Only files in that intersection can conflict. If it is empty, the merge is
mechanical. Report the incoming commit count and the overlap list to the user.

## Step 2 — Merge

```sh
./scripts/sync-upstream.sh --stash
```

`--stash` handles an untracked/dirty tree (e.g. `.serena/`) and pops it back
afterwards. The script already auto-resolves `SOURCE_REV` to upstream.

Never pass `--ours` or `--theirs` on the first attempt. Those are blunt
`-X` strategy flags: they silently drop one side of *every* conflicting hunk,
including hunks that should have been combined. Resolve by hand instead.

If the script reports remaining conflicts, do **not** abort. Go to Step 3.

## Step 3 — Resolve conflicts

```sh
git diff --name-only --diff-filter=U
```

Apply this precedence, file class by file class:

| File class | Resolution |
|---|---|
| `SOURCE_REV`, `Cargo.lock` | **upstream** — regenerate `Cargo.lock` with `cargo check` if it stays messy |
| `FORK.md`, `README.md`, `docs/announcement-drafts.md`, `scripts/*`, `.grok/workflows/*` | **fork** — these are fork-owned files |
| `crates/**/effort_router/**`, `views/credit_bar.rs`, `slash/commands/usage.rs` | **fork** — fork-only features; a conflict here means upstream added a same-named file, so read both |
| `README.upstream.md` | **upstream** — it is the mirror of xAI's README |
| Everything else | **integrate both sides by hand** |

"Integrate both sides" is the default, not the exception. The fork's edits to
shared files (`app/actions.rs`, `app/app_view.rs`, `agent_view/render.rs`,
`acp_handler/session_notification.rs`, `settings/defs.rs`, `settings/registry.rs`,
`agent/models.rs`) are *insertions* — an extra enum variant, an extra dispatch
arm, an extra settings row. Upstream's change to the same region is usually an
unrelated insertion. Keep both; delete a fork line only when upstream deleted
the thing it hooked into.

For each conflicted file: read the full conflict region with surrounding
context, decide, edit out the markers, then `git add <file>`.

Verify no markers survive anywhere before committing:

```sh
git diff --check
grep -rn '^<<<<<<< \|^>>>>>>> ' --include='*.rs' --include='*.toml' --include='*.md' . | grep -v '^./target/'
```

Commit the merge:

```sh
git commit --no-edit
```

## Step 4 — Confirm fork surfaces survived

```sh
ls crates/codegen/xai-grok-shell/src/agent/effort_router/
ls crates/codegen/xai-grok-pager/src/views/credit_bar.rs \
   crates/codegen/xai-grok-pager/src/slash/commands/usage.rs
```

The sync script prints its own marker check. If it warns about a missing file,
check whether the file was *refactored* (renamed/split) rather than lost, and
update the marker list in `scripts/sync-upstream.sh` to match reality.

## Step 5 — Build and install

```sh
./scripts/install-groka.sh
```

This is a full release build of a large Rust workspace — expect several
minutes. Run it in the background and poll the log rather than blocking.

**Commit before you build.** `build.rs` bakes `git rev-parse --short HEAD`
into the binary and the installer asserts `groka --version` contains it. Any
commit made *after* the build starts — including the merge commit, or a
follow-up tooling commit — leaves the installed binary correctly built but
stamped with the wrong sha. If that happens, just re-run
`./scripts/install-groka.sh`: touching `build.rs` recompiles one crate and
relinks, it does not rebuild the workspace.

The script fails loudly if `groka --version` does not contain the current short
`git HEAD`; that check is the point, so do not work around it with
`--skip-build`.

### If the build fails

Compile errors after a sync are nearly always the fork's code calling an
upstream API that changed signature. Read the actual error, find the upstream
commit that moved the API (`git log -p upstream/main -- <path>`), and adapt the
fork's call site. Do not revert the fork feature to make the build pass —
report the breakage to the user if adapting is non-obvious.

## Step 6 — Verify and report

```sh
groka --version
git log --oneline -3
```

Report to the user:
- upstream commits merged (count + subjects)
- conflicts hit and how each was resolved
- installed version string and that it matches HEAD
- anything left dirty or unpushed

## Step 7 — Push (ask first)

The merge is **not** pushed automatically. Ask before running:

```sh
git push origin main
```

## Recovery

```sh
git merge --abort                    # bail out of a merge mid-flight
git stash list && git stash pop      # recover the auto-stash if a step died
git reset --hard ORIG_HEAD           # undo a completed-but-wrong merge
```

`ORIG_HEAD` points at the pre-merge tip until the next git operation
overwrites it — capture the sha up front if you may need it later.
