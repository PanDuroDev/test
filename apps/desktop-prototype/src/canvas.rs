//! Document-backed desktop canvas state and viewport mapping.

use glam::DVec2;
use rasm_engine_core::transform::ViewportTransform;
use rasm_engine_document::{
    Document, DocumentError, Edit, EditResult, History, LayerId, StrokeColor,
};
use rasm_engine_ink::{BrushConfig, StrokeSample};

/// Initial logical canvas size for the desktop prototype.
pub const DOCUMENT_WIDTH: u32 = 1_920;
pub const DOCUMENT_HEIGHT: u32 = 1_080;

/// User-controlled zoom multiplier and pan offset layered over the fitted view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewState {
    zoom: f64,
    pan: DVec2,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan: DVec2::ZERO,
        }
    }
}

impl ViewState {
    pub const MIN_ZOOM: f64 = 0.25;
    pub const MAX_ZOOM: f64 = 8.0;

    #[cfg(test)]
    pub const fn zoom(self) -> f64 {
        self.zoom
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Zooms around a screen anchor so the document point under it stays fixed.
    pub fn zoom_at(
        &mut self,
        factor: f64,
        anchor: DVec2,
        fit_origin: DVec2,
        fit_zoom: f64,
    ) -> bool {
        if !factor.is_finite() || factor <= 0.0 || !fit_zoom.is_finite() || fit_zoom <= 0.0 {
            return false;
        }
        let next_zoom = (self.zoom * factor).clamp(Self::MIN_ZOOM, Self::MAX_ZOOM);
        if next_zoom == self.zoom {
            return false;
        }
        let offset = anchor - fit_origin - self.pan;
        self.pan = anchor - fit_origin - offset * (next_zoom / self.zoom);
        self.zoom = next_zoom;
        true
    }

    pub fn pan_by(&mut self, delta: DVec2) {
        if delta.is_finite() {
            self.pan += delta;
        }
    }
}

/// Screen-space rectangle occupied by the fitted document canvas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanvasViewport {
    transform: ViewportTransform,
    screen_width: f64,
    screen_height: f64,
    screen_origin: DVec2,
    screen_size: DVec2,
    document_size: DVec2,
}

impl CanvasViewport {
    /// Fits the document into the physical window without changing its aspect ratio.
    pub fn fit(
        document_width: u32,
        document_height: u32,
        screen_width: u32,
        screen_height: u32,
    ) -> Option<Self> {
        Self::fit_view(
            document_width,
            document_height,
            screen_width,
            screen_height,
            ViewState::default(),
        )
    }

    /// Fits the document, then applies the user zoom and pan on top of that fit.
    pub fn fit_view(
        document_width: u32,
        document_height: u32,
        screen_width: u32,
        screen_height: u32,
        view: ViewState,
    ) -> Option<Self> {
        if document_width == 0 || document_height == 0 || screen_width == 0 || screen_height == 0 {
            return None;
        }

        let document_size = DVec2::new(f64::from(document_width), f64::from(document_height));
        let screen_size = DVec2::new(f64::from(screen_width), f64::from(screen_height));
        let fit_zoom = (screen_size.x / document_size.x).min(screen_size.y / document_size.y);
        let fit_origin = (screen_size - document_size * fit_zoom) * 0.5;
        let zoom = fit_zoom * view.zoom;
        let screen_origin = fit_origin + view.pan;
        let transform = ViewportTransform::new(zoom, 0.0, screen_origin).ok()?;

        Some(Self {
            transform,
            screen_width: screen_size.x,
            screen_height: screen_size.y,
            screen_origin,
            screen_size: document_size * zoom,
            document_size,
        })
    }

    #[cfg(test)]
    pub const fn transform(self) -> ViewportTransform {
        self.transform
    }

    #[cfg(test)]
    pub const fn screen_origin(self) -> DVec2 {
        self.screen_origin
    }

    #[cfg(test)]
    pub const fn screen_size(self) -> DVec2 {
        self.screen_size
    }

