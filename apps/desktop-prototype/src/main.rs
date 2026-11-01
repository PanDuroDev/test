use std::{
    borrow::Cow,
    collections::HashMap,
    mem::size_of,
    sync::{Arc, Mutex, MutexGuard},
    time::Instant,
};

mod canvas;

use bytemuck::{Pod, Zeroable};
use canvas::{CanvasDocument, CanvasViewport, DOCUMENT_HEIGHT, DOCUMENT_WIDTH};
use glam::DVec2;
use rasm_engine_document::Document;
use rasm_engine_ink::{
    BrushConfig, StrokeGeometryConfig, StrokeSample, tessellate_stroke, tessellate_stroke_mesh,
};
use wgpu::util::DeviceExt;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalPosition, PhysicalSize},
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    keyboard::{KeyCode, ModifiersState, PhysicalKey},
    window::{Window, WindowId},
};

const WINDOW_WIDTH: f64 = 960.0;
const WINDOW_HEIGHT: f64 = 640.0;
const MAX_TOTAL_VERTICES: usize = 2_000_000;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, Copy)]
struct PenAxes {
    pressure: Option<f32>,
    tilt_x: Option<i32>,
    tilt_y: Option<i32>,
    rotation: Option<u32>,
}

struct InputState {
    canvas: CanvasDocument,
    active_pen_id: Option<u32>,
    active_pen_samples: Vec<StrokeSample>,
    active_mouse_samples: Option<Vec<StrokeSample>>,
    cursor_position: Option<DVec2>,
    pan_anchor: Option<DVec2>,
    mouse_epoch: Option<Instant>,
    last_error: Option<String>,
    #[cfg(target_os = "windows")]
    last_pen_axes: Option<PenAxes>,
}

impl InputState {
    fn with_demo_stroke(width: f64, height: f64) -> Self {
        let mut canvas =
            CanvasDocument::new(width as u32, height as u32).expect("valid initial desktop canvas");
        let points = [
            (0.18, 0.58, 0.18_f32, 0_u64),
            (0.28, 0.43, 0.36, 4_000_000),
            (0.40, 0.38, 0.68, 8_000_000),
            (0.54, 0.48, 0.96, 12_000_000),
            (0.70, 0.42, 0.58, 16_000_000),
            (0.82, 0.57, 0.30, 20_000_000),
        ];
        let demo = points
            .into_iter()
            .filter_map(|(x, y, pressure, timestamp)| {
                StrokeSample::new(
                    DVec2::new(
                        f64::from(DOCUMENT_WIDTH) * x,
                        f64::from(DOCUMENT_HEIGHT) * y,
                    ),
                    Some(pressure),
                    timestamp,
                )
                .ok()
            })
            .collect::<Vec<_>>();
        canvas
            .commit_stroke(demo, BrushConfig::default())
            .expect("valid demo stroke");
        canvas.clear_history();

        Self {
            canvas,
            active_pen_id: None,
            active_pen_samples: Vec::new(),
            active_mouse_samples: None,
            cursor_position: None,
            pan_anchor: None,
            mouse_epoch: None,
            last_error: None,
            #[cfg(target_os = "windows")]
            last_pen_axes: None,
        }
    }

    fn resize_viewport(&mut self, size: PhysicalSize<u32>) {
        self.canvas.resize_viewport(size.width, size.height);
    }

    fn active_samples(&self) -> Option<&[StrokeSample]> {
        if !self.active_pen_samples.is_empty() {
            return Some(&self.active_pen_samples);
        }
        self.active_mouse_samples
            .as_deref()
            .filter(|samples| !samples.is_empty())
    }

    fn begin_mouse(&mut self) {
        if self.active_pen_id.is_some() {
            return;
        }
        let Some(position) = self.cursor_position else {
            return;
        };
        if self
            .canvas
            .viewport()
            .screen_to_document(position)
            .is_none()
        {
            return;
        }
        self.mouse_epoch = Some(Instant::now());
        self.active_mouse_samples = Some(Vec::new());
        self.push_mouse_position();
    }

    fn update_mouse_position(&mut self, position: PhysicalPosition<f64>) {
        let position = DVec2::new(position.x, position.y);
        if let Some(anchor) = self.pan_anchor {
            let delta = position - anchor;
            self.pan_anchor = Some(position);
            self.canvas.pan_by(delta);
        }
        self.cursor_position = Some(position);
        if self.active_mouse_samples.is_some() {
            self.push_mouse_position();
        }
    }

