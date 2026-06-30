use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, WindowMode};
use bevy::math::DVec3;
use rand::RngExt as _;
use std::f64::consts::TAU;

pub trait Fullscreen {
    fn fullscreen() -> Self;
}

impl Fullscreen for WindowPlugin {
    fn fullscreen() -> Self {
        WindowPlugin {
            primary_window: Some(Window {
                mode: WindowMode::BorderlessFullscreen(MonitorSelection::Primary),
                ..default()
            }),
            primary_cursor_options: Some(CursorOptions {
                visible: false,
                grab_mode: CursorGrabMode::Locked,
                ..default()
            }),
            ..default()
        }
    }
}

pub trait RandVec3 {
    fn random_vec3(&mut self) -> DVec3;
}

impl<R: rand::Rng> RandVec3 for R {
    fn random_vec3(&mut self) -> DVec3 {
        let a = (2.0 * self.random::<f64>() - 1.0).acos();
        let b = self.random_range(0.0..TAU);
        let (a_sin, a_cos) = a.sin_cos();
        let (b_sin, b_cos) = b.sin_cos();
        let x = a_sin * b_cos;
        let y = a_sin * b_sin;
        let z = a_cos;
        DVec3::new(x, y, z)
    }
}

pub trait NextVariant {
    fn next(&self) -> Self;
}

pub trait PrevVariant {
    fn prev(&self) -> Self;
}