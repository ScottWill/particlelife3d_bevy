# Implementation Plan: Color Distribution Panel

## Overview

Add a `color_weights: Vec<f64>` field to `SimulationConfig` with weight management methods, a "Distribution" UI section with per-color sliders, a `RedistributeColors` message, and weighted sampling in spawn/recolor paths. Pure weight logic is validated with proptest property-based tests.

## Tasks

- [x] 1. Extend SimulationConfig with color_weights field and methods
  - [x] 1.1 Add `color_weights: Vec<f64>` field to `SimulationConfig` and update `Default` impl
    - Add the field to the struct definition in `src/settings_panel.rs`
    - Initialize in `Default::default()` as `vec![1.0 / 5.0; 5]` (matching default `color_count = 5`)
    - _Requirements: 1.1, 1.4_

  - [x] 1.2 Implement `resize_weights(&mut self)` method on `SimulationConfig`
    - Growing: scale existing weights by `old_count / new_count`, set new entries to `1.0 / new_count`
    - Shrinking: remove excess entries, normalize remaining so sum = 1.0
    - Final normalization pass to guarantee sum = 1.0
    - _Requirements: 1.2, 1.3, 7.1, 7.3_

  - [x] 1.3 Implement `set_weight(&mut self, index: usize, new_value: f64)` method
    - Compute diff from old value, redistribute evenly among other weights
    - Clamp all to [0.0, 1.0], re-normalize if needed
    - Single-color case: early return (weight locked at 1.0)
    - _Requirements: 3.3, 7.1, 7.2_

  - [x] 1.4 Extend `clamp_all()` to clamp and normalize `color_weights`
    - Clamp each entry to [0.0, 1.0]
    - Normalize so entries sum to 1.0
    - Update existing `config_clamping_invariant` proptest to include `color_weights` field
    - _Requirements: 1.5_

- [x] 2. Property-based tests for weight invariants
  - [x] 2.1 Write property test for resize_weights invariant
    - **Property 1: Weight vector invariant after resize**
    - Generate random valid weight vectors (sum 1.0, length 1–9) and random new color_count in [1, 9]
    - Call `resize_weights()`, assert length equals new color_count, sum ≈ 1.0, all entries in [0.0, 1.0]
    - **Validates: Requirements 1.1, 1.2, 1.3, 1.4, 7.1, 7.3**

  - [x] 2.2 Write property test for set_weight sum preservation
    - **Property 2: Slider adjustment preserves sum**
    - Generate random valid weight vectors (sum 1.0, length 2–9), random index, random new value in [0.0, 1.0]
    - Call `set_weight(index, new_value)`, assert sum ≈ 1.0 and all entries in [0.0, 1.0]
    - **Validates: Requirements 3.3, 7.1, 7.2**

  - [x] 2.3 Write property test for clamp_all normalization
    - **Property 3: Clamp and normalize correctness**
    - Generate random vectors (length 1–9, entries in [-1.0, 2.0])
    - Call `clamp_all()`, assert all entries in [0.0, 1.0] and sum ≈ 1.0
    - **Validates: Requirements 1.5**

  - [x] 2.4 Write property test for zero-weight sampling exclusion
    - **Property 4: Weighted sampling respects zero weights**
    - Generate valid weight vectors (sum 1.0, length 2–9) with at least one zero entry
    - Construct `WeightedIndex`, sample 1000 times, assert zero-weight index never produced
    - **Validates: Requirements 4.1, 4.3**

- [x] 3. Checkpoint - Verify weight logic
  - Ensure all tests pass, ask the user if questions arise.

- [x] 4. Add RedistributeColors message and handle_redistribute_colors system
  - [x] 4.1 Define `RedistributeColors` message and register it
    - Add `#[derive(Message)] pub struct RedistributeColors;` in `src/settings_panel.rs`
    - Register via `app.add_message::<RedistributeColors>()` in `SettingsPanelPlugin::build`
    - Add `handle_redistribute_colors` system running on `on_message::<RedistributeColors>`
    - _Requirements: 6.1, 6.2, 6.3_

  - [x] 4.2 Implement `handle_redistribute_colors` system
    - Read `Res<SimulationConfig>` for `color_weights`
    - Build `WeightedIndex` from `config.color_weights`
    - Query all `(&mut MeshMaterial3d<StandardMaterial>, &mut PointColor), With<PointBody>`
    - Resample each particle's color, update `PointColor` and `MeshMaterial3d`
    - Do NOT query position/velocity as mutable (preserving them unchanged)
    - _Requirements: 6.2, 6.3_

- [x] 5. Add Distribution UI section with weight sliders
  - [x] 5.1 Add "Distribution" CollapsingHeader to `render_panel`
    - Place between "Simulation" and "Appearance" collapsing sections
    - Default open
    - Display one slider per color index (0 through color_count - 1)
    - Slider range [0.0, 1.0], step 0.01, showing value to 2 decimal places
    - On slider change: call `config.set_weight(i, new_val)` and emit `RedistributeColors`
    - Add `MessageWriter<RedistributeColors>` parameter to `render_panel`
    - Single-color case: show slider but it remains at 1.0 (set_weight handles this)
    - _Requirements: 2.1, 2.2, 2.3, 2.4, 2.5, 3.1, 3.2, 3.3, 3.4, 3.5, 6.1_

- [x] 6. Modify spawn and recolor paths to use WeightedIndex
  - [x] 6.1 Modify `add_components` observer in `src/physics/bodies.rs` to use weighted sampling
    - Add `Res<SimulationConfig>` parameter to the observer
    - Build `WeightedIndex` from `config.color_weights`
    - Replace `random_range(0..palette.size())` with `dist.sample(&mut rng)`
    - _Requirements: 4.1, 4.2, 4.3_

  - [x] 6.2 Modify `handle_palette_rebuild` to use WeightedIndex for recoloring
    - Call `config.resize_weights()` (via `ResMut<SimulationConfig>`) when color_count changes
    - Build `WeightedIndex` from the updated `color_weights`
    - Replace `rng.random_range(0..color_count)` with `dist.sample(&mut rng)`
    - _Requirements: 5.1, 5.2, 5.3_

- [x] 7. Final checkpoint - Full integration verification
  - Ensure all tests pass, ask the user if questions arise.

## Notes

- Tasks marked with `*` are optional and can be skipped for faster MVP
- Each task references specific requirements for traceability
- Checkpoints ensure incremental validation
- Property tests validate universal correctness properties from the design document
- Unit tests validate specific examples and edge cases
- `rand::distr::WeightedIndex` is used from the `rand 0.10` crate already in dependencies
- The existing `config_clamping_invariant` proptest must be updated in task 1.4 since `SimulationConfig` gains the `color_weights` field

## Task Dependency Graph

```json
{
  "waves": [
    { "id": 0, "tasks": ["1.1"] },
    { "id": 1, "tasks": ["1.2", "1.3"] },
    { "id": 2, "tasks": ["1.4"] },
    { "id": 3, "tasks": ["2.1", "2.2", "2.3", "2.4", "4.1"] },
    { "id": 4, "tasks": ["4.2", "5.1"] },
    { "id": 5, "tasks": ["6.1", "6.2"] }
  ]
}
```