    fn end_mouse(&mut self) {
        let Some(samples) = self.active_mouse_samples.take() else {
            return;
        };
        self.mouse_epoch = None;
        self.store_completed(samples);
    }

    fn begin_pan(&mut self) {
        self.pan_anchor = self.cursor_position;
    }

    fn end_pan(&mut self) {
        self.pan_anchor = None;
    }

    fn zoom_at_cursor(&mut self, factor: f64) -> bool {
        let Some(anchor) = self.cursor_position else {
            return false;
        };
        self.canvas.zoom_at(factor, anchor)
    }

    fn push_mouse_position(&mut self) {
        let (Some(position), Some(epoch)) = (self.cursor_position, self.mouse_epoch) else {
            return;
        };
        let Some(document_position) = self.canvas.viewport().screen_to_document_clamped(position)
        else {
            return;
        };
        let timestamp_ns = epoch.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
        let Ok(sample) = StrokeSample::new(document_position, None, timestamp_ns) else {
            return;
        };
        if let Some(samples) = self.active_mouse_samples.as_mut() {
            Self::append_monotonic(samples, sample);
        }
    }

    #[cfg(target_os = "windows")]
    fn sample_from_screen(
        &self,
        screen_position: DVec2,
        pressure: Option<f32>,
        timestamp_ns: u64,
    ) -> Option<StrokeSample> {
        let document_position = self
            .canvas
            .viewport()
            .screen_to_document_clamped(screen_position)?;
        StrokeSample::new(document_position, pressure, timestamp_ns).ok()
    }

    fn store_completed(&mut self, samples: Vec<StrokeSample>) {
        if samples.is_empty() {
            return;
        }
        if let Err(error) = self.canvas.commit_stroke(samples, BrushConfig::default()) {
            self.last_error = Some(format!("stroke rejected: {error}"));
        }
    }

    fn take_error(&mut self) -> Option<String> {
        self.last_error.take()
    }

    fn append_monotonic(target: &mut Vec<StrokeSample>, sample: StrokeSample) {
        if let Some(last) = target.last() {
            if sample.timestamp_ns() < last.timestamp_ns() {
                return;
            }
            if sample.timestamp_ns() == last.timestamp_ns()
                && sample.position() == last.position()
                && sample.pressure() == last.pressure()
            {
                return;
            }
        }
        target.push(sample);
    }

    fn undo(&mut self) -> bool {
        if !self.canvas.can_undo() {
            return false;
        }
        match self.canvas.undo() {
            Ok(()) => true,
            Err(error) => {
                eprintln!("[document] undo failed: {error}");
                false
            }
        }
    }

