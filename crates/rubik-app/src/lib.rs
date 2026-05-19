#![forbid(unsafe_code)]

use std::{cell::RefCell, collections::BTreeSet};

use bevy::{
    core_pipeline::tonemapping::Tonemapping,
    input::{
        mouse::{MouseMotion, MouseScrollUnit, MouseWheel},
        touch::Touches,
    },
    prelude::*,
    window::{Window, WindowPlugin},
};
use rubik_core::{
    CubeEngine, CubeOrder, CubeState, Face, RotationAmount, StickerColor, TurnCommand,
};
use serde::Serialize;

#[cfg(target_arch = "wasm32")]
use serde::Deserialize;

#[cfg(target_arch = "wasm32")]
use js_sys::{Array, Date};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{JsValue, prelude::wasm_bindgen};

thread_local! {
    static RUNTIME: RefCell<RuntimeBridge> = RefCell::new(RuntimeBridge::new(CubeOrder::standard()));
}

#[derive(Component)]
struct CubeVisual;

#[derive(Component)]
struct CubeVisualRoot;

#[derive(Component)]
struct TurnAnimationPivot;

#[derive(Resource, Clone)]
struct ShellConfig {
    base_path: String,
    canvas_selector: String,
}

#[derive(Resource)]
struct OrbitRig {
    yaw: f32,
    pitch: f32,
    radius: f32,
    auto_spin: bool,
    mouse_drag_button: Option<MouseButton>,
    touch_drag_mode: TouchOrbitMode,
    touch_mouse_suppression_secs: f32,
    snap_target: Option<Vec2>,
    touch_start_yaw: f32,
    touch_start_pitch: f32,
    touch_start_position: Option<Vec2>,
    touch_start_center: Option<Vec2>,
    previous_pinch_distance: Option<f32>,
}

#[derive(Resource, Default)]
struct DirectTurnInputState {
    mouse_candidate: Option<PointerGestureCandidate>,
    touch_candidate: Option<TouchGestureCandidate>,
    queued_turn: Option<TurnCommand>,
}

#[derive(Resource, Default)]
struct VisualSyncState {
    rendered_revision: u64,
    active_animation: Option<ActiveTurnAnimation>,
    completed_animation_revision: Option<u64>,
}

#[derive(Debug, Clone)]
struct RuntimeBridge {
    engine: CubeEngine,
    scene_revision: u64,
    last_message: String,
    timer: RuntimeTimer,
    last_transition: Option<RuntimeTransition>,
    animation_active: bool,
}

#[derive(Debug, Clone, Default)]
struct RuntimeTimer {
    elapsed_before_millis: u64,
    started_at_millis: Option<u64>,
}

#[derive(Debug, Serialize)]
struct RuntimeStatus {
    order: u8,
    move_count: usize,
    redo_depth: usize,
    is_solved: bool,
    timing_active: bool,
    animation_active: bool,
    elapsed_millis: u64,
    scene_revision: u64,
    last_message: String,
    recent_turns: Vec<String>,
}

#[derive(Debug, Clone)]
struct RuntimeSnapshot {
    state: CubeState,
    scene_revision: u64,
    transition: Option<RuntimeTransition>,
}

#[derive(Debug, Clone)]
struct RuntimeTransition {
    scene_revision: u64,
    from_state: CubeState,
    animation: Option<TurnCommand>,
}

#[derive(Debug, Clone)]
struct ActiveTurnAnimation {
    scene_revision: u64,
    pivot_entity: Entity,
    angle_radians: f32,
    axis: Vec3,
    elapsed_secs: f32,
    duration_secs: f32,
}

#[derive(Debug, Clone, Copy)]
struct PointerGestureCandidate {
    start_position: Vec2,
    max_distance: f32,
    sticker_candidate: Option<ScreenStickerCandidate>,
}

