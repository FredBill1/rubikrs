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
    previous_touch_center: Option<Vec2>,
    previous_pinch_distance: Option<f32>,
}

#[derive(Resource, Default)]
struct VisualSyncState {
    rendered_revision: u64,
}

#[derive(Debug, Clone)]
struct RuntimeBridge {
    engine: CubeEngine,
    scene_revision: u64,
    last_message: String,
    timer: RuntimeTimer,
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
    elapsed_millis: u64,
    scene_revision: u64,
    last_message: String,
    recent_turns: Vec<String>,
}

#[derive(Debug, Clone)]
struct RuntimeSnapshot {
    state: CubeState,
    scene_revision: u64,
}

impl Default for OrbitRig {
    fn default() -> Self {
        Self {
            yaw: 0.78,
            pitch: 0.26,
            radius: 7.2,
            auto_spin: false,
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
        }
    }

    fn snapshot(&self) -> RuntimeSnapshot {
        RuntimeSnapshot {
            state: self.engine.state().clone(),
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

    fn bump_scene(&mut self) {
        self.scene_revision += 1;
        self.timer.sync(self.engine.move_count(), self.engine.is_solved());
    }

    fn set_order(&mut self, order: CubeOrder) {
        self.engine = CubeEngine::new(order);
        self.timer.reset();
        self.bump_scene();
        self.set_message(format!("Switched to {}x{}.", order.get(), order.get()));
    }

    fn reset(&mut self) {
        self.engine.reset();
        self.timer.reset();
        self.bump_scene();
        self.set_message("Reset cube to solved state.");
    }

    fn apply_turn(&mut self, turn: TurnCommand) -> Result<(), String> {
        self.engine.apply_turn(turn).map_err(|error| error.to_string())?;
        self.bump_scene();
        self.set_message(format!("Applied {}.", format_turn(turn)));
        Ok(())
    }

    fn undo(&mut self) -> Result<(), String> {
        let turn = self.engine.undo().map_err(|error| error.to_string())?;
        self.bump_scene();
        self.set_message(format!("Undid {}.", format_turn(turn)));
        Ok(())
    }

    fn redo(&mut self) -> Result<(), String> {
        let turn = self.engine.redo().map_err(|error| error.to_string())?;
        self.bump_scene();
        self.set_message(format!("Redid {}.", format_turn(turn)));
        Ok(())
    }

    fn scramble(&mut self, length: usize, seed: u64) -> Result<(), String> {
        let scramble = self
            .engine
            .scramble_with_seed(length.max(1), seed)
            .map_err(|error| error.to_string())?;
        self.bump_scene();
        self.set_message(format!(
            "Applied scramble ({} turns, seed {}).",
            scramble.len(),
            seed
        ));
        Ok(())
    }

    fn import_state(&mut self, json: &str) -> Result<(), String> {
        let state = CubeState::from_json(json).map_err(|error| error.to_string())?;
        self.engine = CubeEngine::from_state(state).map_err(|error| error.to_string())?;
        self.timer.reset();
        self.bump_scene();
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
                sync_cube_visuals,
                orbit_camera_input,
                keyboard_turn_shortcuts,
                apply_camera_transform,
            ),
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

    if snapshot.scene_revision == sync_state.rendered_revision {
        return;
    }

    for entity in &existing_visuals {
        commands.entity(entity).despawn();
    }

    let order = usize::from(snapshot.state.order.get());
    let face_span = 1.9_f32;
    let step = face_span / order as f32;
    let sticker_size = step * 0.84;
    let sticker_depth = 0.06_f32;
    let cube_size = face_span + (step * 0.12);
    let face_offset = cube_size / 2.0 + (sticker_depth / 2.0);

    let root = commands
        .spawn((
            CubeVisual,
            Name::new(format!(
                "cube-visual-{}x{}",
                snapshot.state.order.get(),
                snapshot.state.order.get()
            )),
            Transform::default(),
            GlobalTransform::default(),
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
                    let color = snapshot.state.stickers[(face_index * order * order) + (row * order) + col];
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

                    parent.spawn((
                        CubeVisual,
                        Mesh3d(mesh.clone()),
                        MeshMaterial3d(material),
                        Transform::from_translation(translation).with_rotation(rotation),
                        Name::new(format!("sticker-{face_index}-{row}-{col}")),
                    ));
                }
            }
        }
    });

    sync_state.rendered_revision = snapshot.scene_revision;
}