    fn redo(&mut self) -> bool {
        if !self.canvas.can_redo() {
            return false;
        }
        match self.canvas.redo() {
            Ok(()) => true,
            Err(error) => {
                eprintln!("[document] redo failed: {error}");
                false
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn begin_pen(&mut self, pointer_id: u32) {
        if self.active_pen_id != Some(pointer_id) {
            if !self.active_pen_samples.is_empty() {
                let previous = std::mem::take(&mut self.active_pen_samples);
                self.store_completed(previous);
            }
            self.active_pen_id = Some(pointer_id);
            self.active_pen_samples.clear();
        }
        self.active_mouse_samples = None;
        self.mouse_epoch = None;
    }

    #[cfg(target_os = "windows")]
    fn append_pen_sample(&mut self, pointer_id: u32, sample: StrokeSample, axes: PenAxes) {
        if self.active_pen_id != Some(pointer_id) {
            return;
        }
        Self::append_monotonic(&mut self.active_pen_samples, sample);
        self.last_pen_axes = Some(axes);
    }

    #[cfg(target_os = "windows")]
    fn finish_pen(&mut self, pointer_id: u32) {
        if self.active_pen_id != Some(pointer_id) {
            return;
        }
        self.active_pen_id = None;
        let completed = std::mem::take(&mut self.active_pen_samples);
        self.store_completed(completed);
        if let Some(axes) = self.last_pen_axes {
            eprintln!(
                "[pen] stroke ended; pressure={:?}, tilt=({:?}, {:?}), rotation={:?}",
                axes.pressure, axes.tilt_x, axes.tilt_y, axes.rotation
            );
        }
    }
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
#[derive(Debug, Clone, Copy)]
enum AppEvent {
    InputChanged,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuVertex {
    position: [f32; 2],
}

const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x2];

impl GpuVertex {
    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &VERTEX_ATTRIBUTES,
        }
    }
}

struct GpuRenderer {
    instance: wgpu::Instance,
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    render_pipeline: wgpu::RenderPipeline,
    stroke_cache: HashMap<u64, CachedStroke>,
}

/// Document-space geometry of one committed stroke, kept across frames.
#[derive(Default)]
struct CachedStroke {
    positions: Vec<DVec2>,
    indices: Vec<u32>,
}

impl GpuRenderer {
    async fn new(
        display: winit::event_loop::OwnedDisplayHandle,
        window: Arc<Window>,
    ) -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(
            Box::new(display),
        ));
        let surface = instance
            .create_surface(window.clone())
            .map_err(|error| format!("failed to create wgpu surface: {error}"))?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await
            .map_err(|error| format!("no compatible GPU adapter: {error}"))?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .map_err(|error| format!("failed to create wgpu device: {error}"))?;

        let size = window.inner_size();
        let width = size.width.max(1);
        let height = size.height.max(1);
        let config = surface
            .get_default_config(&adapter, width, height)
            .ok_or_else(|| "the selected adapter cannot present to this window".to_owned())?;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Rasm stroke shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shader.wgsl"))),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Rasm stroke pipeline layout"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Rasm stroke pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(GpuVertex::layout())],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(config.format.into())],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        surface.configure(&device, &config);

        Ok(Self {
            instance,
            window,
            surface,
            device,
            queue,
            config,
            render_pipeline,
            stroke_cache: HashMap::new(),
        })
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
    }

    fn rebuild_surface(&mut self) {
        match self.instance.create_surface(self.window.clone()) {
            Ok(surface) => {
                self.surface = surface;
                self.surface.configure(&self.device, &self.config);
            }
            Err(error) => eprintln!("[wgpu] failed to recreate surface: {error}"),
        }
    }

    /// Tessellates a committed stroke once and reuses it on later frames.
    ///
    /// shortcut: the in-progress stroke is tessellated every frame; cache it only
    /// if profiling shows the live stroke is the bottleneck.
    fn cached_stroke(
        &mut self,
        id: u64,
        samples: &[StrokeSample],
        brush: BrushConfig,
    ) -> &CachedStroke {
        self.stroke_cache.entry(id).or_insert_with(|| {
            let Ok(dabs) = tessellate_stroke(samples, brush) else {
                return CachedStroke::default();
            };
            let Ok(mesh) = tessellate_stroke_mesh(&dabs, StrokeGeometryConfig::default()) else {
                return CachedStroke::default();
            };
            CachedStroke {
                positions: mesh.vertices().iter().map(|v| v.position()).collect(),
                indices: mesh.indices().to_vec(),
            }
        })
    }

    fn append_geometry(
        positions: &[DVec2],
        stroke_indices: &[u32],
        viewport: CanvasViewport,
        vertices: &mut Vec<GpuVertex>,
        indices: &mut Vec<u32>,
    ) {
        if stroke_indices.is_empty()
            || vertices.len().saturating_add(positions.len()) > MAX_TOTAL_VERTICES
            || vertices.len() > (u32::MAX as usize).saturating_sub(positions.len())
        {
            return;
        }

        let base = vertices.len() as u32;
        for position in positions {
            vertices.push(GpuVertex {
                position: viewport.document_to_clip(*position),
            });
        }
        indices.extend(stroke_indices.iter().map(|index| base + *index));
    }

    fn append_live_stroke(
        &self,
        samples: &[StrokeSample],
        viewport: CanvasViewport,
        vertices: &mut Vec<GpuVertex>,
        indices: &mut Vec<u32>,
    ) {
        let Ok(dabs) = tessellate_stroke(samples, BrushConfig::default()) else {
            return;
        };
        let Ok(mesh) = tessellate_stroke_mesh(&dabs, StrokeGeometryConfig::default()) else {
            return;
        };
        let positions: Vec<DVec2> = mesh.vertices().iter().map(|v| v.position()).collect();
        Self::append_geometry(&positions, mesh.indices(), viewport, vertices, indices);
    }

    fn make_mesh_buffers(
        &mut self,
        document: &Document,
        active_samples: Option<&[StrokeSample]>,
        viewport: CanvasViewport,
    ) -> (wgpu::Buffer, wgpu::Buffer, u32) {
        let mut vertices = Vec::<GpuVertex>::new();
        let mut indices = Vec::<u32>::new();

        let mut visible_ids = Vec::new();
        for layer in document.layers() {
            if !layer.is_visible() || layer.opacity() <= 0.0 {
                continue;
            }
            for stroke in layer.strokes() {
                let id = stroke.id().get();
                visible_ids.push(id);
                let cached = self.cached_stroke(id, stroke.samples(), stroke.brush());
                Self::append_geometry(
                    &cached.positions,
                    &cached.indices,
                    viewport,
                    &mut vertices,
                    &mut indices,
                );
            }
        }
        self.stroke_cache.retain(|id, _| visible_ids.contains(id));

        if let Some(samples) = active_samples {
            self.append_live_stroke(samples, viewport, &mut vertices, &mut indices);
        }

        if vertices.is_empty() {
            vertices.push(GpuVertex::zeroed());
        }
        let vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Rasm stroke vertices"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let index_count = indices.len().min(u32::MAX as usize) as u32;
        if indices.is_empty() {
            indices.push(0);
        }
        let index_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Rasm stroke indices"),
                contents: bytemuck::cast_slice(&indices),
                usage: wgpu::BufferUsages::INDEX,
            });
        (vertex_buffer, index_buffer, index_count)
    }

    fn render(
        &mut self,
        document: &Document,
        active_samples: Option<&[StrokeSample]>,
        viewport: CanvasViewport,
    ) {
        if self.config.width == 0 || self.config.height == 0 {
            return;
        }

        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => texture,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => return,
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                drop(texture);
                self.surface.configure(&self.device, &self.config);
                self.window.request_redraw();
                return;
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                self.window.request_redraw();
                return;
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.rebuild_surface();
                self.window.request_redraw();
                return;
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                eprintln!("[wgpu] surface validation error");
                return;
            }
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let (vertex_buffer, index_buffer, index_count) =
            self.make_mesh_buffers(document, active_samples, viewport);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Rasm desktop frame"),
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Rasm canvas pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.055,
                            g: 0.065,
                            b: 0.075,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            let (scissor_x, scissor_y, scissor_width, scissor_height) = viewport.scissor_rect();
            if index_count > 0 && scissor_width > 0 && scissor_height > 0 {
                pass.set_scissor_rect(scissor_x, scissor_y, scissor_width, scissor_height);
                pass.set_pipeline(&self.render_pipeline);
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..index_count, 0, 0..1);
            }
        }

        self.queue.submit([encoder.finish()]);
        self.window.pre_present_notify();
        self.queue.present(frame);
    }
}

