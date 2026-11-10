# WinIsland — AI Agent Guide

WinIsland is a **Dynamic Island for Windows**, written in Rust. It replicates the iPhone Dynamic Island experience on Windows, integrating with System Media Transport Controls (SMTC) to display now-playing media, real-time lyrics, audio spectrum visualization, and more.

You are working in a codebase you did not design. Read before you write.

## Quick start

```bash
cargo fmt --all                          # Format
cargo clippy --workspace -- -D warnings  # Lint and type-check; replaces cargo check
cargo build --release                    # Production build, only when asked
```

Clippy type-checks the workspace, so also running `cargo check` pays the compile cost twice. `cargo test` compiles the workspace in the test profile and runs nothing: this project accepts no tests.

## Where to look

Read in this order. Stop when you have what you need; do not read all of it for a small change.

| File | Read it when |
|------|--------------|
| [.agents/ARCHITECTURE.md](.agents/ARCHITECTURE.md) | You need to know which crate owns a concern, or where `unsafe` is allowed |
| [.agents/STYLE-GUIDE.md](.agents/STYLE-GUIDE.md) | You are writing Rust and need naming, import, error, or rendering conventions |
| [CONTRIBUTING.md](CONTRIBUTING.md) | You are preparing a pull request |
| [.agents/skills/pr-check/SKILL.md](.agents/skills/pr-check/SKILL.md) | You changed code and need to verify it |
| [.agents/skills/commit-push/SKILL.md](.agents/skills/commit-push/SKILL.md) | The user asked you to commit |
| [.agents/skills/pr-write/SKILL.md](.agents/skills/pr-write/SKILL.md) | You are writing a PR description |

## Writing code here

The island sits in the user's shell for hours at a time, next to whatever they are doing. A panic is a visible crash, a wrong pixel is a bug report, and a dropped frame is noticed.

- **Match the file you are editing.** Imports, error style, naming, and layout come from its neighbours, not from preference. When the code and a document disagree, the code is right; say which document is stale.
- **Respect the seams.** Win32, COM, D3D12, and winit live in the platform and render crates; `src/` orchestrates and draws through `Painter`. Decide where code goes with [.agents/ARCHITECTURE.md](.agents/ARCHITECTURE.md) before writing it.
- **Do not panic on data you did not create.** No new `unwrap()`, `expect()`, or `panic!()` outside `main` startup. Media metadata, filenames, plugin payloads, and config files all arrive from outside, and all of them can be malformed.
- **Do not discard a result that can fail.** `let _ =` is right for a fire-and-forget call with no consequence — the style guide's `PostMessageW` example — and wrong anywhere the caller could act on the failure.
- **Fix warnings, do not mute them.** `#[allow(...)]`, a catch-all arm that swallows a case, and a lossy `as` cast each trade the next reader's time for silence. If suppression is genuinely correct, say why in the PR.
- **Delete what you replace.** No fallback path, dead flag, or unused config key kept "just in case". A change that only adds deserves a second look: it may be a parallel implementation rather than a replacement.
- **No abstraction for one call site.** Three uses is the usual threshold; one use is a guess about the future.
- **Comments explain why.** A comment that restates the line below it is noise. Invariants, workarounds, and ordering constraints are worth writing down. Do not write facts that expire — versions, counts, line numbers — into comments or documents; point at the source.
- **Adding a dependency is an architecture decision.** Say what it replaces, what it costs at build and run time, and why the standard library is not enough. `Cargo.lock` is generated; never hand-edit it.
- **Stay inside the task.** Reformatting, renaming, or reordering unrelated code buries the real change. A task that legitimately touches many files is fine — explain why each file is in the diff — but do not widen the change on your own.

## Where a new feature goes

**The core experience lives in `src/`.** The island composition, the expanded pages and pager, the music view, lyrics, the spectrum, the settings shell, and the window and event loop orchestration. These decide how the island looks, moves, and responds; changing one changes WinIsland.

