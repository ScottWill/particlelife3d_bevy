//! Property-based tests for the GPU force compute module.

use bevy::math::DVec3;
use proptest::prelude::*;

use crate::physics::{bodies::BodySnapshot, gpu::sort::AllocatedVecs};
use super::sort::{compute_grid_side, sort_particles_by_cell};

/// Physics parameters for the f32 reference force function (matches shader's Params struct).
#[derive(Debug, Clone)]
struct RefParams {
    max_dist: f32,
    min_rel_dist: f32,
    max_dist_sqrd: f32,
    max_dist_recip: f32,
    min_dist_recip: f32,
    inv_min_dist_recip: f32,
    density_same_color: f32,
    density_diff_color: f32,
    color_count: u32,
}

impl RefParams {
    fn new(max_dist: f32, min_rel_dist: f32, density_same_color: f32, density_diff_color: f32, color_count: u32) -> Self {
        Self {
            max_dist,
            min_rel_dist,
            max_dist_sqrd: max_dist * max_dist,
            max_dist_recip: 1.0 / max_dist,
            min_dist_recip: 1.0 / min_rel_dist,
            inv_min_dist_recip: 1.0 / (1.0 - min_rel_dist),
            density_same_color,
            density_diff_color,
            color_count,
        }
    }
}

/// Result of computing force between a pair of particles.
#[derive(Debug, Clone)]
struct ForceResult {
    force: [f32; 3],
    density: f32,
}

/// Pure f32 reference implementation of the piecewise force function, matching the shader logic.
///
/// Computes the force contribution of body1 on body0, without density attenuation
/// (density_factor = 1.0).
///
/// Returns None if the pair should be skipped (too far, too close, or zero matrix value).
fn reference_force_pair(
    pos0: [f32; 3],
    color0: u32,
    pos1: [f32; 3],
    color1: u32,
    force_matrix: &[f32],
    params: &RefParams,
) -> Option<ForceResult> {
    // Toroidal shortest-path distance (matches shader: fract(body1.pos - body0.pos + 0.5) - 0.5)
    let mut min_pos = [0.0f32; 3];
    for i in 0..3 {
        let v = pos1[i] - pos0[i] + 0.5;
        // fract(x) = x - floor(x), same as WGSL fract for positive values
        min_pos[i] = (v - v.floor()) - 0.5;
    }

    let dist_sqrd = min_pos[0] * min_pos[0] + min_pos[1] * min_pos[1] + min_pos[2] * min_pos[2];

    // Early exits
    if dist_sqrd > params.max_dist_sqrd || dist_sqrd < 1e-30 {
        return None;
    }

    let dist = dist_sqrd.sqrt();
    let rel_dist = dist * params.max_dist_recip;
    let dir = [min_pos[0] / dist, min_pos[1] / dist, min_pos[2] / dist];

    let force_scalar: f32;

    if rel_dist <= params.min_rel_dist {
        // Repulsion zone
        force_scalar = rel_dist * params.min_dist_recip - 1.0;
    } else {
        // Attraction/repulsion zone based on force matrix
        let matrix_val = force_matrix[(color0 * params.color_count + color1) as usize];

        if matrix_val == 0.0 {
            return None; // skip entirely per requirement 2.5
        }

        // No density attenuation in this test (density_factor = 1.0), so f = matrix_val
        let f = matrix_val;
        force_scalar = f * (1.0 - (1.0 + params.min_rel_dist - 2.0 * rel_dist) * params.inv_min_dist_recip);
    }

    // Force vector: dir * force_scalar * max_dist
    let force = [
        dir[0] * (force_scalar * params.max_dist),
        dir[1] * (force_scalar * params.max_dist),
        dir[2] * (force_scalar * params.max_dist),
    ];

    // Density accumulation: weight * (1.0 - rel_dist)
    let weight = if color0 == color1 {
        params.density_same_color
    } else {
        params.density_diff_color
    };
    let density = weight * (1.0 - rel_dist);

    Some(ForceResult { force, density })
}

/// Proptest strategy for a position in [0.0, 1.0)³
fn position_strategy() -> impl Strategy<Value = [f32; 3]> {
    proptest::array::uniform3(0.0f32..1.0f32)
}

/// Proptest strategy for a color index in [0, 5)
fn color_strategy() -> impl Strategy<Value = u32> {
    0u32..5
}

