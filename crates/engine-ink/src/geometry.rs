//! Renderer-independent connected geometry for pressure-aware brush strokes.
//!
//! This module converts the dab stream into a bounded triangle mesh. It does
//! not depend on Tileink, WGPU, Android, or any graphics API.

use std::f64::consts::PI;
use std::fmt;

use glam::DVec2;

use crate::BrushDab;

/// Errors returned while building connected stroke geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrokeGeometryError {
    InvalidConfig,
    InvalidDab,
    TooManyVertices,
    IndexOverflow,
}

impl fmt::Display for StrokeGeometryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidConfig => "stroke geometry configuration is invalid",
            Self::InvalidDab => "stroke dab contains a non-finite position or invalid radius",
            Self::TooManyVertices => "stroke mesh exceeds the configured vertex limit",
            Self::IndexOverflow => "stroke mesh cannot be represented with 32-bit indices",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for StrokeGeometryError {}

/// Limits and quality settings for connected stroke geometry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrokeGeometryConfig {
    max_miter_ratio: f64,
    cap_segments: u8,
    max_vertices: usize,
}

impl StrokeGeometryConfig {
    /// Creates a validated configuration.
    ///
    /// `cap_segments` controls the number of line segments in each round cap
    /// (and half of the segment count used for a single-dab disk).
    pub fn new(
        max_miter_ratio: f64,
        cap_segments: u8,
        max_vertices: usize,
    ) -> Result<Self, StrokeGeometryError> {
        if !max_miter_ratio.is_finite()
            || max_miter_ratio < 1.0
            || !(2..=64).contains(&cap_segments)
            || max_vertices < 3
            || max_vertices > u32::MAX as usize
        {
            return Err(StrokeGeometryError::InvalidConfig);
        }

        Ok(Self {
            max_miter_ratio,
            cap_segments,
            max_vertices,
        })
    }

    pub const fn max_miter_ratio(self) -> f64 {
        self.max_miter_ratio
    }

    pub const fn cap_segments(self) -> u8 {
        self.cap_segments
    }

    pub const fn max_vertices(self) -> usize {
        self.max_vertices
    }
}

impl Default for StrokeGeometryConfig {
    fn default() -> Self {
        Self {
            max_miter_ratio: 4.0,
            cap_segments: 8,
            max_vertices: 1_000_000,
        }
    }
}

/// One vertex in document coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrokeVertex {
    position: DVec2,
}

impl StrokeVertex {
    pub const fn position(self) -> DVec2 {
        self.position
    }
}

/// Indexed triangle mesh for one connected brush stroke.
#[derive(Debug, Clone, PartialEq)]
pub struct StrokeMesh {
    vertices: Vec<StrokeVertex>,
    indices: Vec<u32>,
}

impl StrokeMesh {
    pub fn vertices(&self) -> &[StrokeVertex] {
        &self.vertices
    }

    pub fn indices(&self) -> &[u32] {
        &self.indices
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }
}

