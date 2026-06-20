use bevy::{math::DVec3, prelude::*};

use crate::camera::Position;

#[derive(Clone, Copy, Component, Default, Deref, DerefMut)]
pub struct PointBodyIndex(usize);

#[derive(Clone, Copy, Component, Debug, Default, Deref, DerefMut)]
pub struct PointColor(pub usize);

#[derive(Clone, Copy, Component, Debug, Default, Deref, DerefMut)]
pub struct PointPosition(pub DVec3);

impl Position for PointPosition {
    fn position(&self) -> &DVec3 {
        &self.0
    }

    fn position_mut(&mut self) -> &mut DVec3 {
        &mut self.0
    }
}

#[derive(Clone, Copy, Component, Debug, Default, Deref, DerefMut)]
pub struct PointVelocity(pub DVec3);

/// Marker component that requires all point body sub-components.
#[derive(Clone, Copy, Component, Debug, Default)]
#[require(PointBodyIndex, PointColor, PointPosition, PointVelocity)]
pub struct PointBody;

/// A snapshot of a single body's data, used for physics computation.
#[derive(Clone, Copy, Debug)]
pub struct BodySnapshot {
    pub color: usize,
    pub position: DVec3,
}
