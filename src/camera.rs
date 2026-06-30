use std::marker::PhantomData;
use std::{f32::consts::{FRAC_PI_2, TAU}, ops::Range};

use bevy::ecs::component::Mutable;
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::input::{common_conditions::input_pressed};
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use bevy_egui::EguiContexts;
use crate::settings_panel::PanelWidth;

#[derive(Default)]
pub struct CameraPlugin<C> {
    _phantom: PhantomData<C>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, SystemSet)]
struct PanSet;

impl<C: Component<Mutability = Mutable> + Position> Plugin for CameraPlugin<C> {
    fn build(&self, app: &mut App) {
        app.init_resource::<AutoOrbit>();
        app.init_resource::<CameraInputEnabled>();
        app.init_resource::<CameraSettings>();
        app.init_resource::<DragStartedOverEgui>();
        app.add_systems(Startup, setup_camera);
        app.add_systems(Update, (
            auto_orbit_camera.after(update_camera),
            cancel_auto_orbit_on_input,
            gate_camera_input,
            toggle_auto_orbit,
            toggle_mouse_control,
            update_camera.after(PanSet).after(gate_camera_input),
        ));
        app.add_systems(Update, (
            pan_bodies::<C,  0,  0,  1>.run_if(input_pressed(KeyCode::KeyS)),
            pan_bodies::<C,  0,  0, -1>.run_if(input_pressed(KeyCode::KeyW)),
            pan_bodies::<C,  0, -1,  0>.run_if(input_pressed(KeyCode::KeyQ)),
            pan_bodies::<C,  0,  1,  0>.run_if(input_pressed(KeyCode::KeyE)),
            pan_bodies::<C, -1,  0,  0>.run_if(input_pressed(KeyCode::KeyD)),
            pan_bodies::<C,  1,  0,  0>.run_if(input_pressed(KeyCode::KeyA)),
        ).in_set(PanSet).run_if(camera_input_enabled));
    }
}


#[derive(Component)]
pub struct MainCamera;

#[derive(Debug, Resource)]
struct CameraSettings {
    pub orbit_distance: f32,
    pub orbit_distance_range: Range<f32>,
    pub zoom_speed: f32,
    pub pitch_speed: f32,
    pub pitch_range: Range<f32>,
    pub yaw_speed: f32,
}

const DEFAULT_ORBIT_DISTANCE: f32 = 384.0;

impl Default for CameraSettings {
    fn default() -> Self {
        let pitch_limit = FRAC_PI_2 - 0.01;
        Self {
            orbit_distance: DEFAULT_ORBIT_DISTANCE,
            orbit_distance_range: 10.0..1500.0,
            zoom_speed: 0.0025,
            pitch_speed: 0.003,
            pitch_range: -pitch_limit..pitch_limit,
            yaw_speed: 0.004,
        }
    }
}

/// When active, the camera orbits the origin at one revolution per minute
/// and slerps the zoom back to the default distance.
#[derive(Resource)]
struct AutoOrbit {
    active: bool,
}

impl Default for AutoOrbit {
    fn default() -> Self {
        Self { active: true }
    }
}

/// Gates camera systems — when false, camera ignores mouse/keyboard.
#[derive(Resource)]
pub struct CameraInputEnabled(bool);

impl Default for CameraInputEnabled {
    fn default() -> Self {
        Self(true)
    }
}

impl CameraInputEnabled {
    pub fn enabled(&self) -> bool {
        self.0
    }
}

/// Tracks whether the current mouse drag started over egui.
/// If it did, camera input stays disabled for the entire drag.
#[derive(Resource, Default)]
struct DragStartedOverEgui(bool);

pub trait Position {
    #[allow(dead_code)]
    fn position(&self) -> &DVec3;
    fn position_mut(&mut self) -> &mut DVec3;
}

/// Run condition: returns `true` when camera input is enabled (i.e. egui does not have focus).
pub fn camera_input_enabled(enabled: Res<CameraInputEnabled>) -> bool {
    enabled.0
}

