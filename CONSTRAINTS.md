# Constraints

Last reviewed: 2026-11-17. What changed and when is under [History](#history).

Read alongside `CLAUDE.md`. This file does not get weakened to make a change pass.

## Floor (always enforced)

- **No `#[allow(...)]`.** Use `#[expect(lint, reason = "...")]` at the narrowest scope. Enforced by `clippy::allow_attributes` and `clippy::allow_attributes_without_reason`, both denied.
- **No panics outside tests.** No `todo!()`, `unimplemented!()`, `unwrap`, `expect` or slice indexing. Release builds abort on panic, so no thread (`dock-tracker`, `launch-log`, main) may panic. `clippy::todo`, `unimplemented`, `unwrap_used`, `expect_used` and `indexing_slicing` are denied crate-wide in `Cargo.toml` `[lints]`, and `clippy.toml` allows the last three in tests. The only exceptions are named startup invariants, each with `#[expect(clippy::expect_used, reason = "...")]` on its one statement: the `MainThreadMarker` in `setup()`, the Tauri runtime start in `run()`, and `build.rs`.
- **No silently dropped errors.** No `let _ = <Result>`, and no `.ok()` on a `Result` whose value is then dropped. Log it or propagate it. Enforced by `clippy::let_underscore_must_use` and `clippy::unused_result_ok`, both denied.
- **No skipped tests.** No `#[ignore]`d, skipped or deleted test without a reason in the commit message.
- **No type-check escapes.** No `@ts-ignore` or `@ts-expect-error` under `src/` (a grep in `check:fast`). No `eslint-disable` without a reason comment.
- **`unsafe` only in `macos/`.** Every `unsafe` block lives in `src-tauri/src/macos/` with a `// SAFETY:` comment. `unsafe_code` is denied crate-wide, so each `macos/` item that needs it opts in with `#[expect(unsafe_code, reason = "...")]`, and `clippy::undocumented_unsafe_blocks` is denied. A grep in `check:fast` refuses an `unsafe_code` opt-in outside `src/macos/`.
- **One inline test module per Rust module.** Unit tests sit in one `#[cfg(test)] mod tests { ... }`, the module's last item. `check:fast` refuses:
  - `#[cfg(test)]` anywhere but the line directly above `mod tests {`;
  - `#![cfg(test)]` and `include!(`;
  - any `tests.rs`, and any file under a `tests/` folder in `src`.

  Clippy denies a `#[test]` outside a test module (`tests_outside_test_module`) and an item after it (`items_after_test_module`).
- **No secrets in source.**

## Enforced with numbers

| Dimension | Rule | Checked by | Runs at | Why |
|---|---|---|---|---|
| Format | Zero diffs | `cargo fmt --check` | every edit, CI | Trivial to satisfy; keeps diffs reviewable |
| Lint (Rust) | Zero warnings, pedantic on, in the dev and release profiles | `cargo clippy --all-targets -- -D warnings && cargo clippy --release --all-targets -- -D warnings` | every edit, CI | The lints in `Cargo.toml` are the bar. The release pass runs with `debug_assertions` off, so it catches code only debug builds use. Neither pass covers Tauri's `cfg(dev)`, which only `pnpm tauri build` turns off; CI runs that build and fails on any warning. CI pins Rust 1.99.1, so a new clippy changes the bar only when the pin moves |
| Types (TS) | Zero errors, `strict: true` | `pnpm exec tsc --noEmit` | every edit, CI | Keeps the IPC wrapper honest |
| Tests | All pass | `cargo test` (in `src-tauri/`) and `pnpm test` (vitest, the pure TS beside its `*.test.ts`) | task end, CI | CI runs the Rust tests once, under `pnpm coverage:lcov` (`cargo llvm-cov`): the same test binaries, and any failure fails the job. `cargo llvm-cov` skips doctests. The crate has none, so the first doctest needs a `cargo test --doc` step in that job |
| Release bundle | No debug layer and no dev override in `dist/` | `pnpm build && test -d dist && ! grep -rqE 'debug-dock-item\|VITE_DOCKMOSS' dist` | task end, CI | The outlines, labels and overgrowth override are for dev builds only. `overlay.ts` loads them under `import.meta.env.DEV`; this checks the bundle, so a static import anywhere fails it |
| Deps: advisories | No advisory at high or above; no unknown licences | `cargo deny check advisories licenses` | task end, CI | External opinion (RustSec DB) |
| Deps: JS | Nothing at high or above | `pnpm audit --audit-level high` | task end, CI | External opinion |
| Secrets | No findings in history, unstaged changes or staged changes | `gitleaks git --redact --no-banner && gitleaks git --pre-commit --redact --no-banner && gitleaks git --pre-commit --staged --redact --no-banner` | task end, CI | `--redact` keeps any match out of transcripts. Untracked files are scanned only once added. Pinned: gitleaks 8.30.1 |
| Idle CPU | Under 1%, with the Dock idle and no icon at Patchy or above, summed over DockMoss, its WebKit WebContent and GPU processes, and the CPU it adds to the Dock process. Idle wakeups/s and WindowServer's added CPU are recorded beside the sum, not in it | `top`, by hand ([Measuring idle CPU](#measuring-idle-cpu)) | checkpoints A, C, D | The core promise in SUMMARY.md, "Battery and CPU". Rendering runs in WebKit's processes, and the Dock answers every Accessibility read (0.44 to 0.49% at the 2 Hz poll, against 0.00% without DockMoss), so DockMoss alone under-reports. WindowServer draws the overlay, and the sum misses work done there: a CSS compositor animation once added 43 to 50 points of WindowServer while the sum read 0.76% |

Dropped: Lighthouse and axe. A desktop overlay has no URL to audit, and the settings form is covered by review.

### Measuring idle CPU

1. Build with embedded assets: `pnpm tauri build --no-bundle` (release profile). Never measure `tauri dev`.
2. Find the WebKit pids: the `com.apple.WebKit.WebContent` and `com.apple.WebKit.GPU` pids that `pgrep -f com.apple.WebKit` lists after DockMoss starts and not before. If other WebKit hosts are running, confirm each pid's responsible process with `sudo launchctl procinfo <pid>`. If `sudo` isn't available unattended, say so and rely on the before-and-after `pgrep`.
3. Wait at least 60 s after `dockmoss started` is logged, with no builds or tests running. Record the load average (`uptime`).
4. Sample: `top -l 11 -s 1 -c d -pid <a> -pid <b> -pid <c> -pid <dock> -pid <ws> -stats pid,command,cpu,idlew`, where `<dock>` is `pgrep -x Dock` and `<ws>` is `pgrep -x WindowServer`. Drop the first sample and take the mean of the other 10. `-c d` with `-s 1` makes IDLEW wakeups per second.
5. Quit DockMoss and take the same sample under the same conditions, for the Dock and WindowServer baselines.
6. Report:
   - **The sum:** DockMoss, WebContent, GPU, plus the Dock's mean minus its baseline mean.
   - **Beside it:** WindowServer's mean minus its baseline mean. Other apps' screen updates put spikes into WindowServer, so also give the median of its per-second samples against the baseline's median.

## Measured, not yet enforced

| Metric | Today | Direction |
|---|---|---|
| Release `.app` size | 3.8 MiB (3912 KB by `du -sk DockMoss.app`; binary 3,765,712 bytes) | Record. Must not grow more than 10% per change |
| Cold start to first overlay frame | 382 ms (365 to 389 ms, and one 508 ms run). 416 ms (409 to 419 ms, and one 508 ms run) for the 4th mossy icon, whose bake waits behind the first two | Record. Must not grow |
| WebKit GPU process memory, every mossy icon drawn | 16 to 17 MB idle, 173 MB at the peak of a magnification sweep, 198 to 204 MB peak at startup. WebContent beside it: 123 to 146 MB idle | Record. Must not grow |
| Rust line coverage | 99.24% (6200 lines, 47 missed) | On demand, no threshold; a rough guide only |

How each was measured:

- **`.app` size:** `pnpm tauri build`, opt-level "s", `targets: ["app"]`, unsigned. At opt-level 3 it was 4928 KB.
- **Cold start:** medians of 6 runs, every Dock icon forced Overgrown, the `.bakedtest` test identifier with fresh data, the Dock shown. Timed from launch to the first ScreenCaptureKit capture with at least 200 moss pixels in the icon's lower art box (captures 27 to 32 ms apart; job 017's `tools/m/coldstart.swift`). Earlier figures of about 0.72 s used a different setup, so don't compare against them.
- **GPU memory:** `footprint -p` (phys_footprint), ten overgrown icons, the canvas the size of the 3024×1964 display, the Dock shown.
- **Coverage:** `pnpm coverage` (`cargo llvm-cov`), excluding the hand-verified platform and glue files: `macos/`, `main.rs`, `lib.rs`, `tray.rs`, `commands.rs`. Extra arguments pass through, such as `--html`. Tests sit inline, so their lines count too; the non-test lines alone (those before each file's `#[cfg(test)] mod tests {`) are 97.86% covered. CI runs `pnpm coverage:lcov` as its Rust test run, and on pushes to `main`, same-repo PRs and manual runs uploads the report to Codecov for the README badge. Codecov sets no status check, and a failed upload never fails CI.

## Scripts

The scripts in `package.json` mirror this file. If they drift, this file wins.

```json
"check:fast": "cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo clippy --release --all-targets -- -D warnings && ! grep -rl --include='*.rs' 'unsafe_code' src | grep -v '^src/macos/' && ! grep -rnE --include='*.rs' -A1 '#!?\\[cfg\\(test\\)\\]' src | grep -vE '#\\[cfg\\(test\\)\\]$|-[[:space:]]*mod tests \\{$|^--$' && ! grep -rn --include='*.rs' 'include!(' src && ! find src -name tests.rs -o -path '*/tests/*' | grep . && cd .. && ! grep -rnE '@ts-(ignore|expect-error)' src && tsc --noEmit",
"check:web": "pnpm test && pnpm build && test -d dist && ! grep -rqE 'debug-dock-item|VITE_DOCKMOSS' dist",
"check:rust-test": "cd src-tauri && cargo test",
"check:deps": "cd src-tauri && cargo deny check advisories licenses && cd .. && pnpm audit --audit-level high",
"check:secrets": "gitleaks git --redact --no-banner && gitleaks git --pre-commit --redact --no-banner && gitleaks git --pre-commit --staged --redact --no-banner",
"check:task": "pnpm check:fast && pnpm check:web && pnpm check:rust-test && pnpm check:deps && pnpm check:secrets",
"check": "pnpm check:fast",
"coverage": "cd src-tauri && cargo llvm-cov --ignore-filename-regex 'src/(macos/|(main|lib|tray|commands)[.]rs)'",
"coverage:lcov": "pnpm coverage --lcov --output-path target/lcov.info"
```

## Exceptions

| ID | Rule | Path | Reason | Owner | Expires |
|---|---|---|---|---|---|
| (none) | | | | | |

## History

- **Job 001, Gate 1:** this file approved.
- **Job 005, Gate 2:** Idle CPU counts the CPU DockMoss adds to the Dock process.
- **Job 007:** no-panic lints denied crate-wide.
- **Job 009:** `pnpm test` added to Tests (Gate 1), and the Release bundle row added (polish).
- **Job 010:** cold start and GPU memory recorded.
- **Job 011:** the release-bundle guard fails on a missing `dist/`.
- **Job 014:** the first `.app` size and Checkpoint D's cold start recorded.
- **Job 015, at the user's choice at Gate 1:** unit tests moved inline into their modules. `clippy::tests_outside_test_module` replaced the `#[test]` grep in `check:fast`, and a new grep and `find` there keep each module's tests in one inline `mod tests {` (review rounds 1 to 3). The 80% line-coverage threshold left `check:task` for the on-demand `pnpm coverage`.
- **Job 017, the user's decision of 2026-11-06:** GPU memory and cold start re-baselined for baked moss, with the WebContent figure beside GPU memory.
- **Job 018, the user's decision of 2026-11-06:** Idle CPU records WindowServer beside the sum, not in it. No bar changed.
- **Job 023, Gate 1:** the Lint row and `check:fast` added a release-profile clippy pass.
- **Job 024, Gate 1:** GitHub Actions CI runs `pnpm check:task`, `cargo build` and the release build on PRs to `main` and pushes to `main`. Its release build closes the Lint row's `cfg(dev)` gap.
- **Job 025, Gate 1:** CI reports line coverage to Codecov through `coverage:lcov`, with no threshold.
- **Job 026, Gate 1:** `check:task` became the composition of named parts (`check:fast`, `check:web`, `check:rust-test`, `check:deps`, `check:secrets`), running the same commands in the same order. CI runs those parts as parallel jobs, and runs the Rust tests once under `cargo llvm-cov` in place of `cargo test`. No bar changed.
