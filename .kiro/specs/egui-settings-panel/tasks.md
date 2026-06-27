# Implementation Plan: egui Settings Panel

## Overview

This plan replaces the existing `DebugPlugin` text overlay with a full egui-based settings panel. It introduces `bevy_egui` as a dependency, creates a `SettingsPanelPlugin` with a `SimulationConfig` resource, migrates gizmos and debug durations out of the old plugin, wires up message-driven particle count / palette changes, and renders an interactive egui side panel with collapsible sections for all runtime-configurable parameters.

## Tasks

- [x] 1. Add bevy_egui dependency and create foundation modules
  - [x] 1.1 Add `bevy_egui` 0.40 to Cargo.toml and create `src/settings_panel.rs` module
    - Add `bevy_egui = "0.40"` to `[dependencies]` in Cargo.toml
    - Create `src/settings_panel.rs` with an empty `SettingsPanelPlugin` struct implementing `Plugin`
    - Register `EguiPlugin::default()` inside the plugin's `build` method
    - Add `mod settings_panel;` to `main.rs`
    - Add `SettingsPanelPlugin` to the `add_plugins` tuple in `main.rs`
    - Verify compilation with `cargo check`
    - _Requirements: 1.1, 1.2, 1.3_

  - [x] 1.2 Create `SimulationConfig` resource with defaults and constraints
    - Define `SimulationConfig` struct with all fields: `particle_count`, `color_count`, `max_dist`, `min_rel_dist`, `drag_halflife`, `density_limit`, `density_same_color`, `density_diff_color`, `world_scale`
    - Implement `Default` with values: 50_000, 5, 0.045, 0.333, 0.043, 12.0, 1.0, 0.5, 128.0
    - Implement a `clamp_all(&mut self)` method enforcing all range constraints from the design table
    - Register `SimulationConfig` via `init_resource` in the plugin
    - _Requirements: 5.1, 5.3, 6.1, 6.3, 7.1, 7.3, 7.4, 7.5, 7.6, 7.7, 7.8, 7.9, 8.1, 8.3_

  - [x] 1.3 Create `PanelVisibility` resource and `FpsTracker` resource
    - Define `PanelVisibility { visible: bool }` defaulting to `true`
    - Define `FpsTracker { frame_count: u32, elapsed: f32, current_fps: f32 }` with update logic that refreshes `current_fps` every 100ms
    - Register both via `init_resource` in the plugin
    - _Requirements: 2.3, 3.1_

  - [x] 1.4 Create `CameraInputEnabled` resource and `RebuildPalette` message
    - Define `CameraInputEnabled(pub bool)` defaulting to `true`
    - Define `#[derive(Message)] struct RebuildPalette;`
    - Register `CameraInputEnabled` via `init_resource` and `RebuildPalette` via `add_message` in the plugin
    - _Requirements: 12.4, 12.5, 6.2_

- [x] 2. Retire DebugPlugin and migrate gizmos/durations
  - [x] 2.1 Remove DebugPlugin registration and migrate `DebugDurations` insertion
    - Remove `DebugPlugin` from the `add_plugins` tuple in `main.rs`
    - Remove `use crate::debug::DebugPlugin;` from `main.rs`
    - In `SettingsPanelPlugin::build`, insert `DebugDurations::with_order(&["islands", "forces", "stepping"])` as a resource
    - Ensure `debug.rs` still compiles (keep the module for `DebugDurations` struct and its impls, remove the `Plugin` impl or gate it)
    - Remove the `FpsOverlayPlugin` usage (no longer registered anywhere)
    - _Requirements: 4.1, 4.2, 4.3, 4.5_

  - [x] 2.2 Migrate gizmo spawning to `SettingsPanelPlugin`
    - Create `setup_gizmos` system in `settings_panel.rs` that reads `SimulationConfig` and spawns:
      - A bounding-box wireframe cube at `world_scale` dimensions (using `GizmoAsset`)
      - RGB axis arrows (X=red, Y=green, Z=blue)
    - Add a `BoundingBoxGizmo` marker component on the gizmo entity for later scale updates
    - Register `setup_gizmos` in `Startup` schedule within the plugin
    - Remove the gizmo spawning code from `debug.rs` `setup_ui` function
    - _Requirements: 4.4_

