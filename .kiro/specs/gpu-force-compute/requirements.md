# Requirements Document

## Introduction

This feature moves the pairwise force computation step of the particle life simulation from the CPU (Rayon parallel iteration over a spatial grid) to a GPU compute shader. The goal is to achieve significantly better performance at high particle counts (50k–500k) by leveraging GPU parallelism for the O(n × neighbors) force calculation. The rest of the physics pipeline (snapshotting, island assignment, force application, position integration) may remain on the CPU or optionally move to GPU as well, depending on the data transfer cost trade-off.

## Glossary

- **Compute_Shader**: A wgpu compute shader dispatched via Bevy's render graph that runs the pairwise force calculation on the GPU
- **Force_Pipeline**: The full system responsible for uploading particle data, dispatching the compute shader, and reading back results
- **Particle_Buffer**: A GPU storage buffer containing per-particle data (position and color index) uploaded each physics tick
- **Params_Buffer**: A GPU uniform buffer containing simulation configuration parameters (max_dist, min_rel_dist, density thresholds, etc.)
- **Force_Matrix_Buffer**: A GPU storage buffer containing the flattened color×color force matrix
- **Result_Buffer**: A GPU storage buffer where the compute shader writes per-particle force vectors and density values, read back to CPU
- **Spatial_Sort**: A GPU-friendly spatial partitioning strategy (e.g., Morton-code sorting or grid-based binning) replacing the CPU island grid for neighbor lookups on the GPU
- **Behavioral_Parity**: The requirement that GPU-computed forces produce visually and numerically equivalent results to the CPU implementation, within f32 floating-point tolerance
- **Readback**: The process of copying GPU buffer contents back to CPU-accessible memory for downstream consumption

## Requirements

### Requirement 1: GPU Buffer Upload

**User Story:** As a simulation developer, I want particle data uploaded to GPU buffers each physics tick, so that the compute shader has current positions and colors to work with.

#### Acceptance Criteria

1. WHEN a physics tick begins, THE Force_Pipeline SHALL upload all particle positions (converted from DVec3 f64 to three contiguous f32 values per particle) and color indices (cast from usize to u32) to the Particle_Buffer, laid out as repeating strides of [f32; 3] position followed by one u32 color index per particle
2. WHEN the SimulationConfig resource changes, THE Force_Pipeline SHALL update the Params_Buffer with the current max_dist, min_rel_dist, density_limit, density_same_color, and density_diff_color values, each converted from f64 to f32
3. WHEN the ForceMatrix resource changes, THE Force_Pipeline SHALL upload the flattened force matrix data (up to color_count² entries, maximum 81) to the Force_Matrix_Buffer as f32 values converted from the source f64 values
4. THE Force_Pipeline SHALL use f32 precision for all GPU-side position and force data, converting from f64 source values by truncation to f32
5. IF the particle count changes between ticks, THEN THE Force_Pipeline SHALL reallocate the Particle_Buffer and Result_Buffer to match the new particle count (ranging from 100 to 500,000 particles) before the next upload occurs
6. WHEN buffer uploads for a physics tick are complete, THE Force_Pipeline SHALL trust the GPU's completion signal and proceed with compute shader dispatch once uploads are marked complete
7. IF the actual particle count exceeds 500,000 at upload time, THEN THE Force_Pipeline SHALL clamp the upload to 500,000 particles and skip any excess entries

### Requirement 2: Compute Shader Force Calculation

**User Story:** As a simulation developer, I want pairwise forces computed on the GPU, so that the simulation can handle high particle counts with better performance than CPU Rayon.

#### Acceptance Criteria

