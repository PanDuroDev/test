# Rasm Studio

**Rasm Studio** is the working title for a local-first drawing and 2D animation app with a lightweight 2.5D stage. It is intended to make drawing, frame-by-frame animation, and simple depth-aware scenes feel like one workflow.

> Early foundation stage. This repository contains the mathematical and brush-input foundations plus an experimental desktop renderer adapter. An Android ARM64 FFI proof is under development, but there is not yet a runnable Android application.

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
| Input-to-brush sampling | rasm-engine-ink |
| Experimental desktop renderer | Tileink 0.1.2 behind rasm-renderer-tileink |
| 3D stage | A focused wgpu scene pass; no full game engine in the first milestone |

The core crate is platform-independent. There is no remote backend requirement for the first product: documents, rendering, and autosave are local.

Tileink is an experimental desktop adapter, not the selected Android renderer. Its published platform matrix does not claim Android/iOS support, and its Canvas/Renderer types are not Send/Sync. Keep it isolated behind the adapter until a real Android surface integration has been tested.

## Repository layout

- `crates/engine-core`: platform-independent math and timeline contracts.
- `crates/engine-ink`: validated stylus samples, pressure-aware brush dabs, and renderer-independent connected stroke mesh geometry.
- `crates/engine-tiles`: document-pixel tile coverage and bounded transactional dirty-tile tracking, independent of the renderer.\n- `crates/engine-ffi`: narrow UniFFI DTO boundary; Android ARM64 cross-build and Kotlin binding generation are being added in the current spike.
- `crates/renderer-tileink`: experimental Tileink Canvas adapter and offscreen PNG example.
- `docs/architecture/FOUNDATION.md`: product architecture, math, rendering, and roadmap.
- `docs/PROJECT_PLAN.md`: maintained implementation log and contributor/AI handoff instructions.
- `docs/architecture/RENDERER_SPIKE.md`: Tileink evaluation and current platform limitations.
- `.github/workflows/ci.yml`: formatting, lint, and cross-platform tests.

## Development

The workspace MSRV is Rust 1.87, matching the current wgpu 30 requirement.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

To render a sample stroke offscreen on a desktop machine with a usable WGPU adapter:

```sh
cargo run -p rasm-renderer-tileink --example render_stroke --release
```

The example writes `target/renderer-spike.png`. GPU initialization is deliberately not part of automated unit tests, so CI does not assume every runner has a usable graphics adapter.

## Current status

The repository now has exact timeline time, reversible canvas/screen transforms, adaptive pointer smoothing, pressure-aware dab sampling, connected stroke mesh geometry, bounded CPU-side tile coverage and dirty tracking, and an experimental GPU canvas adapter. A minimal UniFFI boundary is being proven for Android ARM64. There is still no Android APK, Compose app shell, device-tested stylus input, Android Surface renderer, document model, GPU tile cache, or animation editor.

Pull requests are validated by GitHub Actions on Linux, Windows, and macOS.
