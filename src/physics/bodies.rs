use bevy::{math::DVec3, prelude::*};
use rand::distr::weighted::WeightedIndex;
use rand::prelude::*;
use rand::random_range;

use crate::{camera::Position, palette::Palette, settings_panel::SimulationConfig};

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
#[require(PointBodyIndex, PointPosition, PointVelocity)]
pub struct PointBody;

/// A snapshot of a single body's data, used for physics computation.
#[derive(Clone, Copy, Debug)]
pub struct BodySnapshot {
    pub color: usize,
    pub position: DVec3,
}

pub struct BodyPlugin;

impl Plugin for BodyPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(add_components);
    }
}

fn add_components(
    trigger: On<Add, PointBody>,
    mut commands: Commands,
    palette: Res<Palette>,
    config: Res<SimulationConfig>,
) {
    let color = match WeightedIndex::new(&config.color_weights) {
        Ok(dist) => {
            let mut rng = rand::rng();
            dist.sample(&mut rng)
        }
        Err(_) => {
            // Fallback to uniform if weights are invalid
            random_range(0..palette.size())
        }
    };
    commands.entity(trigger.entity).insert((
        MeshMaterial3d(palette[color].clone()),
        PointColor(color),
    ));
}
