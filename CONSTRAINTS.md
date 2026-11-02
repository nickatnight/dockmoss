# Constraints

Last reviewed: 2026-11-02 (draft, emgee job 001, defaults pending Gate 1)

Read alongside `CLAUDE.md`. This file does not get weakened to make a change pass.

## Floor (always enforced)

- No new `#[allow(...)]`. Use `#[expect(lint, reason = "...")]` at the narrowest scope.
- No `todo!()`, `unimplemented!()`, or `.unwrap()` outside `#[cfg(test)]` (enforced by `clippy::unwrap_used`, `clippy::todo`, `clippy::unimplemented` set to deny in `[lints]`).
- No `let _ = <Result>` that discards an error silently. Log it or propagate it.
- No `#[ignore]`d, skipped or deleted tests without a reason in the commit message.
- No `@ts-ignore` / `@ts-expect-error` / `eslint-disable` without a reason comment.
- Every `unsafe` block lives in `src-tauri/src/macos/` and has a `// SAFETY:` comment (`clippy::undocumented_unsafe_blocks` set to deny).
- No secrets in source.

## Enforced with numbers

| Dimension | Rule | Checked by | Runs at | Why |
|---|---|---|---|---|
| Format | Zero diffs | `cargo fmt --check` | every edit | Trivial to satisfy; keeps diffs reviewable |
| Lint (Rust) | Zero warnings, pedantic on | `cargo clippy --all-targets -- -D warnings` | every edit | Lints in `Cargo.toml` are the project's bar |
| Types (TS) | Zero errors, `strict: true` | `pnpm exec tsc --noEmit` | every edit | Keeps the IPC wrapper honest |
| Tests | All pass | `cargo test` (in `src-tauri/`) | task end | |
| Coverage | ≥ 80% line coverage of `growth/`, `usage/`, `geometry.rs`, `settings.rs` | `cargo llvm-cov --summary-only` filtered to those paths | task end | Pure logic is fully testable. `macos/` and the render code are verified manually, so they are excluded instead of padded. |
| Deps: advisories | No advisory at high or above; no unknown licences | `cargo deny check advisories licenses` | task end | External opinion (RustSec DB) |
| Deps: JS | Nothing at high or above | `pnpm audit --audit-level high` | task end | External opinion |
| Secrets | No findings | `gitleaks detect --redact --no-banner` | task end | `--redact` keeps any match out of transcripts |
| Idle CPU | < 1% with the Dock idle and no icon ≥ Patchy | Activity Monitor / `ps -o %cpu -p <pid>` sampled 10 s (manual) | checkpoints A, C, D | Core promise in SUMMARY.md "Battery" |

Dropped: Lighthouse and axe. A desktop overlay has no URL to audit, and the settings form is covered by review.

Scripts in `package.json` mirror this file; if they drift, this file wins:

```json
"check:fast": "cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cd .. && tsc --noEmit",
"check:task": "pnpm check:fast && cd src-tauri && cargo test && cargo llvm-cov --summary-only && cargo deny check advisories licenses && cd .. && pnpm audit --audit-level high && gitleaks detect --redact --no-banner",
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
