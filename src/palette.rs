use bevy::input::common_conditions::input_just_pressed;
use bevy::prelude::*;
use rand::random;
use crate::config::COLORS;
use crate::physics::forces::ForceMatrix;
use crate::physics::{PointBody, PointColor};

pub struct PalettePlugin;

impl Plugin for PalettePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, create_resource);
        app.add_systems(Update, (
            reset_palette::<1>.run_if(input_just_pressed(KeyCode::Digit1)),
            reset_palette::<2>.run_if(input_just_pressed(KeyCode::Digit2)),
            reset_palette::<3>.run_if(input_just_pressed(KeyCode::Digit3)),
            reset_palette::<4>.run_if(input_just_pressed(KeyCode::Digit4)),
            reset_palette::<5>.run_if(input_just_pressed(KeyCode::Digit5)),
            reset_palette::<6>.run_if(input_just_pressed(KeyCode::Digit6)),
            reset_palette::<7>.run_if(input_just_pressed(KeyCode::Digit7)),
            reset_palette::<8>.run_if(input_just_pressed(KeyCode::Digit8)),
            reset_palette::<9>.run_if(input_just_pressed(KeyCode::Digit9)),
        ));
    }
}

fn create_resource(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(Palette::new(&mut materials, COLORS));
}

fn reset_palette<
    const K: u8,
>(
    mut forces: ResMut<ForceMatrix>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut palette: ResMut<Palette>,
    mut query: Query<(&mut MeshMaterial3d<StandardMaterial>, &mut PointColor), With<PointBody>>,
) {
    let count = K as usize;
    if palette.size == count {
        return;
    }

    // Rebuild palette with new color count
    *palette = Palette::new(&mut materials, count);

    // Rebuild force matrix with new color count, keeping current matrix type
    let matrix_type = forces.matrix_type;
    *forces = ForceMatrix::new(count, matrix_type);

    // Randomly recolor all point bodies
    for (mut mat_handle, mut point_color) in query.iter_mut() {
        let color = palette.random();
        point_color.0 = color;
        **mat_handle = palette[color].clone();
    }
}

#[derive(Deref, Resource)]
pub struct Palette {
    #[deref]
    data: Vec<Handle<StandardMaterial>>,
    size: usize,
}

impl Palette {
    pub fn new(
        materials: &mut Assets<StandardMaterial>,
        size: usize,
    ) -> Self {
        Self {
            data: (0..size)
                .map(|i| {
                    let hue = (i as f32 / size as f32) * 360.0;
                    let emissive = Color::hsl(hue, 1.0, 0.5).into();
                    materials.add(StandardMaterial { emissive, ..default() })
                })
                .collect(),
            size,
        }
    }

    pub fn random(&self) -> usize {
        random::<u64>() as usize % self.size
    }
}

// pub fn update_palette(
//     mut materials: ResMut<Assets<StandardMaterial>>,
//     mut palette: ResMut<Palette>,
//     mut query: Query<(&mut MeshMaterial3d<StandardMaterial>, &mut PointBody)>,
// ) {
//     if COLORS != palette.size {
//         // re-init palette
//         *palette = Palette::new(&mut materials, COLORS);
//         // reassign colors
//         for (mut hndl, mut body) in query.iter_mut() {
//             match body.color.cmp(&COLORS) {
//                 std::cmp::Ordering::Less => {
//                     **hndl = palette.get(body.color).clone();
//                 },
//                 _ => {
//                     let color = palette.random();
//                     body.color = color;
//                     **hndl = palette.get(color).clone();
//                 },
//             }
//         }
//     }
// }