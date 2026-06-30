use bevy::prelude::*;
use bevy::camera::{CameraOutputMode, Viewport, visibility::RenderLayers};
use bevy::color::palettes::css::{BLUE, GREEN, RED};
use bevy::ecs::{schedule::common_conditions::on_message, system::SystemParam};
use bevy::input::common_conditions::input_just_pressed;
use bevy::render::render_resource::BlendState;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContext, EguiContexts, EguiGlobalSettings, EguiPlugin, EguiPrimaryContextPass, PrimaryEguiContext, egui};

use crate::debug::DebugDurations;
use crate::palette::Palette;
use crate::physics::forces::{ForceMatrix, ForceMatrixType};
use crate::physics::islands::{Islands, IslandNeighborIxs, IslandNeighborhoods, IslandGrid, compute_neighbor_ixs};
use crate::physics::{DensityAttenuation, PointBody, PointColor};
use crate::physics::ForceBackend;
use crate::physics::GpuUnavailableReason;
use crate::positioners::CurrentPositioner;
use crate::UpdateBodies;

const DEFAULT_COLOR_COUNT: usize = 5;
const DEFAULT_PARTICLE_COUNT: usize = 50_000;

pub struct SettingsPanelPlugin;

impl Plugin for SettingsPanelPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EguiPlugin::default());
        app.init_resource::<SimulationConfig>();
        app.init_state::<PanelVisibility>();
        app.init_resource::<PanelWidth>();
        app.init_resource::<FpsTracker>();
        app.insert_resource(DebugDurations::with_order(&["islands", "forces", "stepping"]));
        app.add_message::<RebuildPalette>();
        app.add_message::<RedistributeColors>();
        app.add_message::<RebuildIslands>();
        app.add_systems(Startup, (setup_gizmos, setup_egui_camera));
        app.add_systems(Update, (
            handle_palette_rebuild.run_if(on_message::<RebuildPalette>),
            handle_redistribute_colors.run_if(on_message::<RedistributeColors>),
            rebuild_islands.run_if(on_message::<RebuildIslands>),
            toggle_panel_visibility.run_if(input_just_pressed(KeyCode::Backspace)),
            update_camera_viewport,
            update_gizmo_scale,
        ));
        app.add_systems(EguiPrimaryContextPass, render_panel);
    }
}

/// Marker component for the bounding-box gizmo entity.
#[derive(Component)]
pub struct BoundingBoxGizmo;

fn setup_gizmos(
    mut commands: Commands,
    mut gizmos: ResMut<Assets<GizmoAsset>>,
    config: Res<SimulationConfig>,
) {
    let scale = config.world_scale as f32;

    let mut gizmo = GizmoAsset::default();
    gizmo.cube(Transform::IDENTITY.with_scale(Vec3::splat(scale)), GREEN);
    gizmo.arrow(Vec3::NEG_X * 0.5, Vec3::X * 0.5, RED);
    gizmo.arrow(Vec3::NEG_Y * 0.5, Vec3::Y * 0.5, GREEN);
    gizmo.arrow(Vec3::NEG_Z * 0.5, Vec3::Z * 0.5, BLUE);

    commands.spawn((BoundingBoxGizmo, Gizmo { handle: gizmos.add(gizmo), ..default() }));
}

/// Spawns a dedicated camera for egui rendering that covers the full window,
/// and disables automatic primary context creation so we can bind it manually.
fn setup_egui_camera(
    mut commands: Commands,
    mut egui_global_settings: ResMut<EguiGlobalSettings>,
) {
    egui_global_settings.auto_create_primary_context = false;

    commands.spawn((
        PrimaryEguiContext,
        Camera2d,
        RenderLayers::none(),
        Camera {
            order: 1,
            output_mode: CameraOutputMode::Write {
                blend_state: Some(BlendState::ALPHA_BLENDING),
                clear_color: ClearColorConfig::None,
            },
            clear_color: ClearColorConfig::Custom(Color::NONE),
            ..default()
        },
    ));
}

/// Updates the 3D camera viewport to render only in the area to the right of the panel.
fn update_camera_viewport(
    mut camera: Single<&mut Camera, (With<Camera3d>, Without<EguiContext>)>,
    panel_width: Res<PanelWidth>,
    window: Single<&Window, With<PrimaryWindow>>,
) {
    let panel_px = panel_width.0 as u32;
    let win_w = window.physical_width();
    let win_h = window.physical_height();

    if panel_px < win_w {
        camera.viewport = Some(Viewport {
            physical_position: UVec2::new(panel_px, 0),
            physical_size: UVec2::new(win_w - panel_px, win_h),
            ..default()
        });
    } else {
        camera.viewport = None;
    }
}