    /// Maps a point only when it is inside the fitted canvas, excluding letterbox space.
    pub fn screen_to_document(self, point: DVec2) -> Option<DVec2> {
        // Half-open rect: the right/bottom edges belong to the excluded letterbox.
        if !point.x.is_finite()
            || !point.y.is_finite()
            || point.x < self.screen_origin.x
            || point.y < self.screen_origin.y
            || point.x >= self.screen_origin.x + self.screen_size.x
            || point.y >= self.screen_origin.y + self.screen_size.y
        {
            return None;
        }

        let document_point = self.transform.screen_to_canvas(point);
        if document_point.x < 0.0
            || document_point.y < 0.0
            || document_point.x >= self.document_size.x
            || document_point.y >= self.document_size.y
        {
            return None;
        }

        Some(document_point)
    }

    /// Maps an active pointer to the nearest canvas edge after it leaves the canvas.
    pub fn screen_to_document_clamped(self, point: DVec2) -> Option<DVec2> {
        if !point.x.is_finite() || !point.y.is_finite() {
            return None;
        }
        let document_point = self.transform.screen_to_canvas(point);
        Some(document_point.clamp(DVec2::ZERO, self.document_size))
    }

    /// Converts a document point into normalized GPU clip coordinates.
    pub fn document_to_clip(self, point: DVec2) -> [f32; 2] {
        let screen = self.transform.canvas_to_screen(point);
        [
            (screen.x / self.screen_width * 2.0 - 1.0) as f32,
            (1.0 - screen.y / self.screen_height * 2.0) as f32,
        ]
    }

    /// Returns an integer scissor rectangle that clips drawing to the fitted canvas.
    pub fn scissor_rect(self) -> (u32, u32, u32, u32) {
        let left = self.screen_origin.x.floor().clamp(0.0, self.screen_width) as u32;
        let top = self.screen_origin.y.floor().clamp(0.0, self.screen_height) as u32;
        let right = (self.screen_origin.x + self.screen_size.x)
            .ceil()
            .clamp(0.0, self.screen_width) as u32;
        let bottom = (self.screen_origin.y + self.screen_size.y)
            .ceil()
            .clamp(0.0, self.screen_height) as u32;
        (
            left,
            top,
            right.saturating_sub(left),
            bottom.saturating_sub(top),
        )
    }
}

/// Mutable document and command history owned by the desktop drawing session.
#[derive(Debug)]
pub struct CanvasDocument {
    document: Document,
    history: History,
    viewport: CanvasViewport,
    view: ViewState,
    screen_width: u32,
    screen_height: u32,
}

impl CanvasDocument {
    pub fn new(screen_width: u32, screen_height: u32) -> Result<Self, DocumentError> {
        let document = Document::new(DOCUMENT_WIDTH, DOCUMENT_HEIGHT)?;
        let viewport =
            CanvasViewport::fit(DOCUMENT_WIDTH, DOCUMENT_HEIGHT, screen_width, screen_height)
                .expect("a desktop canvas requires a non-zero window size");

        Ok(Self {
            document,
            history: History::default(),
            viewport,
            view: ViewState::default(),
            screen_width,
            screen_height,
        })
    }

    pub fn document(&self) -> &Document {
        &self.document
    }

    /// Replaces the project document and starts a fresh editing history.
    pub fn replace_document(&mut self, document: Document) {
        self.document = document;
        self.history.clear();
        self.reset_view();
    }

    /// Adds a visible layer at the top of the stack and records it in history.
    pub fn add_layer(&mut self, name: impl Into<String>) -> Result<LayerId, DocumentError> {
        match self
            .history
            .execute(&mut self.document, Edit::add_layer(name))?
        {
            EditResult::LayerAdded(id) => Ok(id),
            EditResult::None | EditResult::StrokeAdded(_) => {
                unreachable!("adding a layer must return its new identifier")
            }
        }
    }

    /// Changes layer visibility as an undoable document edit.
    pub fn set_layer_visibility(
        &mut self,
        id: LayerId,
        visible: bool,
    ) -> Result<(), DocumentError> {
        let layer = self
            .document
            .layer(id)
            .ok_or(DocumentError::LayerNotFound)?;
        if layer.is_visible() == visible {
            return Ok(());
        }
        self.history
            .execute(&mut self.document, Edit::set_layer_visibility(id, visible))?;
        Ok(())
    }

