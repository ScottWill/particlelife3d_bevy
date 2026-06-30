use bevy::input::common_conditions::input_just_pressed;
use bevy::math::{DVec2, DVec3};
use bevy::prelude::*;
use rand::RngExt as _;
use std::f64::consts::TAU;
use std::fmt::{Display, Formatter, Result as FmtResult};

use crate::traits::{NextVariant, PrevVariant, RandVec3 as _};

pub struct PositionerPlugin;

impl Plugin for PositionerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CurrentPositioner>();
        app.add_systems(Update, (
            next_positioner.run_if(input_just_pressed(KeyCode::Period)),
            prev_positioner.run_if(input_just_pressed(KeyCode::Comma)),
        ));
    }
}

#[derive(Default, Resource)]
pub struct CurrentPositioner(pub PositionerType);

impl Display for CurrentPositioner {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{:?}", self.0)
    }
}

fn next_positioner(mut positioner: ResMut<CurrentPositioner>) {
    positioner.0 = positioner.0.next();
}

fn prev_positioner(mut positioner: ResMut<CurrentPositioner>) {
    positioner.0 = positioner.0.prev();
}

pub fn get_position(rng: &mut impl rand::Rng, pos_type: PositionerType) -> DVec3 {
    match pos_type {
        PositionerType::BigBang => BigBangPositioner::get_pos(rng),
        PositionerType::Sphere => SpherePositioner::get_pos(rng),
        PositionerType::Rod => RodPositioner::get_pos(rng),
        PositionerType::Cylinder => CylinderPositioner::get_pos(rng),
        PositionerType::STorus => STorusPositioner::get_pos(rng),
        PositionerType::MTorus => MTorusPositioner::get_pos(rng),
        PositionerType::LTorus => LTorusPositioner::get_pos(rng),
        PositionerType::Spiral => SpiralPositioner::get_pos(rng),
        PositionerType::Uniform => UniformPositioner::get_pos(rng),
        PositionerType::UniformSphere => UniformSpherePositioner::get_pos(rng),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum PositionerType {
    BigBang,
    Sphere,
    #[default]
    Uniform,
    UniformSphere,
    Rod,
    Cylinder,
    STorus,
    MTorus,
    LTorus,
    Spiral,
}

impl NextVariant for PositionerType {
    fn next(&self) -> Self {
        match self {
            PositionerType::BigBang       => PositionerType::Sphere,
            PositionerType::Sphere        => PositionerType::Uniform,
            PositionerType::Uniform       => PositionerType::UniformSphere,
            PositionerType::UniformSphere => PositionerType::Rod,
            PositionerType::Rod           => PositionerType::Cylinder,
            PositionerType::Cylinder      => PositionerType::STorus,
            PositionerType::STorus        => PositionerType::MTorus,
            PositionerType::MTorus        => PositionerType::LTorus,
            PositionerType::LTorus        => PositionerType::Spiral,
            PositionerType::Spiral        => PositionerType::BigBang,
        }
    }
}

impl PrevVariant for PositionerType {
    fn prev(&self) -> Self {
        match self {
            PositionerType::BigBang       => PositionerType::Spiral,
            PositionerType::Sphere        => PositionerType::BigBang,
            PositionerType::Uniform       => PositionerType::Sphere,
            PositionerType::UniformSphere => PositionerType::Uniform,
            PositionerType::Rod           => PositionerType::UniformSphere,
            PositionerType::Cylinder      => PositionerType::Rod,
            PositionerType::STorus        => PositionerType::Cylinder,
            PositionerType::MTorus        => PositionerType::STorus,
            PositionerType::LTorus        => PositionerType::MTorus,
            PositionerType::Spiral        => PositionerType::LTorus,
        }
    }
}

// impl Debug for PositionerType {
//     fn fmt(&self, f: &mut Formatter) -> Result {
//         write!(f, "{}", match &self {
//             PositionerType::BigBang => "Big Bang",
//             PositionerType::Sphere => "Sphere",
//             PositionerType::Rod => "Rod",
//             PositionerType::Cylinder => "Cylinder",
//             PositionerType::STorus => "S Torus",
//             PositionerType::MTorus => "M Torus",
//             PositionerType::LTorus => "L Torus",
//             PositionerType::Spiral => "Spiral",
//             PositionerType::Uniform => "Uniform",
//             PositionerType::UniformSphere => "U Sphere",
//         })
//     }
// }

pub trait Positioner {
    fn get_pos(rng: &mut impl rand::Rng) -> DVec3;
}

pub struct BigBangPositioner;
impl Positioner for BigBangPositioner {
    fn get_pos(rng: &mut impl rand::Rng) -> DVec3 {
        0.5 + 0.00015 * rng.random::<f64>() * rng.random_vec3()
    }
}

pub struct SpherePositioner;
impl Positioner for SpherePositioner {
    fn get_pos(rng: &mut impl rand::Rng) -> DVec3 {
        0.5 + 0.5 * rng.random::<f64>() * rng.random_vec3()
    }
}

// pub struct ExactPositioner(DVec3);
// impl Positioner for ExactPositioner {
//     fn get_pos(&self) -> DVec3 {
//         self.0
//     }
// }

// 2d plane
// let yz = DVec2::splat(0.5 + 0.125 * rng.random::<f64>() - 0.06125);
// yz.extend(rng.random::<f64>()).zxy()

pub struct RodPositioner;
impl Positioner for RodPositioner {
    fn get_pos(rng: &mut impl rand::Rng) -> DVec3 {
        DVec3 {
            x: rng.random::<f64>(),
            y: 0.125 * rng.random::<f64>() + 0.43875,
            z: 0.125 * rng.random::<f64>() + 0.43875,
        }
    }
}

pub struct CylinderPositioner;
impl Positioner for CylinderPositioner {
    fn get_pos(rng: &mut impl rand::Rng) -> DVec3 {
        let circle = 0.5 + 0.125 * rng.random::<f64>().sqrt() * DVec2::from_angle(TAU * rng.random::<f64>());
        DVec3 {
            x: rng.random::<f64>(),
            y: circle.x,
            z: circle.y,
        }
    }
}

pub struct STorusPositioner;
impl Positioner for STorusPositioner {
    fn get_pos(rng: &mut impl rand::Rng) -> DVec3 {
        let major_radius = 0.125;
        let tube_radius = 0.05;
        let theta = rng.random::<f64>() * TAU;
        let phi = rng.random::<f64>() * TAU;
        let r = tube_radius * rng.random::<f64>().sqrt();
        let x = (major_radius + r * phi.cos()) * theta.cos() + 0.5;
        let y = (major_radius + r * phi.cos()) * theta.sin() + 0.5;
        let z = r * phi.sin() + 0.5;
        DVec3 { x, y, z }
    }
}

pub struct MTorusPositioner;
impl Positioner for MTorusPositioner {
    fn get_pos(rng: &mut impl rand::Rng) -> DVec3 {
        let major_radius = 0.25;
        let tube_radius = 0.075;
        let theta = rng.random::<f64>() * TAU;
        let phi = rng.random::<f64>() * TAU;
        let r = tube_radius * rng.random::<f64>().sqrt();
        let x = (major_radius + r * phi.cos()) * theta.cos() + 0.5;
        let y = (major_radius + r * phi.cos()) * theta.sin() + 0.5;
        let z = r * phi.sin() + 0.5;
        DVec3 { x, y, z }
    }
}

pub struct LTorusPositioner;
impl Positioner for LTorusPositioner {
    fn get_pos(rng: &mut impl rand::Rng) -> DVec3 {
        let major_radius = 1.0 / 3.0;
        let tube_radius = 0.1;
        let theta = rng.random::<f64>() * TAU;
        let phi = rng.random::<f64>() * TAU;
        let r = tube_radius * rng.random::<f64>().sqrt();
        let x = (major_radius + r * phi.cos()) * theta.cos() + 0.5;
        let y = (major_radius + r * phi.cos()) * theta.sin() + 0.5;
        let z = r * phi.sin() + 0.5;
        DVec3 { x, y, z }
    }
}

pub struct SpiralPositioner;
impl Positioner for SpiralPositioner {
    fn get_pos(rng: &mut impl rand::Rng) -> DVec3 {
        let max_rotations = 2.0;
        let f = rng.random::<f64>();
        let angle = max_rotations * TAU * f;
        let spread = 0.5 * f.min(0.2);
        let radius = (0.9 * f + spread * spread * rng.random::<f64>()) * 0.5;
        let x = radius * angle.cos() + 0.5;
        let y = radius * angle.sin() + 0.5;
        let z = f * 0.8 + 0.1; // spread along z with progression
        DVec3 { x, y, z }
    }
}

pub struct UniformPositioner;
impl Positioner for UniformPositioner {
    fn get_pos(rng: &mut impl rand::Rng) -> DVec3 {
        DVec3 {
            x: rng.random::<f64>(),
            y: rng.random::<f64>(),
            z: rng.random::<f64>(),
        }
    }
}

pub struct UniformSpherePositioner;
impl Positioner for UniformSpherePositioner {
    fn get_pos(rng: &mut impl rand::Rng) -> DVec3 {
        0.5 + 0.5 * rng.random::<f64>().cbrt() * rng.random_vec3()
    }
}