1. THE Compute_Shader SHALL compute, for each particle, the sum of pairwise forces from all particles within max_dist, using toroidal wrapping defined as min_pos = (body1.position - body0.position + 0.5) mod 1.0 - 0.5
2. THE Compute_Shader SHALL skip a particle pair when the squared distance is greater than max_dist² or less than 1e-30
3. THE Compute_Shader SHALL compute the force magnitude for rel_dist <= min_rel_dist as (rel_dist / min_rel_dist - 1.0), representing repulsion that increases as particles approach
4. THE Compute_Shader SHALL compute the force magnitude for rel_dist > min_rel_dist as force_matrix[(source_color, target_color)] * (1.0 - (1.0 + min_rel_dist - 2.0 * rel_dist) / (1.0 - min_rel_dist)), where rel_dist = dist / max_dist
5. IF the force_matrix entry for a pair is 0.0 and rel_dist > min_rel_dist, THEN THE Compute_Shader SHALL skip that pair and contribute zero force and zero density
6. THE Compute_Shader SHALL multiply the final scalar force by the unit direction vector and by max_dist to produce the per-particle force contribution as a vec3
7. THE Compute_Shader SHALL compute per-particle density as the sum of (weight * (1.0 - rel_dist)) for each neighbor within max_dist, where weight is density_same_color when source and target share the same color index, and density_diff_color otherwise
8. THE Compute_Shader SHALL write per-particle force (vec3) and density (f32) to the Result_Buffer
9. WHEN density attenuation is enabled, THE Compute_Shader SHALL attenuate only positive (attractive) force_matrix entries by the factor (1.0 - clamp(prev_density - density_limit, 0.0, 1.0)), leaving negative (repulsive) entries unmodified
10. WHEN density attenuation is disabled, THE Compute_Shader SHALL apply a density factor of 1.0 to all force calculations
11. THE Compute_Shader SHALL produce per-particle force and density values that match the CPU Rayon implementation within a tolerance of 1e-3 for any configuration of up to 500,000 particles and up to 9 colors

### Requirement 3: GPU Spatial Partitioning

**User Story:** As a simulation developer, I want a GPU-compatible spatial partitioning strategy, so that the compute shader avoids O(n²) all-pairs computation.

#### Acceptance Criteria

1. THE Spatial_Sort SHALL partition particles into grid cells by mapping each particle position (wrapped to [0.0, 1.0) per axis) to a cell index using floor(position_component × grid_side), such that each compute shader thread only examines particles in the same and 26 adjacent cells (3×3×3 neighborhood)
2. THE Spatial_Sort SHALL support toroidal wrapping when determining adjacent cells by using modular arithmetic (rem_euclid) on cell coordinates, so that cells at grid boundaries neighbor cells on the opposite edge
3. THE Spatial_Sort SHALL produce a cell-offset array (one entry per cell storing the start index into the particle buffer) and a particle buffer sorted by cell index, such that all particles belonging to the same cell occupy a contiguous range in the buffer
4. WHEN max_dist changes, THE Spatial_Sort SHALL recompute the grid dimensions where side = floor(1.0 / max_dist), yielding side³ total cells (ranging from 125 cells at max_dist=0.2 to 10,648 cells at max_dist=0.045)
5. THE Spatial_Sort SHALL handle up to 500,000 particles with total partitioning data structure memory usage not exceeding 9 MB (particle buffer: 500,000 × 16 bytes = 8 MB for position + color; cell-offset array: up to 10,648 × 4 bytes = 42 KB at default grid size)
6. IF max_dist is set to a value that would produce more than 1,000,000 cells (side > 100), THEN THE Spatial_Sort SHALL clamp grid_side to 100 and proceed with the clamped grid dimensions

### Requirement 4: Result Readback

**User Story:** As a simulation developer, I want computed forces read back from the GPU to CPU memory, so that the existing apply_forces system can integrate velocities and positions.

#### Acceptance Criteria

1. WHEN the compute shader finishes execution, THE Force_Pipeline SHALL copy the Result_Buffer contents to a CPU-accessible staging buffer, where Result_Buffer index N corresponds to particle N in the BodySnapshots ordering
2. WHEN the staging buffer readback completes, THE Force_Pipeline SHALL populate the ParticleComputations resource with exactly one entry per particle, where each entry contains the force as DVec3 (widened from f32 with no additional rounding) and density as f64 (widened from f32 with no additional rounding)
3. THE Force_Pipeline SHALL complete the readback and ParticleComputations population before the apply_forces system runs in the PhysicsSet within the same FixedUpdate tick, treating any readback that exceeds 16ms as a failure
4. IF the map_async callback returns an error, the GPU device is lost, or the readback does not complete within 16ms, THEN THE Force_Pipeline SHALL clear existing particle computation data, discard the partial GPU result, and fall back to the existing CPU Rayon force computation for that tick, producing identical ParticleComputations output as the CPU path

