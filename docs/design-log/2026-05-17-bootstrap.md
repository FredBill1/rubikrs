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
- Keep the first async solve slice intentionally narrow: one dedicated Rust wasm worker, a small JSON protocol, depth-limited IDDFS, and terminate/recreate cancellation on the TS side before attempting a real worker pool.
- Expose solver turns from Rust in camelCase and keep the browser bridge typed to that payload shape; otherwise wasm-bindgen calls quietly coerce `undefined` turn codes and replay the wrong move.
- Avoid `std::time::Instant` in the wasm runtime shell state; use a platform-safe millisecond clock so browser-side turns, scrambles, and solve replays do not panic on unsupported wasm timing APIs.
- A second round of Bevy feature pruning (dropping `bevy_state`, `default_font`, `multi_threaded`, and `x11`) is safe for the current slice but only shaves a tiny amount off the wasm artifact, so larger bundle wins will likely need architectural rather than flag-level changes.
- Move the solver shell from one long-running worker to a small worker pool that searches one exact depth at a time and partitions the six root faces across lanes; this preserves GitHub Pages compatibility, makes cancellation simple, and gives deterministic global "first solved depth" behavior without relying on `SharedArrayBuffer`.
- Keep turn animation state in the Rust runtime as a lightweight transition snapshot (`from_state + turn + revision`) and let Bevy animate only the affected cubie layer; when a new revision lands mid-animation, snap the old animation away and start from the latest committed transition instead of queueing stale visuals.
- Use a tiny set of curated isometric orbit snap targets rather than full free-camera quantization; this keeps mouse/touch orbit feeling loose during drag but still lets release settle back onto readable three-face compositions.
- For non-3x3 states generated inside the current session, expose the full Rust turn history to the solver worker and let the worker replay the inverse history as a feasible async solve path; this preserves the canonical state schema while giving NxN a deterministic fallback before a real reduction solver lands.
- Keep keyboard cube turns Rust-native and scale them with simple modifiers instead of duplicating NxN turn logic in TypeScript: digit keys select the starting layer, `Alt` widens the turn to two layers, `Shift` flips direction, and `Ctrl` keeps the half-turn override.
- Reuse the same `start_layer` / `width` turn contract in the DOM button shell so touch users can still reach inner and wide NxN turns even before direct canvas picking exists.
- Add a Rust-native `kewb` two-phase 3x3 fallback behind the existing shallow exact-depth worker search so the browser can escalate deeper 3x3 solves without blocking the UI; this is explicitly an interim usability slice, not the final strict-optimal solver promised for 3x3.
- Build the worker wasm with `getrandom`'s `wasm_js` backend enabled so `kewb`'s dependency chain works on `wasm32-unknown-unknown` in both local builds and Pages-style CI.
- Lock `U/D` turn semantics to standard cubing notation and compare every exported single-move facelet state against `kewb`'s own move tables; this catches geometry / notation drift before it turns async solver results into incorrect runtime replays.
- Add direct canvas face-tap turning through camera projection instead of mesh/ray picking: project the six outer-face centers into screen space, discard faces not pointing toward the camera, and accept taps only inside a conservative projected face radius. This gives mouse/touch cube turns without pulling in heavier picking infrastructure.
- Separate orbit from tap with a drag-distance threshold on both mouse and touch. Orbit should only start after the pointer clearly moves; otherwise a release is treated as a face tap. This avoids the earlier failure mode where the orbit handler consumed the same press that should have been a cube turn.
- Make the caption overlay visually present but input-transparent, and set `touch-action: none` on the canvas so the shell copy and the browser's default pan/zoom behavior do not steal cube gestures on mobile.

## Next risks to validate

- Bevy canvas mounting in a GitHub Pages-friendly base path
- Input delivery in the browser runtime
- wasm build ergonomics between local development and CI
- touch interaction polish beyond baseline orbit / pinch
- layer picking and direct canvas turn gestures, which are still open even though animated turns and orbit snapping are now live
- the current solver is still not the required 3x3 strict-optimal / NxN final solver set; 3x3 remains shallow search, while NxN now has an in-session recorded-history feasible fallback rather than a full general-purpose reduction pipeline