    /// Removes a layer through the document history, preserving undo support.
    pub fn remove_layer(&mut self, id: LayerId) -> Result<(), DocumentError> {
        self.history
            .execute(&mut self.document, Edit::remove_layer(id))?;
        Ok(())
    }

    pub const fn viewport(&self) -> CanvasViewport {
        self.viewport
    }

    pub fn resize_viewport(&mut self, screen_width: u32, screen_height: u32) {
        self.screen_width = screen_width;
        self.screen_height = screen_height;
        self.refresh_viewport();
    }

    /// Zooms around a screen-space anchor and returns whether the view changed.
    pub fn zoom_at(&mut self, factor: f64, anchor: DVec2) -> bool {
        let (fit_origin, fit_zoom) = self.fit_basis();
        if !self.view.zoom_at(factor, anchor, fit_origin, fit_zoom) {
            return false;
        }
        self.refresh_viewport();
        true
    }

    pub fn pan_by(&mut self, delta: DVec2) {
        if !delta.is_finite() || delta == DVec2::ZERO {
            return;
        }
        self.view.pan_by(delta);
        self.refresh_viewport();
    }

    pub fn reset_view(&mut self) {
        self.view.reset();
        self.refresh_viewport();
    }

    #[cfg(test)]
    pub const fn view(&self) -> ViewState {
        self.view
    }

    fn fit_basis(&self) -> (DVec2, f64) {
        let document_size = DVec2::new(
            f64::from(self.document.width_px()),
            f64::from(self.document.height_px()),
        );
        let screen_size = DVec2::new(f64::from(self.screen_width), f64::from(self.screen_height));
        let fit_zoom = (screen_size.x / document_size.x).min(screen_size.y / document_size.y);
        ((screen_size - document_size * fit_zoom) * 0.5, fit_zoom)
    }

    fn refresh_viewport(&mut self) {
        if let Some(viewport) = CanvasViewport::fit_view(
            self.document.width_px(),
            self.document.height_px(),
            self.screen_width,
            self.screen_height,
            self.view,
        ) {
            self.viewport = viewport;
        }
    }

    pub fn commit_stroke(
        &mut self,
        samples: Vec<StrokeSample>,
        brush: BrushConfig,
    ) -> Result<(), DocumentError> {
        let layer_id = self
            .document
            .layers()
            .last()
            .expect("a document always contains one layer")
            .id();
        self.commit_stroke_to_with_color(layer_id, samples, brush, StrokeColor::DEFAULT)
            .map(|_| ())
    }

    /// Commits a stroke to a specific layer using the legacy default colour.
    #[cfg(test)]
    pub fn commit_stroke_to(
        &mut self,
        layer_id: LayerId,
        samples: Vec<StrokeSample>,
        brush: BrushConfig,
    ) -> Result<Option<u64>, DocumentError> {
        self.commit_stroke_to_with_color(layer_id, samples, brush, StrokeColor::DEFAULT)
    }

    /// Commits a coloured stroke to a specific layer and returns its stable identifier.
    pub fn commit_stroke_to_with_color(
        &mut self,
        layer_id: LayerId,
        samples: Vec<StrokeSample>,
        brush: BrushConfig,
        color: StrokeColor,
    ) -> Result<Option<u64>, DocumentError> {
        if samples.is_empty() {
            return Ok(None);
        }

        match self.history.execute(
            &mut self.document,
            Edit::add_stroke_with_color(layer_id, samples, brush, color),
        )? {
            EditResult::StrokeAdded(id) => Ok(Some(id.get())),
            EditResult::None | EditResult::LayerAdded(_) => {
                unreachable!("adding a stroke must return its new identifier")
            }
        }
    }

    pub fn undo(&mut self) -> Result<(), DocumentError> {
        self.history.undo(&mut self.document)
    }

