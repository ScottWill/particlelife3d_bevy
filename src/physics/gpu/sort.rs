//! CPU-side counting sort for spatial partitioning.

use crate::physics::bodies::BodySnapshot;

/// Sorts particles into grid cells using a counting sort.
///
/// Returns a tuple of:
/// - `sorted_buffer`: particle data in sorted order (16 bytes per particle: f32×3 position + u32 color, little-endian)
/// - `cell_offsets`: start index of each cell in the sorted buffer (length = grid_side³ + 1)
/// - `original_indices`: maps sorted index → original BodySnapshot index
pub fn sort_particles_by_cell(
    snapshots: &[BodySnapshot],
    grid_side: usize,
) -> (Vec<u8>, Vec<u32>, Vec<u32>) {
    let cell_count = grid_side * grid_side * grid_side;

    // Pass 1: Histogram — count particles per cell and record each particle's cell index.
    // NOTE: We compute the cell index using f32-truncated positions because the sorted buffer
    // stores f32 values. This ensures the cell assignment matches what the GPU shader will see.
    let mut counts = vec![0u32; cell_count];
    let cell_indices: Vec<usize> = snapshots
        .iter()
        .map(|body| {
            let cell = compute_cell_index_f32(body.position, grid_side);
            counts[cell] += 1;
            cell
        })
        .collect();

    // Pass 2: Prefix sum → cell_offsets
    let mut cell_offsets = vec![0u32; cell_count + 1];
    for i in 0..cell_count {
        cell_offsets[i + 1] = cell_offsets[i] + counts[i];
    }

    // Pass 3: Scatter into sorted buffer and build original_indices map
    let mut write_pos: Vec<u32> = cell_offsets[..cell_count].to_vec();
    let mut sorted_buffer = vec![0u8; snapshots.len() * 16];
    let mut original_indices = vec![0u32; snapshots.len()];

    for (i, &cell) in cell_indices.iter().enumerate() {
        let dst = write_pos[cell] as usize;
        write_pos[cell] += 1;

        let body = &snapshots[i];
        let offset = dst * 16;

        // Write position as f32×3 + color as u32 (little-endian)
        sorted_buffer[offset..offset + 4]
            .copy_from_slice(&(body.position.x as f32).to_le_bytes());
        sorted_buffer[offset + 4..offset + 8]
            .copy_from_slice(&(body.position.y as f32).to_le_bytes());
        sorted_buffer[offset + 8..offset + 12]
            .copy_from_slice(&(body.position.z as f32).to_le_bytes());
        sorted_buffer[offset + 12..offset + 16]
            .copy_from_slice(&(body.color as u32).to_le_bytes());

        // Record reverse mapping: sorted position → original index
        original_indices[dst] = i as u32;
    }

    (sorted_buffer, cell_offsets, original_indices)
}

/// Computes the flat cell index for a position given a grid side length.
///
/// Cell index formula: `floor(pos.x * grid_side).clamp(0, grid_side-1)` for each axis,
/// then `x + y * side + z * side * side`.
///
/// Uses f32-truncated positions to match GPU shader precision. This ensures the cell
/// assignment is consistent with the stored buffer data.
fn compute_cell_index_f32(position: bevy::math::DVec3, grid_side: usize) -> usize {
    let max = (grid_side - 1) as f32;
    let side = grid_side as f32;
    let x = ((position.x as f32) * side).floor().clamp(0.0, max) as usize;
    let y = ((position.y as f32) * side).floor().clamp(0.0, max) as usize;
    let z = ((position.z as f32) * side).floor().clamp(0.0, max) as usize;
    x + y * grid_side + z * grid_side * grid_side
}

