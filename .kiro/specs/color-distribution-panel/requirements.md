# Requirements Document

## Introduction

Add a "Distribution" collapsing section to the egui settings panel that controls the relative weight/proportion of each particle color. When particles are spawned or recolored, the distribution respects these weights instead of using uniform random assignment. Changing weights live-redistributes existing particles.

## Glossary

- **Settings_Panel**: The egui left panel containing collapsible sections for tuning simulation parameters
- **Distribution_Section**: A new collapsible section within the Settings_Panel for configuring color weights
- **Color_Weight**: A non-negative f64 value in the range [0.0, 1.0] representing the probability that a particle is assigned a given color index; all Color_Weights must sum to exactly 1.0
- **Weighted_Distribution**: The probability distribution defined directly by Color_Weights (which always sum to 1.0); probability of color i = Color_Weights[i]
- **SimulationConfig**: The Bevy Resource holding all tunable simulation parameters
- **Palette**: The Bevy Resource managing materials per color index
- **RebuildPalette**: A Bevy message that triggers palette rebuild and particle recoloring
- **UpdateBodies**: A Bevy message that triggers particle count adjustment
- **RedistributeColors**: A new Bevy message that triggers recoloring of existing particles according to current weights

## Requirements

### Requirement 1: Color Weight Storage

**User Story:** As a user, I want the simulation to store a weight value for each active color, so that particle color assignment can be driven by configurable proportions.

#### Acceptance Criteria

1. THE SimulationConfig SHALL store a Color_Weights list of f64 values whose length equals the current color_count (ranging from 1 to 9 entries), where all entries sum to exactly 1.0
2. WHEN the color_count increases, THE SimulationConfig SHALL redistribute weights evenly: each new color receives an equal share taken proportionally from existing colors, such that the sum remains 1.0
3. WHEN the color_count decreases, THE SimulationConfig SHALL remove entries for colors beyond the new color_count and redistribute their weight proportionally among the remaining colors, such that the sum remains 1.0
4. THE SimulationConfig SHALL initialize Color_Weights with color_count entries each set to 1.0 / color_count at startup, producing a uniform distribution that sums to 1.0
5. WHEN clamp_all is invoked, THE SimulationConfig SHALL clamp each Color_Weight to the range [0.0, 1.0] and normalize the list so it sums to 1.0

### Requirement 2: Distribution Section UI

**User Story:** As a user, I want a "Distribution" collapsing section in the settings panel, so that I can visually adjust color weights.

#### Acceptance Criteria

1. THE Settings_Panel SHALL display a "Distribution" collapsing section between the "Simulation" and "Appearance" sections when color_count is 1 or greater
2. THE Distribution_Section SHALL be open by default
3. THE Distribution_Section SHALL display one slider per active color index, labeled with the color index number (0 through color_count minus 1)
4. WHEN the color_count is 1, THE Distribution_Section SHALL display exactly one slider for color index 0 with weight locked at 1.0
5. WHEN color_count changes, THE Distribution_Section SHALL dynamically add or remove sliders to match the new color_count

### Requirement 3: Weight Slider Behavior

**User Story:** As a user, I want sliders that control each color's weight with sensible ranges and steps, so that adjustments are intuitive.

#### Acceptance Criteria

1. THE Distribution_Section SHALL render one weight slider per active color (matching the current color_count in SimulationConfig, between 1 and 9 inclusive), each with a range of [0.0, 1.0] and a step size of 0.01
2. THE Distribution_Section SHALL display the current numeric weight value to two decimal places alongside each slider, labelled with the color index (0-based)
3. WHEN a slider value changes, THE SimulationConfig SHALL update the corresponding Color_Weight for that color index within the same frame, redistributing the difference evenly among all other colors so that the total remains 1.0
4. WHEN color_count increases, THE Distribution_Section SHALL add sliders for the new colors with weight redistributed evenly from existing colors
5. WHEN color_count decreases, THE Distribution_Section SHALL remove sliders for colors beyond the new color_count and redistribute their weight among remaining colors

### Requirement 4: Weighted Color Assignment on Spawn

**User Story:** As a user, I want newly spawned particles to respect the configured color weights, so that the distribution is applied from the moment particles appear.

#### Acceptance Criteria

1. WHEN a new particle is spawned via the add_components observer, THE BodyPlugin SHALL assign a color by sampling from the Weighted_Distribution defined by Color_Weights (which always sum to 1.0), where the probability of selecting color i equals Color_Weights[i]
2. WHEN all Color_Weights are equal (each = 1/color_count), THE Weighted_Distribution SHALL produce a uniform distribution across all active colors
3. IF a color's weight is zero, THEN THE BodyPlugin SHALL never assign that zero-weighted color to a spawned particle

### Requirement 5: Weighted Color Assignment on Palette Rebuild

**User Story:** As a user, I want existing particles to be recolored according to weights when the palette is rebuilt (e.g., after changing color count), so that the distribution is consistently applied.

#### Acceptance Criteria

1. WHEN a RebuildPalette message is handled, THE handle_palette_rebuild system SHALL recolor all existing particles so that the resulting color distribution matches the current Color_Weights
2. WHEN all Color_Weights are equal (each = 1/color_count), THE handle_palette_rebuild system SHALL produce a uniform distribution across all active colors
3. WHEN a particle is recolored, THE system SHALL update both its PointColor component and its MeshMaterial3d component to reference the Palette material handle for the new color index

### Requirement 6: Live Redistribution on Weight Change

**User Story:** As a user, I want existing particles to be redistributed immediately when I change a weight slider, so that I can see the effect of my adjustments in real time.

#### Acceptance Criteria

1. WHEN any Color_Weight slider value changes, THE Settings_Panel SHALL emit a RedistributeColors message
2. WHEN a RedistributeColors message is received, THE system SHALL reassign colors to all existing particles by sampling each particle's color independently using the Weighted_Distribution (Color_Weights which sum to 1.0), and SHALL preserve each particle's position and velocity unchanged
3. WHEN a RedistributeColors message is received, THE system SHALL update each particle's PointColor component and MeshMaterial3d component to reference the Palette material handle corresponding to the newly assigned color index

### Requirement 7: Normalization Invariant

**User Story:** As a user, I want color weights to always sum to 1.0, so that they directly represent probabilities and the system remains in a consistent state.

#### Acceptance Criteria

1. THE Color_Weights list SHALL maintain the invariant that the sum of all entries equals 1.0 at all times (within floating-point precision, epsilon of 1e-10)
2. WHEN a single Color_Weight is adjusted via slider, THE system SHALL redistribute the difference evenly among all other Color_Weights so the sum remains 1.0
3. WHEN color_count changes, THE system SHALL recompute Color_Weights so that the sum of the new list equals 1.0
4. THE Weighted_Distribution SHALL use Color_Weights directly as per-color probabilities without additional normalization (since they always sum to 1.0)
