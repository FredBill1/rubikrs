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

## Change details

Iteration 1 stays quality-safe and only removes CPU overhead in Rust:

- `sync_cube_visuals` now reads a lightweight runtime scene revision before cloning the full cube state, so steady-state animation frames stop cloning the entire 17x17 sticker buffer every frame.
- `CubeVisualPool` now caches cubie and sticker slot layouts, so state application no longer regenerates shell coordinates and sticker-slot metadata on every sync.
- shell startup no longer eagerly creates solver workers before the user asks for a solve, which keeps the benchmark baseline cleaner and reduces boot overhead.

## Result

The first runtime-sync tuning step improved the measured continuous 17x17 median throughput from **192.3 FPS** to **196.1 FPS** at 1080p without changing cube visuals.