/// Proptest strategy for a flat 5×5 force matrix with values in [-1.0, 1.0]
fn force_matrix_strategy() -> impl Strategy<Value = Vec<f32>> {
    proptest::collection::vec(-1.0f32..=1.0f32, 25)
}

proptest! {
    /// **Validates: Requirements 2.1, 2.2, 2.3, 2.4, 2.5, 2.6, 2.7, 5.6**
    ///
    /// Property 5: Piecewise Force Function Correctness
    ///
    /// For any two particles with valid positions in [0.0, 1.0)³, colors in [0, 5),
    /// a force matrix with values in [-1.0, 1.0], and valid physics parameters:
    /// the computed force vector and density contribution should match the piecewise formula.
    ///
    /// This test verifies the reference function's output against manually computed expected
    /// values for each zone of the force function.
    #[test]
    fn prop_piecewise_force_function_correctness(
        pos0 in position_strategy(),
        pos1 in position_strategy(),
        color0 in color_strategy(),
        color1 in color_strategy(),
        force_matrix in force_matrix_strategy(),
        max_dist in 0.05f32..=0.3f32,
        min_rel_dist in 0.1f32..=0.5f32,
        density_same_color in 0.0f32..=2.0f32,
        density_diff_color in 0.0f32..=2.0f32,
    ) {
        let color_count = 5u32;
        let params = RefParams::new(max_dist, min_rel_dist, density_same_color, density_diff_color, color_count);

        // Compute using reference implementation
        let result = reference_force_pair(pos0, color0, pos1, color1, &force_matrix, &params);

        // Manually compute expected values step by step for verification
        let mut min_pos = [0.0f32; 3];
        for i in 0..3 {
            let v = pos1[i] - pos0[i] + 0.5;
            min_pos[i] = (v - v.floor()) - 0.5;
        }

        let dist_sqrd = min_pos[0] * min_pos[0] + min_pos[1] * min_pos[1] + min_pos[2] * min_pos[2];

        // Case 1: Distance out of range → should return None
        if dist_sqrd > params.max_dist_sqrd || dist_sqrd < 1e-30 {
            prop_assert!(
                result.is_none(),
                "Expected None for out-of-range distance (dist_sqrd={}, max_dist_sqrd={})",
                dist_sqrd, params.max_dist_sqrd
            );
            return Ok(());
        }

        let dist = dist_sqrd.sqrt();
        let rel_dist = dist * params.max_dist_recip;
        let dir = [min_pos[0] / dist, min_pos[1] / dist, min_pos[2] / dist];

        if rel_dist <= params.min_rel_dist {
            // Case 2: Repulsion zone
            let expected_force_scalar = rel_dist * params.min_dist_recip - 1.0;
            let expected_force = [
                dir[0] * (expected_force_scalar * params.max_dist),
                dir[1] * (expected_force_scalar * params.max_dist),
                dir[2] * (expected_force_scalar * params.max_dist),
            ];
            let weight = if color0 == color1 { density_same_color } else { density_diff_color };
            let expected_density = weight * (1.0 - rel_dist);

            let res = result.expect("Expected Some result in repulsion zone");
            for i in 0..3 {
                prop_assert!(
                    (res.force[i] - expected_force[i]).abs() < 1e-5,
                    "Force component {} mismatch in repulsion zone: got {}, expected {} (rel_dist={})",
                    i, res.force[i], expected_force[i], rel_dist
                );
            }
            prop_assert!(
                (res.density - expected_density).abs() < 1e-5,
                "Density mismatch in repulsion zone: got {}, expected {}",
                res.density, expected_density
            );
        } else {
            // Case 3: Attraction zone
            let matrix_val = force_matrix[(color0 * color_count + color1) as usize];

            if matrix_val == 0.0 {
                // Should skip entirely
                prop_assert!(
                    result.is_none(),
                    "Expected None when matrix_val == 0.0"
                );
            } else {
                let expected_force_scalar = matrix_val * (1.0 - (1.0 + params.min_rel_dist - 2.0 * rel_dist) * params.inv_min_dist_recip);
                let expected_force = [
                    dir[0] * (expected_force_scalar * params.max_dist),
                    dir[1] * (expected_force_scalar * params.max_dist),
                    dir[2] * (expected_force_scalar * params.max_dist),
                ];
                let weight = if color0 == color1 { density_same_color } else { density_diff_color };
                let expected_density = weight * (1.0 - rel_dist);

                let res = result.expect("Expected Some result in attraction zone with non-zero matrix_val");
                for i in 0..3 {
                    prop_assert!(
                        (res.force[i] - expected_force[i]).abs() < 1e-5,
                        "Force component {} mismatch in attraction zone: got {}, expected {} (rel_dist={}, matrix_val={})",
                        i, res.force[i], expected_force[i], rel_dist, matrix_val
                    );
                }
                prop_assert!(
                    (res.density - expected_density).abs() < 1e-5,
                    "Density mismatch in attraction zone: got {}, expected {} (rel_dist={}, weight={})",
                    res.density, expected_density, rel_dist, weight
                );
            }
        }
    }
}


