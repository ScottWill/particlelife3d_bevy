use std::time::Instant;

use bevy::math::{DVec3, USizeVec3};
use rayon::iter::{IntoParallelIterator as _, ParallelIterator as _};

use crate::debug::DebugDurations;

use super::bodies::PointBody;

const NEIGHBORS: [[isize; 2]; 9] = [
    [-1, -1], [0, -1], [1, -1],
    [-1,  0], [0,  0], [1,  0],
    [-1,  1], [0,  1], [1,  1],
];

#[derive(Default)]
pub struct IslandManager {
    islands: Vec<Vec<usize>>, // islands and their per step computed body indicies
    neighbor_ixs: Vec<[usize; 27]>, // pre-cached neighbor indices
    neighbors: Vec<Vec<usize>>, // per-step computed bodies per island
    scale: USizeVec3,
    side_f64: f64,
    side: usize,
}

impl IslandManager {
    pub fn new(max_radius: f64) -> Self {
        let side = max_radius.recip().floor() as usize;
        let size = side * side * side;
        // build and return self
        let mut this = Self {
            islands: vec![vec![]; size],
            neighbor_ixs: Vec::with_capacity(size),
            neighbors: vec![vec![]; size],
            scale: USizeVec3::new(1, side, side * side),
            side_f64: side as f64,
            side,
        };
        this.setup_neighbors();
        this
    }

    // cache the computed indices of each island's group
    fn setup_neighbors(&mut self) {
        let side = self.side as isize;
        // for each island
        for i in 0..side * side * side {
            let x = i % side;
            let y = i % (side * side) / side;
            let z = i / (side * side);

            let mut neighborhood = [0; 27];
            let mut i = 0;
            // find the index of each surrounding island
            for m in -1..=1 {
                for n in &NEIGHBORS {
                    let u = x + n[0];
                    let v = y + n[1];
                    let w = z + m;
                    let j = u.rem_euclid(side) + v.rem_euclid(side) * side + w.rem_euclid(side) * side * side;
                    neighborhood[i] = j as usize;
                    i += 1;
                }
            }
            self.neighbor_ixs.push(neighborhood);
        }
    }

    // add each body's vec index into the appropriate island
    pub fn index_positions(&mut self, bodies: &[PointBody], durations: &mut DebugDurations) {

        let now = Instant::now();

        // clear all the islands w/o reallocating memory
        for island in &mut self.islands {
            island.clear();
        }

        // for each body, add its index to the appropriate island
        // based on its current position
        for (bx, body) in bodies.iter().enumerate() {
            let ix = self.get_local_island_ix(body.position);
            if let Some(island) = self.islands.get_mut(ix) {
                island.push(bx);
            }
        }

        // for each island, find all body indexes for its local neighborhood
        self.neighbors = (0..self.neighbor_ixs.len())
            .into_par_iter()
            .map(|i| {
                let mut ixs = Vec::new();
                if let Some(nixs) = self.neighbor_ixs.get(i) {
                    for nix in nixs {
                        ixs.extend_from_slice(&self.islands[*nix]);
                    }
                }
                ixs
            })
            .collect::<Vec<_>>();

        durations.add("islands", now.elapsed());

    }

    #[inline]
    pub fn get_neighboring_ixs(&self, pos: DVec3) -> Option<&Vec<usize>> {
        let ix = self.get_local_island_ix(pos);
        self.neighbors.get(ix)
    }

    #[inline]
    fn get_local_island_ix(&self, pos: DVec3) -> usize {
        let pos = (pos * self.side_f64).as_usizevec3();
        (pos * self.scale).element_sum()
    }

}
