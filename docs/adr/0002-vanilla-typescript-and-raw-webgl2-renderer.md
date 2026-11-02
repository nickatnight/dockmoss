# ADR-0002: Vanilla TypeScript and one raw WebGL2 shader for the moss

## Status

Accepted at Gate 1, 2026-11-02. Slice 1 ships the Vite + vanilla TypeScript frontend with no window. The renderer arrives in slices 9 to 11, and the settings form in slice 12.

## Date

2026-11-02

## Context

The overlay draws procedural moss (noise-driven patches, slight sway) on every Dock app icon. The overlay is always on screen above the Dock, so WebKit never throttles it for being hidden. Its idle cost lands directly on the battery promise in `SUMMARY.md`: "keep the render loop idle when nothing is animating", and "use one shared shader or instanced draw rather than a layer per icon". Apart from the moss, the only UI is a settings window with a few fields: thresholds, intensity, exclusions, launch at login and fast-forward.

## Decision

- **Frontend:** Vite and vanilla TypeScript (`strict: true`), with `pnpm` and `pnpm-lock.yaml` as the only lockfile. No UI framework.
- **Rendering:** raw WebGL2 with one shader program for all icons. Per-icon data (rect, overgrowth, seed) goes into a uniform array or UBO, and everything draws in one call.
- **Idle:** when geometry and overgrowth are unchanged, the `requestAnimationFrame` loop stops. Rust sends geometry over a `tauri::ipc::Channel` only when a rect moves by 0.5 px or more.
- **Settings:** a plain HTML form.
- **IPC:** every `invoke` call goes through one typed module, `src/ipc.ts`, that mirrors the Rust command signatures.

## Alternatives considered

### three.js
- Pros: a scene graph and helpers.
- Cons: a large dependency for one full-screen quad and one fragment shader.
- Rejected in the MVP spec.

### A UI framework for the settings window
- Cons: runtime and build weight for one small form.
- Rejected in the MVP spec in favour of a plain HTML form.

### One canvas or layer per icon
- Cons: cost grows with the number of Dock icons. `SUMMARY.md` asks for one shared shader or an instanced draw instead.
- Rejected.

## Consequences

- The shader code (fbm noise, rounded-rect clipping at a corner radius of about 22.5% of icon size, Overgrown-stage sprites) is written by hand and checked by screenshots (slices 9 to 11), not unit tests. CONSTRAINTS.md leaves it out of the coverage figure.
- Idle cost depends on the frontend keeping rAF stopped and running no CSS animations at rest. From slice 2 on, the CONSTRAINTS.md idle-CPU check has to include WebKit's WebContent and GPU processes (follow-up for job 002 in `docs/mvp/plan.md`).
- The frontend has no runtime dependency except `@tauri-apps/api`.