/// Converts an ordered dab stream into a connected triangle mesh with round caps.
///
/// Consecutive cross-sections are joined into a triangle strip. Interior
/// cross-sections use a bounded miter, preventing acute turns from creating
/// unbounded spikes. Near-reversals fall back to a finite normal. Round cap
/// fans close both ends; a single dab is represented as a disk.
pub fn tessellate_stroke_mesh(
    dabs: &[BrushDab],
    config: StrokeGeometryConfig,
) -> Result<StrokeMesh, StrokeGeometryError> {
    if dabs.iter().any(|dab| {
        let position = dab.position();
        !position.x.is_finite()
            || !position.y.is_finite()
            || !dab.radius().is_finite()
            || dab.radius() <= 0.0
    }) {
        return Err(StrokeGeometryError::InvalidDab);
    }

    let mut mesh = StrokeMesh {
        vertices: Vec::new(),
        indices: Vec::new(),
    };

    let cap_segments = usize::from(config.cap_segments);
    let required_vertices = match dabs.len() {
        0 => return Ok(mesh),
        1 => 1_usize.checked_add(cap_segments * 2),
        count => count
            .checked_mul(2)
            .and_then(|body| body.checked_add(2 * (cap_segments + 2))),
    }
    .ok_or(StrokeGeometryError::TooManyVertices)?;
    if required_vertices > config.max_vertices {
        return Err(StrokeGeometryError::TooManyVertices);
    }

    if dabs.len() == 1 {
        push_disk(&mut mesh, dabs[0], config)?;
        return Ok(mesh);
    }

    let mut sections = Vec::with_capacity(dabs.len());
    for (index, dab) in dabs.iter().copied().enumerate() {
        let normal = section_normal(dabs, index);
        let offset = miter_offset(dabs, index, normal, config.max_miter_ratio);
        let left = dab.position() + offset;
        let right = dab.position() - offset;
        sections.push((
            push_vertex(&mut mesh, left, config)?,
            push_vertex(&mut mesh, right, config)?,
        ));
    }

    for pair in sections.windows(2) {
        let (left_a, right_a) = pair[0];
        let (left_b, right_b) = pair[1];
        push_triangle(&mut mesh, left_a, right_a, left_b);
        push_triangle(&mut mesh, right_a, right_b, left_b);
    }

    let first_direction = unit_or(dabs[1].position() - dabs[0].position(), DVec2::X);
    let last = dabs.len() - 1;
    let last_direction = unit_or(
        dabs[last].position() - dabs[last - 1].position(),
        first_direction,
    );

    push_round_cap(
        &mut mesh,
        dabs[0].position(),
        -first_direction,
        dabs[0].radius(),
        config,
    )?;
    push_round_cap(
        &mut mesh,
        dabs[last].position(),
        last_direction,
        dabs[last].radius(),
        config,
    )?;

    Ok(mesh)
}

fn section_normal(dabs: &[BrushDab], index: usize) -> DVec2 {
    if index == 0 {
        return perpendicular(unit_or(dabs[1].position() - dabs[0].position(), DVec2::X));
    }

    let last = dabs.len() - 1;
    if index == last {
        return perpendicular(unit_or(
            dabs[last].position() - dabs[last - 1].position(),
            DVec2::X,
        ));
    }

    let incoming = unit_or(
        dabs[index].position() - dabs[index - 1].position(),
        DVec2::X,
    );
    let outgoing = unit_or(
        dabs[index + 1].position() - dabs[index].position(),
        incoming,
    );
    let normal_sum = perpendicular(incoming) + perpendicular(outgoing);
    unit_or(normal_sum, perpendicular(outgoing))
}

fn miter_offset(dabs: &[BrushDab], index: usize, normal: DVec2, max_miter_ratio: f64) -> DVec2 {
    let radius = dabs[index].radius();
    if index == 0 || index + 1 == dabs.len() {
        return normal * radius;
    }

    let outgoing = unit_or(
        dabs[index + 1].position() - dabs[index].position(),
        DVec2::X,
    );
    let outgoing_normal = perpendicular(outgoing);
    let denominator = normal.dot(outgoing_normal).abs();
    let limit = radius * max_miter_ratio;
    let miter_length = if denominator > 1.0e-6 {
        (radius / denominator).min(limit)
    } else {
        limit
    };
    normal * miter_length
}

fn push_disk(
    mesh: &mut StrokeMesh,
    dab: BrushDab,
    config: StrokeGeometryConfig,
) -> Result<(), StrokeGeometryError> {
    let center = push_vertex(mesh, dab.position(), config)?;
    let segment_count = usize::from(config.cap_segments) * 2;
    let mut perimeter = Vec::with_capacity(segment_count);

    for segment in 0..segment_count {
        let angle = 2.0 * PI * segment as f64 / segment_count as f64;
        let point = dab.position() + DVec2::new(angle.cos(), angle.sin()) * dab.radius();
        perimeter.push(push_vertex(mesh, point, config)?);
    }

    for (current, next) in perimeter.iter().zip(perimeter.iter().cycle().skip(1)) {
        push_triangle(mesh, center, *current, *next);
    }

    Ok(())
}