    pub fn redo(&mut self) -> Result<(), DocumentError> {
        self.history.redo(&mut self.document)
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    /// Returns the id of the most recent stroke on the top layer.
    #[cfg(test)]
    pub fn last_stroke_id(&self) -> Option<u64> {
        let layer = self.document.layers().last()?;
        layer.strokes().last().map(|stroke| stroke.id().get())
    }

    #[cfg(test)]
    pub fn last_stroke_id_on(&self, layer_id: LayerId) -> Option<u64> {
        self.document
            .layer(layer_id)?
            .strokes()
            .last()
            .map(|stroke| stroke.id().get())
    }
}

/// Current pen or eraser choice, shared by mouse and pen input.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BrushSettings {
    radius: f64,
    color: [f32; 4],
    eraser: bool,
}

pub const MIN_BRUSH_RADIUS: f64 = 1.0;
pub const MAX_BRUSH_RADIUS: f64 = 128.0;
pub const BRUSH_RADIUS_STEP: f64 = 1.25;

/// shortcut: the eraser paints the canvas background colour instead of
/// blending the layer away; swap it for a real erase blend once the renderer
/// composites layers with alpha.
pub const ERASER_COLOR: [f32; 4] = [0.055, 0.065, 0.075, 1.0];

pub const PALETTE: [[f32; 4]; 4] = [
    [0.10, 0.78, 0.92, 1.0],
    [0.96, 0.96, 0.98, 1.0],
    [0.95, 0.36, 0.36, 1.0],
    [0.98, 0.83, 0.35, 1.0],
];

impl Default for BrushSettings {
    fn default() -> Self {
        Self {
            radius: 8.0,
            color: PALETTE[0],
            eraser: false,
        }
    }
}

impl BrushSettings {
    #[cfg(test)]
    pub const fn radius(self) -> f64 {
        self.radius
    }

    /// Colour written by the next stroke; the eraser overrides the palette.
    pub const fn color(self) -> [f32; 4] {
        if self.eraser {
            ERASER_COLOR
        } else {
            self.color
        }
    }

    #[cfg(test)]
    pub const fn is_eraser(self) -> bool {
        self.eraser
    }

    pub fn set_eraser(&mut self, eraser: bool) -> bool {
        let changed = self.eraser != eraser;
        self.eraser = eraser;
        changed
    }

    pub fn scale_radius(&mut self, factor: f64) -> bool {
        if !factor.is_finite() || factor <= 0.0 {
            return false;
        }
        let radius = (self.radius * factor).clamp(MIN_BRUSH_RADIUS, MAX_BRUSH_RADIUS);
        let changed = radius != self.radius;
        self.radius = radius;
        changed
    }

    pub fn select_color(&mut self, index: usize) -> bool {
        let Some(color) = PALETTE.get(index) else {
            return false;
        };
        let changed = self.color != *color || self.eraser;
        self.color = *color;
        self.eraser = false;
        changed
    }

    pub fn brush(self) -> BrushConfig {
        BrushConfig::default()
            .with_base_radius(self.radius)
            .expect("brush radius stays inside the validated range")
    }

    pub fn status(self) -> String {
        let tool = if self.eraser { "eraser" } else { "pen" };
        format!("{tool} {:.0}px", self.radius)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BRUSH_RADIUS_STEP, BrushSettings, CanvasDocument, CanvasViewport, DOCUMENT_HEIGHT,
        DOCUMENT_WIDTH, ERASER_COLOR, MAX_BRUSH_RADIUS, MIN_BRUSH_RADIUS, PALETTE, ViewState,
    };
    use glam::DVec2;
    use rasm_engine_document::{Document, DocumentError, StrokeColor};
    use rasm_engine_ink::{BrushConfig, StrokeSample};

    fn sample_stroke() -> Vec<StrokeSample> {
        vec![
            StrokeSample::new(DVec2::new(100.0, 200.0), Some(0.4), 1).expect("first sample"),
            StrokeSample::new(DVec2::new(140.0, 220.0), Some(0.8), 2).expect("second sample"),
        ]
    }

