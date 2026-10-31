# Rasm Studio

**Rasm Studio** is the working title for a local-first **PC desktop** drawing and 2D animation application with a focused 2.5D stage. The product target is desktop only, starting with Windows. Mobile apps are not part of the product scope.

## Product goal

Build a responsive drawing workspace first, then add frame-by-frame animation and a lightweight 2.5D stage:

- **Canvas:** pressure-aware brushes, eraser, colors, layers, selection, transforms, and reliable undo/redo.
- **Timeline:** cels, held exposures, onion skin, and frame playback.
- **Stage:** combine 2D artwork with a limited set of real 3D objects, surfaces, and lights.
- **Local-first:** project files, rendering, recovery, and autosave remain local. No account or cloud backend is required for the first release.

This is not a general video editor or a full 3D modelling package.

## Current status

The repository contains a Rust workspace with timeline math, reversible canvas transforms, pointer smoothing, pressure-aware brush sampling, connected stroke mesh geometry, and bounded CPU-side tile coverage/dirty tracking. It also contains an experimental Tileink desktop adapter and a narrow UniFFI boundary.

There is **no usable product desktop application, production renderer, document model, layer system, save/recovery flow, or animation editor yet**. The first integration prototype now exists under `apps/desktop-prototype`: it creates a winit window and wgpu surface, renders a test `StrokeMesh`, and hooks Windows pen history. [PR #18](https://github.com/PanDuroDev/test/pull/18) passed CI on Linux, Windows, and macOS, including formatting and Clippy. This validates compilation and automated tests only; physical Windows GPU presentation and a real pen device remain unverified. The workflow now also builds a Windows Release executable and uploads it as a downloadable Actions artifact.

The Android FFI work is preserved as an independent integration experiment:
- PR #14 added the Rust/Kotlin UniFFI boundary and Android ARM64 build/binding generation.
- PR #15 is an open Android emulator smoke-test harness. It is not the product UI and does not change the PC-only product target.

## Technical direction

| Area | Current direction |
| --- | --- |
| Product platform | PC desktop only; Windows first |
| Core engine | Rust, kept platform-independent where practical |
| Desktop window/input/UI | `winit` 0.30.13 prototype in progress; final UI framework not selected |
| GPU abstraction | wgpu candidate; no production renderer selected |
| 2D math | glam, kurbo |
| Vector path tessellation | lyon candidate |
| Input-to-brush sampling | rasm-engine-ink |
| Tile tracking | rasm-engine-tiles |
| Experimental desktop renderer | Tileink 0.1.2 behind rasm-renderer-tileink; not production-approved |
| 3D stage | A focused wgpu scene pass; no full game engine in the first milestone |

The desktop prototype must evaluate pen pressure/tilt and pointer history, low-latency input, wgpu surface lifecycle, Windows integration, UI composition, and thread separation before a UI framework or renderer is committed.

The existing Android/Kotlin UniFFI smoke host remains isolated test infrastructure. It is not a mobile application target, and the desktop product must not depend on it.

## Repository layout

- `crates/engine-core`: platform-independent math and timeline contracts.
- `crates/engine-ink`: validated pointer/pen samples, pressure-aware brush dabs, and renderer-independent connected stroke mesh geometry.
- `crates/engine-tiles`: document-pixel tile coverage and bounded transactional dirty-tile tracking.
- `crates/engine-ffi`: narrow UniFFI DTO boundary retained for integration testing.
- `android/ffi-smoke`: optional Android host for testing the existing Kotlin/Rust boundary; not product UI.
- `crates/renderer-tileink`: experimental Tileink Canvas adapter and offscreen PNG example.
- `apps/desktop-prototype`: early winit + wgpu window/surface and Windows pen-input prototype; not product UI.
- `docs/architecture/FOUNDATION.md`: product architecture, math, rendering, and roadmap.
- `docs/PROJECT_PLAN.md`: maintained implementation log and contributor/AI handoff instructions.
- `docs/architecture/RENDERER_SPIKE.md`: Tileink evaluation and current limitations.
- `.github/workflows/ci.yml`: formatting, lint, cross-platform Rust tests, and the separate Android FFI build/binding job.

## Development

The workspace MSRV is Rust 1.87, matching the current wgpu 30 requirement.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

To run the first desktop window/surface prototype (requires a working desktop GPU adapter):

```sh
cargo run -p rasm-desktop-prototype
```

It opens a dark window with a deterministic cyan test stroke. Use the left mouse button or a Windows pen to add strokes. The Windows pen hook reads coalesced pointer history, pressure, and tilt where provided. This is a development prototype, not the final application.

To render a sample stroke offscreen on a desktop machine with a usable WGPU adapter:

```sh
cargo run -p rasm-renderer-tileink --example render_stroke --release
```

The example writes `target/renderer-spike.png`. GPU initialization is deliberately not part of automated unit tests, so CI does not assume every runner has a usable graphics adapter.

## Development workflow

Every functional or architectural change follows the repository rules in `docs/PROJECT_PLAN.md`: Issue → branch from clean `main` → focused implementation → tests → PR → CI → update the project log → merge only after required checks pass.

Pull requests are validated by GitHub Actions on Linux, Windows, and macOS. The Windows desktop prototype must also be exercised on a real Windows environment before platform or GPU support is claimed.
