use std::{
    borrow::Cow,
    collections::HashMap,
    mem::size_of,
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
    time::Instant,
};

mod canvas;
mod layer_panel;

use bytemuck::{Pod, Zeroable};
use canvas::{
    BRUSH_RADIUS_STEP, BrushSettings, CanvasDocument, CanvasViewport, DOCUMENT_HEIGHT,
    DOCUMENT_WIDTH,
};
use glam::DVec2;
use rasm_engine_document::{Document, LayerId, StrokeColor};
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

#[derive(Debug, Clone, Copy)]
struct LayerPanelView {
    active_layer: LayerId,
    scroll_offset: usize,
}

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
    brush: BrushSettings,
    current_path: Option<PathBuf>,
    status_message: Option<String>,
    title_dirty: bool,
    active_layer_id: LayerId,
    panel_scroll: usize,
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
        let active_layer_id = canvas
            .document()
            .layers()
            .last()
            .expect("a document always contains one layer")
            .id();
        let brush = BrushSettings::default();

        Self {
            canvas,
            active_pen_id: None,
            active_pen_samples: Vec::new(),
            active_mouse_samples: None,
            cursor_position: None,
            pan_anchor: None,
            mouse_epoch: None,
            brush,
            current_path: None,
            status_message: Some("Ready. Unsaved project.".to_owned()),
            title_dirty: true,
            active_layer_id,
            panel_scroll: 0,
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

    /// Commits any active pointer stroke before a project operation starts.
    fn finish_active_input(&mut self) {
        self.end_mouse();
        #[cfg(target_os = "windows")]
        if let Some(pointer_id) = self.active_pen_id {
            self.finish_pen(pointer_id);
        }
        self.end_pan();
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
        let color = StrokeColor::new(self.brush.color())
            .expect("built-in brush colours use normalized RGBA channels");
        match self.canvas.commit_stroke_to_with_color(
            self.active_layer_id,
            samples,
            self.brush.brush(),
            color,
        ) {
            Ok(Some(_)) | Ok(None) => {}
            Err(error) => self.status_message = Some(format!("Stroke rejected: {error}")),
        }
    }

    fn take_title_change(&mut self) -> bool {
        std::mem::take(&mut self.title_dirty)
    }

    fn title(&self) -> String {
        let project_path = self
            .current_path
            .as_ref()
            .map_or_else(|| "Untitled".to_owned(), |path| path.display().to_string());
        let layer_name = self
            .canvas
            .document()
            .layer(self.active_layer_id)
            .map_or("No layer", |layer| layer.name());
        format!(
            "Rasm Studio | {project_path} | {layer_name} | {}",
            self.brush.status()
        )
    }

    fn open_project(&mut self, path: PathBuf) -> Result<(), String> {
        let document =
            rasm_engine_storage::load(&path).map_err(|error| format!("Open failed: {error}"))?;
        let active_layer = document
            .layers()
            .last()
            .map(|layer| layer.id())
            .ok_or_else(|| "Open failed: project contains no layers".to_owned())?;

        // Replace only after decoding and all document validation have succeeded.
        self.active_mouse_samples = None;
        self.mouse_epoch = None;
        self.active_pen_id = None;
        self.active_pen_samples.clear();
        self.pan_anchor = None;
        #[cfg(target_os = "windows")]
        {
            self.last_pen_axes = None;
        }
        self.canvas.replace_document(document);
        self.active_layer_id = active_layer;
        self.panel_scroll = 0;
        self.current_path = Some(path.clone());
        self.status_message = Some(format!("Opened {}", path.display()));
        self.title_dirty = true;
        Ok(())
    }

    fn save_project(&mut self, mut path: PathBuf) -> Result<(), String> {
        if !path
            .extension()
            .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("rasm"))
        {
            path.set_extension("rasm");
        }

        match rasm_engine_storage::save_atomic(&path, self.canvas.document()) {
            Ok(()) => {
                self.current_path = Some(path.clone());
                self.status_message = Some(format!("Saved {}", path.display()));
                self.title_dirty = true;
                Ok(())
            }
            Err(rasm_engine_storage::FileStorageError::DirectorySyncAfterCommit(error)) => {
                self.current_path = Some(path.clone());
                let message = format!(
                    "Saved {}, but syncing its directory failed: {error}",
                    path.display()
                );
                self.status_message = Some(message.clone());
                self.title_dirty = true;
                Err(message)
            }
            Err(error) => Err(format!("Save failed: {error}")),
        }
    }

    fn select_eraser(&mut self, eraser: bool) -> bool {
        let changed = self.brush.set_eraser(eraser);
        self.note_brush_change(changed)
    }

    fn scale_brush(&mut self, factor: f64) -> bool {
        let changed = self.brush.scale_radius(factor);
        self.note_brush_change(changed)
    }

    fn select_color(&mut self, index: usize) -> bool {
        let changed = self.brush.select_color(index);
        self.note_brush_change(changed)
    }

    fn note_brush_change(&mut self, changed: bool) -> bool {
        self.title_dirty |= changed;
        changed
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
            Ok(()) => {
                self.reconcile_active_layer();
                true
            }
            Err(error) => {
                self.status_message = Some(format!("Undo failed: {error}"));
                false
            }
        }
    }

    fn redo(&mut self) -> bool {
        if !self.canvas.can_redo() {
            return false;
        }
        match self.canvas.redo() {
            Ok(()) => {
                self.reconcile_active_layer();
                true
            }
            Err(error) => {
                self.status_message = Some(format!("Redo failed: {error}"));
                false
            }
        }
    }

    fn reconcile_active_layer(&mut self) {
        if self.canvas.document().layer(self.active_layer_id).is_none() {
            if let Some(layer) = self.canvas.document().layers().last() {
                self.active_layer_id = layer.id();
                self.title_dirty = true;
            }
        }
    }

    fn pointer_over_layer_panel(&self, size: PhysicalSize<u32>) -> bool {
        self.cursor_position.is_some_and(|position| {
            layer_panel::contains(
                position,
                size.width,
                size.height,
                self.canvas.document(),
                self.panel_scroll,
            )
        })
    }

    /// Handles a left click over the panel. Panel background clicks are consumed
    /// too, so they never create accidental marks on the canvas underneath.
    fn handle_layer_panel_click(&mut self, size: PhysicalSize<u32>) -> bool {
        let Some(position) = self.cursor_position else {
            return false;
        };
        if !layer_panel::contains(
            position,
            size.width,
            size.height,
            self.canvas.document(),
            self.panel_scroll,
        ) {
            return false;
        }

        if let Some(action) = layer_panel::hit_test(
            self.canvas.document(),
            position,
            size.width,
            size.height,
            self.panel_scroll,
        ) {
            match action {
                layer_panel::LayerPanelAction::Add => {
                    let name = format!("Layer {}", self.canvas.document().layers().len() + 1);
                    match self.canvas.add_layer(name) {
                        Ok(id) => {
                            self.active_layer_id = id;
                            self.title_dirty = true;
                        }
                        Err(error) => {
                            self.status_message = Some(format!("Cannot add layer: {error}"))
                        }
                    }
                }
                layer_panel::LayerPanelAction::Select(id) => {
                    if self.canvas.document().layer(id).is_some() {
                        self.active_layer_id = id;
                        self.title_dirty = true;
                    }
                }
                layer_panel::LayerPanelAction::ToggleVisibility(id) => {
                    let Some(visible) = self
                        .canvas
                        .document()
                        .layer(id)
                        .map(|layer| !layer.is_visible())
                    else {
                        return true;
                    };
                    if let Err(error) = self.canvas.set_layer_visibility(id, visible) {
                        self.status_message =
                            Some(format!("Cannot change layer visibility: {error}"));
                    }
                    self.title_dirty = true;
                }
                layer_panel::LayerPanelAction::Remove(id) => {
                    let was_active = self.active_layer_id == id;
                    let old_index = self
                        .canvas
                        .document()
                        .layers()
                        .iter()
                        .position(|layer| layer.id() == id)
                        .unwrap_or(0);
                    match self.canvas.remove_layer(id) {
                        Ok(()) => {
                            if was_active {
                                let layers = self.canvas.document().layers();
                                let index = old_index.min(layers.len().saturating_sub(1));
                                self.active_layer_id = layers[index].id();
                            }
                            self.reconcile_active_layer();
                            self.title_dirty = true;
                        }
                        Err(error) => {
                            self.status_message = Some(format!("Cannot remove layer: {error}"));
                        }
                    }
                }
            }
        }
        true
    }

    fn scroll_layer_panel(&mut self, step: f64, screen_height: u32) -> bool {
        if !step.is_finite() || step == 0.0 {
            return false;
        }
        let max_scroll = layer_panel::max_scroll(self.canvas.document(), screen_height);
        let amount = step.abs().ceil().clamp(1.0, 5.0) as usize;
        let next = if step > 0.0 {
            self.panel_scroll.saturating_sub(amount)
        } else {
            self.panel_scroll.saturating_add(amount).min(max_scroll)
        };
        let changed = next != self.panel_scroll;
        self.panel_scroll = next;
        changed
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
    color: [f32; 4],
}

