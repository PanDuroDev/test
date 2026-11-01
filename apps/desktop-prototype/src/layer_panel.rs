//! Lightweight, renderer-independent layer panel geometry and hit testing.
//!
//! The prototype intentionally keeps its UI dependency-free. This module
//! emits simple colored triangles and small bitmap labels for the WGPU pass.

use glam::DVec2;
use rasm_engine_document::{Document, LayerId};

const PANEL_X: f32 = 14.0;
const PANEL_Y: f32 = 14.0;
const PANEL_WIDTH: f32 = 232.0;
const HEADER_HEIGHT: f32 = 32.0;
const ROW_HEIGHT: f32 = 28.0;
const PANEL_BOTTOM_PADDING: f32 = 7.0;
const MAX_VISIBLE_ROWS: usize = 14;
const TEXT_SCALE: f32 = 1.25;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerPanelAction {
    Add,
    Select(LayerId),
    ToggleVisibility(LayerId),
    Remove(LayerId),
}

#[derive(Debug, Clone, Copy)]
pub struct PanelVertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
}

#[derive(Debug, Clone, Copy)]
struct PanelLayout {
    left: f32,
    top: f32,
    right: f32,
    row_top: f32,
    bottom: f32,
    visible_rows: usize,
    scroll_offset: usize,
}

fn layout(
    screen_width: u32,
    screen_height: u32,
    layer_count: usize,
    scroll: usize,
) -> Option<PanelLayout> {
    if screen_width < 160 || screen_height < 96 || layer_count == 0 {
        return None;
    }

    let width = PANEL_WIDTH.min(screen_width.saturating_sub(28) as f32);
    if width < 120.0 {
        return None;
    }

    let available = (screen_height as f32 - PANEL_Y - HEADER_HEIGHT - PANEL_BOTTOM_PADDING - 12.0)
        .max(ROW_HEIGHT);
    let rows_by_height = (available / ROW_HEIGHT).floor() as usize;
    let visible_rows = layer_count.min(rows_by_height.max(1)).min(MAX_VISIBLE_ROWS);
    let row_top = PANEL_Y + HEADER_HEIGHT;
    let bottom = row_top + visible_rows as f32 * ROW_HEIGHT + PANEL_BOTTOM_PADDING;
    let max_scroll = layer_count.saturating_sub(visible_rows);

    Some(PanelLayout {
        left: PANEL_X,
        top: PANEL_Y,
        right: PANEL_X + width,
        row_top,
        bottom,
        visible_rows,
        scroll_offset: scroll.min(max_scroll),
    })
}

pub fn max_scroll(document: &Document, screen_height: u32) -> usize {
    layout(960, screen_height, document.layers().len(), 0)
        .map(|panel| document.layers().len().saturating_sub(panel.visible_rows))
        .unwrap_or(0)
}

pub fn contains(
    position: DVec2,
    screen_width: u32,
    screen_height: u32,
    document: &Document,
    scroll: usize,
) -> bool {
    let Some(panel) = layout(screen_width, screen_height, document.layers().len(), scroll) else {
        return false;
    };

    position.x >= f64::from(panel.left)
        && position.x < f64::from(panel.right)
        && position.y >= f64::from(panel.top)
        && position.y < f64::from(panel.bottom)
}

pub fn hit_test(
    document: &Document,
    position: DVec2,
    screen_width: u32,
    screen_height: u32,
    scroll: usize,
) -> Option<LayerPanelAction> {
    let panel = layout(screen_width, screen_height, document.layers().len(), scroll)?;
    if position.x < f64::from(panel.left)
        || position.x >= f64::from(panel.right)
        || position.y < f64::from(panel.top)
        || position.y >= f64::from(panel.bottom)
    {
        return None;
    }

    if position.y < f64::from(panel.row_top) {
        return (position.x >= f64::from(panel.right - 60.0)).then_some(LayerPanelAction::Add);
    }

    let row = ((position.y - f64::from(panel.row_top)) / f64::from(ROW_HEIGHT)).floor() as usize;
    if row >= panel.visible_rows {
        return None;
    }

    // The document stores layers bottom-to-top; the panel shows the top layer first.
    let display_index = panel.scroll_offset + row;
    let document_index = document.layers().len().checked_sub(display_index + 1)?;
    let layer = document.layers().get(document_index)?;
    let right = f64::from(panel.right);

    if position.x >= right - 30.0 {
        Some(LayerPanelAction::Remove(layer.id()))
    } else if position.x >= right - 72.0 {
        Some(LayerPanelAction::ToggleVisibility(layer.id()))
    } else {
        Some(LayerPanelAction::Select(layer.id()))
    }
}