    #[test]
    fn fit_preserves_document_aspect_ratio_and_centers_letterbox() {
        let viewport =
            CanvasViewport::fit(DOCUMENT_WIDTH, DOCUMENT_HEIGHT, 960, 640).expect("valid viewport");
        assert!(
            (viewport.screen_size().x / viewport.screen_size().y
                - f64::from(DOCUMENT_WIDTH) / f64::from(DOCUMENT_HEIGHT))
            .abs()
                < 1.0e-9
        );
        assert!((viewport.screen_origin().x - 0.0).abs() < 1.0e-9);
        assert!(viewport.screen_origin().y > 0.0);
    }

    #[test]
    fn screen_mapping_round_trips_and_rejects_letterbox_space() {
        let viewport =
            CanvasViewport::fit(DOCUMENT_WIDTH, DOCUMENT_HEIGHT, 960, 640).expect("valid viewport");
        let document_point = DVec2::new(480.0, 540.0);
        let screen_point = viewport.transform().canvas_to_screen(document_point);
        let mapped = viewport
            .screen_to_document(screen_point)
            .expect("inside canvas");
        assert!(mapped.distance(document_point) < 1.0e-8);
        assert!(
            viewport
                .screen_to_document(DVec2::new(480.0, 10.0))
                .is_none()
        );
    }

    #[test]
    fn changing_window_aspect_ratio_does_not_change_document_coordinates() {
        let wide = CanvasViewport::fit(DOCUMENT_WIDTH, DOCUMENT_HEIGHT, 1_600, 900)
            .expect("wide viewport");
        let narrow = CanvasViewport::fit(DOCUMENT_WIDTH, DOCUMENT_HEIGHT, 900, 1_000)
            .expect("narrow viewport");
        let point = DVec2::new(700.0, 350.0);
        let wide_screen = wide.transform().canvas_to_screen(point);
        let narrow_screen = narrow.transform().canvas_to_screen(point);
        assert_ne!(wide_screen, narrow_screen);
        assert!(
            wide.screen_to_document(wide_screen)
                .unwrap()
                .distance(point)
                < 1.0e-8
        );
        assert!(
            narrow
                .screen_to_document(narrow_screen)
                .unwrap()
                .distance(point)
                < 1.0e-8
        );
    }

    #[test]
    fn committed_strokes_are_undoable_and_redoable() {
        let mut canvas = CanvasDocument::new(960, 640).expect("canvas");
        canvas
            .commit_stroke(sample_stroke(), BrushConfig::default())
            .expect("commit");
        assert_eq!(canvas.document().layers()[0].strokes().len(), 1);
        assert!(canvas.can_undo());

        canvas.undo().expect("undo");
        assert!(canvas.document().layers()[0].strokes().is_empty());
        assert!(canvas.can_redo());

        canvas.redo().expect("redo");
        assert_eq!(canvas.document().layers()[0].strokes().len(), 1);
    }

    #[test]
    fn empty_input_does_not_create_a_history_entry() {
        let mut canvas = CanvasDocument::new(960, 640).expect("canvas");
        canvas
            .commit_stroke(Vec::new(), BrushConfig::default())
            .expect("empty no-op");
        assert!(!canvas.can_undo());
    }

    #[test]
    fn zoom_keeps_the_document_point_under_the_cursor_fixed() {
        let mut canvas = CanvasDocument::new(960, 640).expect("canvas");
        let document_point = DVec2::new(700.0, 350.0);
        let anchor = canvas
            .viewport()
            .transform()
            .canvas_to_screen(document_point);

        assert!(canvas.zoom_at(2.0, anchor));
        assert!((canvas.view().zoom() - 2.0).abs() < 1.0e-9);
        let mapped = canvas
            .viewport()
            .screen_to_document(anchor)
            .expect("anchor stays on the canvas");
        assert!(mapped.distance(document_point) < 1.0e-8);
    }

    #[test]
    fn zoom_is_clamped_to_the_supported_range() {
        let mut canvas = CanvasDocument::new(960, 640).expect("canvas");
        let anchor = DVec2::new(480.0, 320.0);
        for _ in 0..32 {
            canvas.zoom_at(2.0, anchor);
        }
        assert!((canvas.view().zoom() - ViewState::MAX_ZOOM).abs() < 1.0e-9);
        for _ in 0..64 {
            canvas.zoom_at(0.5, anchor);
        }
        assert!((canvas.view().zoom() - ViewState::MIN_ZOOM).abs() < 1.0e-9);
        assert!(!canvas.zoom_at(1.0, anchor));
    }

