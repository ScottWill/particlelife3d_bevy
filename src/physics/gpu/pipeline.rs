//! Compute pipeline setup and bind group layouts.
//!
//! Defines [`GpuForcePipeline`], a render-world resource that holds the cached compute
//! pipeline ID and the bind group layout matching `shader.wgsl`.

use bevy::prelude::*;
use bevy::render::render_resource::{
    BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType,
    BufferBindingType, CachedComputePipelineId, ComputePipelineDescriptor, PipelineCache,
    ShaderStages,
};
use bevy::shader::Shader;

/// Render-world resource holding the shader handle, loaded in the main world
/// and passed to the render world during plugin setup.
#[derive(Resource)]
pub struct ForceComputeShader(pub Handle<Shader>);

/// Bind group layout entries for the GPU force compute pipeline.
///
/// Matches the bindings declared in `shader.wgsl`:
/// - binding 0: particles (storage, read-only)
/// - binding 1: cell_offsets (storage, read-only)
/// - binding 2: params (uniform)
/// - binding 3: force_matrix (storage, read-only)
/// - binding 4: prev_densities (storage, read-only)
/// - binding 5: results (storage, read-write)
const BIND_GROUP_LAYOUT_ENTRIES: &[BindGroupLayoutEntry] = &[
    // @binding(0): particles — storage buffer (read-only)
    BindGroupLayoutEntry {
        binding: 0,
        visibility: ShaderStages::COMPUTE,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    },
    // @binding(1): cell_offsets — storage buffer (read-only)
    BindGroupLayoutEntry {
        binding: 1,
        visibility: ShaderStages::COMPUTE,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    },
    // @binding(2): params — uniform buffer
    BindGroupLayoutEntry {
        binding: 2,
        visibility: ShaderStages::COMPUTE,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    },
    // @binding(3): force_matrix — storage buffer (read-only)
    BindGroupLayoutEntry {
        binding: 3,
        visibility: ShaderStages::COMPUTE,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    },
    // @binding(4): prev_densities — storage buffer (read-only)
    BindGroupLayoutEntry {
        binding: 4,
        visibility: ShaderStages::COMPUTE,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    },
    // @binding(5): results — storage buffer (read-write)
    BindGroupLayoutEntry {
        binding: 5,
        visibility: ShaderStages::COMPUTE,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Storage { read_only: false },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    },
];

/// Creates the bind group layout descriptor for the force compute pipeline.
/// Used both for pipeline creation and for obtaining the `BindGroupLayout` from the cache.
pub fn bind_group_layout_descriptor() -> BindGroupLayoutDescriptor {
    BindGroupLayoutDescriptor {
        label: "gpu_force_bind_group_layout".into(),
        entries: BIND_GROUP_LAYOUT_ENTRIES.to_vec(),
    }
}

/// Render-world resource holding the compute pipeline ID and bind group layout
/// for the GPU force computation shader.
#[derive(Resource)]
pub struct GpuForcePipeline {
    /// Cached pipeline ID for the force compute shader.
    pub pipeline_id: CachedComputePipelineId,
    /// Bind group layout with 6 bindings matching `shader.wgsl`.
    pub bind_group_layout: BindGroupLayout,
}

impl FromWorld for GpuForcePipeline {
    fn from_world(world: &mut World) -> Self {
        let layout_desc = bind_group_layout_descriptor();

        let pipeline_cache = world.resource::<PipelineCache>();

        // Get the bind group layout from the pipeline cache (handles deduplication).
        let bind_group_layout = pipeline_cache.get_bind_group_layout(&layout_desc);

        // Get the shader handle that was loaded in the main world and passed here.
        let shader = world.resource::<ForceComputeShader>().0.clone();

        let pipeline_cache = world.resource::<PipelineCache>();
        let pipeline_id = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: Some("gpu_force_compute_pipeline".into()),
            layout: vec![layout_desc],
            immediate_size: 0,
            shader,
            shader_defs: vec![],
            entry_point: Some("main".into()),
            zero_initialize_workgroup_memory: true,
        });

        Self {
            pipeline_id,
            bind_group_layout,
        }
    }
}
