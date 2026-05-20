# WebGL2 render tuning (Bevy 0.18)

## Context

When Bevy runs on the WebGL2 backend (`AdapterInfo.backend == Gl`), several optional rendering paths are unavailable (notably compute-shader-based features). Bevy logs warnings when those plugins decline to load.

In `rubikrs`, WebGL2 also has stricter shadow constraints:

- Directional light cascades default to **1 cascade** on WebGL2.
- The default `CascadeShadowConfigBuilder` still uses a large `maximum_distance` (150), which spreads a single shadow map over a large region and makes edges look harsh / aliased in close-up scenes like a Rubik’s Cube.

## Changes

`crates/rubik-app/src/lib.rs` applies WebGL2-specific defaults:

- Suppress known-safe renderer logs from Bevy modules that are expected to be unsupported on WebGL2.
- Disable *point light shadow casting* on WebGL2 to reduce per-frame shadow pass cost and avoid hard, unfiltered point-shadow edges dominating the scene.
- Tighten the *directional light* shadow range (`maximum_distance: 25.0`) so the single WebGL2 cascade has much higher texel density in the active play space.

## Notes

- If WebGL2 devices still show shadow aliasing, prefer further reducing `maximum_distance` before increasing `DirectionalLightShadowMap.size`, as larger shadow maps can hurt performance and may reduce device compatibility.
