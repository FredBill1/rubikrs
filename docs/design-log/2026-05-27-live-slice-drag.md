# Live slice drag interaction

## Context

Pointer and touch drags on a sticker used to resolve only on release: the Rust app inspected the drag vector, selected a `TurnCommand`, queued it, and then ran the normal quarter-turn animation from rest. That kept the web shell thin, but it made direct manipulation feel delayed because the selected slice did not move while the pointer was down.

## Decision

Direct slice manipulation remains in `crates/rubik-app` rather than TypeScript. The Bevy runtime now tracks an active slice drag after the drag passes the tap threshold on a sticker. The selected slice reuses the existing turn pivot and mesh partitioning path, but the pivot angle is driven by the pointer displacement projected onto the selected turn's screen-space motion.

On release, the active drag creates a snap request:

- At 10 degrees or less, the slice snaps back and no cube state is committed.
- Above 10 degrees through 90 degrees, the slice snaps to one quarter turn in the drag direction.
- Above 90 degrees, the slice snaps to the nearest 90-degree position, with the committed `TurnCommand` reduced to the equivalent quarter, half, inverse-quarter, or no-op state change.

The snap animation starts from the live drag angle instead of restarting from zero. Runtime state is committed only after the snap completes so the rendered slice and engine state stay aligned without a second animation.

## Consequences

The web DOM layer still only captures normalized touch points. Gesture semantics, slice selection, snap thresholds, and final cube mutation all remain Rust-side. The existing queued-turn animation path now supports a nonzero start angle and an explicit completion mode so keyboard/API turns keep their original behavior while direct drags can commit after their snap animation.