struct App {
    window: Option<Arc<Window>>,
    renderer: Option<GpuRenderer>,
    input: Arc<Mutex<InputState>>,
    modifiers: ModifiersState,
}

impl App {
    fn new(input: Arc<Mutex<InputState>>) -> Self {
        Self {
            window: None,
            renderer: None,
            input,
            modifiers: ModifiersState::default(),
        }
    }
}

impl ApplicationHandler<AppEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("Rasm Studio | wheel zoom, middle-drag pan, Ctrl+0 reset")
                        .with_inner_size(LogicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT)),
                )
                .expect("failed to create desktop window"),
        );
        lock(&self.input).resize_viewport(window.inner_size());
        let renderer = match pollster::block_on(GpuRenderer::new(
            event_loop.owned_display_handle(),
            window.clone(),
        )) {
            Ok(renderer) => renderer,
            Err(error) => {
                eprintln!("[startup] {error}");
                event_loop.exit();
                return;
            }
        };

        eprintln!("[desktop] wgpu surface ready; draw with a pen or left mouse button");
        self.window = Some(window.clone());
        self.renderer = Some(renderer);
        window.request_redraw();
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: AppEvent) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                lock(&self.input).resize_viewport(size);
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.resize(size);
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                let window = self.window.clone();
                if let Some(renderer) = self.renderer.as_mut() {
                    let mut input = lock(&self.input);
                    renderer.render(
                        input.canvas.document(),
                        input.active_samples(),
                        input.canvas.viewport(),
                    );
                    if let Some(error) = input.take_error() {
                        if let Some(window) = &window {
                            window.set_title(&format!("Rasm Studio | {error}"));
                        }
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                lock(&self.input).update_mouse_position(position);
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                let mut input = lock(&self.input);
                match state {
                    ElementState::Pressed => input.begin_mouse(),
                    ElementState::Released => input.end_mouse(),
                }
                drop(input);
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Middle,
                ..
            } => {
                let mut input = lock(&self.input);
                match state {
                    ElementState::Pressed => input.begin_pan(),
                    ElementState::Released => input.end_pan(),
                }
                drop(input);
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let step = match delta {
                    MouseScrollDelta::LineDelta(_, y) => f64::from(y),
                    MouseScrollDelta::PixelDelta(position) => position.y / 100.0,
                };
                if step != 0.0 && lock(&self.input).zoom_at_cursor(1.2_f64.powf(step)) {
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                }
            }
            WindowEvent::Focused(false) => {
                self.modifiers = ModifiersState::default();
                let mut input = lock(&self.input);
                input.end_mouse();
                input.end_pan();
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
            }
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed
                    && !event.repeat
                    && self.modifiers.control_key() =>
            {
                let changed = match event.physical_key {
                    PhysicalKey::Code(KeyCode::KeyZ) if self.modifiers.shift_key() => {
                        lock(&self.input).redo()
                    }
                    PhysicalKey::Code(KeyCode::KeyZ) => lock(&self.input).undo(),
                    PhysicalKey::Code(KeyCode::KeyY) => lock(&self.input).redo(),
                    PhysicalKey::Code(KeyCode::Digit0) => {
                        lock(&self.input).canvas.reset_view();
                        true
                    }
                    _ => false,
                };
                if changed {
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                }
            }
            _ => {}
        }
    }
}

