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

The repository contains a Rust workspace with timeline math, reversible canvas transforms, pointer smoothing, pressure-aware brush sampling, connected stroke mesh geometry, bounded CPU-side tile coverage/dirty tracking, a document/layer/stroke model with undo/redo, validated snapshots, a versioned RASM v1 byte codec, and filesystem-backed save/load with atomic replacement. It also contains an experimental Tileink desktop adapter and a narrow UniFFI boundary.

There is **no complete product drawing application, production renderer, crash recovery flow, or animation editor yet**. The first desktop integration prototype was merged in [PR #18](https://github.com/PanDuroDev/test/pull/18). The user confirmed the Windows smoke test for the window, visible test stroke, mouse drawing, and resize/restore behavior. A dedicated pen pressure/tilt hardware test and latency/resource benchmark remain unverified. The document foundation is in `main`: [PR #20](https://github.com/PanDuroDev/test/pull/20) added the Document/Layer/Stroke model and bounded command-based undo/redo; [PR #22](https://github.com/PanDuroDev/test/pull/22) added validated snapshots and the versioned RASM v1 byte codec. Filesystem-backed `save_atomic` and `load` are implemented in [PR #26](https://github.com/PanDuroDev/test/pull/26), with same-directory temporary files and atomic replacement. CI run [#41258015343](https://github.com/PanDuroDev/test/actions/runs/41258015343) passed all six jobs, including Windows, Linux, macOS, Format, Clippy, Android FFI, and the Windows prototype artifact. This does not include autosave, journaling, or crash recovery. Issue [#27](https://github.com/PanDuroDev/test/issues/27) is now implementing a document-backed desktop canvas: completed strokes are committed to `Document`, the active stroke remains a preview, and mouse/pen coordinates map through an aspect-preserving 1920×1080 viewport with undo/redo shortcuts. Implementation was merged in [PR #28](https://github.com/PanDuroDev/test/pull/28) (merge commit `5f39d46c082aac621d91276f831c6d7e07e36fe8`), closing Issue #27. CI runs [#41260460992](https://github.com/PanDuroDev/test/actions/runs/41260460992) and [#41260977695](https://github.com/PanDuroDev/test/actions/runs/41260977695) passed all six jobs: Linux, Windows, macOS, Format/Clippy, Android FFI, and the Windows prototype artifact build. The Windows artifact is `rasm-desktop-prototype-windows-x64` (artifact ID `13065571623`). This does not verify dedicated pen hardware pressure/tilt or latency/resource benchmarks. Canvas view controls ([Issue #29](https://github.com/PanDuroDev/test/issues/29)) add wheel zoom around the cursor (clamped to 0.25×–8×), middle-button pan, and `Ctrl+0` reset; the chosen view survives window resizes. Brush tools ([Issue #31](https://github.com/PanDuroDev/test/issues/31), [PR #35](https://github.com/PanDuroDev/test/pull/35)) add brush size (`[` and `]`, 1–128 px), four palette colours (`1`–`4`), and an eraser (`E`); the active tool and size are shown in the window title. Per-stroke RGBA colour is now part of `Document::Stroke` on the file-workflow branch, rather than a transient renderer map. PR [#37](https://github.com/PanDuroDev/test/pull/37) adds RASM JSON schema v2 inside the existing binary RASM v1 container; the decoder continues to accept schema v1 files and assigns their strokes the previous default cyan. Layer management is now in main via [PR #36](https://github.com/PanDuroDev/test/pull/36), closing Issue #32: the WGPU prototype has a compact scrollable layer panel for adding, selecting, hiding, and deleting layers, and routes new strokes to the selected layer with undo/redo support. CI [#41334018329](https://github.com/PanDuroDev/test/actions/runs/41334018329) passed all six jobs, including the Windows x64 prototype artifact. The panel remains prototype UI, not the final application interface. Project file actions are under review in [PR #37](https://github.com/PanDuroDev/test/pull/37): `Ctrl+O` opens, `Ctrl+S` saves, and `Ctrl+Shift+S` saves as through native dialogs. It uses the existing atomic storage API and displays file errors in a visible status strip. The branch also preserves per-stroke colours in project files.

The Android FFI work is preserved as an independent integration experiment:
- PR #14 added the Rust/Kotlin UniFFI boundary and Android ARM64 build/binding generation.
- PR #15 is an open Android emulator smoke-test harness. It is not the product UI and does not change the PC-only product target.

## Technical direction

| Area | Current direction |
| --- | --- |
| Product platform | PC desktop only; Windows first |
| Core engine | Rust, kept platform-independent where practical |
| Desktop window/input/UI | `winit` 0.30.13 Windows prototype available; final UI framework not selected |
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
- `crates/engine-document`: document/layer/stroke model, validated snapshot conversion, and reversible command history.
- `crates/engine-storage`: versioned RASM v1 byte codec, corruption checks, and filesystem-backed atomic save/load.
- `crates/engine-ffi`: narrow UniFFI DTO boundary retained for integration testing.
- `android/ffi-smoke`: optional Android host for testing the existing Kotlin/Rust boundary; not product UI.
- `crates/renderer-tileink`: experimental Tileink Canvas adapter and offscreen PNG example.
- `apps/desktop-prototype`: winit + wgpu Windows-first desktop prototype; Issue #27 connects it to `Document`/`History`, maps input into a fixed 1920×1080 document viewport, and adds Ctrl+Z / Ctrl+Y / Ctrl+Shift+Z. It is still a prototype, not the final product UI.
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

It opens the Windows-first WGPU drawing prototype with a deterministic test stroke. Use the left mouse button or a Windows pen to draw; completed strokes are stored in the document model and can be undone with Ctrl+Z or redone with Ctrl+Y / Ctrl+Shift+Z. The layer panel supports adding, selecting, hiding, and deleting layers; wheel over the panel scrolls its list, while wheel over the canvas zooms around the cursor. Use the middle mouse button to pan and Ctrl+0 to reset the viewport. Brush size is controlled with [ and ], E selects the eraser, and 1–4 select the quick palette. The Windows pen hook reads coalesced pointer history, pressure, and tilt where provided. On the file-workflow branch, `Ctrl+O` opens a RASM project, `Ctrl+S` saves, and `Ctrl+Shift+S` saves as. The path appears in the window title; save/load errors appear in the status strip. This remains an integration prototype, not the final application; autosave, crash recovery, production compositing, and animation remain future work.

To render a sample stroke offscreen on a desktop machine with a usable WGPU adapter:

```sh
cargo run -p rasm-renderer-tileink --example render_stroke --release
```

The example writes `target/renderer-spike.png`. GPU initialization is deliberately not part of automated unit tests, so CI does not assume every runner has a usable graphics adapter.

## Development workflow

Every functional or architectural change follows the repository rules in `docs/PROJECT_PLAN.md`: Issue → branch from clean `main` → focused implementation → tests → PR → CI → update the project log → merge only after required checks pass.

Pull requests are validated by GitHub Actions on Linux, Windows, and macOS. The Windows desktop prototype must also be exercised on a real Windows environment before platform or GPU support is claimed.
