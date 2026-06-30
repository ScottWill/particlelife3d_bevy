use bevy::ecs::schedule::common_conditions::on_message;
use bevy::input::common_conditions::{input_just_pressed, input_pressed};
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::state::state::FreelyMutableState;
use rand::{RngExt as _, SeedableRng as _};
use rand_chacha::ChaCha12Rng;

use crate::camera::CameraPlugin;
use crate::palette::{PalettePlugin};
use crate::physics::ParticlePhysicsPlugin;
use crate::physics::{PointBody, PointPosition};
use crate::positioners::{CurrentPositioner, PositionerPlugin, get_position};
use crate::settings_panel::{SettingsPanelPlugin, SimulationConfig};
use crate::traits::{Fullscreen as _, NextVariant, PrevVariant};

mod camera;
mod debug;
mod palette;
mod physics;
mod positioners;
mod settings_panel;
mod traits;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowPlugin::fullscreen()),
            CameraPlugin::<PointPosition>::default(),
            PalettePlugin,
            ParticlePhysicsPlugin,
            PositionerPlugin,
            SettingsPanelPlugin,
        ))
        .add_message::<UpdateBodies>()
        .add_systems(Startup, setup)
        .add_systems(Update, (
            match_body_count.run_if(on_message::<UpdateBodies>).after(reset_bodies),
            reset_bodies.run_if(
                input_just_pressed(KeyCode::KeyR)
                    .and_then(input_pressed(KeyCode::SuperLeft))
            ),
        ))
        .run();
}

#[derive(Message)]
pub struct UpdateBodies;

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
    messages.write(UpdateBodies);
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
    query: Query<Entity, With<PointBody>>,
    mesh: Res<SphereHandle>,
    positioner: Res<CurrentPositioner>,
    config: Res<SimulationConfig>,
) {
    let target = config.particle_count;
    let mut current_size = query.count();

    if target > current_size {
        build_batch(
            &mut commands,
            &mesh,
            target - current_size,
            positioner.0,
            config.world_scale,
        );
        return
    }

    let mut rng = rand::rng();
    while current_size > target {
        let rix = rng.random::<u64>() as usize % current_size;
        if let Some(entity) = query.iter().nth(rix) {
            commands.entity(entity).despawn();
            current_size -= 1;
        } else {
            panic!("stuck!");
        }
    }
}

fn build_batch(
    commands: &mut Commands,
    mesh: &Handle<Mesh>,
    count: usize,
    pos_type: crate::positioners::PositionerType,
    scale: f64,
) {
    // let mut rng = rand::rng();
    let mut rng = ChaCha12Rng::seed_from_u64(42);
    let mut batch = Vec::with_capacity(count);
    for _ in 0..count {
        let position = get_position(&mut rng, pos_type);
        batch.push((
            PointBody,
            PointPosition(position),
            Mesh3d(mesh.clone()),
            Transform::from_translation(translate(position, scale)),
        ));
    }

    commands.spawn_batch(batch);
}

#[inline]
pub fn translate(pos: DVec3, scale: f64) -> Vec3 {
    ((pos - 0.5) * scale).as_vec3()
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