- [x] 3. Checkpoint
  - Ensure `cargo check` passes with DebugPlugin removed, bevy_egui integrated, and all new resources registered. Ask the user if questions arise.

- [x] 4. Implement panel visibility toggle and input gating
  - [x] 4.1 Implement `toggle_panel` system
    - On Escape key press, flip `PanelVisibility.visible`
    - Register in `Update` schedule
    - _Requirements: 2.1, 2.2_

  - [x] 4.2 Implement `gate_camera_input` system
    - Read `EguiContexts` to check `is_pointer_over_area()` and `wants_keyboard_input()`
    - Set `CameraInputEnabled.0 = !pointer_over_egui && !keyboard_captured`
    - Register in `Update` schedule
    - _Requirements: 12.4, 12.5_

  - [x] 4.3 Integrate `CameraInputEnabled` into camera systems
    - In `camera.rs`, add `Res<CameraInputEnabled>` parameter to `update_camera`, `pan_bodies`, `cancel_auto_orbit_on_input`, and `auto_orbit_camera`
    - Add early-return `if !camera_input.0 { return; }` at the start of each
    - Make `CameraInputEnabled` public from `settings_panel.rs` module
    - _Requirements: 12.4, 12.5, 2.4_

- [x] 5. Implement the egui render system — panel skeleton and Performance section
  - [x] 5.1 Create `render_panel` system with side panel layout
    - System signature accepts: `EguiContexts`, `Res<PanelVisibility>`, `ResMut<SimulationConfig>`, `ResMut<ForceMatrix>`, `ResMut<CurrentPositioner>`, `ResMut<DensityAttenuation>`, `Res<DebugDurations>`, `ResMut<FpsTracker>`, `Res<Time>`, `MessageWriter<UpdateBodies>`, `MessageWriter<RebuildPalette>`
    - Early-return if `!visibility.visible`
    - Use `egui::SidePanel::left("settings_panel").default_width(320.0)` with vertical `ScrollArea`
    - Register in `EguiPrimaryContextPass` schedule
    - _Requirements: 12.1, 12.2, 12.3, 2.2_

  - [x] 5.2 Implement Performance section (FPS + timing durations)
    - Update `FpsTracker` each frame; display `current_fps` only when 100ms has elapsed since last refresh
    - Display rolling average durations from `DebugDurations` for "islands", "forces", "stepping" in milliseconds to 3 decimal places
    - Wrap in `CollapsingHeader::new("Performance").default_open(true)`
    - _Requirements: 3.1, 3.4_

- [x] 6. Implement Physics section in panel
  - [x] 6.1 Add physics constant sliders
    - Add sliders for `max_dist` (0.01..=0.2, step 0.005), `min_rel_dist` (0.05..=0.95, step 0.05), `drag_halflife` (0.001..=0.5, step 0.01)
    - Add sliders for `density_limit` (1.0..=50.0, step 1.0), `density_same_color` (0.0..=5.0, step 0.25), `density_diff_color` (0.0..=5.0, step 0.25)
    - Add toggle checkbox for `DensityAttenuation` status, displaying "ON"/"OFF"
    - Wrap in `CollapsingHeader::new("Physics").default_open(true)`
    - _Requirements: 7.1, 7.2, 7.3, 7.4, 7.5, 7.6, 7.7, 7.8, 7.9, 7.10, 7.11, 3.5_