pub fn append_geometry(
    document: &Document,
    active_layer: LayerId,
    screen_width: u32,
    screen_height: u32,
    scroll: usize,
    vertices: &mut Vec<PanelVertex>,
    indices: &mut Vec<u32>,
) {
    let Some(panel) = layout(screen_width, screen_height, document.layers().len(), scroll) else {
        return;
    };

    let width = panel.right - panel.left;
    rect(
        vertices,
        indices,
        panel.left - 1.0,
        panel.top - 1.0,
        width + 2.0,
        panel.bottom - panel.top + 2.0,
        [0.025, 0.032, 0.040, 1.0],
    );
    rect(
        vertices,
        indices,
        panel.left,
        panel.top,
        width,
        panel.bottom - panel.top,
        [0.075, 0.090, 0.108, 1.0],
    );
    rect(
        vertices,
        indices,
        panel.left,
        panel.top,
        width,
        HEADER_HEIGHT,
        [0.105, 0.128, 0.150, 1.0],
    );
    draw_text(
        vertices,
        indices,
        panel.left + 10.0,
        panel.top + 11.0,
        "LAYERS",
        [0.88, 0.93, 0.97, 1.0],
    );

    let add_x = panel.right - 54.0;
    rect(
        vertices,
        indices,
        add_x,
        panel.top + 5.0,
        46.0,
        22.0,
        [0.035, 0.35, 0.42, 1.0],
    );
    draw_text(
        vertices,
        indices,
        add_x + 11.0,
        panel.top + 12.0,
        "ADD",
        [0.96, 0.99, 1.0, 1.0],
    );

    for row in 0..panel.visible_rows {
        let display_index = panel.scroll_offset + row;
        let Some(document_index) = document.layers().len().checked_sub(display_index + 1) else {
            continue;
        };
        let Some(layer) = document.layers().get(document_index) else {
            continue;
        };

        let y = panel.row_top + row as f32 * ROW_HEIGHT;
        let selected = layer.id() == active_layer;
        let background = if selected {
            [0.055, 0.245, 0.285, 1.0]
        } else {
            [0.105, 0.123, 0.145, 1.0]
        };
        rect(
            vertices,
            indices,
            panel.left + 1.0,
            y,
            width - 2.0,
            ROW_HEIGHT - 1.0,
            background,
        );

        let name = layer
            .name()
            .chars()
            .take(11)
            .collect::<String>()
            .to_ascii_uppercase();
        draw_text(
            vertices,
            indices,
            panel.left + 9.0,
            y + 10.0,
            &name,
            [0.88, 0.91, 0.94, 1.0],
        );

        let visibility_color = if layer.is_visible() {
            [0.45, 0.86, 0.70, 1.0]
        } else {
            [0.86, 0.61, 0.43, 1.0]
        };
        draw_text(
            vertices,
            indices,
            panel.right - 66.0,
            y + 10.0,
            if layer.is_visible() { "ON" } else { "OFF" },
            visibility_color,
        );
        draw_text(
            vertices,
            indices,
            panel.right - 21.0,
            y + 10.0,
            "X",
            [0.95, 0.55, 0.55, 1.0],
        );

        rect(
            vertices,
            indices,
            panel.right - 74.0,
            y + 4.0,
            1.0,
            ROW_HEIGHT - 9.0,
            [0.20, 0.24, 0.27, 1.0],
        );
        rect(
            vertices,
            indices,
            panel.right - 34.0,
            y + 4.0,
            1.0,
            ROW_HEIGHT - 9.0,
            [0.20, 0.24, 0.27, 1.0],
        );
    }
}

fn rect(
    vertices: &mut Vec<PanelVertex>,
    indices: &mut Vec<u32>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: [f32; 4],
) {
    if width <= 0.0 || height <= 0.0 || vertices.len() > (u32::MAX as usize).saturating_sub(4) {
        return;
    }

    let base = vertices.len() as u32;
    vertices.extend([
        PanelVertex {
            position: [x, y],
            color,
        },
        PanelVertex {
            position: [x + width, y],
            color,
        },
        PanelVertex {
            position: [x + width, y + height],
            color,
        },
        PanelVertex {
            position: [x, y + height],
            color,
        },
    ]);
    indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
}

fn draw_text(
    vertices: &mut Vec<PanelVertex>,
    indices: &mut Vec<u32>,
    x: f32,
    y: f32,
    text: &str,
    color: [f32; 4],
) {
    let mut cursor_x = x;
    for character in text.chars() {
        let rows = glyph(character);
        for (row, bits) in rows.into_iter().enumerate() {
            for column in 0..5 {
                if bits & (1 << (4 - column)) != 0 {
                    rect(
                        vertices,
                        indices,
                        cursor_x + column as f32 * TEXT_SCALE,
                        y + row as f32 * TEXT_SCALE,
                        TEXT_SCALE,
                        TEXT_SCALE,
                        color,
                    );
                }
            }
        }
        cursor_x += 6.0 * TEXT_SCALE;
    }
}

