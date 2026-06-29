struct Particle {
    pos: vec3<f32>,
    color: u32,
};

struct Result {
    force: vec3<f32>,
    density: f32,
};

struct Params {
    max_dist: f32,
    min_rel_dist: f32,
    max_dist_sqrd: f32,
    max_dist_recip: f32,
    min_dist_recip: f32,
    inv_min_dist_recip: f32,
    density_limit: f32,
    density_same_color: f32,
    density_diff_color: f32,
    attenuation_enabled: u32,
    particle_count: u32,
    grid_side: u32,
    color_count: u32,
    _padding: u32,
};

@group(0) @binding(0) var<storage, read> particles: array<Particle>;
@group(0) @binding(1) var<storage, read> cell_offsets: array<u32>;
@group(0) @binding(2) var<uniform> params: Params;
@group(0) @binding(3) var<storage, read> force_matrix: array<f32>;
@group(0) @binding(4) var<storage, read> prev_densities: array<f32>;
@group(0) @binding(5) var<storage, read_write> results: array<Result>;

@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let idx = global_id.x;
    if (idx >= params.particle_count) {
        return;
    }

    let body0 = particles[idx];
    let source_color = body0.color;

    // Compute density factor from previous tick
    var density_factor: f32 = 1.0;
    if (params.attenuation_enabled == 1u) {
        let prev_density = prev_densities[idx];
        density_factor = 1.0 - clamp(prev_density - params.density_limit, 0.0, 1.0);
    }

    // Determine which grid cell this particle belongs to
    let side_f = f32(params.grid_side);
    let cell_x = u32(clamp(body0.pos.x * side_f, 0.0, side_f - 1.0));
    let cell_y = u32(clamp(body0.pos.y * side_f, 0.0, side_f - 1.0));
    let cell_z = u32(clamp(body0.pos.z * side_f, 0.0, side_f - 1.0));

    var total_force = vec3<f32>(0.0, 0.0, 0.0);
    var total_density: f32 = 0.0;
    let side = params.grid_side;

    // Iterate over 3×3×3 neighboring cells
    for (var dz: i32 = -1; dz <= 1; dz++) {
        for (var dy: i32 = -1; dy <= 1; dy++) {
            for (var dx: i32 = -1; dx <= 1; dx++) {
                // Toroidal wrapping on cell coordinates
                let nx = u32((i32(cell_x) + dx + i32(side)) % i32(side));
                let ny = u32((i32(cell_y) + dy + i32(side)) % i32(side));
                let nz = u32((i32(cell_z) + dz + i32(side)) % i32(side));
                let cell_idx = nx + ny * side + nz * side * side;

                // Range of particles in this cell
                let start = cell_offsets[cell_idx];
                let end = cell_offsets[cell_idx + 1u];

                for (var j: u32 = start; j < end; j++) {
                    if (j == idx) { continue; }  // skip self

                    let body1 = particles[j];

                    // Toroidal shortest-path distance
                    var min_pos = body1.pos - body0.pos + vec3<f32>(0.5);
                    min_pos = fract(min_pos) - vec3<f32>(0.5);

                    let dist_sqrd = dot(min_pos, min_pos);

                    // Early exits
                    if (dist_sqrd > params.max_dist_sqrd || dist_sqrd < 1e-30) {
                        continue;
                    }

                    let dist = sqrt(dist_sqrd);
                    let rel_dist = dist * params.max_dist_recip;
                    let dir = min_pos / dist;

                    var force_scalar: f32;

                    if (rel_dist <= params.min_rel_dist) {
                        // Repulsion zone
                        force_scalar = rel_dist * params.min_dist_recip - 1.0;
                    } else {
                        // Attraction/repulsion zone based on force matrix
                        let neighbor_color = body1.color;
                        let matrix_val = force_matrix[source_color * params.color_count + neighbor_color];

                        if (matrix_val == 0.0) {
                            continue;
                        }

                        var f = matrix_val;
                        if (f > 0.0) {
                            f = f * density_factor;
                        }
                        force_scalar = f * (1.0 - (1.0 + params.min_rel_dist - 2.0 * rel_dist) * params.inv_min_dist_recip);
                    }

                    total_force += dir * (force_scalar * params.max_dist);

                    // Density accumulation
                    let weight = select(params.density_diff_color, params.density_same_color, source_color == body1.color);
                    total_density += weight * (1.0 - rel_dist);
                }
            }
        }
    }

    results[idx] = Result(total_force, total_density);
}