### Requirement 5: Behavioral Parity

**User Story:** As a user, I want the GPU-computed simulation to look and behave the same as the CPU version, so that switching to GPU does not change the emergent patterns I observe.

#### Acceptance Criteria

1. THE Compute_Shader SHALL implement the same force function as the CPU get_computation function: toroidal shortest-path distance via `(pos1 - pos0 + 0.5).rem_euclid(1.0) - 0.5`, min_rel_dist threshold separating repulsion and attraction zones, force matrix lookup by `(source_color, neighbor_color)`, and density weighting using `weight * (1.0 - rel_dist)` where weight is density_same_color for same-color pairs and density_diff_color for different-color pairs
2. THE Force_Pipeline SHALL produce per-particle force components that match the CPU implementation within 1e-3 absolute tolerance per component for any single tick given identical input positions, colors, force matrix, and simulation parameters, accounting for f64-to-f32 precision reduction and summation-order differences
3. THE Force_Pipeline SHALL preserve the density attenuation toggle behavior: WHEN enabled, only positive (attractive) force values are multiplied by the attenuation factor `1.0 - (previous_tick_density - density_limit).clamp(0.0, 1.0)`, capped at a maximum of 1.0 to prevent force amplification when density is below the limit; negative (repulsive) force values are never attenuated; WHEN disabled, the density factor is 1.0 for all forces
4. THE Compute_Shader SHALL skip force contributions when distance squared exceeds max_dist_squared or is below 1e-30, and SHALL skip self-interaction (source particle index equals neighbor particle index), matching the CPU early-exit conditions
5. THE Force_Pipeline SHALL produce per-particle density accumulations that match the CPU implementation within 1e-3 absolute tolerance per particle for any single tick given identical inputs, using the same density contribution formula and same-color/diff-color weight distinction as the CPU
6. THE Compute_Shader SHALL implement the same piecewise force curve as the CPU: when rel_dist is less than or equal to min_rel_dist, force equals `rel_dist / min_rel_dist - 1.0` (repulsion); when rel_dist exceeds min_rel_dist, force equals `matrix_value * (1.0 - (1.0 + min_rel_dist - 2.0 * rel_dist) / (1.0 - min_rel_dist))` scaled by the density attenuation factor for positive matrix values only

### Requirement 6: Integration with Bevy Render Graph

**User Story:** As a simulation developer, I want the compute shader integrated into Bevy's render graph, so that GPU work is scheduled correctly relative to rendering.

#### Acceptance Criteria

1. WHEN the render graph executes a frame, THE Force_Pipeline SHALL register a compute pass node that dispatches once per frame using the most recent physics snapshot extracted from the main world
2. THE Force_Pipeline SHALL insert a write-after-write or write-after-read buffer barrier between buffer upload and compute shader dispatch, ensuring particle data is fully available in GPU memory before the shader reads it
3. THE Force_Pipeline SHALL use Bevy's wgpu abstraction layer (RenderDevice, RenderQueue, PipelineCache) for buffer creation, bind group layout, and shader module compilation
4. WHILE the PhysicsRunState is Paused, THE Force_Pipeline SHALL skip both compute shader dispatch and buffer uploads, leaving the previous frame's GPU buffer contents unchanged
5. WHEN the extract phase runs, THE Force_Pipeline SHALL copy the latest BodySnapshots resource from the main world into the render world so the compute pass operates on up-to-date particle positions and colors; IF the copy operation fails, THE Force_Pipeline SHALL proceed with stale data from the previous frame
6. IF buffer creation or shader module compilation fails during initialization, THEN THE Force_Pipeline SHALL log an error indicating the failure reason and disable the compute pass node without crashing the application

