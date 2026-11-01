//! Document-backed desktop canvas state and viewport mapping.

use glam::DVec2;
use rasm_engine_core::transform::ViewportTransform;
use rasm_engine_document::{Document, DocumentError, Edit, History};
use rasm_engine_ink::{BrushConfig, StrokeSample};

/// Initial logical canvas size for the desktop prototype.
pub const DOCUMENT_WIDTH: u32 = 1_920;
pub const DOCUMENT_HEIGHT: u32 = 1_080;

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
        if document_width == 0 || document_height == 0 || screen_width == 0 || screen_height == 0 {
            return None;
        }

        let document_size = DVec2::new(f64::from(document_width), f64::from(document_height));
        let screen_size = DVec2::new(f64::from(screen_width), f64::from(screen_height));
        let zoom = (screen_size.x / document_size.x).min(screen_size.y / document_size.y);
        let fitted_size = document_size * zoom;
        let screen_origin = (screen_size - fitted_size) * 0.5;
        let transform = ViewportTransform::new(zoom, 0.0, screen_origin).ok()?;

        Some(Self {
            transform,
            screen_width: screen_size.x,
            screen_height: screen_size.y,
            screen_origin,
            screen_size: fitted_size,
            document_size,
        })
    }

    pub const fn transform(self) -> ViewportTransform {
        self.transform
    }

    pub const fn screen_origin(self) -> DVec2 {
        self.screen_origin
    }

    pub const fn screen_size(self) -> DVec2 {
        self.screen_size
    }

    /// Maps a point only when it is inside the fitted canvas, excluding letterbox space.
    pub fn screen_to_document(self, point: DVec2) -> Option<DVec2> {
        if !point.x.is_finite()
            || !point.y.is_finite()
            || point.x < self.screen_origin.x
            || point.y < self.screen_origin.y
            || point.x > self.screen_origin.x + self.screen_size.x
            || point.y > self.screen_origin.y + self.screen_size.y
        {
            return None;
        }

        let document_point = self.transform.screen_to_canvas(point);
        if document_point.x < 0.0
            || document_point.y < 0.0
            || document_point.x > self.document_size.x
            || document_point.y > self.document_size.y
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
        })
    }

    pub fn document(&self) -> &Document {
        &self.document
    }

    pub const fn viewport(&self) -> CanvasViewport {
        self.viewport
    }

    pub fn resize_viewport(&mut self, screen_width: u32, screen_height: u32) {
        if let Some(viewport) = CanvasViewport::fit(
            self.document.width_px(),
            self.document.height_px(),
            screen_width,
            screen_height,
        ) {
            self.viewport = viewport;
        }
    }

    pub fn commit_stroke(
        &mut self,
        samples: Vec<StrokeSample>,
        brush: BrushConfig,
    ) -> Result<(), DocumentError> {
        if samples.is_empty() {
            return Ok(());
        }
        let layer_id = self
            .document
            .layers()
            .last()
            .expect("a document always contains one layer")
            .id();
        self.history.execute(
            &mut self.document,
            Edit::add_stroke(layer_id, samples, brush),
        )?;
        Ok(())
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
}

#[cfg(test)]
mod tests {
    use super::{CanvasDocument, CanvasViewport, DOCUMENT_HEIGHT, DOCUMENT_WIDTH};
    use glam::DVec2;
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
}
