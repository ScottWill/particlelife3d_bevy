//! Render graph compute pass node for GPU force computation.
//!
//! In Bevy 0.19, the render graph is a schedule (`RenderGraph`) rather than a
//! node-based graph. This module provides the [`force_compute_system`] which runs
//! as a system in the `RenderGraph` schedule under [`RenderGraphSystems::Render`],
//! dispatching the force compute shader and copying results to the staging buffer.

use bevy::prelude::*;
use bevy::render::render_resource::{
    BindGroupEntry, ComputePassDescriptor, PipelineCache,
};
use bevy::render::renderer::RenderContext;

use super::buffers::GpuForceBuffers;
use super::pipeline::GpuForcePipeline;

/// System that dispatches the GPU force compute shader and copies results to
/// the active staging buffer for async CPU readback.
///
/// Runs in the `RenderGraph` schedule under `RenderGraphSystems::Render`.
/// Gracefully skips execution if any required resource is missing or the
/// pipeline is not yet compiled.
pub fn force_compute_system(
    mut render_context: RenderContext,
    buffers: Option<Res<GpuForceBuffers>>,
    pipeline_resource: Option<Res<GpuForcePipeline>>,
    pipeline_cache: Res<PipelineCache>,
) {
    let Some(buffers) = buffers else { return };
    let Some(pipeline_resource) = pipeline_resource else { return };

    let Some(pipeline) = pipeline_cache.get_compute_pipeline(pipeline_resource.pipeline_id) else {
        // Pipeline not ready yet (still compiling)
        return;
    };

    // Create bind group from current buffers
    let bind_group = render_context.render_device().create_bind_group(
        "gpu_force_bind_group",
        &pipeline_resource.bind_group_layout,
        &[
            BindGroupEntry {
                binding: 0,
                resource: buffers.particle_buffer.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: buffers.cell_offset_buffer.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 2,
                resource: buffers.params_buffer.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 3,
                resource: buffers.force_matrix_buffer.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 4,
                resource: buffers.prev_density_buffer.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 5,
                resource: buffers.result_buffer.as_entire_binding(),
            },
        ],
    );

    let encoder = render_context.command_encoder();

    // Begin compute pass and dispatch workgroups
    {
        let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label: Some("gpu_force_compute_pass"),
            ..default()
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        let workgroup_count = buffers.capacity.div_ceil(256);
        pass.dispatch_workgroups(workgroup_count, 1, 1);
    }

    // Copy result buffer to the active staging buffer for async readback
    let result_size = buffers.capacity as u64 * 16; // 16 bytes per particle (vec3 force + f32 density)
    encoder.copy_buffer_to_buffer(
        &buffers.result_buffer,
        0,
        &buffers.staging_buffers[buffers.active_staging],
        0,
        result_size,
    );
}