fn push_round_cap(
    mesh: &mut StrokeMesh,
    center: DVec2,
    outward_direction: DVec2,
    radius: f64,
    config: StrokeGeometryConfig,
) -> Result<(), StrokeGeometryError> {
    let tangent = unit_or(outward_direction, DVec2::X);
    let normal = perpendicular(tangent);
    let center_index = push_vertex(mesh, center, config)?;
    let segments = usize::from(config.cap_segments);
    let mut arc = Vec::with_capacity(segments + 1);

    for segment in 0..=segments {
        let angle = PI / 2.0 - PI * segment as f64 / segments as f64;
        let point = center + (tangent * angle.cos() + normal * angle.sin()) * radius;
        arc.push(push_vertex(mesh, point, config)?);
    }

    for pair in arc.windows(2) {
        push_triangle(mesh, center_index, pair[0], pair[1]);
    }

    Ok(())
}

fn push_vertex(
    mesh: &mut StrokeMesh,
    position: DVec2,
    config: StrokeGeometryConfig,
) -> Result<u32, StrokeGeometryError> {
    if mesh.vertices.len() >= config.max_vertices {
        return Err(StrokeGeometryError::TooManyVertices);
    }
    let index =
        u32::try_from(mesh.vertices.len()).map_err(|_| StrokeGeometryError::IndexOverflow)?;
    mesh.vertices.push(StrokeVertex { position });
    Ok(index)
}

fn push_triangle(mesh: &mut StrokeMesh, a: u32, b: u32, c: u32) {
    let pa = mesh.vertices[a as usize].position;
    let pb = mesh.vertices[b as usize].position;
    let pc = mesh.vertices[c as usize].position;
    let ab = pb - pa;
    let ac = pc - pa;
    let area_twice = ab.x * ac.y - ab.y * ac.x;

    if !area_twice.is_finite() || area_twice.abs() <= f64::EPSILON {
        return;
    }

    if area_twice > 0.0 {
        mesh.indices.extend_from_slice(&[a, b, c]);
    } else {
        mesh.indices.extend_from_slice(&[a, c, b]);
    }
}

fn perpendicular(vector: DVec2) -> DVec2 {
    DVec2::new(-vector.y, vector.x)
}

fn unit_or(vector: DVec2, fallback: DVec2) -> DVec2 {
    let length_squared = vector.length_squared();
    if length_squared.is_finite() && length_squared > 1.0e-24 {
        vector / length_squared.sqrt()
    } else {
        fallback
    }
}

#[cfg(test)]
mod tests {
    use glam::DVec2;

    use crate::{BrushConfig, StrokeSample, tessellate_stroke};

    use super::{StrokeGeometryConfig, StrokeGeometryError, tessellate_stroke_mesh};

    fn dabs(points: &[(f64, f64, f32)]) -> Vec<crate::BrushDab> {
        let samples: Vec<_> = points
            .iter()
            .enumerate()
            .map(|(index, (x, y, pressure))| {
                StrokeSample::new(
                    DVec2::new(*x, *y),
                    Some(*pressure),
                    index as u64 * 1_000_000,
                )
                .expect("valid sample")
            })
            .collect();
        tessellate_stroke(&samples, BrushConfig::default()).expect("valid stroke")
    }

    fn assert_mesh_indices_are_valid(mesh: &super::StrokeMesh) {
        assert_eq!(mesh.indices().len() % 3, 0);
        assert!(
            mesh.indices()
                .iter()
                .all(|index| (*index as usize) < mesh.vertices().len())
        );
        for triangle in mesh.indices().chunks_exact(3) {
            let a = mesh.vertices()[triangle[0] as usize].position();
            let b = mesh.vertices()[triangle[1] as usize].position();
            let c = mesh.vertices()[triangle[2] as usize].position();
            let ab = b - a;
            let ac = c - a;
            assert!(ab.x * ac.y - ab.y * ac.x > 0.0);
        }
        assert!(
            mesh.vertices().iter().all(|vertex| {
                vertex.position().x.is_finite() && vertex.position().y.is_finite()
            })
        );
    }

    #[test]
    fn empty_input_produces_an_empty_mesh() {
        let mesh = tessellate_stroke_mesh(&[], StrokeGeometryConfig::default())
            .expect("empty geometry is valid");
        assert!(mesh.is_empty());
        assert!(mesh.vertices().is_empty());
    }