const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 2] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4];

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
    fn clear_document_cache(&mut self) {
        self.stroke_cache.clear();
    }

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
        color: [f32; 4],
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
                color,
            });
        }
        indices.extend(stroke_indices.iter().map(|index| base + *index));
    }

    fn append_live_stroke(
        &self,
        samples: &[StrokeSample],
        brush: BrushSettings,
        viewport: CanvasViewport,
        vertices: &mut Vec<GpuVertex>,
        indices: &mut Vec<u32>,
    ) {
        let Ok(dabs) = tessellate_stroke(samples, brush.brush()) else {
            return;
        };
        let Ok(mesh) = tessellate_stroke_mesh(&dabs, StrokeGeometryConfig::default()) else {
            return;
        };
        let positions: Vec<DVec2> = mesh.vertices().iter().map(|v| v.position()).collect();
        Self::append_geometry(
            &positions,
            mesh.indices(),
            brush.color(),
            viewport,
            vertices,
            indices,
        );
    }

    fn make_mesh_buffers(
        &mut self,
        document: &Document,
        active_samples: Option<&[StrokeSample]>,
        brush: BrushSettings,
        layer_panel: LayerPanelView,
        status_message: Option<&str>,
        viewport: CanvasViewport,
    ) -> (wgpu::Buffer, wgpu::Buffer, u32, u32) {
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
                let color = stroke.color().rgba();
                Self::append_geometry(
                    &cached.positions,
                    &cached.indices,
                    color,
                    viewport,
                    &mut vertices,
                    &mut indices,
                );
            }
        }
        self.stroke_cache.retain(|id, _| visible_ids.contains(id));

        if let Some(samples) = active_samples {
            self.append_live_stroke(samples, brush, viewport, &mut vertices, &mut indices);
        }

        let canvas_index_count = indices.len().min(u32::MAX as usize) as u32;
        let mut panel_vertices = Vec::new();
        let mut panel_indices = Vec::new();
        layer_panel::append_geometry(
            document,
            layer_panel.active_layer,
            self.config.width,
            self.config.height,
            layer_panel.scroll_offset,
            &mut panel_vertices,
            &mut panel_indices,
        );
        layer_panel::append_status_geometry(
            status_message,
            self.config.width,
            self.config.height,
            &mut panel_vertices,
            &mut panel_indices,
        );
        if vertices.len().saturating_add(panel_vertices.len()) <= MAX_TOTAL_VERTICES
            && vertices.len() <= (u32::MAX as usize).saturating_sub(panel_vertices.len())
        {
            let base = vertices.len() as u32;
            let width = self.config.width.max(1) as f32;
            let height = self.config.height.max(1) as f32;
            for vertex in panel_vertices {
                vertices.push(GpuVertex {
                    position: [
                        vertex.position[0] / width * 2.0 - 1.0,
                        1.0 - vertex.position[1] / height * 2.0,
                    ],
                    color: vertex.color,
                });
            }
            indices.extend(panel_indices.into_iter().map(|index| base + index));
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
        let panel_index_count = indices
            .len()
            .saturating_sub(canvas_index_count as usize)
            .min(u32::MAX as usize) as u32;
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
        (
            vertex_buffer,
            index_buffer,
            canvas_index_count,
            panel_index_count,
        )
    }

    fn render(
        &mut self,
        document: &Document,
        active_samples: Option<&[StrokeSample]>,
        brush: BrushSettings,
        layer_panel: LayerPanelView,
        status_message: Option<&str>,
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
        let (vertex_buffer, index_buffer, canvas_index_count, panel_index_count) = self
            .make_mesh_buffers(
                document,
                active_samples,
                brush,
                layer_panel,
                status_message,
                viewport,
            );
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
            if (canvas_index_count > 0 || panel_index_count > 0)
                && scissor_width > 0
                && scissor_height > 0
            {
                pass.set_pipeline(&self.render_pipeline);
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);

                if canvas_index_count > 0 {
                    pass.set_scissor_rect(scissor_x, scissor_y, scissor_width, scissor_height);
                    pass.draw_indexed(0..canvas_index_count, 0, 0..1);
                }

                if panel_index_count > 0 {
                    // UI is outside the canvas letterbox in some window sizes.
                    pass.set_scissor_rect(0, 0, self.config.width, self.config.height);
                    pass.draw_indexed(
                        canvas_index_count..canvas_index_count + panel_index_count,
                        0,
                        0..1,
                    );
                }
            }
        }

        self.queue.submit([encoder.finish()]);
        self.window.pre_present_notify();
        self.queue.present(frame);
    }
}

