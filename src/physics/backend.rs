use bevy::prelude::*;

use crate::traits::NextVariant;

/// Selects which backend performs pairwise force computation.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Default)]
pub enum ForceBackend {
    #[default]
    Gpu,
    Cpu,
}

impl NextVariant for ForceBackend {
    fn next(&self) -> Self {
        match self {
            ForceBackend::Gpu => ForceBackend::Cpu,
            ForceBackend::Cpu => ForceBackend::Gpu,
        }
    }
}
