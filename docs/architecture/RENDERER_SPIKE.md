# Tileink Renderer Spike

**Status:** Experimental desktop adapter, not a production renderer decision  
**Dependency:** tileink 0.1.2, peniko 0.6.1, kurbo 0.13.1, WGPU 30  
**License:** Tileink offers MIT or Apache-2.0

## Why evaluate Tileink?

Tileink is a Rust/WGPU renderer with an immediate Canvas API and a transactional RetainedScene API. Its documented pipeline includes coarse 16x16 tile binning, fine compute rasterization, analytic SDF shapes, vector paths, images, layers, masks, filters, backdrops, SVG, and incremental damage tracking. It exposes statistics such as dirty tile counts, uploaded bytes, rebuilt batches, and full-redraw reasons.

This is unusually close to the rendering workload we need. It can save us from building a general-purpose vector/tile renderer from scratch.

## Why it is not the Android renderer yet

The upstream project labels Windows 10/11 as its primary path, macOS as supported, Linux desktop as provisional, and iOS/Android as not claimed. WGPU itself may run on mobile, but that does not establish that Tileink's specific compute shaders, surface usage requirements, and renderer lifecycle work correctly on mobile devices.

Its Canvas, Renderer, and RetainedScene types are also marked !Send and !Sync. They must stay on the thread where they are created. This is compatible with a dedicated render thread if the renderer is created and used there, but it rules out casually passing these objects through shared application state or arbitrary task pools.

Therefore:
- Tileink is isolated behind rasm-renderer-tileink.
- The core stores normalized input samples and generates BrushDab values without Tileink types.
- Android/iOS support remains unproven.
- Do not use Tileink types in document serialization, Kotlin FFI, or engine-core public types.
- Do not claim production performance based on its own benchmark pages.

## Current adapter

rasm-engine-ink validates normalized input:
- finite document-space coordinates in f64;
- optional pressure in [0, 1];
- nondecreasing monotonic timestamps.

It produces a variable-radius dab stream:
- pressure follows a configurable power curve;
- dabs are placed from path distance, not MotionEvent callback count;
- spacing is bounded by the smaller endpoint radius to avoid visible gaps;
- duplicate positions are coalesced;
- the first and last points are preserved;
- a maximum dab count bounds pathological inputs.

rasm-renderer-tileink converts every dab to an analytic SDF circle in a Tileink Canvas. Its example creates a pressure ramp, renders it with WgpuRenderer, and writes target/renderer-spike.png.

## Running the desktop example

~~~sh
cargo run -p rasm-renderer-tileink --example render_stroke --release
~~~

The command needs a working WGPU adapter. It is not part of the automated tests because CI runners do not provide a stable GPU adapter guarantee.

## Acceptance gates before considering adoption

1. Build a native desktop prototype on Windows, macOS, and Linux.
2. Measure input-to-visible latency for real pressure samples, including high-rate pen events.
3. Measure retained damage and GPU upload statistics for local edits on large canvases.
4. Verify undo/redo and layer invalidation do not produce incorrect cached output.
5. Build a real Android Surface integration and run on at least one ARM64 GPU device.
6. Validate device limits, storage-texture fallback behavior, surface texture usage, memory pressure, and lifecycle recreation.
7. Verify all APIs and licenses required for distribution, including transitive assets and codecs.

Tileink should become a production dependency only if it passes these gates. Otherwise, retain the same dab/document interfaces and implement a small wgpu renderer specifically for the app's needs.
