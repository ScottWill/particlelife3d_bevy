//! GPU buffer allocation, upload, and readback logic.
//!
//! Defines the GPU-side data structures for the force computation pipeline:
//! - [`GpuForceBuffers`]: Render-world resource holding all wgpu buffers
//! - [`GpuSimParams`]: C-layout struct matching the WGSL `Params` uniform
//! - [`GpuResult`]: Per-particle output (force vector + density) from the compute shader

use bevy::prelude::*;
use bevy::render::render_resource::{Buffer, BufferDescriptor, BufferUsages};
use bevy::render::renderer::{RenderDevice, RenderQueue};

/// Per-particle result written by the compute shader.
///
/// Layout matches the WGSL `Result` struct: 16 bytes total (4×f32).
#[repr(C)]
#[derive(Clone, Copy, Default, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuResult {
    pub force_x: f32,
    pub force_y: f32,
    pub force_z: f32,
    pub density: f32,
}

/// Simulation parameters uploaded as a uniform buffer to the compute shader.
///
/// Layout matches the WGSL `Params` struct exactly (56 bytes, 14×f32/u32 fields).
/// Padded to 64 bytes for uniform buffer alignment requirements.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuSimParams {
    pub max_dist: f32,
    pub min_rel_dist: f32,
    pub max_dist_sqrd: f32,
    pub max_dist_recip: f32,
    pub min_dist_recip: f32,
    pub inv_min_dist_recip: f32,
    pub density_limit: f32,
    pub density_same_color: f32,
    pub density_diff_color: f32,
    pub attenuation_enabled: u32,
    pub particle_count: u32,
    pub grid_side: u32,
    pub color_count: u32,
    pub _padding: u32,
}

/// Render-world resource holding all GPU buffers for the force computation pipeline.
///
/// Uses double-buffered staging for async readback without blocking the next frame's
/// copy operation.
#[derive(Resource)]
pub struct GpuForceBuffers {
    /// Storage buffer containing sorted particle data (f32×3 position + u32 color per particle).
    pub particle_buffer: Buffer,
    /// Storage buffer containing cell offset array for spatial partitioning lookups.
    pub cell_offset_buffer: Buffer,
    /// Uniform buffer containing simulation parameters ([`GpuSimParams`]).
    pub params_buffer: Buffer,
    /// Storage buffer containing the flattened color×color force matrix (f32 values).
    pub force_matrix_buffer: Buffer,
    /// Storage buffer containing per-particle density values from the previous tick.
    pub prev_density_buffer: Buffer,
    /// Storage buffer where the compute shader writes per-particle results ([`GpuResult`]).
    pub result_buffer: Buffer,
    /// Double-buffered staging buffers for async CPU readback (MAP_READ + COPY_DST).
    /// Each tick writes to `staging_buffers[active_staging]` and reads from the other.
    pub staging_buffers: [Buffer; 2],
    /// Index (0 or 1) of the staging buffer currently being written to by the GPU.
    pub active_staging: usize,
    /// Current buffer capacity in number of particles (used for reallocation detection).
    pub capacity: u32,
    /// Current grid side length (used for reallocation detection of cell_offset_buffer).
    pub grid_side: u32,
}