    #[test]
    fn pan_shifts_the_screen_mapping_by_the_same_delta() {
        let mut canvas = CanvasDocument::new(960, 640).expect("canvas");
        let document_point = DVec2::new(300.0, 400.0);
        let before = canvas
            .viewport()
            .transform()
            .canvas_to_screen(document_point);
        let delta = DVec2::new(35.0, -20.0);
        canvas.pan_by(delta);
        let after = canvas
            .viewport()
            .transform()
            .canvas_to_screen(document_point);
        assert!((after - before - delta).length() < 1.0e-8);
    }

    #[test]
    fn reset_view_restores_the_fitted_mapping() {
        let mut canvas = CanvasDocument::new(960, 640).expect("canvas");
        let anchor = DVec2::new(500.0, 360.0);
        canvas.zoom_at(3.0, anchor);
        canvas.pan_by(DVec2::new(40.0, 25.0));
        canvas.reset_view();

        let fitted = CanvasViewport::fit(DOCUMENT_WIDTH, DOCUMENT_HEIGHT, 960, 640)
            .expect("fitted viewport");
        assert_eq!(canvas.viewport().transform(), fitted.transform());
    }

    #[test]
    fn hit_testing_excludes_the_far_edges() {
        let canvas = CanvasDocument::new(960, 640).expect("canvas");
        let viewport = canvas.viewport();
        let far_edge = viewport.screen_origin() + viewport.screen_size();
        assert!(viewport.screen_to_document(far_edge).is_none());
        assert!(
            viewport
                .screen_to_document(far_edge - DVec2::new(1.0, 1.0))
                .is_some()
        );
    }

    #[test]
    fn brush_radius_stays_inside_the_supported_range() {
        let mut brush = BrushSettings::default();
        for _ in 0..64 {
            brush.scale_radius(BRUSH_RADIUS_STEP);
        }
        assert!((brush.radius() - MAX_BRUSH_RADIUS).abs() < 1.0e-9);
        for _ in 0..64 {
            brush.scale_radius(1.0 / BRUSH_RADIUS_STEP);
        }
        assert!((brush.radius() - MIN_BRUSH_RADIUS).abs() < 1.0e-9);
        assert!(!brush.scale_radius(f64::NAN));
    }

    #[test]
    fn eraser_paints_the_canvas_background_colour() {
        let mut brush = BrushSettings::default();
        assert_eq!(brush.color(), PALETTE[0]);
        assert!(brush.set_eraser(true));
        assert_eq!(brush.color(), ERASER_COLOR);
        assert!(brush.status().starts_with("eraser"));
    }

    #[test]
    fn palette_selection_leaves_eraser_mode() {
        let mut brush = BrushSettings::default();
        brush.set_eraser(true);
        assert!(brush.select_color(2));
        assert!(!brush.is_eraser());
        assert_eq!(brush.color(), PALETTE[2]);
        assert!(!brush.select_color(99));
    }

    #[test]
    fn committed_stroke_exposes_its_id_for_colouring() {
        let mut canvas = CanvasDocument::new(960, 640).expect("canvas");
        canvas
            .commit_stroke(sample_stroke(), BrushSettings::default().brush())
            .expect("committed stroke");
        assert!(canvas.last_stroke_id().is_some());
    }

    #[test]
    fn selected_layer_receives_strokes_and_history_restores_them() {
        let mut canvas = CanvasDocument::new(960, 640).expect("canvas");
        let original = canvas.document().layers()[0].id();
        let selected = canvas.add_layer("Sketch").expect("new layer");

        let stroke_id = canvas
            .commit_stroke_to(selected, sample_stroke(), BrushConfig::default())
            .expect("commit to selected layer")
            .expect("non-empty stroke");
        assert_eq!(canvas.last_stroke_id_on(selected), Some(stroke_id));
        assert!(
            canvas
                .document()
                .layer(original)
                .unwrap()
                .strokes()
                .is_empty()
        );
        assert_eq!(
            canvas.document().layer(selected).unwrap().strokes().len(),
            1
        );

        canvas.undo().expect("undo stroke");
        assert!(
            canvas
                .document()
                .layer(selected)
                .unwrap()
                .strokes()
                .is_empty()
        );
        canvas.redo().expect("redo stroke");
        assert_eq!(
            canvas.document().layer(selected).unwrap().strokes().len(),
            1
        );
    }

