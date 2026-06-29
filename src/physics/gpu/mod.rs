//! GPU-accelerated pairwise force computation plugin.
//!
//! This module provides a compute shader pipeline that evaluates pairwise forces
//! on the GPU, replacing the CPU Rayon path for improved throughput at high
//! particle counts.

mod buffers;
mod detect;
mod extract;
mod node;
mod pipeline;
pub mod readback;
mod sort;

use bevy::prelude::*;
use bevy::render::render_resource::{CachedPipelineState, MapMode, PipelineCache, PollType};
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::{ExtractSchedule, GpuResourceAppExt, Render, RenderApp, RenderStartup, RenderSystems};
use bevy::shader::Shader;

use buffers::{GpuForceBuffers, GpuResult, GpuSimParams};
use detect::{
    create_availability_channel, GpuAvailabilitySender, GpuErrorTracker,
};
use extract::{ExtractedAttenuation, ExtractedConfig, ExtractedForceMatrix, ExtractedSnapshots};
use pipeline::{ForceComputeShader, GpuForcePipeline};
use readback::{create_readback_channel, GpuComputePayload, GpuReadbackSender};

pub use detect::{check_gpu_availability, GpuUnavailableReason};
pub use readback::{poll_gpu_readback, GpuComputeResults};

#[cfg(test)]
mod tests;

/// Render-world resource for tracking previous-tick per-particle density values.
#[derive(Resource, Default)]
pub struct GpuPrevDensities(pub Vec<f32>);

/// Render-world resource for tracking the original_indices mapping from the last sort.
#[derive(Resource, Default)]
pub struct GpuOriginalIndices(pub Vec<u32>);

/// Plugin that registers GPU force computation systems, resources, and render graph nodes.
pub struct GpuForcePlugin;

impl Plugin for GpuForcePlugin {
    fn build(&self, app: &mut App) {
        // Load the compute shader in the main world (which has Assets<Shader>)
        let shader_handle = {
            let mut shaders = app.world_mut().resource_mut::<Assets<Shader>>();
            shaders.add(Shader::from_wgsl(
                include_str!("shader.wgsl"),
                "gpu_force_compute_shader",
            ))
        };

        // Create the crossbeam channel for GPU readback communication
        let (sender, receiver) = create_readback_channel();

        // Create the channel for GPU availability/error signaling
        let (avail_sender, avail_receiver) = create_availability_channel();

        // Insert main-world resources
        app.insert_resource(receiver);
        app.insert_resource(avail_receiver);
        app.init_resource::<readback::GpuComputeResults>();

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        // Insert render-world resources
        render_app.insert_resource(sender);
        render_app.insert_resource(avail_sender);
        render_app.init_resource::<GpuErrorTracker>();
        render_app.insert_resource(ForceComputeShader(shader_handle));

        // Register GPU availability detection as a startup system in the render world
        render_app.add_systems(RenderStartup, detect::detect_gpu_compute_support);

        // Register extraction system in the ExtractSchedule
        render_app.add_systems(ExtractSchedule, extract::extract_particle_data);

        // Initialize render-world resources
        render_app.init_gpu_resource::<GpuForcePipeline>();
        render_app.init_resource::<GpuPrevDensities>();
        render_app.init_resource::<GpuOriginalIndices>();

        // Register prepare system (sort + upload) in render world's Prepare set
        render_app.add_systems(
            Render,
            prepare_gpu_forces.in_set(RenderSystems::PrepareResources),
        );

        // Register compute dispatch + pipeline monitoring in the Render set
        render_app.add_systems(
            Render,
            (check_pipeline_state, node::force_compute_system)
                .chain()
                .in_set(RenderSystems::Render),
        );

        // Register readback system after compute dispatch (in Cleanup set)
        render_app.add_systems(
            Render,
            submit_readback.in_set(RenderSystems::Cleanup),
        );
    }
}

