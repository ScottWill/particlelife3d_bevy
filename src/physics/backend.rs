use bevy::prelude::*;

/// Selects which backend performs pairwise force computation.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Default)]
pub enum ForceBackend {
    #[default]
    Gpu,
    Cpu,
}