impl GpuForceBuffers {
    /// Creates all GPU buffers for the given particle count and grid dimensions.
    pub fn new(device: &RenderDevice, particle_count: u32, grid_side: u32) -> Self {
        let particle_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("gpu_force_particle_buffer"),
            size: particle_count as u64 * 16,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let cell_count = (grid_side as u64) * (grid_side as u64) * (grid_side as u64);
        let cell_offset_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("gpu_force_cell_offset_buffer"),
            size: (cell_count + 1) * 4,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let params_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("gpu_force_params_buffer"),
            size: 64, // GpuSimParams padded to 64 bytes
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let force_matrix_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("gpu_force_matrix_buffer"),
            size: 81 * 4, // max 9×9 force matrix of f32
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let prev_density_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("gpu_force_prev_density_buffer"),
            size: particle_count as u64 * 4,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let result_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("gpu_force_result_buffer"),
            size: particle_count as u64 * 16,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let staging_buffers = [
            device.create_buffer(&BufferDescriptor {
                label: Some("gpu_force_staging_buffer_0"),
                size: particle_count as u64 * 16,
                usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            device.create_buffer(&BufferDescriptor {
                label: Some("gpu_force_staging_buffer_1"),
                size: particle_count as u64 * 16,
                usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
        ];

        Self {
            particle_buffer,
            cell_offset_buffer,
            params_buffer,
            force_matrix_buffer,
            prev_density_buffer,
            result_buffer,
            staging_buffers,
            active_staging: 0,
            capacity: particle_count,
            grid_side,
        }
    }

    /// Reallocates buffers if particle count or grid side changed.
    /// Returns true if any reallocation occurred.
    pub fn reallocate_if_needed(
        &mut self,
        device: &RenderDevice,
        particle_count: u32,
        grid_side: u32,
    ) -> bool {
        let mut reallocated = false;

        // Reallocate particle-count-dependent buffers when count changes
        if particle_count != self.capacity {
            self.particle_buffer = device.create_buffer(&BufferDescriptor {
                label: Some("gpu_force_particle_buffer"),
                size: particle_count as u64 * 16,
                usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            self.result_buffer = device.create_buffer(&BufferDescriptor {
                label: Some("gpu_force_result_buffer"),
                size: particle_count as u64 * 16,
                usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });

            self.staging_buffers = [
                device.create_buffer(&BufferDescriptor {
                    label: Some("gpu_force_staging_buffer_0"),
                    size: particle_count as u64 * 16,
                    usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                device.create_buffer(&BufferDescriptor {
                    label: Some("gpu_force_staging_buffer_1"),
                    size: particle_count as u64 * 16,
                    usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
            ];

            self.prev_density_buffer = device.create_buffer(&BufferDescriptor {
                label: Some("gpu_force_prev_density_buffer"),
                size: particle_count as u64 * 4,
                usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            self.capacity = particle_count;
            reallocated = true;
        }

        // Reallocate cell offset buffer when grid side changes
        if grid_side != self.grid_side {
            let cell_count = (grid_side as u64) * (grid_side as u64) * (grid_side as u64);
            self.cell_offset_buffer = device.create_buffer(&BufferDescriptor {
                label: Some("gpu_force_cell_offset_buffer"),
                size: (cell_count + 1) * 4,
                usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            self.grid_side = grid_side;
            reallocated = true;
        }

        reallocated
    }

    /// Uploads sorted particle data to the GPU.
    ///
    /// The `sorted_data` slice should contain particle data in the sorted-by-cell order,
    /// laid out as 16 bytes per particle: [f32; 3] position followed by u32 color index.
    pub fn upload_particles(&self, queue: &RenderQueue, sorted_data: &[u8]) {
        queue.write_buffer(&self.particle_buffer, 0, sorted_data);
    }

    /// Uploads the cell offset array to the GPU.
    ///
    /// The `offsets` slice has length = grid_side³ + 1, where entry `i` is the start index
    /// of cell `i` in the sorted particle buffer and the final entry is the total particle count.
    pub fn upload_cell_offsets(&self, queue: &RenderQueue, offsets: &[u32]) {
        queue.write_buffer(&self.cell_offset_buffer, 0, bytemuck::cast_slice(offsets));
    }

    /// Uploads simulation parameters to the GPU uniform buffer.
    pub fn upload_params(&self, queue: &RenderQueue, params: &GpuSimParams) {
        queue.write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(params));
    }

    /// Uploads the force matrix (converted from f64 to f32) to the GPU.
    ///
    /// The `data` slice contains the flattened color×color force matrix in f64.
    /// Only the first `color_count²` entries are uploaded.
    pub fn upload_force_matrix(&self, queue: &RenderQueue, data: &[f64], color_count: usize) {
        let count = color_count * color_count;
        let f32_data: Vec<f32> = data.iter().take(count).map(|&v| v as f32).collect();
        queue.write_buffer(
            &self.force_matrix_buffer,
            0,
            bytemuck::cast_slice(&f32_data),
        );
    }

    /// Uploads previous density values (remapped to sorted order) to the GPU.
    ///
    /// The `densities` slice should already be in sorted-particle order for the current tick.
    pub fn upload_prev_densities(&self, queue: &RenderQueue, densities: &[f32]) {
        queue.write_buffer(
            &self.prev_density_buffer,
            0,
            bytemuck::cast_slice(densities),
        );
    }
}

impl GpuSimParams {
    /// Builds `GpuSimParams` from extracted simulation configuration values.
    ///
    /// Converts f64 source values to f32 and pre-computes derived values used by
    /// the compute shader (squared distances, reciprocals) to avoid per-thread division.
    #[allow(clippy::too_many_arguments)]
    pub fn from_config(
        max_dist: f64,
        min_rel_dist: f64,
        density_limit: f64,
        density_same_color: f64,
        density_diff_color: f64,
        attenuation_enabled: bool,
        particle_count: u32,
        grid_side: u32,
        color_count: u32,
    ) -> Self {
        let max_dist_f32 = max_dist as f32;
        let min_rel_dist_f32 = min_rel_dist as f32;

        Self {
            max_dist: max_dist_f32,
            min_rel_dist: min_rel_dist_f32,
            max_dist_sqrd: max_dist_f32 * max_dist_f32,
            max_dist_recip: 1.0 / max_dist_f32,
            min_dist_recip: 1.0 / min_rel_dist_f32,
            inv_min_dist_recip: 1.0 / (1.0 - min_rel_dist_f32),
            density_limit: density_limit as f32,
            density_same_color: density_same_color as f32,
            density_diff_color: density_diff_color as f32,
            attenuation_enabled: if attenuation_enabled { 1 } else { 0 },
            particle_count,
            grid_side,
            color_count,
            _padding: 0,
        }
    }
}