fn main() {
    let input = Arc::new(Mutex::new(InputState::with_demo_stroke(
        WINDOW_WIDTH,
        WINDOW_HEIGHT,
    )));
    let proxy_slot: Arc<Mutex<Option<EventLoopProxy<AppEvent>>>> = Arc::new(Mutex::new(None));

    let mut builder = EventLoop::<AppEvent>::with_user_event();

    #[cfg(target_os = "windows")]
    {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        let input_for_hook = Arc::clone(&input);
        let proxy_for_hook = Arc::clone(&proxy_slot);
        builder.with_msg_hook(move |message| {
            let changed = unsafe { handle_windows_pointer_message(message, &input_for_hook) };
            if changed {
                if let Some(proxy) = lock(&proxy_for_hook).as_ref() {
                    let _ = proxy.send_event(AppEvent::InputChanged);
                }
            }
            false
        });
    }

    let event_loop = builder.build().expect("failed to create event loop");
    *lock(&proxy_slot) = Some(event_loop.create_proxy());
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = App::new(input);
    event_loop
        .run_app(&mut app)
        .expect("desktop event loop failed");
}

#[cfg(target_os = "windows")]
unsafe fn handle_windows_pointer_message(
    message: *const std::ffi::c_void,
    input: &Mutex<InputState>,
) -> bool {
    use windows_sys::Win32::{
        Foundation::POINT,
        Graphics::Gdi::ScreenToClient,
        UI::{
            Input::Pointer::{
                GetPointerPenInfo, GetPointerPenInfoHistory, GetPointerType, POINTER_PEN_INFO,
            },
            WindowsAndMessaging::{
                MSG, PEN_MASK_PRESSURE, PEN_MASK_ROTATION, PEN_MASK_TILT_X, PEN_MASK_TILT_Y,
                PT_PEN, WM_POINTERDOWN, WM_POINTERUP, WM_POINTERUPDATE,
            },
        },
    };

    const MAX_HISTORY_ENTRIES: usize = 512;

    if message.is_null() {
        return false;
    }

    let message = unsafe { &*message.cast::<MSG>() };
    if !matches!(
        message.message,
        WM_POINTERDOWN | WM_POINTERUPDATE | WM_POINTERUP
    ) {
        return false;
    }

    let pointer_id = (message.wParam & 0xFFFF) as u32;
    let mut pointer_type = 0_i32;
    if unsafe { GetPointerType(pointer_id, &mut pointer_type) } == 0 || pointer_type != PT_PEN {
        return false;
    }

    let mut latest = POINTER_PEN_INFO::default();
    if unsafe { GetPointerPenInfo(pointer_id, &mut latest) } == 0 {
        return false;
    }

    let available = latest.pointerInfo.historyCount.max(1) as usize;
    let capacity = available.min(MAX_HISTORY_ENTRIES);
    let mut history = vec![POINTER_PEN_INFO::default(); capacity];
    let mut entries = capacity as u32;
    let history_ok =
        unsafe { GetPointerPenInfoHistory(pointer_id, &mut entries, history.as_mut_ptr()) } != 0;

    if history_ok {
        history.truncate((entries as usize).min(history.len()));
        history.reverse();
    } else {
        history.clear();
        history.push(latest);
    }

    let mut input = lock(input);
    if message.message == WM_POINTERDOWN {
        let mut client_point: POINT = latest.pointerInfo.ptPixelLocation;
        if unsafe { ScreenToClient(message.hwnd, &mut client_point) } == 0
            || input
                .canvas
                .viewport()
                .screen_to_document(DVec2::new(
                    f64::from(client_point.x),
                    f64::from(client_point.y),
                ))
                .is_none()
        {
            return false;
        }
        input.begin_pen(pointer_id);
    }
    if input.active_pen_id != Some(pointer_id) {
        return false;
    }

    let truncated = available > MAX_HISTORY_ENTRIES;
    for info in history {
        let mut client_point: POINT = info.pointerInfo.ptPixelLocation;
        if unsafe { ScreenToClient(message.hwnd, &mut client_point) } == 0 {
            continue;
        }

        let pressure = (info.penMask & PEN_MASK_PRESSURE != 0)
            .then(|| (info.pressure as f32 / 1024.0).clamp(0.0, 1.0));
        let tilt_x = (info.penMask & PEN_MASK_TILT_X != 0).then_some(info.tiltX);
        let tilt_y = (info.penMask & PEN_MASK_TILT_Y != 0).then_some(info.tiltY);
        let rotation = (info.penMask & PEN_MASK_ROTATION != 0).then_some(info.rotation);
        let axes = PenAxes {
            pressure,
            tilt_x,
            tilt_y,
            rotation,
        };
        let timestamp_ns = u64::from(info.pointerInfo.dwTime) * 1_000_000;
        let screen_position = DVec2::new(f64::from(client_point.x), f64::from(client_point.y));

        if let Some(sample) = input.sample_from_screen(screen_position, pressure, timestamp_ns) {
            input.append_pen_sample(pointer_id, sample, axes);
        }
    }

    if truncated {
        eprintln!(
            "[pen] Windows coalesced more than {MAX_HISTORY_ENTRIES} samples; oldest samples in this message were truncated"
        );
    }

    if message.message == WM_POINTERUP {
        input.finish_pen(pointer_id);
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_stroke_is_stored_in_the_document_and_produces_a_mesh() {
        let input = InputState::with_demo_stroke(960.0, 640.0);
        let stroke = &input.canvas.document().layers()[0].strokes()[0];
        let dabs = tessellate_stroke(stroke.samples(), stroke.brush()).expect("demo dabs");
        let mesh =
            tessellate_stroke_mesh(&dabs, StrokeGeometryConfig::default()).expect("demo mesh");

        assert!(!mesh.is_empty());
        assert!(!mesh.vertices().is_empty());
        assert!(mesh.triangle_count() > 0);
    }

    #[test]
    fn completed_mouse_stroke_is_committed_and_undoable() {
        let mut input = InputState::with_demo_stroke(960.0, 640.0);
        let initial = input.canvas.document().layers()[0].strokes().len();

        input.update_mouse_position(PhysicalPosition::new(480.0, 320.0));
        input.begin_mouse();
        input.update_mouse_position(PhysicalPosition::new(520.0, 340.0));
        input.end_mouse();

        assert_eq!(
            input.canvas.document().layers()[0].strokes().len(),
            initial + 1
        );
        assert!(input.undo());
        assert_eq!(input.canvas.document().layers()[0].strokes().len(), initial);
        assert!(input.redo());
        assert_eq!(
            input.canvas.document().layers()[0].strokes().len(),
            initial + 1
        );
    }

    #[test]
    fn mouse_down_in_letterbox_does_not_begin_a_stroke() {
        let mut input = InputState::with_demo_stroke(960.0, 640.0);
        input.update_mouse_position(PhysicalPosition::new(480.0, 10.0));
        input.begin_mouse();
        assert!(input.active_mouse_samples.is_none());
    }

    #[test]
    fn sample_storage_rejects_out_of_order_timestamps() {
        let mut samples = Vec::new();
        let first =
            StrokeSample::new(DVec2::new(1.0, 1.0), Some(0.5), 10).expect("valid first sample");
        let earlier =
            StrokeSample::new(DVec2::new(2.0, 2.0), Some(0.6), 9).expect("valid earlier sample");

        InputState::append_monotonic(&mut samples, first);
        InputState::append_monotonic(&mut samples, earlier);

        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].timestamp_ns(), 10);
    }
}
