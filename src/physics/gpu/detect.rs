//! GPU availability detection and error tracking.
//!
//! Provides:
//! - [`check_gpu_availability`]: Main-world system that receives GPU availability/error
//!   signals from the render world and disables GPU backend if needed.
//! - [`GpuErrorTracker`]: Render-world resource tracking consecutive readback failures
//!   and pipeline compilation errors.
//! - [`GpuAvailabilitySender`] / [`GpuAvailabilityReceiver`]: Channel pair for
//!   communicating GPU disable signals from render world to main world.
//! - [`detect_gpu_compute_support`]: Render-world startup system that checks device limits.

use bevy::prelude::*;
use bevy::render::renderer::RenderDevice;
use crossbeam_channel::{Receiver, Sender};

use crate::physics::backend::ForceBackend;

/// Signals sent from the render world to the main world when GPU errors occur.
#[derive(Debug, Clone)]
pub enum GpuDisableReason {
    /// Device limits are insufficient for GPU force compute.
    InsufficientDevice(String),
    /// Shader compilation failed.
    ShaderCompilationError(String),
    /// Device was lost (map_async or device poll failed).
    DeviceLost,
    /// Too many consecutive readback failures (threshold exceeded).
    ConsecutiveReadbackFailures(u32),
}

/// Render-world resource that tracks GPU errors and decides when to signal disable.
#[derive(Resource)]
pub struct GpuErrorTracker {
    /// Number of consecutive readback failures (map_async errors or poll failures).
    pub consecutive_failures: u32,
    /// Whether the GPU has been permanently disabled due to unrecoverable errors.
    pub permanently_disabled: bool,
    /// Whether a pipeline compilation error was detected.
    pub pipeline_error: bool,
    /// Threshold for consecutive failures before permanently disabling GPU.
    pub failure_threshold: u32,
}

impl Default for GpuErrorTracker {
    fn default() -> Self {
        Self {
            consecutive_failures: 0,
            permanently_disabled: false,
            pipeline_error: false,
            failure_threshold: 3,
        }
    }
}

impl GpuErrorTracker {
    /// Records a successful readback, resetting the failure counter.
    pub fn record_success(&mut self) {
        self.consecutive_failures = 0;
    }

    /// Records a readback failure. Returns `Some(reason)` if the threshold is exceeded
    /// and GPU should be permanently disabled.
    pub fn record_failure(&mut self) -> Option<GpuDisableReason> {
        self.consecutive_failures += 1;
        if self.consecutive_failures >= self.failure_threshold {
            self.permanently_disabled = true;
            Some(GpuDisableReason::ConsecutiveReadbackFailures(
                self.consecutive_failures,
            ))
        } else {
            None
        }
    }

    /// Records a device loss event. Always triggers permanent disable.
    pub fn record_device_lost(&mut self) -> GpuDisableReason {
        self.permanently_disabled = true;
        GpuDisableReason::DeviceLost
    }

    /// Records a pipeline compilation error. Always triggers permanent disable.
    pub fn record_pipeline_error(&mut self, error_msg: String) -> GpuDisableReason {
        self.permanently_disabled = true;
        self.pipeline_error = true;
        GpuDisableReason::ShaderCompilationError(error_msg)
    }
}

/// Render-world resource: sender for GPU disable signals to the main world.
#[derive(Resource)]
pub struct GpuAvailabilitySender {
    pub tx: Sender<GpuDisableReason>,
}

/// Main-world resource: receiver for GPU disable signals from the render world.
#[derive(Resource)]
pub struct GpuAvailabilityReceiver {
    pub rx: Receiver<GpuDisableReason>,
}

/// Creates the channel pair for GPU availability signaling.
pub fn create_availability_channel() -> (GpuAvailabilitySender, GpuAvailabilityReceiver) {
    let (tx, rx) = crossbeam_channel::bounded(4);
    (GpuAvailabilitySender { tx }, GpuAvailabilityReceiver { rx })
}

/// Render-world startup system that checks device limits for GPU force compute support.
///
/// Verifies that `max_storage_buffer_binding_size` is at least 8 MB (enough for 500k particles).
/// If insufficient, sends a disable signal to the main world.
pub fn detect_gpu_compute_support(
    device: Res<RenderDevice>,
    sender: Res<GpuAvailabilitySender>,
) {
    let limits = device.limits();
    let min_required: u64 = 8_000_000; // 500k particles × 16 bytes

    if limits.max_storage_buffer_binding_size < min_required {
        let reason = GpuDisableReason::InsufficientDevice(format!(
            "max_storage_buffer_binding_size ({}) < required ({})",
            limits.max_storage_buffer_binding_size, min_required
        ));
        sender.tx.try_send(reason).ok();
    }
}

/// Main-world resource that stores the reason GPU backend was disabled.
///
/// Inserted by [`check_gpu_availability`] when a disable signal is received.
/// The settings panel uses this to display why GPU is unavailable.
#[derive(Resource, Clone)]
pub struct GpuUnavailableReason(pub String);

/// Main-world system that checks for GPU disable signals from the render world.
///
/// When a disable signal is received, switches `ForceBackend` to `Cpu` permanently
/// and logs the reason. This system runs before `poll_gpu_readback` to ensure
/// the backend switch takes effect before any GPU results are consumed.
pub fn check_gpu_availability(
    mut backend: ResMut<ForceBackend>,
    mut commands: Commands,
    receiver: Option<Res<GpuAvailabilityReceiver>>,
) {
    let Some(receiver) = receiver else { return };

    // Drain all pending disable signals
    while let Ok(reason) = receiver.rx.try_recv() {
        if *backend == ForceBackend::Gpu {
            *backend = ForceBackend::Cpu;
            let reason_text = match &reason {
                GpuDisableReason::InsufficientDevice(msg) => {
                    warn!("GPU force compute unavailable: {msg}");
                    format!("GPU unavailable: {msg}")
                }
                GpuDisableReason::ShaderCompilationError(msg) => {
                    error!("GPU force compute disabled: shader compilation failed — {msg}");
                    "GPU unavailable: shader error".to_string()
                }
                GpuDisableReason::DeviceLost => {
                    error!("GPU force compute disabled: GPU device lost");
                    "GPU unavailable: device lost".to_string()
                }
                GpuDisableReason::ConsecutiveReadbackFailures(count) => {
                    warn!(
                        "GPU force compute disabled: {count} consecutive readback failures, \
                         switching to CPU permanently"
                    );
                    "GPU unavailable: readback failures".to_string()
                }
            };
            commands.insert_resource(GpuUnavailableReason(reason_text));
        }
    }
}
