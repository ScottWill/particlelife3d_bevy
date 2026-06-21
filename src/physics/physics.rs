use bevy::input::common_conditions::{input_just_pressed, input_pressed};
use bevy::math::DVec3;
use bevy::prelude::*;
use rayon::prelude::*;
use std::time::Instant;

use crate::physics::bodies::BodyPlugin;
use crate::{next_state, debug::DebugDurations, traits::NextVariant, translate};
use super::bodies::{BodySnapshot, PointColor, PointPosition, PointVelocity};
use super::forces::{ForceMatrix, ForceMatrixPlugin};
use super::islands::{
    BodySnapshots, IslandGrid, IslandNeighborhoods, IslandsPlugin,
    assign_islands, build_neighborhoods, clear_islands, get_island_ix,
};

const MAX_DIST: f64 = 0.045;
const MIN_REL_DIST: f64 = 1.0 / 3.0;
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

/// Holds computed forces per body for the current physics tick.
#[derive(Default, Resource)]
struct ParticleForces(Vec<DVec3>);

/// When set, the physics pipeline runs once then returns to Paused.
#[derive(Default, Resource)]
struct StepOnce(bool);

/// SystemSet for island computation. Runs in FixedUpdate before physics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
struct IslandSet;

/// SystemSet for the physics pipeline (snapshot, forces, apply).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
struct PhysicsSet;

pub struct ParticlePhysicsPlugin;

impl Plugin for ParticlePhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            BodyPlugin,
            ForceMatrixPlugin,
            IslandsPlugin,
        ));
        app.init_state::<PhysicsRunState>();
        app.init_resource::<ParticleForces>();
        app.init_resource::<StepOnce>();

        // Fixed physics timestep at 1/240s
        // app.insert_resource(Time::<Fixed>::from_hz(240.0));

        app.add_systems(Update, (
            next_state::<PhysicsRunState>.run_if(input_just_pressed(KeyCode::Enter)),
            trigger_step.run_if(input_pressed(KeyCode::Space)),
        ));

        app.configure_sets(FixedUpdate, IslandSet.before(PhysicsSet));

        app.add_systems(FixedUpdate, (
            clear_islands,
            snapshot_bodies,
            assign_islands,
            build_neighborhoods,
        ).chain().in_set(IslandSet).run_if(
            in_state(PhysicsRunState::Running)
        ));

        app.add_systems(FixedUpdate, (
            compute_forces,
            apply_forces,
            finish_step,
        ).chain().in_set(PhysicsSet).run_if(
            in_state(PhysicsRunState::Running)
        ));

        app.add_systems(FixedPostUpdate, translate_bodies);
    }
}

/// Pressing Space sets the state to Running with a step-once flag.
fn trigger_step(
    mut next_state: ResMut<NextState<PhysicsRunState>>,
    mut step_once: ResMut<StepOnce>,
) {
    step_once.0 = true;
    next_state.set(PhysicsRunState::Running);
}

/// After a step-once tick completes, return to Paused.
fn finish_step(
    mut next_state: ResMut<NextState<PhysicsRunState>>,
    mut step_once: ResMut<StepOnce>,
) {
    if step_once.0 {
        step_once.0 = false;
        next_state.set(PhysicsRunState::Paused);
    }
}

/// Collect body snapshots into the shared resource.
fn snapshot_bodies(
    mut snapshots: ResMut<BodySnapshots>,
    query: Query<(&PointColor, &PointPosition)>,
) {
    snapshots.0.clear();
    for (color, position) in query.iter() {
        snapshots.0.push(BodySnapshot {
            color: color.0,
            position: position.0,
        });
    }
}

/// Compute inter-particle forces using islands and the force matrix.
fn compute_forces(
    mut debug_info: ResMut<DebugDurations>,
    mut forces: ResMut<ParticleForces>,
    snapshots: Res<BodySnapshots>,
    neighborhoods: Res<IslandNeighborhoods>,
    grid: Res<IslandGrid>,
    force_matrix: Res<ForceMatrix>,
) {
    if snapshots.0.is_empty() { return }

    let now = Instant::now();

    snapshots.0
        .par_iter()
        .enumerate()
        .map(|(ix, body0)| {
            let island_ix = get_island_ix(body0.position, &grid);
            let mut total_force = DVec3::ZERO;
            if let Some(neighborhood) = neighborhoods.0.get(island_ix) {
                for &jx in neighborhood {
                    if ix == jx { continue }
                    total_force += get_force(body0, &snapshots.0[jx], &force_matrix);
                }
            }
            total_force
        })
        .collect_into_vec(&mut forces.0);

    debug_info.add("forces", now.elapsed());
}

/// Apply forces, drag, and velocity integration to body positions.
fn apply_forces(
    mut debug_info: ResMut<DebugDurations>,
    mut query: Query<(&mut PointVelocity, &mut PointPosition)>,
    forces: Res<ParticleForces>,
    time: Res<Time>,
) {
    const DRAG_HALFLIFE: f64 = 1.0 / 0.043;

    if forces.0.is_empty() { return }

    let dt = time.delta_secs_f64();
    if dt == 0.0 { return }

    let now = Instant::now();

    // DO NOT change these nested loop patterns, it is more performant than a single iter_mut!
    for (mut velocities, positions) in query.contiguous_iter_mut().unwrap() {
        for (i, (velocity, position)) in velocities.iter_mut().zip(positions).enumerate() {
            let force = forces.0[i];
            **velocity *= 0.5f64.powf(DRAG_HALFLIFE * dt);
            **velocity += force * dt;
            **position += **velocity * dt;
        }
    }

    debug_info.add("stepping", now.elapsed());
}

fn translate_bodies(
    mut query: Query<(&mut Transform, &mut PointPosition)>,
) {
    for (mut transform, mut position) in &mut query {
        **position = position.rem_euclid(DVec3::ONE);
        transform.translation = translate(**position);
    }
}

#[inline]
fn get_force(body0: &BodySnapshot, body1: &BodySnapshot, forces: &ForceMatrix) -> DVec3 {
    let min_pos = (body1.position - body0.position + 0.5).rem_euclid(DVec3::ONE) - 0.5;
    let dist_sqrd = min_pos.length_squared();
    if dist_sqrd > MAX_DIST_SQRD || dist_sqrd < 1e-30 {
        return DVec3::ZERO;
    }

    let dist = dist_sqrd.sqrt();
    let dist_recip = dist.recip();
    let rel_dist = dist * MAX_DIST_RECIP;
    let dir = min_pos * dist_recip;

    let force = if rel_dist <= MIN_REL_DIST {
        rel_dist * MIN_DIST_RECIP - 1.0
    } else {
        let f = forces[(body0.color, body1.color)];
        if f == 0.0 { return DVec3::ZERO }
        f * (1.0 - (1.0 + MIN_REL_DIST - 2.0 * rel_dist) * INV_MIN_DIST_RECIP)
    };

    dir * (force * MAX_DIST)
}
