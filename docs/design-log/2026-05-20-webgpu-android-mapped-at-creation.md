# WebGPU Android: avoid `mappedAtCreation` for buffer uploads

## Problem

On some Android Chrome WebGPU implementations, calling `GPUDevice.createBuffer()` with
`mappedAtCreation: true` fails with a `RangeError` (even for tiny buffers), which then
panics inside `wgpu` when Bevy initializes GPU buffers using `wgpu::util::DeviceExt::create_buffer_init`.

This shows up as:

- `createBuffer failed, size (...) is too large for the implementation when mappedAtCreation == true`
- followed by a panic at `wgpu-*/src/backend/webgpu.rs:2275` (`create_buffer(...).unwrap()`)

Related upstream report: https://github.com/bevyengine/bevy/issues/23266

## Constraints

- Keep WebGPU enabled (no WebGL2 fallback).
- Don’t reduce rendering quality or scene complexity.
- Keep cube / solver / turn logic in Rust (web shell stays thin).

## Fix

We patch Bevy’s `RenderDevice::create_buffer_with_data` *on `wasm32`* to avoid the
`mappedAtCreation` path entirely:

- allocate the destination buffer with `mapped_at_creation: false`
- add `COPY_DST` usage
- upload initial bytes via `Queue::write_buffer`
- preserve wgpu’s `COPY_BUFFER_ALIGNMENT` padding behavior

This bypasses the `create_buffer_init` implementation that relies on `mappedAtCreation`.

Implementation lives in a local `bevy_render` patch via Cargo’s `[patch.crates-io]` so
the change is isolated and can be removed once upstream fixes land.

## Tradeoffs

- Slightly more work at initialization time due to an explicit queue write.
- Buffer usages become a strict superset on `wasm32` (`COPY_DST` is added), which is
  safe and matches how texture uploads already work.