    #[test]
    fn layer_add_visibility_and_removal_are_undoable() {
        let mut canvas = CanvasDocument::new(960, 640).expect("canvas");
        let first = canvas.document().layers()[0].id();
        let second = canvas.add_layer("Ink").expect("add layer");
        assert_eq!(canvas.document().layers().len(), 2);

        canvas
            .set_layer_visibility(second, false)
            .expect("hide layer");
        assert!(!canvas.document().layer(second).unwrap().is_visible());
        canvas.undo().expect("undo visibility");
        assert!(canvas.document().layer(second).unwrap().is_visible());
        canvas.redo().expect("redo visibility");
        assert!(!canvas.document().layer(second).unwrap().is_visible());

        canvas.remove_layer(second).expect("remove layer");
        assert!(canvas.document().layer(second).is_none());
        canvas.undo().expect("undo removal");
        assert!(canvas.document().layer(second).is_some());
        canvas.redo().expect("redo removal");
        assert!(canvas.document().layer(second).is_none());
        assert!(canvas.document().layer(first).is_some());
    }

    #[test]
    fn removing_the_last_layer_is_rejected() {
        let mut canvas = CanvasDocument::new(960, 640).expect("canvas");
        let only_layer = canvas.document().layers()[0].id();
        assert_eq!(
            canvas.remove_layer(only_layer),
            Err(DocumentError::CannotRemoveLastLayer)
        );
        assert_eq!(canvas.document().layers().len(), 1);
    }

    #[test]
    fn replacing_document_clears_history_and_refits_viewport() {
        let mut canvas = CanvasDocument::new(960, 640).expect("canvas");
        canvas
            .add_layer("Temporary")
            .expect("create temporary history entry");
        canvas.zoom_at(2.0, DVec2::new(480.0, 320.0));
        let replacement = Document::new(800, 600).expect("replacement");

        canvas.replace_document(replacement.clone());

        assert_eq!(canvas.document(), &replacement);
        assert!(!canvas.can_undo());
        assert!(!canvas.can_redo());
        assert_eq!(
            canvas.viewport().transform(),
            CanvasViewport::fit(800, 600, 960, 640)
                .expect("replacement viewport")
                .transform()
        );
    }

    #[test]
    fn explicit_stroke_colour_is_stored_in_document_and_undo_history() {
        let mut canvas = CanvasDocument::new(960, 640).expect("canvas");
        let layer = canvas.document().layers()[0].id();
        let colour = StrokeColor::new([0.25, 0.5, 0.75, 1.0]).expect("colour");

        canvas
            .commit_stroke_to_with_color(layer, sample_stroke(), BrushConfig::default(), colour)
            .expect("commit coloured stroke");

        assert_eq!(
            canvas.document().layer(layer).unwrap().strokes()[0].color(),
            colour
        );
        canvas.undo().expect("undo");
        assert!(canvas.document().layer(layer).unwrap().strokes().is_empty());
        canvas.redo().expect("redo");
        assert_eq!(
            canvas.document().layer(layer).unwrap().strokes()[0].color(),
            colour
        );
    }

    #[test]
    fn resize_keeps_the_user_zoom() {
        let mut canvas = CanvasDocument::new(960, 640).expect("canvas");
        canvas.zoom_at(2.0, DVec2::new(480.0, 320.0));
        canvas.resize_viewport(1_600, 900);
        assert!((canvas.view().zoom() - 2.0).abs() < 1.0e-9);
        assert!(
            canvas
                .viewport()
                .screen_to_document(DVec2::new(800.0, 450.0))
                .is_some()
        );
    }
}