- [x] 7. Implement Force Matrix section in panel
  - [x] 7.1 Add force matrix type dropdown and editable grid
    - Add `egui::ComboBox` listing all 7 `ForceMatrixType` variants with the active variant selected
    - When selection changes: regenerate `ForceMatrix` with `ForceMatrix::new(config.color_count, new_type)`; skip if same type
    - Display force matrix as a grid of `DragValue` fields (color_count × color_count), clamped to [-1.0, 1.0], 3 decimal places, with row/column headers
    - Wrap in `CollapsingHeader::new("Force Matrix").default_open(true)`
    - _Requirements: 9.1, 9.2, 9.3, 10.1, 10.2, 10.3, 10.4, 3.2_

- [x] 8. Implement Simulation section in panel
  - [x] 8.1 Add particle count input, color count input, and positioner dropdown
    - Add `DragValue` for particle count (100..=500_000, step 100, integer), fire `UpdateBodies` message on value commit (lost_focus or enter)
    - Add `DragValue` for color count (1..=9, step 1, integer), fire `RebuildPalette` message when value changes and differs from current palette size
    - Add `ComboBox` listing all 10 `PositionerType` variants, updating `CurrentPositioner` resource on selection
    - Display current positioner name
    - Wrap in `CollapsingHeader::new("Simulation").default_open(true)`
    - _Requirements: 5.1, 5.2, 5.3, 5.4, 6.1, 6.2, 6.3, 6.4, 11.1, 11.2, 3.3_

- [x] 9. Implement Appearance section in panel
  - [x] 9.1 Add world scale slider
    - Add slider for `world_scale` (16.0..=512.0, step 1.0)
    - Wrap in `CollapsingHeader::new("Appearance").default_open(true)`
    - _Requirements: 8.1, 8.3_

- [x] 10. Checkpoint
  - Ensure all panel sections render correctly with `cargo check`. Ask the user if questions arise.

- [x] 11. Wire physics pipeline to read from SimulationConfig
  - [x] 11.1 Refactor `physics.rs` to read `SimulationConfig` instead of module-level constants
    - Replace `const MAX_DIST`, `MIN_REL_DIST`, `DRAG_HALFLIFE`, `DENSITY_LIMIT`, `DENSITY_SAME_COLOR`, `DENSITY_DIFF_COLOR` with reads from `Res<SimulationConfig>`
    - Compute derived values (`MAX_DIST_RECIP`, `MAX_DIST_SQRD`, `MIN_DIST_RECIP`, `INV_MIN_DIST_RECIP`) from config each tick
    - Pass config values into `get_computation` function (or pre-compute struct)
    - _Requirements: 7.2, 7.11_

  - [x] 11.2 Refactor `translate_bodies` and `main.rs::translate` to use `SimulationConfig::world_scale`
    - Replace `SCALE` constant usage in `translate_bodies` and `build_batch` with config read
    - Ensure gizmo updates when `world_scale` changes (implement `update_gizmo_scale` system)
    - _Requirements: 8.2_

  - [x] 11.3 Implement island grid rebuild when `max_dist` changes
    - Add `rebuild_islands_if_needed` system that detects `SimulationConfig` changes
    - If `max_dist.recip().floor() as usize != grid.side`, rebuild `Islands`, `IslandNeighborIxs`, `IslandNeighborhoods`, and `IslandGrid` resources
    - Register in `Update` schedule before `FixedUpdate`
    - _Requirements: 7.2_

- [x] 12. Wire particle count and palette rebuild message handlers
  - [x] 12.1 Refactor `match_body_count` to read from `SimulationConfig::particle_count`
    - Replace the `BODIES` constant with `config.particle_count` in the existing `match_body_count` system
    - Ensure newly spawned particles use current `CurrentPositioner`
    - Ensure randomly selected despawns preserve surviving particle state
    - _Requirements: 5.2_

  - [x] 12.2 Implement `handle_palette_rebuild` system triggered by `RebuildPalette` message
    - Read `SimulationConfig::color_count`
    - Rebuild `Palette` with new size, regenerate `ForceMatrix` at new dimensions with current `matrix_type`, randomly reassign all particle colors
    - Skip all work if palette size already matches (Requirement 6.4)
    - _Requirements: 6.2, 6.4_

  - [x] 12.3 Remove the `config.rs` file and the `BODIES`/`COLORS` constants
    - Remove `src/config.rs`
    - Remove `mod config;` from `main.rs`
    - Update all imports that reference `crate::config::BODIES` or `crate::config::COLORS` to use `SimulationConfig` resource instead
    - Update `palette.rs` startup to read initial `color_count` from `SimulationConfig`
    - Update `forces.rs` startup to read initial `color_count` from `SimulationConfig`
    - _Requirements: 5.1, 6.1, 7.1_

