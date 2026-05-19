# Absolute-displacement orbit for mouse and touch

## Decision

Replace frame-to-frame incremental orbit rotation (`yaw += delta * sensitivity`) with
absolute displacement from the drag start position
(`yaw = start_yaw + (current_position - start_position) * sensitivity`) for both
mouse and touch camera control.

## Rationale

The previous incremental approach accumulated per-frame deltas. On touch this
felt indirect because the view did not track the finger 1:1 — returning the
finger to the start position did not return the view to the start orientation.
The mouse used Bevy `MouseMotion` events natively, which are inherently
delta-based, but that created an inconsistent feel between mouse and touch
orbiting.

Switching both input paths to absolute displacement makes orbit feel like
"grabbing and rotating" the cube: the view tracks the pointer displacement
proportionally, and releasing the pointer snaps back to an isometric view.

## Implementation

In `crates/rubik-app/src/lib.rs`:

- Added `touch_start_yaw`, `touch_start_pitch`, `touch_start_position`,
  `touch_start_center` fields to `OrbitRig` for touch.
- Added `mouse_start_yaw`, `mouse_start_pitch`, `mouse_start_position` fields
  to `OrbitRig` for mouse.
- Replaced `position_delta()` (frame-to-frame delta) with direct
  `current - start` displacement in the single-finger, multi-finger, and mouse
  orbit application blocks.
- Removed the `previous_single_touch_position` and `previous_touch_center`
  fields, which stored per-frame previous positions for delta computation.
- Pinch zoom and mouse scroll zoom remain incremental; their delta-based
  semantics are the natural fit for distance/scroll events.

Sensitivity values (`0.008` for yaw, `0.006` for pitch) are unchanged because
the telescoping sum of per-frame deltas equals the total displacement.
