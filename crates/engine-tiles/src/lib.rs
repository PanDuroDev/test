//! Platform-independent tile coverage and bounded dirty-tile tracking.
//!
//! Coordinates are expressed in document raster pixels. Tile bounds are
//! half-open and anchored at the document origin, so negative coordinates
//! are handled by mathematical floor rather than truncation toward zero.

use std::collections::BTreeSet;
use std::fmt;
use std::iter::FusedIterator;

use glam::DVec2;

/// Errors returned by tile-grid and dirty-tile operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileError {
    InvalidTileSize,
    InvalidTileLimit,
    InvalidRectangle,
    InvalidMargin,
    ExpansionOverflow,
    CoordinateOutOfRange,
    TooManyTiles,
    TooManyDirtyTiles,
}

impl fmt::Display for TileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidTileSize => "tile size must be greater than zero",
            Self::InvalidTileLimit => "tile limits must be greater than zero",
            Self::InvalidRectangle => "rectangle coordinates must be finite and ordered",
            Self::InvalidMargin => "rectangle expansion margin must be finite and non-negative",
            Self::ExpansionOverflow => "rectangle expansion produced non-finite coordinates",
            Self::CoordinateOutOfRange => {
                "document coordinate cannot be represented as a tile index"
            },
            Self::TooManyTiles => "rectangle exceeds the configured tile coverage limit",
            Self::TooManyDirtyTiles => "dirty tile set exceeds its configured limit",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for TileError {}

/// A document-space rectangle in raster-pixel coordinates.
///
/// The rectangle uses half-open bounds: `[min.x, max.x) × [min.y, max.y)`.
/// A zero-width or zero-height rectangle is empty and covers no tiles.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DocumentRect {
    min: DVec2,
    max: DVec2,
}

impl DocumentRect {
    /// Creates a rectangle after validating finite, ordered bounds.
    pub fn new(min: DVec2, max: DVec2) -> Result<Self, TileError> {
        if !min.x.is_finite()
            || !min.y.is_finite()
            || !max.x.is_finite()
            || !max.y.is_finite()
            || min.x > max.x
            || min.y > max.y
        {
            return Err(TileError::InvalidRectangle);
        }

        Ok(Self { min, max })
    }

    pub const fn min(self) -> DVec2 {
        self.min
    }

    pub const fn max(self) -> DVec2 {
        self.max
    }

    pub fn is_empty(self) -> bool {
        self.min.x == self.max.x || self.min.y == self.max.y
    }

    /// Expands all four sides by a finite, non-negative raster-pixel margin.
    pub fn expanded(self, margin: f64) -> Result<Self, TileError> {
        if !margin.is_finite() || margin < 0.0 {
            return Err(TileError::InvalidMargin);
        }

        let delta = DVec2::splat(margin);
        let min = self.min - delta;
        let max = self.max + delta;
        if !min.x.is_finite() || !min.y.is_finite() || !max.x.is_finite() || !max.y.is_finite() {
            return Err(TileError::ExpansionOverflow);
        }

        Self::new(min, max)
    }
}

/// Signed tile coordinate. Ordering is deterministic row-major (y, then x).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileCoord {
    x: i64,
    y: i64,
}

impl TileCoord {
    pub const fn new(x: i64, y: i64) -> Self {
        Self { x, y }
    }

    pub const fn x(self) -> i64 {
        self.x
    }

    pub const fn y(self) -> i64 {
        self.y
    }
}

impl PartialOrd for TileCoord {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TileCoord {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.y, self.x).cmp(&(other.y, other.x))
    }
}

/// A bounded grid whose cells are square and anchored at document origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileGrid {
    tile_size: u32,
    max_tiles_per_coverage: usize,
}

impl TileGrid {
    /// Creates a grid with a maximum number of tiles returned by one coverage query.
    pub fn new(tile_size: u32, max_tiles_per_coverage: usize) -> Result<Self, TileError> {
        if tile_size == 0 {
            return Err(TileError::InvalidTileSize);
        }
        if max_tiles_per_coverage == 0 {
            return Err(TileError::InvalidTileLimit);
        }

        Ok(Self {
            tile_size,
            max_tiles_per_coverage,
        })
    }

