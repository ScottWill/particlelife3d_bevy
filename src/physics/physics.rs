use bevy::input::common_conditions::{input_just_pressed, input_pressed};
use bevy::math::DVec3;
use bevy::prelude::*;
use rayon::prelude::*;
use std::time::Instant;
use std::ops::AddAssign;

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

/// Density above this threshold starts attenuating attractive forces.
const DENSITY_LIMIT: f64 = 12.0;
/// Density contribution weight for same-color neighbors.
const DENSITY_SAME_COLOR: f64 = 1.0;
/// Density contribution weight for different-color neighbors.
const DENSITY_DIFF_COLOR: f64 = 0.5;

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

#[derive(Default)]
struct ParticleComputation {
    density: f64,
    force: DVec3,
}

impl AddAssign for ParticleComputation {
    fn add_assign(&mut self, rhs: Self) {
        self.density += rhs.density;
        self.force += rhs.force;
    }
}

impl ParticleComputation {
    const ZERO: Self = Self {
        density: 0.0,
        force: DVec3::ZERO,
    };
}

/// Holds computed forces and densities per body for the current physics tick.
#[derive(Default, Resource)]
struct ParticleComputations(Vec<ParticleComputation>);

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
        app.init_resource::<ParticleComputations>();
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
    mut computations: ResMut<ParticleComputations>,
    mut debug_info: ResMut<DebugDurations>,
    force_matrix: Res<ForceMatrix>,
    grid: Res<IslandGrid>,
    neighborhoods: Res<IslandNeighborhoods>,
    snapshots: Res<BodySnapshots>,
) {
    if snapshots.0.is_empty() { return }

    let now = Instant::now();

    // Snapshot previous-tick densities so we can write into computations without aliasing.
    let prev_densities = computations.0
        .iter()
        .map(|c| c.density)
        .collect::<Vec<_>>();

    snapshots.0
        .par_iter()
        .enumerate()
        .map(|(ix, body0)| {
            let density = prev_densities.get(ix).copied().unwrap_or_default();
            let density_factor = 1.0 - (density - DENSITY_LIMIT).clamp(0.0, 1.0);

            let island_ix = get_island_ix(body0.position, &grid);
            // let mut total_force = DVec3::ZERO;
            // let mut total_density = 0.0;
            let mut total_computation = ParticleComputation::default();

            if let Some(neighborhood) = neighborhoods.0.get(island_ix) {
                for &jx in neighborhood {
                    if ix == jx { continue }
                    total_computation += get_computation(body0, &snapshots.0[jx], &force_matrix, density_factor);
                }
            }
            total_computation
        })
        .collect_into_vec(&mut computations.0);

    debug_info.add("forces", now.elapsed());
}

/// Apply forces, drag, and velocity integration to body positions.
fn apply_forces(
    mut debug_info: ResMut<DebugDurations>,
    mut query: Query<(&mut PointVelocity, &mut PointPosition)>,
    computations: Res<ParticleComputations>,
    time: Res<Time>,
) {
    const DRAG_HALFLIFE: f64 = 1.0 / 0.043;

    if computations.0.is_empty() { return }

    let dt = time.delta_secs_f64();
    if dt == 0.0 { return }

    let now = Instant::now();

    // DO NOT change these nested loop patterns, it is more performant than a single iter_mut!
    for (mut velocities, positions) in query.contiguous_iter_mut().unwrap() {
        for (i, (velocity, position)) in velocities.iter_mut().zip(positions).enumerate() {
            let force = computations.0[i].force;
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
fn get_computation(body0: &BodySnapshot, body1: &BodySnapshot, forces: &ForceMatrix, density_factor: f64) -> ParticleComputation {
    let min_pos = (body1.position - body0.position + 0.5).rem_euclid(DVec3::ONE) - 0.5;
    let dist_sqrd = min_pos.length_squared();
    if dist_sqrd > MAX_DIST_SQRD || dist_sqrd < 1e-30 {
        return ParticleComputation::ZERO;
    }

    let dist = dist_sqrd.sqrt();
    let dist_recip = dist.recip();
    let rel_dist = dist * MAX_DIST_RECIP;
    let dir = min_pos * dist_recip;

    let force = if rel_dist <= MIN_REL_DIST {
        rel_dist * MIN_DIST_RECIP - 1.0
    } else {
        let f = forces[(body0.color, body1.color)];
        if f == 0.0 { return ParticleComputation::ZERO }
        // Attenuate attraction when local density exceeds the limit.
        let f = if f > 0.0 { f * density_factor } else { f };
        f * (1.0 - (1.0 + MIN_REL_DIST - 2.0 * rel_dist) * INV_MIN_DIST_RECIP)
    };

    let weight = if body0.color == body1.color {
        DENSITY_SAME_COLOR
    } else {
        DENSITY_DIFF_COLOR
    };

    ParticleComputation {
        force: dir * (force * MAX_DIST),
        density: weight * (1.0 - rel_dist),
    }
}
