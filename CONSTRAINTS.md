# Constraints

Last reviewed: 2026-11-02 (approved at Gate 1, emgee job 001; Idle CPU counts the Dock process from job 005 Gate 2)

Read alongside `CLAUDE.md`. This file does not get weakened to make a change pass.

## Floor (always enforced)

- No new `#[allow(...)]`. Use `#[expect(lint, reason = "...")]` at the narrowest scope (enforced by `clippy::allow_attributes` and `clippy::allow_attributes_without_reason` set to deny).
- No `todo!()`, `unimplemented!()`, or `.unwrap()` outside `#[cfg(test)]` (enforced by `clippy::unwrap_used`, `clippy::todo`, `clippy::unimplemented` set to deny in `[lints]`).
- No `let _ = <Result>` that discards an error silently. Log it or propagate it (enforced by `clippy::let_underscore_must_use` set to deny). The same goes for `.ok()` on a `Result` whose value is then dropped (enforced by `clippy::unused_result_ok` set to deny).
- No `#[ignore]`d, skipped or deleted tests without a reason in the commit message.
- No `@ts-ignore` or `@ts-expect-error` anywhere under `src/` (enforced by a grep in `check:fast`). No `eslint-disable` without a reason comment.
- Every `unsafe` block lives in `src-tauri/src/macos/` and has a `// SAFETY:` comment. `unsafe_code` is set to deny crate-wide, so each `macos/` item that needs `unsafe` opts in with `#[expect(unsafe_code, reason = "...")]`, and `clippy::undocumented_unsafe_blocks` is set to deny. No `unsafe_code` opt-in exists outside `src/macos/` (enforced by a grep in `check:fast`).
- No `expect`, `unwrap` or slice indexing on the Dock polling path. Release builds abort on panic, so the `dock-tracker` thread must not panic. `unwrap_used` is denied crate-wide; every polling-path item (`mod dock_tracker` and `primary_display` in `lib.rs`, and the `dock`, `geometry`, `tracker`, `macos::dock` and `macos::cursor` modules) also denies `clippy::expect_used` and `clippy::indexing_slicing`. A module the loop starts calling gets the same attribute.
- Unit tests live in `<module>/tests.rs`, declared with `#[cfg(test)] mod tests;`, not in an inline module (enforced by a grep in `check:fast`). Coverage excludes those files, so it measures only logic.
- No secrets in source.

## Enforced with numbers

| Dimension | Rule | Checked by | Runs at | Why |
|---|---|---|---|---|
| Format | Zero diffs | `cargo fmt --check` | every edit | Trivial to satisfy; keeps diffs reviewable |
| Lint (Rust) | Zero warnings, pedantic on | `cargo clippy --all-targets -- -D warnings` | every edit | Lints in `Cargo.toml` are the project's bar |
| Types (TS) | Zero errors, `strict: true` | `pnpm exec tsc --noEmit` | every edit | Keeps the IPC wrapper honest |
| Tests | All pass | `cargo test` (in `src-tauri/`) | task end | |
| Coverage | ≥ 80% line coverage of `src-tauri/src/`, excluding only the hand-verified platform and glue files (`macos/`, `main.rs`, `lib.rs`, `tray.rs`, `commands.rs`) and the `tests.rs` files that hold unit tests | `cargo llvm-cov --fail-under-lines 80 --ignore-filename-regex 'src/(macos/\|(main\|lib\|tray\|commands)[.]rs)\|/tests[.]rs$'` | task end | Pure logic is fully testable. Platform and glue code is verified manually, so it is excluded instead of padded. Excluding a new file means adding it to this regex in CONSTRAINTS.md, where review sees it. |
| Deps: advisories | No advisory at high or above; no unknown licences | `cargo deny check advisories licenses` | task end | External opinion (RustSec DB) |
| Deps: JS | Nothing at high or above | `pnpm audit --audit-level high` | task end | External opinion |
| Secrets | No findings in history, unstaged changes or staged changes | `gitleaks git --redact --no-banner && gitleaks git --pre-commit --redact --no-banner && gitleaks git --pre-commit --staged --redact --no-banner` | task end | `--redact` keeps any match out of transcripts. Untracked files are scanned only once added. Pinned: gitleaks 8.30.1. |
| Idle CPU | < 1% with the Dock idle and no icon ≥ Patchy, summed over dockmoss, its WebKit WebContent process, its WebKit GPU process, and the CPU it adds to the Dock process (the Dock's mean with dockmoss running minus its mean with dockmoss quit); idle wakeups/s recorded alongside | `top -l 11 -s 1 -c d -pid <a> -pid <b> -pid <c> -pid <dock> -stats pid,command,cpu,idlew`, first sample dropped, mean of the other 10 (manual). `<dock>` is `pgrep -x Dock`; its baseline is the same `top` sample of the Dock alone, taken under the same conditions with dockmoss quit, and only the difference counts toward the sum. The WebKit pids are the `com.apple.WebKit.WebContent` and `com.apple.WebKit.GPU` pids that `pgrep -f com.apple.WebKit` lists after dockmoss starts and not before. `-c d` makes IDLEW a per-sample count, so with `-s 1` it is wakeups/s. Measure an embedded-asset build (`pnpm tauri build --no-bundle`, release profile), never `tauri dev`. Start `top` at least 60 s after `dockmoss started` is logged, with no builds or tests running, and record the load average (`uptime`) beside the result. When other WebKit hosts run, confirm each WebKit pid's responsible process with `sudo launchctl procinfo <pid>`; if `sudo` is not available unattended, record that and rely on the before/after `pgrep` diff | checkpoints A, C, D | Core promise in SUMMARY.md "Battery". The webview's rendering runs in the WebKit processes, and every Accessibility read is answered by the Dock process (0.44–0.49% at the 2 Hz poll in job 005, against 0.00% without dockmoss), so dockmoss alone under-reports |

Dropped: Lighthouse and axe. A desktop overlay has no URL to audit, and the settings form is covered by review.

Scripts in `package.json` mirror this file; if they drift, this file wins:

```json
"check:fast": "cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && ! grep -rlE --include='*.rs' '#\\[([a-z_]+::)?test\\]' src | grep -v '/tests[.]rs$' && ! grep -rl --include='*.rs' 'unsafe_code' src | grep -v '^src/macos/' && cd .. && ! grep -rnE '@ts-(ignore|expect-error)' src && tsc --noEmit",
"check:task": "pnpm check:fast && cd src-tauri && cargo test && cargo llvm-cov --fail-under-lines 80 --ignore-filename-regex 'src/(macos/|(main|lib|tray|commands)[.]rs)|/tests[.]rs$' && cargo deny check advisories licenses && cd .. && pnpm audit --audit-level high && gitleaks git --redact --no-banner && gitleaks git --pre-commit --redact --no-banner && gitleaks git --pre-commit --staged --redact --no-banner",
"check": "pnpm check:fast"
```

## Measured, not yet enforced

| Metric | Today | Direction |
|---|---|---|
| Release `.app` size | n/a until slice 13 | record, must not grow > 10% per change |
| Cold start to first overlay frame | n/a until slice 10 | record, must not grow |

## Exceptions

| ID | Rule | Path | Reason | Owner | Expires |
|---|---|---|---|---|---|
| (none) | | | | | |
