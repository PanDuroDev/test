# Rasm Studio Desktop Prototype

This is the Windows-first desktop integration prototype. It is not the final product UI.

## What it currently demonstrates

- Creates a native desktop window with winit 0.30.13 and presents a wgpu 30 surface.
- Draws a document-backed canvas using the existing Rust document, ink, and stroke-mesh crates.
- Stores completed strokes in `Document` layers and supports command-based Undo/Redo.
- Draws with the left mouse button or the Windows pen input hook. On Windows, the hook reads coalesced `WM_POINTERDOWN`, `WM_POINTERUPDATE`, and `WM_POINTERUP` history, including pressure and tilt when the device reports those fields.
- Provides a prototype layer panel with Add, Select, Visibility, and Delete actions. The list scrolls when the number of layers exceeds the available height. Panel interaction currently uses the mouse.
- Supports viewport fit, wheel zoom around the cursor, middle-button pan, and `Ctrl+0` reset.
- Provides quick brush controls: `[` and `]` change brush size, `E` selects the eraser, and `1`–`4` select one of four palette colours.
- Reconfigures the surface after resize and recreates it after a lost surface.

The initial cyan line is a deterministic test stroke. It is not loaded from a project file. Pen pressure and tilt are captured only when Windows reports the corresponding data masks. CI compilation does not validate a physical pen device.

## Run

From the repository root:

```sh
cargo run -p rasm-desktop-prototype
```

On Windows, use a normal desktop session with a working D3D12 or Vulkan adapter.

## Current limitations

- The prototype does not yet connect native Open/Save dialogs to the UI. The separate `rasm-engine-storage` crate already implements the RASM v1 codec and atomic filesystem `save_atomic`/`load` functions; the desktop integration is tracked by [Issue #33](https://github.com/PanDuroDev/test/issues/33).
- Per-stroke colours are currently held in an in-memory map in the prototype. They are not part of `Document` or RASM v1, so saving and loading a drawing would not preserve its individual stroke colours. Durable colour storage must be included in the file workflow before it is considered complete.
- The eraser currently draws using the canvas background colour. It is not an alpha-compositing eraser.
- The renderer does not yet implement full layer-opacity compositing. Layer reordering, renaming, groups, and opacity controls are not exposed by this panel.
- This is not a production renderer or final design system. It has no animation timeline, onion skinning, playback, 2.5D stage, autosave, or crash-recovery workflow.

## Validation limits

GitHub Actions validates formatting, linting, Rust tests on Linux/Windows/macOS, the Android ARM64 FFI integration build, and the Windows prototype release artifact. The latest layer-panel change passed all six jobs in [CI run #41334018329](https://github.com/PanDuroDev/test/actions/runs/41334018329).

CI does not prove physical pen pressure/tilt behavior, latency, memory use, or GPU presentation on a real Windows machine. Those require a dedicated hardware smoke test and recorded device/backend details.