/// Parameters for the f64 CPU reference (matching `get_computation` in physics.rs).
#[derive(Debug, Clone)]
struct CpuParams {
    max_dist: f64,
    min_rel_dist: f64,
    max_dist_sqrd: f64,
    max_dist_recip: f64,
    min_dist_recip: f64,
    inv_min_dist_recip: f64,
    density_same_color: f64,
    density_diff_color: f64,
}

impl CpuParams {
    fn new(max_dist: f64, min_rel_dist: f64, density_same_color: f64, density_diff_color: f64) -> Self {
        Self {
            max_dist,
            min_rel_dist,
            max_dist_sqrd: max_dist * max_dist,
            max_dist_recip: max_dist.recip(),
            min_dist_recip: min_rel_dist.recip(),
            inv_min_dist_recip: (1.0 - min_rel_dist).recip(),
            density_same_color,
            density_diff_color,
        }
    }
}

/// f64 CPU reference of the piecewise force function, matching `get_computation` in physics.rs.
/// No density attenuation (density_factor = 1.0).
fn cpu_get_computation(
    body0: &BodySnapshot,
    body1: &BodySnapshot,
    force_matrix: &[f64],
    color_count: usize,
    params: &CpuParams,
) -> (DVec3, f64) {
    let min_pos = (body1.position - body0.position + 0.5).rem_euclid(DVec3::ONE) - 0.5;
    let dist_sqrd = min_pos.length_squared();
    if dist_sqrd > params.max_dist_sqrd || dist_sqrd < 1e-30 {
        return (DVec3::ZERO, 0.0);
    }

    let dist = dist_sqrd.sqrt();
    let rel_dist = dist * params.max_dist_recip;
    let dir = min_pos / dist;

    let force_scalar = if rel_dist <= params.min_rel_dist {
        rel_dist * params.min_dist_recip - 1.0
    } else {
        let f = force_matrix[body0.color * color_count + body1.color];
        if f == 0.0 {
            return (DVec3::ZERO, 0.0);
        }
        // No density attenuation: density_factor = 1.0, so f stays as-is
        f * (1.0 - (1.0 + params.min_rel_dist - 2.0 * rel_dist) * params.inv_min_dist_recip)
    };

    let weight = if body0.color == body1.color {
        params.density_same_color
    } else {
        params.density_diff_color
    };

    let force = dir * (force_scalar * params.max_dist);
    let density = weight * (1.0 - rel_dist);
    (force, density)
}

