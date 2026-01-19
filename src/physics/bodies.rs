use bevy::prelude::*;
use glam::DVec3;

#[derive(Clone, Copy, Component, Debug, Default)]
#[require(PointBodyIndex)]
pub struct PointBody {
    pub color: usize,
    pub position: DVec3,
    pub velocity: DVec3,
}

#[derive(Clone, Copy, Component, Default, Deref, DerefMut)]
pub struct PointBodyIndex(usize);

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
