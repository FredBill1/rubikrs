# 17x17 FPS benchmark and runtime sync tuning

## Context

The 17x17 target needs a reproducible browser benchmark instead of anecdotal smoothness checks. The benchmark must:

- run at **1920x1080**
- drive the real web app through **Playwright**
- continuously issue deterministic random turn commands
- prove the browser loop is not refresh-rate capped before trusting the measured cube FPS

## Benchmark pipeline

`apps/web` now includes a Playwright benchmark pipeline:

- `npm run benchmark:fps`
- `apps/web/playwright.fps.config.ts`
- `apps/web/tests/fps-benchmark.spec.ts`

The page exposes `window.__rubikrsBenchmark`, which:

- waits for the wasm runtime to be ready
- measures uncapped `requestAnimationFrame` throughput first
- switches the cube to **17x17**
- runs continuous seeded random turns (`seed = 20260520`, `maxWidth = 2`)
- gates each new turn on `animation_active` so every sampled frame is part of a real animation workload

The Playwright config uses a headless Edge/Chromium session with frame-limit / vsync disabling flags so the benchmark can observe FPS above desktop refresh rate when the machine allows it.

## Measurements

Hardware renderer observed during both runs:

- `ANGLE (NVIDIA, NVIDIA GeForce RTX 2080 Super (0x00001E93) Direct3D11 vs_5_0 ps_5_0, D3D11)`

Benchmark parameters:

- warmup: **240 frames**
- sample: **1200 frames**
- viewport: **1920x1080**
- workload: **continuous 17x17 random turns**

| Iteration | Change | rAF avg FPS | 17x17 avg FPS | 17x17 p50 FPS | 17x17 p95 frame ms |
| --- | --- | ---: | ---: | ---: | ---: |
| Baseline | Benchmark pipeline + lazy solver worker startup | 447.6 | 176.6 | 192.3 | 8.8 |
| Iteration 1 | Avoid per-frame full state clones when scene revision is unchanged; cache cubie/sticker slots in the visual pool | 453.9 | 177.6 | 196.1 | 8.9 |
| Iteration 2 | Remove the always-running 1080p stage scan animation so the compositor is not repainting a full translucent overlay every frame | 565.4 | 201.7 | 217.4 | 8.2 |
| Iteration 3 | Replace 1,538 per-cubie body render entities with persistent static/animated merged body meshes and update them only at animation boundaries (average of two confirmation runs) | 571.6 | 285.0 | 307.8 | 4.8 |
| Iteration 4 | Remove Bevy's `debug` feature from the shared production dependency set so release wasm no longer carries debug-only engine code paths (average of two confirmation runs) | 598.4 | 288.2 | 312.5 | 4.6 |
| Iteration 5 | Skip the full `apply_cube_state_to_pool` reset when the visual pool already matches the pending animation source revision, so ordinary turn starts stop rewriting every sticker before reparenting the animated subset (average of two confirmation runs) | 575.4 | 299.4 | 307.8 | 3.8 |
| Iteration 6 | Cache the current sticker visual state in `CubeVisualPool` so turn-start slice selection no longer scans all sticker entities through ECS queries before reparenting the animated subset (average of two confirmation runs) | 573.8 | 302.4 | 312.5 | 3.8 |

## Change details

Iteration 1 stays quality-safe and only removes CPU overhead in Rust:

- `sync_cube_visuals` now reads a lightweight runtime scene revision before cloning the full cube state, so steady-state animation frames stop cloning the entire 17x17 sticker buffer every frame.
- `CubeVisualPool` now caches cubie and sticker slot layouts, so state application no longer regenerates shell coordinates and sticker-slot metadata on every sync.
- shell startup no longer eagerly creates solver workers before the user asks for a solve, which keeps the benchmark baseline cleaner and reduces boot overhead.

Iteration 3 keeps the same lighting and sticker rendering quality while removing a large amount of Bevy entity overhead from the body shell:

- the 17x17 shell body no longer spawns one render entity per visible cubie body
- `CubeVisualPool` now owns two persistent body meshes: one attached to the cube root for resting geometry and one attached to the turn pivot for the animated slice
- starting a turn now rebuilds those two body meshes in place instead of reparenting 1,538 body entities through the ECS hierarchy
- finishing a turn restores the resting merged body mesh while stickers continue to use the existing per-sticker animation path

Iteration 4 is a build-configuration cleanup rather than a runtime behavior change:

- the shared workspace Bevy dependency no longer enables the `debug` feature for production wasm builds
- the generated `rubik_app_bg.wasm` shrank from roughly **51.7 MB** to **50.2 MB** raw in the benchmark build
- the runtime keeps the same lighting, shadows, animation timing, and shell behavior while shipping less engine code to the browser

Iteration 5 removes a redundant sticker/body reset at the start of ordinary animated turns:

- when the rendered visual pool is already at the immediately previous revision, `sync_cube_visuals` now skips the full `apply_cube_state_to_pool(from_state)` pass
- that avoids rewriting every sticker transform/material and re-restoring the body mesh immediately before `begin_turn_animation()` reparents only the affected slice
- the optimization preserves the same runtime state model and animation output, but cuts a large per-turn CPU spike from the average-frame path

Iteration 6 trims the remaining turn-start sticker selection overhead without changing rendering quality:

- `CubeVisualPool` now keeps the current `StickerVisual` state for every sticker entity alongside the entity handles
- `begin_turn_animation()` can select the animated sticker subset from that cached state instead of querying every sticker entity just to read its current cubie position
- `animate_turn_visuals()` updates the cached sticker state only for the animated subset when a turn completes, so the cache stays authoritative without reintroducing a full-pool rewrite

## Result

The benchmarked continuous 17x17 throughput has improved in six measured steps so far:

- **192.3 FPS -> 196.1 FPS** from Rust runtime-sync and visual-pool caching
- **196.1 FPS -> 217.4 FPS** from removing the full-stage scanline compositor animation
- **217.4 FPS -> 307.8 FPS** p50 from merged body-mesh batching, with average throughput rising from **201.7 FPS -> 285.0 FPS**
- **307.8 FPS -> 312.5 FPS** p50 from removing Bevy's production `debug` feature, with average throughput rising from **285.0 FPS -> 288.2 FPS**
- **288.2 FPS -> 299.4 FPS** average from skipping redundant turn-start pool resets, while p95 frame time improved from **4.6 ms -> 3.8 ms**
- **299.4 FPS -> 302.4 FPS** average from caching sticker visual state outside ECS queries, keeping both confirmation runs above the 300 FPS target

The current measured 1080p continuous-turn result is **302.4 FPS average** and **312.5 FPS p50** across two confirmation runs, with both runs clearing the 300 FPS target on average. The benchmark still has verified headroom above refresh rate (`requestAnimationFrame` average **573.8 FPS** in the same browser session).
