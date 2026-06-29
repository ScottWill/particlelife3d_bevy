# Implementation Plan: GPU Force Compute

## Overview

This plan implements GPU-accelerated pairwise force computation for the particle life simulation. The implementation follows a bottom-up approach: first establishing the backend resource and module structure, then building the CPU-side spatial sort, buffer management, WGSL shader, render graph integration, readback pipeline, and finally UI controls. Each task builds incrementally so that the system is wired together and testable at each checkpoint.

## Tasks

- [x] 1. Set up module structure and ForceBackend resource
  - [x] 1.1 Create `src/physics/backend.rs` with the `ForceBackend` enum resource
    - Define `ForceBackend` enum with `Gpu` and `Cpu` variants, derive Resource, Clone, Copy, PartialEq, Eq
    - Implement `Default` to return `Gpu`
    - _Requirements: 8.1_
  - [x] 1.2 Create `src/physics/gpu/mod.rs` with `GpuForcePlugin` stub and module declarations
    - Declare submodules: `pipeline`, `buffers`, `extract`, `node`, `sort`
    - Define `GpuForcePlugin` struct implementing `Plugin` with an empty `build` method for now
    - Re-export key types (`GpuForcePlugin`)
    - _Requirements: 6.1, 6.3_
  - [x] 1.3 Create stub files for `src/physics/gpu/pipeline.rs`, `buffers.rs`, `extract.rs`, `node.rs`, `sort.rs`
    - Each file should have a brief module doc comment and empty content for now
    - _Requirements: 6.3_
  - [x] 1.4 Wire new modules into `src/physics/mod.rs` and register `GpuForcePlugin` in `ParticlePhysicsPlugin`
    - Add `pub mod gpu;` and `pub mod backend;` to `src/physics/mod.rs`
    - Re-export `ForceBackend` and `GpuForcePlugin`
    - Add `GpuForcePlugin` and `ForceBackend::default()` resource init to `ParticlePhysicsPlugin::build`
    - _Requirements: 6.1, 8.1_
  - [x] 1.5 Add `bytemuck` and `crossbeam-channel` dependencies to `Cargo.toml`
    - Add `bytemuck = { version = "1", features = ["derive"] }` and `crossbeam-channel = "0.5"` under `[dependencies]`
    - _Requirements: 1.1, 4.2_

- [x] 2. Implement CPU-side counting sort (sort.rs)
  - [x] 2.1 Implement `sort_particles_by_cell` function in `src/physics/gpu/sort.rs`
    - Takes `&[BodySnapshot]` and `grid_side: usize`, returns `(Vec<u8>, Vec<u32>, Vec<u32>)` — sorted particle buffer bytes, cell_offsets, and original_indices (reverse index map)
    - Pass 1: Histogram (count particles per cell)
    - Pass 2: Prefix sum to build cell_offsets (length = grid_side³ + 1)
    - Pass 3: Scatter into sorted buffer (16 bytes per particle: f32×3 position + u32 color) and build original_indices map
    - Cell index formula: `floor(pos.x * grid_side).clamp(0, grid_side-1)` etc., then `x + y*side + z*side*side`
    - _Requirements: 3.1, 3.3, 3.5_
  - [x] 2.2 Implement `compute_grid_side` helper function
    - Takes `max_dist: f64`, returns `usize`
    - Formula: `floor(1.0 / max_dist)` clamped to max 100
    - _Requirements: 3.4, 3.6_
  - [x] 2.3 Write property test for counting sort partitioning invariant
    - **Property 2: Counting Sort Partitioning Invariant**
    - Generate random particle positions (100-5000 particles), random grid_side (5-30)
    - Verify: cell_offsets length = grid_side³ + 1, non-decreasing, last entry = particle_count, particles in correct cells, total count preserved
    - **Validates: Requirements 3.1, 3.3**
  - [x] 2.4 Write property test for grid dimension calculation
    - **Property 3: Grid Dimension Calculation**
    - Generate random max_dist in [0.01, 0.2], verify grid_side formula and clamping at 100
    - **Validates: Requirements 3.4, 3.6**
  - [x] 2.5 Write property test for data conversion round-trip
    - **Property 1: Data Conversion Round-Trip**
    - Generate random BodySnapshot vectors (positions in [0,1), colors in [0,9)), serialize to buffer bytes, deserialize, compare within f32 epsilon
    - **Validates: Requirements 1.1, 1.2, 1.3, 4.2**

- [x] 3. Checkpoint - Ensure sort module compiles and tests pass
  - Ensure all tests pass, ask the user if questions arise.