    #[test]
    fn a_single_dab_becomes_a_disk() {
        let input = dabs(&[(4.0, 7.0, 0.75)]);
        let mesh =
            tessellate_stroke_mesh(&input, StrokeGeometryConfig::default()).expect("valid disk");

        assert!(mesh.triangle_count() >= 16);
        assert_mesh_indices_are_valid(&mesh);
    }

    #[test]
    fn straight_strokes_have_connected_body_and_round_caps() {
        let input = dabs(&[(0.0, 0.0, 1.0), (64.0, 0.0, 1.0)]);
        let mesh = tessellate_stroke_mesh(&input, StrokeGeometryConfig::default())
            .expect("valid straight mesh");

        assert!(mesh.triangle_count() > 20);
        let min_x = mesh
            .vertices()
            .iter()
            .map(|vertex| vertex.position().x)
            .fold(f64::INFINITY, f64::min);
        let max_x = mesh
            .vertices()
            .iter()
            .map(|vertex| vertex.position().x)
            .fold(f64::NEG_INFINITY, f64::max);
        assert!(min_x < -7.9);
        assert!(max_x > 71.9);
        assert_mesh_indices_are_valid(&mesh);
    }

    #[test]
    fn sharp_turns_respect_the_miter_limit() {
        let input = dabs(&[
            (0.0, 0.0, 1.0),
            (20.0, 0.0, 1.0),
            (10.0, 5.773_502_691_9, 1.0),
        ]);
        let config = StrokeGeometryConfig::new(2.0, 8, 10_000).expect("valid config");
        let mesh = tessellate_stroke_mesh(&input, config).expect("bounded mesh");
        let corner_index = input
            .iter()
            .position(|dab| dab.position().distance(DVec2::new(20.0, 0.0)) < 1.0e-9)
            .expect("the corner dab is preserved");
        let center = input[corner_index].position();
        let maximum = input[corner_index].radius() * config.max_miter_ratio() + 1.0e-9;
        let section_start = corner_index * 2;
        let section_vertices = &mesh.vertices()[section_start..section_start + 2];

        assert!(
            section_vertices
                .iter()
                .all(|vertex| vertex.position().distance(center) <= maximum)
        );
        assert_mesh_indices_are_valid(&mesh);
    }

    #[test]
    fn near_reversal_does_not_generate_non_finite_vertices() {
        let input = dabs(&[(0.0, 0.0, 0.8), (20.0, 0.0, 0.8), (0.01, 0.001, 0.8)]);
        let mesh = tessellate_stroke_mesh(&input, StrokeGeometryConfig::default())
            .expect("finite bounded mesh");
        assert_mesh_indices_are_valid(&mesh);
    }

    #[test]
    fn pressure_changes_mesh_width() {
        let input = dabs(&[(0.0, 0.0, 0.1), (30.0, 0.0, 1.0)]);
        let mesh =
            tessellate_stroke_mesh(&input, StrokeGeometryConfig::default()).expect("pressure mesh");

        let first_pair = &mesh.vertices()[0..2];
        let last_section_start = (input.len() - 1) * 2;
        let last_pair = &mesh.vertices()[last_section_start..last_section_start + 2];
        let first_width = first_pair[0].position().distance(first_pair[1].position());
        let last_width = last_pair[0].position().distance(last_pair[1].position());
        assert!(last_width > first_width);
    }

    #[test]
    fn invalid_configurations_are_rejected() {
        assert_eq!(
            StrokeGeometryConfig::new(0.5, 8, 100),
            Err(StrokeGeometryError::InvalidConfig)
        );
        assert_eq!(
            StrokeGeometryConfig::new(4.0, 1, 100),
            Err(StrokeGeometryError::InvalidConfig)
        );
        assert_eq!(
            StrokeGeometryConfig::new(4.0, 8, 2),
            Err(StrokeGeometryError::InvalidConfig)
        );
    }

    #[test]
    fn vertex_limit_is_enforced() {
        let input = dabs(&[(0.0, 0.0, 1.0), (20.0, 0.0, 1.0)]);
        let config = StrokeGeometryConfig::new(4.0, 8, 4).expect("valid config");
        assert_eq!(
            tessellate_stroke_mesh(&input, config),
            Err(StrokeGeometryError::TooManyVertices)
        );
    }
}
