use bevy::ecs::schedule::common_conditions::on_message;
use bevy::input::common_conditions::{input_just_pressed, input_pressed};
use bevy::prelude::*;
use bevy::state::state::FreelyMutableState;
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin};
use glam::DVec3;
use rand::Rng as _;

use crate::config::BODIES;
use crate::debug::DebugPlugin;
use crate::palette::{Palette, PalettePlugin};
use crate::physics::ParticlePhysicsPlugin;
use crate::physics::PointBody;
use crate::positioners::{PositionerType, get_position};
use crate::traits::{Fullscreen as _, NextVariant, NoPan as _, PrevVariant};

const SCALE: f64 = 128.0;

mod config;
mod debug;
mod palette;
mod physics;
mod positioners;
mod traits;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowPlugin::fullscreen()),
            DebugPlugin,
            PalettePlugin,
            PanOrbitCameraPlugin,
            ParticlePhysicsPlugin,
        ))
        .add_message::<UpdateBodies>()
        .add_systems(Startup, setup)
        .add_systems(Update, (
            match_body_count.run_if(on_message::<UpdateBodies>).after(reset_bodies),
            reset_bodies.run_if(input_just_pressed(KeyCode::KeyR).and(input_pressed(KeyCode::ShiftLeft))),
            pan_bodies::<-1,  0,  0>.run_if(input_pressed(KeyCode::KeyA)),
            pan_bodies::< 1,  0,  0>.run_if(input_pressed(KeyCode::KeyD)),
            pan_bodies::< 0, -1,  0>.run_if(input_pressed(KeyCode::KeyS)),
            pan_bodies::< 0,  1,  0>.run_if(input_pressed(KeyCode::KeyW)),
            pan_bodies::< 0,  0, -1>.run_if(input_pressed(KeyCode::KeyQ)),
            pan_bodies::< 0,  0,  1>.run_if(input_pressed(KeyCode::KeyE)),
        ))
        .run();
}

#[derive(Message)]
struct UpdateBodies;

#[derive(Deref, Resource)]
struct SphereHandle(Handle<Mesh>);

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut messages: MessageWriter<UpdateBodies>,
) {
    const RADIUS: f32 = 1.0 / 3.0;

    let mesh = meshes.add(Sphere::new(RADIUS));
    commands.insert_resource(SphereHandle(mesh));

    commands.spawn((
        Camera3d::default(),
        PanOrbitCamera::no_pan(),
        Transform::from_translation(Vec3::new(0.0, 48.0, 128.0)).looking_at(Vec3::ZERO, Vec3::Y)
    ));

    messages.write(UpdateBodies);
}

fn pan_bodies<
    const X: i8,
    const Y: i8,
    const Z: i8,
>(
    mut bodies: Query<&mut PointBody>,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
) {
    let factor = if keys.pressed(KeyCode::ShiftLeft) { 0.25 } else { 0.1 };
    let offset = factor * time.delta_secs_f64() * DVec3::new(X as f64, Y as f64, Z as f64);
    for mut body in &mut bodies {
        body.position += offset;
    }
}

fn reset_bodies(
    mut commands: Commands,
    mut messages: MessageWriter<UpdateBodies>,
    query: Query<Entity, With<PointBody>>,
) {
    for entity in query {
        commands.entity(entity).despawn();
    }

    messages.write(UpdateBodies);
}

fn match_body_count(
    mut commands: Commands,
    palette: Res<Palette>,
    query: Query<Entity, With<PointBody>>,
    mesh: Res<SphereHandle>,
) {
    let mut current_size = query.count();

    if BODIES > current_size {
        build_batch(
            &mut commands,
            &palette,
            &mesh,
            BODIES - current_size,
        );
        return
    }

    let mut rng = rand::rng();
    while current_size > BODIES {
        let rix = rng.random::<u64>() as usize % current_size;
        if let Some(entity) = query.iter().nth(rix) {
            commands.entity(entity).despawn();
            current_size -= 1;
        } else {
            panic!("stuck!");
        }
    }
}

#[derive(Bundle)]
struct BodyBundle {
    body: PointBody,
    material: MeshMaterial3d<StandardMaterial>,
    mesh: Mesh3d,
    transform: Transform,
}

fn build_batch(
    commands: &mut Commands,
    palette: &Palette,
    mesh: &Handle<Mesh>,
    count: usize,
) {
    let mut rng = rand::rng();
    let mut batch = Vec::with_capacity(count);
    for _ in 0..count {
        let position = get_position(&mut rng, PositionerType::default());
        let color = palette.random();
        let bundle = BodyBundle {
            body: PointBody::new(color, position),
            material: MeshMaterial3d(palette.get(color).clone()),
            mesh: Mesh3d(mesh.clone()),
            transform: Transform::from_translation(translate(position)),
        };
        batch.push(bundle);
    }

    commands.spawn_batch(batch);
}

#[inline]
pub fn translate(pos: DVec3) -> Vec3 {
    ((pos - 0.5) * SCALE).as_vec3()
}

pub fn next_state<S>(
    state: Res<State<S>>,
    mut next: ResMut<NextState<S>>,
)
where S: NextVariant + FreelyMutableState
{
    next.set(state.next());
}

pub fn prev_state<S>(
    state: Res<State<S>>,
    mut next: ResMut<NextState<S>>,
)
where S: PrevVariant + FreelyMutableState
{
    next.set(state.prev());
}