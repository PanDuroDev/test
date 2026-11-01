# Rasm Studio Desktop Prototype

This is the Windows-first desktop integration prototype. It is not the final product UI.

## What it currently demonstrates

- Creates a native desktop window with winit 0.30.13 and presents a wgpu 30 surface.
- Draws a document-backed canvas using the existing Rust document, ink, and stroke-mesh crates.
- Stores completed strokes in `Document` layers and supports command-based Undo/Redo.
- Stores a validated RGBA colour on each `Document::Stroke`; renderer output reads the colour from the document, not a UI-side map.
- Draws with the left mouse button or the Windows pen input hook. On Windows, the hook reads coalesced `WM_POINTERDOWN`, `WM_POINTERUPDATE`, and `WM_POINTERUP` history, including pressure and tilt when the device reports those fields.
- Provides a prototype layer panel with Add, Select, Visibility, and Delete actions. The list scrolls when the number of layers exceeds the available height. Panel interaction currently uses the mouse.
- Supports viewport fit, wheel zoom around the cursor, middle-button pan, and `Ctrl+0` reset.
- Provides quick brush controls: `[` and `]` change brush size, `E` selects the eraser, and `1`–`4` select one of four palette colours.
- Opens and saves RASM projects through native dialogs: `Ctrl+O`, `Ctrl+S`, and `Ctrl+Shift+S`. Dialogs are parented to the app window.
- Uses `rasm-engine-storage::load` and `save_atomic`; the current path is shown in the title and file errors are shown in the status strip.
- Reconfigures the surface after resize and recreates it after a lost surface.

## Project format and compatibility

The outer binary container remains RASM file version 1. Its JSON payload is now schema version 2 and includes a validated RGBA value per stroke. The decoder continues to accept schema version 1 files without that field, assigning the same default cyan used by the earlier prototype. New saves always use schema version 2.

## Run

From the repository root:

```sh
cargo run -p rasm-desktop-prototype
```

On Windows, use a normal desktop session with a working D3D12 or Vulkan adapter.

## Current limitations

- This prototype has no autosave, journal, crash recovery, recent-file list, or unsaved-change prompt.
- The eraser currently draws using the canvas background colour. It is not an alpha-compositing eraser.
- The renderer does not yet implement full layer-opacity compositing. Layer reordering, renaming, groups, and opacity controls are not exposed by this panel.
- This is not a production renderer or final design system. It has no animation timeline, onion skinning, playback, or 2.5D stage.

## Validation limits

CI checks Rust 1.87 compatibility for the desktop dependency graph, formatting, Clippy, Rust tests on Linux/Windows/macOS, Android ARM64 FFI integration, and the Windows prototype release artifact. These checks validate compilation and tests, not whether a physical native file dialog, pen device, or GPU works correctly in a user's desktop session.