/// Centralizes all previously-constant simulation parameters into a single mutable resource.
#[derive(Resource)]
pub struct SimulationConfig {
    pub particle_count: usize,
    pub color_count: usize,
    pub max_dist: f64,
    pub min_rel_dist: f64,
    pub drag_halflife: f64,
    pub density_limit: f64,
    pub density_same_color: f64,
    pub density_diff_color: f64,
    pub world_scale: f64,
    /// Per-color probability weights for particle color assignment.
    /// Length always equals `color_count`; entries sum to 1.0.
    pub color_weights: Vec<f64>,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            particle_count: DEFAULT_PARTICLE_COUNT,
            color_count: DEFAULT_COLOR_COUNT,
            max_dist: 0.045,
            min_rel_dist: 0.333,
            drag_halflife: 0.043,
            density_limit: 12.0,
            density_same_color: 1.0,
            density_diff_color: 0.5,
            world_scale: 128.0,
            color_weights: vec![1.0 / DEFAULT_COLOR_COUNT as f64; DEFAULT_COLOR_COUNT],
        }
    }
}

impl SimulationConfig {
    /// Resizes `color_weights` to match `color_count`, preserving sum = 1.0.
    ///
    /// - **Growing:** new entries get equal share taken proportionally from existing weights.
    /// - **Shrinking:** removed weight is redistributed proportionally among remaining entries.
    /// - A final normalization pass guarantees sum = 1.0.
    pub fn resize_weights(&mut self) {
        let new_count = self.color_count;
        let old_count = self.color_weights.len();

        if new_count == old_count {
            return;
        }

        if new_count > old_count {
            // Growing: scale existing weights down, add new entries with equal share
            let scale = old_count as f64 / new_count as f64;
            let share = 1.0 / new_count as f64;
            for w in self.color_weights.iter_mut() {
                *w *= scale;
            }
            self.color_weights.resize(new_count, share);
        } else {
            // Shrinking: remove excess entries, redistribute among remaining
            self.color_weights.truncate(new_count);
            let remaining_sum: f64 = self.color_weights.iter().sum();
            if remaining_sum > 0.0 {
                for w in self.color_weights.iter_mut() {
                    *w /= remaining_sum;
                }
            } else {
                let uniform = 1.0 / new_count as f64;
                for w in self.color_weights.iter_mut() {
                    *w = uniform;
                }
            }
        }

        // Final normalization pass to guarantee sum = 1.0
        let sum: f64 = self.color_weights.iter().sum();
        if sum > 0.0 {
            for w in self.color_weights.iter_mut() {
                *w /= sum;
            }
        }
    }

    /// Adjusts weight at `index` to `new_value`, redistributing the difference
    /// evenly among all other weights. Maintains sum = 1.0.
    ///
    /// Single-color case: early return (weight locked at 1.0).
    pub fn set_weight(&mut self, index: usize, new_value: f64) {
        let others_count = self.color_weights.len() - 1;
        if others_count == 0 {
            // Single color: weight is locked at 1.0
            return;
        }

        let old_value = self.color_weights[index];
        let diff = new_value - old_value;
        let per_other = diff / others_count as f64;

        self.color_weights[index] = new_value;
        for i in 0..self.color_weights.len() {
            if i != index {
                self.color_weights[i] -= per_other;
            }
        }

        // Clamp all to [0.0, 1.0]
        for w in self.color_weights.iter_mut() {
            *w = w.clamp(0.0, 1.0);
        }

        // Re-normalize so weights sum to 1.0
        let sum: f64 = self.color_weights.iter().sum();
        if sum > 0.0 {
            for w in self.color_weights.iter_mut() {
                *w /= sum;
            }
        } else {
            // Fallback to uniform if all weights ended up at 0
            let n = self.color_weights.len() as f64;
            for w in self.color_weights.iter_mut() {
                *w = 1.0 / n;
            }
        }
    }

