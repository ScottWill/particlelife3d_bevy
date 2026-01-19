use bevy::input::common_conditions::{input_just_pressed, input_pressed};
use bevy::prelude::*;
use glam::DVec3;
use rayon::prelude::*;
use std::time::Instant;

use crate::physics::bodies::PointBodyIndex;
use crate::{next_state, debug::DebugDurations, traits::NextVariant, translate};
use super::{bodies::PointBody, forces::{ForceMatrix, ForceMatrixPlugin}, islands::IslandManager};

const MAX_DIST: f64 = 0.045; // The maximum distance that a particle can interact with another
const MIN_REL_DIST: f64 = 1.0 / 3.0; // The minimum relative distance that two particles can interact with
const MAX_DIST_RECIP: f64 = 1.0 / MAX_DIST;
const MAX_DIST_SQRD: f64 = MAX_DIST * MAX_DIST;
const MIN_DIST_RECIP: f64 = 1.0 / MIN_REL_DIST;
const INV_MIN_DIST_RECIP: f64 = 1.0 / (1.0 - MIN_REL_DIST);

#[derive(Debug, Default, Clone, Copy, Eq, Hash, PartialEq, States)]
enum PhysicsRunState {
    Running,
    #[default]
    Paused
}

impl NextVariant for PhysicsRunState {
    fn next(&self) -> Self {
        match self {
            PhysicsRunState::Running => PhysicsRunState::Paused,
            PhysicsRunState::Paused => PhysicsRunState::Running,
        }
    }
}

pub struct ParticlePhysicsPlugin;

impl Plugin for ParticlePhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ForceMatrixPlugin);
        app.init_state::<PhysicsRunState>();
        app.init_resource::<ParticlePhysics>();
        app.add_systems(Update, (
            next_state::<PhysicsRunState>.run_if(input_just_pressed(KeyCode::Enter)),
            step_bodies.run_if(input_pressed(KeyCode::Space)),
            update_bodies.run_if(in_state(PhysicsRunState::Running)),
        ));
        app.add_systems(PostUpdate, translate_bodies);
    }
}

fn step_bodies(
    mut bodies: Local<Vec<PointBody>>,
    mut debug_info: ResMut<DebugDurations>,
    mut physics: ResMut<ParticlePhysics>,
    mut query: Query<&mut PointBody>,
    mut next_state: ResMut<NextState<PhysicsRunState>>,
    force_matrix: Res<ForceMatrix>,
) {
    next_state.set(PhysicsRunState::Paused);
    physics_step(
        &mut bodies,
        &mut debug_info,
        &mut physics,
        &mut query,
        &force_matrix,
        1.0 / 480.0,
    );
}

fn update_bodies(
    mut bodies: Local<Vec<PointBody>>,
    mut debug_info: ResMut<DebugDurations>,
    mut physics: ResMut<ParticlePhysics>,
    mut query: Query<&mut PointBody>,
    force_matrix: Res<ForceMatrix>,
    time: Res<Time<Virtual>>,
) {
    physics_step(
        &mut bodies,
        &mut debug_info,
        &mut physics,
        &mut query,
        &force_matrix,
        time.delta_secs_f64(),
    );
}

fn physics_step(
    bodies: &mut Vec<PointBody>,
    debug_info: &mut DebugDurations,
    physics: &mut ParticlePhysics,
    query: &mut Query<&mut PointBody>,
    force_matrix: &ForceMatrix,
    dt: f64,
) {

    bodies.clear();

    for body in query.iter_mut() {
        bodies.push(*body);
    }

    if bodies.is_empty() { return }

    let forces = physics.get_forces(bodies.as_slice(), force_matrix, debug_info);

    for (i, mut body) in query.iter_mut().enumerate() {
        body.step(forces[i], dt);
    }

}

fn translate_bodies(
    mut query: Query<(&mut Transform, &mut PointBody)>,
) {
    for (mut transform, mut body) in &mut query {
        body.position = body.position.rem_euclid(DVec3::ONE);
        transform.translation = translate(body.position);
    }
}

#[derive(Resource)]
pub struct ParticlePhysics {
    forces: Vec<DVec3>,
    islands: IslandManager,
}

impl Default for ParticlePhysics {
    fn default() -> Self {
        Self {
            forces: Vec::default(),
            islands: IslandManager::new(MAX_DIST),
        }
    }
}

impl ParticlePhysics {

    pub fn get_forces(&mut self, bodies: &[PointBody], force_matrix: &ForceMatrix, durations: &mut DebugDurations) -> &[DVec3] {
        // bucket bodies, broad phase

        self.islands.index_positions(&bodies, durations);

        let now = Instant::now();

        bodies
            .par_iter()
            .enumerate()
            .map(|(ix, body0)| {
                let mut total_force = DVec3::ZERO;
                if let Some(neighborhood) = self.islands.get_neighboring_ixs(body0.position) {
                    for &jx in neighborhood {
                        if ix == jx { continue }
                        total_force += get_force(body0, &bodies[jx], force_matrix);
                    }
                }
                total_force
            })
            .collect_into_vec(&mut self.forces);

        durations.add("forces", now.elapsed());

        &self.forces
    }

}

fn get_force(body0: &PointBody, body1: &PointBody, forces: &ForceMatrix) -> DVec3 {
    // shortest distance in wrapped toroidal space
    let min_pos = (body1.position - body0.position + 0.5).rem_euclid(DVec3::ONE) - 0.5;
    if min_pos.length_squared() > MAX_DIST_SQRD {
        return DVec3::ZERO;
    }

    let pos = min_pos * MAX_DIST_RECIP;
    let dist = pos.length();

    let force;
    if dist <= MIN_REL_DIST {
        force = dist * MIN_DIST_RECIP - 1.0;
    } else {
        let f = forces.get_force(body0.color, body1.color);
        if f == 0.0 { return DVec3::ZERO }
        force = f * (1.0 - (1.0 + MIN_REL_DIST - 2.0 * dist) * INV_MIN_DIST_RECIP);
    };

    force / dist * MAX_DIST * pos
}