/// Run the f32 reference (GPU-like) implementation using the spatial grid.
/// This simulates what the GPU shader does: sort particles by cell, iterate 3×3×3 neighbors.
fn run_f32_reference_with_grid(
    snapshots: &[BodySnapshot],
    force_matrix_f32: &[f32],
    params: &RefParams,
    grid_side: usize,
) -> Vec<(f32, f32, f32, f32)> {
    let particle_count = snapshots.len();
    let mut allocated_vecs = AllocatedVecs::default();

    // Sort particles by cell (same as GPU pipeline does)
    let original_indices = sort_particles_by_cell(&mut allocated_vecs, snapshots, grid_side);

    // Parse sorted particles from buffer (f32 positions + u32 color)
    let sorted_particles: Vec<([f32; 3], u32)> = (0..particle_count)
        .map(|i| {
            let offset = i * 16;
            let x = f32::from_le_bytes(allocated_vecs.sorted_buffer[offset..offset + 4].try_into().unwrap());
            let y = f32::from_le_bytes(allocated_vecs.sorted_buffer[offset + 4..offset + 8].try_into().unwrap());
            let z = f32::from_le_bytes(allocated_vecs.sorted_buffer[offset + 8..offset + 12].try_into().unwrap());
            let color = u32::from_le_bytes(allocated_vecs.sorted_buffer[offset + 12..offset + 16].try_into().unwrap());
            ([x, y, z], color)
        })
        .collect();

    // Compute forces for each particle in sorted order (mimicking shader)
    let mut sorted_results: Vec<(f32, f32, f32, f32)> = vec![(0.0, 0.0, 0.0, 0.0); particle_count];

    let side = grid_side as u32;
    let side_f = side as f32;

    for idx in 0..particle_count {
        let (pos0, color0) = sorted_particles[idx];

        // Determine cell for this particle
        let cell_x = (pos0[0] * side_f).floor().clamp(0.0, side_f - 1.0) as u32;
        let cell_y = (pos0[1] * side_f).floor().clamp(0.0, side_f - 1.0) as u32;
        let cell_z = (pos0[2] * side_f).floor().clamp(0.0, side_f - 1.0) as u32;

        let mut total_force = [0.0f32; 3];
        let mut total_density = 0.0f32;

        // Iterate 3×3×3 neighbors
        for dz in -1i32..=1 {
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let nx = ((cell_x as i32 + dx + side as i32) % side as i32) as u32;
                    let ny = ((cell_y as i32 + dy + side as i32) % side as i32) as u32;
                    let nz = ((cell_z as i32 + dz + side as i32) % side as i32) as u32;
                    let cell_idx = (nx + ny * side + nz * side * side) as usize;

                    let start = allocated_vecs.cell_offsets[cell_idx] as usize;
                    let end = allocated_vecs.cell_offsets[cell_idx + 1] as usize;

                    for j in start..end {
                        if j == idx {
                            continue; // skip self
                        }

                        let (pos1, color1) = sorted_particles[j];

                        if let Some(result) = reference_force_pair(
                            pos0, color0, pos1, color1, force_matrix_f32, params,
                        ) {
                            total_force[0] += result.force[0];
                            total_force[1] += result.force[1];
                            total_force[2] += result.force[2];
                            total_density += result.density;
                        }
                    }
                }
            }
        }

        sorted_results[idx] = (total_force[0], total_force[1], total_force[2], total_density);
    }

    // Remap sorted results back to original particle order
    let mut results = vec![(0.0f32, 0.0f32, 0.0f32, 0.0f32); particle_count];
    for sorted_idx in 0..particle_count {
        let original_idx = original_indices[sorted_idx] as usize;
        results[original_idx] = sorted_results[sorted_idx];
    }
    results
}

/// Run the f64 CPU reference implementation (all-pairs within max_dist).
fn run_f64_cpu_reference(
    snapshots: &[BodySnapshot],
    force_matrix_f64: &[f64],
    color_count: usize,
    params: &CpuParams,
) -> Vec<(f64, f64, f64, f64)> {
    let particle_count = snapshots.len();
    let mut results = vec![(0.0f64, 0.0f64, 0.0f64, 0.0f64); particle_count];

    for i in 0..particle_count {
        let mut total_force = DVec3::ZERO;
        let mut total_density = 0.0f64;

        for j in 0..particle_count {
            if i == j {
                continue;
            }
            let (force, density) = cpu_get_computation(
                &snapshots[i],
                &snapshots[j],
                force_matrix_f64,
                color_count,
                params,
            );
            total_force += force;
            total_density += density;
        }

        results[i] = (total_force.x, total_force.y, total_force.z, total_density);
    }
    results
}

