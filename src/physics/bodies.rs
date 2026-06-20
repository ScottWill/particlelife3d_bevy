use bevy::{math::DVec3, prelude::*};

use crate::camera::Position;

#[derive(Clone, Copy, Component, Default, Deref, DerefMut)]
pub struct PointBodyIndex(usize);

#[derive(Clone, Copy, Component, Debug, Default)]
#[require(PointBodyIndex)]
pub struct PointBody {
    pub color: usize,
    pub position: DVec3,
    pub velocity: DVec3,
}

impl Position for PointBody {
    fn position(&self) -> &DVec3 {
        &self.position
    }

    fn position_mut(&mut self) -> &mut DVec3 {
        &mut self.position
    }
}

impl PointBody {

    const DRAG_HALFLIFE: f64 = 1.0 / 0.043;

    pub fn new(color: usize, position: DVec3) -> Self {
        Self { color, position, ..default() }
    }

    #[inline]
    pub fn step(&mut self, force: DVec3, dt: f64) {
        // degrade velocity before adding force
        self.velocity *= 0.5f64.powf(Self::DRAG_HALFLIFE * dt);
        self.velocity += force * dt;

        self.position += self.velocity * dt;
    }

}
