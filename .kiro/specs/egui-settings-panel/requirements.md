# Requirements Document

## Introduction

This feature replaces the existing `DebugPlugin` text overlay with a full egui-based settings panel. The panel consolidates all debug information (FPS, force matrix, timing) and exposes compile-time constants as runtime-configurable parameters. This gives users interactive control over simulation behavior without recompiling.

## Glossary

- **Settings_Panel**: The egui window that displays debug information and runtime-configurable parameters
- **Simulation**: The Particle Life 3D application including physics, rendering, and camera systems
- **Force_Matrix**: The NxN matrix of attraction/repulsion values governing inter-color particle interactions
- **Particle_Count**: The number of particles in the simulation (currently hardcoded as `BODIES = 50_000`)
- **Color_Count**: The number of distinct particle color types (currently hardcoded as `COLORS = 5`)
- **Physics_Constants**: Values governing force computation: `MAX_DIST`, `MIN_REL_DIST`, `DRAG_HALFLIFE`, `DENSITY_LIMIT`, `DENSITY_SAME_COLOR`, `DENSITY_DIFF_COLOR`
- **Density_Attenuation**: A toggle that attenuates attractive forces in high-density regions
- **Positioner**: The algorithm used to generate initial particle positions when spawning or resetting
- **Force_Matrix_Type**: The named preset pattern used to generate the force matrix (Chains, Checkered, Random, etc.)
- **World_Scale**: The multiplier converting unit-cube coordinates to world-space rendering coordinates (currently `SCALE = 128.0`)

## Requirements

### Requirement 1: Add egui Dependency

**User Story:** As a developer, I want egui integrated into the Bevy app, so that I can build interactive UI panels.

#### Acceptance Criteria

1. THE Simulation SHALL include the `bevy_egui` crate as a dependency in Cargo.toml with a version compatible with Bevy 0.19
2. THE Simulation SHALL register the `EguiPlugin` in the `add_plugins` tuple during app initialization
3. WHEN the `bevy_egui` dependency and `EguiPlugin` registration are added, THE Simulation SHALL compile successfully with `cargo check` and produce no errors

### Requirement 2: Settings Panel Visibility Toggle

**User Story:** As a user, I want to toggle the settings panel with a key press, so that I can show or hide it without disrupting the simulation.

#### Acceptance Criteria

1. WHEN the user presses the Escape key, THE Settings_Panel SHALL toggle between visible and hidden states
2. WHILE the Settings_Panel is hidden, THE Simulation SHALL not render any egui elements
3. THE Settings_Panel SHALL default to the visible state on application startup
4. WHILE the Settings_Panel is hidden or visible, THE Simulation SHALL continue running physics and rendering without interruption

### Requirement 3: Display Debug Information

**User Story:** As a user, I want to see FPS, force matrix, positioner name, timing durations, and density attenuation status in the settings panel, so that I can monitor simulation performance without a separate overlay.

#### Acceptance Criteria

1. WHILE the Settings_Panel is visible, THE Settings_Panel SHALL display the current frames-per-second as a numeric value, updated at most every 100 milliseconds
2. WHILE the Settings_Panel is visible, THE Settings_Panel SHALL display the Force_Matrix contents including the matrix type name, the color count, and all cell values formatted to 3 decimal places arranged in rows of length equal to the color count
3. WHILE the Settings_Panel is visible, THE Settings_Panel SHALL display the current Positioner name matching the active PositionerType variant name
4. WHILE the Settings_Panel is visible, THE Settings_Panel SHALL display the rolling average durations in milliseconds (to 3 decimal places, averaged over the last 64 samples) for the islands, forces, and stepping computation phases
5. WHILE the Settings_Panel is visible, THE Settings_Panel SHALL display the current Density_Attenuation status as either "ON" or "OFF"
6. WHEN the user toggles the Settings_Panel from visible to hidden, THE Settings_Panel SHALL hide all debug information fields including FPS, Force_Matrix, Positioner name, timing durations, and Density_Attenuation status

### Requirement 4: Retire DebugPlugin

**User Story:** As a developer, I want to remove the DebugPlugin, so that there is a single source of debug and settings UI without redundant overlays.

#### Acceptance Criteria