fn orbit_camera_input(
    time: Res<'_, Time>,
    buttons: Res<'_, ButtonInput<MouseButton>>,
    keys: Res<'_, ButtonInput<KeyCode>>,
    touches: Res<'_, Touches>,
    mut mouse_motion: MessageReader<'_, '_, MouseMotion>,
    mut mouse_wheel: MessageReader<'_, '_, MouseWheel>,
    mut orbit: ResMut<'_, OrbitRig>,
) {
    if keys.just_pressed(KeyCode::Space) {
        orbit.auto_spin = !orbit.auto_spin;
    }

    if orbit.auto_spin {
        orbit.yaw += time.delta_secs() * 0.18;
    }

    if buttons.pressed(MouseButton::Left) {
        for event in mouse_motion.read() {
            orbit.yaw -= event.delta.x * 0.008;
            orbit.pitch = (orbit.pitch - event.delta.y * 0.006).clamp(-1.15, 1.15);
        }
    } else {
        mouse_motion.clear();
    }

    for event in mouse_wheel.read() {
        orbit.radius = (orbit.radius - event.y * 0.18).clamp(2.9, 9.4);
    }

    let active_touches = touches
        .iter()
        .map(|touch| touch.position())
        .collect::<Vec<Vec2>>();

    match active_touches.as_slice() {
        [] => {
            orbit.previous_touch_center = None;
            orbit.previous_pinch_distance = None;
        }
        [position] => {
            orbit.auto_spin = false;

            if let Some(previous_center) = orbit.previous_touch_center {
                let delta = *position - previous_center;
                orbit.yaw -= delta.x * 0.008;
                orbit.pitch = (orbit.pitch - delta.y * 0.006).clamp(-1.15, 1.15);
            }

            orbit.previous_touch_center = Some(*position);
            orbit.previous_pinch_distance = None;
        }
        [first, second, ..] => {
            orbit.auto_spin = false;

            let center = (*first + *second) * 0.5;
            if let Some(previous_center) = orbit.previous_touch_center {
                let delta = center - previous_center;
                orbit.yaw -= delta.x * 0.006;
                orbit.pitch = (orbit.pitch - delta.y * 0.0045).clamp(-1.15, 1.15);
            }

            let pinch_distance = first.distance(*second);
            if let Some(previous_pinch_distance) = orbit.previous_pinch_distance {
                orbit.radius = (orbit.radius - (pinch_distance - previous_pinch_distance) * 0.01)
                    .clamp(2.9, 9.4);
            }

            orbit.previous_touch_center = Some(center);
            orbit.previous_pinch_distance = Some(pinch_distance);
        }
    }
}

fn keyboard_turn_shortcuts(keys: Res<'_, ButtonInput<KeyCode>>) {
    let shift_pressed = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let half_turn = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
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
            let _ = update_runtime(|runtime| runtime.apply_turn(TurnCommand::outer(face, rotation)));
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
    let max = order.saturating_sub(1);
    let (x, y, z) = match face {
        Face::Up => (col, max, row),
        Face::Right => (max, max - row, max - col),
        Face::Front => (col, max - row, max),
        Face::Down => (col, 0, max - row),
        Face::Left => (0, max - row, col),
        Face::Back => (max - col, max - row, 0),
    };

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
        decode_face, decode_rotation, format_turn, normalize_base_path, normalize_canvas_selector,
        reset_cube,
    };
    use rubik_core::{Face, RotationAmount, TurnCommand};

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
}
