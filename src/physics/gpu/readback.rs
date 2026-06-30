//! Async GPU readback system using crossbeam-channel.
//!
//! This module implements the bridge between the render world (which has access to
//! GPU buffers) and the main world (which needs the computed forces/densities).
//!
//! Architecture:
//! - The render world issues `map_async` on the staging buffer after compute dispatch,
//!   polls the device, and sends the results through a crossbeam channel.
//! - The main world polls `try_recv` each tick before `compute_forces` runs. On success,
//!   it remaps results using `original_indices` into the `ParticleComputations` resource
//!   and stores densities for the next tick's `prev_densities`.
//! - On failure/timeout, computations are left empty, triggering the CPU fallback path.

use bevy::math::DVec3;
use bevy::prelude::*;
use crossbeam_channel::{Receiver, Sender};

use super::buffers::GpuResult;

/// Payload sent from the render world to the main world after a successful GPU readback.
pub struct GpuComputePayload {
    /// Raw GPU results in sorted (by-cell) order.
    pub results: Vec<GpuResult>,
    /// The original_indices mapping: `original_indices[sorted_idx] = original_idx`.
    /// Used to remap sorted GPU results back to ECS entity order.
    pub original_indices: Vec<u32>,
}

/// Main-world resource holding the receiver end of the GPU readback channel.
/// Inserted during plugin build alongside a corresponding sender in the render world.
#[derive(Resource)]
pub struct GpuReadbackReceiver {
    pub rx: Receiver<GpuComputePayload>,
}

/// Render-world resource holding the sender end of the GPU readback channel.
/// The render world sends completed readback payloads through this channel.
#[derive(Resource)]
pub struct GpuReadbackSender {
    pub tx: Sender<GpuComputePayload>,
}

/// Main-world resource indicating whether GPU results are available this tick.
/// When `ready` is true, `forces` and `densities` contain valid data remapped
/// to original particle ordering.
#[derive(Resource, Default)]
pub struct GpuComputeResults {
    /// Per-particle force vectors in original (ECS) ordering.
    pub forces: Vec<DVec3>,
    /// Per-particle density values in original (ECS) ordering.
    pub densities: Vec<f32>,
    /// Whether GPU results were successfully received this tick.
    pub ready: bool,
}

/// Main-world system that checks for completed GPU readback results.
///
/// Runs before `compute_forces` in `PhysicsSet`. On success, remaps GPU results
/// from sorted order to original particle order using `original_indices`.
/// On failure (no data available), marks results as not ready so the CPU fallback runs.
pub fn poll_gpu_readback(
    receiver: Option<Res<GpuReadbackReceiver>>,
    mut gpu_results: ResMut<GpuComputeResults>,
) {
    let Some(receiver) = receiver else {
        gpu_results.ready = false;
        return;
    };

    // Drain the channel, keeping only the most recent payload
    // (in case multiple frames completed since last poll)
    let mut latest_payload: Option<GpuComputePayload> = None;
    while let Ok(payload) = receiver.rx.try_recv() {
        latest_payload = Some(payload);
    }

    let Some(payload) = latest_payload else {
        gpu_results.ready = false;
        return;
    };

    let particle_count = payload.original_indices.len();

    // Resize output vectors if needed
    gpu_results.forces.resize(particle_count, DVec3::ZERO);
    gpu_results.densities.resize(particle_count, 0.0);

    for (sorted_idx, gpu_result) in payload.results.iter().enumerate() {
        if sorted_idx >= particle_count {
            break;
        }
        let original_idx = payload.original_indices[sorted_idx] as usize;
        if original_idx < particle_count {
            gpu_results.forces[original_idx] = DVec3::new(
                gpu_result.force_x as f64,
                gpu_result.force_y as f64,
                gpu_result.force_z as f64,
            );
            gpu_results.densities[original_idx] = gpu_result.density;
        }
    }

    gpu_results.ready = true;
}

/// Creates a crossbeam channel pair for GPU readback communication.
/// Returns (sender for render world, receiver for main world).
pub fn create_readback_channel() -> (GpuReadbackSender, GpuReadbackReceiver) {
    // Bounded channel with capacity 2: allows one in-flight and one ready
    let (tx, rx) = crossbeam_channel::bounded(2);
    (GpuReadbackSender { tx }, GpuReadbackReceiver { rx })
}