#[derive(Debug, Clone, Copy)]
enum ProjectAction {
    Open,
    Save,
    SaveAs,
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

    fn file_dialog(&self, title: &str) -> rfd::FileDialog {
        let dialog = rfd::FileDialog::new()
            .add_filter("Rasm project", &["rasm"])
            .set_title(title);
        match &self.window {
            Some(window) => dialog.set_parent(window.as_ref()),
            None => dialog,
        }
    }

    fn handle_project_action(&mut self, action: ProjectAction) {
        lock(&self.input).finish_active_input();
        let current_path = lock(&self.input).current_path.clone();
        let selected_path = match action {
            ProjectAction::Open => self.file_dialog("Open Rasm project").pick_file(),
            ProjectAction::Save if current_path.is_some() => current_path,
            ProjectAction::Save | ProjectAction::SaveAs => self
                .file_dialog("Save Rasm project")
                .set_file_name("Untitled.rasm")
                .save_file(),
        };
        let Some(path) = selected_path else {
            return;
        };

        let result = match action {
            ProjectAction::Open => {
                let result = lock(&self.input).open_project(path);
                if result.is_ok() {
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.clear_document_cache();
                    }
                }
                result
            }
            ProjectAction::Save | ProjectAction::SaveAs => lock(&self.input).save_project(path),
        };

