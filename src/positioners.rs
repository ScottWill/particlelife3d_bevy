use bevy::math::{DVec2, DVec3};
use rand::{Rng as _, rngs::ThreadRng};
use std::f64::consts::TAU;

use crate::traits::RandVec3 as _;

pub fn get_position(rng: &mut ThreadRng, pos_type: PositionerType) -> DVec3 {
    match pos_type {
        PositionerType::BigBang => BigBangPositioner::get_pos(rng),
        PositionerType::Sphere => SpherePositioner::get_pos(rng),
        PositionerType::Rod => RodPositioner::get_pos(rng),
        PositionerType::Cylinder => CylinderPositioner::get_pos(rng),
        // PositionerType::STorus => STorusPositioner::get_pos(rng),
        // PositionerType::MTorus => MTorusPositioner::get_pos(rng),
        // PositionerType::LTorus => LTorusPositioner::get_pos(rng),
        // PositionerType::Spiral => SpiralPositioner::get_pos(rng),
        PositionerType::Uniform => UniformPositioner::get_pos(rng),
        PositionerType::UniformSphere => UniformSpherePositioner::get_pos(rng),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum PositionerType {
    BigBang,
    Sphere,
    #[default]
    Uniform, // Random
    UniformSphere,
    Rod,
    Cylinder,
    // STorus,
    // MTorus,
    // LTorus,
    // Spiral,
}

// impl Debug for PositionerType {
//     fn fmt(&self, f: &mut Formatter) -> Result {
//         write!(f, "{}", match &self {
//             PositionerType::BigBang => "Big Bang",
//             PositionerType::Sphere => "Sphere",
//             PositionerType::Rod => "Rod",
//             PositionerType::Cylinder => "Cylinder",
//             // PositionerType::STorus => "S Torus",
//             // PositionerType::MTorus => "M Torus",
//             // PositionerType::LTorus => "L Torus",
//             // PositionerType::Spiral => "Spiral",
//             PositionerType::Uniform => "Uniform",
//             PositionerType::UniformSphere => "U Sphere",
//         })
//     }
// }

pub trait Positioner {
    fn get_pos(rng: &mut ThreadRng) -> DVec3;
}

pub struct BigBangPositioner;
impl Positioner for BigBangPositioner {
    fn get_pos(rng: &mut ThreadRng) -> DVec3 {
        0.5 + 0.00015 * rng.random::<f64>() * rng.random_vec3()
    }
}

pub struct SpherePositioner;
impl Positioner for SpherePositioner {
    fn get_pos(rng: &mut ThreadRng) -> DVec3 {
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
    fn get_pos(rng: &mut ThreadRng) -> DVec3 {
        DVec3 {
            x: rng.random::<f64>(),
            y: 0.125 * rng.random::<f64>() + 0.43875,
            z: 0.125 * rng.random::<f64>() + 0.43875,
        }
    }
}

pub struct CylinderPositioner;
impl Positioner for CylinderPositioner {
    fn get_pos(rng: &mut ThreadRng) -> DVec3 {
        let circle = 0.5 + 0.125 * rng.random::<f64>().sqrt() * DVec2::from_angle(TAU * rng.random::<f64>());
        DVec3 {
            x: rng.random::<f64>(),
            y: circle.x,
            z: circle.y,
        }
    }
}

// pub struct STorusPositioner;
// impl Positioner for STorusPositioner {
//     fn get_pos(rng: &mut ThreadRng) -> DVec3 {
//         let mut rng = rng();
//         let radius = rng.random::<f64>() * 0.1 + 0.125;
//         let theta = rng.random::<f64>() * TAU;
//         DVec3 {
//             x: theta.cos() * radius + 0.5,
//             y: theta.sin() * radius + 0.5
//         }
//     }
// }

// pub struct MTorusPositioner;
// impl Positioner for MTorusPositioner {
//     fn get_pos(rng: &mut ThreadRng) -> DVec3 {
//         let mut rng = rng();
//         let radius = rng.random::<f64>() * 0.1 + 0.25;
//         let theta = rng.random::<f64>() * TAU;
//         DVec3 {
//             x: theta.cos() * radius + 0.5,
//             y: theta.sin() * radius + 0.5
//         }
//     }
// }

// pub struct LTorusPositioner;
// impl Positioner for LTorusPositioner {
//     fn get_pos(rng: &mut ThreadRng) -> DVec3 {
//         let mut rng = rng();
//         let radius = rng.random::<f64>() * 0.1 + 1.0 / 3.0;
//         let theta = rng.random::<f64>() * TAU;
//         DVec3 {
//             x: theta.cos() * radius + 0.5,
//             y: theta.sin() * radius + 0.5
//         }
//     }
// }

// pub struct SpiralPositioner;
// impl Positioner for SpiralPositioner {
//     fn get_pos(rng: &mut ThreadRng) -> DVec3 {
//         // let mut rng = Random::default();
//         let mut rng = rng();
//         let max_rotations = 2.0;
//         let f = rng.random::<f64>();
//         let angle = max_rotations * TAU * f;
//         let spread = 0.5 * f.min(0.2);
//         let radius = (0.9 * f + spread * spread * rng.random::<f64>()) * 0.5;
//         DVec3 {
//             x: radius * angle.cos() + 0.5,
//             y: radius * angle.sin() + 0.5,
//         }
//     }
// }

pub struct UniformPositioner;
impl Positioner for UniformPositioner {
    fn get_pos(rng: &mut ThreadRng) -> DVec3 {
        DVec3 {
            x: rng.random::<f64>(),
            y: rng.random::<f64>(),
            z: rng.random::<f64>(),
        }
    }
}

pub struct UniformSpherePositioner;
impl Positioner for UniformSpherePositioner {
    fn get_pos(rng: &mut ThreadRng) -> DVec3 {
        0.5 + 0.5 * rng.random::<f64>().cbrt() * rng.random_vec3()
    }
}