fn gate_camera_input(
    mut contexts: EguiContexts,
    mut camera_input: ResMut<CameraInputEnabled>,
    mut drag_over_egui: ResMut<DragStartedOverEgui>,
    cursor_query: Query<&CursorOptions>,
    mouse: Res<ButtonInput<MouseButton>>,
    panel_width: Res<PanelWidth>,
    window: Single<&Window, With<PrimaryWindow>>,
) {
    // Check if the cursor is over the panel area using physical pixel position.
    let pointer_over_panel = window
        .cursor_position()
        .map(|pos| pos.x * window.scale_factor() < panel_width.0)
        .unwrap_or(false);

    // If cursor is visible (UI mode), only allow camera when left mouse is held
    // and the drag did not start over the egui panel.
    for cursor in cursor_query.iter() {
        if cursor.visible {
            if mouse.just_pressed(MouseButton::Left) {
                // Drag is starting — record whether it began over the panel.
                drag_over_egui.0 = pointer_over_panel;
                camera_input.0 = !drag_over_egui.0;
            } else if mouse.pressed(MouseButton::Left) {
                // Drag is ongoing — keep camera disabled if it started over the panel.
                camera_input.0 = !drag_over_egui.0;
            } else {
                // No mouse button held — reset drag state, disable camera.
                drag_over_egui.0 = false;
                camera_input.0 = false;
            }
            return;
        }
    }

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let pointer_over_egui = ctx.is_pointer_over_egui() || pointer_over_panel;
    let keyboard_captured = ctx.egui_wants_keyboard_input();
    camera_input.0 = !pointer_over_egui && !keyboard_captured;
}

fn pan_bodies<
    C: Component<Mutability = Mutable> + Position,
    const X: i8,
    const Y: i8,
    const Z: i8,
>(
    mut query: Query<&mut C>,
    camera: Single<&GlobalTransform, With<MainCamera>>,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    camera_input: Res<CameraInputEnabled>,
)
{
    if !camera_input.enabled() { return; }
    let (forward, right) = if keys.pressed(KeyCode::ShiftLeft) {
        looking_axis(camera)
    } else {
        (camera.forward(), camera.right())
    };
    let offset_dir = right.to_vec3a() * X as f32 + Vec3A::Y * Y as f32 + forward.to_vec3a() * Z as f32;
    let offset = 0.125 * time.delta_secs_f64() * offset_dir.as_dvec3();
    for mut body in &mut query {
        *body.position_mut() += offset;
    }
}