    pub const fn tile_size(self) -> u32 {
        self.tile_size
    }

    pub const fn max_tiles_per_coverage(self) -> usize {
        self.max_tiles_per_coverage
    }

    /// Computes the tiles intersected by a half-open document rectangle.
    ///
    /// The result is a compact range, not an allocated tile list. Call
    /// `TileCoverage::iter` to visit coordinates in deterministic row-major order.
    pub fn coverage(self, rect: DocumentRect) -> Result<TileCoverage, TileError> {
        if rect.is_empty() {
            return Ok(TileCoverage::empty());
        }

        let size = f64::from(self.tile_size);
        let x_start = checked_tile_index((rect.min.x / size).floor())?;
        let y_start = checked_tile_index((rect.min.y / size).floor())?;
        let x_end_exclusive = checked_tile_index((rect.max.x / size).ceil())?;
        let y_end_exclusive = checked_tile_index((rect.max.y / size).ceil())?;

        let width = i128::from(x_end_exclusive) - i128::from(x_start);
        let height = i128::from(y_end_exclusive) - i128::from(y_start);
        if width < 0 || height < 0 {
            return Err(TileError::CoordinateOutOfRange);
        }

        let count = width
            .checked_mul(height)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or(TileError::TooManyTiles)?;
        if count > self.max_tiles_per_coverage {
            return Err(TileError::TooManyTiles);
        }

        Ok(TileCoverage {
            x_start,
            y_start,
            x_end_exclusive,
            y_end_exclusive,
            tile_count: count,
        })
    }

    /// Computes coverage after expanding the rectangle by a raster-pixel margin.
    pub fn coverage_with_margin(
        self,
        rect: DocumentRect,
        margin: f64,
    ) -> Result<TileCoverage, TileError> {
        self.coverage(rect.expanded(margin)?)
    }
}

const I64_MIN_F64: f64 = -9_223_372_036_854_775_808.0;
const I64_MAX_EXCLUSIVE_F64: f64 = 9_223_372_036_854_775_808.0;

fn checked_tile_index(value: f64) -> Result<i64, TileError> {
    if !value.is_finite() || !(I64_MIN_F64..I64_MAX_EXCLUSIVE_F64).contains(&value) {
        return Err(TileError::CoordinateOutOfRange);
    }
    Ok(value as i64)
}

/// Compact half-open tile-coordinate rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileCoverage {
    x_start: i64,
    y_start: i64,
    x_end_exclusive: i64,
    y_end_exclusive: i64,
    tile_count: usize,
}

impl TileCoverage {
    const fn empty() -> Self {
        Self {
            x_start: 0,
            y_start: 0,
            x_end_exclusive: 0,
            y_end_exclusive: 0,
            tile_count: 0,
        }
    }

    pub const fn x_start(self) -> i64 {
        self.x_start
    }

    pub const fn y_start(self) -> i64 {
        self.y_start
    }

    pub const fn x_end_exclusive(self) -> i64 {
        self.x_end_exclusive
    }

    pub const fn y_end_exclusive(self) -> i64 {
        self.y_end_exclusive
    }

    pub const fn tile_count(self) -> usize {
        self.tile_count
    }

    pub const fn is_empty(self) -> bool {
        self.tile_count == 0
    }

    pub fn iter(self) -> TileIter {
        TileIter {
            coverage: self,
            next_x: self.x_start,
            next_y: self.y_start,
            remaining: self.tile_count,
        }
    }
}

/// Exact-size row-major iterator over a tile coverage rectangle.
#[derive(Debug, Clone)]
pub struct TileIter {
    coverage: TileCoverage,
    next_x: i64,
    next_y: i64,
    remaining: usize,
}

