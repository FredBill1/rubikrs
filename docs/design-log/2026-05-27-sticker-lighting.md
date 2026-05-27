# Sticker lighting refresh

## Context

The cube renderer was technically efficient after the 17x17 mesh-batching work, but the sticker
surface still read as cheap plastic:

- saturated primary colors looked artificial under tone mapping
- a mostly frontal key light flattened the visible face
- the unlit side of the cube fell too dark
- sticker sides used the same albedo as the top face, so the raised stickers looked blocky

## Decision

Keep the quality work in `crates/rubik-app` and keep the web shell thin. The renderer now uses
warmer physical sticker colors, non-metallic plastic-like `StandardMaterial` settings, and a
rebalanced low-cost light setup:

- warm off-axis key point light for visible face shape
- one shadow-casting directional light with the existing WebGL2 one-cascade shadow range
- brighter cool ambient light to keep the cube from crushing to black

Instead of adding a custom ray-tracing pass or per-frame global illumination, sticker vertex colors
now bake a cheap color-bleed approximation whenever the merged sticker meshes are rebuilt. Each
physical face keeps an average sticker color. Sticker vertices near adjacent cube faces receive a
small amount of that neighboring face color, strongest on stickers along edges and corners. The
same baked pass adds subtle orientation-based light shaping while keeping one repeated vertex color
per sticker so the hot 17x17 rebuild path stays close to the original batched renderer.

## Consequences

The effect is not true GI, but it gives the ray-tracing-demo cue the app needs: colored faces softly
influence nearby stickers without adding a per-frame shader pass, extra sticker entities, or a
runtime light per sticker. The cost is paid only when sticker meshes are rebuilt at order changes,
state resets, or turn partition boundaries.

Local benchmark context on May 27, 2026:

- Unmodified HEAD on this machine: 17x17 continuous-turn p50 **166.7 FPS**.
- This change: 17x17 continuous-turn p50 **158.7 FPS** and batched p50 **555.6 FPS**.