fn setup_camera(
    mut commands: Commands,
) {
    commands.spawn((
        Camera3d::default(),
        MainCamera,
        Transform::from_translation(Vec3::new(0.0, 144.0, 384.0))
            .looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn toggle_mouse_control(
    keys: Res<ButtonInput<KeyCode>>,
    mut cursor_query: Query<&mut CursorOptions>,
    mut camera_input: ResMut<CameraInputEnabled>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        for mut cursor in cursor_query.iter_mut() {
            if cursor.visible {
                // Switch to camera mode: hide cursor, lock grab, enable camera
                cursor.visible = false;
                cursor.grab_mode = CursorGrabMode::Locked;
                camera_input.0 = true;
            } else {
                // Switch to UI mode: show cursor, release grab, disable camera
                cursor.visible = true;
                cursor.grab_mode = CursorGrabMode::None;
                camera_input.0 = false;
            }
        }
    }
}

fn update_camera(
    mut camera: Single<&mut Transform, With<MainCamera>>,
    mut camera_settings: ResMut<CameraSettings>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    mouse_scroll: Res<AccumulatedMouseScroll>,
    auto_orbit: Res<AutoOrbit>,
    camera_input: Res<CameraInputEnabled>,
    cursor_query: Query<&CursorOptions>,
) {
    // Zoom: scroll wheel adjusts orbit distance regardless of mouse lock state,
    // but still blocked when pointer is over egui (camera_input handles that).
    // Allow zoom when: camera_input is enabled, OR cursor is locked (mouse captured).
    let cursor_locked = cursor_query.iter().any(|c| c.grab_mode == CursorGrabMode::Locked);
    let zoom_allowed = !auto_orbit.active && (camera_input.enabled() || cursor_locked);

    let zoomed = if zoom_allowed && mouse_scroll.delta.y != 0.0 {
        let delta_zoom = 1.0 - mouse_scroll.delta.y * camera_settings.zoom_speed;
        camera_settings.orbit_distance = (camera_settings.orbit_distance * delta_zoom).clamp(
            camera_settings.orbit_distance_range.start,
            camera_settings.orbit_distance_range.end,
        );
        true
    } else {
        false
    };

    if !camera_input.enabled() {
        // Still update camera position if zoom changed while input is disabled
        if zoomed {
            let target = Vec3::ZERO;
            camera.translation = target - camera.forward() * camera_settings.orbit_distance;
        }
        return;
    }
    // When auto-orbiting, skip manual mouse controls
    if auto_orbit.active {
        return;
    }

    // Orbit: mouse motion adjusts pitch and yaw
    let (delta_pitch, delta_yaw) = {
        let delta = -mouse_motion.delta;
        (
            delta.y * camera_settings.pitch_speed,
            delta.x * camera_settings.yaw_speed,
        )
    };

    let (yaw, pitch, roll) = camera.rotation.to_euler(EulerRot::YXZ);

    let pitch = (pitch + delta_pitch).clamp(
        camera_settings.pitch_range.start,
        camera_settings.pitch_range.end,
    );
    let yaw = yaw + delta_yaw;
    camera.rotation = Quat::from_euler(EulerRot::YXZ, yaw, pitch, roll);

    let target = Vec3::ZERO;
    camera.translation = target - camera.forward() * camera_settings.orbit_distance;
}

/// Toggle auto-orbit mode with KeyC.
fn toggle_auto_orbit(
    keys: Res<ButtonInput<KeyCode>>,
    mut auto_orbit: ResMut<AutoOrbit>,
) {
    if keys.just_pressed(KeyCode::KeyC) {
        auto_orbit.active = !auto_orbit.active;
    }
}

/// Cancel auto-orbit when user provides any camera input (mouse move, scroll, or pan keys).
fn cancel_auto_orbit_on_input(
    mouse_motion: Res<AccumulatedMouseMotion>,
    mouse_scroll: Res<AccumulatedMouseScroll>,
    keys: Res<ButtonInput<KeyCode>>,
    mut auto_orbit: ResMut<AutoOrbit>,
    camera_input: Res<CameraInputEnabled>,
) {
    if !camera_input.enabled() { return; }
    if !auto_orbit.active {
        return;
    }

    let has_mouse_input = mouse_motion.delta != Vec2::ZERO || mouse_scroll.delta != Vec2::ZERO;
    let has_pan_input = keys.pressed(KeyCode::KeyW)
        || keys.pressed(KeyCode::KeyA)
        || keys.pressed(KeyCode::KeyS)
        || keys.pressed(KeyCode::KeyD)
        || keys.pressed(KeyCode::KeyQ)
        || keys.pressed(KeyCode::KeyE);

    if has_mouse_input || has_pan_input {
        auto_orbit.active = false;
    }
}

/// When auto-orbit is active, rotate yaw at 1 revolution/minute and slerp zoom to default.
fn auto_orbit_camera(
    mut camera: Single<&mut Transform, With<MainCamera>>,
    camera_settings: Res<CameraSettings>,
    auto_orbit: Res<AutoOrbit>,
    time: Res<Time>,
    camera_input: Res<CameraInputEnabled>,
) {
    if !camera_input.enabled() { return; }
    if !auto_orbit.active {
        return;
    }

    let dt = time.delta_secs();

    // One full revolution per 300 seconds (5 minutes)
    let yaw_per_sec = TAU / 300.0;
    let (yaw, pitch, roll) = camera.rotation.to_euler(EulerRot::YXZ);
    let new_yaw = yaw + yaw_per_sec * dt;
    camera.rotation = Quat::from_euler(EulerRot::YXZ, new_yaw, pitch, roll);

    let target = Vec3::ZERO;
    camera.translation = target - camera.forward() * camera_settings.orbit_distance;
}

fn looking_axis(camera: Single<'_, '_, &GlobalTransform, With<MainCamera>>) -> (Dir3, Dir3) {
    (
        snap_to(camera.forward()),
        snap_to(camera.right()),
    )
}

fn snap_to(real: Dir3) -> Dir3 {
    let d = real.x.abs() - real.z.abs();
    let x = d.signum().max(0.0) * real.x.signum();
    let z = (1.0 - x) * real.z.signum();
    Dir3::from_xyz_unchecked(x, 0.0, z)
}