impl Iterator for TileIter {
    type Item = TileCoord;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }

        let current = TileCoord::new(self.next_x, self.next_y);
        self.remaining -= 1;

        if self.next_x + 1 < self.coverage.x_end_exclusive {
            self.next_x += 1;
        } else if self.next_y + 1 < self.coverage.y_end_exclusive {
            self.next_x = self.coverage.x_start;
            self.next_y += 1;
        }

        Some(current)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl ExactSizeIterator for TileIter {}
impl FusedIterator for TileIter {}

/// A bounded set of tiles that need redraw or cache invalidation.
///
/// Mark operations are transactional: an error leaves the set unchanged.
/// Iteration and draining are deterministic row-major order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirtyTileSet {
    tiles: BTreeSet<TileCoord>,
    max_tiles: usize,
}

impl DirtyTileSet {
    pub fn new(max_tiles: usize) -> Result<Self, TileError> {
        if max_tiles == 0 {
            return Err(TileError::InvalidTileLimit);
        }

        Ok(Self {
            tiles: BTreeSet::new(),
            max_tiles,
        })
    }

    pub const fn max_tiles(&self) -> usize {
        self.max_tiles
    }

    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    /// Marks all tiles intersected by a rectangle without additional margin.
    ///
    /// Returns the number of newly marked tiles.
    pub fn mark_rect(&mut self, grid: TileGrid, rect: DocumentRect) -> Result<usize, TileError> {
        self.mark_rect_with_margin(grid, rect, 0.0)
    }

    /// Marks expanded rectangle coverage. Existing tiles are not counted twice.
    ///
    /// If the dirty-set limit would be exceeded, no tile is inserted.
    pub fn mark_rect_with_margin(
        &mut self,
        grid: TileGrid,
        rect: DocumentRect,
        margin: f64,
    ) -> Result<usize, TileError> {
        let coverage = grid.coverage_with_margin(rect, margin)?;
        self.mark_coverage(coverage)
    }

    /// Marks a previously computed coverage range transactionally.
    pub fn mark_coverage(&mut self, coverage: TileCoverage) -> Result<usize, TileError> {
        let mut additions = BTreeSet::new();

        for tile in coverage.iter() {
            if self.tiles.contains(&tile) {
                continue;
            }

            additions.insert(tile);
            if self
                .tiles
                .len()
                .checked_add(additions.len())
                .is_none_or(|count| count > self.max_tiles)
            {
                return Err(TileError::TooManyDirtyTiles);
            }
        }

        let added = additions.len();
        self.tiles.extend(additions);
        Ok(added)
    }

    pub fn iter(&self) -> impl Iterator<Item = TileCoord> + '_ {
        self.tiles.iter().copied()
    }

    /// Removes and returns all dirty tiles in deterministic row-major order.
    pub fn drain_tiles(&mut self) -> Vec<TileCoord> {
        std::mem::take(&mut self.tiles).into_iter().collect()
    }

    pub fn clear(&mut self) {
        self.tiles.clear();
    }
}

#[cfg(test)]
mod tests {
    use glam::DVec2;

    use super::{DirtyTileSet, DocumentRect, TileCoord, TileError, TileGrid};

