# 2026-05-27 NxN Solver Route

## Context

The previous public route sent every non-wasm `rubik_solver::solve` call through `rcube-rs`, while the wasm entry point special-cased 2x2 and 3x3. For 4x4 and larger cubes this produced valid but very long constructive solutions, and made it hard to evolve the bounded big-cube path independently.

## Change

Added `crates/rubik-nxn-solver` and routed orders `4..=8` through it from `rubik-solver::solve`.

Current default pipeline:

1. Validate the incoming sticker state and order.
2. Produce a deterministic constructive seed.
3. Compress the seed with an order-aware axis-block pass:
   - expand wide moves to canonical `(axis, depth, amount)` slices,
   - accumulate same-axis contiguous blocks modulo four,
   - emit adjacent equal-depth ranges as wider `TurnCommand`s,
   - repeat to a fixed point.
4. Apply the final turn list back to the original `CubeState` and return an error if verification fails.

The 4..=8 route does not catch a new-solver error and then silently return the old `rcube-rs` answer. Orders `>=9` still use the legacy `rcube-rs` path so very large cubes remain supported without sending them into bounded search work.

## 4x4 Phase-Search Experiment

The new crate also contains a feature-gated `phase-search-4x4` path. It converts a constructive seed into a setup algorithm, calls `twsearch`'s 4x4 multiphase search, decodes the returned algorithm to `TurnCommand`s, and verifies the result with `rubik-core`.

Measured on the integration seed `4x4, scramble seed=1`:

- constructive seed: `1307` turns
- default axis/wide compression: `822` turns
- `phase-search-4x4`: `54` turns

The phase-search path is not enabled by default because the current `twsearch::solve_known_puzzle` call does not observe this project's cancellation token and took roughly `110s` in a debug test build for that seed. Keeping it behind a feature preserves a working short-solution prototype without making normal dev/test/wasm solves block the UI or violate the existing timeout tests.

## Follow-Up

The next step is to replace the constructive-seed bridge with a native, cancellable reduction or phase-search implementation that accepts `CubeState` directly. Once that path can respect cancellation and stay within the app's solve-time budget, the 4x4 phase search can become the default route and the same architecture can be extended by orbit to 5x5..8x8.
