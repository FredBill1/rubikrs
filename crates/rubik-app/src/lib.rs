#![forbid(unsafe_code)]

use std::{
    cell::RefCell,
    collections::{BTreeSet, VecDeque},
};

use bevy::{
    asset::RenderAssetUsages,
    core_pipeline::tonemapping::Tonemapping,
    input::{
        mouse::{MouseMotion, MouseScrollUnit, MouseWheel},
        touch::Touches,
    },
    light::{CascadeShadowConfigBuilder, DirectionalLightShadowMap, ShadowFilteringMethod},
    mesh::Indices,
    prelude::*,
    render::render_resource::PrimitiveTopology,
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

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
struct CubieBodyVisual {
    cubie: UVec3,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
struct StickerVisual {
    face: Face,
    row: u8,
    col: u8,
    cubie: UVec3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StickerSlotSpec {
    face: Face,
    row: usize,
    col: usize,
    cubie: UVec3,
}

#[derive(Debug, Clone)]
struct BodyMeshTemplate {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    indices: Vec<u32>,
}

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
    mouse_start_yaw: f32,
    mouse_start_pitch: f32,
    mouse_start_position: Option<Vec2>,
    previous_pinch_distance: Option<f32>,
}

#[derive(Resource, Default)]
struct DirectTurnInputState {
    mouse_candidate: Option<PointerGestureCandidate>,
    touch_candidate: Option<TouchGestureCandidate>,
    queued_turn: Option<TurnCommand>,
    active_slice_drag: Option<ActiveSliceDrag>,
    pending_slice_snap: Option<SliceSnapRequest>,
}

#[derive(Resource, Default)]
struct VisualSyncState {
    rendered_revision: u64,
    active_animations: Vec<ActiveTurnAnimation>,
    completed_animation_revision: Option<u64>,
    temp_pivot_entities: Vec<Entity>,
    live_slice_turn: Option<TurnCommand>,
    live_slice_animated_stickers: Vec<usize>,
}

#[derive(Resource, Default)]
struct CubeVisualPool {
    order: Option<u32>,
    root_entity: Option<Entity>,
    pivot_entity: Option<Entity>,
    static_body_entity: Option<Entity>,
    animated_body_entity: Option<Entity>,
    static_sticker_entity: Option<Entity>,
    animated_sticker_entity: Option<Entity>,
    sticker_visual_states: Vec<StickerVisual>,
    sticker_colors: Vec<StickerColor>,
    body_mesh_handles: Option<(Handle<Mesh>, Handle<Mesh>)>,
    body_mesh_template: Option<BodyMeshTemplate>,
    sticker_mesh_handles: Option<(Handle<Mesh>, Handle<Mesh>)>,
    sticker_mesh_template: Option<BodyMeshTemplate>,
    cubie_slots: Vec<UVec3>,
    sticker_slots: Vec<StickerSlotSpec>,
}

#[derive(Debug, Clone)]
struct RuntimeBridge {
    engine: CubeEngine,
    scene_revision: u64,
    last_message: String,
    timer: RuntimeTimer,
    last_transition: Option<RuntimeTransition>,
    animation_active: bool,
    turn_queue: VecDeque<TurnCommand>,
    turn_enqueue_count: u64,
}

#[derive(Debug, Clone, Default)]
struct RuntimeTimer {
    elapsed_before_millis: u64,
    started_at_millis: Option<u64>,
}

#[derive(Debug, Serialize)]
struct RuntimeStatus {
    order: u32,
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

#[derive(Debug, Clone, Copy)]
struct RuntimeSceneMeta {
    scene_revision: u64,
}

#[derive(Debug, Clone)]
struct RuntimeTransition {
    scene_revision: u64,
    from_state: CubeState,
    animation: Vec<TurnCommand>,
}

#[derive(Debug, Clone)]
struct ActiveTurnAnimation {
    scene_revision: u64,
    pivot_entity: Entity,
    turn: TurnCommand,
    animated_stickers: Vec<usize>,
    start_angle_radians: f32,
    angle_radians: f32,
    axis: Vec3,
    elapsed_secs: f32,
    duration_secs: f32,
    completion: TurnAnimationCompletion,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SliceDragInput {
    Mouse,
    Touch(u64),
}

#[derive(Debug, Clone, Copy)]
struct ActiveSliceDrag {
    input: SliceDragInput,
    start_position: Vec2,
    turn: TurnCommand,
    drag_direction: Vec2,
    angle_radians: f32,
}

#[derive(Debug, Clone, Copy)]
struct SliceSnapRequest {
    turn: TurnCommand,
    start_angle_radians: f32,
    target_angle_radians: f32,
    commit_turn: Option<TurnCommand>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TurnAnimationCompletion {
    RuntimeApplied,
    DirectSliceSnap { commit_turn: Option<TurnCommand> },
}

#[derive(Debug, Clone)]
struct PreparedTurnVisuals {
    pivot_entity: Entity,
    animated_stickers: Vec<usize>,
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
const SLICE_DRAG_QUARTER_TURN_PX: f32 = 130.0;
const SLICE_SNAP_BACK_DEGREES: f32 = 10.0;
const SLICE_DRAG_MAX_ABS_RADIANS: f32 = std::f32::consts::TAU;
const FACE_TAP_RADIUS_SCALE: f32 = 0.7;
const VIRTUAL_SURFACE_INSET: f32 = 0.03;
const TOUCH_MOUSE_SUPPRESSION_SECS: f32 = 0.12;

impl Default for OrbitRig {
    fn default() -> Self {
        Self {
            yaw: 0.78,
            pitch: 0.5,
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
            mouse_start_yaw: 0.0,
            mouse_start_pitch: 0.0,
            mouse_start_position: None,
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
    order: u32,
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

impl ActiveSliceDrag {
    fn update_angle(&mut self, pointer_position: Vec2) {
        self.angle_radians =
            slice_drag_angle_radians(self.start_position, pointer_position, self.drag_direction);
    }

    fn snap_request(self) -> SliceSnapRequest {
        let signed_quarters = signed_slice_snap_quarters(self.angle_radians);
        let target_angle_radians = signed_quarters as f32 * std::f32::consts::FRAC_PI_2;
        SliceSnapRequest {
            turn: self.turn,
            start_angle_radians: self.angle_radians,
            target_angle_radians,
            commit_turn: turn_for_signed_slice_quarters(self.turn, signed_quarters),
        }
    }

    fn snap_back_request(self) -> SliceSnapRequest {
        SliceSnapRequest {
            turn: self.turn,
            start_angle_radians: self.angle_radians,
            target_angle_radians: 0.0,
            commit_turn: None,
        }
    }
}

fn slice_drag_angle_radians(
    start_position: Vec2,
    pointer_position: Vec2,
    drag_direction: Vec2,
) -> f32 {
    let projected_pixels = (pointer_position - start_position).dot(drag_direction);
    (-(projected_pixels / SLICE_DRAG_QUARTER_TURN_PX) * std::f32::consts::FRAC_PI_2)
        .clamp(-SLICE_DRAG_MAX_ABS_RADIANS, SLICE_DRAG_MAX_ABS_RADIANS)
}

fn signed_slice_snap_quarters(angle_radians: f32) -> i32 {
    let snap_back_radians = SLICE_SNAP_BACK_DEGREES.to_radians();
    let abs_angle = angle_radians.abs();
    if abs_angle <= snap_back_radians {
        return 0;
    }

    if abs_angle <= std::f32::consts::FRAC_PI_2 {
        return if angle_radians.is_sign_positive() {
            1
        } else {
            -1
        };
    }

    (angle_radians / std::f32::consts::FRAC_PI_2).round() as i32
}

fn turn_for_signed_slice_quarters(
    base_turn: TurnCommand,
    signed_quarters: i32,
) -> Option<TurnCommand> {
    let rotation = match signed_quarters.rem_euclid(4) {
        0 => return None,
        1 => RotationAmount::CounterClockwise,
        2 => RotationAmount::HalfTurn,
        3 => RotationAmount::Clockwise,
        _ => unreachable!("quarter turn modulo is always in 0..=3"),
    };

    Some(TurnCommand {
        rotation,
        ..base_turn
    })
}

fn slice_snap_duration_secs(start_angle_radians: f32, target_angle_radians: f32) -> f32 {
    let quarter_turn_delta =
        (target_angle_radians - start_angle_radians).abs() / std::f32::consts::FRAC_PI_2;
    (0.06 + (quarter_turn_delta * 0.08)).clamp(0.08, 0.24)
}

fn active_slice_drag_for_input(
    state: &mut DirectTurnInputState,
    input: SliceDragInput,
) -> Option<&mut ActiveSliceDrag> {
    state
        .active_slice_drag
        .as_mut()
        .filter(|drag| drag.input == input)
}

fn finish_active_slice_drag(
    state: &mut DirectTurnInputState,
    input: SliceDragInput,
    canceled: bool,
) -> bool {
    if !state
        .active_slice_drag
        .is_some_and(|drag| drag.input == input)
    {
        return false;
    }

    if let Some(active_drag) = state.active_slice_drag.take() {
        state.pending_slice_snap = Some(if canceled {
            active_drag.snap_back_request()
        } else {
            active_drag.snap_request()
        });
    }
    true
}

fn cancel_active_slice_drag(state: &mut DirectTurnInputState) -> bool {
    let Some(active_drag) = state.active_slice_drag.take() else {
        return false;
    };
    state.pending_slice_snap = Some(active_drag.snap_back_request());
    true
}

fn build_active_slice_drag(
    camera: &Camera,
    camera_transform: &GlobalTransform,
    order: u32,
    input: SliceDragInput,
    candidate: ScreenStickerCandidate,
    start_position: Vec2,
    current_position: Vec2,
) -> Option<ActiveSliceDrag> {
    let drag = current_position - start_position;
    if drag.length() <= POINTER_TAP_MAX_DRAG_PX {
        return None;
    }

    let turn_face = slice_face_from_sticker_drag(candidate, drag)?;
    let start_layer = slice_start_layer(turn_face, candidate.cubie, order);
    let turn = TurnCommand {
        face: turn_face,
        start_layer,
        width: 1,
        rotation: RotationAmount::Clockwise,
    };
    let drag_direction =
        projected_turn_motion(camera, camera_transform, candidate.world_center, turn)?;
    if drag.normalize().dot(drag_direction).abs() <= 0.2 {
        return None;
    }

    let mut active_drag = ActiveSliceDrag {
        input,
        start_position,
        turn,
        drag_direction,
        angle_radians: 0.0,
    };
    active_drag.update_angle(current_position);
    Some(active_drag)
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
            turn_queue: VecDeque::new(),
            turn_enqueue_count: 0,
        }
    }

    fn snapshot(&self) -> RuntimeSnapshot {
        RuntimeSnapshot {
            state: self.engine.state().clone(),
            scene_revision: self.scene_revision,
            transition: self.last_transition.clone(),
        }
    }

    fn scene_meta(&self) -> RuntimeSceneMeta {
        RuntimeSceneMeta {
            scene_revision: self.scene_revision,
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

    fn record_transition(&mut self, from_state: CubeState, animation: Vec<TurnCommand>) {
        self.last_transition = Some(RuntimeTransition {
            scene_revision: self.scene_revision,
            from_state,
            animation,
        });
        self.animation_active = false;
    }

    fn turn_layer_range(order: u32, turn: TurnCommand) -> (u32, u32) {
        match turn.face {
            Face::Up | Face::Right | Face::Front => {
                let max_world = order.saturating_sub(1);
                let max_layer = max_world.saturating_sub(turn.start_layer);
                let min_layer = max_layer.saturating_sub(turn.width.saturating_sub(1));
                (min_layer, max_layer)
            }
            Face::Down | Face::Left | Face::Back => {
                let min_layer = turn.start_layer;
                let max_layer = min_layer.saturating_add(turn.width.saturating_sub(1));
                (min_layer, max_layer)
            }
        }
    }

    fn turns_share_axis(a: Face, b: Face) -> bool {
        matches!(
            (a, b),
            (Face::Up, Face::Up)
                | (Face::Up, Face::Down)
                | (Face::Down, Face::Up)
                | (Face::Down, Face::Down)
                | (Face::Right, Face::Right)
                | (Face::Right, Face::Left)
                | (Face::Left, Face::Right)
                | (Face::Left, Face::Left)
                | (Face::Front, Face::Front)
                | (Face::Front, Face::Back)
                | (Face::Back, Face::Front)
                | (Face::Back, Face::Back)
        )
    }

    fn turns_compatible(_order: u32, a: TurnCommand, b: TurnCommand) -> bool {
        Self::turns_share_axis(a.face, b.face)
    }

    fn dequeue_compatible_batch(&mut self) -> Vec<TurnCommand> {
        let order = self.engine.order().get();
        let mut batch: Vec<TurnCommand> = Vec::new();
        while let Some(candidate) = self.turn_queue.front().copied() {
            if batch
                .iter()
                .all(|t: &TurnCommand| Self::turns_compatible(order, *t, candidate))
            {
                batch.push(
                    self.turn_queue
                        .pop_front()
                        .expect("front exists but pop failed"),
                );
            } else {
                break;
            }
        }
        batch
    }

    fn merge_batch_turns(order: u32, turns: &[TurnCommand]) -> Vec<TurnCommand> {
        if turns.len() <= 1 {
            return turns.to_vec();
        }

        let canonical = turns[0].face;
        let opposite = opposite_face(canonical);

        let mut layer_net: Vec<i8> = vec![0; order as usize];
        for &turn in turns {
            let (min, max) = Self::turn_layer_range(order, turn);
            let dir = rotation_direction_value(turn.rotation);
            let normalized = if turn.face == canonical {
                dir
            } else if turn.face == opposite {
                -dir
            } else {
                0
            };
            for l in min..=max {
                layer_net[l as usize] += normalized;
            }
        }

        let mut merged: Vec<TurnCommand> = Vec::new();
        let mut i: u32 = 0;
        while i < order {
            let net = ((layer_net[i as usize] % 4) + 4) % 4;
            if net != 0 {
                let rot = match net {
                    1 => RotationAmount::Clockwise,
                    3 => RotationAmount::CounterClockwise,
                    2 => RotationAmount::HalfTurn,
                    _ => unreachable!(),
                };
                let start_abs = i;
                let mut width: u32 = 1;
                // Coalesce consecutive layers with the same net rotation
                let mut j = i + 1;
                while j < order {
                    let next = ((layer_net[j as usize] % 4) + 4) % 4;
                    if next == net {
                        width += 1;
                        j += 1;
                    } else {
                        break;
                    }
                }
                let abs_end = start_abs + width - 1;
                let canonical_start = match canonical {
                    Face::Up | Face::Right | Face::Front => (order - 1).saturating_sub(abs_end),
                    Face::Down | Face::Left | Face::Back => start_abs,
                };
                merged.push(TurnCommand {
                    face: canonical,
                    start_layer: canonical_start,
                    width,
                    rotation: rot,
                });
                i = j;
            } else {
                i += 1;
            }
        }
        merged
    }

    fn process_queue_head(&mut self) {
        let batch = self.dequeue_compatible_batch();
        if batch.is_empty() {
            return;
        }
        let from_state = self.engine.state().clone();
        for turn in &batch {
            if self.engine.apply_turn(*turn).is_err() {
                self.set_message(format!(
                    "Failed to apply queued turn {}.",
                    format_turn(*turn)
                ));
                continue;
            }
        }
        self.bump_scene();
        self.record_transition(from_state, batch);
    }

    fn set_order(&mut self, order: CubeOrder) {
        let from_state = self.engine.state().clone();
        self.engine = CubeEngine::new(order);
        self.timer.reset();
        self.bump_scene();
        self.record_transition(from_state, Vec::new());
        self.set_message(format!("Switched to {}x{}.", order.get(), order.get()));
    }

    fn reset(&mut self) {
        let from_state = self.engine.state().clone();
        self.engine.reset();
        self.timer.reset();
        self.bump_scene();
        self.record_transition(from_state, Vec::new());
        self.set_message("Reset cube to solved state.");
    }

    fn apply_turn(&mut self, turn: TurnCommand) -> Result<(), String> {
        self.turn_queue.push_back(turn);
        self.turn_enqueue_count += 1;
        self.bump_scene();
        self.set_message(format!("Queued {}.", format_turn(turn)));
        Ok(())
    }

    fn commit_direct_turn(&mut self, turn: TurnCommand) -> u64 {
        match self.engine.apply_turn(turn) {
            Ok(()) => {
                self.turn_enqueue_count += 1;
                self.bump_scene();
                self.last_transition = None;
                self.animation_active = false;
                self.set_message(format!("Turned {}.", format_turn(turn)));
            }
            Err(error) => {
                self.set_message(format!(
                    "Failed to apply direct turn {}: {error}.",
                    format_turn(turn)
                ));
            }
        }
        self.scene_revision
    }

    fn undo(&mut self) -> Result<(), String> {
        let from_state = self.engine.state().clone();
        let turn = self.engine.undo().map_err(|error| error.to_string())?;
        self.bump_scene();
        self.record_transition(from_state, vec![turn.inverse()]);
        self.set_message(format!("Undid {}.", format_turn(turn)));
        Ok(())
    }

    fn redo(&mut self) -> Result<(), String> {
        let from_state = self.engine.state().clone();
        let turn = self.engine.redo().map_err(|error| error.to_string())?;
        self.bump_scene();
        self.record_transition(from_state, vec![turn]);
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
        self.record_transition(from_state, Vec::new());
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
        self.record_transition(from_state, Vec::new());
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
        .insert_resource(CubeVisualPool::default())
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
                direct_slice_drag_visuals,
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
pub fn animation_active() -> bool {
    with_runtime(|runtime| runtime.animation_active)
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn queue_idle() -> bool {
    with_runtime(|runtime| runtime.turn_queue.is_empty() && !runtime.animation_active)
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn turn_enqueue_count() -> u64 {
    with_runtime(|runtime| runtime.turn_enqueue_count)
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn cancel_pending_work() {
    with_runtime_mut(|runtime| {
        runtime.turn_queue.clear();
        runtime.last_transition = None;
        runtime.animation_active = false;
        runtime.scene_revision = runtime.scene_revision.wrapping_add(1);
    });
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
pub fn set_cube_order(order: u32) -> bool {
    let Ok(order) = CubeOrder::new(order) else {
        with_runtime_mut(|runtime| {
            runtime.set_message(format!(
                "Order {} is outside the supported {:?} range.",
                order,
                rubik_core::MIN_CUBE_ORDER..=rubik_core::MAX_CUBE_ORDER
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
pub fn apply_turn(face_code: u8, rotation_code: u8, start_layer: u32, width: u32) -> bool {
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

fn setup_scene(
    mut commands: Commands<'_, '_>,
    config: Res<'_, ShellConfig>,
    mut directional_shadow_map: ResMut<'_, DirectionalLightShadowMap>,
) {
    info!(
        "booting Bevy runtime on {} with canvas {}",
        config.base_path, config.canvas_selector
    );

    directional_shadow_map.size = 2048;

    commands.spawn((
        Camera3d::default(),
        Tonemapping::TonyMcMapface,
        ShadowFilteringMethod::Gaussian,
        Transform::from_xyz(-3.85, 3.15, 6.45).looking_at(Vec3::ZERO, Vec3::Y),
        AmbientLight {
            color: Color::srgb(1.0, 1.0, 1.0),
            brightness: 200.0,
            ..default()
        },
    ));

    commands.spawn((
        PointLight {
            intensity: 1_100_000.0,
            range: 42.0,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_xyz(5.5, 8.5, 5.5),
    ));

    commands.spawn((
        DirectionalLight {
            illuminance: 8_000.0,
            shadows_enabled: true,
            ..default()
        },
        CascadeShadowConfigBuilder {
            num_cascades: 1,
            minimum_distance: 0.1,
            maximum_distance: 25.0,
            first_cascade_far_bound: 10.0,
            overlap_proportion: 0.2,
        }
        .build(),
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.78, 0.92, 0.0)),
    ));
}

fn sync_cube_visuals(
    mut commands: Commands<'_, '_>,
    mut meshes: ResMut<'_, Assets<Mesh>>,
    mut materials: ResMut<'_, Assets<StandardMaterial>>,
    mut pool: ResMut<'_, CubeVisualPool>,
    mut sync_state: ResMut<'_, VisualSyncState>,
    mut pivots: Query<
        '_,
        '_,
        &mut Transform,
        (
            With<TurnAnimationPivot>,
            Without<CubieBodyVisual>,
            Without<StickerVisual>,
        ),
    >,
    mut visibilities: Query<'_, '_, &mut Visibility>,
    existing_visual_roots: Query<'_, '_, Entity, With<CubeVisualRoot>>,
) {
    let scene_meta = with_runtime(|runtime| runtime.scene_meta());

    if !sync_state.active_animations.is_empty() {
        let all_match = sync_state
            .active_animations
            .iter()
            .any(|a| a.scene_revision == scene_meta.scene_revision);
        if all_match {
            with_runtime_mut(|runtime| runtime.set_animation_active(true));
            return;
        }

        for &entity in &sync_state.temp_pivot_entities {
            commands.entity(entity).despawn();
        }
        sync_state.active_animations.clear();
        sync_state.temp_pivot_entities.clear();
        sync_state.live_slice_turn = None;
        sync_state.live_slice_animated_stickers.clear();
        sync_state.rendered_revision = 0;
        sync_state.completed_animation_revision = None;
        with_runtime_mut(|runtime| runtime.set_animation_active(false));
    }

    if scene_meta.scene_revision == sync_state.rendered_revision {
        let should_drain =
            with_runtime(|runtime| !runtime.turn_queue.is_empty() && !runtime.animation_active);
        if should_drain {
            with_runtime_mut(|runtime| runtime.process_queue_head());
        } else {
            with_runtime_mut(|runtime| runtime.set_animation_active(false));
            return;
        }
    }

    let scene_meta = with_runtime(|runtime| runtime.scene_meta());
    if scene_meta.scene_revision == sync_state.rendered_revision {
        with_runtime_mut(|runtime| runtime.set_animation_active(false));
        return;
    }

    let snapshot = with_runtime(|runtime| runtime.snapshot());
    let order = snapshot.state.order.get();
    let pending_turns: Option<(CubeState, Vec<TurnCommand>)> = snapshot
        .transition
        .as_ref()
        .filter(|transition| transition.scene_revision == snapshot.scene_revision)
        .filter(|transition| !transition.animation.is_empty())
        .filter(|_| sync_state.completed_animation_revision != Some(snapshot.scene_revision))
        .map(|transition| (transition.from_state.clone(), transition.animation.clone()));

    if cube_visual_pool_needs_rebuild(&pool, order, &existing_visual_roots) {
        clear_cube_visuals(&mut commands, &existing_visual_roots);
        let source_state = pending_turns
            .as_ref()
            .map(|(state, _)| state)
            .unwrap_or(&snapshot.state);
        let animation_turns = pending_turns.as_ref().map(|(_, turns)| turns.clone());
        let (new_pool, new_animations) = spawn_cube_visual_pool(
            &mut commands,
            &mut meshes,
            &mut materials,
            source_state,
            animation_turns,
            snapshot.scene_revision,
        );
        *pool = new_pool;
        sync_state.active_animations = new_animations;
        sync_state.temp_pivot_entities.clear();
        sync_state.live_slice_turn = None;
        sync_state.live_slice_animated_stickers.clear();
        sync_state.completed_animation_revision = None;
        sync_state.rendered_revision = if sync_state.active_animations.is_empty() {
            snapshot.scene_revision
        } else {
            0
        };
        with_runtime_mut(|runtime| {
            runtime.set_animation_active(!sync_state.active_animations.is_empty())
        });
        return;
    }

    if let Some((from_state, turns)) = pending_turns {
        if !pool_matches_pending_animation_source(&sync_state, snapshot.scene_revision) {
            apply_cube_state_to_pool(
                &mut meshes,
                &mut pool,
                &from_state,
                &mut pivots,
                &mut visibilities,
            );
        }
        begin_turn_batch_animation(
            &mut commands,
            &mut meshes,
            &mut materials,
            &pool,
            order,
            &turns,
            snapshot.scene_revision,
            &mut pivots,
            &mut visibilities,
            &mut sync_state,
        );
        sync_state.completed_animation_revision = None;
        sync_state.rendered_revision = if sync_state.active_animations.is_empty() {
            snapshot.scene_revision
        } else {
            0
        };
        with_runtime_mut(|runtime| {
            runtime.set_animation_active(!sync_state.active_animations.is_empty())
        });
        return;
    }

    apply_cube_state_to_pool(
        &mut meshes,
        &mut pool,
        &snapshot.state,
        &mut pivots,
        &mut visibilities,
    );
    sync_state.rendered_revision = snapshot.scene_revision;
    sync_state.completed_animation_revision = None;
    sync_state.live_slice_turn = None;
    sync_state.live_slice_animated_stickers.clear();
    with_runtime_mut(|runtime| runtime.set_animation_active(false));
}

fn animate_turn_visuals(
    time: Res<'_, Time>,
    mut commands: Commands<'_, '_>,
    mut meshes: ResMut<'_, Assets<Mesh>>,
    mut pool: ResMut<'_, CubeVisualPool>,
    mut sync_state: ResMut<'_, VisualSyncState>,
    mut pivots: Query<
        '_,
        '_,
        &mut Transform,
        (
            With<TurnAnimationPivot>,
            Without<CubieBodyVisual>,
            Without<StickerVisual>,
        ),
    >,
    mut visibilities: Query<'_, '_, &mut Visibility>,
) {
    if sync_state.active_animations.is_empty() {
        return;
    }

    let delta = time.delta_secs();
    let order = pool.order.expect("pool root implies order");

    let mut completed_animations: Vec<ActiveTurnAnimation> = Vec::new();
    let mut still_active: Vec<ActiveTurnAnimation> = Vec::new();

    for mut animation in sync_state.active_animations.drain(..) {
        if let Ok(mut pivot_transform) = pivots.get_mut(animation.pivot_entity) {
            animation.elapsed_secs = (animation.elapsed_secs + delta).min(animation.duration_secs);
            let progress = if animation.duration_secs <= f32::EPSILON {
                1.0
            } else {
                animation.elapsed_secs / animation.duration_secs
            };
            let eased = ease_in_out_cubic(progress);
            let angle = animation.start_angle_radians
                + ((animation.angle_radians - animation.start_angle_radians) * eased);
            pivot_transform.rotation = Quat::from_axis_angle(animation.axis, angle);

            if progress < 1.0 {
                still_active.push(animation);
            } else {
                completed_animations.push(animation);
            }
        } else {
            completed_animations.push(animation);
        }
    }

    sync_state.active_animations = still_active;

    if completed_animations.is_empty() {
        return;
    }

    for animation in &completed_animations {
        let visual_turn = match animation.completion {
            TurnAnimationCompletion::RuntimeApplied => Some(animation.turn),
            TurnAnimationCompletion::DirectSliceSnap { commit_turn } => commit_turn,
        };
        if let Some(visual_turn) = visual_turn {
            for index in &animation.animated_stickers {
                let rotated = rotate_sticker_visual_for_turn(
                    order,
                    visual_turn,
                    pool.sticker_visual_states[*index],
                );
                pool.sticker_visual_states[*index] = rotated;
            }
        }
    }

    if sync_state.active_animations.is_empty() {
        for animation in &completed_animations {
            if let Ok(mut t) = pivots.get_mut(animation.pivot_entity) {
                t.rotation = Quat::IDENTITY;
            }
        }

        for &entity in &sync_state.temp_pivot_entities {
            commands.entity(entity).despawn();
        }
        sync_state.temp_pivot_entities.clear();

        if let Some(animated_body) = pool.animated_body_entity {
            if let Ok(mut visibility) = visibilities.get_mut(animated_body) {
                *visibility = Visibility::Hidden;
            }
        }
        if let Some(animated_sticker) = pool.animated_sticker_entity {
            if let Ok(mut visibility) = visibilities.get_mut(animated_sticker) {
                *visibility = Visibility::Hidden;
            }
        }

        restore_resting_body_meshes(&mut meshes, &pool, &mut visibilities);
        restore_resting_sticker_meshes(&mut meshes, &pool, &mut visibilities);

        let mut scene_revision = completed_animations[0].scene_revision;
        let mut should_process_queue = false;
        for animation in &completed_animations {
            match animation.completion {
                TurnAnimationCompletion::RuntimeApplied => {
                    should_process_queue = true;
                }
                TurnAnimationCompletion::DirectSliceSnap {
                    commit_turn: Some(turn),
                } => {
                    scene_revision = with_runtime_mut(|runtime| runtime.commit_direct_turn(turn));
                }
                TurnAnimationCompletion::DirectSliceSnap { commit_turn: None } => {}
            }
        }
        sync_state.rendered_revision = scene_revision;
        sync_state.completed_animation_revision = Some(scene_revision);
        with_runtime_mut(|runtime| {
            runtime.set_animation_active(false);
            if should_process_queue {
                runtime.process_queue_head();
            }
        });
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

fn pool_matches_pending_animation_source(
    sync_state: &VisualSyncState,
    pending_scene_revision: u64,
) -> bool {
    sync_state.rendered_revision != 0
        && sync_state.rendered_revision.saturating_add(1) == pending_scene_revision
}

fn cuboid_mesh_template(width: f32, height: f32, depth: f32) -> BodyMeshTemplate {
    let mesh = Mesh::from(Cuboid::new(width, height, depth));
    let positions = mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .and_then(|values| values.as_float3())
        .expect("cuboid mesh should expose position data")
        .to_vec();
    let normals = mesh
        .attribute(Mesh::ATTRIBUTE_NORMAL)
        .and_then(|values| values.as_float3())
        .expect("cuboid mesh should expose normal data")
        .to_vec();
    let indices = match mesh.indices().expect("cuboid mesh should expose indices") {
        Indices::U16(indices) => indices.iter().map(|index| u32::from(*index)).collect(),
        Indices::U32(indices) => indices.clone(),
    };
    BodyMeshTemplate {
        positions,
        normals,
        indices,
    }
}

fn merged_cubie_body_mesh(
    template: &BodyMeshTemplate,
    cubies: &[UVec3],
    order: usize,
    face_span: f32,
) -> Mesh {
    let vertices_per_cubie = template.positions.len();
    let mut positions = Vec::with_capacity(vertices_per_cubie * cubies.len());
    let mut normals = Vec::with_capacity(template.normals.len() * cubies.len());
    let mut indices = Vec::with_capacity(template.indices.len() * cubies.len());

    for (cubie_index, cubie) in cubies.iter().copied().enumerate() {
        let translation = cubie_body_translation(cubie, order, face_span);
        positions.extend(template.positions.iter().map(|position| {
            [
                position[0] + translation.x,
                position[1] + translation.y,
                position[2] + translation.z,
            ]
        }));
        normals.extend(template.normals.iter().copied());
        let base_index = (cubie_index * vertices_per_cubie) as u32;
        indices.extend(template.indices.iter().map(|index| base_index + index));
    }

    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_indices(Indices::U32(indices))
}

fn partition_body_cubies(
    cubie_slots: &[UVec3],
    order: u32,
    animation_turn: Option<TurnCommand>,
) -> (Vec<UVec3>, Vec<UVec3>) {
    let Some(turn) = animation_turn else {
        return (cubie_slots.to_vec(), Vec::new());
    };

    let mut static_cubies = Vec::with_capacity(cubie_slots.len());
    let mut animated_cubies = Vec::new();
    for cubie in cubie_slots.iter().copied() {
        if cubie_matches_turn(order, turn, cubie) {
            animated_cubies.push(cubie);
        } else {
            static_cubies.push(cubie);
        }
    }
    (static_cubies, animated_cubies)
}

fn apply_body_mesh_partition(
    meshes: &mut Assets<Mesh>,
    pool: &CubeVisualPool,
    static_cubies: &[UVec3],
    animated_cubies: &[UVec3],
    visibilities: &mut Query<'_, '_, &mut Visibility>,
) {
    let Some(order) = pool.order.map(|o| o as usize) else {
        return;
    };
    let Some((static_body_mesh, animated_body_mesh)) = pool.body_mesh_handles.as_ref() else {
        return;
    };
    let Some(template) = pool.body_mesh_template.as_ref() else {
        return;
    };

    if let Some(mesh) = meshes.get_mut(static_body_mesh) {
        *mesh = merged_cubie_body_mesh(template, static_cubies, order, CUBE_FACE_SPAN);
    }
    if let Some(mesh) = meshes.get_mut(animated_body_mesh) {
        *mesh = merged_cubie_body_mesh(template, animated_cubies, order, CUBE_FACE_SPAN);
    }

    if let Some(entity) = pool.static_body_entity {
        if let Ok(mut visibility) = visibilities.get_mut(entity) {
            *visibility = Visibility::Visible;
        }
    }
    if let Some(entity) = pool.animated_body_entity {
        if let Ok(mut visibility) = visibilities.get_mut(entity) {
            *visibility = if animated_cubies.is_empty() {
                Visibility::Hidden
            } else {
                Visibility::Visible
            };
        }
    }
}

fn restore_resting_body_meshes(
    meshes: &mut Assets<Mesh>,
    pool: &CubeVisualPool,
    visibilities: &mut Query<'_, '_, &mut Visibility>,
) {
    apply_body_mesh_partition(meshes, pool, &pool.cubie_slots, &[], visibilities);
}

fn merged_sticker_mesh(
    template: &BodyMeshTemplate,
    stickers: &[(StickerVisual, StickerColor)],
    order: usize,
    face_span: f32,
    face_offset: f32,
) -> Mesh {
    let vertices_per_sticker = template.positions.len();
    let mut positions = Vec::with_capacity(vertices_per_sticker * stickers.len());
    let mut normals = Vec::with_capacity(template.normals.len() * stickers.len());
    let mut colors = Vec::with_capacity(vertices_per_sticker * stickers.len());
    let mut indices = Vec::with_capacity(template.indices.len() * stickers.len());

    for (sticker_index, (visual, color)) in stickers.iter().copied().enumerate() {
        let (translation, rotation) = sticker_world_transform(
            visual.face,
            usize::from(visual.row),
            usize::from(visual.col),
            order,
            face_span,
            face_offset,
        );
        positions.extend(template.positions.iter().map(|position| {
            let rotated = rotation * Vec3::new(position[0], position[1], position[2]);
            [
                rotated.x + translation.x,
                rotated.y + translation.y,
                rotated.z + translation.z,
            ]
        }));
        normals.extend(template.normals.iter().map(|normal| {
            let rotated = rotation * Vec3::new(normal[0], normal[1], normal[2]);
            [rotated.x, rotated.y, rotated.z]
        }));
        colors.extend(std::iter::repeat_n(
            color_for_sticker(color).to_linear().to_f32_array(),
            vertices_per_sticker,
        ));
        let base_index = (sticker_index * vertices_per_sticker) as u32;
        indices.extend(template.indices.iter().map(|index| base_index + index));
    }

    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(indices))
}

fn partition_sticker_visuals(
    sticker_visual_states: &[StickerVisual],
    sticker_colors: &[StickerColor],
    order: u32,
    animation_turn: Option<TurnCommand>,
) -> (
    Vec<(StickerVisual, StickerColor)>,
    Vec<(StickerVisual, StickerColor)>,
    Vec<usize>,
) {
    let mut static_stickers = Vec::with_capacity(sticker_visual_states.len());
    let mut animated_stickers = Vec::new();
    let mut animated_indices = Vec::new();

    for (index, (visual, color)) in sticker_visual_states
        .iter()
        .copied()
        .zip(sticker_colors.iter().copied())
        .enumerate()
    {
        if animation_turn.is_some_and(|turn| cubie_matches_turn(order, turn, visual.cubie)) {
            animated_stickers.push((visual, color));
            animated_indices.push(index);
        } else {
            static_stickers.push((visual, color));
        }
    }

    (static_stickers, animated_stickers, animated_indices)
}

fn apply_sticker_mesh_partition(
    meshes: &mut Assets<Mesh>,
    pool: &CubeVisualPool,
    static_stickers: &[(StickerVisual, StickerColor)],
    animated_stickers: &[(StickerVisual, StickerColor)],
    visibilities: &mut Query<'_, '_, &mut Visibility>,
) {
    let Some(order) = pool.order.map(|o| o as usize) else {
        return;
    };
    let Some((static_sticker_mesh, animated_sticker_mesh)) = pool.sticker_mesh_handles.as_ref()
    else {
        return;
    };
    let Some(template) = pool.sticker_mesh_template.as_ref() else {
        return;
    };
    let face_offset = cube_face_offset(order as u32);

    if let Some(mesh) = meshes.get_mut(static_sticker_mesh) {
        *mesh = merged_sticker_mesh(
            template,
            static_stickers,
            order,
            CUBE_FACE_SPAN,
            face_offset,
        );
    }
    if let Some(mesh) = meshes.get_mut(animated_sticker_mesh) {
        *mesh = merged_sticker_mesh(
            template,
            animated_stickers,
            order,
            CUBE_FACE_SPAN,
            face_offset,
        );
    }

    if let Some(entity) = pool.static_sticker_entity {
        if let Ok(mut visibility) = visibilities.get_mut(entity) {
            *visibility = Visibility::Visible;
        }
    }
    if let Some(entity) = pool.animated_sticker_entity {
        if let Ok(mut visibility) = visibilities.get_mut(entity) {
            *visibility = if animated_stickers.is_empty() {
                Visibility::Hidden
            } else {
                Visibility::Visible
            };
        }
    }
}

fn restore_resting_sticker_meshes(
    meshes: &mut Assets<Mesh>,
    pool: &CubeVisualPool,
    visibilities: &mut Query<'_, '_, &mut Visibility>,
) {
    let (static_stickers, animated_stickers, _) = partition_sticker_visuals(
        &pool.sticker_visual_states,
        &pool.sticker_colors,
        pool.order.unwrap_or_default(),
        None,
    );
    apply_sticker_mesh_partition(
        meshes,
        pool,
        &static_stickers,
        &animated_stickers,
        visibilities,
    );
}

fn cube_visual_pool_needs_rebuild(
    pool: &CubeVisualPool,
    order: u32,
    existing_visual_roots: &Query<'_, '_, Entity, With<CubeVisualRoot>>,
) -> bool {
    let expected_stickers = Face::ALL.len() * order as usize * order as usize;
    let expected_cubies = surface_cubie_count(order as usize);
    pool.order != Some(order)
        || pool.root_entity.is_none()
        || pool.pivot_entity.is_none()
        || pool.static_body_entity.is_none()
        || pool.animated_body_entity.is_none()
        || pool.static_sticker_entity.is_none()
        || pool.animated_sticker_entity.is_none()
        || pool
            .root_entity
            .is_some_and(|entity| existing_visual_roots.get(entity).is_err())
        || pool.sticker_visual_states.len() != expected_stickers
        || pool.sticker_colors.len() != expected_stickers
        || pool.sticker_slots.len() != expected_stickers
        || pool.cubie_slots.len() != expected_cubies
        || pool.body_mesh_handles.is_none()
        || pool.body_mesh_template.is_none()
        || pool.sticker_mesh_handles.is_none()
        || pool.sticker_mesh_template.is_none()
}

fn spawn_cube_visual_pool(
    commands: &mut Commands<'_, '_>,
    meshes: &mut ResMut<'_, Assets<Mesh>>,
    materials: &mut ResMut<'_, Assets<StandardMaterial>>,
    state: &CubeState,
    animation_turns: Option<Vec<TurnCommand>>,
    scene_revision: u64,
) -> (CubeVisualPool, Vec<ActiveTurnAnimation>) {
    let first_animation_turn = animation_turns.as_ref().and_then(|v| v.first().copied());
    let order = state.order.get() as usize;
    let face_span = CUBE_FACE_SPAN;
    let step = face_span / order as f32;
    let sticker_size = step * 0.84;
    let sticker_depth = sticker_size * 0.113_f32;
    let cubie_body_size = step * 0.92;
    let face_offset = cube_face_offset(state.order.get());
    let sticker_material = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        metallic: 0.06,
        perceptual_roughness: 0.21,
        ..default()
    });
    let cubie_slots = surface_cubies(order);
    let body_mesh_template =
        cuboid_mesh_template(cubie_body_size, cubie_body_size, cubie_body_size);
    let sticker_mesh_template = cuboid_mesh_template(sticker_size, sticker_size, sticker_depth);
    let (static_body_cubies, animated_body_cubies) =
        partition_body_cubies(&cubie_slots, state.order.get(), first_animation_turn);
    let static_body_mesh = meshes.add(merged_cubie_body_mesh(
        &body_mesh_template,
        &static_body_cubies,
        order,
        face_span,
    ));
    let animated_body_mesh_seed = if animated_body_cubies.is_empty() {
        cubie_slots.as_slice()
    } else {
        animated_body_cubies.as_slice()
    };
    let animated_body_mesh = meshes.add(merged_cubie_body_mesh(
        &body_mesh_template,
        animated_body_mesh_seed,
        order,
        face_span,
    ));

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

    let shell_material = materials.add(StandardMaterial {
        base_color: Color::srgb_u8(17, 21, 29),
        metallic: 0.18,
        perceptual_roughness: 0.58,
        reflectance: 0.3,
        ..default()
    });

    let sticker_slots = (0..state.stickers.len())
        .map(|index| sticker_slot_from_index(index, order))
        .collect::<Vec<_>>();
    let sticker_visual_states = sticker_slots
        .iter()
        .copied()
        .map(|slot| StickerVisual {
            face: slot.face,
            row: slot.row as u8,
            col: slot.col as u8,
            cubie: slot.cubie,
        })
        .collect::<Vec<_>>();
    let sticker_colors = state.stickers.to_vec();
    let (static_stickers, animated_stickers, animated_sticker_indices) = partition_sticker_visuals(
        &sticker_visual_states,
        &sticker_colors,
        state.order.get(),
        first_animation_turn,
    );
    let static_sticker_mesh = meshes.add(merged_sticker_mesh(
        &sticker_mesh_template,
        &static_stickers,
        order,
        face_span,
        face_offset,
    ));
    let animated_sticker_mesh_seed = if animated_stickers.is_empty() {
        static_stickers.as_slice()
    } else {
        animated_stickers.as_slice()
    };
    let animated_sticker_mesh = meshes.add(merged_sticker_mesh(
        &sticker_mesh_template,
        animated_sticker_mesh_seed,
        order,
        face_span,
        face_offset,
    ));
    let static_body_entity = commands
        .spawn((
            CubeVisual,
            Mesh3d(static_body_mesh.clone()),
            MeshMaterial3d(shell_material.clone()),
            Transform::default(),
            ChildOf(root),
        ))
        .id();
    let animated_body_entity = commands
        .spawn((
            CubeVisual,
            Mesh3d(animated_body_mesh.clone()),
            MeshMaterial3d(shell_material.clone()),
            Transform::default(),
            if animated_body_cubies.is_empty() {
                Visibility::Hidden
            } else {
                Visibility::Visible
            },
            ChildOf(pivot),
        ))
        .id();
    let static_sticker_entity = commands
        .spawn((
            CubeVisual,
            Mesh3d(static_sticker_mesh.clone()),
            MeshMaterial3d(sticker_material.clone()),
            Transform::default(),
            ChildOf(root),
        ))
        .id();
    let animated_sticker_entity = commands
        .spawn((
            CubeVisual,
            Mesh3d(animated_sticker_mesh.clone()),
            MeshMaterial3d(sticker_material),
            Transform::default(),
            if animated_stickers.is_empty() {
                Visibility::Hidden
            } else {
                Visibility::Visible
            },
            ChildOf(pivot),
        ))
        .id();
    let pool = CubeVisualPool {
        order: Some(state.order.get()),
        root_entity: Some(root),
        pivot_entity: Some(pivot),
        static_body_entity: Some(static_body_entity),
        animated_body_entity: Some(animated_body_entity),
        static_sticker_entity: Some(static_sticker_entity),
        animated_sticker_entity: Some(animated_sticker_entity),
        sticker_visual_states,
        sticker_colors,
        body_mesh_handles: Some((static_body_mesh, animated_body_mesh)),
        body_mesh_template: Some(body_mesh_template),
        sticker_mesh_handles: Some((static_sticker_mesh, animated_sticker_mesh)),
        sticker_mesh_template: Some(sticker_mesh_template),
        cubie_slots: cubie_slots.clone(),
        sticker_slots: sticker_slots.clone(),
    };

    let active_animations: Vec<ActiveTurnAnimation> = first_animation_turn
        .map(|turn| ActiveTurnAnimation {
            scene_revision,
            pivot_entity: pivot,
            turn,
            animated_stickers: animated_sticker_indices,
            start_angle_radians: 0.0,
            angle_radians: turn_rotation_angle(turn),
            axis: turn_rotation_axis(turn.face),
            elapsed_secs: 0.0,
            duration_secs: turn_animation_duration_secs(turn),
            completion: TurnAnimationCompletion::RuntimeApplied,
        })
        .into_iter()
        .collect();
    (pool, active_animations)
}

fn apply_cube_state_to_pool(
    meshes: &mut ResMut<'_, Assets<Mesh>>,
    pool: &mut CubeVisualPool,
    state: &CubeState,
    pivots: &mut Query<
        '_,
        '_,
        &mut Transform,
        (
            With<TurnAnimationPivot>,
            Without<CubieBodyVisual>,
            Without<StickerVisual>,
        ),
    >,
    visibilities: &mut Query<'_, '_, &mut Visibility>,
) {
    let Some(pivot_entity) = pool.pivot_entity else {
        return;
    };

    if let Ok(mut pivot_transform) = pivots.get_mut(pivot_entity) {
        *pivot_transform = Transform::default();
    }
    restore_resting_body_meshes(meshes, pool, visibilities);
    for (visual_state, slot) in pool
        .sticker_visual_states
        .iter_mut()
        .zip(pool.sticker_slots.iter().copied())
    {
        *visual_state = StickerVisual {
            face: slot.face,
            row: slot.row as u8,
            col: slot.col as u8,
            cubie: slot.cubie,
        };
    }
    pool.sticker_colors.clear();
    pool.sticker_colors.extend(state.stickers.iter().copied());
    restore_resting_sticker_meshes(meshes, pool, visibilities);
}

fn prepare_single_turn_visuals(
    meshes: &mut ResMut<'_, Assets<Mesh>>,
    pool: &CubeVisualPool,
    order: u32,
    turn: TurnCommand,
    pivots: &mut Query<
        '_,
        '_,
        &mut Transform,
        (
            With<TurnAnimationPivot>,
            Without<CubieBodyVisual>,
            Without<StickerVisual>,
        ),
    >,
    visibilities: &mut Query<'_, '_, &mut Visibility>,
    reset_pivot: bool,
) -> Option<PreparedTurnVisuals> {
    let pivot_entity = pool.pivot_entity?;
    if reset_pivot {
        if let Ok(mut pivot_transform) = pivots.get_mut(pivot_entity) {
            *pivot_transform = Transform::default();
        }
    }

    let (static_body_cubies, animated_body_cubies) =
        partition_body_cubies(&pool.cubie_slots, order, Some(turn));
    apply_body_mesh_partition(
        meshes,
        pool,
        &static_body_cubies,
        &animated_body_cubies,
        visibilities,
    );
    let (static_stickers, animated_stickers, animated_sticker_indices) = partition_sticker_visuals(
        &pool.sticker_visual_states,
        &pool.sticker_colors,
        order,
        Some(turn),
    );
    apply_sticker_mesh_partition(
        meshes,
        pool,
        &static_stickers,
        &animated_stickers,
        visibilities,
    );

    if animated_body_cubies.is_empty() && animated_sticker_indices.is_empty() {
        return None;
    }

    Some(PreparedTurnVisuals {
        pivot_entity,
        animated_stickers: animated_sticker_indices,
    })
}

fn begin_turn_batch_animation(
    commands: &mut Commands<'_, '_>,
    meshes: &mut ResMut<'_, Assets<Mesh>>,
    materials: &mut ResMut<'_, Assets<StandardMaterial>>,
    pool: &CubeVisualPool,
    order: u32,
    turns: &[TurnCommand],
    scene_revision: u64,
    pivots: &mut Query<
        '_,
        '_,
        &mut Transform,
        (
            With<TurnAnimationPivot>,
            Without<CubieBodyVisual>,
            Without<StickerVisual>,
        ),
    >,
    visibilities: &mut Query<'_, '_, &mut Visibility>,
    sync_state: &mut VisualSyncState,
) {
    if turns.is_empty() {
        sync_state.active_animations.clear();
        return;
    }

    sync_state.live_slice_turn = None;
    sync_state.live_slice_animated_stickers.clear();

    let turns = RuntimeBridge::merge_batch_turns(order, turns);

    if turns.is_empty() {
        sync_state.active_animations.clear();
        return;
    }

    if turns.len() == 1 {
        let turn = turns[0];
        let Some(prepared) =
            prepare_single_turn_visuals(meshes, pool, order, turn, pivots, visibilities, true)
        else {
            sync_state.active_animations.clear();
            return;
        };

        sync_state.active_animations = vec![ActiveTurnAnimation {
            scene_revision,
            pivot_entity: prepared.pivot_entity,
            turn,
            animated_stickers: prepared.animated_stickers,
            start_angle_radians: 0.0,
            angle_radians: turn_rotation_angle(turn),
            axis: turn_rotation_axis(turn.face),
            elapsed_secs: 0.0,
            duration_secs: turn_animation_duration_secs(turn),
            completion: TurnAnimationCompletion::RuntimeApplied,
        }];
        return;
    }

    let root_entity = pool.root_entity.expect("pool has root");
    let face_span = CUBE_FACE_SPAN;
    let body_template = pool
        .body_mesh_template
        .as_ref()
        .expect("pool has body template");
    let sticker_template = pool
        .sticker_mesh_template
        .as_ref()
        .expect("pool has sticker template");
    let face_offset = cube_face_offset(order);

    if let Some(animated_body) = pool.animated_body_entity {
        if let Ok(mut visibility) = visibilities.get_mut(animated_body) {
            *visibility = Visibility::Hidden;
        }
    }
    if let Some(animated_sticker) = pool.animated_sticker_entity {
        if let Ok(mut visibility) = visibilities.get_mut(animated_sticker) {
            *visibility = Visibility::Hidden;
        }
    }

    let mut animated_cubies_set: Vec<UVec3> = Vec::new();
    for &turn in &turns {
        for cubie in &pool.cubie_slots {
            if cubie_matches_turn(order, turn, *cubie) {
                if !animated_cubies_set.contains(cubie) {
                    animated_cubies_set.push(*cubie);
                }
            }
        }
    }

    let static_body_cubies: Vec<UVec3> = pool
        .cubie_slots
        .iter()
        .filter(|c| !animated_cubies_set.contains(c))
        .copied()
        .collect();

    let static_sticker_data: Vec<(StickerVisual, StickerColor)> = pool
        .sticker_visual_states
        .iter()
        .zip(pool.sticker_colors.iter())
        .filter(|(v, _)| !animated_cubies_set.contains(&v.cubie))
        .map(|(v, c)| (*v, *c))
        .collect();

    let static_body_mesh_data = merged_cubie_body_mesh(
        body_template,
        &static_body_cubies,
        order as usize,
        face_span,
    );
    let static_sticker_mesh_data = merged_sticker_mesh(
        sticker_template,
        &static_sticker_data,
        order as usize,
        face_span,
        face_offset,
    );

    if let Some((body_handle, _)) = pool.body_mesh_handles.as_ref() {
        if let Some(body_mesh) = meshes.get_mut(body_handle) {
            *body_mesh = static_body_mesh_data;
        }
    }
    if let Some((sticker_handle, _)) = pool.sticker_mesh_handles.as_ref() {
        if let Some(sticker_mesh) = meshes.get_mut(sticker_handle) {
            *sticker_mesh = static_sticker_mesh_data;
        }
    }

    let shell_material = materials.add(StandardMaterial {
        base_color: Color::srgb_u8(17, 21, 29),
        metallic: 0.18,
        perceptual_roughness: 0.58,
        reflectance: 0.3,
        ..default()
    });
    let sticker_material = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        metallic: 0.06,
        perceptual_roughness: 0.21,
        ..default()
    });

    let all_sticker_pairs: Vec<(StickerVisual, StickerColor)> = pool
        .sticker_visual_states
        .iter()
        .zip(pool.sticker_colors.iter())
        .map(|(v, c)| (*v, *c))
        .collect();

    let mut animations: Vec<ActiveTurnAnimation> = Vec::new();
    let mut temp_entities: Vec<Entity> = Vec::new();

    for &turn in &turns {
        let (_, animated_body_cubies) = partition_body_cubies(&pool.cubie_slots, order, Some(turn));
        let animated_body_mesh_data = if animated_body_cubies.is_empty() {
            merged_cubie_body_mesh(body_template, &pool.cubie_slots, order as usize, face_span)
        } else {
            merged_cubie_body_mesh(
                body_template,
                &animated_body_cubies,
                order as usize,
                face_span,
            )
        };
        let animated_body_handle = meshes.add(animated_body_mesh_data);

        let (_, animated_sticker_data, animated_sticker_indices) = partition_sticker_visuals(
            &pool.sticker_visual_states,
            &pool.sticker_colors,
            order,
            Some(turn),
        );
        let animated_sticker_mesh_data = if animated_sticker_data.is_empty() {
            merged_sticker_mesh(
                sticker_template,
                &all_sticker_pairs,
                order as usize,
                face_span,
                face_offset,
            )
        } else {
            merged_sticker_mesh(
                sticker_template,
                &animated_sticker_data,
                order as usize,
                face_span,
                face_offset,
            )
        };
        let animated_sticker_handle = meshes.add(animated_sticker_mesh_data);

        let pivot_entity = commands
            .spawn((
                CubeVisual,
                TurnAnimationPivot,
                Name::new(format!("batch-anim-pivot-{}", animations.len())),
                Transform::default(),
                GlobalTransform::default(),
                Visibility::Visible,
                InheritedVisibility::default(),
                ViewVisibility::default(),
                ChildOf(root_entity),
            ))
            .id();

        let vis = if animated_body_cubies.is_empty() {
            Visibility::Hidden
        } else {
            Visibility::Visible
        };
        commands.spawn((
            CubeVisual,
            Mesh3d(animated_body_handle.clone()),
            MeshMaterial3d(shell_material.clone()),
            Transform::default(),
            vis,
            ChildOf(pivot_entity),
        ));

        let sticker_vis = if animated_sticker_indices.is_empty() {
            Visibility::Hidden
        } else {
            Visibility::Visible
        };
        commands.spawn((
            CubeVisual,
            Mesh3d(animated_sticker_handle.clone()),
            MeshMaterial3d(sticker_material.clone()),
            Transform::default(),
            sticker_vis,
            ChildOf(pivot_entity),
        ));

        temp_entities.push(pivot_entity);

        if animated_sticker_indices.is_empty() {
            continue;
        }

        animations.push(ActiveTurnAnimation {
            scene_revision,
            pivot_entity,
            turn,
            animated_stickers: animated_sticker_indices,
            start_angle_radians: 0.0,
            angle_radians: turn_rotation_angle(turn),
            axis: turn_rotation_axis(turn.face),
            elapsed_secs: 0.0,
            duration_secs: turn_animation_duration_secs(turn),
            completion: TurnAnimationCompletion::RuntimeApplied,
        });
    }

    sync_state.active_animations = animations;
    sync_state.temp_pivot_entities = temp_entities;
}

fn set_turn_pivot_angle(
    pivots: &mut Query<
        '_,
        '_,
        &mut Transform,
        (
            With<TurnAnimationPivot>,
            Without<CubieBodyVisual>,
            Without<StickerVisual>,
        ),
    >,
    pivot_entity: Entity,
    turn: TurnCommand,
    angle_radians: f32,
) {
    if let Ok(mut pivot_transform) = pivots.get_mut(pivot_entity) {
        pivot_transform.rotation =
            Quat::from_axis_angle(turn_rotation_axis(turn.face), angle_radians);
    }
}

fn clear_live_slice_visuals(
    meshes: &mut ResMut<'_, Assets<Mesh>>,
    pool: &CubeVisualPool,
    sync_state: &mut VisualSyncState,
    pivots: &mut Query<
        '_,
        '_,
        &mut Transform,
        (
            With<TurnAnimationPivot>,
            Without<CubieBodyVisual>,
            Without<StickerVisual>,
        ),
    >,
    visibilities: &mut Query<'_, '_, &mut Visibility>,
) {
    if let Some(pivot_entity) = pool.pivot_entity {
        if let Ok(mut pivot_transform) = pivots.get_mut(pivot_entity) {
            *pivot_transform = Transform::default();
        }
    }
    restore_resting_body_meshes(meshes, pool, visibilities);
    restore_resting_sticker_meshes(meshes, pool, visibilities);
    sync_state.live_slice_turn = None;
    sync_state.live_slice_animated_stickers.clear();
}

fn ensure_live_slice_visuals(
    meshes: &mut ResMut<'_, Assets<Mesh>>,
    pool: &CubeVisualPool,
    sync_state: &mut VisualSyncState,
    order: u32,
    turn: TurnCommand,
    pivots: &mut Query<
        '_,
        '_,
        &mut Transform,
        (
            With<TurnAnimationPivot>,
            Without<CubieBodyVisual>,
            Without<StickerVisual>,
        ),
    >,
    visibilities: &mut Query<'_, '_, &mut Visibility>,
) -> Option<PreparedTurnVisuals> {
    if sync_state.live_slice_turn == Some(turn) {
        return Some(PreparedTurnVisuals {
            pivot_entity: pool.pivot_entity?,
            animated_stickers: sync_state.live_slice_animated_stickers.clone(),
        });
    }

    let prepared =
        prepare_single_turn_visuals(meshes, pool, order, turn, pivots, visibilities, false)?;
    sync_state.live_slice_turn = Some(turn);
    sync_state.live_slice_animated_stickers = prepared.animated_stickers.clone();
    Some(prepared)
}

fn direct_slice_drag_visuals(
    mut meshes: ResMut<'_, Assets<Mesh>>,
    pool: Res<'_, CubeVisualPool>,
    mut sync_state: ResMut<'_, VisualSyncState>,
    mut direct_turn_input: ResMut<'_, DirectTurnInputState>,
    mut pivots: Query<
        '_,
        '_,
        &mut Transform,
        (
            With<TurnAnimationPivot>,
            Without<CubieBodyVisual>,
            Without<StickerVisual>,
        ),
    >,
    mut visibilities: Query<'_, '_, &mut Visibility>,
) {
    if !sync_state.active_animations.is_empty() {
        return;
    }

    let Some(order) = pool.order else {
        return;
    };

    if let Some(request) = direct_turn_input.pending_slice_snap {
        let Some(prepared) = ensure_live_slice_visuals(
            &mut meshes,
            &pool,
            &mut sync_state,
            order,
            request.turn,
            &mut pivots,
            &mut visibilities,
        ) else {
            return;
        };

        set_turn_pivot_angle(
            &mut pivots,
            prepared.pivot_entity,
            request.turn,
            request.start_angle_radians,
        );
        sync_state.active_animations = vec![ActiveTurnAnimation {
            scene_revision: with_runtime(|runtime| runtime.scene_revision),
            pivot_entity: prepared.pivot_entity,
            turn: request.turn,
            animated_stickers: prepared.animated_stickers,
            start_angle_radians: request.start_angle_radians,
            angle_radians: request.target_angle_radians,
            axis: turn_rotation_axis(request.turn.face),
            elapsed_secs: 0.0,
            duration_secs: slice_snap_duration_secs(
                request.start_angle_radians,
                request.target_angle_radians,
            ),
            completion: TurnAnimationCompletion::DirectSliceSnap {
                commit_turn: request.commit_turn,
            },
        }];
        sync_state.live_slice_turn = None;
        sync_state.live_slice_animated_stickers.clear();
        direct_turn_input.pending_slice_snap = None;
        with_runtime_mut(|runtime| runtime.set_animation_active(true));
        return;
    }

    if let Some(active_drag) = direct_turn_input.active_slice_drag {
        let Some(prepared) = ensure_live_slice_visuals(
            &mut meshes,
            &pool,
            &mut sync_state,
            order,
            active_drag.turn,
            &mut pivots,
            &mut visibilities,
        ) else {
            return;
        };
        set_turn_pivot_angle(
            &mut pivots,
            prepared.pivot_entity,
            active_drag.turn,
            active_drag.angle_radians,
        );
    } else if sync_state.live_slice_turn.is_some() {
        clear_live_slice_visuals(
            &mut meshes,
            &pool,
            &mut sync_state,
            &mut pivots,
            &mut visibilities,
        );
    }
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
        if direct_turn_input
            .active_slice_drag
            .is_some_and(|drag| drag.input == SliceDragInput::Mouse)
        {
            let _ = finish_active_slice_drag(&mut direct_turn_input, SliceDragInput::Mouse, true);
        }
        direct_turn_input.mouse_candidate = None;
        orbit.mouse_drag_button = None;
        orbit.mouse_start_position = None;
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
            orbit.mouse_start_position = None;
            orbit.snap_target = None;
            orbit.auto_spin = false;
        }

        if buttons.just_pressed(MouseButton::Right) {
            direct_turn_input.mouse_candidate = None;
            orbit.mouse_drag_button = Some(MouseButton::Right);
            orbit.mouse_start_yaw = orbit.yaw;
            orbit.mouse_start_pitch = orbit.pitch;
            orbit.mouse_start_position = cursor_position;
            orbit.snap_target = None;
            orbit.auto_spin = false;
        }

        if buttons.pressed(MouseButton::Left) {
            let mut updated_active_drag = false;
            if let Some(position) = cursor_position {
                if let Some(active_drag) =
                    active_slice_drag_for_input(&mut direct_turn_input, SliceDragInput::Mouse)
                {
                    active_drag.update_angle(position);
                    updated_active_drag = true;
                }
            }

            if !updated_active_drag {
                if let Some(candidate) = direct_turn_input.mouse_candidate.as_mut() {
                    candidate.max_distance = candidate.max_distance.max(mouse_delta.length());
                    if let Some(position) = cursor_position {
                        candidate.max_distance = candidate
                            .max_distance
                            .max(position.distance(candidate.start_position));
                    }

                    let candidate_snapshot = *candidate;
                    let mut active_slice_drag = None;
                    if let (
                        Some(position),
                        Some(sticker_candidate),
                        Some((camera, camera_transform)),
                    ) = (
                        cursor_position,
                        candidate_snapshot.sticker_candidate,
                        camera_context,
                    ) {
                        if candidate_snapshot.max_distance > POINTER_TAP_MAX_DRAG_PX {
                            active_slice_drag = build_active_slice_drag(
                                camera,
                                camera_transform,
                                order,
                                SliceDragInput::Mouse,
                                sticker_candidate,
                                candidate_snapshot.start_position,
                                position,
                            );
                        }
                    }

                    if let Some(active_slice_drag) = active_slice_drag {
                        direct_turn_input.mouse_candidate = None;
                        direct_turn_input.active_slice_drag = Some(active_slice_drag);
                        orbit.snap_target = None;
                        orbit.auto_spin = false;
                    } else if should_begin_mouse_orbit(
                        MouseButton::Left,
                        candidate_snapshot.max_distance,
                        candidate_snapshot.sticker_candidate.is_some(),
                    ) {
                        direct_turn_input.mouse_candidate = None;
                        orbit.mouse_drag_button = Some(MouseButton::Left);
                        orbit.mouse_start_yaw = orbit.yaw;
                        orbit.mouse_start_pitch = orbit.pitch;
                        orbit.mouse_start_position = cursor_position;
                        orbit.snap_target = None;
                    }
                }
            }
        }

        if orbit
            .mouse_drag_button
            .is_some_and(|button| buttons.pressed(button))
        {
            orbit.snap_target = None;
            orbit.auto_spin = false;
            if let (Some(start_pos), Some(current_pos)) =
                (orbit.mouse_start_position, cursor_position)
            {
                let displacement = current_pos - start_pos;
                if displacement.length_squared() > 0.0 {
                    orbit.yaw = orbit.mouse_start_yaw + displacement.x * 0.008;
                    orbit.pitch =
                        (orbit.mouse_start_pitch + displacement.y * 0.006).clamp(-1.15, 1.15);
                }
            }
        }

        if buttons.just_released(MouseButton::Left) {
            if finish_active_slice_drag(&mut direct_turn_input, SliceDragInput::Mouse, false) {
                direct_turn_input.mouse_candidate = None;
            } else if orbit.mouse_drag_button == Some(MouseButton::Left) {
                orbit.mouse_drag_button = None;
                orbit.mouse_start_position = None;
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
            orbit.mouse_start_position = None;
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

    if let Some(ActiveSliceDrag {
        input: SliceDragInput::Touch(active_touch_id),
        ..
    }) = direct_turn_input.active_slice_drag
    {
        if let Some(released_touch) = touch_frame
            .released
            .iter()
            .find(|touch| touch.id == active_touch_id)
        {
            let _ = finish_active_slice_drag(
                &mut direct_turn_input,
                SliceDragInput::Touch(active_touch_id),
                released_touch.canceled,
            );
            publish_touch_diagnostic(
                if released_touch.canceled {
                    "slice-cancel"
                } else {
                    "slice-release"
                },
                primary_window,
                touch_space,
                released_touch.raw_position,
                released_touch.position,
                cursor_position,
                Some(true),
            );
        }
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
            if direct_turn_input
                .active_slice_drag
                .is_some_and(|drag| matches!(drag.input, SliceDragInput::Touch(_)))
            {
                let _ = cancel_active_slice_drag(&mut direct_turn_input);
            }
            clear_touch_orbit_state(&mut orbit);
        }
        [touch] => {
            orbit.auto_spin = false;
            orbit.snap_target = None;
            let emulate_two_finger = should_emulate_two_finger_touch(shift_pressed, 1);

            let mut updated_active_slice_drag = false;
            if let Some(active_drag) =
                active_slice_drag_for_input(&mut direct_turn_input, SliceDragInput::Touch(touch.id))
            {
                active_drag.update_angle(touch.position);
                updated_active_slice_drag = true;
            }

            if updated_active_slice_drag {
                direct_turn_input.touch_candidate = None;
                clear_touch_orbit_state(&mut orbit);
            } else if emulate_two_finger {
                let _ = cancel_active_slice_drag(&mut direct_turn_input);
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
                let mut active_slice_drag = None;

                if let Some(candidate) = direct_turn_input
                    .touch_candidate
                    .as_mut()
                    .filter(|candidate| candidate.id == touch.id)
                {
                    candidate.max_distance = candidate
                        .max_distance
                        .max(touch.position.distance(candidate.start_position));

                    let candidate_snapshot = *candidate;
                    if let (Some(sticker_candidate), Some((camera, camera_transform))) =
                        (candidate_snapshot.sticker_candidate, camera_context)
                    {
                        if candidate_snapshot.max_distance > POINTER_TAP_MAX_DRAG_PX {
                            active_slice_drag = build_active_slice_drag(
                                camera,
                                camera_transform,
                                order,
                                SliceDragInput::Touch(touch.id),
                                sticker_candidate,
                                candidate_snapshot.start_position,
                                touch.position,
                            );
                        }
                    }

                    should_begin_touch_orbit = should_begin_mouse_orbit(
                        MouseButton::Left,
                        candidate.max_distance,
                        candidate.sticker_candidate.is_some(),
                    );
                }

                if let Some(active_slice_drag) = active_slice_drag {
                    direct_turn_input.touch_candidate = None;
                    direct_turn_input.active_slice_drag = Some(active_slice_drag);
                    clear_touch_orbit_state(&mut orbit);
                    publish_touch_diagnostic(
                        "slice",
                        primary_window,
                        touch_space,
                        touch.raw_position,
                        touch.position,
                        cursor_position,
                        Some(true),
                    );
                } else if should_begin_touch_orbit {
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
                        orbit.pitch =
                            (orbit.touch_start_pitch + displacement.y * 0.006).clamp(-1.15, 1.15);
                    }
                }
            }
        }
        [first, second, ..] => {
            orbit.auto_spin = false;
            orbit.snap_target = None;
            let _ = cancel_active_slice_drag(&mut direct_turn_input);
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
                    orbit.pitch =
                        (orbit.touch_start_pitch + displacement.y * 0.0045).clamp(-1.15, 1.15);
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
    if !sync_state.active_animations.is_empty() {
        return;
    }

    if direct_turn_input.active_slice_drag.is_some()
        || direct_turn_input.pending_slice_snap.is_some()
    {
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
    order: u32,
    pointer_position: Vec2,
) -> Option<ScreenStickerCandidate> {
    let surface_hit = cube_surface_hit(camera, camera_transform, order, pointer_position)?;
    surface_hit_candidate(camera, camera_transform, order, surface_hit)
}

fn slice_turn_from_sticker_drag(
    camera: &Camera,
    camera_transform: &GlobalTransform,
    order: u32,
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
    order: u32,
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
    order: u32,
    surface_hit: CubeSurfaceHit,
) -> Option<ScreenStickerCandidate> {
    let order_usize = order as usize;
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

fn virtual_cube_half_extent(order: u32) -> f32 {
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

fn slice_start_layer(face: Face, cubie: UVec3, order: u32) -> u32 {
    let max = order.saturating_sub(1);
    match face {
        Face::Up => max.saturating_sub(cubie.y),
        Face::Right => max.saturating_sub(cubie.x),
        Face::Front => max.saturating_sub(cubie.z),
        Face::Down => cubie.y,
        Face::Left => cubie.x,
        Face::Back => cubie.z,
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

fn cube_face_offset(order: u32) -> f32 {
    let step = CUBE_FACE_SPAN / order.max(1) as f32;
    let cube_size = CUBE_FACE_SPAN + (step * 0.12);
    cube_size / 2.0 + 0.001
}

fn surface_cubie_count(order: usize) -> usize {
    order.pow(3) - order.saturating_sub(2).pow(3)
}

fn surface_cubies(order: usize) -> Vec<UVec3> {
    let mut cubies = Vec::with_capacity(surface_cubie_count(order));
    let max = order.saturating_sub(1);
    for x in 0..order {
        for y in 0..order {
            for z in 0..order {
                if x != 0 && x != max && y != 0 && y != max && z != 0 && z != max {
                    continue;
                }

                cubies.push(UVec3::new(x as u32, y as u32, z as u32));
            }
        }
    }
    cubies
}

fn sticker_slot_from_index(index: usize, order: usize) -> StickerSlotSpec {
    let face_index = index / (order * order);
    let within_face = index % (order * order);
    let row = within_face / order;
    let col = within_face % order;
    let face = Face::ALL[face_index];
    StickerSlotSpec {
        face,
        row,
        col,
        cubie: sticker_cubie_coord(face, row, col, order),
    }
}

fn sticker_row_col(face: Face, cubie: UVec3, order: usize) -> (usize, usize) {
    let max = order.saturating_sub(1);
    match face {
        Face::Up => (cubie.z as usize, cubie.x as usize),
        Face::Right => (max - cubie.y as usize, max - cubie.z as usize),
        Face::Front => (max - cubie.y as usize, cubie.x as usize),
        Face::Down => (max - cubie.z as usize, cubie.x as usize),
        Face::Left => (max - cubie.y as usize, cubie.z as usize),
        Face::Back => (max - cubie.y as usize, max - cubie.x as usize),
    }
}

fn rotate_positive_face_step(face: Face, point: IVec3) -> IVec3 {
    match face {
        Face::Front => IVec3::new(-point.y, point.x, point.z),
        Face::Back => IVec3::new(point.y, -point.x, point.z),
        Face::Right => IVec3::new(point.x, -point.z, point.y),
        Face::Left => IVec3::new(point.x, point.z, -point.y),
        Face::Up => IVec3::new(point.z, point.y, -point.x),
        Face::Down => IVec3::new(-point.z, point.y, point.x),
    }
}

fn face_axis_vector(face: Face) -> IVec3 {
    match face {
        Face::Up => IVec3::Y,
        Face::Right => IVec3::X,
        Face::Front => IVec3::Z,
        Face::Down => -IVec3::Y,
        Face::Left => -IVec3::X,
        Face::Back => -IVec3::Z,
    }
}

fn axis_face_ivec(axis: IVec3) -> Face {
    match (axis.x, axis.y, axis.z) {
        (1, 0, 0) => Face::Right,
        (-1, 0, 0) => Face::Left,
        (0, 1, 0) => Face::Up,
        (0, -1, 0) => Face::Down,
        (0, 0, 1) => Face::Front,
        (0, 0, -1) => Face::Back,
        _ => unreachable!("sticker axis rotations stay on the primary axes"),
    }
}

fn opposite_face(face: Face) -> Face {
    match face {
        Face::Up => Face::Down,
        Face::Down => Face::Up,
        Face::Right => Face::Left,
        Face::Left => Face::Right,
        Face::Front => Face::Back,
        Face::Back => Face::Front,
    }
}

fn positive_turn_steps(rotation: RotationAmount) -> usize {
    match rotation {
        RotationAmount::Clockwise => 3,
        RotationAmount::HalfTurn => 2,
        RotationAmount::CounterClockwise => 1,
    }
}

fn rotation_direction_value(rotation: RotationAmount) -> i8 {
    match rotation {
        RotationAmount::Clockwise => 1,
        RotationAmount::HalfTurn => 2,
        RotationAmount::CounterClockwise => -1,
    }
}

fn cubie_to_centered(cubie: UVec3, order: u32) -> IVec3 {
    let max = order.saturating_sub(1) as i32;
    IVec3::new(
        (cubie.x as i32 * 2) - max,
        (cubie.y as i32 * 2) - max,
        (cubie.z as i32 * 2) - max,
    )
}

fn centered_to_cubie(point: IVec3, order: u32) -> UVec3 {
    let max = order.saturating_sub(1) as i32;
    UVec3::new(
        ((point.x + max) / 2) as u32,
        ((point.y + max) / 2) as u32,
        ((point.z + max) / 2) as u32,
    )
}

fn rotate_cubie_for_turn(order: u32, turn: TurnCommand, cubie: UVec3) -> UVec3 {
    let mut centered = cubie_to_centered(cubie, order);
    for _ in 0..positive_turn_steps(turn.rotation) {
        centered = rotate_positive_face_step(turn.face, centered);
    }
    centered_to_cubie(centered, order)
}

fn rotate_face_for_turn(turn: TurnCommand, face: Face) -> Face {
    let mut axis = face_axis_vector(face);
    for _ in 0..positive_turn_steps(turn.rotation) {
        axis = rotate_positive_face_step(turn.face, axis);
    }
    axis_face_ivec(axis)
}

fn rotate_sticker_visual_for_turn(
    order: u32,
    turn: TurnCommand,
    visual: StickerVisual,
) -> StickerVisual {
    let cubie = rotate_cubie_for_turn(order, turn, visual.cubie);
    let face = rotate_face_for_turn(turn, visual.face);
    let (row, col) = sticker_row_col(face, cubie, order as usize);
    StickerVisual {
        face,
        row: row as u8,
        col: col as u8,
        cubie,
    }
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

fn ease_in_out_cubic(progress: f32) -> f32 {
    let t = progress.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        let f = 2.0 * t - 2.0;
        1.0 + 0.5 * f * f * f
    }
}

fn normalize_angle(angle: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    (angle + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI
}

fn shortest_angle_delta(current: f32, target: f32) -> f32 {
    normalize_angle(target - current)
}

fn nearest_orbit_snap(yaw: f32, pitch: f32) -> Option<Vec2> {
    const SNAP_PITCHES: [f32; 2] = [0.5, -0.5];
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

fn cubie_matches_turn(order: u32, turn: TurnCommand, cubie: UVec3) -> bool {
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
        StickerColor::White => Color::srgb_u8(255, 255, 255),
        StickerColor::Red => Color::srgb_u8(185, 0, 0),
        StickerColor::Green => Color::srgb_u8(0, 155, 72),
        StickerColor::Yellow => Color::srgb_u8(255, 213, 0),
        StickerColor::Orange => Color::srgb_u8(255, 89, 0),
        StickerColor::Blue => Color::srgb_u8(0, 69, 173),
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
    order: u32,
    wide_turn: bool,
    selected_layer: u8,
) -> Option<TurnCommand> {
    if selected_layer == 0 || u32::from(selected_layer) > order {
        return None;
    }

    let start_layer = u32::from(selected_layer - 1);
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
        CUBE_FACE_SPAN, CanvasTouchSpace, CubeVisual, CubeVisualPool, CubeVisualRoot, OrbitRig,
        RuntimeBridge, SLICE_DRAG_QUARTER_TURN_PX, ScreenStickerCandidate, StickerVisual,
        TOUCH_MOUSE_SUPPRESSION_SECS, TouchOrbitMode, TurnAnimationPivot, VisualSyncState,
        animate_turn_visuals, clear_cube_visuals, cube_surface_hit_from_ray, cubie_matches_turn,
        cuboid_mesh_template, decode_face, decode_rotation, face_outward_normal, format_turn,
        keyboard_shortcut_turn, merged_cubie_body_mesh, nearest_orbit_snap, normalize_base_path,
        normalize_canvas_selector, normalize_touch_position, partition_body_cubies, reset_cube,
        rotate_sticker_visual_for_turn, set_touch_orbit_mode, should_begin_mouse_orbit,
        should_emulate_two_finger_touch, should_reset_single_touch_gesture,
        signed_slice_snap_quarters, slice_drag_angle_radians, slice_face_from_sticker_drag,
        slice_start_layer, sticker_cubie_coord, sticker_rotation, surface_axis_index,
        surface_cubies, sync_cube_visuals, turn_for_signed_slice_quarters, turn_rotation_angle,
        virtual_cube_half_extent, virtual_surface_center,
    };
    use bevy::{
        ecs::system::SystemState,
        prelude::{
            Assets, ChildOf, Commands, Entity, IntoScheduleConfigs, Mesh, MouseButton, Query,
            Schedule, StandardMaterial, Time, UVec3, Vec2, Vec3, With, World,
        },
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
        runtime.process_queue_head();

        let transition = runtime.last_transition.expect("transition should exist");
        assert_eq!(transition.scene_revision, runtime.scene_revision);
        assert_eq!(transition.from_state, previous);
        assert_eq!(transition.animation, vec![turn]);
    }

    #[test]
    fn undo_records_the_inverse_turn_for_animation() {
        let mut runtime = RuntimeBridge::new(CubeOrder::standard());
        let turn = TurnCommand::outer(Face::Right, RotationAmount::Clockwise);
        runtime.apply_turn(turn).expect("turn should apply");
        runtime.process_queue_head();
        let scrambled = runtime.engine.state().clone();

        runtime.undo().expect("undo should apply");

        let transition = runtime.last_transition.expect("transition should exist");
        assert_eq!(transition.from_state, scrambled);
        assert_eq!(transition.animation, vec![turn.inverse()]);
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
    fn surface_cubies_only_counts_visible_shell_positions() {
        assert_eq!(surface_cubies(3).len(), 26);
        assert_eq!(surface_cubies(17).len(), 1_538);
    }

    #[test]
    fn pending_animation_source_match_requires_adjacent_rendered_revision() {
        let matching = VisualSyncState {
            rendered_revision: 41,
            ..Default::default()
        };
        let missing = VisualSyncState::default();
        let skipped = VisualSyncState {
            rendered_revision: 39,
            ..Default::default()
        };

        assert!(super::pool_matches_pending_animation_source(&matching, 42));
        assert!(!super::pool_matches_pending_animation_source(&missing, 42));
        assert!(!super::pool_matches_pending_animation_source(&skipped, 42));
    }

    #[test]
    fn partition_body_cubies_splits_surface_shell_by_turn_slice() {
        let turn = TurnCommand {
            face: Face::Right,
            start_layer: 1,
            width: 2,
            rotation: RotationAmount::Clockwise,
        };
        let cubies = surface_cubies(5);
        let (static_cubies, animated_cubies) = partition_body_cubies(&cubies, 5, Some(turn));

        assert_eq!(static_cubies.len() + animated_cubies.len(), cubies.len());
        assert!(
            animated_cubies
                .iter()
                .all(|cubie| cubie_matches_turn(5, turn, *cubie))
        );
        assert!(
            static_cubies
                .iter()
                .all(|cubie| !cubie_matches_turn(5, turn, *cubie))
        );
    }

    #[test]
    fn merged_cubie_body_mesh_reuses_template_per_requested_body() {
        let template = cuboid_mesh_template(1.0, 1.0, 1.0);
        let mesh = merged_cubie_body_mesh(
            &template,
            &[UVec3::new(0, 0, 0), UVec3::new(2, 1, 0)],
            3,
            CUBE_FACE_SPAN,
        );
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|values| values.as_float3())
            .expect("merged body mesh should expose positions");
        let index_count = match mesh
            .indices()
            .expect("merged body mesh should expose indices")
        {
            bevy::mesh::Indices::U16(indices) => indices.len(),
            bevy::mesh::Indices::U32(indices) => indices.len(),
        };

        assert_eq!(positions.len(), template.positions.len() * 2);
        assert_eq!(index_count, template.indices.len() * 2);
    }

    #[test]
    fn rotating_a_front_sticker_updates_its_slot_without_rebuilding() {
        let visual = StickerVisual {
            face: Face::Front,
            row: 0,
            col: 0,
            cubie: sticker_cubie_coord(Face::Front, 0, 0, 3),
        };

        let rotated = rotate_sticker_visual_for_turn(
            3,
            TurnCommand::outer(Face::Front, RotationAmount::Clockwise),
            visual,
        );

        assert_eq!(rotated.face, Face::Front);
        assert_eq!((rotated.row, rotated.col), (0, 2));
        assert_eq!(rotated.cubie, sticker_cubie_coord(Face::Front, 0, 2, 3));
    }

    #[test]
    fn rotating_an_adjacent_sticker_moves_it_onto_the_next_face() {
        let visual = StickerVisual {
            face: Face::Up,
            row: 2,
            col: 0,
            cubie: sticker_cubie_coord(Face::Up, 2, 0, 3),
        };

        let rotated = rotate_sticker_visual_for_turn(
            3,
            TurnCommand::outer(Face::Front, RotationAmount::Clockwise),
            visual,
        );

        assert_eq!(rotated.face, Face::Right);
        assert_eq!((rotated.row, rotated.col), (0, 0));
        assert_eq!(rotated.cubie, sticker_cubie_coord(Face::Right, 0, 0, 3));
    }

    #[test]
    fn orbit_snap_targets_nearby_isometric_views() {
        let target = nearest_orbit_snap(0.81, 0.48).expect("should snap to a nearby view");
        assert!((target.x - std::f32::consts::FRAC_PI_4).abs() < 0.05);
        assert!((target.y - 0.5).abs() < 0.05);
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
    fn slice_drag_projection_maps_screen_motion_to_turn_angle() {
        let angle = slice_drag_angle_radians(
            Vec2::ZERO,
            Vec2::new(SLICE_DRAG_QUARTER_TURN_PX, 0.0),
            Vec2::X,
        );

        assert!((angle + std::f32::consts::FRAC_PI_2).abs() < 0.0001);
    }

    #[test]
    fn slice_snap_thresholds_follow_release_rules() {
        assert_eq!(signed_slice_snap_quarters(9.0_f32.to_radians()), 0);
        assert_eq!(signed_slice_snap_quarters(10.0_f32.to_radians()), 0);
        assert_eq!(signed_slice_snap_quarters(11.0_f32.to_radians()), 1);
        assert_eq!(signed_slice_snap_quarters(89.0_f32.to_radians()), 1);
        assert_eq!(signed_slice_snap_quarters(-89.0_f32.to_radians()), -1);
        assert_eq!(signed_slice_snap_quarters(120.0_f32.to_radians()), 1);
        assert_eq!(signed_slice_snap_quarters(140.0_f32.to_radians()), 2);
        assert_eq!(signed_slice_snap_quarters(-140.0_f32.to_radians()), -2);
    }

    #[test]
    fn signed_slice_quarters_convert_to_supported_turn_commands() {
        let base = TurnCommand::outer(Face::Front, RotationAmount::Clockwise);

        assert_eq!(
            turn_for_signed_slice_quarters(base, 1).map(|turn| turn.rotation),
            Some(RotationAmount::CounterClockwise)
        );
        assert_eq!(
            turn_for_signed_slice_quarters(base, -1).map(|turn| turn.rotation),
            Some(RotationAmount::Clockwise)
        );
        assert_eq!(
            turn_for_signed_slice_quarters(base, 2).map(|turn| turn.rotation),
            Some(RotationAmount::HalfTurn)
        );
        assert_eq!(turn_for_signed_slice_quarters(base, 4), None);
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

    #[test]
    fn render_systems_initialize_without_transform_query_conflicts() {
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        world.insert_resource(Assets::<Mesh>::default());
        world.insert_resource(Assets::<StandardMaterial>::default());
        world.insert_resource(CubeVisualPool::default());
        world.insert_resource(VisualSyncState::default());

        let mut schedule = Schedule::default();
        schedule.add_systems((sync_cube_visuals, animate_turn_visuals).chain());
        schedule.run(&mut world);
    }
}
