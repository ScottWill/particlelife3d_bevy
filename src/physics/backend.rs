use std::fmt::Display;

use bevy::prelude::*;

use crate::traits::NextVariant;

/// Selects which backend performs pairwise force computation.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Default)]
pub enum ForceBackend {
    Gpu,
    #[default]
    Cpu,
}

impl Display for ForceBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", match self {
            ForceBackend::Gpu => "GPU",
            ForceBackend::Cpu => "CPU",
        })
    }
}

impl NextVariant for ForceBackend {
    fn next(&self) -> Self {
        match self {
            ForceBackend::Gpu => ForceBackend::Cpu,
            ForceBackend::Cpu => ForceBackend::Gpu,
        }
    }
}