/// Computes the grid side length from the maximum interaction distance.
///
/// Formula: `floor(1.0 / max_dist)` clamped to a maximum of 100.
/// Returns 100 if `max_dist` is non-positive or NaN to avoid division issues.
pub fn compute_grid_side(max_dist: f64) -> usize {
    if max_dist <= 0.0 || max_dist.is_nan() {
        return 100;
    }
    let side = (1.0 / max_dist).floor() as usize;
    side.min(100)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::DVec3;
    use proptest::prelude::*;

    /// Strategy to generate a random BodySnapshot with position in [0.0, 1.0) and color in [0, 9).
    fn body_snapshot_strategy() -> impl Strategy<Value = BodySnapshot> {
        (0.0f64..1.0, 0.0f64..1.0, 0.0f64..1.0, 0usize..9).prop_map(|(x, y, z, color)| {
            BodySnapshot {
                position: DVec3::new(x, y, z),
                color,
            }
        })
    }

    /// Compute expected cell index for a position, matching the implementation in sort.rs.
    /// Uses f32 precision to match GPU shader behavior and buffer storage format.
    fn expected_cell_index(position: DVec3, grid_side: usize) -> usize {
        let max = (grid_side - 1) as f32;
        let side = grid_side as f32;
        let x = ((position.x as f32) * side).floor().clamp(0.0, max) as usize;
        let y = ((position.y as f32) * side).floor().clamp(0.0, max) as usize;
        let z = ((position.z as f32) * side).floor().clamp(0.0, max) as usize;
        x + y * grid_side + z * grid_side * grid_side
    }

    proptest! {
        /// **Validates: Requirements 1.1, 1.2, 1.3, 4.2**
        ///
        /// Property 1: Data Conversion Round-Trip
        ///
        /// For any valid particle positions (DVec3 in [0,1)) and colors (usize in [0,9)),
        /// converting to GPU buffer format (f32×3 + u32) and reading back should produce
        /// values that match the expected f32-truncated values exactly.
        #[test]
        fn data_conversion_round_trip(
            snapshots in proptest::collection::vec(body_snapshot_strategy(), 10..=100)
        ) {
            // Use grid_side=1 so all particles go to a single cell (cell 0).
            let grid_side = 1;
            let (sorted_buffer, _cell_offsets, original_indices) =
                sort_particles_by_cell(&snapshots, grid_side);

            let particle_count = snapshots.len();

            // Verify buffer size
            prop_assert_eq!(sorted_buffer.len(), particle_count * 16);
            prop_assert_eq!(original_indices.len(), particle_count);

            // Deserialize each particle from the sorted buffer and compare with original
            for sorted_idx in 0..particle_count {
                let original_idx = original_indices[sorted_idx] as usize;
                let original = &snapshots[original_idx];

                let offset = sorted_idx * 16;

                // Read f32×3 position from little-endian bytes
                let x_bytes: [u8; 4] = sorted_buffer[offset..offset + 4].try_into().unwrap();
                let y_bytes: [u8; 4] = sorted_buffer[offset + 4..offset + 8].try_into().unwrap();
                let z_bytes: [u8; 4] = sorted_buffer[offset + 8..offset + 12].try_into().unwrap();
                let color_bytes: [u8; 4] = sorted_buffer[offset + 12..offset + 16].try_into().unwrap();

                let read_x = f32::from_le_bytes(x_bytes);
                let read_y = f32::from_le_bytes(y_bytes);
                let read_z = f32::from_le_bytes(z_bytes);
                let read_color = u32::from_le_bytes(color_bytes);

                // Position should match the f32 cast of the original f64 value exactly
                let expected_x = original.position.x as f32;
                let expected_y = original.position.y as f32;
                let expected_z = original.position.z as f32;

                prop_assert_eq!(
                    read_x, expected_x,
                    "Position x mismatch at sorted_idx={}, original_idx={}",
                    sorted_idx, original_idx
                );
                prop_assert_eq!(
                    read_y, expected_y,
                    "Position y mismatch at sorted_idx={}, original_idx={}",
                    sorted_idx, original_idx
                );
                prop_assert_eq!(
                    read_z, expected_z,
                    "Position z mismatch at sorted_idx={}, original_idx={}",
                    sorted_idx, original_idx
                );

                // Color should round-trip exactly (usize → u32 → usize)
                prop_assert_eq!(
                    read_color as usize, original.color,
                    "Color mismatch at sorted_idx={}, original_idx={}",
                    sorted_idx, original_idx
                );
            }
        }

        /// **Validates: Requirements 3.1, 3.3**
        ///
        /// Property 2: Counting Sort Partitioning Invariant
        ///
        /// For any set of particles with positions in [0.0, 1.0)³ and a given grid_side,
        /// the counting sort must produce:
        /// (a) cell_offsets with length grid_side³ + 1
        /// (b) cell_offsets is non-decreasing with cell_offsets[last] = particle_count
        /// (c) every particle in range [cell_offsets[i], cell_offsets[i+1]) maps to cell i
        /// (d) original_indices is a permutation of 0..particle_count
        #[test]
        fn prop_counting_sort_partitioning_invariant(
            particles in proptest::collection::vec(body_snapshot_strategy(), 100..=5000),
            grid_side in 5usize..=30,
        ) {
            let particle_count = particles.len();
            let cell_count = grid_side * grid_side * grid_side;

            let (sorted_buffer, cell_offsets, original_indices) =
                sort_particles_by_cell(&particles, grid_side);

            // (a) cell_offsets has length grid_side³ + 1
            prop_assert_eq!(
                cell_offsets.len(),
                cell_count + 1,
                "cell_offsets length should be grid_side³ + 1"
            );

            // (b) cell_offsets is non-decreasing
            for i in 0..cell_count {
                prop_assert!(
                    cell_offsets[i] <= cell_offsets[i + 1],
                    "cell_offsets must be non-decreasing: cell_offsets[{}]={} > cell_offsets[{}]={}",
                    i, cell_offsets[i], i + 1, cell_offsets[i + 1]
                );
            }

            // (b) cell_offsets[last] = particle_count
            prop_assert_eq!(
                cell_offsets[cell_count] as usize,
                particle_count,
                "cell_offsets[last] should equal particle_count"
            );

            // (c) Every particle in range [cell_offsets[i], cell_offsets[i+1]) maps to cell i
            for cell_idx in 0..cell_count {
                let start = cell_offsets[cell_idx] as usize;
                let end = cell_offsets[cell_idx + 1] as usize;

                for sorted_idx in start..end {
                    // Parse the 16-byte entry from sorted_buffer
                    let offset = sorted_idx * 16;
                    let x = f32::from_le_bytes(
                        sorted_buffer[offset..offset + 4].try_into().unwrap(),
                    );
                    let y = f32::from_le_bytes(
                        sorted_buffer[offset + 4..offset + 8].try_into().unwrap(),
                    );
                    let z = f32::from_le_bytes(
                        sorted_buffer[offset + 8..offset + 12].try_into().unwrap(),
                    );

                    // Compute the cell index from the stored f32 position
                    let pos = DVec3::new(x as f64, y as f64, z as f64);
                    let computed_cell = expected_cell_index(pos, grid_side);

                    prop_assert_eq!(
                        computed_cell,
                        cell_idx,
                        "Particle at sorted index {} has position ({}, {}, {}) mapping to cell {} but is in cell {} range",
                        sorted_idx, x, y, z, computed_cell, cell_idx
                    );
                }
            }

            // (d) original_indices is a permutation of 0..particle_count
            prop_assert_eq!(
                original_indices.len(),
                particle_count,
                "original_indices length should equal particle_count"
            );
            let mut seen = vec![false; particle_count];
            for &idx in &original_indices {
                let idx = idx as usize;
                prop_assert!(
                    idx < particle_count,
                    "original_indices contains out-of-bounds index {}",
                    idx
                );
                prop_assert!(
                    !seen[idx],
                    "original_indices contains duplicate index {}",
                    idx
                );
                seen[idx] = true;
            }
        }

        /// **Property 3: Grid Dimension Calculation**
        /// **Validates: Requirements 3.4, 3.6**
        ///
        /// For any max_dist in [0.01, 0.2], the computed grid_side should equal
        /// floor(1.0 / max_dist) when that value <= 100, and should be clamped to 100 otherwise.
        /// The total cell count (grid_side³) should be reasonable (not exceeding 1M cells).
        #[test]
        fn prop_grid_dimension_calculation(max_dist in 0.01f64..=0.2f64) {
            let grid_side = compute_grid_side(max_dist);
            let expected_raw = (1.0 / max_dist).floor() as usize;

            // Result equals floor(1.0 / max_dist) when that's <= 100
            if expected_raw <= 100 {
                prop_assert_eq!(grid_side, expected_raw,
                    "grid_side should equal floor(1.0/max_dist) when <= 100. max_dist={}, expected={}, got={}",
                    max_dist, expected_raw, grid_side);
            } else {
                // Result is clamped to 100 when floor(1.0 / max_dist) > 100
                prop_assert_eq!(grid_side, 100,
                    "grid_side should be clamped to 100 when floor(1.0/max_dist) > 100. max_dist={}, raw={}, got={}",
                    max_dist, expected_raw, grid_side);
            }

            // Result is always >= 1 for valid max_dist values
            prop_assert!(grid_side >= 1,
                "grid_side must be >= 1 for valid max_dist. max_dist={}, got={}",
                max_dist, grid_side);

            // Total cell count = grid_side³ should not exceed 1M cells
            let total_cells = grid_side * grid_side * grid_side;
            prop_assert!(total_cells <= 1_000_000,
                "total cell count (grid_side³) should not exceed 1M. grid_side={}, total={}",
                grid_side, total_cells);
        }
    }

    /// Compute all 27 toroidal neighbor cell indices for a given (x, y, z) cell coordinate.
    /// Uses rem_euclid to wrap coordinates at grid boundaries.
    fn toroidal_neighbors(cell_x: usize, cell_y: usize, cell_z: usize, grid_side: usize) -> Vec<usize> {
        let mut neighbors = Vec::with_capacity(27);
        for dz in [-1i32, 0, 1] {
            for dy in [-1i32, 0, 1] {
                for dx in [-1i32, 0, 1] {
                    let nx = ((cell_x as i32 + dx).rem_euclid(grid_side as i32)) as usize;
                    let ny = ((cell_y as i32 + dy).rem_euclid(grid_side as i32)) as usize;
                    let nz = ((cell_z as i32 + dz).rem_euclid(grid_side as i32)) as usize;
                    neighbors.push(nx + ny * grid_side + nz * grid_side * grid_side);
                }
            }
        }
        neighbors
    }

    proptest! {
        /// **Validates: Requirements 3.2**
        ///
        /// Property 4: Toroidal Cell Neighbor Wrapping
        ///
        /// For any cell coordinate (x, y, z) in a grid of side S, the 27 neighbor cell indices
        /// (computed via (coord + offset).rem_euclid(S) for offsets in {-1, 0, 1}³) should all
        /// be valid indices in [0, S³), and boundary cells (where any coordinate is 0 or S-1)
        /// should have neighbors that wrap to the opposite edge.
        #[test]
        fn prop_toroidal_cell_neighbor_wrapping(
            grid_side in 2usize..=50,
            cell_x in 0usize..50,
            cell_y in 0usize..50,
            cell_z in 0usize..50,
        ) {
            // Constrain cell coordinates to be within the grid
            let cell_x = cell_x % grid_side;
            let cell_y = cell_y % grid_side;
            let cell_z = cell_z % grid_side;

            let neighbors = toroidal_neighbors(cell_x, cell_y, cell_z, grid_side);
            let total_cells = grid_side * grid_side * grid_side;

            // Exactly 27 neighbors
            prop_assert_eq!(neighbors.len(), 27,
                "Expected exactly 27 neighbors, got {}", neighbors.len());

            // All neighbor indices are in [0, grid_side³)
            for (i, &n) in neighbors.iter().enumerate() {
                prop_assert!(n < total_cells,
                    "Neighbor {} has index {} which exceeds total_cells {} (grid_side={}, cell=({},{},{}))",
                    i, n, total_cells, grid_side, cell_x, cell_y, cell_z);
            }

            // Verify wrapping at edges: if cell_x == 0, the dx=-1 neighbors should have nx == grid_side - 1
            if cell_x == 0 {
                // dx=-1 neighbors are at positions where dx=-1 (indices 0, 3, 6, 9, 12, 15, 18, 21, 24 in iteration order)
                // In our loop order (dz outer, dy mid, dx inner), dx=-1 is at offsets 0, 3, 6 within each dz*9+dy*3 block
                for dz_idx in 0..3 {
                    for dy_idx in 0..3 {
                        let idx = dz_idx * 9 + dy_idx * 3 + 0; // dx = -1
                        let n = neighbors[idx];
                        let nx = n % grid_side;
                        prop_assert_eq!(nx, grid_side - 1,
                            "With cell_x=0, dx=-1 neighbor should wrap to x={}, but got nx={} (grid_side={})",
                            grid_side - 1, nx, grid_side);
                    }
                }
            }

            if cell_x == grid_side - 1 {
                // dx=+1 neighbors should wrap to nx == 0
                for dz_idx in 0..3 {
                    for dy_idx in 0..3 {
                        let idx = dz_idx * 9 + dy_idx * 3 + 2; // dx = +1
                        let n = neighbors[idx];
                        let nx = n % grid_side;
                        prop_assert_eq!(nx, 0,
                            "With cell_x={}, dx=+1 neighbor should wrap to x=0, but got nx={} (grid_side={})",
                            grid_side - 1, nx, grid_side);
                    }
                }
            }

            if cell_y == 0 {
                // dy=-1 neighbors should wrap to ny == grid_side - 1
                for dz_idx in 0..3 {
                    for dx_idx in 0..3 {
                        let idx = dz_idx * 9 + 0 * 3 + dx_idx; // dy = -1
                        let n = neighbors[idx];
                        let ny = (n / grid_side) % grid_side;
                        prop_assert_eq!(ny, grid_side - 1,
                            "With cell_y=0, dy=-1 neighbor should wrap to y={}, but got ny={} (grid_side={})",
                            grid_side - 1, ny, grid_side);
                    }
                }
            }

            if cell_y == grid_side - 1 {
                // dy=+1 neighbors should wrap to ny == 0
                for dz_idx in 0..3 {
                    for dx_idx in 0..3 {
                        let idx = dz_idx * 9 + 2 * 3 + dx_idx; // dy = +1
                        let n = neighbors[idx];
                        let ny = (n / grid_side) % grid_side;
                        prop_assert_eq!(ny, 0,
                            "With cell_y={}, dy=+1 neighbor should wrap to y=0, but got ny={} (grid_side={})",
                            grid_side - 1, ny, grid_side);
                    }
                }
            }

            if cell_z == 0 {
                // dz=-1 neighbors should wrap to nz == grid_side - 1
                for dy_idx in 0..3 {
                    for dx_idx in 0..3 {
                        let idx = 0 * 9 + dy_idx * 3 + dx_idx; // dz = -1
                        let n = neighbors[idx];
                        let nz = n / (grid_side * grid_side);
                        prop_assert_eq!(nz, grid_side - 1,
                            "With cell_z=0, dz=-1 neighbor should wrap to z={}, but got nz={} (grid_side={})",
                            grid_side - 1, nz, grid_side);
                    }
                }
            }

            if cell_z == grid_side - 1 {
                // dz=+1 neighbors should wrap to nz == 0
                for dy_idx in 0..3 {
                    for dx_idx in 0..3 {
                        let idx = 2 * 9 + dy_idx * 3 + dx_idx; // dz = +1
                        let n = neighbors[idx];
                        let nz = n / (grid_side * grid_side);
                        prop_assert_eq!(nz, 0,
                            "With cell_z={}, dz=+1 neighbor should wrap to z=0, but got nz={} (grid_side={})",
                            grid_side - 1, nz, grid_side);
                    }
                }
            }
        }
    }

    /// Pure Rust reference implementation of density attenuation logic,
    /// matching the WGSL shader behavior.
    fn compute_density_factor(prev_density: f32, density_limit: f32, attenuation_enabled: bool) -> f32 {
        if attenuation_enabled {
            1.0_f32 - (prev_density - density_limit).clamp(0.0, 1.0)
        } else {
            1.0_f32
        }
    }

    /// Applies the density attenuation to a force matrix value, matching shader logic:
    /// only positive values are attenuated; negative and zero values pass through unchanged.
    fn apply_attenuation(matrix_value: f32, density_factor: f32) -> f32 {
        if matrix_value > 0.0 {
            matrix_value * density_factor
        } else {
            matrix_value
        }
    }

    proptest! {
        /// **Validates: Requirements 2.9, 2.10, 5.3, 9.2, 9.3**
        ///
        /// Property 6: Density Attenuation Selectivity
        ///
        /// For any force matrix value and any previous density value:
        /// - When attenuation is disabled: density_factor is always 1.0
        /// - When attenuation is enabled AND matrix_value > 0: effective force = matrix_value * density_factor
        /// - When attenuation is enabled AND matrix_value <= 0: effective force = matrix_value (unchanged)
        /// - density_factor is always in [0.0, 1.0]
        /// - When prev_density <= density_limit: density_factor == 1.0
        /// - When prev_density >= density_limit + 1.0: density_factor == 0.0
        #[test]
        fn prop_density_attenuation_selectivity(
            matrix_value in -1.0f32..=1.0f32,
            prev_density in 0.0f32..=5.0f32,
            density_limit in 0.0f32..=3.0f32,
            attenuation_enabled in proptest::bool::ANY,
        ) {
            let density_factor = compute_density_factor(prev_density, density_limit, attenuation_enabled);
            let effective_force = apply_attenuation(matrix_value, density_factor);

            // 1. When attenuation is disabled: density_factor is always 1.0 regardless of prev_density
            if !attenuation_enabled {
                prop_assert_eq!(density_factor, 1.0_f32,
                    "density_factor must be 1.0 when attenuation is disabled, got {}",
                    density_factor);
                // Effective force equals matrix_value unmodified
                prop_assert_eq!(effective_force, matrix_value,
                    "When attenuation disabled, effective_force should equal matrix_value. \
                     matrix_value={}, effective_force={}",
                    matrix_value, effective_force);
            }

            // 2. When attenuation is enabled AND matrix_value > 0: effective force = matrix_value * density_factor
            if attenuation_enabled && matrix_value > 0.0 {
                let expected = matrix_value * density_factor;
                prop_assert!(
                    (effective_force - expected).abs() < 1e-7,
                    "When attenuation enabled and matrix_value > 0, effective_force should be \
                     matrix_value * density_factor. matrix_value={}, density_factor={}, \
                     expected={}, got={}",
                    matrix_value, density_factor, expected, effective_force
                );
            }

            // 3. When attenuation is enabled AND matrix_value <= 0: effective force = matrix_value (unchanged)
            if attenuation_enabled && matrix_value <= 0.0 {
                prop_assert_eq!(effective_force, matrix_value,
                    "When attenuation enabled and matrix_value <= 0, effective_force must be \
                     unchanged. matrix_value={}, density_factor={}, effective_force={}",
                    matrix_value, density_factor, effective_force);
            }

            // 4. density_factor is always in [0.0, 1.0] when enabled
            if attenuation_enabled {
                prop_assert!(density_factor >= 0.0 && density_factor <= 1.0,
                    "density_factor must be in [0.0, 1.0] when enabled, got {} \
                     (prev_density={}, density_limit={})",
                    density_factor, prev_density, density_limit);
            }

            // 5. When prev_density <= density_limit: density_factor == 1.0 (no attenuation)
            if attenuation_enabled && prev_density <= density_limit {
                prop_assert_eq!(density_factor, 1.0_f32,
                    "When prev_density ({}) <= density_limit ({}), density_factor should be 1.0, got {}",
                    prev_density, density_limit, density_factor);
            }

            // 6. When prev_density >= density_limit + 1.0: density_factor == 0.0 (full attenuation)
            if attenuation_enabled && prev_density >= density_limit + 1.0 {
                prop_assert_eq!(density_factor, 0.0_f32,
                    "When prev_density ({}) >= density_limit + 1.0 ({}), density_factor should be 0.0, got {}",
                    prev_density, density_limit + 1.0, density_factor);
            }
        }
    }

    /// Edge case: very small max_dist should clamp grid_side to 100
    #[test]
    fn test_grid_dimension_clamps_at_100() {
        // max_dist = 0.005 → floor(1.0/0.005) = 200, should clamp to 100
        let grid_side = compute_grid_side(0.005);
        assert_eq!(grid_side, 100);
    }

    /// Edge case: max_dist = 0.2 → floor(1.0/0.2) = 5
    #[test]
    fn test_grid_dimension_exact_value() {
        let grid_side = compute_grid_side(0.2);
        assert_eq!(grid_side, 5);
    }
}
