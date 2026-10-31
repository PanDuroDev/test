# Rasm Studio

**Rasm Studio** is the working title for a local-first drawing and 2D animation app with a lightweight 2.5D stage. It is intended to make drawing, frame-by-frame animation, and simple depth-aware scenes feel like one workflow.

> Early foundation stage. This repository does not yet contain a usable drawing application.

## Product direction

- Draw with pressure- and tilt-aware brushes on a large canvas.
- Animate with cels, held exposures, onion skin, and a simple timeline.
- Add keyframed transforms after the frame-by-frame workflow is stable.
- Place 2D artwork on real 3D surfaces, add simple 3D objects and lights, and composite the result with 2D layers.
- Keep projects local and recoverable by default.

The goal is not to reproduce every feature of a full video editor. The focus is a responsive drawing surface and a clear animation workflow.

## Technical direction

| Area | Initial choice |
| --- | --- |
| Core engine | Rust |
| Android application/UI | Kotlin + Compose |
| Rust/Kotlin boundary | UniFFI, subject to a small integration spike |
| GPU abstraction | wgpu |
| 2D math | glam, kurbo |
| Vector path tessellation | lyon |
| Brush engine | Evaluate Hokusai/libmypaint compatibility before adopting |
| 3D stage | A focused wgpu scene pass; no full game engine in the first milestone |

The core crate is platform-independent. There is no remote backend requirement for the first product: documents, rendering, and autosave are local.

## Repository layout

- crates/engine-core: platform-independent math and timeline contracts.
- docs/architecture/FOUNDATION.md: product architecture, math, rendering, and roadmap.
- .github/workflows/ci.yml: formatting, lint, and cross-platform tests.

## Development

Install a stable Rust toolchain with rustfmt and clippy, then run:

~~~sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
~~~

## Current status

The first milestone establishes exact timeline time, reversible canvas/screen transforms, adaptive pointer filtering, and CI. The actual renderer and app shell are deliberately deferred until these contracts are tested.
