use bevy::input::common_conditions::{input_just_pressed, input_pressed};
use bevy::math::DVec3;
use bevy::prelude::*;
use rayon::prelude::*;
use std::time::Instant;
use std::ops::AddAssign;

use crate::physics::bodies::BodyPlugin;
use crate::settings_panel::SimulationConfig;
use crate::{next_state, debug::DebugDurations, traits::NextVariant, translate};
use super::backend::ForceBackend;
use super::bodies::{BodySnapshot, PointColor, PointPosition, PointVelocity};
use super::forces::{ForceMatrix, ForceMatrixPlugin};
use super::gpu::{poll_gpu_readback, GpuComputeResults, GpuForcePlugin, check_gpu_availability};
use super::islands::{
    BodySnapshots, IslandGrid, IslandNeighborhoods, IslandsPlugin,
    assign_islands, build_neighborhoods, clear_islands, get_island_ix,
};

/// Pre-computed physics parameters derived from `SimulationConfig` each tick.
struct PhysicsParams {
    max_dist: f64,
    min_rel_dist: f64,
    max_dist_recip: f64,
    max_dist_sqrd: f64,
    min_dist_recip: f64,
    inv_min_dist_recip: f64,
    density_limit: f64,
    density_same_color: f64,
    density_diff_color: f64,
}

impl PhysicsParams {
    fn from_config(config: &SimulationConfig) -> Self {
        Self {
            max_dist: config.max_dist,
            min_rel_dist: config.min_rel_dist,
            max_dist_recip: config.max_dist.recip(),
            max_dist_sqrd: config.max_dist * config.max_dist,
            min_dist_recip: config.min_rel_dist.recip(),
            inv_min_dist_recip: (1.0 - config.min_rel_dist).recip(),
            density_limit: config.density_limit,
            density_same_color: config.density_same_color,
            density_diff_color: config.density_diff_color,
        }
    }
}

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

/// When enabled, attractive forces are attenuated in high-density regions.
#[derive(Resource)]
pub struct DensityAttenuation(pub bool);

