use std::time::Instant;

use bevy::math::DVec3;
use bevy::prelude::*;
use rayon::iter::{IndexedParallelIterator as _, IntoParallelRefMutIterator as _, ParallelIterator as _};

use crate::debug::DebugDurations;
use super::bodies::BodySnapshot;

const NEIGHBORS: [[isize; 2]; 9] = [
    [-1, -1], [0, -1], [1, -1],
    [-1,  0], [0,  0], [1,  0],
    [-1,  1], [0,  1], [1,  1],
];

/// The grid cells (islands). Each cell holds the body indices currently in it.
#[derive(Resource)]
pub struct Islands(pub Vec<Vec<usize>>);

/// Pre-cached neighbor indices for each island cell.
#[derive(Resource)]
pub struct IslandNeighborIxs(pub Vec<[usize; 27]>);

/// Per-island aggregated body indices from all neighboring cells.
#[derive(Resource)]
pub struct IslandNeighborhoods(pub Vec<Vec<usize>>);

/// Grid dimension metadata.
#[derive(Resource)]
pub struct IslandGrid {
    pub side: usize,
    pub side_f64: f64,
}

pub struct IslandsPlugin;

impl Plugin for IslandsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BodySnapshots>();
        app.add_systems(Startup, setup_islands);
    }
}

fn setup_islands(
    mut commands: Commands,
) {
    const MAX_DIST: f64 = 0.045;

    let side = MAX_DIST.recip().floor() as usize;
    let size = side * side * side;

    let islands = Islands(vec![vec![]; size]);
    let neighborhoods = IslandNeighborhoods(vec![vec![]; size]);

    // Pre-compute neighbor indices
    let neighbor_ixs = compute_neighbor_ixs(side, size);

    let grid = IslandGrid {
        side,
        side_f64: side as f64,
    };

    commands.insert_resource(islands);
    commands.insert_resource(IslandNeighborIxs(neighbor_ixs));
    commands.insert_resource(neighborhoods);
    commands.insert_resource(grid);
}

fn compute_neighbor_ixs(side: usize, size: usize) -> Vec<[usize; 27]> {
    let s = side as isize;
    let mut result = Vec::with_capacity(size);

    for i in 0..s * s * s {
        let x = i % s;
        let y = i % (s * s) / s;
        let z = i / (s * s);

        let mut neighborhood = [0; 27];
        let mut idx = 0;
        for m in -1..=1 {
            for n in &NEIGHBORS {
                let u = x + n[0];
                let v = y + n[1];
                let w = z + m;
                let j = u.rem_euclid(s) + v.rem_euclid(s) * s + w.rem_euclid(s) * s * s;
                neighborhood[idx] = j as usize;
                idx += 1;
            }
        }
        result.push(neighborhood);
    }

    result
}

/// System 1: Clear all island cells.
pub fn clear_islands(
    mut islands: ResMut<Islands>,
) {
    for island in &mut islands.0 {
        island.clear();
    }
}

/// System 2: Assign each body to its island cell based on position.
pub fn assign_islands(
    mut islands: ResMut<Islands>,
    grid: Res<IslandGrid>,
    snapshots: Res<BodySnapshots>,
) {
    for (bx, body) in snapshots.0.iter().enumerate() {
        let ix = get_island_ix(body.position, &grid);
        if let Some(island) = islands.0.get_mut(ix) {
            island.push(bx);
        }
    }

    for island in &mut islands.0 {
        island.shrink_to(island.len() * 3);
    }
}

/// System 3: Build per-island neighborhoods from neighbor indices (parallel).
pub fn build_neighborhoods(
    mut debug_info: ResMut<DebugDurations>,
    mut neighborhoods: ResMut<IslandNeighborhoods>,
    islands: Res<Islands>,
    neighbor_ixs: Res<IslandNeighborIxs>,
) {
    let now = Instant::now();

    neighborhoods.0
        .par_iter_mut()
        .enumerate()
        .for_each(|(i, neighbors)| {
            neighbors.clear();
            for nix in &neighbor_ixs.0[i] {
                let island = &islands.0[*nix];
                neighbors.extend_from_slice(&island);
            }
        });

    debug_info.add("islands", now.elapsed());
}

/// Shared resource holding per-frame body snapshots.
#[derive(Default, Resource)]
pub struct BodySnapshots(pub Vec<BodySnapshot>);

/// Compute the island grid index for a given position.
#[inline]
pub fn get_island_ix(pos: DVec3, grid: &IslandGrid) -> usize {
    let max = grid.side_f64 - 1.0;
    let x = (pos.x * grid.side_f64).clamp(0.0, max) as usize;
    let y = (pos.y * grid.side_f64).clamp(0.0, max) as usize;
    let z = (pos.z * grid.side_f64).clamp(0.0, max) as usize;
    x + y * grid.side + z * grid.side * grid.side
}
