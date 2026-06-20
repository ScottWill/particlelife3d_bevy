use std::marker::PhantomData;
use std::{f32::consts::FRAC_PI_2, ops::Range};

use bevy::ecs::component::Mutable;
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::input::{common_conditions::input_pressed};
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};

#[derive(Default)]
pub struct CameraPlugin<C> {
    _phantom: PhantomData<C>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, SystemSet)]
struct PanSet;

impl<C: Component<Mutability = Mutable> + Position> Plugin for CameraPlugin<C> {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraSettings>();
        app.add_systems(Startup, setup_camera);
        app.add_systems(Update, (
            update_camera.after(PanSet),
            (
                pan_bodies::<C,  0,  0, -1>.run_if(input_pressed(KeyCode::KeyS)),
                pan_bodies::<C,  0,  0,  1>.run_if(input_pressed(KeyCode::KeyW)),
                pan_bodies::<C,  0,  1,  0>.run_if(input_pressed(KeyCode::KeyQ)),
                pan_bodies::<C,  0, -1,  0>.run_if(input_pressed(KeyCode::KeyE)),
                pan_bodies::<C,  1,  0,  0>.run_if(input_pressed(KeyCode::KeyD)),
                pan_bodies::<C, -1,  0,  0>.run_if(input_pressed(KeyCode::KeyA)),
            ).in_set(PanSet),
        ));
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

impl Default for CameraSettings {
    fn default() -> Self {
        let pitch_limit = FRAC_PI_2 - 0.01;
        Self {
            orbit_distance: 128.0,
            orbit_distance_range: 10.0..500.0,
            zoom_speed: 0.0025,
            pitch_speed: 0.003,
            pitch_range: -pitch_limit..pitch_limit,
            yaw_speed: 0.004,
        }
    }
}

pub trait Position {
    #[allow(dead_code)]
    fn position(&self) -> &DVec3;
    fn position_mut(&mut self) -> &mut DVec3;
}

fn pan_bodies<
    C,
    const X: i8,
    const Y: i8,
    const Z: i8,
>(
    mut query: Query<&mut C>,
    camera: Single<&GlobalTransform, With<MainCamera>>,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
)
where
    C: Component<Mutability = Mutable> + Position
{
    let (forward, right) = looking_axis(camera);
    // Build the offset: X maps to camera-right, Z maps to camera-forward, Y stays world-up
    let input = DVec3::new(X as f64, Y as f64, Z as f64);
    let offset_dir = right * input.x + DVec3::Y * input.y + forward * input.z;
    let factor = if keys.pressed(KeyCode::ShiftLeft) { 0.25 } else { 0.1 };
    let offset = factor * time.delta_secs_f64() * offset_dir;
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
        Transform::from_translation(Vec3::new(0.0, 48.0, 128.0)).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn update_camera(
    mut camera: Single<&mut Transform, With<MainCamera>>,
    mut camera_settings: ResMut<CameraSettings>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    mouse_scroll: Res<AccumulatedMouseScroll>,
) {
    // Zoom: scroll wheel adjusts orbit distance logarithmically
    let delta_zoom = 1.0 - mouse_scroll.delta.y * camera_settings.zoom_speed;
    camera_settings.orbit_distance = (camera_settings.orbit_distance * delta_zoom).clamp(
        camera_settings.orbit_distance_range.start,
        camera_settings.orbit_distance_range.end,
    );

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

fn looking_axis(camera: Single<'_, '_, &GlobalTransform, With<MainCamera>>) -> (DVec3, DVec3) {
    (
        snap_to(*camera.forward()).as_dvec3(),
        snap_to(*camera.right()).as_dvec3(),
    )
}

fn snap_to(real: Vec3) -> Vec3 {
    let d = real.x.abs() - real.z.abs();
    let x = d.signum().max(0.0);
    let z = 1.0 - x;
    Vec3::new(x, 0.0, z) * real.signum()
}