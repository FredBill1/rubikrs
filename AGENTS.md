# AGENTS.md for `rubikrs`

This file is for future agent sessions working in this repository. After reading it, you should be able to pick the right place to make a change, run the right verification commands, and preserve the Rust-first architecture instead of accidentally moving core behavior into the web shell.

## Build, test, and check commands

### Repository-wide verification

Run these from the repository root unless noted otherwise.

| Purpose | Command | Notes |
| --- | --- | --- |
| Full Rust test suite | `cargo test --workspace --locked` | Mirrors CI. |
| Targeted Rust crate tests | `cargo test -p rubik-app -- --nocapture` | Replace `rubik-app` with `rubik-core` or `solver-worker` as needed. |
| Single Rust test | `cargo test -p rubik-app virtual_surface_maps_front_face_seams_to_a_real_cubie -- --nocapture` | Use the same pattern for any named test in any crate. |

### Web shell commands

Run these from `apps/web`.

| Purpose | Command | Notes |
| --- | --- | --- |
| Install dependencies | `npm ci --prefer-offline --no-audit` | Matches CI and Pages workflows. |
| Build wasm packages only | `npm run build:wasm` | Compiles both Rust wasm entrypoints into the generated Vite source tree. |
| Start local dev shell | `npm run dev` | Runs a dev wasm build first, then starts Vite. |
| Production build | `npm run build` | Runs wasm build, TypeScript compile, and Vite build. |
| TypeScript check | `npm run check` | This repository does not define a separate JS/TS lint script. |
| Preview built site | `npm run preview` | Useful after `npm run build`. |

## High-level architecture

### Rust owns the product logic

The repository is a Rust workspace with three crates:

- `rubik-core` is the canonical cube domain layer. It owns the schema, cube order validation, turn representation, state import/export, scramble generation, and undo/redo-ready state mutation.
- `rubik-app` is the Bevy runtime compiled to wasm. It owns rendering, input handling, turn animation, timer/status reporting, and the authoritative in-memory runtime state.
- `solver-worker` is a separate Rust wasm entrypoint loaded inside web workers so solving does not block the Bevy runtime.

The web app is intentionally thin. It mounts the canvas, renders the DOM control shell, polls runtime status, starts or cancels solve requests, and bridges browser-only concerns such as file import/export and GitHub Pages base paths. If a change affects cube state, turns, validation, solving, or input semantics, the default place to implement it is Rust, not TypeScript.

### Shared state and turn contract

All layers speak the same cube model:

- Sticker state is canonical `U, R, F, D, L, B` face order.
- Each face is row-major as viewed from outside the cube.
- `TurnCommand` uses `face`, `start_layer`, `width`, and `rotation`, and that same contract is reused by the core engine, Bevy runtime, TS shell controls, and solver worker messages.

That shared contract is what makes NxN support work across manual turns, drag gestures, import/export, scramble, undo/redo, and solver replay. Preserve it rather than introducing layer-specific side contracts in the shell.

### Runtime and shell interaction

The Bevy runtime exposes a wasm API that the shell calls directly:

- status and scene state come from JSON exports such as `runtime_status_json`, `export_cube_state`, and `export_turn_history_json`
- user actions flow back through wasm entrypoints such as `apply_turn`, `scramble_cube`, `set_cube_order`, `undo_turn`, and `redo_turn`

The shell treats the runtime as authoritative. It also watches `scene_revision` so any direct runtime change cancels in-flight solve work instead of replaying stale solver output onto a newer cube state.

### Solver orchestration

The solver flow is split between the shell and `solver-worker`:

- the shell partitions the six outer faces across a worker pool for exact-depth search
- 3x3 requests escalate to a Rust-side fallback after shallow search is exhausted
- NxN requests can use recorded turn history as a feasible inverse-replay fallback

`solver-worker` returns camelCase JSON payloads because the shell replays them directly. For 3x3 center-slice states, the worker now normalizes the cube into a center-derived frame before solving, remaps the solution back into the runtime frame, and may append center-frame alignment turns so exported solved states stay canonical.

### Build and deploy shape

The web build is designed around GitHub Pages:

- Vite `base` comes from `RUBIK_BASE_PATH`
- the wasm build script compiles both Rust entrypoints with `wasm-pack`, injects `--cfg getrandom_backend="wasm_js"`, and writes the generated bindings into the Vite source tree
- Pages and CI both install Node 22, install `wasm-pack`, build the web app from `apps/web`, and target `wasm32-unknown-unknown`

Because Pages is the deployment target, the architecture assumes ordinary web workers and message passing. Do not introduce `SharedArrayBuffer`, wasm threads, or concurrency designs that depend on cross-origin isolation.

## Key repository conventions

- **Keep TypeScript thin.** The DOM shell should orchestrate the runtime, not reimplement cube logic, gesture semantics, or solver rules.
- **Preserve canonical cube notation.** Clockwise and counter-clockwise are defined from the perspective of looking directly at the named face. The codebase has tests and solver compatibility checks that assume this exact convention.
- **Treat generated wasm as part of the app import graph.** `npm run build:wasm` writes fresh generated bindings that the shell and worker import directly. If you change wasm exports or package names, update the generated import sites and rebuild before trusting TypeScript errors.
- **Respect solver cancellation and scene revisions.** Any feature that changes cube state while solving must keep the stale-result protections intact.
- **Direct manipulation is surface-based, not sticker-polygon-based.** Pointer picking now uses a virtual outer cube surface, then maps the hit point into a face grid. Blank-space drags should still orbit; only cube-surface drags should turn slices.
- **Bevy is intentionally trimmed for web delivery.** The workspace enables an explicit Bevy feature set with `webgl2` instead of relying on the default desktop-oriented stack.
- **NxN support depends on `start_layer` and `width` everywhere.** Keyboard shortcuts, DOM controls, scramble generation, runtime animation, and solver replay all assume inner and wide turns are expressed through that shared turn shape rather than special-case move types.
- **The current 3x3 solver is not the final promised optimal solver.** The exact-depth worker search plus fallback path is the current implementation. Preserve behavior, but do not document or assume it is the final long-term solving architecture.