        if let Err(error) = result {
            let mut input = lock(&self.input);
            input.status_message = Some(error);
            input.title_dirty = true;
        }
        if let Some(window) = &self.window {
            window.request_redraw();
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
                        input.brush,
                        LayerPanelView {
                            active_layer: input.active_layer_id,
                            scroll_offset: input.panel_scroll,
                        },
                        input.status_message.as_deref(),
                        input.canvas.viewport(),
                    );
                    if input.take_title_change() {
                        if let Some(window) = &window {
                            window.set_title(&input.title());
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
                    ElementState::Pressed => {
                        let size = self
                            .window
                            .as_ref()
                            .map_or(PhysicalSize::new(0, 0), |window| window.inner_size());
                        if !input.handle_layer_panel_click(size) {
                            input.begin_mouse();
                        }
                    }
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
                if step != 0.0 {
                    let size = self
                        .window
                        .as_ref()
                        .map_or(PhysicalSize::new(0, 0), |window| window.inner_size());
                    let mut input = lock(&self.input);
                    let changed = if input.pointer_over_layer_panel(size) {
                        input.scroll_layer_panel(step, size.height)
                    } else {
                        input.zoom_at_cursor(1.2_f64.powf(step))
                    };
                    drop(input);
                    if changed {
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
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
                let file_action = match event.physical_key {
                    PhysicalKey::Code(KeyCode::KeyO) => Some(ProjectAction::Open),
                    PhysicalKey::Code(KeyCode::KeyS) if self.modifiers.shift_key() => {
                        Some(ProjectAction::SaveAs)
                    }
                    PhysicalKey::Code(KeyCode::KeyS) => Some(ProjectAction::Save),
                    _ => None,
                };
                if let Some(action) = file_action {
                    self.handle_project_action(action);
                    return;
                }

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
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed
                    && !event.repeat
                    && !self.modifiers.control_key() =>
            {
                let changed = match event.physical_key {
                    PhysicalKey::Code(KeyCode::KeyB) => lock(&self.input).select_eraser(false),
                    PhysicalKey::Code(KeyCode::KeyE) => lock(&self.input).select_eraser(true),
                    PhysicalKey::Code(KeyCode::BracketLeft) => {
                        lock(&self.input).scale_brush(1.0 / BRUSH_RADIUS_STEP)
                    }
                    PhysicalKey::Code(KeyCode::BracketRight) => {
                        lock(&self.input).scale_brush(BRUSH_RADIUS_STEP)
                    }
                    PhysicalKey::Code(KeyCode::Digit1) => lock(&self.input).select_color(0),
                    PhysicalKey::Code(KeyCode::Digit2) => lock(&self.input).select_color(1),
                    PhysicalKey::Code(KeyCode::Digit3) => lock(&self.input).select_color(2),
                    PhysicalKey::Code(KeyCode::Digit4) => lock(&self.input).select_color(3),
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
    fn project_action_finalizes_an_in_progress_mouse_stroke() {
        let mut input = InputState::with_demo_stroke(960.0, 640.0);
        let initial = input.canvas.document().layers()[0].strokes().len();

        input.update_mouse_position(PhysicalPosition::new(480.0, 320.0));
        input.begin_mouse();
        input.update_mouse_position(PhysicalPosition::new(510.0, 340.0));
        assert!(input.active_mouse_samples.is_some());

        input.finish_active_input();

        assert!(input.active_mouse_samples.is_none());
        assert_eq!(
            input.canvas.document().layers()[0].strokes().len(),
            initial + 1
        );
        assert!(!input.canvas.can_redo());
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
    fn layer_panel_routes_drawing_and_visibility_through_the_selected_layer() {
        let mut input = InputState::with_demo_stroke(960.0, 640.0);
        let window_size = PhysicalSize::new(960, 640);
        let original_layer = input.active_layer_id;

        input.update_mouse_position(PhysicalPosition::new(220.0, 28.0));
        assert!(input.handle_layer_panel_click(window_size));
        let selected_layer = input.active_layer_id;
        assert_ne!(selected_layer, original_layer);

        input.update_mouse_position(PhysicalPosition::new(480.0, 320.0));
        input.begin_mouse();
        input.update_mouse_position(PhysicalPosition::new(520.0, 340.0));
        input.end_mouse();
        assert_eq!(
            input
                .canvas
                .document()
                .layer(selected_layer)
                .unwrap()
                .strokes()
                .len(),
            1
        );

        input.update_mouse_position(PhysicalPosition::new(190.0, 58.0));
        assert!(input.handle_layer_panel_click(window_size));
        assert!(
            !input
                .canvas
                .document()
                .layer(selected_layer)
                .unwrap()
                .is_visible()
        );

        input.update_mouse_position(PhysicalPosition::new(230.0, 58.0));
        assert!(input.handle_layer_panel_click(window_size));
        assert!(input.canvas.document().layer(selected_layer).is_none());
        assert_eq!(input.active_layer_id, original_layer);

        assert!(input.undo());
        assert!(input.canvas.document().layer(selected_layer).is_some());
        assert!(input.undo());
        assert!(
            input
                .canvas
                .document()
                .layer(selected_layer)
                .unwrap()
                .is_visible()
        );
        assert!(input.undo());
        assert!(
            input
                .canvas
                .document()
                .layer(selected_layer)
                .unwrap()
                .strokes()
                .is_empty()
        );
        assert!(input.undo());
        assert!(input.canvas.document().layer(selected_layer).is_none());
        assert!(input.redo());
        assert!(input.canvas.document().layer(selected_layer).is_some());
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