fn glyph(character: char) -> [u8; 7] {
    match character {
        'A' => [14, 17, 17, 31, 17, 17, 17],
        'B' => [30, 17, 17, 30, 17, 17, 30],
        'C' => [14, 17, 16, 16, 16, 17, 14],
        'D' => [30, 17, 17, 17, 17, 17, 30],
        'E' => [31, 16, 16, 30, 16, 16, 31],
        'F' => [31, 16, 16, 30, 16, 16, 16],
        'G' => [14, 17, 16, 23, 17, 17, 15],
        'H' => [17, 17, 17, 31, 17, 17, 17],
        'I' => [14, 4, 4, 4, 4, 4, 14],
        'J' => [7, 2, 2, 2, 18, 18, 12],
        'K' => [17, 18, 20, 24, 20, 18, 17],
        'L' => [16, 16, 16, 16, 16, 16, 31],
        'M' => [17, 27, 21, 21, 17, 17, 17],
        'N' => [17, 25, 25, 21, 19, 19, 17],
        'O' => [14, 17, 17, 17, 17, 17, 14],
        'P' => [30, 17, 17, 30, 16, 16, 16],
        'Q' => [14, 17, 17, 17, 21, 18, 13],
        'R' => [30, 17, 17, 30, 20, 18, 17],
        'S' => [15, 16, 16, 14, 1, 1, 30],
        'T' => [31, 4, 4, 4, 4, 4, 4],
        'U' => [17, 17, 17, 17, 17, 17, 14],
        'V' => [17, 17, 17, 17, 17, 10, 4],
        'W' => [17, 17, 17, 21, 21, 21, 10],
        'X' => [17, 17, 10, 4, 10, 17, 17],
        'Y' => [17, 17, 10, 4, 4, 4, 4],
        'Z' => [31, 1, 2, 4, 8, 16, 31],
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 4, 4, 4, 4, 14],
        '2' => [14, 17, 1, 2, 4, 8, 31],
        '3' => [30, 1, 1, 14, 1, 1, 30],
        '4' => [2, 6, 10, 18, 31, 2, 2],
        '5' => [31, 16, 16, 30, 1, 1, 30],
        '6' => [14, 16, 16, 30, 17, 17, 14],
        '7' => [31, 1, 2, 4, 8, 8, 8],
        '8' => [14, 17, 17, 14, 17, 17, 14],
        '9' => [14, 17, 17, 15, 1, 1, 14],
        '+' => [0, 4, 4, 31, 4, 4, 0],
        '-' => [0, 0, 0, 31, 0, 0, 0],
        ' ' => [0; 7],
        _ => [0; 7],
    }
}

#[cfg(test)]
mod tests {
    use super::{LayerPanelAction, append_geometry, contains, hit_test, max_scroll};
    use glam::DVec2;
    use rasm_engine_document::{Document, Edit, History};

    fn document_with_two_layers() -> Document {
        let mut document = Document::new(1920, 1080).expect("document");
        let mut history = History::default();
        history
            .execute(&mut document, Edit::add_layer("Layer 2"))
            .expect("add second layer");
        document
    }

    #[test]
    fn header_add_button_and_row_actions_hit_the_expected_layer() {
        let document = document_with_two_layers();
        let top_layer = document.layers()[1].id();

        assert_eq!(
            hit_test(&document, DVec2::new(220.0, 28.0), 960, 640, 0),
            Some(LayerPanelAction::Add)
        );
        assert_eq!(
            hit_test(&document, DVec2::new(40.0, 58.0), 960, 640, 0),
            Some(LayerPanelAction::Select(top_layer))
        );
        assert_eq!(
            hit_test(&document, DVec2::new(190.0, 58.0), 960, 640, 0),
            Some(LayerPanelAction::ToggleVisibility(top_layer))
        );
        assert_eq!(
            hit_test(&document, DVec2::new(230.0, 58.0), 960, 640, 0),
            Some(LayerPanelAction::Remove(top_layer))
        );
    }

    #[test]
    fn panel_consumes_background_clicks_and_supports_scrolling() {
        let mut document = document_with_two_layers();
        let mut history = History::default();
        for index in 0..20 {
            history
                .execute(
                    &mut document,
                    Edit::add_layer(format!("Layer {}", index + 3)),
                )
                .expect("add layer");
        }

        assert!(contains(DVec2::new(20.0, 20.0), 960, 640, &document, 0));
        assert!(max_scroll(&document, 640) > 0);
        let first_visible = document.layers()[document.layers().len() - 1].id();
        let after_scroll = document.layers()[document.layers().len() - 2].id();
        assert_eq!(
            hit_test(&document, DVec2::new(40.0, 58.0), 960, 640, 1),
            Some(LayerPanelAction::Select(after_scroll))
        );
        assert_ne!(first_visible, after_scroll);
    }

    #[test]
    fn panel_geometry_is_finite_and_uses_valid_indices() {
        let document = document_with_two_layers();
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        append_geometry(
            &document,
            document.layers()[1].id(),
            960,
            640,
            0,
            &mut vertices,
            &mut indices,
        );

        assert!(!vertices.is_empty());
        assert!(!indices.is_empty());
        assert!(vertices.iter().all(|vertex| {
            vertex.position.iter().all(|value| value.is_finite())
                && vertex.color.iter().all(|value| value.is_finite())
        }));
        assert!(
            indices
                .iter()
                .all(|index| (*index as usize) < vertices.len())
        );
    }
}
