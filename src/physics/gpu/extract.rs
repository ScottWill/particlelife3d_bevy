//! Extract systems for copying main world data to render world.

use bevy::prelude::*;
use bevy::render::Extract;

use crate::physics::backend::ForceBackend;
use crate::physics::bodies::BodySnapshot;
use crate::physics::forces::ForceMatrix;
use crate::physics::islands::BodySnapshots;
use crate::physics::DensityAttenuation;
use crate::settings_panel::SimulationConfig;

/// Render-world resource holding extracted particle snapshots.
#[derive(Resource, Clone)]
pub struct ExtractedSnapshots(pub Vec<BodySnapshot>);

/// Render-world resource holding extracted simulation configuration.
#[derive(Resource, Clone)]
pub struct ExtractedConfig {
    pub max_dist: f64,
    pub min_rel_dist: f64,
    pub density_limit: f64,
    pub density_same_color: f64,
    pub density_diff_color: f64,
    #[allow(dead_code)]
    pub color_count: usize,
}

/// Render-world resource holding the extracted force matrix.
#[derive(Resource, Clone)]
pub struct ExtractedForceMatrix {
    pub data: Vec<f64>,
    pub color_count: usize,
}

/// Render-world resource holding the extracted density attenuation flag.
#[derive(Resource, Clone, Copy)]
pub struct ExtractedAttenuation(pub bool);

/// Extracts particle data from the main world into the render world when GPU backend is active.
///
/// Skips extraction entirely when the backend is set to CPU.
pub fn extract_particle_data(
    mut commands: Commands,
    snapshots: Extract<Res<BodySnapshots>>,
    config: Extract<Res<SimulationConfig>>,
    force_matrix: Extract<Res<ForceMatrix>>,
    attenuation: Extract<Res<DensityAttenuation>>,
    backend: Extract<Res<ForceBackend>>,
) {
    if **backend != ForceBackend::Gpu {
        return;
    }

    commands.insert_resource(ExtractedSnapshots(snapshots.0.clone()));

    commands.insert_resource(ExtractedConfig {
        max_dist: config.max_dist,
        min_rel_dist: config.min_rel_dist,
        density_limit: config.density_limit,
        density_same_color: config.density_same_color,
        density_diff_color: config.density_diff_color,
        color_count: config.color_count,
    });

    commands.insert_resource(ExtractedForceMatrix {
        data: force_matrix.data.clone(),
        color_count: force_matrix.color_count,
    });

    commands.insert_resource(ExtractedAttenuation(attenuation.0));
}
