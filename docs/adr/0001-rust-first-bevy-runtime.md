# ADR 0001: Rust-first Bevy runtime for the browser

## Status

Accepted

## Context

The simulator targets GitHub Pages, must run in the browser, and is expected to keep Rust in the dominant role. The project also needs a thin TypeScript surface for page bootstrapping and browser-only affordances such as file pickers and download hooks.

## Decision

Use a Rust-first workspace with:

- `crates/rubik-app` as the Bevy-driven browser runtime
- `crates/rubik-core` as the pure domain crate
- `crates/solver-worker` as the future worker-side wasm entrypoint
- `apps/web` as a minimal TypeScript shell that mounts the canvas and boots the wasm runtime

GitHub Pages is treated as the baseline deployment target, so the architecture avoids SharedArrayBuffer and wasm threads. Parallel solve work is reserved for multiple single-threaded workers.

## Consequences

### Positive

- Rendering, interaction, animation, and state orchestration stay inside Rust.
- The boundary between domain logic and browser glue remains explicit.
- Future non-web targets can reuse more of the runtime and core code.

### Negative

- Wasm payload size and startup cost will be higher than a thin Three.js shell.
- Bevy web compatibility must be validated early instead of assumed.
- Asset loading and GitHub Pages base-path handling need explicit care on the Rust side.