#[derive(Debug, Clone, Copy)]
struct TouchGestureCandidate {
    id: u64,
    raw_start_position: Vec2,
    start_position: Vec2,
    max_distance: f32,
    sticker_candidate: Option<ScreenStickerCandidate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum TouchOrbitMode {
    #[default]
    Idle,
    SingleFinger {
        id: u64,
    },
    MultiFinger,
}

#[derive(Debug, Clone, Copy)]
struct ActiveTouch {
    id: u64,
    raw_position: Vec2,
    position: Vec2,
}

#[derive(Debug, Clone, Copy)]
struct CanvasTouchSpace {
    offset: Vec2,
    scale: Vec2,
    rect_size: Vec2,
    canvas_size: Vec2,
    device_pixel_ratio: f32,
    viewport_scale: f32,
}

#[derive(Debug, Clone, Copy)]
struct ReleasedTouch {
    id: u64,
    raw_position: Vec2,
    position: Vec2,
    canceled: bool,
}

#[derive(Debug, Default)]
struct TouchInputFrame {
    active: Vec<ActiveTouch>,
    just_pressed_ids: BTreeSet<u64>,
    released: Vec<ReleasedTouch>,
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Deserialize)]
struct BrowserTouchSnapshot {
    active: Vec<BrowserTouchPoint>,
    started: Vec<BrowserTouchPoint>,
    released: Vec<BrowserTouchRelease>,
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Deserialize)]
struct BrowserTouchPoint {
    id: u64,
    x: f32,
    y: f32,
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Deserialize)]
struct BrowserTouchRelease {
    id: u64,
    x: f32,
    y: f32,
    canceled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ScreenStickerCandidate {
    face: Face,
    row: usize,
    col: usize,
    cubie: UVec3,
    world_center: Vec3,
    center: Vec2,
    radius: f32,
    projected_col_axis: Vec2,
    projected_row_axis: Vec2,
    col_axis: Vec3,
    row_axis: Vec3,
    corners: [Vec2; 4],
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct CubeSurfaceHit {
    face: Face,
    point: Vec3,
}

const CUBE_FACE_SPAN: f32 = 1.9;
const POINTER_TAP_MAX_DRAG_PX: f32 = 8.0;
const FACE_TAP_RADIUS_SCALE: f32 = 0.7;
const VIRTUAL_SURFACE_INSET: f32 = 0.03;
const TOUCH_MOUSE_SUPPRESSION_SECS: f32 = 0.12;

impl Default for OrbitRig {
    fn default() -> Self {
        Self {
            yaw: 0.78,
            pitch: 0.26,
            radius: 7.2,
            auto_spin: false,
            mouse_drag_button: None,
            touch_drag_mode: TouchOrbitMode::Idle,
            touch_mouse_suppression_secs: 0.0,
            snap_target: None,
            touch_start_yaw: 0.0,
            touch_start_pitch: 0.0,
            touch_start_position: None,
            touch_start_center: None,
            previous_pinch_distance: None,
        }
    }
}

impl Default for CanvasTouchSpace {
    fn default() -> Self {
        Self {
            offset: Vec2::ZERO,
            scale: Vec2::ONE,
            rect_size: Vec2::ZERO,
            canvas_size: Vec2::ZERO,
            device_pixel_ratio: 1.0,
            viewport_scale: 1.0,
        }
    }
}

impl TouchOrbitMode {
    fn is_orbiting(self) -> bool {
        !matches!(self, Self::Idle)
    }
}

impl CanvasTouchSpace {
    fn offset_only_position(self, pointer_position: Vec2) -> Vec2 {
        pointer_position - self.offset
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn canvas_relative_position(self, pointer_position: Vec2) -> Vec2 {
        pointer_position * self.scale
    }

    fn scaled_position(self, pointer_position: Vec2) -> Vec2 {
        self.offset_only_position(pointer_position) * self.scale
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(inline_js = r#"
export function rubikCanvasClientMetrics(selector) {
  if (typeof document === 'undefined' || typeof window === 'undefined') {
    return null;
  }

  const canvas = document.querySelector(selector);
  if (!(canvas instanceof HTMLCanvasElement)) {
    return null;
  }

  const rect = canvas.getBoundingClientRect();
  return [
    rect.left,
    rect.top,
    rect.width,
    rect.height,
    canvas.width,
    canvas.height,
    window.devicePixelRatio ?? 1,
    window.visualViewport?.scale ?? 1,
  ];
}

const rubikTouchStores = globalThis.__rubikTouchStores ??= new Map();

function ensureRubikTouchStore(selector) {
  if (typeof document === 'undefined') {
    return null;
  }

  const canvas = document.querySelector(selector);
  if (!(canvas instanceof HTMLCanvasElement)) {
    return null;
  }

  let store = rubikTouchStores.get(selector);
  if (store) {
    return store;
  }

  store = {
    active: new Map(),
    started: [],
    released: [],
  };

  const pointFromEvent = (event) => ({
    id: event.pointerId,
    x: event.offsetX,
    y: event.offsetY,
  });

  const onPointerDown = (event) => {
    if (event.pointerType !== 'touch') {
      return;
    }
    try {
      canvas.setPointerCapture(event.pointerId);
    } catch {}
    const point = pointFromEvent(event);
    store.active.set(event.pointerId, point);
    store.started.push(point);
  };

  const onPointerMove = (event) => {
    if (event.pointerType !== 'touch') {
      return;
    }
    if (!store.active.has(event.pointerId)) {
      return;
    }
    store.active.set(event.pointerId, pointFromEvent(event));
  };

  const onPointerEnd = (event, canceled) => {
    if (event.pointerType !== 'touch') {
      return;
    }
    try {
      canvas.releasePointerCapture(event.pointerId);
    } catch {}
    const point = pointFromEvent(event);
    store.active.delete(event.pointerId);
    store.released.push({ ...point, canceled });
  };

  canvas.addEventListener('pointerdown', onPointerDown, { passive: false });
  canvas.addEventListener('pointermove', onPointerMove, { passive: false });
  canvas.addEventListener('pointerup', (event) => onPointerEnd(event, false), { passive: false });
  canvas.addEventListener('pointercancel', (event) => onPointerEnd(event, true), { passive: false });

  rubikTouchStores.set(selector, store);
  return store;
}

export function rubikConsumeTouchPointerSnapshot(selector) {
  const store = ensureRubikTouchStore(selector);
  if (!store) {
    return null;
  }

  const payload = {
    active: Array.from(store.active.values()),
    started: store.started.splice(0),
    released: store.released.splice(0),
  };
  return JSON.stringify(payload);
}
"#)]
extern "C" {
    fn rubikCanvasClientMetrics(selector: &str) -> JsValue;
    fn rubikConsumeTouchPointerSnapshot(selector: &str) -> JsValue;
}

fn build_touch_candidate(
    camera_context: Option<(&Camera, &GlobalTransform)>,
    order: u8,
    id: u64,
    raw_position: Vec2,
    position: Vec2,
) -> TouchGestureCandidate {
    TouchGestureCandidate {
        id,
        raw_start_position: raw_position,
        start_position: position,
        max_distance: 0.0,
        sticker_candidate: camera_context.and_then(|(camera, camera_transform)| {
            projected_sticker_hit(camera, camera_transform, order, position)
        }),
    }
}

fn should_reset_single_touch_gesture(
    touch_mode: TouchOrbitMode,
    candidate_id: Option<u64>,
    touch_id: u64,
    just_pressed: bool,
) -> bool {
    just_pressed
        || matches!(touch_mode, TouchOrbitMode::MultiFinger)
        || matches!(touch_mode, TouchOrbitMode::SingleFinger { id } if id != touch_id)
        || candidate_id.is_some_and(|candidate_id| candidate_id != touch_id)
}

fn set_touch_orbit_mode(orbit: &mut OrbitRig, mode: TouchOrbitMode) {
    orbit.touch_drag_mode = mode;
    orbit.touch_start_position = None;
    orbit.touch_start_center = None;
    orbit.previous_pinch_distance = None;
    orbit.snap_target = None;
}

fn clear_touch_orbit_state(orbit: &mut OrbitRig) {
    orbit.touch_drag_mode = TouchOrbitMode::Idle;
    orbit.touch_start_position = None;
    orbit.touch_start_center = None;
    orbit.previous_pinch_distance = None;
}

fn should_emulate_two_finger_touch(shift_pressed: bool, active_touch_count: usize) -> bool {
    shift_pressed && active_touch_count == 1
}

fn canvas_touch_space(config: &ShellConfig, window: &Window) -> CanvasTouchSpace {
    #[cfg(target_arch = "wasm32")]
    {
        let value = rubikCanvasClientMetrics(&config.canvas_selector);
        if value.is_null() || value.is_undefined() {
            return CanvasTouchSpace::default();
        }

        let values = Array::from(&value);
        let rect_width = values.get(2).as_f64().unwrap_or_default() as f32;
        let rect_height = values.get(3).as_f64().unwrap_or_default() as f32;
        if rect_width <= f32::EPSILON || rect_height <= f32::EPSILON {
            return CanvasTouchSpace::default();
        }

        return CanvasTouchSpace {
            offset: Vec2::new(
                values.get(0).as_f64().unwrap_or_default() as f32,
                values.get(1).as_f64().unwrap_or_default() as f32,
            ),
            scale: Vec2::new(window.width() / rect_width, window.height() / rect_height),
            rect_size: Vec2::new(rect_width, rect_height),
            canvas_size: Vec2::new(
                values.get(4).as_f64().unwrap_or_default() as f32,
                values.get(5).as_f64().unwrap_or_default() as f32,
            ),
            device_pixel_ratio: values.get(6).as_f64().unwrap_or(1.0) as f32,
            viewport_scale: values.get(7).as_f64().unwrap_or(1.0) as f32,
        };
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (config, window);
        CanvasTouchSpace::default()
    }
}

fn normalize_touch_position(touch_space: CanvasTouchSpace, position: Vec2) -> Vec2 {
    touch_space.offset_only_position(position)
}

fn bevy_touch_frame(touches: &Touches, touch_space: CanvasTouchSpace) -> TouchInputFrame {
    let active = touches
        .iter()
        .map(|touch| ActiveTouch {
            id: touch.id(),
            raw_position: touch.position(),
            position: normalize_touch_position(touch_space, touch.position()),
        })
        .collect::<Vec<_>>();
    let just_pressed_ids = touches
        .iter_just_pressed()
        .map(|touch| touch.id())
        .collect::<BTreeSet<_>>();
    let mut released = touches
        .iter_just_released()
        .map(|touch| ReleasedTouch {
            id: touch.id(),
            raw_position: touch.position(),
            position: normalize_touch_position(touch_space, touch.position()),
            canceled: false,
        })
        .collect::<Vec<_>>();
    released.extend(touches.iter_just_canceled().map(|touch| ReleasedTouch {
        id: touch.id(),
        raw_position: touch.position(),
        position: normalize_touch_position(touch_space, touch.position()),
        canceled: true,
    }));

    TouchInputFrame {
        active,
        just_pressed_ids,
        released,
    }
}

fn dom_touch_frame(config: &ShellConfig, touch_space: CanvasTouchSpace) -> Option<TouchInputFrame> {
    #[cfg(target_arch = "wasm32")]
    {
        let value = rubikConsumeTouchPointerSnapshot(&config.canvas_selector);
        if value.is_null() || value.is_undefined() {
            return None;
        }

        let snapshot = serde_json::from_str::<BrowserTouchSnapshot>(&value.as_string()?).ok()?;
        return Some(TouchInputFrame {
            active: snapshot
                .active
                .into_iter()
                .map(|point| ActiveTouch {
                    id: point.id,
                    raw_position: Vec2::new(point.x, point.y),
                    position: touch_space.canvas_relative_position(Vec2::new(point.x, point.y)),
                })
                .collect(),
            just_pressed_ids: snapshot.started.into_iter().map(|point| point.id).collect(),
            released: snapshot
                .released
                .into_iter()
                .map(|point| ReleasedTouch {
                    id: point.id,
                    raw_position: Vec2::new(point.x, point.y),
                    position: touch_space.canvas_relative_position(Vec2::new(point.x, point.y)),
                    canceled: point.canceled,
                })
                .collect(),
        });
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (config, touch_space);
        None
    }
}

fn format_vec2(value: Vec2) -> String {
    format!("{:.1},{:.1}", value.x, value.y)
}

fn format_optional_vec2(value: Option<Vec2>) -> String {
    value.map(format_vec2).unwrap_or_else(|| "—".to_owned())
}

fn publish_touch_diagnostic(
    label: &str,
    window: Option<&Window>,
    touch_space: CanvasTouchSpace,
    raw_position: Vec2,
    local_position: Vec2,
    cursor_position: Option<Vec2>,
    sticker_hit: Option<bool>,
) {
    let scaled = touch_space.scaled_position(raw_position);
    let sticker_hit = sticker_hit
        .map(|value| if value { "Y" } else { "N" })
        .unwrap_or("?");
    let compact = format!(
        "{label} hit:{sticker_hit} raw {} local {} scaled {} cursor {}",
        format_vec2(raw_position),
        format_vec2(local_position),
        format_vec2(scaled),
        format_optional_vec2(cursor_position),
    );

    let detail = if let Some(window) = window {
        format!(
            "{compact} | win {:.1}x{:.1} sf {:.2} | rect {} {}x{} | canvas {} dpr {:.2} vvp {:.2}",
            window.width(),
            window.height(),
            window.scale_factor(),
            format_vec2(touch_space.offset),
            format_vec2(touch_space.rect_size),
            format_vec2(touch_space.scale),
            format_vec2(touch_space.canvas_size),
            touch_space.device_pixel_ratio,
            touch_space.viewport_scale,
        )
    } else {
        compact.clone()
    };

    info!("touch diagnostic: {detail}");
}

impl RuntimeBridge {
    fn new(order: CubeOrder) -> Self {
        Self {
            engine: CubeEngine::new(order),
            scene_revision: 1,
            last_message: format!("Booted {}x{} runtime.", order.get(), order.get()),
            timer: RuntimeTimer::default(),
            last_transition: None,
            animation_active: false,
        }
    }

    fn snapshot(&self) -> RuntimeSnapshot {
        RuntimeSnapshot {
            state: self.engine.state().clone(),
            scene_revision: self.scene_revision,
            transition: self.last_transition.clone(),
        }
    }

    fn status(&self) -> RuntimeStatus {
        RuntimeStatus {
            order: self.engine.order().get(),
            move_count: self.engine.move_count(),
            redo_depth: self.engine.redo_depth(),
            is_solved: self.engine.is_solved(),
            timing_active: self.timer.is_active(),
            animation_active: self.animation_active,
            elapsed_millis: self.timer.elapsed_millis(),
            scene_revision: self.scene_revision,
            last_message: self.last_message.clone(),
            recent_turns: self
                .engine
                .turn_history()
                .iter()
                .rev()
                .take(10)
                .copied()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .map(format_turn)
                .collect(),
        }
    }

    fn set_message(&mut self, message: impl Into<String>) {
        self.last_message = message.into();
    }

    fn set_animation_active(&mut self, active: bool) {
        self.animation_active = active;
    }

    fn bump_scene(&mut self) {
        self.scene_revision += 1;
        self.timer
            .sync(self.engine.move_count(), self.engine.is_solved());
    }

    fn record_transition(&mut self, from_state: CubeState, animation: Option<TurnCommand>) {
        self.last_transition = Some(RuntimeTransition {
            scene_revision: self.scene_revision,
            from_state,
            animation,
        });
        self.animation_active = false;
    }

    fn set_order(&mut self, order: CubeOrder) {
        let from_state = self.engine.state().clone();
        self.engine = CubeEngine::new(order);
        self.timer.reset();
        self.bump_scene();
        self.record_transition(from_state, None);
        self.set_message(format!("Switched to {}x{}.", order.get(), order.get()));
    }

    fn reset(&mut self) {
        let from_state = self.engine.state().clone();
        self.engine.reset();
        self.timer.reset();
        self.bump_scene();
        self.record_transition(from_state, None);
        self.set_message("Reset cube to solved state.");
    }

    fn apply_turn(&mut self, turn: TurnCommand) -> Result<(), String> {
        let from_state = self.engine.state().clone();
        self.engine
            .apply_turn(turn)
            .map_err(|error| error.to_string())?;
        self.bump_scene();
        self.record_transition(from_state, Some(turn));
        self.set_message(format!("Applied {}.", format_turn(turn)));
        Ok(())
    }

    fn undo(&mut self) -> Result<(), String> {
        let from_state = self.engine.state().clone();
        let turn = self.engine.undo().map_err(|error| error.to_string())?;
        self.bump_scene();
        self.record_transition(from_state, Some(turn.inverse()));
        self.set_message(format!("Undid {}.", format_turn(turn)));
        Ok(())
    }

    fn redo(&mut self) -> Result<(), String> {
        let from_state = self.engine.state().clone();
        let turn = self.engine.redo().map_err(|error| error.to_string())?;
        self.bump_scene();
        self.record_transition(from_state, Some(turn));
        self.set_message(format!("Redid {}.", format_turn(turn)));
        Ok(())
    }

    fn scramble(&mut self, length: usize, seed: u64) -> Result<(), String> {
        let from_state = self.engine.state().clone();
        let scramble = self
            .engine
            .scramble_with_seed(length.max(1), seed)
            .map_err(|error| error.to_string())?;
        self.bump_scene();
        self.record_transition(from_state, None);
        self.set_message(format!(
            "Applied scramble ({} turns, seed {}).",
            scramble.len(),
            seed
        ));
        Ok(())
    }

    fn import_state(&mut self, json: &str) -> Result<(), String> {
        let from_state = self.engine.state().clone();
        let state = CubeState::from_json(json).map_err(|error| error.to_string())?;
        self.engine = CubeEngine::from_state(state).map_err(|error| error.to_string())?;
        self.timer.reset();
        self.bump_scene();
        self.record_transition(from_state, None);
        self.set_message(format!(
            "Imported {}x{} sticker state.",
            self.engine.order().get(),
            self.engine.order().get()
        ));
        Ok(())
    }
}

impl RuntimeTimer {
    fn reset(&mut self) {
        self.elapsed_before_millis = 0;
        self.started_at_millis = None;
    }

    fn sync(&mut self, move_count: usize, solved: bool) {
        if move_count == 0 {
            self.reset();
            return;
        }

        if !solved && self.started_at_millis.is_none() {
            self.started_at_millis = Some(now_millis());
        }

        if solved {
            self.stop();
        }
    }

    fn stop(&mut self) {
        if let Some(started_at_millis) = self.started_at_millis.take() {
            self.elapsed_before_millis = self
                .elapsed_before_millis
                .saturating_add(now_millis().saturating_sub(started_at_millis));
        }
    }

    fn elapsed_millis(&self) -> u64 {
        self.started_at_millis
            .map(|started_at_millis| {
                self.elapsed_before_millis
                    .saturating_add(now_millis().saturating_sub(started_at_millis))
            })
            .unwrap_or(self.elapsed_before_millis)
    }

    fn is_active(&self) -> bool {
        self.started_at_millis.is_some()
    }
}

fn now_millis() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        return Date::now().max(0.0) as u64;
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};

        return SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64;
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn start_app(canvas_id: &str, base_path: &str) {
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();

    let canvas_selector = normalize_canvas_selector(canvas_id);

    with_runtime_mut(|runtime| {
        *runtime = RuntimeBridge::new(CubeOrder::standard());
        runtime.set_message(
            "Bevy runtime mounted. Use the shell or keyboard shortcuts to manipulate the cube.",
        );
    });

    App::new()
        .insert_resource(ClearColor(Color::srgb_u8(5, 8, 15)))
        .insert_resource(ShellConfig {
            base_path: normalize_base_path(base_path),
            canvas_selector: canvas_selector.clone(),
        })
        .insert_resource(OrbitRig::default())
        .insert_resource(DirectTurnInputState::default())
        .insert_resource(VisualSyncState::default())
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "rubikrs // bevy runtime".to_owned(),
                canvas: Some(canvas_selector),
                fit_canvas_to_parent: true,
                prevent_default_event_handling: false,
                ..default()
            }),
            ..default()
        }))
        .add_systems(Startup, setup_scene)
        .add_systems(
            Update,
            (
                orbit_camera_input,
                keyboard_turn_shortcuts,
                apply_camera_transform,
                canvas_face_tap_input,
                animate_turn_visuals,
                sync_cube_visuals,
            )
                .chain(),
        )
        .run();
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn runtime_status_json() -> String {
    with_runtime(|runtime| {
        serde_json::to_string(&runtime.status()).unwrap_or_else(|_| "{}".to_owned())
    })
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn export_cube_state() -> String {
    with_runtime(|runtime| {
        runtime
            .engine
            .state()
            .to_json()
            .unwrap_or_else(|_| "{}".to_owned())
    })
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn export_turn_history_json() -> String {
    with_runtime(|runtime| {
        serde_json::to_string(runtime.engine.turn_history()).unwrap_or_else(|_| "[]".to_owned())
    })
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn import_cube_state(json: &str) -> bool {
    update_runtime(|runtime| runtime.import_state(json))
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn set_cube_order(order: u8) -> bool {
    let Ok(order) = CubeOrder::new(order) else {
        with_runtime_mut(|runtime| {
            runtime.set_message(format!(
                "Order {} is outside the supported 2..=17 range.",
                order
            ));
        });
        return false;
    };

    with_runtime_mut(|runtime| {
        runtime.set_order(order);
    });

    true
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn reset_cube() -> bool {
    with_runtime_mut(|runtime| runtime.reset());
    true
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn undo_turn() -> bool {
    update_runtime(|runtime| runtime.undo())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn redo_turn() -> bool {
    update_runtime(|runtime| runtime.redo())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn scramble_cube(length: u32, seed: u64) -> bool {
    update_runtime(|runtime| runtime.scramble(length as usize, seed))
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn apply_turn(face_code: u8, rotation_code: u8, start_layer: u8, width: u8) -> bool {
    let Some(face) = decode_face(face_code) else {
        with_runtime_mut(|runtime| {
            runtime.set_message(format!("Unknown face code {}.", face_code));
        });
        return false;
    };

    let Some(rotation) = decode_rotation(rotation_code) else {
        with_runtime_mut(|runtime| {
            runtime.set_message(format!("Unknown rotation code {}.", rotation_code));
        });
        return false;
    };

    update_runtime(|runtime| {
        runtime.apply_turn(TurnCommand {
            face,
            start_layer,
            width,
            rotation,
        })
    })
}

fn setup_scene(mut commands: Commands<'_, '_>, config: Res<'_, ShellConfig>) {
    info!(
        "booting Bevy runtime on {} with canvas {}",
        config.base_path, config.canvas_selector
    );

    commands.spawn((
        Camera3d::default(),
        Tonemapping::None,
        Transform::from_xyz(-3.85, 3.15, 6.45).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    commands.spawn((
        PointLight {
            intensity: 2_200_000.0,
            range: 42.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(5.5, 8.5, 5.5),
    ));

    commands.spawn((
        DirectionalLight {
            illuminance: 16_000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.78, 0.92, 0.0)),
    ));
}

fn sync_cube_visuals(
    mut commands: Commands<'_, '_>,
    mut meshes: ResMut<'_, Assets<Mesh>>,
    mut materials: ResMut<'_, Assets<StandardMaterial>>,
    mut sync_state: ResMut<'_, VisualSyncState>,
    existing_visual_roots: Query<'_, '_, Entity, With<CubeVisualRoot>>,
) {
    let snapshot = with_runtime(|runtime| runtime.snapshot());

    if let Some(active) = &sync_state.active_animation {
        if snapshot.scene_revision == active.scene_revision {
            with_runtime_mut(|runtime| runtime.set_animation_active(true));
            return;
        }

        clear_cube_visuals(&mut commands, &existing_visual_roots);
        sync_state.active_animation = None;
        sync_state.rendered_revision = 0;
        sync_state.completed_animation_revision = None;
        with_runtime_mut(|runtime| runtime.set_animation_active(false));
    }

    if snapshot.scene_revision == sync_state.rendered_revision {
        with_runtime_mut(|runtime| runtime.set_animation_active(false));
        return;
    }

    clear_cube_visuals(&mut commands, &existing_visual_roots);

    if let Some(transition) = snapshot
        .transition
        .filter(|transition| transition.scene_revision == snapshot.scene_revision)
        .filter(|transition| transition.animation.is_some())
        .filter(|_| sync_state.completed_animation_revision != Some(snapshot.scene_revision))
    {
        let (root, stickers) = spawn_cube_visuals(
            &mut commands,
            &mut meshes,
            &mut materials,
            &transition.from_state,
        );
        let turn = transition.animation.expect("filtered animation transition");
        let pivot = commands
            .spawn((
                CubeVisual,
                TurnAnimationPivot,
                Name::new("turn-animation-pivot"),
                Transform::default(),
                GlobalTransform::default(),
                Visibility::Visible,
                InheritedVisibility::default(),
                ViewVisibility::default(),
                ChildOf(root),
            ))
            .id();

        for (entity, cubie) in stickers {
            if cubie_matches_turn(snapshot.state.order.get(), turn, cubie) {
                commands.entity(entity).insert(ChildOf(pivot));
            }
        }

        sync_state.active_animation = Some(ActiveTurnAnimation {
            scene_revision: snapshot.scene_revision,
            pivot_entity: pivot,
            angle_radians: turn_rotation_angle(turn),
            axis: turn_rotation_axis(turn.face),
            elapsed_secs: 0.0,
            duration_secs: turn_animation_duration_secs(turn),
        });
        sync_state.completed_animation_revision = None;
        with_runtime_mut(|runtime| runtime.set_animation_active(true));
        return;
    }

    spawn_cube_visuals(&mut commands, &mut meshes, &mut materials, &snapshot.state);
    sync_state.rendered_revision = snapshot.scene_revision;
    sync_state.completed_animation_revision = None;
    with_runtime_mut(|runtime| runtime.set_animation_active(false));
}

fn animate_turn_visuals(
    time: Res<'_, Time>,
    mut commands: Commands<'_, '_>,
    mut sync_state: ResMut<'_, VisualSyncState>,
    existing_visual_roots: Query<'_, '_, Entity, With<CubeVisualRoot>>,
    mut pivots: Query<'_, '_, &mut Transform, With<TurnAnimationPivot>>,
) {
    let Some(animation) = sync_state.active_animation.as_mut() else {
        return;
    };

    let Ok(mut pivot_transform) = pivots.get_mut(animation.pivot_entity) else {
        sync_state.active_animation = None;
        sync_state.rendered_revision = 0;
        sync_state.completed_animation_revision = None;
        with_runtime_mut(|runtime| runtime.set_animation_active(false));
        return;
    };

    animation.elapsed_secs =
        (animation.elapsed_secs + time.delta_secs()).min(animation.duration_secs);
    let progress = if animation.duration_secs <= f32::EPSILON {
        1.0
    } else {
        animation.elapsed_secs / animation.duration_secs
    };
    let eased = ease_out_cubic(progress);
    pivot_transform.rotation =
        Quat::from_axis_angle(animation.axis, animation.angle_radians * eased);

    if progress >= 1.0 {
        let scene_revision = animation.scene_revision;
        clear_cube_visuals(&mut commands, &existing_visual_roots);
        sync_state.rendered_revision = 0;
        sync_state.active_animation = None;
        sync_state.completed_animation_revision = Some(scene_revision);
        with_runtime_mut(|runtime| runtime.set_animation_active(false));
    }
}

fn clear_cube_visuals(
    commands: &mut Commands<'_, '_>,
    existing_visual_roots: &Query<'_, '_, Entity, With<CubeVisualRoot>>,
) {
    for entity in existing_visual_roots.iter() {
        commands.entity(entity).despawn();
    }
}

fn spawn_cube_visuals(
    commands: &mut Commands<'_, '_>,
    meshes: &mut ResMut<'_, Assets<Mesh>>,
    materials: &mut ResMut<'_, Assets<StandardMaterial>>,
    state: &CubeState,
) -> (Entity, Vec<(Entity, UVec3)>) {
    let order = usize::from(state.order.get());
    let face_span = CUBE_FACE_SPAN;
    let step = face_span / order as f32;
    let sticker_size = step * 0.84;
    let sticker_depth = 0.06_f32;
    let cubie_body_size = step * 0.92;
    let face_offset = cube_face_offset(state.order.get());

    let root = commands
        .spawn((
            CubeVisual,
            CubeVisualRoot,
            Name::new(format!(
                "cube-visual-{}x{}",
                state.order.get(),
                state.order.get()
            )),
            Transform::default(),
            GlobalTransform::default(),
            Visibility::Visible,
            InheritedVisibility::default(),
            ViewVisibility::default(),
        ))
        .id();

    let shell_material = materials.add(StandardMaterial {
        base_color: Color::srgb_u8(17, 21, 29),
        metallic: 0.18,
        perceptual_roughness: 0.58,
        reflectance: 0.3,
        ..default()
    });

    let cubie_body_mesh = meshes.add(Cuboid::new(
        cubie_body_size,
        cubie_body_size,
        cubie_body_size,
    ));
    let sticker_mesh = meshes.add(Cuboid::new(sticker_size, sticker_size, sticker_depth));
    let mut animated_entities =
        Vec::with_capacity((Face::ALL.len() * order * order) + (order * order * 6));
    let mut surface_cubies = BTreeSet::new();

    for face in Face::ALL {
        for row in 0..order {
            for col in 0..order {
                let cubie = sticker_cubie_coord(face, row, col, order);
                surface_cubies.insert((cubie.x, cubie.y, cubie.z));
            }
        }
    }

    commands.entity(root).with_children(|parent| {
        for (x, y, z) in &surface_cubies {
            let cubie = UVec3::new(*x, *y, *z);
            let entity = parent
                .spawn((
                    CubeVisual,
                    Mesh3d(cubie_body_mesh.clone()),
                    MeshMaterial3d(shell_material.clone()),
                    Transform::from_translation(cubie_body_translation(cubie, order, face_span)),
                    Name::new(format!("cubie-body-{x}-{y}-{z}")),
                ))
                .id();
            animated_entities.push((entity, cubie));
        }

        for face_index in 0..Face::ALL.len() {
            let face = Face::ALL[face_index];
            for row in 0..order {
                for col in 0..order {
                    let color = state.stickers[(face_index * order * order) + (row * order) + col];
                    let cubie = sticker_cubie_coord(face, row, col, order);
                    let (translation, rotation, mesh) = sticker_transform(
                        face,
                        row,
                        col,
                        order,
                        face_span,
                        face_offset,
                        &sticker_mesh,
                    );

                    let material = materials.add(StandardMaterial {
                        base_color: color_for_sticker(color),
                        metallic: 0.06,
                        perceptual_roughness: 0.21,
                        ..default()
                    });

                    let entity = parent
                        .spawn((
                            CubeVisual,
                            Mesh3d(mesh.clone()),
                            MeshMaterial3d(material),
                            Transform::from_translation(translation).with_rotation(rotation),
                            Name::new(format!("sticker-{face_index}-{row}-{col}")),
                        ))
                        .id();
                    animated_entities.push((entity, cubie));
                }
            }
        }
    });

    (root, animated_entities)
}

fn orbit_camera_input(
    time: Res<'_, Time>,
    buttons: Res<'_, ButtonInput<MouseButton>>,
    keys: Res<'_, ButtonInput<KeyCode>>,
    touches: Res<'_, Touches>,
    config: Res<'_, ShellConfig>,
    windows: Query<'_, '_, &Window>,
    cameras: Query<'_, '_, (&Camera, &GlobalTransform), With<Camera3d>>,
    mut mouse_motion: MessageReader<'_, '_, MouseMotion>,
    mut mouse_wheel: MessageReader<'_, '_, MouseWheel>,
    mut orbit: ResMut<'_, OrbitRig>,
    mut direct_turn_input: ResMut<'_, DirectTurnInputState>,
) {
    let camera_context = cameras.single().ok();
    let order = with_runtime(|runtime| runtime.engine.order().get());
    let shift_pressed = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    orbit.touch_mouse_suppression_secs =
        (orbit.touch_mouse_suppression_secs - time.delta_secs()).max(0.0);

    if keys.just_pressed(KeyCode::Space) {
        orbit.auto_spin = !orbit.auto_spin;
        orbit.snap_target = None;
    }

    if orbit.auto_spin {
        orbit.yaw += time.delta_secs() * 0.18;
    }

    let primary_window = windows.iter().next();
    let cursor_position = primary_window.and_then(|window| window.cursor_position());
    let touch_space = primary_window
        .map(|window| canvas_touch_space(&config, window))
        .unwrap_or_default();
    let touch_frame = dom_touch_frame(&config, touch_space)
        .unwrap_or_else(|| bevy_touch_frame(&touches, touch_space));
    let active_touches = touch_frame.active.as_slice();
    let mouse_delta = mouse_motion
        .read()
        .fold(Vec2::ZERO, |total, event| total + event.delta);
    if !active_touches.is_empty() {
        orbit.touch_mouse_suppression_secs = TOUCH_MOUSE_SUPPRESSION_SECS;
    }
    let suppress_mouse = orbit.touch_mouse_suppression_secs > 0.0;

    if suppress_mouse {
        direct_turn_input.mouse_candidate = None;
        orbit.mouse_drag_button = None;
    } else {
        if buttons.just_pressed(MouseButton::Left) {
            direct_turn_input.mouse_candidate =
                cursor_position.map(|position| PointerGestureCandidate {
                    start_position: position,
                    max_distance: 0.0,
                    sticker_candidate: camera_context.and_then(|(camera, camera_transform)| {
                        projected_sticker_hit(camera, camera_transform, order, position)
                    }),
                });
            orbit.mouse_drag_button = None;
            orbit.snap_target = None;
            orbit.auto_spin = false;
        }

        if buttons.just_pressed(MouseButton::Right) {
            direct_turn_input.mouse_candidate = None;
            orbit.mouse_drag_button = Some(MouseButton::Right);
            orbit.snap_target = None;
            orbit.auto_spin = false;
        }

        if buttons.pressed(MouseButton::Left) {
            if let Some(candidate) = direct_turn_input.mouse_candidate.as_mut() {
                candidate.max_distance = candidate.max_distance.max(mouse_delta.length());
                if let Some(position) = cursor_position {
                    candidate.max_distance = candidate
                        .max_distance
                        .max(position.distance(candidate.start_position));
                }

                if should_begin_mouse_orbit(
                    MouseButton::Left,
                    candidate.max_distance,
                    candidate.sticker_candidate.is_some(),
                ) {
                    direct_turn_input.mouse_candidate = None;
                    orbit.mouse_drag_button = Some(MouseButton::Left);
                    orbit.snap_target = None;
                }
            }
        }

        if orbit
            .mouse_drag_button
            .is_some_and(|button| buttons.pressed(button))
        {
            orbit.snap_target = None;
            orbit.auto_spin = false;
            orbit.yaw += mouse_delta.x * 0.008;
            orbit.pitch = (orbit.pitch + mouse_delta.y * 0.006).clamp(-1.15, 1.15);
        }

        if buttons.just_released(MouseButton::Left) {
            if orbit.mouse_drag_button == Some(MouseButton::Left) {
                orbit.mouse_drag_button = None;
                orbit.snap_target = nearest_orbit_snap(orbit.yaw, orbit.pitch);
            } else if let Some(candidate) = direct_turn_input.mouse_candidate.take() {
                let position = cursor_position.unwrap_or(candidate.start_position);
                if let (Some(sticker_candidate), Some((camera, camera_transform))) =
                    (candidate.sticker_candidate, camera_context)
                {
                    if candidate.max_distance > POINTER_TAP_MAX_DRAG_PX {
                        if let Some(turn) = slice_turn_from_sticker_drag(
                            camera,
                            camera_transform,
                            order,
                            sticker_candidate,
                            candidate.start_position,
                            position,
                        ) {
                            direct_turn_input.queued_turn = Some(turn);
                        }
                    }
                }
            }
        }

        if buttons.just_released(MouseButton::Right)
            && orbit.mouse_drag_button == Some(MouseButton::Right)
        {
            orbit.mouse_drag_button = None;
            orbit.snap_target = nearest_orbit_snap(orbit.yaw, orbit.pitch);
        }
    }

    for event in mouse_wheel.read() {
        let zoom_delta = match event.unit {
            MouseScrollUnit::Line => event.y * 0.065,
            MouseScrollUnit::Pixel => event.y * 0.0022,
        };
        orbit.radius = (orbit.radius * (-zoom_delta).exp()).clamp(2.9, 9.4);
    }

    if let Some(candidate) = direct_turn_input.touch_candidate {
        if let Some(released_touch) = touch_frame
            .released
            .iter()
            .find(|touch| touch.id == candidate.id && !touch.canceled)
        {
            if let (Some(sticker_candidate), Some((camera, camera_transform))) =
                (candidate.sticker_candidate, camera_context)
            {
                if candidate.max_distance > POINTER_TAP_MAX_DRAG_PX {
                    if let Some(turn) = slice_turn_from_sticker_drag(
                        camera,
                        camera_transform,
                        order,
                        sticker_candidate,
                        candidate.start_position,
                        released_touch.position,
                    ) {
                        direct_turn_input.queued_turn = Some(turn);
                    }
                }
            }
            publish_touch_diagnostic(
                "release",
                primary_window,
                touch_space,
                released_touch.raw_position,
                released_touch.position,
                cursor_position,
                Some(candidate.sticker_candidate.is_some()),
            );
            direct_turn_input.touch_candidate = None;
        } else if touch_frame
            .released
            .iter()
            .any(|touch| touch.id == candidate.id && touch.canceled)
        {
            publish_touch_diagnostic(
                "cancel",
                primary_window,
                touch_space,
                candidate.raw_start_position,
                candidate.start_position,
                cursor_position,
                Some(candidate.sticker_candidate.is_some()),
            );
            direct_turn_input.touch_candidate = None;
        }
    }

    match active_touches {
        [] => {
            if orbit.touch_drag_mode.is_orbiting() {
                orbit.snap_target = nearest_orbit_snap(orbit.yaw, orbit.pitch);
            }
            clear_touch_orbit_state(&mut orbit);
        }
        [touch] => {
            orbit.auto_spin = false;
            orbit.snap_target = None;
            let emulate_two_finger = should_emulate_two_finger_touch(shift_pressed, 1);

            if emulate_two_finger {
                direct_turn_input.touch_candidate = None;
                if !matches!(
                    orbit.touch_drag_mode,
                    TouchOrbitMode::SingleFinger { id } if id == touch.id
                ) {
                    set_touch_orbit_mode(&mut orbit, TouchOrbitMode::SingleFinger { id: touch.id });
                    orbit.touch_start_yaw = orbit.yaw;
                    orbit.touch_start_pitch = orbit.pitch;
                    orbit.touch_start_position = Some(touch.position);
                    publish_touch_diagnostic(
                        "emulate-2f",
                        primary_window,
                        touch_space,
                        touch.raw_position,
                        touch.position,
                        cursor_position,
                        None,
                    );
                }
            } else {
                let touch_just_pressed = touch_frame.just_pressed_ids.contains(&touch.id);
                if should_reset_single_touch_gesture(
                    orbit.touch_drag_mode,
                    direct_turn_input
                        .touch_candidate
                        .map(|candidate| candidate.id),
                    touch.id,
                    touch_just_pressed,
                ) {
                    clear_touch_orbit_state(&mut orbit);
                    let candidate = build_touch_candidate(
                        camera_context,
                        order,
                        touch.id,
                        touch.raw_position,
                        touch.position,
                    );
                    publish_touch_diagnostic(
                        "start",
                        primary_window,
                        touch_space,
                        touch.raw_position,
                        touch.position,
                        cursor_position,
                        Some(candidate.sticker_candidate.is_some()),
                    );
                    direct_turn_input.touch_candidate = Some(candidate);
                }

                let mut should_begin_touch_orbit = false;

                if let Some(candidate) = direct_turn_input
                    .touch_candidate
                    .as_mut()
                    .filter(|candidate| candidate.id == touch.id)
                {
                    candidate.max_distance = candidate
                        .max_distance
                        .max(touch.position.distance(candidate.start_position));

                    should_begin_touch_orbit = should_begin_mouse_orbit(
                        MouseButton::Left,
                        candidate.max_distance,
                        candidate.sticker_candidate.is_some(),
                    );
                }

                if should_begin_touch_orbit {
                    direct_turn_input.touch_candidate = None;
                    set_touch_orbit_mode(&mut orbit, TouchOrbitMode::SingleFinger { id: touch.id });
                    orbit.touch_start_yaw = orbit.yaw;
                    orbit.touch_start_pitch = orbit.pitch;
                    orbit.touch_start_position = Some(touch.position);
                    publish_touch_diagnostic(
                        "orbit",
                        primary_window,
                        touch_space,
                        touch.raw_position,
                        touch.position,
                        cursor_position,
                        None,
                    );
                }
            }

            if matches!(
                orbit.touch_drag_mode,
                TouchOrbitMode::SingleFinger { id } if id == touch.id
            ) {
                if let Some(start_pos) = orbit.touch_start_position {
                    let displacement = touch.position - start_pos;
                    if displacement.length_squared() > 0.0 {
                        orbit.yaw = orbit.touch_start_yaw + displacement.x * 0.008;
                        orbit.pitch = (orbit.touch_start_pitch + displacement.y * 0.006)
                            .clamp(-1.15, 1.15);
                    }
                }
            }
        }
        [first, second, ..] => {
            orbit.auto_spin = false;
            orbit.snap_target = None;
            direct_turn_input.touch_candidate = None;
            if orbit.touch_drag_mode != TouchOrbitMode::MultiFinger {
                set_touch_orbit_mode(&mut orbit, TouchOrbitMode::MultiFinger);
                orbit.touch_start_yaw = orbit.yaw;
                orbit.touch_start_pitch = orbit.pitch;
                orbit.touch_start_center = Some((first.position + second.position) * 0.5);
            }

            let center = (first.position + second.position) * 0.5;
            if let Some(start_center) = orbit.touch_start_center {
                let displacement = center - start_center;
                if displacement.length_squared() > 0.0 {
                    orbit.yaw = orbit.touch_start_yaw + displacement.x * 0.006;
                    orbit.pitch = (orbit.touch_start_pitch + displacement.y * 0.0045)
                        .clamp(-1.15, 1.15);
                }
            }

            let pinch_distance = first.position.distance(second.position);
            if let Some(previous_pinch_distance) = orbit.previous_pinch_distance {
                let zoom_delta = (pinch_distance - previous_pinch_distance) * 0.0022;
                orbit.radius = (orbit.radius * (-zoom_delta).exp()).clamp(2.9, 9.4);
            }

            orbit.previous_pinch_distance = Some(pinch_distance);
        }
    }

    if !orbit.auto_spin && orbit.mouse_drag_button.is_none() && !orbit.touch_drag_mode.is_orbiting()
    {
        if let Some(target) = orbit.snap_target {
            let yaw_delta = shortest_angle_delta(orbit.yaw, target.x);
            let pitch_delta = target.y - orbit.pitch;
            let step = (time.delta_secs() * 14.0).clamp(0.0, 1.0);
            orbit.yaw += yaw_delta * step;
            orbit.pitch += pitch_delta * step;

            if yaw_delta.abs() < 0.01 && pitch_delta.abs() < 0.01 {
                orbit.yaw = normalize_angle(target.x);
                orbit.pitch = target.y;
                orbit.snap_target = None;
            }
        }
    }
}

fn should_begin_mouse_orbit(
    button: MouseButton,
    max_distance: f32,
    has_sticker_candidate: bool,
) -> bool {
    match button {
        MouseButton::Right => true,
        MouseButton::Left => max_distance > POINTER_TAP_MAX_DRAG_PX && !has_sticker_candidate,
        _ => false,
    }
}

fn canvas_face_tap_input(
    mut direct_turn_input: ResMut<'_, DirectTurnInputState>,
    sync_state: Res<'_, VisualSyncState>,
) {
    if sync_state.active_animation.is_some() {
        return;
    }

    if let Some(turn) = direct_turn_input.queued_turn.take() {
        let _ = update_runtime(|runtime| runtime.apply_turn(turn));
    };
}

fn keyboard_turn_shortcuts(keys: Res<'_, ButtonInput<KeyCode>>) {
    let shift_pressed = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let half_turn = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let wide_turn = keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]);
    let order = with_runtime(|runtime| runtime.engine.order().get());
    let rotation = if half_turn {
        RotationAmount::HalfTurn
    } else if shift_pressed {
        RotationAmount::CounterClockwise
    } else {
        RotationAmount::Clockwise
    };

    for (key, face) in [
        (KeyCode::KeyU, Face::Up),
        (KeyCode::KeyR, Face::Right),
        (KeyCode::KeyF, Face::Front),
        (KeyCode::KeyD, Face::Down),
        (KeyCode::KeyL, Face::Left),
        (KeyCode::KeyB, Face::Back),
    ] {
        if keys.just_pressed(key) {
            if let Some(turn) = keyboard_shortcut_turn(
                face,
                rotation,
                order,
                wide_turn,
                keyboard_selected_layer(&keys),
            ) {
                let _ = update_runtime(|runtime| runtime.apply_turn(turn));
            }
        }
    }

    if keys.just_pressed(KeyCode::Backspace) {
        let _ = update_runtime(|runtime| runtime.undo());
    }

    if keys.just_pressed(KeyCode::Enter) {
        let _ = update_runtime(|runtime| runtime.redo());
    }

    if keys.just_pressed(KeyCode::KeyS) {
        let seed = with_runtime(|runtime| runtime.scene_revision.saturating_mul(977));
        let _ = update_runtime(|runtime| runtime.scramble(20, seed));
    }

    if keys.just_pressed(KeyCode::KeyX) {
        with_runtime_mut(|runtime| runtime.reset());
    }
}

fn apply_camera_transform(
    orbit: Res<'_, OrbitRig>,
    mut cameras: Query<'_, '_, &mut Transform, With<Camera3d>>,
) {
    let Ok(mut transform) = cameras.single_mut() else {
        return;
    };

    let horizontal = orbit.radius * orbit.pitch.cos();
    let position = Vec3::new(
        horizontal * orbit.yaw.cos(),
        orbit.radius * orbit.pitch.sin(),
        horizontal * orbit.yaw.sin(),
    );

    *transform = Transform::from_translation(position).looking_at(Vec3::ZERO, Vec3::Y);
}

fn projected_sticker_hit(
    camera: &Camera,
    camera_transform: &GlobalTransform,
    order: u8,
    pointer_position: Vec2,
) -> Option<ScreenStickerCandidate> {
    let surface_hit = cube_surface_hit(camera, camera_transform, order, pointer_position)?;
    surface_hit_candidate(camera, camera_transform, order, surface_hit)
}

fn slice_turn_from_sticker_drag(
    camera: &Camera,
    camera_transform: &GlobalTransform,
    order: u8,
    candidate: ScreenStickerCandidate,
    start_position: Vec2,
    end_position: Vec2,
) -> Option<TurnCommand> {
    let drag = end_position - start_position;
    if drag.length() <= POINTER_TAP_MAX_DRAG_PX {
        return None;
    }

    let turn_face = slice_face_from_sticker_drag(candidate, drag)?;
    let start_layer = slice_start_layer(turn_face, candidate.cubie, order);

    let clockwise = TurnCommand {
        face: turn_face,
        start_layer,
        width: 1,
        rotation: RotationAmount::Clockwise,
    };
    let counter_clockwise = TurnCommand {
        rotation: RotationAmount::CounterClockwise,
        ..clockwise
    };

    let drag_direction = drag.normalize();
    let clockwise_score = drag_direction.dot(projected_turn_motion(
        camera,
        camera_transform,
        candidate.world_center,
        clockwise,
    )?);
    let counter_clockwise_score = drag_direction.dot(projected_turn_motion(
        camera,
        camera_transform,
        candidate.world_center,
        counter_clockwise,
    )?);
    if clockwise_score.max(counter_clockwise_score) <= 0.2 {
        return None;
    }

    Some(if clockwise_score >= counter_clockwise_score {
        clockwise
    } else {
        counter_clockwise
    })
}

fn slice_face_from_sticker_drag(candidate: ScreenStickerCandidate, drag: Vec2) -> Option<Face> {
    let col_alignment = drag.dot(candidate.projected_col_axis);
    let row_alignment = drag.dot(candidate.projected_row_axis);
    let selected_axis = if col_alignment.abs() >= row_alignment.abs() {
        -candidate.row_axis
    } else {
        candidate.col_axis
    };
    axis_face(selected_axis)
}

fn cube_surface_hit(
    camera: &Camera,
    camera_transform: &GlobalTransform,
    order: u8,
    pointer_position: Vec2,
) -> Option<CubeSurfaceHit> {
    let ray = camera
        .viewport_to_world(camera_transform, pointer_position)
        .ok()?;
    cube_surface_hit_from_ray(
        ray.origin,
        ray.direction.as_vec3(),
        virtual_cube_half_extent(order),
    )
}

fn cube_surface_hit_from_ray(
    origin: Vec3,
    direction: Vec3,
    half_extent: f32,
) -> Option<CubeSurfaceHit> {
    let axes = [
        (origin.x, direction.x, Vec3::X),
        (origin.y, direction.y, Vec3::Y),
        (origin.z, direction.z, Vec3::Z),
    ];
    let mut near = f32::NEG_INFINITY;
    let mut far = f32::INFINITY;
    let mut hit_normal = None;

    for (axis_origin, axis_direction, axis_vector) in axes {
        if axis_direction.abs() <= f32::EPSILON {
            if axis_origin.abs() > half_extent {
                return None;
            }
            continue;
        }

        let inverse_direction = 1.0 / axis_direction;
        let mut enter = (-half_extent - axis_origin) * inverse_direction;
        let mut exit = (half_extent - axis_origin) * inverse_direction;
        let mut enter_normal = -axis_vector;
        if enter > exit {
            std::mem::swap(&mut enter, &mut exit);
            enter_normal = axis_vector;
        }

        if enter > near {
            near = enter;
            hit_normal = Some(enter_normal);
        }
        far = far.min(exit);
        if near > far {
            return None;
        }
    }

    if far < 0.0 {
        return None;
    }

    let distance = if near >= 0.0 { near } else { far };
    let point = origin + (direction * distance);
    let face = axis_face(hit_normal?)?;
    Some(CubeSurfaceHit { face, point })
}

fn surface_hit_candidate(
    camera: &Camera,
    camera_transform: &GlobalTransform,
    order: u8,
    surface_hit: CubeSurfaceHit,
) -> Option<ScreenStickerCandidate> {
    let order_usize = usize::from(order);
    let half_extent = virtual_cube_half_extent(order);
    let surface_span = half_extent * 2.0;
    let row = surface_axis_index(
        surface_hit.face,
        surface_hit.point,
        order_usize,
        surface_span,
        true,
    );
    let col = surface_axis_index(
        surface_hit.face,
        surface_hit.point,
        order_usize,
        surface_span,
        false,
    );
    let cubie = sticker_cubie_coord(surface_hit.face, row, col, order_usize);
    let world_center = virtual_surface_center(
        surface_hit.face,
        row,
        col,
        order_usize,
        surface_span,
        half_extent,
    );
    let projected_center = camera
        .world_to_viewport(camera_transform, world_center)
        .ok()?;
    let half_cell = surface_span / order.max(1) as f32 * 0.5;
    let col_axis = sticker_col_direction(surface_hit.face);
    let row_axis = sticker_row_direction(surface_hit.face);
    let projected_col_axis = camera
        .world_to_viewport(camera_transform, world_center + col_axis * half_cell)
        .ok()?
        - projected_center;
    let projected_row_axis = camera
        .world_to_viewport(camera_transform, world_center + row_axis * half_cell)
        .ok()?
        - projected_center;
    if projected_col_axis.length_squared() <= f32::EPSILON
        || projected_row_axis.length_squared() <= f32::EPSILON
    {
        return None;
    }

    let projected_corners = [
        camera
            .world_to_viewport(
                camera_transform,
                world_center - (col_axis * half_cell) - (row_axis * half_cell),
            )
            .ok(),
        camera
            .world_to_viewport(
                camera_transform,
                world_center + (col_axis * half_cell) - (row_axis * half_cell),
            )
            .ok(),
        camera
            .world_to_viewport(
                camera_transform,
                world_center + (col_axis * half_cell) + (row_axis * half_cell),
            )
            .ok(),
        camera
            .world_to_viewport(
                camera_transform,
                world_center - (col_axis * half_cell) + (row_axis * half_cell),
            )
            .ok(),
    ];
    if projected_corners.iter().any(Option::is_none) {
        return None;
    }

    let projected_corners =
        projected_corners.map(|corner| corner.expect("checked projected surface corner"));
    let radius = projected_center.distance(projected_corners[0]) * FACE_TAP_RADIUS_SCALE;
    if radius <= f32::EPSILON {
        return None;
    }

    Some(ScreenStickerCandidate {
        face: surface_hit.face,
        row,
        col,
        cubie,
        world_center,
        center: projected_center,
        radius,
        projected_col_axis: projected_col_axis.normalize(),
        projected_row_axis: projected_row_axis.normalize(),
        col_axis,
        row_axis,
        corners: projected_corners,
    })
}

fn virtual_cube_half_extent(order: u8) -> f32 {
    cube_face_offset(order) - VIRTUAL_SURFACE_INSET
}

fn surface_axis_index(
    face: Face,
    point: Vec3,
    order: usize,
    surface_span: f32,
    is_row: bool,
) -> usize {
    let axis = if is_row {
        sticker_row_direction(face)
    } else {
        sticker_col_direction(face)
    };
    let step = surface_span / order.max(1) as f32;
    let local = (point.dot(axis) + (surface_span * 0.5)) / step;
    local.floor().clamp(0.0, order.saturating_sub(1) as f32) as usize
}

fn virtual_surface_center(
    face: Face,
    row: usize,
    col: usize,
    order: usize,
    surface_span: f32,
    half_extent: f32,
) -> Vec3 {
    let cubie = sticker_cubie_coord(face, row, col, order);
    let mut center = cubie_body_translation(cubie, order, surface_span);

    match face {
        Face::Front => center.z = half_extent,
        Face::Back => center.z = -half_extent,
        Face::Up => center.y = half_extent,
        Face::Down => center.y = -half_extent,
        Face::Right => center.x = half_extent,
        Face::Left => center.x = -half_extent,
    }

    center
}

fn axis_face(axis: Vec3) -> Option<Face> {
    if axis.abs_diff_eq(Vec3::X, 0.0001) {
        Some(Face::Right)
    } else if axis.abs_diff_eq(-Vec3::X, 0.0001) {
        Some(Face::Left)
    } else if axis.abs_diff_eq(Vec3::Y, 0.0001) {
        Some(Face::Up)
    } else if axis.abs_diff_eq(-Vec3::Y, 0.0001) {
        Some(Face::Down)
    } else if axis.abs_diff_eq(Vec3::Z, 0.0001) {
        Some(Face::Front)
    } else if axis.abs_diff_eq(-Vec3::Z, 0.0001) {
        Some(Face::Back)
    } else {
        None
    }
}

fn slice_start_layer(face: Face, cubie: UVec3, order: u8) -> u8 {
    let max = order.saturating_sub(1);
    match face {
        Face::Up => max.saturating_sub(cubie.y as u8),
        Face::Right => max.saturating_sub(cubie.x as u8),
        Face::Front => max.saturating_sub(cubie.z as u8),
        Face::Down => cubie.y as u8,
        Face::Left => cubie.x as u8,
        Face::Back => cubie.z as u8,
    }
}

fn projected_turn_motion(
    camera: &Camera,
    camera_transform: &GlobalTransform,
    world_point: Vec3,
    turn: TurnCommand,
) -> Option<Vec2> {
    let projected_start = camera
        .world_to_viewport(camera_transform, world_point)
        .ok()?;
    let direction = turn_rotation_angle(turn).signum();
    if direction.abs() <= f32::EPSILON {
        return None;
    }

    let projected_end = camera
        .world_to_viewport(
            camera_transform,
            Quat::from_axis_angle(turn_rotation_axis(turn.face), 0.12 * direction) * world_point,
        )
        .ok()?;
    let delta = projected_end - projected_start;
    (delta.length_squared() > f32::EPSILON).then_some(delta.normalize())
}

fn cube_face_offset(order: u8) -> f32 {
    let step = CUBE_FACE_SPAN / order.max(1) as f32;
    let cube_size = CUBE_FACE_SPAN + (step * 0.12);
    cube_size / 2.0 + 0.03
}

#[cfg(test)]
fn face_outward_normal(face: Face) -> Vec3 {
    match face {
        Face::Up => Vec3::Y,
        Face::Right => Vec3::X,
        Face::Front => Vec3::Z,
        Face::Down => -Vec3::Y,
        Face::Left => -Vec3::X,
        Face::Back => -Vec3::Z,
    }
}

fn ease_out_cubic(progress: f32) -> f32 {
    let inverse = 1.0 - progress.clamp(0.0, 1.0);
    1.0 - inverse * inverse * inverse
}

fn normalize_angle(angle: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    (angle + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI
}

fn shortest_angle_delta(current: f32, target: f32) -> f32 {
    normalize_angle(target - current)
}

fn nearest_orbit_snap(yaw: f32, pitch: f32) -> Option<Vec2> {
    const SNAP_PITCHES: [f32; 2] = [0.26, -0.26];
    const SNAP_THRESHOLD: f32 = 0.2;
    let snap_yaws = [
        std::f32::consts::FRAC_PI_4,
        std::f32::consts::FRAC_PI_4 + std::f32::consts::FRAC_PI_2,
        std::f32::consts::FRAC_PI_4 + std::f32::consts::PI,
        std::f32::consts::FRAC_PI_4 + (3.0 * std::f32::consts::FRAC_PI_2),
    ];

    let mut best: Option<(f32, Vec2)> = None;
    for snap_yaw in snap_yaws {
        for snap_pitch in SNAP_PITCHES {
            let delta = shortest_angle_delta(yaw, snap_yaw).abs() + (pitch - snap_pitch).abs();
            if delta > SNAP_THRESHOLD {
                continue;
            }

            match best {
                Some((best_delta, _)) if delta >= best_delta => {}
                _ => best = Some((delta, Vec2::new(normalize_angle(snap_yaw), snap_pitch))),
            }
        }
    }

    best.map(|(_, target)| target)
}

fn turn_animation_duration_secs(turn: TurnCommand) -> f32 {
    match turn.rotation {
        RotationAmount::HalfTurn => 0.2,
        RotationAmount::Clockwise | RotationAmount::CounterClockwise => 0.14,
    }
}

fn turn_rotation_axis(face: Face) -> Vec3 {
    match face {
        Face::Up => Vec3::Y,
        Face::Right => Vec3::X,
        Face::Front => Vec3::Z,
        Face::Down => -Vec3::Y,
        Face::Left => -Vec3::X,
        Face::Back => -Vec3::Z,
    }
}

fn turn_rotation_angle(turn: TurnCommand) -> f32 {
    let quarter_turns = match turn.rotation {
        RotationAmount::Clockwise => 1.0,
        RotationAmount::HalfTurn => 2.0,
        RotationAmount::CounterClockwise => -1.0,
    };
    -quarter_turns * std::f32::consts::FRAC_PI_2
}

fn cubie_matches_turn(order: u8, turn: TurnCommand, cubie: UVec3) -> bool {
    let (axis_value, min_layer, max_layer) = match turn.face {
        Face::Up => {
            let max = u32::from(order.saturating_sub(1));
            let min_layer = max
                .saturating_sub(u32::from(turn.start_layer))
                .saturating_sub(u32::from(turn.width.saturating_sub(1)));
            let max_layer = max.saturating_sub(u32::from(turn.start_layer));
            (cubie.y, min_layer, max_layer)
        }
        Face::Right => {
            let max = u32::from(order.saturating_sub(1));
            let min_layer = max
                .saturating_sub(u32::from(turn.start_layer))
                .saturating_sub(u32::from(turn.width.saturating_sub(1)));
            let max_layer = max.saturating_sub(u32::from(turn.start_layer));
            (cubie.x, min_layer, max_layer)
        }
        Face::Front => {
            let max = u32::from(order.saturating_sub(1));
            let min_layer = max
                .saturating_sub(u32::from(turn.start_layer))
                .saturating_sub(u32::from(turn.width.saturating_sub(1)));
            let max_layer = max.saturating_sub(u32::from(turn.start_layer));
            (cubie.z, min_layer, max_layer)
        }
        Face::Down => {
            let min_layer = u32::from(turn.start_layer);
            let max_layer = min_layer.saturating_add(u32::from(turn.width.saturating_sub(1)));
            (cubie.y, min_layer, max_layer)
        }
        Face::Left => {
            let min_layer = u32::from(turn.start_layer);
            let max_layer = min_layer.saturating_add(u32::from(turn.width.saturating_sub(1)));
            (cubie.x, min_layer, max_layer)
        }
        Face::Back => {
            let min_layer = u32::from(turn.start_layer);
            let max_layer = min_layer.saturating_add(u32::from(turn.width.saturating_sub(1)));
            (cubie.z, min_layer, max_layer)
        }
    };

    axis_value >= min_layer && axis_value <= max_layer
}

fn sticker_cubie_coord(face: Face, row: usize, col: usize, order: usize) -> UVec3 {
    let max = order.saturating_sub(1);
    let (x, y, z) = match face {
        Face::Up => (col, max, row),
        Face::Right => (max, max - row, max - col),
        Face::Front => (col, max - row, max),
        Face::Down => (col, 0, max - row),
        Face::Left => (0, max - row, col),
        Face::Back => (max - col, max - row, 0),
    };

    UVec3::new(x as u32, y as u32, z as u32)
}

fn cubie_axis_position(index: usize, order: usize, face_span: f32) -> f32 {
    if order <= 1 {
        0.0
    } else {
        let step = face_span / order as f32;
        (-face_span / 2.0) + (step * 0.5) + (index as f32 * step)
    }
}

fn cubie_body_translation(cubie: UVec3, order: usize, face_span: f32) -> Vec3 {
    Vec3::new(
        cubie_axis_position(cubie.x as usize, order, face_span),
        cubie_axis_position(cubie.y as usize, order, face_span),
        cubie_axis_position(cubie.z as usize, order, face_span),
    )
}

fn sticker_col_direction(face: Face) -> Vec3 {
    match face {
        Face::Up => Vec3::X,
        Face::Right => -Vec3::Z,
        Face::Front => Vec3::X,
        Face::Down => Vec3::X,
        Face::Left => Vec3::Z,
        Face::Back => -Vec3::X,
    }
}

fn sticker_row_direction(face: Face) -> Vec3 {
    match face {
        Face::Up => Vec3::Z,
        Face::Right | Face::Front | Face::Left | Face::Back => -Vec3::Y,
        Face::Down => -Vec3::Z,
    }
}

fn sticker_world_transform(
    face: Face,
    row: usize,
    col: usize,
    order: usize,
    face_span: f32,
    face_offset: f32,
) -> (Vec3, Quat) {
    let cubie = sticker_cubie_coord(face, row, col, order);
    let x = cubie.x as usize;
    let y = cubie.y as usize;
    let z = cubie.z as usize;

    match face {
        Face::Front => (
            Vec3::new(
                cubie_axis_position(x, order, face_span),
                cubie_axis_position(y, order, face_span),
                face_offset,
            ),
            sticker_rotation(face),
        ),
        Face::Back => (
            Vec3::new(
                cubie_axis_position(x, order, face_span),
                cubie_axis_position(y, order, face_span),
                -face_offset,
            ),
            sticker_rotation(face),
        ),
        Face::Right => (
            Vec3::new(
                face_offset,
                cubie_axis_position(y, order, face_span),
                cubie_axis_position(z, order, face_span),
            ),
            sticker_rotation(face),
        ),
        Face::Left => (
            Vec3::new(
                -face_offset,
                cubie_axis_position(y, order, face_span),
                cubie_axis_position(z, order, face_span),
            ),
            sticker_rotation(face),
        ),
        Face::Up => (
            Vec3::new(
                cubie_axis_position(x, order, face_span),
                face_offset,
                cubie_axis_position(z, order, face_span),
            ),
            sticker_rotation(face),
        ),
        Face::Down => (
            Vec3::new(
                cubie_axis_position(x, order, face_span),
                -face_offset,
                cubie_axis_position(z, order, face_span),
            ),
            sticker_rotation(face),
        ),
    }
}

fn sticker_transform(
    face: Face,
    row: usize,
    col: usize,
    order: usize,
    face_span: f32,
    face_offset: f32,
    sticker_mesh: &Handle<Mesh>,
) -> (Vec3, Quat, Handle<Mesh>) {
    let (translation, rotation) =
        sticker_world_transform(face, row, col, order, face_span, face_offset);
    (translation, rotation, sticker_mesh.clone())
}

fn sticker_rotation(face: Face) -> Quat {
    match face {
        Face::Front => Quat::IDENTITY,
        Face::Back => Quat::from_rotation_y(std::f32::consts::PI),
        Face::Right => Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
        Face::Left => Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2),
        Face::Up => Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
        Face::Down => Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
    }
}

fn color_for_sticker(color: StickerColor) -> Color {
    match color {
        StickerColor::White => Color::srgb_u8(239, 244, 248),
        StickerColor::Red => Color::srgb_u8(222, 75, 63),
        StickerColor::Green => Color::srgb_u8(54, 190, 126),
        StickerColor::Yellow => Color::srgb_u8(244, 191, 66),
        StickerColor::Orange => Color::srgb_u8(255, 139, 39),
        StickerColor::Blue => Color::srgb_u8(58, 118, 247),
    }
}

fn decode_face(face_code: u8) -> Option<Face> {
    match face_code {
        0 => Some(Face::Up),
        1 => Some(Face::Right),
        2 => Some(Face::Front),
        3 => Some(Face::Down),
        4 => Some(Face::Left),
        5 => Some(Face::Back),
        _ => None,
    }
}

fn decode_rotation(rotation_code: u8) -> Option<RotationAmount> {
    match rotation_code {
        0 => Some(RotationAmount::Clockwise),
        1 => Some(RotationAmount::HalfTurn),
        2 => Some(RotationAmount::CounterClockwise),
        _ => None,
    }
}

fn keyboard_selected_layer(keys: &ButtonInput<KeyCode>) -> u8 {
    for (key, layer) in [
        (KeyCode::Digit9, 9),
        (KeyCode::Digit8, 8),
        (KeyCode::Digit7, 7),
        (KeyCode::Digit6, 6),
        (KeyCode::Digit5, 5),
        (KeyCode::Digit4, 4),
        (KeyCode::Digit3, 3),
        (KeyCode::Digit2, 2),
        (KeyCode::Digit1, 1),
    ] {
        if keys.pressed(key) {
            return layer;
        }
    }

    1
}

fn keyboard_shortcut_turn(
    face: Face,
    rotation: RotationAmount,
    order: u8,
    wide_turn: bool,
    selected_layer: u8,
) -> Option<TurnCommand> {
    if selected_layer == 0 || selected_layer > order {
        return None;
    }

    let start_layer = selected_layer - 1;
    let width = if wide_turn { 2 } else { 1 };
    let turn = TurnCommand {
        face,
        start_layer,
        width,
        rotation,
    };

    CubeOrder::new(order)
        .ok()
        .filter(|cube_order| turn.validate_for(*cube_order).is_ok())
        .map(|_| turn)
}

fn format_turn(turn: TurnCommand) -> String {
    let face = match turn.face {
        Face::Up => "U",
        Face::Right => "R",
        Face::Front => "F",
        Face::Down => "D",
        Face::Left => "L",
        Face::Back => "B",
    };

    let width = if turn.width > 1 {
        format!("{}w", turn.width)
    } else {
        String::new()
    };

    let inner = if turn.start_layer > 0 {
        format!("[{}]", turn.start_layer + 1)
    } else {
        String::new()
    };

    let rotation = match turn.rotation {
        RotationAmount::Clockwise => "",
        RotationAmount::HalfTurn => "2",
        RotationAmount::CounterClockwise => "'",
    };

    format!("{width}{face}{inner}{rotation}")
}

fn normalize_canvas_selector(canvas_id: &str) -> String {
    if canvas_id.starts_with('#') {
        canvas_id.to_owned()
    } else {
        format!("#{canvas_id}")
    }
}

fn normalize_base_path(base_path: &str) -> String {
    let trimmed = base_path.trim();

    if trimmed.is_empty() || trimmed == "/" {
        "/".to_owned()
    } else {
        let without_trailing = trimmed.trim_end_matches('/');
        format!("{without_trailing}/")
    }
}

fn with_runtime<R>(f: impl FnOnce(&RuntimeBridge) -> R) -> R {
    RUNTIME.with(|runtime| f(&runtime.borrow()))
}

fn with_runtime_mut<R>(f: impl FnOnce(&mut RuntimeBridge) -> R) -> R {
    RUNTIME.with(|runtime| f(&mut runtime.borrow_mut()))
}

fn update_runtime(f: impl FnOnce(&mut RuntimeBridge) -> Result<(), String>) -> bool {
    with_runtime_mut(|runtime| match f(runtime) {
        Ok(()) => true,
        Err(error) => {
            runtime.set_message(error);
            false
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{
        CanvasTouchSpace, CubeVisual, CubeVisualRoot, OrbitRig, RuntimeBridge,
        ScreenStickerCandidate, TurnAnimationPivot,
        TOUCH_MOUSE_SUPPRESSION_SECS, TouchOrbitMode, cube_surface_hit_from_ray,
        clear_cube_visuals, cubie_matches_turn, decode_face, decode_rotation,
        face_outward_normal, format_turn, keyboard_shortcut_turn, nearest_orbit_snap,
        normalize_base_path, normalize_canvas_selector, normalize_touch_position,
        reset_cube, set_touch_orbit_mode, should_begin_mouse_orbit,
        should_emulate_two_finger_touch, should_reset_single_touch_gesture,
        slice_face_from_sticker_drag, slice_start_layer, sticker_rotation, surface_axis_index,
        turn_rotation_angle, virtual_cube_half_extent, virtual_surface_center,
    };
    use bevy::{
        ecs::system::SystemState,
        prelude::{ChildOf, Commands, Entity, MouseButton, Query, UVec3, Vec2, Vec3, With, World},
    };
    use rubik_core::{CubeOrder, Face, RotationAmount, TurnCommand};

    fn sample_sticker_candidate(face: Face, cubie: UVec3) -> ScreenStickerCandidate {
        ScreenStickerCandidate {
            face,
            row: 0,
            col: 0,
            cubie,
            world_center: Vec3::new(0.0, 0.0, 1.0),
            center: Vec2::new(100.0, 100.0),
            radius: 18.0,
            projected_col_axis: Vec2::X,
            projected_row_axis: Vec2::NEG_Y,
            col_axis: Vec3::X,
            row_axis: -Vec3::Y,
            corners: [
                Vec2::new(82.0, 82.0),
                Vec2::new(118.0, 82.0),
                Vec2::new(118.0, 118.0),
                Vec2::new(82.0, 118.0),
            ],
        }
    }

    #[test]
    fn normalizes_empty_base_path_to_root() {
        assert_eq!(normalize_base_path(""), "/");
    }

    #[test]
    fn keeps_repository_subpaths_trailing_slash() {
        assert_eq!(normalize_base_path("/rubikrs"), "/rubikrs/");
        assert_eq!(normalize_base_path("/rubikrs/"), "/rubikrs/");
    }

    #[test]
    fn prefixes_canvas_ids_with_a_hash() {
        assert_eq!(normalize_canvas_selector("rubik-canvas"), "#rubik-canvas");
        assert_eq!(normalize_canvas_selector("#rubik-canvas"), "#rubik-canvas");
    }

    #[test]
    fn decodes_faces_and_rotations_from_shell_codes() {
        assert_eq!(decode_face(2), Some(Face::Front));
        assert_eq!(decode_rotation(2), Some(RotationAmount::CounterClockwise));
        assert_eq!(decode_face(9), None);
    }

    #[test]
    fn formats_turns_for_status_panels() {
        let turn = TurnCommand {
            face: Face::Right,
            start_layer: 1,
            width: 2,
            rotation: RotationAmount::CounterClockwise,
        };

        assert_eq!(format_turn(turn), "2wR[2]'");
    }

    #[test]
    fn reset_export_is_callable_in_native_tests() {
        assert!(reset_cube());
    }

    #[test]
    fn apply_turn_records_animating_transition_from_previous_state() {
        let mut runtime = RuntimeBridge::new(CubeOrder::standard());
        let previous = runtime.engine.state().clone();
        let turn = TurnCommand::outer(Face::Front, RotationAmount::Clockwise);

        runtime.apply_turn(turn).expect("turn should apply");

        let transition = runtime.last_transition.expect("transition should exist");
        assert_eq!(transition.scene_revision, runtime.scene_revision);
        assert_eq!(transition.from_state, previous);
        assert_eq!(transition.animation, Some(turn));
    }

    #[test]
    fn undo_records_the_inverse_turn_for_animation() {
        let mut runtime = RuntimeBridge::new(CubeOrder::standard());
        let turn = TurnCommand::outer(Face::Right, RotationAmount::Clockwise);
        runtime.apply_turn(turn).expect("turn should apply");
        let scrambled = runtime.engine.state().clone();

        runtime.undo().expect("undo should apply");

        let transition = runtime.last_transition.expect("transition should exist");
        assert_eq!(transition.from_state, scrambled);
        assert_eq!(transition.animation, Some(turn.inverse()));
    }

    #[test]
    fn layer_selection_matches_named_face_depths() {
        let turn = TurnCommand {
            face: Face::Right,
            start_layer: 1,
            width: 2,
            rotation: RotationAmount::CounterClockwise,
        };

        assert!(cubie_matches_turn(5, turn, UVec3::new(3, 1, 2)));
        assert!(cubie_matches_turn(5, turn, UVec3::new(2, 4, 0)));
        assert!(!cubie_matches_turn(5, turn, UVec3::new(4, 1, 2)));
        assert!(!cubie_matches_turn(5, turn, UVec3::new(1, 1, 2)));
    }

    #[test]
    fn turn_rotation_angle_respects_face_clockwise_view() {
        assert_eq!(
            turn_rotation_angle(TurnCommand::outer(Face::Front, RotationAmount::Clockwise)),
            -std::f32::consts::FRAC_PI_2
        );
        assert_eq!(
            turn_rotation_angle(TurnCommand::outer(
                Face::Back,
                RotationAmount::CounterClockwise
            )),
            std::f32::consts::FRAC_PI_2
        );
    }

    #[test]
    fn sticker_rotations_face_the_expected_cube_normals() {
        for face in Face::ALL {
            let rotated_normal = sticker_rotation(face) * Vec3::Z;
            assert!(rotated_normal.abs_diff_eq(face_outward_normal(face), 0.0001));
        }
    }

    #[test]
    fn orbit_snap_targets_nearby_isometric_views() {
        let target = nearest_orbit_snap(0.81, 0.24).expect("should snap to a nearby view");
        assert!((target.x - std::f32::consts::FRAC_PI_4).abs() < 0.05);
        assert!((target.y - 0.26).abs() < 0.05);
        assert!(nearest_orbit_snap(1.7, 0.24).is_none());
    }

    #[test]
    fn right_drag_always_starts_mouse_orbit() {
        assert!(should_begin_mouse_orbit(MouseButton::Right, 0.0, true));
        assert!(should_begin_mouse_orbit(MouseButton::Right, 12.0, false));
    }

    #[test]
    fn left_drag_only_orbits_when_blank_space_moves_past_threshold() {
        assert!(!should_begin_mouse_orbit(MouseButton::Left, 12.0, true));
        assert!(!should_begin_mouse_orbit(MouseButton::Left, 4.0, false));
        assert!(should_begin_mouse_orbit(MouseButton::Left, 12.0, false));
    }

    #[test]
    fn multi_touch_transition_restarts_single_touch_gesture() {
        assert!(should_reset_single_touch_gesture(
            TouchOrbitMode::MultiFinger,
            None,
            7,
            false
        ));
    }

    #[test]
    fn continuing_single_touch_keeps_current_gesture() {
        assert!(!should_reset_single_touch_gesture(
            TouchOrbitMode::SingleFinger { id: 7 },
            Some(7),
            7,
            false
        ));
    }

    #[test]
    fn touch_positions_are_shifted_into_canvas_space_without_extra_scaling() {
        let touch_space = CanvasTouchSpace {
            offset: Vec2::new(40.0, 12.0),
            scale: Vec2::new(0.5, 2.0),
            ..Default::default()
        };

        assert_eq!(
            normalize_touch_position(touch_space, Vec2::new(164.0, 108.0)),
            Vec2::new(124.0, 96.0)
        );
    }

    #[test]
    fn dom_touch_positions_scale_canvas_relative_offsets_without_rect_subtraction() {
        let touch_space = CanvasTouchSpace {
            offset: Vec2::new(19.0, 19.0),
            scale: Vec2::splat(0.5),
            ..Default::default()
        };

        assert_eq!(
            touch_space.canvas_relative_position(Vec2::new(795.0, 515.0)),
            Vec2::new(397.5, 257.5)
        );
    }

    #[test]
    fn entering_multi_touch_orbit_clears_stale_touch_history() {
        let mut orbit = OrbitRig::default();
        orbit.touch_start_position = Some(Vec2::new(12.0, 18.0));
        orbit.touch_start_center = Some(Vec2::new(32.0, 48.0));
        orbit.previous_pinch_distance = Some(120.0);

        set_touch_orbit_mode(&mut orbit, TouchOrbitMode::MultiFinger);

        assert_eq!(orbit.touch_drag_mode, TouchOrbitMode::MultiFinger);
        assert_eq!(orbit.touch_start_position, None);
        assert_eq!(orbit.touch_start_center, None);
        assert_eq!(orbit.previous_pinch_distance, None);
    }

    #[test]
    fn touch_orbit_yaw_uses_absolute_displacement_from_start() {
        let mut orbit = OrbitRig::default();
        orbit.yaw = 1.0;
        orbit.pitch = 0.3;
        orbit.touch_drag_mode = TouchOrbitMode::SingleFinger { id: 0 };
        orbit.touch_start_yaw = 1.0;
        orbit.touch_start_pitch = 0.3;
        orbit.touch_start_position = Some(Vec2::new(100.0, 200.0));

        let displacement = Vec2::new(150.0, 180.0) - Vec2::new(100.0, 200.0);
        let expected_yaw = 1.0 + displacement.x * 0.008;
        let expected_pitch = (0.3 + displacement.y * 0.006).clamp(-1.15, 1.15);

        assert!((orbit.yaw - 1.0).abs() < 0.001);
        assert!((orbit.pitch - 0.3).abs() < 0.001);
        assert_eq!(expected_yaw, 1.0 + 50.0 * 0.008);
        assert_eq!(expected_pitch, 0.18);
    }

    #[test]
    fn shift_pressed_emulates_two_finger_touch_for_single_touch_gesture() {
        assert!(should_emulate_two_finger_touch(true, 1));
        assert!(!should_emulate_two_finger_touch(false, 1));
        assert!(!should_emulate_two_finger_touch(true, 2));
    }

    #[test]
    fn orbit_rig_defaults_to_no_touch_mouse_suppression() {
        let orbit = OrbitRig::default();

        assert_eq!(orbit.touch_mouse_suppression_secs, 0.0);
        assert_eq!(orbit.touch_start_position, None);
        assert!(TOUCH_MOUSE_SUPPRESSION_SECS > 0.0);
    }

    #[test]
    fn keyboard_shortcut_can_target_inner_layers() {
        let turn = keyboard_shortcut_turn(Face::Right, RotationAmount::Clockwise, 4, false, 2)
            .expect("second layer should be valid on 4x4");
        assert_eq!(turn.start_layer, 1);
        assert_eq!(turn.width, 1);
    }

    #[test]
    fn keyboard_shortcut_can_expand_to_wide_turns() {
        let turn =
            keyboard_shortcut_turn(Face::Front, RotationAmount::CounterClockwise, 4, true, 1)
                .expect("wide outer turn should be valid on 4x4");
        assert_eq!(turn.start_layer, 0);
        assert_eq!(turn.width, 2);
        assert!(
            keyboard_shortcut_turn(Face::Front, RotationAmount::Clockwise, 2, true, 2).is_none()
        );
    }

    #[test]
    fn horizontal_drag_on_front_sticker_selects_the_up_slice() {
        let candidate = sample_sticker_candidate(Face::Front, UVec3::new(1, 2, 2));
        let face = slice_face_from_sticker_drag(candidate, Vec2::new(36.0, 0.0));

        assert_eq!(face, Some(Face::Up));
        assert_eq!(
            slice_start_layer(
                face.expect("front drag should map to up"),
                candidate.cubie,
                4
            ),
            1
        );
    }

    #[test]
    fn vertical_drag_on_front_sticker_selects_the_right_slice() {
        let candidate = sample_sticker_candidate(Face::Front, UVec3::new(2, 1, 2));
        let face = slice_face_from_sticker_drag(candidate, Vec2::new(0.0, -32.0));

        assert_eq!(face, Some(Face::Right));
        assert_eq!(
            slice_start_layer(
                face.expect("front drag should map to right"),
                candidate.cubie,
                4
            ),
            1
        );
    }

    #[test]
    fn cube_surface_hit_reaches_the_front_face() {
        let half_extent = virtual_cube_half_extent(3);
        let hit = cube_surface_hit_from_ray(Vec3::new(0.0, 0.0, 6.0), Vec3::NEG_Z, half_extent)
            .expect("front-facing ray should hit the virtual cube");

        assert_eq!(hit.face, Face::Front);
        assert!((hit.point.z - half_extent).abs() < 0.0001);
    }

    #[test]
    fn cube_surface_hit_rejects_blank_space_outside_the_cube_silhouette() {
        let half_extent = virtual_cube_half_extent(3);
        let hit = cube_surface_hit_from_ray(Vec3::new(3.0, 3.0, 6.0), Vec3::NEG_Z, half_extent);

        assert_eq!(hit, None);
    }

    #[test]
    fn virtual_surface_maps_front_face_seams_to_a_real_cubie() {
        let half_extent = virtual_cube_half_extent(3);
        let surface_span = half_extent * 2.0;
        let point = Vec3::new(0.0, 0.0, half_extent);

        let row = surface_axis_index(Face::Front, point, 3, surface_span, true);
        let col = surface_axis_index(Face::Front, point, 3, surface_span, false);
        let center = virtual_surface_center(Face::Front, row, col, 3, surface_span, half_extent);

        assert_eq!((row, col), (1, 1));
        assert!(center.abs_diff_eq(Vec3::new(0.0, 0.0, half_extent), 0.0001));
    }

    #[test]
    fn virtual_surface_clamps_front_face_edges_to_outer_cubies() {
        let half_extent = virtual_cube_half_extent(3);
        let surface_span = half_extent * 2.0;
        let point = Vec3::new(-half_extent + 0.001, half_extent - 0.001, half_extent);

        let row = surface_axis_index(Face::Front, point, 3, surface_span, true);
        let col = surface_axis_index(Face::Front, point, 3, surface_span, false);

        assert_eq!((row, col), (0, 0));
    }

    #[test]
    fn clear_cube_visuals_despawns_a_visual_tree_from_its_root() {
        let mut world = World::new();
        let root = world.spawn((CubeVisual, CubeVisualRoot)).id();
        let pivot = world
            .spawn((CubeVisual, TurnAnimationPivot, ChildOf(root)))
            .id();
        let cubie = world.spawn((CubeVisual, ChildOf(root))).id();

        let mut system_state: SystemState<(
            Commands<'_, '_>,
            Query<'_, '_, Entity, With<CubeVisualRoot>>,
        )> = SystemState::new(&mut world);
        {
            let (mut commands, visual_roots) = system_state.get_mut(&mut world);
            clear_cube_visuals(&mut commands, &visual_roots);
        }
        system_state.apply(&mut world);
        world.flush();

        assert!(!world.entities().contains(root));
        assert!(!world.entities().contains(pivot));
        assert!(!world.entities().contains(cubie));
    }
}