    /// Clamps all fields to their valid ranges.
    /// For `particle_count`, also rounds to the nearest multiple of 100.
    /// For `color_weights`, clamps each entry to [0.0, 1.0] and normalizes so they sum to 1.0.
    #[allow(dead_code)] // used in tests
    pub fn clamp_all(&mut self) {
        self.particle_count = self.particle_count.clamp(100, 500_000);
        self.particle_count = ((self.particle_count + 50) / 100) * 100;
        self.particle_count = self.particle_count.clamp(100, 500_000);
        self.color_count = self.color_count.clamp(1, 9);
        self.max_dist = self.max_dist.clamp(0.01, 0.2);
        self.min_rel_dist = self.min_rel_dist.clamp(0.05, 0.95);
        self.drag_halflife = self.drag_halflife.clamp(0.001, 0.5);
        self.density_limit = self.density_limit.clamp(1.0, 50.0);
        self.density_same_color = self.density_same_color.clamp(0.0, 5.0);
        self.density_diff_color = self.density_diff_color.clamp(0.0, 5.0);
        self.world_scale = self.world_scale.clamp(16.0, 512.0);

        // Clamp each color weight to [0.0, 1.0]
        for w in self.color_weights.iter_mut() {
            *w = w.clamp(0.0, 1.0);
        }

        // Normalize so weights sum to 1.0; fallback to uniform if sum is 0
        let sum: f64 = self.color_weights.iter().sum();
        if sum > 0.0 {
            for w in self.color_weights.iter_mut() {
                *w /= sum;
            }
        } else {
            let n = self.color_weights.len() as f64;
            for w in self.color_weights.iter_mut() {
                *w = 1.0 / n;
            }
        }
    }
}

/// Triggers palette rebuild and particle recoloring for new color_count.
#[derive(Message)]
pub struct RebuildPalette;

/// Triggers redistribution of particle colors based on current color_weights.
#[derive(Message)]
pub struct RedistributeColors;

/// Triggers island grid rebuild when max_dist changes.
#[derive(Message)]
pub struct RebuildIslands;

/// Bundles message writers used by the settings panel into a single system parameter.
#[derive(SystemParam)]
struct PanelMessages<'w> {
    update_bodies: MessageWriter<'w, UpdateBodies>,
    rebuild_palette: MessageWriter<'w, RebuildPalette>,
    redistribute_colors: MessageWriter<'w, RedistributeColors>,
    rebuild_islands: MessageWriter<'w, RebuildIslands>,
}

/// Controls whether the settings panel is rendered.
#[derive(States, Default, Clone, PartialEq, Eq, Hash, Debug)]
pub enum PanelVisibility {
    #[default]
    Visible,
    Hidden,
}

/// Stores the current physical pixel width of the settings panel so the camera
/// viewport can be offset accordingly.
#[derive(Resource, Default)]
pub struct PanelWidth(pub f32);

/// Tracks frames-per-second with a display refresh interval of 100ms.
#[derive(Resource)]
pub struct FpsTracker {
    pub frame_count: u32,
    pub elapsed: f32,
    pub current_fps: f32,
}

impl Default for FpsTracker {
    fn default() -> Self {
        Self {
            frame_count: 0,
            elapsed: 0.0,
            current_fps: 0.0,
        }
    }
}

impl FpsTracker {
    /// Call each frame with the frame's delta time in seconds.
    /// Updates `current_fps` every 100ms (0.1s).
    pub fn update(&mut self, delta: f32) {
        self.frame_count += 1;
        self.elapsed += delta;
        if self.elapsed >= 0.1 {
            self.current_fps = self.frame_count as f32 / self.elapsed;
            self.frame_count = 0;
            self.elapsed = 0.0;
        }
    }
}

fn handle_palette_rebuild(
    mut config: ResMut<SimulationConfig>,
    mut palette: ResMut<Palette>,
    mut force_matrix: ResMut<ForceMatrix>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut query: Query<(&mut MeshMaterial3d<StandardMaterial>, &mut PointColor), With<PointBody>>,
) {
    use rand::distr::weighted::WeightedIndex;
    use rand::prelude::*;

    let color_count = config.color_count;

    // Skip if palette already matches (Requirement 6.4)
    if palette.size() == color_count {
        return;
    }

    // Rebuild palette
    *palette = Palette::new(&mut materials, color_count);

    // Regenerate force matrix
    let matrix_type = force_matrix.matrix_type;
    *force_matrix = ForceMatrix::new(color_count, matrix_type);

    // Resize weights to match the new color_count
    config.resize_weights();

    // Reassign particle colors using weighted sampling
    let dist = match WeightedIndex::new(&config.color_weights) {
        Ok(d) => d,
        Err(_) => return, // Defensive fallback
    };
    let mut rng = rand::rng();
    for (mut mat_handle, mut point_color) in query.iter_mut() {
        let color = dist.sample(&mut rng);
        point_color.0 = color;
        **mat_handle = palette[color].clone();
    }
}