/// Render-world system that monitors the compute pipeline compilation state.
///
/// If the pipeline enters an error state (shader compilation failure), records the
/// error in `GpuErrorTracker` and sends a disable signal to the main world.
/// Only checks once — after the error is recorded, subsequent calls are no-ops.
fn check_pipeline_state(
    pipeline_resource: Option<Res<GpuForcePipeline>>,
    pipeline_cache: Res<PipelineCache>,
    mut error_tracker: ResMut<GpuErrorTracker>,
    avail_sender: Res<GpuAvailabilitySender>,
) {
    // If already permanently disabled, nothing to check
    if error_tracker.permanently_disabled {
        return;
    }

    let Some(pipeline_resource) = pipeline_resource else {
        return;
    };

    let state = pipeline_cache.get_compute_pipeline_state(pipeline_resource.pipeline_id);
    if let CachedPipelineState::Err(err) = state {
        let reason = error_tracker.record_pipeline_error(format!("{err}"));
        avail_sender.tx.try_send(reason).ok();
    }
}

/// Prepare system that runs the counting sort on extracted snapshots and uploads
/// sorted data, cell_offsets, params, force_matrix, and prev_densities to the GPU.
///
/// Handles buffer allocation/reallocation when particle count or grid dimensions change.
/// Skips processing if the GPU has been permanently disabled.
#[allow(clippy::too_many_arguments)]
fn prepare_gpu_forces(
    mut commands: Commands,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    extracted_snapshots: Option<Res<ExtractedSnapshots>>,
    extracted_config: Option<Res<ExtractedConfig>>,
    extracted_matrix: Option<Res<ExtractedForceMatrix>>,
    extracted_attenuation: Option<Res<ExtractedAttenuation>>,
    mut buffers: Option<ResMut<GpuForceBuffers>>,
    prev_densities: Res<GpuPrevDensities>,
    mut original_indices: ResMut<GpuOriginalIndices>,
    error_tracker: Res<GpuErrorTracker>,
) {
    // Skip GPU work if permanently disabled
    if error_tracker.permanently_disabled {
        return;
    }

    let Some(snapshots) = extracted_snapshots else {
        return;
    };
    let Some(config) = extracted_config else {
        return;
    };
    let Some(matrix) = extracted_matrix else {
        return;
    };
    let Some(attenuation) = extracted_attenuation else {
        return;
    };

    if snapshots.0.is_empty() {
        return;
    }

    let particle_count = snapshots.0.len().min(500_000) as u32;
    let grid_side = sort::compute_grid_side(config.max_dist) as u32;

    // Create or reallocate buffers
    match buffers.as_mut() {
        Some(bufs) => {
            bufs.reallocate_if_needed(&device, particle_count, grid_side);
        }
        None => {
            commands.insert_resource(GpuForceBuffers::new(&device, particle_count, grid_side));
            return; // Buffers will be available next frame
        }
    }
    let buffers = buffers.unwrap();

    // Sort particles by cell
    let (sorted_data, cell_offsets, orig_indices) =
        sort::sort_particles_by_cell(&snapshots.0[..particle_count as usize], grid_side as usize);

    // Remap prev densities to sorted order
    let sorted_prev_densities: Vec<f32> = (0..particle_count as usize)
        .map(|sorted_idx| {
            let original_idx = orig_indices[sorted_idx] as usize;
            prev_densities.0.get(original_idx).copied().unwrap_or(0.0)
        })
        .collect();

    // Build params
    let params = GpuSimParams::from_config(
        config.max_dist,
        config.min_rel_dist,
        config.density_limit,
        config.density_same_color,
        config.density_diff_color,
        attenuation.0,
        particle_count,
        grid_side,
        matrix.color_count as u32,
    );

    // Upload everything to the GPU
    buffers.upload_particles(&queue, &sorted_data);
    buffers.upload_cell_offsets(&queue, &cell_offsets);
    buffers.upload_params(&queue, &params);
    buffers.upload_force_matrix(&queue, &matrix.data, matrix.color_count);
    buffers.upload_prev_densities(&queue, &sorted_prev_densities);

    // Store original_indices for readback remapping
    original_indices.0 = orig_indices;
}

