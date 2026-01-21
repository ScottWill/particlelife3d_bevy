use bevy::prelude::*;
use rand::random;
use crate::config::COLORS;
// use crate::physics::PointBody;

pub struct PalettePlugin;

impl Plugin for PalettePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, create_resource);
    }
}

fn create_resource(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(Palette::new(&mut materials, COLORS));
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