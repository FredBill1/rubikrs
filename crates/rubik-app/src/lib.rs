#![forbid(unsafe_code)]

use std::{
    cell::RefCell,
};

use bevy::{
    core_pipeline::tonemapping::Tonemapping,
    input::{
        mouse::{MouseMotion, MouseWheel},
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
use js_sys::Date;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::wasm_bindgen;

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
    mouse_drag_active: bool,
    touch_drag_active: bool,
    snap_target: Option<Vec2>,
    previous_touch_center: Option<Vec2>,
    previous_pinch_distance: Option<f32>,
}

#[derive(Resource, Default)]
struct DirectTurnInputState {
    mouse_candidate: Option<PointerTapCandidate>,
    touch_candidate: Option<TouchTapCandidate>,
    queued_tap: Option<QueuedFaceTap>,
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
struct PointerTapCandidate {
    start_position: Vec2,
    max_distance: f32,
}

#[derive(Debug, Clone, Copy)]
struct TouchTapCandidate {
    id: u64,
    start_position: Vec2,
    max_distance: f32,
}

#[derive(Debug, Clone, Copy)]
struct QueuedFaceTap {
    position: Vec2,
    inverse: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ScreenFaceCandidate {
    face: Face,
    center: Vec2,
    radius: f32,
}

const CUBE_FACE_SPAN: f32 = 1.9;
const POINTER_TAP_MAX_DRAG_PX: f32 = 8.0;
const FACE_TAP_VISIBILITY_THRESHOLD: f32 = 0.18;
const FACE_TAP_RADIUS_SCALE: f32 = 0.7;

impl Default for OrbitRig {
    fn default() -> Self {
        Self {
            yaw: 0.78,
            pitch: 0.26,
            radius: 7.2,
            auto_spin: false,
            mouse_drag_active: false,
            touch_drag_active: false,
            snap_target: None,
            previous_touch_center: None,
            previous_pinch_distance: None,
        }
    }
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
        self.timer.sync(self.engine.move_count(), self.engine.is_solved());
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
        self.engine.apply_turn(turn).map_err(|error| error.to_string())?;
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
        runtime.set_message("Bevy runtime mounted. Use the shell or keyboard shortcuts to manipulate the cube.");
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
    with_runtime(|runtime| serde_json::to_string(&runtime.status()).unwrap_or_else(|_| "{}".to_owned()))
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn export_cube_state() -> String {
    with_runtime(|runtime| runtime.engine.state().to_json().unwrap_or_else(|_| "{}".to_owned()))
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
            runtime.set_message(format!("Order {} is outside the supported 2..=17 range.", order));
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

fn setup_scene(
    mut commands: Commands<'_, '_>,
    config: Res<'_, ShellConfig>,
) {
    info!(
        "booting Bevy runtime on {} with canvas {}",
        config.base_path,
        config.canvas_selector
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
    existing_visuals: Query<'_, '_, Entity, With<CubeVisual>>,
) {
    let snapshot = with_runtime(|runtime| runtime.snapshot());

    if let Some(active) = &sync_state.active_animation {
        if snapshot.scene_revision == active.scene_revision {
            with_runtime_mut(|runtime| runtime.set_animation_active(true));
            return;
        }

        clear_cube_visuals(&mut commands, &existing_visuals);
        sync_state.active_animation = None;
        sync_state.rendered_revision = 0;
        sync_state.completed_animation_revision = None;
        with_runtime_mut(|runtime| runtime.set_animation_active(false));
    }

    if snapshot.scene_revision == sync_state.rendered_revision {
        with_runtime_mut(|runtime| runtime.set_animation_active(false));
        return;
    }

    clear_cube_visuals(&mut commands, &existing_visuals);

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
    existing_visuals: Query<'_, '_, Entity, With<CubeVisual>>,
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

    animation.elapsed_secs = (animation.elapsed_secs + time.delta_secs()).min(animation.duration_secs);
    let progress = if animation.duration_secs <= f32::EPSILON {
        1.0
    } else {
        animation.elapsed_secs / animation.duration_secs
    };
    let eased = ease_out_cubic(progress);
    pivot_transform.rotation = Quat::from_axis_angle(animation.axis, animation.angle_radians * eased);

    if progress >= 1.0 {
        let scene_revision = animation.scene_revision;
        clear_cube_visuals(&mut commands, &existing_visuals);
        sync_state.rendered_revision = 0;
        sync_state.active_animation = None;
        sync_state.completed_animation_revision = Some(scene_revision);
        with_runtime_mut(|runtime| runtime.set_animation_active(false));
    }
}

fn clear_cube_visuals(
    commands: &mut Commands<'_, '_>,
    existing_visuals: &Query<'_, '_, Entity, With<CubeVisual>>,
) {
    for entity in existing_visuals.iter() {
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
    let cube_size = face_span + (step * 0.12);
    let face_offset = cube_face_offset(state.order.get());

    let root = commands
        .spawn((
            CubeVisual,
            CubeVisualRoot,
            Name::new(format!("cube-visual-{}x{}", state.order.get(), state.order.get())),
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

    let accent_material = materials.add(StandardMaterial {
        base_color: Color::srgb_u8(13, 15, 23),
        metallic: 0.55,
        perceptual_roughness: 0.22,
        ..default()
    });

    let sticker_mesh_front = meshes.add(Cuboid::new(sticker_size, sticker_size, sticker_depth));
    let sticker_mesh_side = meshes.add(Cuboid::new(sticker_depth, sticker_size, sticker_size));
    let sticker_mesh_top = meshes.add(Cuboid::new(sticker_size, sticker_depth, sticker_size));
    let mut sticker_entities = Vec::with_capacity(Face::ALL.len() * order * order);

    commands.entity(root).with_children(|parent| {
        parent.spawn((
            CubeVisual,
            Mesh3d(meshes.add(Cuboid::new(cube_size, cube_size, cube_size))),
            MeshMaterial3d(shell_material.clone()),
            Transform::default(),
            Name::new("cube-core"),
        ));

        parent.spawn((
            CubeVisual,
            Mesh3d(meshes.add(Cuboid::new(
                cube_size * 1.02,
                cube_size * 1.02,
                cube_size * 1.02,
            ))),
            MeshMaterial3d(accent_material.clone()),
            Transform::from_scale(Vec3::splat(1.0)),
            Name::new("cube-hull"),
        ));

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
                        &sticker_mesh_front,
                        &sticker_mesh_side,
                        &sticker_mesh_top,
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
                    sticker_entities.push((entity, cubie));
                }
            }
        }
    });

    (root, sticker_entities)
}

fn orbit_camera_input(
    time: Res<'_, Time>,
    buttons: Res<'_, ButtonInput<MouseButton>>,
    keys: Res<'_, ButtonInput<KeyCode>>,
    touches: Res<'_, Touches>,
    windows: Query<'_, '_, &Window>,
    mut mouse_motion: MessageReader<'_, '_, MouseMotion>,
    mut mouse_wheel: MessageReader<'_, '_, MouseWheel>,
    mut orbit: ResMut<'_, OrbitRig>,
    mut direct_turn_input: ResMut<'_, DirectTurnInputState>,
) {
    let shift_pressed = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);

    if keys.just_pressed(KeyCode::Space) {
        orbit.auto_spin = !orbit.auto_spin;
        orbit.snap_target = None;
    }

    if orbit.auto_spin {
        orbit.yaw += time.delta_secs() * 0.18;
    }

    let cursor_position = windows.iter().next().and_then(|window| window.cursor_position());
    let mouse_delta = mouse_motion
        .read()
        .fold(Vec2::ZERO, |total, event| total + event.delta);

    if buttons.just_pressed(MouseButton::Left) {
        direct_turn_input.mouse_candidate = cursor_position.map(|position| PointerTapCandidate {
            start_position: position,
            max_distance: 0.0,
        });
        orbit.mouse_drag_active = false;
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

            if candidate.max_distance > POINTER_TAP_MAX_DRAG_PX {
                direct_turn_input.mouse_candidate = None;
                orbit.mouse_drag_active = true;
                orbit.snap_target = None;
            }
        }

        if orbit.mouse_drag_active {
            orbit.snap_target = None;
            orbit.auto_spin = false;
            orbit.yaw -= mouse_delta.x * 0.008;
            orbit.pitch = (orbit.pitch - mouse_delta.y * 0.006).clamp(-1.15, 1.15);
        }
    }

    if buttons.just_released(MouseButton::Left) {
        if orbit.mouse_drag_active {
            orbit.mouse_drag_active = false;
            orbit.snap_target = nearest_orbit_snap(orbit.yaw, orbit.pitch);
        } else if let Some(candidate) = direct_turn_input.mouse_candidate.take() {
            let position = cursor_position.unwrap_or(candidate.start_position);
            if position.distance(candidate.start_position) <= POINTER_TAP_MAX_DRAG_PX {
                direct_turn_input.queued_tap = Some(QueuedFaceTap {
                    position,
                    inverse: shift_pressed,
                });
            }
        }
    }

    if buttons.just_released(MouseButton::Right) {
        if let Some(position) = cursor_position {
            direct_turn_input.queued_tap = Some(QueuedFaceTap {
                position,
                inverse: true,
            });
        }
    }

    for event in mouse_wheel.read() {
        orbit.radius = (orbit.radius - event.y * 0.18).clamp(2.9, 9.4);
    }

    if let Some(candidate) = direct_turn_input.touch_candidate {
        if let Some(released_touch) = touches.get_released(candidate.id) {
            if !orbit.touch_drag_active && candidate.max_distance <= POINTER_TAP_MAX_DRAG_PX {
                direct_turn_input.queued_tap = Some(QueuedFaceTap {
                    position: released_touch.position(),
                    inverse: false,
                });
            }
            direct_turn_input.touch_candidate = None;
        } else if touches.just_canceled(candidate.id) {
            direct_turn_input.touch_candidate = None;
        }
    } else if let Some(one_frame_tap) = touches
        .iter_just_released()
        .find(|touch| touches.just_pressed(touch.id()))
    {
        direct_turn_input.queued_tap = Some(QueuedFaceTap {
            position: one_frame_tap.position(),
            inverse: false,
        });
    }

    let active_touches = touches.iter().copied().collect::<Vec<_>>();

    match active_touches.as_slice() {
        [] => {
            if orbit.touch_drag_active {
                orbit.snap_target = nearest_orbit_snap(orbit.yaw, orbit.pitch);
            }
            orbit.touch_drag_active = false;
            orbit.previous_touch_center = None;
            orbit.previous_pinch_distance = None;
        }
        [touch] => {
            orbit.auto_spin = false;

            if touches.just_pressed(touch.id()) {
                direct_turn_input.touch_candidate = Some(TouchTapCandidate {
                    id: touch.id(),
                    start_position: touch.position(),
                    max_distance: 0.0,
                });
                orbit.touch_drag_active = false;
            }

            if let Some(candidate) = direct_turn_input
                .touch_candidate
                .as_mut()
                .filter(|candidate| candidate.id == touch.id())
            {
                candidate.max_distance = candidate
                    .max_distance
                    .max(touch.position().distance(candidate.start_position))
                    .max(touch.delta().length());

                if candidate.max_distance > POINTER_TAP_MAX_DRAG_PX {
                    direct_turn_input.touch_candidate = None;
                    orbit.touch_drag_active = true;
                    orbit.snap_target = None;
                }
            }

            if orbit.touch_drag_active {
                orbit.snap_target = None;
                orbit.yaw -= touch.delta().x * 0.008;
                orbit.pitch = (orbit.pitch - touch.delta().y * 0.006).clamp(-1.15, 1.15);
            }

            orbit.previous_touch_center = Some(touch.position());
            orbit.previous_pinch_distance = None;
        }
        [first, second, ..] => {
            orbit.auto_spin = false;
            orbit.touch_drag_active = true;
            orbit.snap_target = None;
            direct_turn_input.touch_candidate = None;

            let center = (first.position() + second.position()) * 0.5;
            if let Some(previous_center) = orbit.previous_touch_center {
                let delta = center - previous_center;
                orbit.yaw -= delta.x * 0.006;
                orbit.pitch = (orbit.pitch - delta.y * 0.0045).clamp(-1.15, 1.15);
            }

            let pinch_distance = first.position().distance(second.position());
            if let Some(previous_pinch_distance) = orbit.previous_pinch_distance {
                orbit.radius = (orbit.radius - (pinch_distance - previous_pinch_distance) * 0.01)
                    .clamp(2.9, 9.4);
            }

            orbit.previous_touch_center = Some(center);
            orbit.previous_pinch_distance = Some(pinch_distance);
        }
    }

    if !orbit.auto_spin && !orbit.mouse_drag_active && !orbit.touch_drag_active {
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

fn canvas_face_tap_input(
    mut direct_turn_input: ResMut<'_, DirectTurnInputState>,
    sync_state: Res<'_, VisualSyncState>,
    cameras: Query<'_, '_, (&Camera, &GlobalTransform), With<Camera3d>>,
) {
    let Some(queued_tap) = direct_turn_input.queued_tap.take() else {
        return;
    };

    if sync_state.active_animation.is_some() {
        return;
    }

    let Ok((camera, camera_transform)) = cameras.single() else {
        return;
    };

    let order = with_runtime(|runtime| runtime.engine.order().get());
    let Some(turn) = projected_face_tap_turn(
        camera,
        camera_transform,
        order,
        queued_tap.position,
        queued_tap.inverse,
    ) else {
        return;
    };

    let _ = update_runtime(|runtime| runtime.apply_turn(turn));
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
            if let Some(turn) = keyboard_shortcut_turn(face, rotation, order, wide_turn, keyboard_selected_layer(&keys))
            {
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

fn projected_face_tap_turn(
    camera: &Camera,
    camera_transform: &GlobalTransform,
    order: u8,
    pointer_position: Vec2,
    inverse: bool,
) -> Option<TurnCommand> {
    let candidates = projected_face_candidates(camera, camera_transform, order);
    pick_face_candidate(pointer_position, &candidates).map(|face| face_tap_turn(face, inverse))
}

fn projected_face_candidates(
    camera: &Camera,
    camera_transform: &GlobalTransform,
    order: u8,
) -> Vec<ScreenFaceCandidate> {
    let half_span = CUBE_FACE_SPAN * 0.5;
    let face_offset = cube_face_offset(order);
    let camera_forward = camera_transform.forward().as_vec3();
    let mut candidates = Vec::with_capacity(Face::ALL.len());

    for face in Face::ALL {
        let visibility = face_outward_normal(face).dot(-camera_forward);
        if visibility <= FACE_TAP_VISIBILITY_THRESHOLD {
            continue;
        }

        let face_center = cube_face_center(face, face_offset);
        let (axis_u, axis_v) = face_pick_axes(face);
        let projected_center = match camera.world_to_viewport(camera_transform, face_center) {
            Ok(position) => position,
            Err(_) => continue,
        };
        let projected_corner = match camera.world_to_viewport(
            camera_transform,
            face_center + axis_u * half_span + axis_v * half_span,
        ) {
            Ok(position) => position,
            Err(_) => continue,
        };
        let radius = projected_center.distance(projected_corner) * FACE_TAP_RADIUS_SCALE;
        if radius <= f32::EPSILON {
            continue;
        }

        candidates.push(ScreenFaceCandidate {
            face,
            center: projected_center,
            radius,
        });
    }

    candidates
}

fn pick_face_candidate(pointer_position: Vec2, candidates: &[ScreenFaceCandidate]) -> Option<Face> {
    candidates
        .iter()
        .filter_map(|candidate| {
            let distance = candidate.center.distance(pointer_position);
            (distance <= candidate.radius).then_some((distance, candidate.face))
        })
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .map(|(_, face)| face)
}

fn face_tap_turn(face: Face, inverse: bool) -> TurnCommand {
    TurnCommand::outer(
        face,
        if inverse {
            RotationAmount::CounterClockwise
        } else {
            RotationAmount::Clockwise
        },
    )
}

fn cube_face_offset(order: u8) -> f32 {
    let step = CUBE_FACE_SPAN / order.max(1) as f32;
    let cube_size = CUBE_FACE_SPAN + (step * 0.12);
    cube_size / 2.0 + 0.03
}

fn cube_face_center(face: Face, face_offset: f32) -> Vec3 {
    match face {
        Face::Up => Vec3::new(0.0, face_offset, 0.0),
        Face::Right => Vec3::new(face_offset, 0.0, 0.0),
        Face::Front => Vec3::new(0.0, 0.0, face_offset),
        Face::Down => Vec3::new(0.0, -face_offset, 0.0),
        Face::Left => Vec3::new(-face_offset, 0.0, 0.0),
        Face::Back => Vec3::new(0.0, 0.0, -face_offset),
    }
}

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

fn face_pick_axes(face: Face) -> (Vec3, Vec3) {
    match face {
        Face::Up | Face::Down => (Vec3::X, Vec3::Z),
        Face::Right | Face::Left => (Vec3::Y, Vec3::Z),
        Face::Front | Face::Back => (Vec3::X, Vec3::Y),
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

fn sticker_transform(
    face: Face,
    row: usize,
    col: usize,
    order: usize,
    face_span: f32,
    face_offset: f32,
    front_mesh: &Handle<Mesh>,
    side_mesh: &Handle<Mesh>,
    top_mesh: &Handle<Mesh>,
) -> (Vec3, Quat, Handle<Mesh>) {
    let cubie = sticker_cubie_coord(face, row, col, order);
    let x = cubie.x as usize;
    let y = cubie.y as usize;
    let z = cubie.z as usize;

    let axis = |index: usize| -> f32 {
        if order <= 1 {
            0.0
        } else {
            let step = face_span / order as f32;
            (-face_span / 2.0) + (step * 0.5) + (index as f32 * step)
        }
    };

    match face {
        Face::Front => (
            Vec3::new(axis(x), axis(y), face_offset),
            Quat::IDENTITY,
            front_mesh.clone(),
        ),
        Face::Back => (
            Vec3::new(axis(x), axis(y), -face_offset),
            Quat::from_rotation_y(std::f32::consts::PI),
            front_mesh.clone(),
        ),
        Face::Right => (
            Vec3::new(face_offset, axis(y), axis(z)),
            Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2),
            side_mesh.clone(),
        ),
        Face::Left => (
            Vec3::new(-face_offset, axis(y), axis(z)),
            Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
            side_mesh.clone(),
        ),
        Face::Up => (
            Vec3::new(axis(x), face_offset, axis(z)),
            Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
            top_mesh.clone(),
        ),
        Face::Down => (
            Vec3::new(axis(x), -face_offset, axis(z)),
            Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
            top_mesh.clone(),
        ),
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
        RuntimeBridge, ScreenFaceCandidate, cubie_matches_turn, decode_face, decode_rotation,
        face_tap_turn, format_turn, keyboard_shortcut_turn, nearest_orbit_snap,
        normalize_base_path, normalize_canvas_selector, pick_face_candidate, reset_cube,
        turn_rotation_angle,
    };
    use rubik_core::{CubeOrder, Face, RotationAmount, TurnCommand};
    use bevy::prelude::{UVec3, Vec2};

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
            turn_rotation_angle(TurnCommand::outer(Face::Back, RotationAmount::CounterClockwise)),
            std::f32::consts::FRAC_PI_2
        );
    }

    #[test]
    fn orbit_snap_targets_nearby_isometric_views() {
        let target = nearest_orbit_snap(0.81, 0.24).expect("should snap to a nearby view");
        assert!((target.x - std::f32::consts::FRAC_PI_4).abs() < 0.05);
        assert!((target.y - 0.26).abs() < 0.05);
        assert!(nearest_orbit_snap(1.7, 0.24).is_none());
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
        let turn = keyboard_shortcut_turn(Face::Front, RotationAmount::CounterClockwise, 4, true, 1)
            .expect("wide outer turn should be valid on 4x4");
        assert_eq!(turn.start_layer, 0);
        assert_eq!(turn.width, 2);
        assert!(keyboard_shortcut_turn(Face::Front, RotationAmount::Clockwise, 2, true, 2).is_none());
    }

    #[test]
    fn face_tap_turn_uses_inverse_flag() {
        assert_eq!(
            face_tap_turn(Face::Up, false),
            TurnCommand::outer(Face::Up, RotationAmount::Clockwise)
        );
        assert_eq!(
            face_tap_turn(Face::Up, true),
            TurnCommand::outer(Face::Up, RotationAmount::CounterClockwise)
        );
    }

    #[test]
    fn pick_face_candidate_prefers_the_closest_matching_face() {
        let selected = pick_face_candidate(
            Vec2::new(104.0, 96.0),
            &[
                ScreenFaceCandidate {
                    face: Face::Front,
                    center: Vec2::new(100.0, 100.0),
                    radius: 18.0,
                },
                ScreenFaceCandidate {
                    face: Face::Right,
                    center: Vec2::new(126.0, 98.0),
                    radius: 18.0,
                },
            ],
        );

        assert_eq!(selected, Some(Face::Front));
    }

    #[test]
    fn pick_face_candidate_rejects_pointers_outside_face_radius() {
        let selected = pick_face_candidate(
            Vec2::new(140.0, 140.0),
            &[ScreenFaceCandidate {
                face: Face::Front,
                center: Vec2::new(100.0, 100.0),
                radius: 16.0,
            }],
        );

        assert_eq!(selected, None);
    }
}