1. THE Simulation SHALL not register the DebugPlugin in its plugin set
2. THE Simulation SHALL not register the Bevy `FpsOverlayPlugin` (neither directly nor via DebugPlugin)
3. THE Simulation SHALL insert the `DebugDurations` resource (initialized with ordered keys `["islands", "forces", "stepping"]`) during app setup, outside of DebugPlugin, so that physics systems can record timing data
4. THE Simulation SHALL spawn the bounding-box wireframe gizmo (cube at SCALE dimensions) and RGB axis arrow gizmos during Startup, in a system registered outside of DebugPlugin
5. THE Simulation SHALL not spawn the `DebugText` UI entities or register the `UiState` state machine previously managed by DebugPlugin

### Requirement 5: Runtime-Configurable Particle Count

**User Story:** As a user, I want to change the number of particles at runtime, so that I can observe behavior at different densities without restarting.

#### Acceptance Criteria

1. WHILE the Settings_Panel is visible, THE Settings_Panel SHALL display a numeric input for Particle_Count showing the current number of spawned particles, defaulting to 50,000
2. WHEN the user commits a new Particle_Count value (by pressing Enter or defocusing the input), THE Simulation SHALL spawn or despawn particles to match the new count within 1 second, where newly spawned particles use the current Positioner for initial positions and despawned particles are selected randomly while surviving particles retain their existing position, velocity, and color
3. THE Settings_Panel SHALL constrain Particle_Count input to integer values in the range 100 to 500,000 inclusive with a step size of 100
4. IF the user enters a Particle_Count value outside the range 100 to 500,000 or a non-integer value, THEN THE Settings_Panel SHALL clamp the value to the nearest valid bound and display the clamped value in the input

### Requirement 6: Runtime-Configurable Color Count

**User Story:** As a user, I want to adjust the number of color types at runtime, so that I can explore different interaction complexity levels.

#### Acceptance Criteria

1. WHILE the Settings_Panel is visible, THE Settings_Panel SHALL display a numeric input for Color_Count showing the current value as an integer in the range 1 to 9 inclusive
2. WHEN the user changes the Color_Count value, THE Simulation SHALL rebuild the palette to contain exactly Color_Count evenly-spaced hues, regenerate the Force_Matrix at Color_Count × Color_Count dimensions using the currently selected Force_Matrix type, and randomly reassign each particle a color index in the range 0 to Color_Count − 1
3. THE Settings_Panel SHALL constrain Color_Count input to the range 1 to 9 inclusive by clamping any out-of-range value to the nearest bound
4. IF the user sets Color_Count to the value already in use, THEN THE Simulation SHALL not rebuild the palette, regenerate the Force_Matrix, or recolor particles

### Requirement 7: Runtime-Configurable Physics Constants

**User Story:** As a user, I want to adjust physics parameters at runtime, so that I can explore different force behaviors interactively.

#### Acceptance Criteria

1. WHILE the Settings_Panel is visible, THE Settings_Panel SHALL display sliders for MAX_DIST, MIN_REL_DIST, and DRAG_HALFLIFE with initial values of 0.045, 0.333, and 0.043 respectively
2. WHEN the user adjusts a physics constant slider, THE Simulation SHALL use the updated value in the next physics tick without requiring a restart or re-initialization
3. THE Settings_Panel SHALL constrain MAX_DIST to the range 0.01 to 0.2 with a step size no larger than 0.005
4. THE Settings_Panel SHALL constrain MIN_REL_DIST to the range 0.05 to 0.95 with a step size no larger than 0.05
5. THE Settings_Panel SHALL constrain DRAG_HALFLIFE to the range 0.001 to 0.5 with a step size no larger than 0.01
6. WHILE the Settings_Panel is visible, THE Settings_Panel SHALL display sliders for DENSITY_LIMIT, DENSITY_SAME_COLOR, and DENSITY_DIFF_COLOR with initial values of 12.0, 1.0, and 0.5 respectively
7. THE Settings_Panel SHALL constrain DENSITY_LIMIT to the range 1.0 to 50.0 with a step size no larger than 1.0
8. THE Settings_Panel SHALL constrain DENSITY_SAME_COLOR to the range 0.0 to 5.0 with a step size no larger than 0.25
9. THE Settings_Panel SHALL constrain DENSITY_DIFF_COLOR to the range 0.0 to 5.0 with a step size no larger than 0.25
10. WHILE the Settings_Panel is visible, THE Settings_Panel SHALL display a toggle for Density_Attenuation with an initial state of enabled
11. WHEN the user changes the Density_Attenuation toggle, THE Simulation SHALL enable or disable density-based force attenuation in the next physics tick corresponding to the toggle state

