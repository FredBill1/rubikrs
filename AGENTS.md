# AGENTS.md for `rubikrs`

`rubikrs` is a Rust and WebAssembly project for simulating and solving Rubik's Cubes on GitHub Pages. This file is for future agent sessions working in this repository. After reading it, you should be able to pick the right place to make a change, run the right verification commands, and preserve the Rust-first architecture instead of accidentally moving core behavior into the web shell.

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
| Development build | `npm run build:dev` | Builds wasm and TypeScript without WASM or Vite optimizations for faster iteration. |
| TypeScript check | `npm run check` | This repository does not define a separate JS/TS lint script. |
| Preview built site | `npm run preview` | Useful after `npm run build`. |
| 17x17 FPS benchmark | `npm run benchmark:fps` | Builds the production web app, serves it with Vite preview, and runs the Playwright 1080p continuous-turn benchmark. |

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

- 2x2 uses One-Phase Search
- 3x3 uses Kociemba's Two-Phase Algorithm
- NxN uses reduction methods

`solver-worker` returns camelCase JSON payloads because the shell replays them directly. For 3x3 center-slice states, the worker now normalizes the cube into a center-derived frame before solving, remaps the solution back into the runtime frame, and may append center-frame alignment turns so exported solved states stay canonical.

### Build and deploy shape

The web build is designed around GitHub Pages:

- the wasm build script compiles both Rust entrypoints with `wasm-pack`, injects `--cfg getrandom_backend="wasm_js"`, and writes the generated bindings into the Vite source tree
- Pages and CI both install Node 22, install `wasm-pack`, build the web app from `apps/web`, and target `wasm32-unknown-unknown`

Because GitHub Pages is the deployment target, which does not support `SharedArrayBuffer` or wasm threads, the architecture assumes ordinary web workers and message passing. We therefore use multiple single-threaded web workers for parallelism.

## Key repository conventions

- **Keep TypeScript thin.** The DOM shell should orchestrate the runtime, not reimplement cube logic, gesture semantics, or solver rules.
- **Preserve canonical cube notation.** Clockwise and counter-clockwise are defined from the perspective of looking directly at the named face. The codebase has tests and solver compatibility checks that assume this exact convention.
- **Treat generated wasm as part of the app import graph.** `npm run build:wasm` writes fresh generated bindings that the shell and worker import directly. If you change wasm exports or package names, update the generated import sites and rebuild before trusting TypeScript errors.
- **Respect solver cancellation and scene revisions.** Any feature that changes cube state while solving must keep the stale-result protections intact.
- **Direct manipulation is surface-based, not sticker-polygon-based.** Pointer picking now uses a virtual outer cube surface, then maps the hit point into a face grid. Blank-space drags should still orbit; only cube-surface drags should turn slices.
- **Bevy is intentionally trimmed for web delivery.** `Cargo.toml` enables only the needed features of Bevy
- **Large-order rendering is pool-based.** `rubik-app` now keeps a persistent Bevy visual pool per order and updates cubie/sticker transforms in place; avoid reintroducing full-cube despawn/respawn on ordinary turns.
- **NxN support depends on `start_layer` and `width` everywhere.** Keyboard shortcuts, DOM controls, scramble generation and runtime animation all assume inner and wide turns are expressed through that shared turn shape rather than special-case move types.
- **Orbit rotation uses absolute displacement from drag start, not frame-to-frame deltas.** `OrbitRig` saves `start_yaw`, `start_pitch`, and the pointer position when a drag begins. Every frame the orbit yaw/pitch is recomputed as `start + (current - start) * sensitivity` rather than accumulating `yaw += delta` per frame. This gives 1:1 pointer-to-view tracking for both mouse and touch, and matches the "grabbing the cube" feel. Pinch/scroll zoom remains delta-based.
- **Document ADR and design decisions in `docs`. Update `AGENTS.md` when necessary.** Always keep documentation up to date after making any changes.
- **Make Git commits with clear messages after any change.**
