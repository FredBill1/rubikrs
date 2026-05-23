# 2026-05-23-solver-rewrite

Replaced the buggy `solver-worker` crate with a new `rubik-solver` crate implementing a from-scratch Rubik's Cube solver using deterministic algorithms with no third-party solving libraries.

## Architecture

**`rubik-solver`** is a Rust crate that builds as both a native library (rlib) and a WebAssembly module (cdylib). It depends only on `rubik-core` for state types and turn application.

### Pipeline

- **N=2**: Beginner's method: D layer → OLL → PLL (corner-only algorithms)
- **N=3**: Systematic beginner's method: cross → D corners → middle edges → OLL → PLL
- **N≥4**: Reduction method:
  1. Center solving via commutator-based batch reduction
  2. Edge pairing via free-slice method
  3. Reduce to 3×3 and solve using the 3×3 solver

### Type System Changes

To support arbitrary-order cubes (N≥2 up to N=2048):

- **CubeOrder**: inner type changed from `u8` to `u32`
- **TurnCommand.start_layer** and **TurnCommand.width**: changed from `u8` to `u32`
- **MAX_CUBE_ORDER**: increased from 17 to 2048
- **wasm-bindgen API**: `set_cube_order(order: u32)`, `apply_turn(..., start_layer: u32, width: u32)`

### Web Worker Integration

- `solver.worker.ts` lazy-loads the generated `rubik_solver.js` wasm module
- Calls `solve_request_json()` which takes a JSON request with `stateJson` and returns a JSON response with `turns`
- Cancel is supported via `request_cancel_solver()` wasm export and `cancel` worker messages
- `main.ts` sends cancel messages to workers before terminating them

## Key Decisions

1. **No third-party solving libraries**: The solver implements all algorithms from scratch (beginner's method for 2×2/3×3, commutator-based reduction for N≥4). The `min2phase` dependency has been removed.
2. **No fallback logic**: All algorithms are deterministic and guaranteed to complete without max iteration/depth limits.
3. **Systematic beginner's method**: For 2×2 and 3×3, each step detects the specific case and applies the correct algorithm, rather than trying turns greedily.
4. **Commutator-based centers**: N≥4 center solving uses the commutator `r U' l' U r' U' l U` pattern adapted from RCube's approach, with batch row operations for large N.

## Files Changed

- **Modified**: `crates/rubik-core/src/schema.rs` (TurnCommand u8→u32), `crates/rubik-core/src/order.rs` (CubeOrder u8→u32, MAX=2048)
- **Modified**: `crates/rubik-core/src/engine.rs` (type casts for u32)
- **Rewritten**: `crates/rubik-solver/src/lib.rs` (complete from-scratch solver)
- **Rewritten**: `crates/rubik-solver/Cargo.toml` (removed min2phase)
- **Rewritten**: `crates/rubik-solver/tests/solver_tests.rs` (comprehensive test suite with timeouts)
- **Modified**: `crates/rubik-app/src/lib.rs` (type migrations u8→u32)
- **Modified**: `apps/web/src/solver.worker.ts` (cancellation support)
- **Modified**: `apps/web/src/main.ts` (cancel message propagation)
- **Modified**: `.gitignore` (updated generated path)
- **Updated**: `AGENTS.md`