### Requirement 7: Performance

**User Story:** As a user, I want the GPU force computation to be faster than the CPU version, so that I can run more particles at interactive frame rates.

#### Acceptance Criteria

1. THE Force_Pipeline SHALL achieve at least 2x higher throughput (particles processed per second) than the CPU Rayon implementation for particle counts above 10,000, measured as the wall-clock duration of the force computation step per tick
2. THE Force_Pipeline SHALL complete the force computation step within 33ms per tick (maintaining above 30 FPS) with 50,000 particles on discrete GPUs with at least 4GB VRAM
3. IF the particle count is 50,000 or fewer, THEN THE Force_Pipeline SHALL not introduce per-frame CPU stalls longer than 2ms for buffer upload and readback operations; for particle counts above 50,000, stall limits scale proportionally (e.g., 4ms at 100,000 particles)
4. WHERE the wgpu backend supports map_async, THE Force_Pipeline SHALL use asynchronous readback, allowing the CPU to continue scheduling non-GPU work while waiting for GPU results
5. IF the GPU device is unavailable or lacks sufficient VRAM for the current particle count, THEN THE Force_Pipeline SHALL fall back to the CPU Rayon implementation without major disruption to the simulation, though brief transition pauses for reinitializing data structures are acceptable

### Requirement 8: Runtime Backend Selection

**User Story:** As a user, I want to switch between GPU and CPU force computation at runtime, so that I can compare behavior or fall back if my GPU is unsupported.

#### Acceptance Criteria

1. THE Force_Pipeline SHALL expose a Bevy resource that holds the currently selected force computation backend (GPU or CPU), defaulting to GPU when the wgpu adapter reports compute shader support, and defaulting to CPU otherwise
2. WHEN the user switches from GPU to CPU backend, THE Force_Pipeline SHALL immediately deactivate all GPU resources and use the Rayon-based compute_forces system starting on the next physics tick, with no simulation pause or frame skip during the transition
3. WHEN the user switches from CPU to GPU backend, THE Force_Pipeline SHALL immediately allocate GPU buffers and begin GPU dispatch starting on the next physics tick, with no simulation pause or frame skip during the transition
4. IF the user selects GPU backend and the wgpu adapter does not support compute shaders, THEN THE Force_Pipeline SHALL remain on the CPU backend and display an indication in the settings panel that GPU is unavailable
5. WHILE PhysicsRunState is Paused, THE Force_Pipeline SHALL not execute force computation on either backend
6. THE settings panel SHALL display the currently active backend (GPU or CPU) and provide a control to switch between them

### Requirement 9: Density Attenuation State

**User Story:** As a user, I want the density attenuation toggle to work identically on GPU as on CPU, so that toggling it at runtime produces the same visual change.

#### Acceptance Criteria

1. WHEN density attenuation is toggled at runtime, THE Force_Pipeline SHALL write the new attenuation-enabled flag (u32: 0 = disabled, 1 = enabled) to the Params_Buffer before the next compute dispatch
2. WHILE density attenuation is enabled, THE Compute_Shader SHALL compute density_factor as (1.0 - clamp(prev_density - density_limit, 0.0, 1.0)) and multiply only positive force_matrix entries by density_factor; negative force_matrix entries SHALL NOT be attenuated
3. WHILE density attenuation is disabled, THE Compute_Shader SHALL use a density_factor of 1.0 for all force_matrix entries regardless of previous density values
4. WHILE density attenuation is enabled, THE Force_Pipeline SHALL pass the previous tick's per-particle density values (f32 from the prior dispatch's Result_Buffer) to the compute shader as an input buffer
5. IF density attenuation is toggled from disabled to enabled and no previous density data exists, THEN THE Compute_Shader SHALL use 0.0 for all previous density values on that tick, matching the CPU behavior where prev_densities defaults to 0.0
6. WHILE density attenuation is disabled, THE Compute_Shader SHALL still write per-particle density values to the Result_Buffer each dispatch so that valid previous densities are immediately available if attenuation is re-enabled on the next tick