- [x] 4. Implement WGSL compute shader
  - [x] 4.1 Create `src/physics/gpu/shader.wgsl` with the full compute shader
    - Define `Particle`, `Result`, and `Params` structs
    - Bind group layout: particles (binding 0), cell_offsets (binding 1), params (binding 2), force_matrix (binding 3), prev_densities (binding 4), results (binding 5)
    - Implement `@compute @workgroup_size(256) fn main` with 3×3×3 neighbor cell iteration, toroidal wrapping, piecewise force function, density accumulation, and density attenuation logic
    - _Requirements: 2.1, 2.2, 2.3, 2.4, 2.5, 2.6, 2.7, 2.8, 2.9, 2.10, 5.1, 5.6_
  - [x] 4.2 Write property test for toroidal cell neighbor wrapping
    - **Property 4: Toroidal Cell Neighbor Wrapping**
    - Generate random cell coordinate and grid_side (2-50), compute all 27 neighbors, verify bounds and wrapping at edges
    - **Validates: Requirements 3.2**
  - [x] 4.3 Write property test for piecewise force function correctness
    - **Property 5: Piecewise Force Function Correctness**
    - Generate random particle pairs, force matrix, physics params; run force function (pure Rust reference using f32); verify output matches formula
    - **Validates: Requirements 2.1, 2.2, 2.3, 2.4, 2.5, 2.6, 2.7, 5.6**
  - [x] 4.4 Write property test for density attenuation selectivity
    - **Property 6: Density Attenuation Selectivity**
    - Generate random (matrix_value, prev_density, density_limit, attenuation_enabled) tuples; verify attenuation logic (positive values attenuated, negative unchanged)
    - **Validates: Requirements 2.9, 2.10, 5.3, 9.2, 9.3**

- [x] 5. Implement GPU buffer management (buffers.rs)
  - [x] 5.1 Define `GpuForceBuffers` resource and `GpuSimParams` struct in `src/physics/gpu/buffers.rs`
    - `GpuForceBuffers`: particle_buffer, cell_offset_buffer, params_buffer, force_matrix_buffer, prev_density_buffer, result_buffer, staging_buffers (double-buffered [Buffer; 2]), active_staging index, capacity
    - `GpuSimParams`: all f32 fields matching the WGSL Params struct, implement `bytemuck::Pod` and `Zeroable`
    - Define `GpuResult` struct (force_x, force_y, force_z, density as f32) with bytemuck derives
    - _Requirements: 1.1, 1.2, 1.4, 4.1_
  - [x] 5.2 Implement buffer creation and reallocation logic
    - Function to create all buffers for a given particle count and grid_side using `RenderDevice`
    - Handle particle count changes by reallocating particle_buffer, result_buffer, staging_buffers, and prev_density_buffer
    - Use `BufferUsages::STORAGE | COPY_SRC` for result_buffer, `MAP_READ | COPY_DST` for staging_buffers
    - _Requirements: 1.5, 1.7, 6.3, 7.3_
  - [x] 5.3 Implement buffer upload functions
    - Upload sorted particle data, cell_offsets, params, force matrix, and remapped prev_densities via `RenderQueue::write_buffer`
    - Convert SimulationConfig f64 values to f32 for GpuSimParams
    - _Requirements: 1.1, 1.2, 1.3, 1.4, 1.6, 9.1_

- [x] 6. Implement extract systems (extract.rs)
  - [x] 6.1 Implement render world extraction system in `src/physics/gpu/extract.rs`
    - Define `ExtractedSnapshots`, `ExtractedConfig`, `ExtractedForceMatrix`, `ExtractedAttenuation` render-world resources
    - Implement extract system that copies `BodySnapshots`, `SimulationConfig`, `ForceMatrix`, `DensityAttenuation` from main world when `ForceBackend::Gpu`
    - Skip extraction when backend is CPU
    - _Requirements: 6.5, 6.4, 8.5_

- [x] 7. Implement compute pipeline setup (pipeline.rs)
  - [x] 7.1 Define bind group layout and compute pipeline in `src/physics/gpu/pipeline.rs`
    - Create `BindGroupLayout` with 6 bindings matching shader.wgsl
    - Use `PipelineCache` to create the compute pipeline from shader.wgsl
    - Define `GpuForcePipeline` resource holding the pipeline id and bind group layout
    - _Requirements: 6.2, 6.3_

- [x] 8. Implement render graph compute node (node.rs)
  - [x] 8.1 Implement `ForceComputeNode` in `src/physics/gpu/node.rs`
    - Implement Bevy's render graph `Node` trait
    - In `run`: create bind group from current buffers, begin compute pass, set pipeline, set bind group, dispatch `ceil(particle_count / 256)` workgroups, copy result_buffer to active staging_buffer
    - _Requirements: 6.1, 6.2, 2.8_

