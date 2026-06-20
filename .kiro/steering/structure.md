# Project Structure

```
src/
├── main.rs          # App entry point, plugin registration, entity spawning
├── config.rs        # Global constants (BODIES count, COLORS count)
├── camera.rs        # Orbital camera plugin (generic over Position trait)
├── debug.rs         # FPS overlay, debug UI text, timing display
├── palette.rs       # Material/color palette resource
├── positioners.rs   # Initial particle position generators (Uniform, Sphere, BigBang, etc.)
├── traits.rs        # Shared traits (NextVariant, PrevVariant, RandVec3, Fullscreen, FpsOverlay)
└── physics/
    ├── mod.rs       # Module re-exports
    ├── bodies.rs    # ECS components (PointBody, PointPosition, PointVelocity, PointColor)
    ├── forces.rs    # Force matrix types and keyboard controls
    ├── islands.rs   # Spatial partitioning grid for neighbor lookups
    └── physics.rs   # Physics pipeline (snapshot → compute forces → apply → translate)
```

## Architecture Patterns

- **Plugin-per-concern:** Each module exposes a Bevy `Plugin` struct (CameraPlugin, DebugPlugin, ParticlePhysicsPlugin, etc.)
- **Generic camera:** `CameraPlugin<C>` is generic over any component implementing the `Position` trait
- **Trait-based cycling:** `NextVariant` / `PrevVariant` traits for cycling enum states with keyboard
- **Snapshot pattern:** Physics reads a frozen `BodySnapshots` resource (copied from ECS) for parallel force computation, then applies results back
- **Island grid:** 3D spatial hash for O(1) neighbor cell lookup; pre-computed neighbor index tables

## Conventions

- Constants live in `config.rs` or as module-level `const` in the relevant file
- Keyboard bindings are registered via Bevy's `run_if(input_just_pressed(...))` conditions
- Commented-out code is retained for reference (prior UI/clipboard features) — don't delete it without asking
- ECS queries use Bevy's `Single<>` for camera, `Query<>` for particles
- Resources are initialized in `Startup` systems or via `init_resource`
