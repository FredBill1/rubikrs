# AGENTS.md for `rubikrs`

`rubikrs` is a Rust and WebAssembly project for simulating and solving Rubik's Cubes on GitHub Pages. This file is for future agent sessions working in this repository. After reading it, you should be able to pick the right place to make a change, run the right verification commands, and preserve the Rust-first architecture instead of accidentally moving core behavior into the web shell.

## Build, test, and check commands

### Repository-wide verification

Run these from the repository root unless noted otherwise.

| Purpose | Command | Notes |
| --- | --- | --- |
| Full Rust test suite | `cargo test --workspace --locked` | Mirrors CI. |
| Targeted Rust crate tests | `cargo test -p rubik-app -- --nocapture` | Replace `rubik-app` with `rubik-core` or `rubik-solver` as needed. |
| Targeted solver integration tests | `cargo test -p rubik-solver --test solver_tests -- --nocapture` | Tests N=2..7 single-turn, inner-layer, edge-case, and 200-step scramble solves. |
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
| 17x17 FPS benchmark | `npm run benchmark:fps` | Builds the production web app, serves it with Vite preview, and runs the Playwright 1080p benchmark covering raw rAF, single-turn continuous stress, and batched parallel-turn stress. |

## Key repository conventions

- **Keep TypeScript thin.** The DOM shell should orchestrate the runtime, not reimplement cube logic, gesture semantics, or solver rules.
- **Keep direct manipulation Rust-side.** Sticker drag selection, live slice angles, snap thresholds, and final `TurnCommand` commits live in `crates/rubik-app`; the web shell should only forward DOM input state.
- **Keep rendering polish Rust-side.** Sticker material, lighting, color palette, and baked color-bleed approximations live in `crates/rubik-app`; prefer mesh-rebuild-time work over per-frame JS or per-sticker entities unless a benchmark justifies it.
- **Implement correct rubik's cube solving algorithms.** The implemented algorithm should be able to solve any valid cube state for N>=2 in a reasonable time. DO NOT USE any "max iteration" limits or fallback logic, because the correctly implemented algorithm should work end-to-end.
- **Documentations and Git commits.** Always keep documentation (ADR and design decisions in `./docs`. `./AGENTS.md` for short bullet-point notes) up to date and make git commits after any change. Always run `npm run build:dev` before committing to make sure the code compiles.
