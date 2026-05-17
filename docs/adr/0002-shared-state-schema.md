# ADR 0002: Canonical shared cube state schema

## Status

Accepted

## Context

The runtime, solver worker, persistence layer, and thin TypeScript shell all need to agree on one cube-state contract before the real move engine arrives. Without that contract, later work on validation, import/export, replay, and solve orchestration would fragment quickly.

## Decision

Adopt a versioned sticker-based schema:

- `CubeState.version` starts at `1`
- `CubeState.order` stores the NxN order directly
- `CubeState.stickers` stores every sticker in canonical `U, R, F, D, L, B` face order
- within each face, stickers use row-major order as viewed directly from outside that face
- solved colors are fixed as `white, red, green, yellow, orange, blue`

Move commands are represented structurally instead of as free-form notation strings:

- `face`
- `start_layer`
- `width`
- `rotation`

Clockwise / counter-clockwise are defined from the perspective of looking straight at the named face from outside the cube.

This makes the schema unambiguous for NxN support and keeps wide turns / inner-slice turns explicit.

## Consequences

### Positive

- JSON import/export stays deterministic and easy to validate.
- Worker messages can use the same contract as saved states.
- NxN support is built into the move envelope from the start.

### Negative

- Human-readable notation parsing is deferred to a later layer.
- Sticker-level schema is larger than compact cubie-coordinate formats.