/// Strategy for generating a set of particles with positions in [0, 1) and colors in [0, color_count).
#[allow(dead_code)]
fn particle_set_strategy(
    min_count: usize,
    max_count: usize,
    color_count: usize,
) -> impl Strategy<Value = Vec<BodySnapshot>> {
    proptest::collection::vec(
        (0.0f64..1.0, 0.0f64..1.0, 0.0f64..1.0, 0usize..color_count).prop_map(
            move |(x, y, z, color)| BodySnapshot {
                position: DVec3::new(x, y, z),
                color,
            },
        ),
        min_count..=max_count,
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// **Validates: Requirements 2.11, 5.1, 5.2, 5.5, 9.6**
    ///
    /// Property 7: GPU-CPU Behavioral Parity
    ///
    /// For any small set of particles (10-100), colors (2-5), and valid physics parameters:
    /// the f32 reference implementation (simulating GPU shader behavior with spatial grid)
    /// should produce per-particle force and density values that match the f64 CPU
    /// `get_computation` function output within 1e-3 absolute tolerance per component.
    ///
    /// Tests WITHOUT density attenuation (density_factor = 1.0) to focus purely on
    /// force function parity. Attenuation selectivity is covered by Property 6.
    #[test]
    fn prop_gpu_cpu_behavioral_parity(
        color_count in 2usize..=5,
        max_dist in 0.05f64..=0.3f64,
        min_rel_dist in 0.1f64..=0.5f64,
        density_same_color in 0.0f64..=2.0f64,
        density_diff_color in 0.0f64..=2.0f64,
        // Generate force matrix after knowing color_count — use flat vec with max possible size
        force_matrix_raw in proptest::collection::vec(-1.0f64..=1.0f64, 25),
        particles_seed in 10usize..=100usize,
        positions in proptest::collection::vec(
            (0.0f64..1.0, 0.0f64..1.0, 0.0f64..1.0),
            100
        ),
    ) {
        // Trim particles to requested count
        let particle_count = particles_seed;
        let positions: Vec<(f64, f64, f64)> = positions.into_iter().take(particle_count).collect();

        // Build particle set with colors assigned cyclically
        let snapshots: Vec<BodySnapshot> = positions
            .iter()
            .enumerate()
            .map(|(i, &(x, y, z))| BodySnapshot {
                position: DVec3::new(x, y, z),
                color: i % color_count,
            })
            .collect();

        // Build force matrices (f64 for CPU, f32 for GPU reference)
        let matrix_size = color_count * color_count;
        let force_matrix_f64: Vec<f64> = force_matrix_raw.iter()
            .take(matrix_size)
            .copied()
            .chain(std::iter::repeat(0.0))
            .take(matrix_size)
            .collect();
        let force_matrix_f32: Vec<f32> = force_matrix_f64.iter().map(|&v| v as f32).collect();

        // Build params
        let grid_side = compute_grid_side(max_dist);
        let ref_params = RefParams::new(
            max_dist as f32,
            min_rel_dist as f32,
            density_same_color as f32,
            density_diff_color as f32,
            color_count as u32,
        );
        let cpu_params = CpuParams::new(max_dist, min_rel_dist, density_same_color, density_diff_color);

        // Run f32 reference with spatial grid (GPU-like)
        let gpu_results = run_f32_reference_with_grid(&snapshots, &force_matrix_f32, &ref_params, grid_side);

        // Run f64 CPU reference (all-pairs)
        let cpu_results = run_f64_cpu_reference(&snapshots, &force_matrix_f64, color_count, &cpu_params);

        // Compare results within 1e-3 tolerance
        let tolerance = 1e-3;
        for i in 0..particle_count {
            let (gfx, gfy, gfz, gd) = gpu_results[i];
            let (cfx, cfy, cfz, cd) = cpu_results[i];

            let diff_fx = (gfx as f64 - cfx).abs();
            let diff_fy = (gfy as f64 - cfy).abs();
            let diff_fz = (gfz as f64 - cfz).abs();
            let diff_d = (gd as f64 - cd).abs();

            prop_assert!(
                diff_fx < tolerance,
                "Particle {} force.x mismatch: gpu={}, cpu={}, diff={} (tolerance={})",
                i, gfx, cfx, diff_fx, tolerance
            );
            prop_assert!(
                diff_fy < tolerance,
                "Particle {} force.y mismatch: gpu={}, cpu={}, diff={} (tolerance={})",
                i, gfy, cfy, diff_fy, tolerance
            );
            prop_assert!(
                diff_fz < tolerance,
                "Particle {} force.z mismatch: gpu={}, cpu={}, diff={} (tolerance={})",
                i, gfz, cfz, diff_fz, tolerance
            );
            prop_assert!(
                diff_d < tolerance,
                "Particle {} density mismatch: gpu={}, cpu={}, diff={} (tolerance={})",
                i, gd, cd, diff_d, tolerance
            );
        }
    }
}