### Requirement 8: Runtime-Configurable World Scale

**User Story:** As a user, I want to adjust the world scale at runtime, so that I can change the visual spread of particles without restarting.

#### Acceptance Criteria

1. WHILE the Settings_Panel is visible, THE Settings_Panel SHALL display a slider for World_Scale with a default value of 128.0, a minimum of 16.0, a maximum of 512.0, and a step increment of 1.0
2. WHEN the user adjusts the World_Scale slider, THE Simulation SHALL apply the new scale value to all position-to-transform translations and to the bounding-box gizmo dimensions within the same frame
3. THE Settings_Panel SHALL constrain World_Scale to the range 16.0 to 512.0, clamping any value outside this range to the nearest bound

### Requirement 9: Force Matrix Type Selection

**User Story:** As a user, I want to select a force matrix preset from the panel, so that I can switch patterns without memorizing keyboard shortcuts.

#### Acceptance Criteria

1. WHILE the Settings_Panel is visible, THE Settings_Panel SHALL display a dropdown listing all seven Force_Matrix_Type variants (Chains, Checkered, RandomEx, Random, Snakes, Zeros, Ones) with the currently active Force_Matrix_Type shown as the selected value
2. WHEN the user selects a Force_Matrix_Type from the dropdown that differs from the currently active type, THE Simulation SHALL replace the Force_Matrix resource by regenerating it using the selected type and the current Color_Count; IF the selected type equals the currently active type, THEN THE Simulation SHALL skip regeneration
3. WHEN the user selects a Force_Matrix_Type from the dropdown, THE Settings_Panel SHALL update the dropdown's selected value to reflect the newly active Force_Matrix_Type within the same frame

### Requirement 10: Force Matrix Cell Editing

**User Story:** As a user, I want to edit individual force matrix cells in the panel, so that I can fine-tune inter-color interactions.

#### Acceptance Criteria

1. WHILE the Settings_Panel is visible, THE Settings_Panel SHALL display the Force_Matrix as a grid of editable numeric fields arranged in Color_Count rows and Color_Count columns, with row and column headers indicating color indices
2. WHEN the user modifies a cell value, THE Simulation SHALL use the updated Force_Matrix in the next physics tick
3. THE Settings_Panel SHALL constrain individual cell values to the range -1.0 to 1.0, clamping any value outside this range to the nearest bound
4. THE Settings_Panel SHALL display cell values with 3 decimal places of precision

### Requirement 11: Positioner Selection

**User Story:** As a user, I want to choose the initial particle positioner from the panel, so that I can set the spawn shape before resetting.

#### Acceptance Criteria

1. WHILE the Settings_Panel is visible, THE Settings_Panel SHALL display a dropdown listing all 10 Positioner variants (BigBang, Sphere, Uniform, UniformSphere, Rod, Cylinder, STorus, MTorus, LTorus, Spiral) with the currently active variant shown as the selected value
2. WHEN the user selects a Positioner from the dropdown, THE Simulation SHALL update the CurrentPositioner resource to the selected variant without repositioning existing particles
3. WHEN the user triggers a simulation reset after selecting a Positioner, THE Simulation SHALL spawn particles using the currently selected Positioner variant; IF no valid Positioner selection exists, THEN THE Simulation SHALL default to the first variant (BigBang)

### Requirement 12: Panel Layout and Usability

**User Story:** As a user, I want the settings panel to be organized into collapsible sections, so that I can focus on the parameters I care about.

#### Acceptance Criteria

1. THE Settings_Panel SHALL organize controls into collapsible sections: Performance, Physics, Force Matrix, Simulation, and Appearance, with all sections expanded by default on application start
2. THE Settings_Panel SHALL be rendered as a left-anchored side panel with a target width of 320 logical pixels, allowing minor overages for UI framework rounding or minimum widget sizes
3. IF the Settings_Panel content height exceeds the viewport height, THEN THE Settings_Panel SHALL provide vertical scrolling to access all controls
4. WHILE the Settings_Panel is visible, THE Settings_Panel SHALL forward all pointer input (mouse movement, clicks, and scroll events) to camera orbit and zoom systems whenever bevy_egui reports that egui does not want pointer input
5. WHILE the Settings_Panel is visible, THE Settings_Panel SHALL NOT consume keyboard input for WASD panning, scroll zoom, or other camera controls when no egui text field is focused