- [x] 9. Wire up the GPU plugin (gpu/mod.rs)
  - [x] 9.1 Complete `GpuForcePlugin::build` implementation
    - Register extract system in the `ExtractSchedule`
    - Register prepare system (sort + upload) in render world's `Prepare` schedule
    - Add `ForceComputeNode` to Bevy's render graph
    - Initialize render-world resources (`GpuPrevDensities`, etc.)
    - _Requirements: 6.1, 6.3, 6.5_

- [x] 10. Checkpoint - Ensure GPU pipeline compiles and integrates
  - Ensure all tests pass, ask the user if questions arise.

- [x] 11. Implement readback and CPU fallback
  - [x] 11.1 Implement async readback system using crossbeam-channel
    - In a main-world system running before `compute_forces`: check `map_async` result via `try_recv`
    - On success: remap results using `original_indices` into `ParticleComputations` resource, store current densities for next tick's prev_densities
    - On failure/timeout: leave computations empty (triggers CPU fallback)
    - Use double-buffered staging: read from `staging_buffers[1 - active_staging]`, write new dispatch to `staging_buffers[active_staging]`
    - _Requirements: 4.1, 4.2, 4.3, 4.4, 7.4_
  - [x] 11.2 Modify `compute_forces` system to check `ForceBackend` and use GPU results or CPU fallback
    - When `ForceBackend::Gpu`: check if GPU computations are ready (populated by readback system); if ready, use them; if not, run existing Rayon CPU path
    - When `ForceBackend::Cpu`: run existing Rayon CPU path unchanged
    - Extract the existing Rayon logic into a helper function `run_cpu_forces` called by both paths
    - _Requirements: 8.2, 8.3, 4.4, 5.2_
  - [x] 11.3 Write property test for GPU-CPU behavioral parity
    - **Property 7: GPU-CPU Behavioral Parity**
    - Generate small particle sets (10-100 particles, 2-5 colors), run f32 reference implementation (pure Rust matching shader logic) and f64 CPU implementation, compare within 1e-3
    - **Validates: Requirements 2.11, 5.1, 5.2, 5.5, 9.6**

- [x] 12. Implement error handling and GPU availability detection
  - [x] 12.1 Add GPU compute support detection system
    - On startup (in render world): check device limits (max_storage_buffer_binding_size >= 8MB)
    - If GPU is insufficient: set `ForceBackend` to `Cpu`, log warning
    - _Requirements: 8.1, 8.4, 7.5_
  - [x] 12.2 Handle buffer creation failure, shader compilation failure, and device loss
    - Wrap buffer creation in error handling: on failure, set backend to CPU, log error
    - Monitor `PipelineCache` for shader compilation errors: on failure, set backend to CPU, log error
    - Handle `map_async` errors (device lost): set backend to CPU, log event
    - Track consecutive readback timeouts: after 3, switch to CPU permanently with warning
    - _Requirements: 6.6, 4.4, 7.5_

- [x] 13. Add backend selector to settings panel
  - [x] 13.1 Add backend selector UI to `src/settings_panel.rs`
    - Add a "Backend" section or combo box in the Performance collapsing header
    - Show current backend (GPU / CPU) and allow switching
    - If GPU unavailable, show disabled state with reason text (e.g., "GPU unavailable: insufficient VRAM")
    - Import `ForceBackend` resource and add it to the `render_panel` system parameters
    - _Requirements: 8.6, 8.4_

- [x] 14. Final checkpoint - Ensure full pipeline works end-to-end
  - Ensure all tests pass, ask the user if questions arise.

## Notes

- Tasks marked with `*` are optional and can be skipped for faster MVP
- Each task references specific requirements for traceability
- Checkpoints ensure incremental validation
- Property tests validate universal correctness properties from the design document
- Unit tests validate specific examples and edge cases
- The one-tick latency model means GPU results from tick N are consumed in tick N+1
- The double-buffered staging approach avoids blocking on previous frame's map completion
- All GPU buffer data uses f32 precision; CPU physics retains f64 precision
- The `original_indices` reverse map is critical for correctly mapping sorted GPU results back to ECS entity order

## Task Dependency Graph

```json
{
  "waves": [
    { "id": 0, "tasks": ["1.1", "1.2", "1.3", "1.5"] },
    { "id": 1, "tasks": ["1.4", "2.1", "2.2"] },
    { "id": 2, "tasks": ["2.3", "2.4", "2.5", "4.1"] },
    { "id": 3, "tasks": ["4.2", "4.3", "4.4", "5.1"] },
    { "id": 4, "tasks": ["5.2", "5.3", "6.1"] },
    { "id": 5, "tasks": ["7.1"] },
    { "id": 6, "tasks": ["8.1"] },
    { "id": 7, "tasks": ["9.1"] },
    { "id": 8, "tasks": ["11.1", "11.2"] },
    { "id": 9, "tasks": ["11.3", "12.1", "12.2"] },
    { "id": 10, "tasks": ["13.1"] }
  ]
}
```