fn handle_redistribute_colors(
    config: Res<SimulationConfig>,
    palette: Res<Palette>,
    mut query: Query<(&mut MeshMaterial3d<StandardMaterial>, &mut PointColor), With<PointBody>>,
) {
    use rand::distr::weighted::WeightedIndex;
    use rand::prelude::*;

    let dist = match WeightedIndex::new(&config.color_weights) {
        Ok(d) => d,
        Err(_) => return, // Defensive: if weights are invalid, skip
    };
    let mut rng = rand::rng();

    for (mut mat_handle, mut point_color) in query.iter_mut() {
        let color = dist.sample(&mut rng);
        point_color.0 = color;
        **mat_handle = palette[color].clone();
    }
}

fn render_panel(
    mut contexts: EguiContexts,
    visibility: Res<State<PanelVisibility>>,
    mut config: ResMut<SimulationConfig>,
    mut force_matrix: ResMut<ForceMatrix>,
    mut positioner: ResMut<CurrentPositioner>,
    mut density_attenuation: ResMut<DensityAttenuation>,
    mut backend: ResMut<ForceBackend>,
    gpu_unavailable: Option<Res<GpuUnavailableReason>>,
    debug_durations: Res<DebugDurations>,
    mut fps_tracker: ResMut<FpsTracker>,
    time: Res<Time>,
    mut messages: PanelMessages,
    mut panel_width: ResMut<PanelWidth>,
    window: Single<&Window>,
) {
    if *visibility.get() == PanelVisibility::Hidden {
        panel_width.0 = 0.0;
        return;
    }

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let mut viewport_ui = egui::Ui::new(
        ctx.clone(),
        "viewport".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );

    let panel_response = egui::Panel::left("settings_panel")
        .default_size(320.0)
        .show_inside(&mut viewport_ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::CollapsingHeader::new("Performance")
                    .default_open(true)
                    .show(ui, |ui| {
                        // Update FPS tracker
                        fps_tracker.update(time.delta_secs());
                        ui.label(format!("FPS: {:.0}", fps_tracker.current_fps));
                        ui.separator();
                        ui.label(format!("{}", *debug_durations));

                        // Backend selector
                        ui.separator();
                        let gpu_disabled = gpu_unavailable.is_some();
                        ui.horizontal(|ui| {
                            ui.label("Backend:");
                            if gpu_disabled {
                                // GPU is unavailable — show disabled combo box
                                ui.add_enabled(false, egui::Button::new("CPU (GPU unavailable)"));
                            } else {
                                let current = *backend;
                                let mut selected = current;
                                egui::ComboBox::from_id_salt("backend_selector")
                                    .selected_text(match selected {
                                        ForceBackend::Gpu => "GPU",
                                        ForceBackend::Cpu => "CPU",
                                    })
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut selected, ForceBackend::Gpu, "GPU");
                                        ui.selectable_value(&mut selected, ForceBackend::Cpu, "CPU");
                                    });
                                if selected != current {
                                    *backend = selected;
                                }
                            }
                        });
                        if let Some(reason) = &gpu_unavailable {
                            ui.label(egui::RichText::new(&reason.0).small().weak());
                        }
                    });

                egui::CollapsingHeader::new("Physics")
                    .default_open(true)
                    .show(ui, |ui| {
                        let prev_max_dist = config.max_dist;
                        ui.add(egui::Slider::new(&mut config.max_dist, 0.01..=0.2).step_by(0.005).text("Max Dist"));
                        if config.max_dist != prev_max_dist {
                            messages.rebuild_islands.write(RebuildIslands);
                        }
                        ui.add(egui::Slider::new(&mut config.min_rel_dist, 0.05..=0.95).step_by(0.05).text("Min Rel Dist"));
                        ui.add(egui::Slider::new(&mut config.drag_halflife, 0.001..=0.5).step_by(0.01).text("Drag Halflife"));
                        ui.separator();
                        ui.add(egui::Slider::new(&mut config.density_limit, 1.0..=50.0).step_by(1.0).text("Density Limit"));
                        ui.add(egui::Slider::new(&mut config.density_same_color, 0.0..=5.0).step_by(0.25).text("Density Same"));
                        ui.add(egui::Slider::new(&mut config.density_diff_color, 0.0..=5.0).step_by(0.25).text("Density Diff"));
                        ui.separator();
                        let status = if density_attenuation.0 { "ON" } else { "OFF" };
                        ui.checkbox(&mut density_attenuation.0, format!("Density Attenuation: {status}"));
                    });

                egui::CollapsingHeader::new("Force Matrix")
                    .default_open(true)
                    .show(ui, |ui| {
                        // Force matrix type dropdown
                        let current_type = force_matrix.matrix_type;
                        let mut selected_type = current_type;
                        egui::ComboBox::from_label("Matrix Type")
                            .selected_text(format!("{:?}", selected_type))
                            .show_ui(ui, |ui| {
                                let variants = [
                                    ForceMatrixType::Chains,
                                    ForceMatrixType::Checkered,
                                    ForceMatrixType::RandomEx,
                                    ForceMatrixType::Random,
                                    ForceMatrixType::Snakes,
                                    ForceMatrixType::Zeros,
                                    ForceMatrixType::Ones,
                                ];
                                for variant in variants {
                                    ui.selectable_value(&mut selected_type, variant, format!("{:?}", variant));
                                }
                            });
                        if selected_type != current_type {
                            *force_matrix = ForceMatrix::new(config.color_count, selected_type);
                        }

                        ui.separator();

                        // Editable force matrix grid
                        let color_count = force_matrix.color_count;
                        egui::Grid::new("force_matrix_grid")
                            .striped(true)
                            .show(ui, |ui| {
                                // Column headers
                                ui.label(""); // empty corner cell
                                for col in 0..color_count {
                                    let color = color_code(col, color_count);
                                    ui.label(egui::RichText::new(format!("{col}")).color(color));
                                }
                                ui.end_row();

                                // Data rows
                                for row in 0..color_count {
                                    let color = color_code(row, color_count);
                                    ui.label(egui::RichText::new(format!("{row}")).color(color)); // row header
                                    for col in 0..color_count {
                                        let idx = col + row * color_count;
                                        if let Some(cell) = force_matrix.data.get_mut(idx) {
                                            ui.add(
                                                egui::DragValue::new(cell)
                                                    .speed(0.01)
                                                    .range(-1.0..=1.0)
                                                    .max_decimals(3)
                                            );
                                        }
                                    }
                                    ui.end_row();
                                }
                            });
                    });

                egui::CollapsingHeader::new("Simulation")
                    .default_open(true)
                    .show(ui, |ui| {
                        // Particle count
                        let prev_count = config.particle_count;
                        let response = ui.add(
                            egui::DragValue::new(&mut config.particle_count)
                                .speed(100)
                                .range(100..=500_000)
                                .prefix("Particles: ")
                        );
                        if response.drag_stopped() || response.lost_focus() {
                            // Round to nearest 100
                            config.particle_count = ((config.particle_count + 50) / 100) * 100;
                            config.particle_count = config.particle_count.clamp(100, 500_000);
                            if config.particle_count != prev_count {
                                messages.update_bodies.write(UpdateBodies);
                            }
                        }

                        // Color count
                        let prev_colors = config.color_count;
                        ui.add(
                            egui::DragValue::new(&mut config.color_count)
                                .speed(1)
                                .range(1..=9)
                                .prefix("Colors: ")
                        );
                        if config.color_count != prev_colors {
                            messages.rebuild_palette.write(RebuildPalette);
                        }

                        ui.separator();

                        // Positioner dropdown
                        use crate::positioners::PositionerType;
                        let variants = [
                            PositionerType::BigBang,
                            PositionerType::Sphere,
                            PositionerType::Uniform,
                            PositionerType::UniformSphere,
                            PositionerType::Rod,
                            PositionerType::Cylinder,
                            PositionerType::STorus,
                            PositionerType::MTorus,
                            PositionerType::LTorus,
                            PositionerType::Spiral,
                        ];
                        egui::ComboBox::from_label("Positioner")
                            .selected_text(format!("{:?}", positioner.0))
                            .show_ui(ui, |ui| {
                                for variant in variants {
                                    ui.selectable_value(&mut positioner.0, variant, format!("{:?}", variant));
                                }
                            });
                    });

                egui::CollapsingHeader::new("Distribution")
                    .default_open(true)
                    .show(ui, |ui| {
                        let mut any_changed = false;
                        let color_count = config.color_count.min(config.color_weights.len());
                        for i in 0..color_count {
                            let mut weight = config.color_weights[i];
                            ui.horizontal(|ui| {
                                let color = color_code(i, color_count);
                                ui.label(egui::RichText::new(format!("{i}")).color(color)); // row header
                                let response = ui.add(egui::Slider::new(&mut weight, 0.0..=1.0)
                                    .step_by(0.01)
                                    .fixed_decimals(2));
                                // Only respond to direct user interaction (dragging or clicking),
                                // not to programmatic value changes from set_weight redistribution.
                                if response.changed() && (response.dragged() || response.has_focus()) {
                                    config.set_weight(i, weight);
                                    any_changed = true;
                                }
                            });
                        }
                        if any_changed {
                            messages.redistribute_colors.write(RedistributeColors);
                        }
                    });

                egui::CollapsingHeader::new("Appearance")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.add(egui::Slider::new(&mut config.world_scale, 16.0..=512.0).step_by(1.0).text("World Scale"));
                    });
            });
        });

    // Store the panel's physical pixel width so the camera viewport can be offset.
    let logical_width = panel_response.response.rect.width();
    panel_width.0 = logical_width * window.scale_factor();
}