    fn rect(min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> DocumentRect {
        DocumentRect::new(DVec2::new(min_x, min_y), DVec2::new(max_x, max_y))
            .expect("valid rectangle")
    }

    fn grid() -> TileGrid {
        TileGrid::new(64, 256).expect("valid grid")
    }

    #[test]
    fn zero_tile_size_and_zero_limits_are_rejected() {
        assert_eq!(TileGrid::new(0, 16), Err(TileError::InvalidTileSize));
        assert_eq!(TileGrid::new(64, 0), Err(TileError::InvalidTileLimit));
        assert_eq!(
            DirtyTileSet::new(0).err(),
            Some(TileError::InvalidTileLimit)
        );
    }

    #[test]
    fn positive_rectangle_uses_half_open_tile_bounds() {
        let coverage = grid()
            .coverage(rect(1.0, 2.0, 65.0, 66.0))
            .expect("coverage");
        let expected = vec![
            TileCoord::new(0, 0),
            TileCoord::new(1, 0),
            TileCoord::new(0, 1),
            TileCoord::new(1, 1),
        ];

        assert_eq!(coverage.tile_count(), expected.len());
        assert_eq!(coverage.iter().collect::<Vec<_>>(), expected);
    }

    #[test]
    fn exact_max_boundary_does_not_include_the_next_tile() {
        let coverage = grid()
            .coverage(rect(0.0, 0.0, 64.0, 64.0))
            .expect("coverage");
        assert_eq!(
            coverage.iter().collect::<Vec<_>>(),
            vec![TileCoord::new(0, 0)]
        );
    }

    #[test]
    fn negative_coordinates_use_floor_not_truncation() {
        let coverage = grid()
            .coverage(rect(-64.0, -64.0, 0.0, 0.0))
            .expect("coverage");
        assert_eq!(
            coverage.iter().collect::<Vec<_>>(),
            vec![TileCoord::new(-1, -1)]
        );
    }

    #[test]
    fn negative_fractional_rectangle_stays_in_negative_tile() {
        let coverage = grid()
            .coverage(rect(-63.0, -20.0, -0.1, -0.1))
            .expect("coverage");
        assert_eq!(
            coverage.iter().collect::<Vec<_>>(),
            vec![TileCoord::new(-1, -1)]
        );
    }

    #[test]
    fn zero_width_or_height_rectangle_is_empty() {
        assert!(
            grid()
                .coverage(rect(10.0, 0.0, 10.0, 64.0))
                .expect("coverage")
                .is_empty()
        );
        assert!(
            grid()
                .coverage(rect(0.0, 10.0, 64.0, 10.0))
                .expect("coverage")
                .is_empty()
        );
    }

    #[test]
    fn non_finite_and_inverted_rectangles_are_rejected() {
        assert_eq!(
            DocumentRect::new(DVec2::new(f64::NAN, 0.0), DVec2::ONE),
            Err(TileError::InvalidRectangle)
        );
        assert_eq!(
            DocumentRect::new(DVec2::new(4.0, 0.0), DVec2::new(3.0, 1.0)),
            Err(TileError::InvalidRectangle)
        );
    }

    #[test]
    fn invalid_margins_are_rejected() {
        let source = rect(0.0, 0.0, 1.0, 1.0);
        assert_eq!(source.expanded(-1.0), Err(TileError::InvalidMargin));
        assert_eq!(
            source.expanded(f64::INFINITY),
            Err(TileError::InvalidMargin)
        );
    }

    #[test]
    fn margin_expansion_marks_adjacent_tiles() {
        let source = rect(63.8, 10.0, 64.0, 20.0);
        let without_margin = grid().coverage(source).expect("coverage");
        let with_margin = grid()
            .coverage_with_margin(source, 0.5)
            .expect("expanded coverage");

        assert_eq!(
            without_margin.iter().collect::<Vec<_>>(),
            vec![TileCoord::new(0, 0)]
        );
        assert_eq!(
            with_margin.iter().collect::<Vec<_>>(),
            vec![TileCoord::new(0, 0), TileCoord::new(1, 0)]
        );
    }

    #[test]
    fn expansion_overflow_is_rejected() {
        let source = rect(f64::MAX / 2.0, 0.0, f64::MAX, 1.0);
        assert_eq!(source.expanded(f64::MAX), Err(TileError::ExpansionOverflow));
    }

    #[test]
    fn coverage_limit_rejects_large_regions_before_iteration() {
        let small_grid = TileGrid::new(1, 4).expect("valid grid");
        assert_eq!(
            small_grid.coverage(rect(0.0, 0.0, 3.0, 3.0)),
            Err(TileError::TooManyTiles)
        );
    }

    #[test]
    fn out_of_range_coordinates_are_rejected() {
        let source = rect(9.0e20, 0.0, 9.1e20, 1.0);
        assert_eq!(
            grid().coverage(source),
            Err(TileError::CoordinateOutOfRange)
        );
    }

    #[test]
    fn iterator_is_exact_size_and_row_major() {
        let coverage = grid()
            .coverage(rect(0.0, 0.0, 129.0, 129.0))
            .expect("coverage");
        let mut iter = coverage.iter();
        assert_eq!(iter.len(), 9);
        assert_eq!(iter.next(), Some(TileCoord::new(0, 0)));
        assert_eq!(iter.len(), 8);
        assert_eq!(
            iter.collect::<Vec<_>>(),
            vec![
                TileCoord::new(1, 0),
                TileCoord::new(2, 0),
                TileCoord::new(0, 1),
                TileCoord::new(1, 1),
                TileCoord::new(2, 1),
                TileCoord::new(0, 2),
                TileCoord::new(1, 2),
                TileCoord::new(2, 2),
            ]
        );
    }

    #[test]
    fn dirty_marking_is_idempotent() {
        let mut dirty = DirtyTileSet::new(8).expect("valid set");
        let region = rect(0.0, 0.0, 64.0, 64.0);

        assert_eq!(dirty.mark_rect(grid(), region), Ok(1));
        assert_eq!(dirty.mark_rect(grid(), region), Ok(0));
        assert_eq!(dirty.len(), 1);
    }

    #[test]
    fn overlapping_marks_only_add_new_tiles() {
        let mut dirty = DirtyTileSet::new(8).expect("valid set");
        assert_eq!(dirty.mark_rect(grid(), rect(0.0, 0.0, 64.0, 64.0)), Ok(1));
        assert_eq!(dirty.mark_rect(grid(), rect(32.0, 0.0, 96.0, 64.0)), Ok(1));
        assert_eq!(dirty.len(), 2);
    }

    #[test]
    fn dirty_set_iterates_and_drains_in_row_major_order() {
        let mut dirty = DirtyTileSet::new(16).expect("valid set");
        dirty
            .mark_rect(grid(), rect(0.0, 0.0, 129.0, 129.0))
            .expect("mark");
        let expected = vec![
            TileCoord::new(0, 0),
            TileCoord::new(1, 0),
            TileCoord::new(2, 0),
            TileCoord::new(0, 1),
            TileCoord::new(1, 1),
            TileCoord::new(2, 1),
            TileCoord::new(0, 2),
            TileCoord::new(1, 2),
            TileCoord::new(2, 2),
        ];

        assert_eq!(dirty.iter().collect::<Vec<_>>(), expected);
        assert_eq!(dirty.drain_tiles(), expected);
        assert!(dirty.is_empty());
    }

    #[test]
    fn dirty_limit_failure_is_transactional() {
        let mut dirty = DirtyTileSet::new(2).expect("valid set");
        dirty
            .mark_rect(grid(), rect(0.0, 0.0, 64.0, 64.0))
            .expect("initial mark");
        let before = dirty.iter().collect::<Vec<_>>();

        assert_eq!(
            dirty.mark_rect(grid(), rect(64.0, 0.0, 192.0, 64.0)),
            Err(TileError::TooManyDirtyTiles)
        );
        assert_eq!(dirty.iter().collect::<Vec<_>>(), before);
    }

    #[test]
    fn coverage_failure_does_not_change_dirty_set() {
        let mut dirty = DirtyTileSet::new(8).expect("valid set");
        dirty.mark_rect(grid(), rect(0.0, 0.0, 64.0, 64.0)).expect("initial mark");
        let before = dirty.iter().collect::<Vec<_>>();
        let limited_grid = TileGrid::new(1, 2).expect("valid grid");

        assert_eq!(
            dirty.mark_rect(limited_grid, rect(0.0, 0.0, 2.0, 2.0)),
            Err(TileError::TooManyTiles)
        );
        assert_eq!(dirty.iter().collect::<Vec<_>>(), before);
    }

    #[test]
    fn clear_removes_all_dirty_tiles() {
        let mut dirty = DirtyTileSet::new(8).expect("valid set");
        dirty
            .mark_rect(grid(), rect(0.0, 0.0, 64.0, 64.0))
            .expect("mark");
        dirty.clear();
        assert!(dirty.is_empty());
    }
}