**An extension is a built-in plugin.** Calendar, timer, widgets, and third-party plugins. They add a page, a widget, or a settings panel inside surfaces that already exist. The rule of thumb: if it changes how the island itself looks or behaves, it is core; if it adds something inside it, it is an extension.

When it is an extension:

1. Add a module under `crates/winisland-builtin-plugins/src/`. Built-in plugins share this crate and depend only on `winisland-plugin-api`.
2. Implement the ABI v2 entry point and expose `descriptor()`.
3. Register it in `builtin_plugins()` in `crates/winisland-builtin-plugins/src/lib.rs` and add its translation keys.
4. Draw, take input, and persist through the plugin services. Do not reach into `src/`.

`crates/winisland-builtin-plugins/src/calendar/` is the reference: the shared crate uses `default-features = false` plus the `sdk` feature, and each plugin exposes `descriptor()` and has an entry in `builtin_plugins()`. See [.agents/ARCHITECTURE.md](.agents/ARCHITECTURE.md#plugin-system) and [Page/plugin-dev.md](Page/plugin-dev.md).

If an extension cannot be expressed through the plugin ABI, name the missing host capability and propose adding it to the ABI. Do not route around the seam by writing it into `src/`.

## Non-negotiables

1. Do not commit, tag, push, or open a PR unless asked in this turn.
2. Do not make a check pass by weakening it. No edits to this guide, `.agents/**`, `.github/workflows/**`, or `CODEOWNERS`, and no changes to CI configuration, path filters, skip markers, or test selection.
3. Do not claim a change works without running it, and never invent output. "Should be fine" is not verification.
4. No tests. The project accepts no unit, integration, snapshot, or UI tests, and no temporary scaffolding standing in for them.
5. Do not create README or documentation files unless asked.
6. No secrets in the tree, the logs, or the PR body.
7. `Changelog.md` and `Changelog-zh.md` belong to the maintainers; do not edit them.

## Before you report

- Run `cargo fmt --all` and `cargo clippy --workspace -- -D warnings`, and quote the real result, including the exit code.
- Say what you could not verify. Unverified is not done.
- List the files you actually changed.
- For UI work, describe how a person can see the effect; there are no UI tests to fall back on.
- If the request conflicts with something here, name the conflict and ask, rather than picking a side.

## Commits and pull requests

- Commit only when asked. A commit message describes what changed and why; format, types, and scopes live in [.agents/skills/commit-push/SKILL.md](.agents/skills/commit-push/SKILL.md).
- No workflow bookkeeping in the message: no `phase 1`, `P0`, `step 3`, `batch 4`, or `[ci-skip]` markers.
- Stage related files together; do not `git add -A` across unrelated changes.
- Pull requests follow [.github/pull_request_template.md](.github/pull_request_template.md).

## Project-specific

- **Platform order**: `winisland-core` is the domain layer and never depends on a platform crate; `winisland-platform` holds OS-neutral contracts with no `unsafe`; `winisland-platform-windows` owns the Win32 surface; `winisland-render` owns the GPU; `src/` depends on `winisland-core`, never the reverse. Full table: [.agents/ARCHITECTURE.md](.agents/ARCHITECTURE.md).
- **`unsafe`**: only in the crates [.agents/ARCHITECTURE.md](.agents/ARCHITECTURE.md) lists, always with a `// SAFETY:` comment naming the invariant and who upholds it.
- **Capture exclusion**: `crates/winisland-platform-windows/src/capture.rs` sets `WDA_EXCLUDEFROMCAPTURE` so the island does not appear in its own backdrop. The magnifier fallback and the liquid glass style depend on that ordering; read the module before touching it.
- **Do not touch**: `Cargo.lock` by hand, and CI or agent configuration unless the task is about it.
- **Conventions**: [.agents/STYLE-GUIDE.md](.agents/STYLE-GUIDE.md). The code around you wins over it.
- **Emoji**: none, unless asked.