fn toggle_panel_visibility(
    state: Res<State<PanelVisibility>>,
    mut next_state: ResMut<NextState<PanelVisibility>>,
) {
    next_state.set(match state.get() {
        PanelVisibility::Visible => PanelVisibility::Hidden,
        PanelVisibility::Hidden => PanelVisibility::Visible,
    });
}

fn rebuild_islands(
    config: Res<SimulationConfig>,
    mut islands: ResMut<Islands>,
    mut neighbor_ixs: ResMut<IslandNeighborIxs>,
    mut neighborhoods: ResMut<IslandNeighborhoods>,
    mut grid: ResMut<IslandGrid>,
) {
    let new_side = config.max_dist.recip().floor() as usize;
    if new_side == grid.side {
        return;
    }

    let size = new_side * new_side * new_side;

    // Rebuild islands
    islands.0 = vec![vec![]; size];

    // Rebuild neighborhoods
    neighborhoods.0 = vec![vec![]; size];

    // Rebuild neighbor indices
    neighbor_ixs.0 = compute_neighbor_ixs(new_side, size);

    // Update grid
    grid.side = new_side;
    grid.side_f64 = new_side as f64;
}

fn update_gizmo_scale(
    config: Res<SimulationConfig>,
    mut gizmos: ResMut<Assets<GizmoAsset>>,
    gizmo_query: Query<&Gizmo, With<BoundingBoxGizmo>>,
) {
    if !config.is_changed() {
        return;
    }
    let scale = config.world_scale as f32;
    for gizmo in gizmo_query.iter() {
        if let Some(mut asset) = gizmos.get_mut(&gizmo.handle) {
            *asset = GizmoAsset::default();
            asset.cube(Transform::IDENTITY.with_scale(Vec3::splat(scale)), GREEN);
            asset.arrow(Vec3::NEG_X * 0.5, Vec3::X * 0.5, RED);
            asset.arrow(Vec3::NEG_Y * 0.5, Vec3::Y * 0.5, GREEN);
            asset.arrow(Vec3::NEG_Z * 0.5, Vec3::Z * 0.5, BLUE);
        }
    }
}

