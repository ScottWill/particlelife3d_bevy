# Tech Stack

- **Language:** Rust (edition 2024)
- **Engine:** Bevy 0.19 (ECS game engine)
- **Parallelism:** Rayon for data-parallel force computation
- **RNG:** rand 0.10

## Build & Run

```sh
# Debug build + run
cargo run

# Release build (optimized, single codegen unit)
cargo run --release

# Check compilation without running
cargo check

# Run clippy lints
cargo clippy
```

## Bevy Version Notes

This project uses Bevy 0.19 with selective features. Key APIs:
- `App::add_message` / `MessageWriter` / `on_message` for event-like messaging
- `children![]` macro for UI hierarchy
- `Single<>` query for guaranteed-single entities
- `#[require(...)]` attribute for component bundles
- `Query::contiguous_iter_mut()` for cache-friendly iteration
- `On<Add, C>` observers for reactive component initialization
- `DVec3` (f64) for physics precision, `Vec3` (f32) for rendering

## Performance Considerations

- Physics uses `f64` (DVec3) for precision; rendering uses `f32` (Vec3)
- Force computation is parallelized with Rayon (`par_iter`)
- Spatial grid (islands) avoids O(n²) all-pairs interaction
- Release profile uses `codegen-units = 1` for maximum optimization
