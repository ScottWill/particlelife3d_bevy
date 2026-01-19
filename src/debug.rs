use bevy::color::palettes::css::{BLUE, GREEN, RED};
use bevy::dev_tools::fps_overlay::FpsOverlayConfig;
use bevy::input::common_conditions::input_just_pressed;
use bevy::platform::collections::HashMap;
use bevy::{dev_tools::fps_overlay::FpsOverlayPlugin, prelude::*};
use bevy::time::common_conditions::on_timer;
use std::collections::VecDeque;
use std::fmt::{Formatter, Result};
use std::{fmt::Display, time::Duration};

use crate::{SCALE, next_state};
use crate::physics::forces::ForceMatrix;
use crate::traits::{FpsOverlay as _, NextVariant};

#[derive(Component)]
struct DebugText;

pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FpsOverlayPlugin::overlay());
        app.init_resource::<DebugDurations>();
        app.init_state::<UiState>();
        app.add_systems(Startup, setup_ui);
        app.add_systems(Update, (
            next_state::<UiState>.run_if(input_just_pressed(KeyCode::Escape)),
            toggle_ui.run_if(state_changed::<UiState>).after(next_state::<UiState>),
        ));
        app.add_systems(PostUpdate,
            debug_ui.run_if(
                in_state(UiState::Visible).and(on_timer(Duration::from_millis(100)))
            )
        );
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, States)]
enum UiState {
    Hidden,
    #[default]
    Visible,
}

impl NextVariant for UiState {
    fn next(&self) -> Self {
        match self {
            UiState::Hidden => UiState::Visible,
            UiState::Visible => UiState::Hidden,
        }
    }
}

fn setup_ui(
    mut commands: Commands,
    mut gizmos: ResMut<Assets<GizmoAsset>>,
) {
    commands.spawn((
        Node {
            margin: UiRect::axes(px(1), px(47)),
            ..default()
        },
        children![(
            DebugText,
            BackgroundColor(Color::linear_rgba(0.3, 0.3, 0.3, 0.3)),
            Text::new("---- Debug Info ----\n"),
            TextColor(Color::linear_rgb(0.0, 1.0, 0.0)),
            TextFont::from_font_size(12.0),
            children![
                (
                    TextSpan::new("   --- Forces ---\n"),
                    TextColor(Color::linear_rgb(0.0, 1.0, 0.0)),
                    TextFont::from_font_size(12.0),
                ),
                (
                    TextSpan::default(),
                    TextColor(Color::linear_rgb(0.0, 1.0, 0.0)),
                    TextFont::from_font_size(12.0),
                ),
                (
                    TextSpan::default(),
                    TextColor(Color::linear_rgb(0.0, 1.0, 0.0)),
                    TextFont::from_font_size(12.0),
                ),
            ]
        )],
    ));

    let mut gizmo = GizmoAsset::default();
    gizmo.cube(Transform::IDENTITY.with_scale(Vec3::splat(SCALE as f32)), GREEN);
    gizmo.arrow(Vec3::NEG_X * 0.5, Vec3::X * 0.5, RED);
    gizmo.arrow(Vec3::NEG_Y * 0.5, Vec3::Y * 0.5, GREEN);
    gizmo.arrow(Vec3::NEG_Z * 0.5, Vec3::Z * 0.5, BLUE);

    commands.spawn(Gizmo { handle: gizmos.add(gizmo), ..default() });
}

fn toggle_ui(
    mut commands: Commands,
    mut overlay: ResMut<FpsOverlayConfig>,
    text: Single<Entity, With<DebugText>>,
    state: Res<State<UiState>>,
) {
    overlay.enabled = match state.get() {
        UiState::Hidden => false,
        UiState::Visible => true,
    };
    overlay.frame_time_graph_config.enabled = overlay.enabled;

    let visibility = match overlay.enabled {
        true => Visibility::Visible,
        false => Visibility::Hidden,
    };
    commands.entity(*text).insert(visibility);
}

fn debug_ui(
    mut writer: TextUiWriter,
    debug_info: Res<DebugDurations>,
    forces: Res<ForceMatrix>,
    ui_text: Single<Entity, With<DebugText>>,
) {
    *writer.text(*ui_text, 2) = forces.to_string();
    *writer.text(*ui_text, 3) = debug_info.to_string();
}

#[derive(Default, Deref, DerefMut, Resource)]
pub struct DebugDurations(HashMap<String,VecDeque<f32>>);

impl Display for DebugDurations {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let result = self
            .iter()
            .map(|(k, v)| {
                let avg = avg_duration(v);
                format!("{k}: {avg:.3}ms")
            })
            .collect::<Vec<_>>()
            .join("\n");
        write!(f, "{result}")
    }
}

impl DebugDurations {
    const MAX_ITEMS: usize = 64;

    pub fn add(&mut self, name: &str, duration: Duration) {
        let ms = 1000.0 * duration.as_secs_f32();
        if let Some(vdq) = self.get_mut(name) {
            vdq.truncate(Self::MAX_ITEMS - 1);
            vdq.push_front(ms);
        } else {
            let mut value = VecDeque::with_capacity(Self::MAX_ITEMS + 1);
            value.push_front(ms);
            self.insert(name.to_owned(), value);
        }
    }
}

#[inline]
fn avg_duration(vdq: &VecDeque<f32>) -> f32 {
    if vdq.len() == 0 { return 0.0 }
    if vdq.len() == 1 { return vdq[0] }

    let total = vdq.iter().sum::<f32>();
    total / vdq.len() as f32
}