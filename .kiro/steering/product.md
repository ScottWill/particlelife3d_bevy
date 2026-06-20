# Product

Particle Life 3D — a real-time 3D particle simulation built with Bevy. Particles of different colors interact via a configurable force matrix, producing emergent behavior (flocking, chains, clusters, etc.). The simulation runs in a toroidal (wrapping) unit cube scaled to world space. It's a creative/exploratory tool, not a game — there's no win condition, just keyboard-driven controls for tweaking forces and observing emergent patterns.

## Key Behaviors

- 50,000 particles by default, 5 color types
- Force matrices (Chains, Checkered, Random, Snakes, etc.) define inter-color attraction/repulsion
- Spatial partitioning via an "island grid" for O(n) neighbor lookups
- Orbital camera with WASD/QE panning, mouse orbit, scroll zoom
- Physics can be paused (Enter), stepped (Space), or reset (Cmd+R)
- Debug overlay (Escape toggle) shows FPS, force matrix, and timing info
