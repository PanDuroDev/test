//! Canvas-to-screen transforms used by pan, zoom, and rotation.

use glam::DVec2;

/// A 2D view transform with a positive uniform zoom and a rotation in radians.
///
/// Document coordinates remain in f64. Conversion to f32 should happen only
/// when values are packed for a GPU buffer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportTransform {
    zoom: f64,
    rotation_radians: f64,
    translation: DVec2,
}

/// Errors produced by invalid viewport transforms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransformError {
    NonFiniteValue,
    NonPositiveZoom,
}

impl ViewportTransform {
    /// Creates a viewport transform.
    pub fn new(
        zoom: f64,
        rotation_radians: f64,
        translation: DVec2,
    ) -> Result<Self, TransformError> {
        if !zoom.is_finite()
            || !rotation_radians.is_finite()
            || !translation.x.is_finite()
            || !translation.y.is_finite()
        {
            return Err(TransformError::NonFiniteValue);
        }
        if zoom <= 0.0 {
            return Err(TransformError::NonPositiveZoom);
        }

        Ok(Self {
            zoom,
            rotation_radians,
            translation,
        })
    }

    pub const fn zoom(self) -> f64 {
        self.zoom
    }

    pub const fn rotation_radians(self) -> f64 {
        self.rotation_radians
    }

    pub const fn translation(self) -> DVec2 {
        self.translation
    }

    /// Maps a point from document space to screen space.
    pub fn canvas_to_screen(self, point: DVec2) -> DVec2 {
        self.rotate(point * self.zoom) + self.translation
    }

    /// Maps a point from screen space back into document space.
    pub fn screen_to_canvas(self, point: DVec2) -> DVec2 {
        self.rotate_inverse(point - self.translation) / self.zoom
    }

    /// Changes zoom while keeping screen_anchor over the same document point.
    pub fn zoom_around(
        &mut self,
        new_zoom: f64,
        screen_anchor: DVec2,
    ) -> Result<(), TransformError> {
        if !new_zoom.is_finite()
            || !screen_anchor.x.is_finite()
            || !screen_anchor.y.is_finite()
        {
            return Err(TransformError::NonFiniteValue);
        }
        if new_zoom <= 0.0 {
            return Err(TransformError::NonPositiveZoom);
        }

        let anchored_document_point = self.screen_to_canvas(screen_anchor);
        self.zoom = new_zoom;
        self.translation = screen_anchor - self.rotate(anchored_document_point * self.zoom);
        Ok(())
    }

    /// Changes rotation while keeping screen_anchor over the same document point.
    pub fn rotate_around(
        &mut self,
        new_rotation_radians: f64,
        screen_anchor: DVec2,
    ) -> Result<(), TransformError> {
        if !new_rotation_radians.is_finite()
            || !screen_anchor.x.is_finite()
            || !screen_anchor.y.is_finite()
        {
            return Err(TransformError::NonFiniteValue);
        }

        let anchored_document_point = self.screen_to_canvas(screen_anchor);
        self.rotation_radians = new_rotation_radians;
        self.translation = screen_anchor - self.rotate(anchored_document_point * self.zoom);
        Ok(())
    }

    fn rotate(self, point: DVec2) -> DVec2 {
        let (sin, cos) = self.rotation_radians.sin_cos();
        DVec2::new(
            cos.mul_add(point.x, -sin * point.y),
            sin.mul_add(point.x, cos * point.y),
        )
    }

    fn rotate_inverse(self, point: DVec2) -> DVec2 {
        let (sin, cos) = self.rotation_radians.sin_cos();
        DVec2::new(
            cos.mul_add(point.x, sin * point.y),
            (-sin).mul_add(point.x, cos * point.y),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{TransformError, ViewportTransform};
    use glam::DVec2;

    const EPSILON: f64 = 1.0e-9;

    fn assert_vec2_near(actual: DVec2, expected: DVec2) {
        assert!(
            actual.distance(expected) <= EPSILON,
            "expected {expected:?}, got {actual:?}"
        );
    }

    #[test]
    fn canvas_screen_round_trip_survives_pan_zoom_and_rotation() {
        let transform = ViewportTransform::new(
            2.75,
            0.73,
            DVec2::new(120.0, -42.0),
        )
        .expect("valid transform");
        let point = DVec2::new(1_000_000.25, -900_000.5);

        let screen = transform.canvas_to_screen(point);
        let restored = transform.screen_to_canvas(screen);

        assert_vec2_near(restored, point);
    }

    #[test]
    fn zoom_keeps_the_anchor_fixed_on_screen() {
        let mut transform = ViewportTransform::new(
            1.5,
            0.4,
            DVec2::new(80.0, 25.0),
        )
        .expect("valid transform");
        let anchor = DVec2::new(320.0, 180.0);

        transform.zoom_around(3.25, anchor).expect("valid zoom");

        assert_vec2_near(transform.canvas_to_screen(transform.screen_to_canvas(anchor)), anchor);
    }

    #[test]
    fn rotation_keeps_the_anchor_fixed_on_screen() {
        let mut transform = ViewportTransform::new(
            2.0,
            0.2,
            DVec2::new(12.0, 18.0),
        )
        .expect("valid transform");
        let anchor = DVec2::new(450.0, 250.0);
        let document_point = transform.screen_to_canvas(anchor);

        transform.rotate_around(1.2, anchor).expect("valid rotation");

        assert_vec2_near(transform.canvas_to_screen(document_point), anchor);
    }

    #[test]
    fn rejects_invalid_zoom_and_non_finite_values() {
        assert_eq!(
            ViewportTransform::new(0.0, 0.0, DVec2::ZERO),
            Err(TransformError::NonPositiveZoom)
        );
        assert_eq!(
            ViewportTransform::new(f64::NAN, 0.0, DVec2::ZERO),
            Err(TransformError::NonFiniteValue)
        );

        let mut transform =
            ViewportTransform::new(1.0, 0.0, DVec2::ZERO).expect("valid transform");
        assert_eq!(
            transform.zoom_around(-1.0, DVec2::ZERO),
            Err(TransformError::NonPositiveZoom)
        );
        assert_eq!(
            transform.rotate_around(f64::INFINITY, DVec2::ZERO),
            Err(TransformError::NonFiniteValue)
        );
    }
}
