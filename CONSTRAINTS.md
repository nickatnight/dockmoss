# Constraints

Last reviewed: 2026-11-05 (approved at Gate 1, emgee job 001; Idle CPU counts the Dock process from job 005 Gate 2; no-panic lints crate-wide from job 007; `pnpm test` added to Tests from job 009 Gate 1; the Release bundle row from job 009's polish; cold start and GPU memory recorded in job 010; the release guard fails on a missing `dist/` from job 011; the first `.app` size and Checkpoint D's cold start recorded in job 014; in job 015, at the user's choice at Gate 1, unit tests moved inline into their modules, `clippy::tests_outside_test_module` replaced the `#[test]` grep in `check:fast`, and the 80% line-coverage threshold left `check:task` for the on-demand `pnpm coverage`)

Read alongside `CLAUDE.md`. This file does not get weakened to make a change pass.

## Floor (always enforced)

- No new `#[allow(...)]`. Use `#[expect(lint, reason = "...")]` at the narrowest scope (enforced by `clippy::allow_attributes` and `clippy::allow_attributes_without_reason` set to deny).
- No `todo!()`, `unimplemented!()`, or `.unwrap()` outside `#[cfg(test)]` (enforced by `clippy::unwrap_used`, `clippy::todo`, `clippy::unimplemented` set to deny in `[lints]`).
- No `let _ = <Result>` that discards an error silently. Log it or propagate it (enforced by `clippy::let_underscore_must_use` set to deny). The same goes for `.ok()` on a `Result` whose value is then dropped (enforced by `clippy::unused_result_ok` set to deny).
- No `#[ignore]`d, skipped or deleted tests without a reason in the commit message.
- No `@ts-ignore` or `@ts-expect-error` anywhere under `src/` (enforced by a grep in `check:fast`). No `eslint-disable` without a reason comment.
- Every `unsafe` block lives in `src-tauri/src/macos/` and has a `// SAFETY:` comment. `unsafe_code` is set to deny crate-wide, so each `macos/` item that needs `unsafe` opts in with `#[expect(unsafe_code, reason = "...")]`, and `clippy::undocumented_unsafe_blocks` is set to deny. No `unsafe_code` opt-in exists outside `src/macos/` (enforced by a grep in `check:fast`).
- No `expect`, `unwrap` or slice indexing outside `#[cfg(test)]`. Release builds abort on panic, so no thread (`dock-tracker`, `launch-log`, the main thread) may panic. `clippy::unwrap_used`, `clippy::expect_used` and `clippy::indexing_slicing` are denied crate-wide in `Cargo.toml` `[lints]`, so new code is covered by default; `clippy.toml` allows `expect` and indexing in tests. The only exceptions are named startup invariants, each with `#[expect(clippy::expect_used, reason = "...")]` on the one statement: the `MainThreadMarker` in `setup()`, the Tauri runtime start in `run()`, and `build.rs`.
- A `#[test]` fn sits only inside a `#[cfg(test)]` module (enforced by `clippy::tests_outside_test_module` set to deny), and that module is the last item of its parent (enforced by `clippy::items_after_test_module`, on by default).
- No secrets in source.

## Enforced with numbers

| Dimension | Rule | Checked by | Runs at | Why |
|---|---|---|---|---|
| Format | Zero diffs | `cargo fmt --check` | every edit | Trivial to satisfy; keeps diffs reviewable |
| Lint (Rust) | Zero warnings, pedantic on | `cargo clippy --all-targets -- -D warnings` | every edit | Lints in `Cargo.toml` are the project's bar |
| Types (TS) | Zero errors, `strict: true` | `pnpm exec tsc --noEmit` | every edit | Keeps the IPC wrapper honest |
| Tests | All pass | `cargo test` (in `src-tauri/`) and `pnpm test` (vitest, the pure TS beside its `*.test.ts`) | task end | |
| Release bundle | No debug layer and no dev override in `dist/` | `pnpm build && test -d dist && ! grep -rqE 'debug-dock-item\|VITE_DOCKMOSS' dist` | task end | The outlines, labels and overgrowth override are for dev builds only (job 009 Gate 1 Q3). `overlay.ts` loads them under `import.meta.env.DEV`; this checks the bundle, so a static import anywhere fails it |
| Deps: advisories | No advisory at high or above; no unknown licences | `cargo deny check advisories licenses` | task end | External opinion (RustSec DB) |
| Deps: JS | Nothing at high or above | `pnpm audit --audit-level high` | task end | External opinion |
| Secrets | No findings in history, unstaged changes or staged changes | `gitleaks git --redact --no-banner && gitleaks git --pre-commit --redact --no-banner && gitleaks git --pre-commit --staged --redact --no-banner` | task end | `--redact` keeps any match out of transcripts. Untracked files are scanned only once added. Pinned: gitleaks 8.30.1. |
| Idle CPU | < 1% with the Dock idle and no icon ≥ Patchy, summed over dockmoss, its WebKit WebContent process, its WebKit GPU process, and the CPU it adds to the Dock process (the Dock's mean with dockmoss running minus its mean with dockmoss quit); idle wakeups/s recorded alongside | `top -l 11 -s 1 -c d -pid <a> -pid <b> -pid <c> -pid <dock> -stats pid,command,cpu,idlew`, first sample dropped, mean of the other 10 (manual). `<dock>` is `pgrep -x Dock`; its baseline is the same `top` sample of the Dock alone, taken under the same conditions with dockmoss quit, and only the difference counts toward the sum. The WebKit pids are the `com.apple.WebKit.WebContent` and `com.apple.WebKit.GPU` pids that `pgrep -f com.apple.WebKit` lists after dockmoss starts and not before. `-c d` makes IDLEW a per-sample count, so with `-s 1` it is wakeups/s. Measure an embedded-asset build (`pnpm tauri build --no-bundle`, release profile), never `tauri dev`. Start `top` at least 60 s after `dockmoss started` is logged, with no builds or tests running, and record the load average (`uptime`) beside the result. When other WebKit hosts run, confirm each WebKit pid's responsible process with `sudo launchctl procinfo <pid>`; if `sudo` is not available unattended, record that and rely on the before/after `pgrep` diff | checkpoints A, C, D | Core promise in SUMMARY.md "Battery". The webview's rendering runs in the WebKit processes, and every Accessibility read is answered by the Dock process (0.44–0.49% at the 2 Hz poll in job 005, against 0.00% without dockmoss), so dockmoss alone under-reports |

Dropped: Lighthouse and axe. A desktop overlay has no URL to audit, and the settings form is covered by review.

Scripts in `package.json` mirror this file; if they drift, this file wins:

```json
"check:fast": "cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && ! grep -rl --include='*.rs' 'unsafe_code' src | grep -v '^src/macos/' && cd .. && ! grep -rnE '@ts-(ignore|expect-error)' src && tsc --noEmit",
"check:task": "pnpm check:fast && pnpm test && pnpm build && test -d dist && ! grep -rqE 'debug-dock-item|VITE_DOCKMOSS' dist && cd src-tauri && cargo test && cargo deny check advisories licenses && cd .. && pnpm audit --audit-level high && gitleaks git --redact --no-banner && gitleaks git --pre-commit --redact --no-banner && gitleaks git --pre-commit --staged --redact --no-banner",
"check": "pnpm check:fast",
"coverage": "cd src-tauri && cargo llvm-cov --ignore-filename-regex 'src/(macos/|(main|lib|tray|commands)[.]rs)'"
```

## Measured, not yet enforced

| Metric | Today | Direction |
|---|---|---|
| Release `.app` size | 3.8 MiB (3912 KB, `du -sk DockMoss.app`; binary 3,765,712 bytes), job 014: `pnpm tauri build`, opt-level "s", `targets: ["app"]`, unsigned. It was 4928 KB at opt-level 3 | record, must not grow > 10% per change |
| Cold start to first overlay frame | 0.72 s, median of 5 (0.714 to 0.745 s), job 014 (Checkpoint D): the release bundle's binary on the real log, same method as job 010, from launch to the first ScreenCaptureKit capture of Discord's lower art box that shows moss (captures about 22 ms apart). An earlier set of 5 had a median of 0.736 s, its first run (1.04 s) the first launch of a fresh build. Job 010: 0.72 s (0.718 to 0.741 s) | record, must not grow |
| WebKit GPU process memory (`footprint -p`, phys_footprint), every mossy icon drawn | 17 MB idle and 174 MB at the peak of a magnification sweep, with a 191 MB process peak at startup, job 010 (four overgrown icons, the canvas the size of the 3024x1964 display) | record, must not grow |
| Line coverage of `src-tauri/src/`, excluding the hand-verified platform and glue files (`macos/`, `main.rs`, `lib.rs`, `tray.rs`, `commands.rs`) | 99.24% (6200 lines, 47 missed), job 015, from `pnpm coverage` (`cargo llvm-cov` with that exclusion; extra arguments pass through, such as `--html` or `--lcov --output-path target/lcov.info`). Unit tests sit inline since job 015, so their lines count too: of the 6099 line records `--lcov` writes, 4793 are in test modules, and the other 1306 are 97.86% covered (28 missed; a test line is one at or after its file's `#[cfg(test)] mod tests {`) | on demand, no threshold: a rough guide only |

## Exceptions

| ID | Rule | Path | Reason | Owner | Expires |
|---|---|---|---|---|---|
| (none) | | | | | |