impl Default for DensityAttenuation {
    fn default() -> Self { Self(true) }
}

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
            GpuForcePlugin,
        ));
        app.init_state::<PhysicsRunState>();
        app.init_resource::<ParticleComputations>();
        app.init_resource::<StepOnce>();
        app.init_resource::<DensityAttenuation>();
        app.init_resource::<ForceBackend>();

        // Fixed physics timestep at 1/240s
        // app.insert_resource(Time::<Fixed>::from_hz(240.0));

        app.add_systems(Update, (
            check_gpu_availability,
            next_state::<PhysicsRunState>.run_if(input_just_pressed(KeyCode::Enter)),
            trigger_step.run_if(input_pressed(KeyCode::Space)),
            toggle_density_attenuation.run_if(input_just_pressed(KeyCode::F2)),
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
            poll_gpu_readback,
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

/// Toggle density-based force attenuation on/off.
fn toggle_density_attenuation(mut attenuation: ResMut<DensityAttenuation>) {
    attenuation.0 = !attenuation.0;
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
///
/// When `ForceBackend::Gpu` is active and GPU results are ready, uses the pre-computed
/// results from the readback system. Otherwise, falls back to the CPU Rayon path.
#[allow(clippy::too_many_arguments)]
fn compute_forces(
    backend: Res<ForceBackend>,
    config: Res<SimulationConfig>,
    mut computations: ResMut<ParticleComputations>,
    mut debug_info: ResMut<DebugDurations>,
    force_matrix: Res<ForceMatrix>,
    grid: Res<IslandGrid>,
    neighborhoods: Res<IslandNeighborhoods>,
    snapshots: Res<BodySnapshots>,
    attenuation: Res<DensityAttenuation>,
    gpu_results: Option<Res<GpuComputeResults>>,
) {
    if snapshots.0.is_empty() { return }

    let now = Instant::now();

    match *backend {
        ForceBackend::Gpu => {
            if let Some(ref results) = gpu_results
                && results.ready && results.forces.len() == snapshots.0.len()
            {
                // Use GPU results: copy force and density into ParticleComputations
                computations.0.clear();
                computations.0.extend(
                    results.forces.iter().zip(results.densities.iter()).map(|(&force, &density)| {
                        ParticleComputation { force, density }
                    })
                );
                debug_info.add("forces", now.elapsed());
                return;
            }
            // Fallback: GPU results not ready, run CPU path
            run_cpu_forces(
                &config, &mut computations, &force_matrix,
                &grid, &neighborhoods, &snapshots, &attenuation,
            );
        }
        ForceBackend::Cpu => {
            run_cpu_forces(
                &config, &mut computations, &force_matrix,
                &grid, &neighborhoods, &snapshots, &attenuation,
            );
        }
    }

    debug_info.add("forces", now.elapsed());
}

/// CPU fallback: compute forces in parallel using Rayon and the island grid.
///
/// Extracted from `compute_forces` so both the GPU-fallback and CPU-only paths
/// can share the same logic without duplication.
fn run_cpu_forces(
    config: &SimulationConfig,
    computations: &mut ParticleComputations,
    force_matrix: &ForceMatrix,
    grid: &IslandGrid,
    neighborhoods: &IslandNeighborhoods,
    snapshots: &BodySnapshots,
    attenuation: &DensityAttenuation,
) {
    let params = PhysicsParams::from_config(config);
    let use_attenuation = attenuation.0;

    // Snapshot previous-tick densities so we can write into computations without aliasing.
    let prev_densities = computations.0
        .iter()
        .map(|c| c.density)
        .collect::<Vec<_>>();

    snapshots.0
        .par_iter()
        .enumerate()
        .map(|(ix, body0)| {
            let density_factor = if use_attenuation {
                let density = prev_densities.get(ix).copied().unwrap_or_default();
                1.0 - (density - params.density_limit).clamp(0.0, 1.0)
            } else {
                1.0
            };

            let island_ix = get_island_ix(body0.position, &grid);
            let mut total_computation = ParticleComputation::default();

            if let Some(neighborhood) = neighborhoods.0.get(island_ix) {
                for &jx in neighborhood {
                    if ix == jx { continue }
                    total_computation += get_computation(body0, &snapshots.0[jx], &force_matrix, density_factor, &params);
                }
            }
            total_computation
        })
        .collect_into_vec(&mut computations.0);
}

/// Apply forces, drag, and velocity integration to body positions.
fn apply_forces(
    config: Res<SimulationConfig>,
    mut debug_info: ResMut<DebugDurations>,
    mut query: Query<(&mut PointVelocity, &mut PointPosition)>,
    computations: Res<ParticleComputations>,
    time: Res<Time>,
) {
    if computations.0.is_empty() { return }

    let dt = time.delta_secs_f64();
    if dt == 0.0 { return }

    let now = Instant::now();
    let drag_halflife_recip = config.drag_halflife.recip();

    // DO NOT change these nested loop patterns, it is more performant than a single iter_mut!
    for (mut velocities, positions) in query.contiguous_iter_mut().unwrap() {
        for (i, (velocity, position)) in velocities.iter_mut().zip(positions).enumerate() {
            let force = computations.0[i].force;
            **velocity *= 0.5f64.powf(drag_halflife_recip * dt);
            **velocity += force * dt;
            **position += **velocity * dt;
        }
    }

    debug_info.add("stepping", now.elapsed());
}

fn translate_bodies(
    mut query: Query<(&mut Transform, &mut PointPosition)>,
    config: Res<SimulationConfig>,
) {
    for (mut transform, mut position) in &mut query {
        **position = position.rem_euclid(DVec3::ONE);
        transform.translation = translate(**position, config.world_scale);
    }
}

#[inline]
fn get_computation(body0: &BodySnapshot, body1: &BodySnapshot, forces: &ForceMatrix, density_factor: f64, params: &PhysicsParams) -> ParticleComputation {
    let min_pos = (body1.position - body0.position + 0.5).rem_euclid(DVec3::ONE) - 0.5;
    let dist_sqrd = min_pos.length_squared();
    if dist_sqrd > params.max_dist_sqrd || dist_sqrd < 1e-30 {
        return ParticleComputation::ZERO;
    }

    let dist = dist_sqrd.sqrt();
    let dist_recip = dist.recip();
    let rel_dist = dist * params.max_dist_recip;
    let dir = min_pos * dist_recip;

    let force = if rel_dist <= params.min_rel_dist {
        rel_dist * params.min_dist_recip - 1.0
    } else {
        let f = forces[(body0.color, body1.color)];
        if f == 0.0 { return ParticleComputation::ZERO }
        // Attenuate attraction when local density exceeds the limit.
        let f = if f > 0.0 { f * density_factor } else { f };
        f * (1.0 - (1.0 + params.min_rel_dist - 2.0 * rel_dist) * params.inv_min_dist_recip)
    };

    let weight = if body0.color == body1.color {
        params.density_same_color
    } else {
        params.density_diff_color
    };

    ParticleComputation {
        force: dir * (force * params.max_dist),
        density: weight * (1.0 - rel_dist),
    }
}
