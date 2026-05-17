# Bootstrap log

## Decisions captured during the first implementation slice

- Keep TypeScript intentionally thin and do not let it own the 3D runtime.
- Build `rubik-app` with Bevy and enable `webgl2` explicitly so the browser slice does not depend on WebGPU availability.
- Generate wasm into the Vite source tree during the build step instead of copying artifacts into `public/`, which keeps the app on Vite's asset pipeline and avoids stale unversioned wasm assets on Pages deploys.
- Treat `solver-worker` as a dedicated wasm entrypoint from day one, even before solve logic exists, so the worker-pool architecture is encoded in the workspace layout.
- Pin the shared sticker schema to canonical `U, R, F, D, L, B` face order with row-major scan order as viewed from outside each face.
- Define clockwise / counter-clockwise from the perspective of looking straight at the named face, then lock that rule with permutation tests.
- Model history as a linear applied-turn log plus redo stack, instead of trying to reconstruct history from inverse moves later.
- Replace Bevy's default feature set with an explicit runtime whitelist once the first browser slice is stable; this removes UI, audio, scene, picking and glTF overhead from the wasm bundle while keeping the current 3D runtime intact.
- Keep the 3D camera on `Tonemapping::None` for the trimmed WebGL2 build instead of depending on LUT-backed tonemapping variants that are easy to break when pruning Bevy features.
- Add touch orbit controls directly in the Bevy runtime (single-finger orbit + two-finger pinch zoom) so mobile interaction stays in the Rust-side input layer rather than fragmenting between TS and Bevy.

## Next risks to validate

- Bevy canvas mounting in a GitHub Pages-friendly base path
- Input delivery in the browser runtime
- wasm build ergonomics between local development and CI
- touch interaction polish beyond baseline orbit / pinch
- animated turns and layer picking, which are still open even though the browser shell and state-driven renderer are now live