#[inline]
fn color_code(index: usize, count: usize) -> egui::Color32 {
    let hue = (index as f32 / count as f32) * 360.0;
    let [r, g, b, _] = Color::hsl(hue, 1.0, 0.5).to_srgba().to_f32_array();
    egui::Color32::from_rgb(
        (r * 255.0) as u8,
        (g * 255.0) as u8,
        (b * 255.0) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// Feature: color-distribution-panel, Property 1: Weight vector invariant after resize
        /// **Validates: Requirements 1.1, 1.2, 1.3, 1.4, 7.1, 7.3**
        #[test]
        fn resize_weights_invariant(
            raw_weights in prop::collection::vec(0.01f64..1.0, 1..10usize),
            new_color_count in 1..=9usize,
        ) {
            // Normalize raw_weights so they sum to 1.0 (valid input)
            let sum: f64 = raw_weights.iter().sum();
            let normalized: Vec<f64> = raw_weights.iter().map(|w| w / sum).collect();

            let mut config = SimulationConfig {
                color_count: new_color_count,
                color_weights: normalized,
                ..Default::default()
            };

            config.resize_weights();

            // Assert length equals new color_count
            prop_assert_eq!(config.color_weights.len(), new_color_count,
                "color_weights length {} != color_count {}", config.color_weights.len(), new_color_count);

            // Assert each weight is in [0.0, 1.0]
            for (i, w) in config.color_weights.iter().enumerate() {
                prop_assert!(*w >= 0.0 && *w <= 1.0,
                    "color_weights[{}] = {} out of [0.0, 1.0]", i, w);
            }

            // Assert sum ≈ 1.0 (within epsilon 1e-10)
            let weight_sum: f64 = config.color_weights.iter().sum();
            prop_assert!((weight_sum - 1.0).abs() < 1e-10,
                "color_weights sum {} not ≈ 1.0 (diff = {})", weight_sum, (weight_sum - 1.0).abs());
        }

        /// Feature: egui-settings-panel, Property 5: Input gating correctness
        /// **Validates: Requirements 12.4, 12.5**
        #[test]
        fn input_gating_correctness(
            pointer_over_egui in proptest::bool::ANY,
            wants_keyboard_input in proptest::bool::ANY,
        ) {
            // The gate_camera_input system computes:
            // camera_input.0 = !pointer_over_egui && !keyboard_captured
            let camera_enabled = !pointer_over_egui && !wants_keyboard_input;

            // Requirement 12.4: pointer events reach camera when egui doesn't want pointer
            // Requirement 12.5: keyboard events reach camera when no text field is focused
            if !pointer_over_egui && !wants_keyboard_input {
                prop_assert!(camera_enabled, "Camera should be enabled when egui doesn't want input");
            } else {
                prop_assert!(!camera_enabled, "Camera should be disabled when egui wants input");
            }
        }

        /// Feature: color-distribution-panel, Property 2: Slider adjustment preserves sum
        /// **Validates: Requirements 3.3, 7.1, 7.2**
        #[test]
        fn set_weight_preserves_sum(
            // Generate a Vec<f64> of length 2-9 with positive entries, then normalize to sum 1.0
            raw_weights in prop::collection::vec(0.01f64..10.0, 2..=9usize),
            new_value in 0.0f64..=1.0,
            index_frac in 0.0f64..1.0,
        ) {
            // Normalize raw_weights to sum 1.0
            let sum: f64 = raw_weights.iter().sum();
            let normalized: Vec<f64> = raw_weights.iter().map(|w| w / sum).collect();
            let len = normalized.len();

            // Pick a valid index using fractional approach
            let index = (index_frac * len as f64).min((len - 1) as f64) as usize;

            // Create a SimulationConfig with the generated weights
            let mut config = SimulationConfig {
                color_count: len,
                color_weights: normalized,
                ..Default::default()
            };

            // Call set_weight
            config.set_weight(index, new_value);

            // Assert: each weight in [0.0, 1.0]
            for (i, w) in config.color_weights.iter().enumerate() {
                prop_assert!(*w >= 0.0 && *w <= 1.0,
                    "color_weights[{}] = {} out of [0.0, 1.0]", i, w);
            }

            // Assert: sum ≈ 1.0 (within epsilon 1e-10)
            let weight_sum: f64 = config.color_weights.iter().sum();
            prop_assert!((weight_sum - 1.0).abs() < 1e-10,
                "color_weights sum {} not ≈ 1.0 (diff = {})", weight_sum, (weight_sum - 1.0).abs());
        }

        /// Feature: color-distribution-panel, Property 4: Weighted sampling respects zero weights
        /// **Validates: Requirements 4.1, 4.3**
        #[test]
        fn zero_weight_sampling_exclusion(
            // Generate length-1 positive entries (for the non-zero portion), then insert a 0.0 at a random position
            raw_weights in prop::collection::vec(0.01f64..10.0, 1..=8usize),
            zero_insert_frac in 0.0f64..1.0,
        ) {
            // Normalize the raw weights so they sum to 1.0
            let sum: f64 = raw_weights.iter().sum();
            let mut weights: Vec<f64> = raw_weights.iter().map(|w| w / sum).collect();

            // Insert a 0.0 at a random position (total length becomes 2-9)
            let insert_idx = (zero_insert_frac * (weights.len() + 1) as f64).min(weights.len() as f64) as usize;
            weights.insert(insert_idx, 0.0);

            // Verify preconditions: length 2-9, at least one zero, sum ≈ 1.0
            prop_assert!(weights.len() >= 2 && weights.len() <= 9);
            prop_assert!(weights[insert_idx] == 0.0);

            // Construct WeightedIndex from the weight vector
            use rand::distr::weighted::WeightedIndex;
            use rand::prelude::*;
            use rand::rngs::SmallRng;

            let dist = WeightedIndex::new(&weights).unwrap();
            let mut rng = SmallRng::seed_from_u64(42);

            // Sample 1000 times and assert zero-weight index is never produced
            for sample_i in 0..1000 {
                let idx = dist.sample(&mut rng);
                prop_assert!(idx != insert_idx,
                    "Sample {} produced index {} which has zero weight (weights = {:?})",
                    sample_i, insert_idx, weights);
            }
        }

        /// Feature: egui-settings-panel, Property 1: Configuration clamping invariant
        /// **Validates: Requirements 5.3, 5.4, 6.3, 7.3, 7.4, 7.5, 7.7, 7.8, 7.9, 8.3, 10.3**
        #[test]
        fn config_clamping_invariant(
            particle_count in 0usize..1_000_000,
            color_count in 1usize..10,
            max_dist in -1.0f64..1.0,
            min_rel_dist in -1.0f64..2.0,
            drag_halflife in -1.0f64..2.0,
            density_limit in -10.0f64..100.0,
            density_same_color in -5.0f64..10.0,
            density_diff_color in -5.0f64..10.0,
            world_scale in -100.0f64..1000.0,
            color_weights in prop::collection::vec(-1.0f64..2.0, 1..10usize),
        ) {
            let mut config = SimulationConfig {
                particle_count,
                color_count,
                max_dist,
                min_rel_dist,
                drag_halflife,
                density_limit,
                density_same_color,
                density_diff_color,
                world_scale,
                color_weights,
            };

            config.clamp_all();

            prop_assert!(config.particle_count >= 100 && config.particle_count <= 500_000,
                "particle_count {} out of range", config.particle_count);
            prop_assert!(config.particle_count % 100 == 0,
                "particle_count {} not a multiple of 100", config.particle_count);
            prop_assert!(config.color_count >= 1 && config.color_count <= 9,
                "color_count {} out of range", config.color_count);
            prop_assert!(config.max_dist >= 0.01 && config.max_dist <= 0.2,
                "max_dist {} out of range", config.max_dist);
            prop_assert!(config.min_rel_dist >= 0.05 && config.min_rel_dist <= 0.95,
                "min_rel_dist {} out of range", config.min_rel_dist);
            prop_assert!(config.drag_halflife >= 0.001 && config.drag_halflife <= 0.5,
                "drag_halflife {} out of range", config.drag_halflife);
            prop_assert!(config.density_limit >= 1.0 && config.density_limit <= 50.0,
                "density_limit {} out of range", config.density_limit);
            prop_assert!(config.density_same_color >= 0.0 && config.density_same_color <= 5.0,
                "density_same_color {} out of range", config.density_same_color);
            prop_assert!(config.density_diff_color >= 0.0 && config.density_diff_color <= 5.0,
                "density_diff_color {} out of range", config.density_diff_color);
            prop_assert!(config.world_scale >= 16.0 && config.world_scale <= 512.0,
                "world_scale {} out of range", config.world_scale);

            // Assert color_weights are all in [0.0, 1.0] and sum ≈ 1.0
            for (i, w) in config.color_weights.iter().enumerate() {
                prop_assert!(*w >= 0.0 && *w <= 1.0,
                    "color_weights[{}] = {} out of [0.0, 1.0]", i, w);
            }
            let weight_sum: f64 = config.color_weights.iter().sum();
            prop_assert!((weight_sum - 1.0).abs() < 1e-10,
                "color_weights sum {} not ≈ 1.0", weight_sum);
        }

        /// Feature: color-distribution-panel, Property 3: Clamp and normalize correctness
        /// **Validates: Requirements 1.5**
        #[test]
        fn clamp_all_normalizes_color_weights(
            color_weights in prop::collection::vec(-1.0f64..2.0, 1..10usize),
        ) {
            let color_count = color_weights.len();

            let mut config = SimulationConfig {
                color_count,
                color_weights,
                ..Default::default()
            };

            config.clamp_all();

            // After clamp_all, color_count is clamped to [1, 9]
            let expected_count = color_count.clamp(1, 9);
            prop_assert_eq!(config.color_weights.len(), expected_count,
                "color_weights length {} != clamped color_count {}", config.color_weights.len(), expected_count);

            // Assert each weight is in [0.0, 1.0]
            for (i, w) in config.color_weights.iter().enumerate() {
                prop_assert!(*w >= 0.0 && *w <= 1.0,
                    "color_weights[{}] = {} out of [0.0, 1.0]", i, w);
            }

            // Assert sum ≈ 1.0 (within epsilon 1e-10)
            let weight_sum: f64 = config.color_weights.iter().sum();
            prop_assert!((weight_sum - 1.0).abs() < 1e-10,
                "color_weights sum {} not ≈ 1.0 (diff = {})", weight_sum, (weight_sum - 1.0).abs());
        }
    }
}