- [x] 13. Checkpoint
  - Ensure all tests pass and `cargo check` succeeds with full integration. Ask the user if questions arise.

- [x] 14. Property-based tests
  - [x] 14.1 Write property test for configuration clamping invariant
    - **Property 1: Configuration clamping invariant**
    - Use `proptest` to generate arbitrary numeric values for all `SimulationConfig` fields
    - Assert `clamp_all()` produces values within defined ranges for every field
    - **Validates: Requirements 5.3, 5.4, 6.3, 7.3, 7.4, 7.5, 7.7, 7.8, 7.9, 8.3, 10.3**

  - [x] 14.2 Write property test for force matrix dimension invariant
    - **Property 2: Force matrix dimension invariant**
    - Generate `color_count` in 1..=9 and random `ForceMatrixType`
    - Assert `ForceMatrix::new(color_count, type).data.len() == color_count * color_count`
    - Assert all cell values in [-1.0, 1.0]
    - **Validates: Requirements 9.2, 10.3**

  - [x] 14.3 Write property test for force matrix display completeness
    - **Property 3: Force matrix display completeness**
    - Generate random matrices, format to string, parse and verify it contains matrix type name, color count, and exactly `color_count²` numeric values at 3 decimal places
    - **Validates: Requirements 3.2, 10.4**

  - [x] 14.4 Write property test for rolling average correctness
    - **Property 4: Rolling average correctness**
    - Generate random Duration sequences (1..=128 items), verify `AvgDuration` reports the arithmetic mean of the most recent min(n, 64) samples in milliseconds
    - **Validates: Requirements 3.4**

  - [x] 14.5 Write property test for input gating correctness
    - **Property 5: Input gating correctness**
    - Generate random boolean pairs (pointer_over_egui, wants_keyboard_input)
    - Assert `CameraInputEnabled = !pointer_over_egui && !wants_keyboard_input`
    - **Validates: Requirements 12.4, 12.5**

- [x] 15. Final checkpoint
  - Ensure all tests pass and `cargo clippy` produces no warnings. Ask the user if questions arise.

## Notes

- Tasks marked with `*` are optional and can be skipped for faster MVP
- Each task references specific requirements for traceability
- Checkpoints ensure incremental validation
- Property tests validate universal correctness properties from the design document
- The `proptest` crate must be added as a dev-dependency for property tests
- The `DebugDurations` struct and its impls remain in `debug.rs` (public) — only the `DebugPlugin` impl and its systems are removed
- Commented-out code in `forces.rs` (clipboard, UI methods) is preserved per project convention
- The `UpdateBodies` message already exists in `main.rs`; the new `RebuildPalette` message is added by the settings panel plugin

## Task Dependency Graph

```json
{
  "waves": [
    { "id": 0, "tasks": ["1.1", "1.2", "1.3", "1.4"] },
    { "id": 1, "tasks": ["2.1", "2.2"] },
    { "id": 2, "tasks": ["4.1", "4.2", "4.3"] },
    { "id": 3, "tasks": ["5.1"] },
    { "id": 4, "tasks": ["5.2", "6.1", "7.1", "8.1", "9.1"] },
    { "id": 5, "tasks": ["11.1", "11.2", "11.3", "12.1", "12.2", "12.3"] },
    { "id": 6, "tasks": ["14.1", "14.2", "14.3", "14.4", "14.5"] }
  ]
}
```
