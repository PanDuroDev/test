# Rasm Studio Desktop Prototype

This is an integration prototype for Issue #17. It is intentionally not the product UI.

## What it proves

- Creates a native desktop window with winit 0.30.
- Creates and presents a wgpu 30 surface.
- Converts the existing `rasm-engine-ink` stroke mesh into GPU vertex/index buffers and displays it.
- On Windows, observes `WM_POINTERDOWN`, `WM_POINTERUPDATE`, and `WM_POINTERUP` before winit dispatches them, then reads coalesced pen samples using `GetPointerPenInfoHistory`.
- Uses winit mouse events as a fallback input path when no pen pointer is active.
- Reconfigures the surface after resize and recreates it after a lost surface.

The initial cyan line is a deterministic test stroke from the engine, not a saved document. Pen pressure and tilt are captured only when Windows reports the corresponding pen-data masks. No real pen device has been validated by CI.

## Run

From the repository root:

```sh
cargo run -p rasm-desktop-prototype
```

On Windows, use a normal desktop session with a working D3D12 or Vulkan adapter. The prototype currently writes pen diagnostics to stdout. It does not include the final tool panels, document model, file format, or animation timeline.

## Validation limits

GitHub Actions can compile and test the crate across supported CI targets, but it does not prove a physical pen device or a real Windows GPU presentation path. Those require running this executable on a Windows machine and recording the hardware/backend used.