/// Render-world system that performs async readback of GPU results from the staging buffer.
///
/// Uses double-buffered staging: reads from `staging_buffers[1 - active_staging]`
/// (the buffer written to in the previous tick's compute dispatch), while the current
/// tick's compute writes to `staging_buffers[active_staging]`.
///
/// Issues `map_async` on the staging buffer and polls the device to wait for completion.
/// Since this reads from the PREVIOUS tick's staging buffer, the GPU work should already
/// be complete by the time we poll, making the wait near-instant.
///
/// On success: sends results to the main world via crossbeam channel, updates
/// prev_densities for the next tick's compute, and resets the failure counter.
/// On failure: increments the consecutive failure counter. After 3 consecutive failures,
/// sends a permanent disable signal to the main world.
fn submit_readback(
    mut buffers: Option<ResMut<GpuForceBuffers>>,
    device: Res<RenderDevice>,
    original_indices: Res<GpuOriginalIndices>,
    sender: Res<GpuReadbackSender>,
    mut prev_densities: ResMut<GpuPrevDensities>,
    mut error_tracker: ResMut<GpuErrorTracker>,
    avail_sender: Res<GpuAvailabilitySender>,
) {
    // Skip if GPU is permanently disabled
    if error_tracker.permanently_disabled {
        return;
    }

    let Some(ref mut buffers) = buffers else {
        return;
    };

    if buffers.capacity == 0 || original_indices.0.is_empty() {
        return;
    }

    // Read from the staging buffer written in the PREVIOUS tick
    let read_staging_idx = 1 - buffers.active_staging;
    let staging = &buffers.staging_buffers[read_staging_idx];

    // Issue map_async with a oneshot channel to signal completion
    let (map_tx, map_rx) = crossbeam_channel::bounded(1);
    let slice = staging.slice(..);
    slice.map_async(MapMode::Read, move |result| {
        map_tx.send(result).ok();
    });

    // Poll the device to drive the map operation to completion.
    // The buffer was written in the previous frame, so this should complete near-instantly.
    let poll_result = device.poll(PollType::wait_indefinitely());
    if poll_result.is_err() {
        // Device poll failed (e.g., device lost) — signal permanent disable
        let reason = error_tracker.record_device_lost();
        avail_sender.tx.try_send(reason).ok();
        buffers.active_staging = 1 - buffers.active_staging;
        return;
    }

    // Check if mapping completed successfully
    match map_rx.try_recv() {
        Ok(Ok(())) => {
            let data = slice.get_mapped_range();
            let gpu_results: &[GpuResult] = bytemuck::cast_slice(&data);
            let results = gpu_results.to_vec();

            // Update prev_densities for next tick (in original ordering)
            let particle_count = original_indices.0.len();
            prev_densities.0.resize(particle_count, 0.0);
            for (sorted_idx, result) in results.iter().enumerate() {
                if sorted_idx < original_indices.0.len() {
                    let original_idx = original_indices.0[sorted_idx] as usize;
                    if original_idx < particle_count {
                        prev_densities.0[original_idx] = result.density;
                    }
                }
            }

            // Send payload to main world
            let payload = GpuComputePayload {
                results,
                original_indices: original_indices.0.clone(),
            };
            sender.tx.try_send(payload).ok();

            drop(data);
            staging.unmap();

            // Record success — resets consecutive failure counter
            error_tracker.record_success();
        }
        Ok(Err(_map_err)) => {
            // map_async returned an error (e.g., device lost during mapping)
            // Record as device lost since map errors typically indicate this
            let reason = error_tracker.record_device_lost();
            avail_sender.tx.try_send(reason).ok();
        }
        Err(_) => {
            // Channel empty — mapping didn't complete (timeout-like scenario)
            // Record as a readback failure; after threshold, permanently disable
            if let Some(reason) = error_tracker.record_failure() {
                avail_sender.tx.try_send(reason).ok();
            }
        }
    }

    // Flip active_staging for next tick's double-buffer swap
    buffers.active_staging = 1 - buffers.active_staging;
}
