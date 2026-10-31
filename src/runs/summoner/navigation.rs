//! 현재 관찰한 바닥의 연결·착지 후보. 안전이나 실제 이동 성공은 보증하지 않는다.
mod connected;
mod grid_sample;
mod teleport;
use crate::common::{
    color_palette::Palette,
    config::ConfigError,
    error::ObservationError,
    frame::{BgrView, FloatMap, GrayFrame, GrayView, PixelRect, Size},
    time,
    vision_ops::{
        components::{self, Connectivity},
        morphology::{self, Kernel, Morphology},
    },
};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Wing {
    North,
    East,
    South,
    West,
}
impl Wing {
    pub const ALL: [Self; 4] = [Self::North, Self::East, Self::South, Self::West];
    pub fn heading(self) -> [f64; 2] {
        match self {
            Self::North => [1.0, -0.5],
            Self::East => [1.0, 0.5],
            Self::South => [-1.0, 0.5],
            Self::West => [-1.0, -0.5],
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::North => "north",
            Self::East => "east",
            Self::South => "south",
            Self::West => "west",
        }
    }
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSettings {
    cell_pixels: usize,
    floor_fraction: f64,
    seed_radius: f64,
    maximum_step: f64,
    turn_degrees: f64,
    revisit_radius: f64,
    minimum_component_cells: usize,
}
#[derive(Debug, Clone, Serialize)]
pub struct PathSettings {
    pub(crate) cell_pixels: usize,
    pub(crate) floor_fraction: f64,
    pub(crate) seed_radius: f64,
    pub(crate) maximum_step: f64,
    pub(crate) turn_degrees: f64,
    pub(crate) revisit_radius: f64,
    pub(crate) minimum_component_cells: usize,
}
impl PathSettings {
    pub fn new(
        cell_pixels: usize,
        floor_fraction: f64,
        seed_radius: f64,
        maximum_step: f64,
        turn_degrees: f64,
        revisit_radius: f64,
        minimum_component_cells: usize,
    ) -> Result<Self, ConfigError> {
        if !(2..=64).contains(&cell_pixels) || minimum_component_cells < 2 {
            return Err(ConfigError::value("path cell/component bounds invalid"));
        }
        for (name, v) in [
            ("floor_fraction", floor_fraction),
            ("seed_radius", seed_radius),
            ("maximum_step", maximum_step),
            ("turn_degrees", turn_degrees),
            ("revisit_radius", revisit_radius),
        ] {
            time::positive(v, name)?;
        }
        if floor_fraction > 1.0 || turn_degrees >= 180.0 {
            return Err(ConfigError::value("fraction/turn outside supported range"));
        }
        Ok(Self {
            cell_pixels,
            floor_fraction,
            seed_radius,
            maximum_step,
            turn_degrees,
            revisit_radius,
            minimum_component_cells,
        })
    }
    pub fn from_value(value: &serde_json::Value) -> Result<Self, ConfigError> {
        let r: RawSettings =
            serde_json::from_value(value.clone()).map_err(|e| ConfigError::value(e.to_string()))?;
        Self::new(
            r.cell_pixels,
            r.floor_fraction,
            r.seed_radius,
            r.maximum_step,
            r.turn_degrees,
            r.revisit_radius,
            r.minimum_component_cells,
        )
    }
    pub fn cell_pixels(&self) -> usize {
        self.cell_pixels
    }
    pub fn seed_radius(&self) -> f64 {
        self.seed_radius
    }
    pub fn maximum_step(&self) -> f64 {
        self.maximum_step
    }
    pub fn revisit_radius(&self) -> f64 {
        self.revisit_radius
    }
    pub fn with_seed_step(&self, seed: f64, step: f64) -> Result<Self, ConfigError> {
        Self::new(
            self.cell_pixels,
            self.floor_fraction,
            seed,
            step,
            self.turn_degrees,
            self.revisit_radius,
            self.minimum_component_cells,
        )
    }
}
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTeleport {
    maximum_distance: f64,
    clearance_pixels: f64,
    preferred_distance: Option<f64>,
}
#[derive(Debug, Clone, Copy)]
pub struct TeleportGeometry {
    pub maximum_distance: f64,
    pub clearance_pixels: f64,
    pub preferred_distance: Option<f64>,
}
impl TeleportGeometry {
    pub fn new(
        maximum_distance: f64,
        clearance_pixels: f64,
        preferred_distance: Option<f64>,
    ) -> Result<Self, ConfigError> {
        time::positive(maximum_distance, "maximum_distance")?;
        time::positive(clearance_pixels, "clearance_pixels")?;
        if let Some(p) = preferred_distance {
            time::positive(p, "preferred_distance")?;
        }
        if clearance_pixels >= maximum_distance
            || preferred_distance.is_some_and(|p| p > maximum_distance)
        {
            return Err(ConfigError::value("teleport geometry exceeds its bound"));
        }
        Ok(Self {
            maximum_distance,
            clearance_pixels,
            preferred_distance,
        })
    }
    pub fn from_value(value: &serde_json::Value) -> Result<Self, ConfigError> {
        if value
            .get("preferred_distance")
            .is_some_and(serde_json::Value::is_null)
        {
            return Err(ConfigError::value("positive teleport geometry required"));
        }
        let r: RawTeleport =
            serde_json::from_value(value.clone()).map_err(|e| ConfigError::value(e.to_string()))?;
        Self::new(r.maximum_distance, r.clearance_pixels, r.preferred_distance)
    }
}
#[derive(Debug, Clone, Deserialize)]
pub struct TerrainPalette {
    floor_bgr: Vec<[u8; 3]>,
    void_bgr: Vec<[u8; 3]>,
    floor_lab_distance: f64,
    void_lab_distance: f64,
}
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutomapOverlay {
    #[serde(deserialize_with = "neutral_fraction")]
    neutral_pixel_fraction: f64,
}
fn neutral_fraction<'de, D: serde::Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    match value {
        serde_json::Value::Bool(b) => Ok(f64::from(u8::from(b))),
        serde_json::Value::Number(n) => n
            .as_f64()
            .ok_or_else(|| serde::de::Error::custom("numeric neutral fraction required")),
        _ => Err(serde::de::Error::custom(
            "numeric neutral fraction required",
        )),
    }
}
impl AutomapOverlay {
    pub fn new(neutral_pixel_fraction: f64) -> Result<Self, ConfigError> {
        time::positive(neutral_pixel_fraction, "neutral_pixel_fraction")?;
        if neutral_pixel_fraction > 1.0 {
            return Err(ConfigError::value("neutral fraction exceeds one"));
        }
        Ok(Self {
            neutral_pixel_fraction,
        })
    }
}
#[derive(Debug, Clone, Copy)]
pub struct PathContext {
    pub sequence: u64,
    pub generation: u64,
    pub captured_at: f64,
    pub frame_size: Size,
    pub viewport: PixelRect,
    pub anchor: [f64; 2],
}
#[derive(Debug, Clone)]
pub struct PathView {
    pub context: PathContext,
    pub cell_pixels: usize,
    floor: GrayFrame,
    excluded_pixels: Option<GrayFrame>,
}
impl PathView {
    pub fn from_grid(
        context: PathContext,
        cell_pixels: usize,
        floor: GrayFrame,
    ) -> Result<Self, ObservationError> {
        time::nonnegative(context.captured_at, "captured_at")?;
        context.viewport.checked_in(context.frame_size)?;
        if cell_pixels == 0
            || context.anchor.iter().any(|v| !v.is_finite())
            || floor.size().width() != context.viewport.width() / cell_pixels
            || floor.size().height() != context.viewport.height() / cell_pixels
        {
            return Err(ObservationError::invalid("path grid geometry mismatch"));
        }
        Ok(Self {
            context,
            cell_pixels,
            floor,
            excluded_pixels: None,
        })
    }
    pub fn floor(&self) -> GrayView<'_> {
        self.floor.view()
    }
    pub fn excluded_pixels(&self) -> Option<GrayView<'_>> {
        self.excluded_pixels.as_ref().map(GrayFrame::view)
    }
    pub fn point(&self, [x, y]: [usize; 2]) -> [f64; 2] {
        [
            self.context.viewport.left() as f64 + (x as f64 + 0.5) * self.cell_pixels as f64,
            self.context.viewport.top() as f64 + (y as f64 + 0.5) * self.cell_pixels as f64,
        ]
    }
    pub fn clearance(&self) -> Result<FloatMap, ObservationError> {
        floor_clearance(&self.floor)
    }
}
fn floor_clearance(floor: &GrayFrame) -> Result<FloatMap, ObservationError> {
    let size = floor.size();
    let mut padded = GrayFrame::zeros(Size::new(size.width() + 2, size.height() + 2)?)?;
    for y in 0..size.height() {
        let at = (y + 1) * (size.width() + 2) + 1;
        padded.pixels_mut()[at..at + size.width()].copy_from_slice(
            floor
                .view()
                .row(y)
                .ok_or_else(|| ObservationError::invalid("floor row missing"))?,
        );
    }
    let full = components::distance_l2_mask5(padded.view())?;
    Ok(full
        .view()
        .crop(PixelRect::new(1, 1, size.width() + 1, size.height() + 1)?)?
        .to_owned()?)
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PathChoice {
    pub aim: Option<[i32; 2]>,
    pub reason: String,
    pub path: Vec<[f64; 2]>,
}
impl PathChoice {
    pub fn unknown(reason: impl Into<String>) -> Self {
        Self {
            aim: None,
            reason: reason.into(),
            path: Vec::new(),
        }
    }
}
#[derive(Clone, Copy)]
pub struct ChooseOptions<'a> {
    pub position: [f64; 2],
    pub visited: &'a [[f64; 2]],
    pub destination: Option<[f64; 2]>,
    pub excluded: &'a [[f64; 4]],
    pub allow_detour: bool,
    pub heading: Option<[f64; 2]>,
    pub prefer_frontier: bool,
}
impl Default for ChooseOptions<'_> {
    fn default() -> Self {
        Self {
            position: [0.0, 0.0],
            visited: &[],
            destination: None,
            excluded: &[],
            allow_detour: false,
            heading: None,
            prefer_frontier: false,
        }
    }
}
#[derive(Clone)]
pub struct ArcanePathObserver {
    settings: PathSettings,
    teleport: Option<TeleportGeometry>,
    automap: Option<AutomapOverlay>,
    floor_palette: Palette,
    void_palette: Palette,
    floor_distance: f64,
    void_distance: f64,
}
fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
impl ArcanePathObserver {
    pub fn new(
        settings: PathSettings,
        terrain: Option<TerrainPalette>,
        teleport: Option<TeleportGeometry>,
        automap: Option<AutomapOverlay>,
    ) -> Result<Self, ObservationError> {
        if let Some(a) = automap {
            AutomapOverlay::new(a.neutral_pixel_fraction)?;
        }
        if let Some(t) = teleport {
            TeleportGeometry::new(t.maximum_distance, t.clearance_pixels, t.preferred_distance)?;
        }
        let t = terrain.unwrap_or(TerrainPalette {
            floor_bgr: vec![
                [59, 64, 68],
                [67, 65, 65],
                [107, 108, 109],
                [111, 109, 109],
                [139, 144, 147],
            ],
            void_bgr: vec![
                [0, 0, 0],
                [8, 8, 8],
                [11, 11, 11],
                [18, 18, 18],
                [125, 126, 126],
            ],
            floor_lab_distance: 18.0,
            void_lab_distance: 16.0,
        });
        time::positive(t.floor_lab_distance, "floor_lab_distance")?;
        time::positive(t.void_lab_distance, "void_lab_distance")?;
        Ok(Self {
            settings,
            teleport,
            automap,
            floor_palette: Palette::new(t.floor_bgr)?,
            void_palette: Palette::new(t.void_bgr)?,
            floor_distance: t.floor_lab_distance,
            void_distance: t.void_lab_distance,
        })
    }
    pub fn settings(&self) -> &PathSettings {
        &self.settings
    }
    pub fn teleport(&self) -> Option<TeleportGeometry> {
        self.teleport
    }
    pub fn with_settings(&self, settings: PathSettings) -> Self {
        let mut result = self.clone();
        result.settings = settings;
        result
    }
    pub fn set_teleport(&mut self, geometry: Option<TeleportGeometry>) -> Result<(), ConfigError> {
        if let Some(t) = geometry {
            TeleportGeometry::new(t.maximum_distance, t.clearance_pixels, t.preferred_distance)?;
        }
        self.teleport = geometry;
        Ok(())
    }
    pub fn observe(
        &self,
        frame: BgrView<'_>,
        context: PathContext,
    ) -> Result<PathView, ObservationError> {
        time::nonnegative(context.captured_at, "captured_at")?;
        context.viewport.checked_in(frame.size())?;
        let r = context.viewport;
        if context.frame_size != frame.size()
            || !context.anchor[0].is_finite()
            || !context.anchor[1].is_finite()
            || !(r.left() as f64..r.right() as f64).contains(&context.anchor[0])
            || !(r.top() as f64..r.bottom() as f64).contains(&context.anchor[1])
        {
            return Err(ObservationError::invalid("anchor outside world viewport"));
        }
        let cell = self.settings.cell_pixels;
        let [columns, rows] = [r.width() / cell, r.height() / cell];
        if columns.min(rows) < 3 {
            return Err(ObservationError::invalid(
                "viewport too small for path grid",
            ));
        }
        let mut excluded = if self.automap.is_some() {
            Some(GrayFrame::zeros(frame.size())?)
        } else {
            None
        };
        if let Some(mask) = &mut excluded {
            for y in 0..frame.size().height() {
                let row = frame
                    .row(y)
                    .ok_or_else(|| ObservationError::invalid("path image row missing"))?;
                for (x, p) in row.as_chunks::<3>().0.iter().enumerate() {
                    mask.pixels_mut()[y * frame.size().width() + x] =
                        u8::from(p[0] == p[1] && p[1] == p[2] && p[0] >= 30);
                }
            }
        }
        let bounds = PixelRect::new(
            r.left(),
            r.top(),
            r.left() + columns * cell,
            r.top() + rows * cell,
        )?;
        let (patch, neutral) = grid_sample::sample(
            frame.crop(bounds)?,
            excluded
                .as_ref()
                .map(|m| m.view().crop(bounds))
                .transpose()?,
            Size::new(columns * 2, rows * 2)?,
        )?;
        let floor = self.floor_palette.distance_map(patch.view())?;
        let void = self.void_palette.distance_map(patch.view())?;
        let mut grid = GrayFrame::zeros(Size::new(columns, rows)?)?;
        for y in 0..rows {
            for x in 0..columns {
                let mut count = 0;
                for dy in 0..2 {
                    for dx in 0..2 {
                        let i = (y * 2 + dy) * columns * 2 + x * 2 + dx;
                        let f = floor.pixels()[i];
                        let v = void.pixels()[i];
                        let admitted = f <= self.floor_distance as f32
                            && (v > self.void_distance as f32 || f < v)
                            && self.automap.is_none_or(|a| {
                                neutral.as_ref().is_some_and(|n| {
                                    n.pixels()[i] < a.neutral_pixel_fraction as f32
                                })
                            });
                        count += usize::from(admitted);
                    }
                }
                grid.pixels_mut()[y * columns + x] =
                    u8::from(count as f64 / 4.0 >= self.settings.floor_fraction);
            }
        }
        let kernel = Kernel::Rectangle {
            width: 3,
            height: 3,
        };
        let grid = morphology::morphology(grid.view(), kernel, Morphology::Open)?;
        let grid = morphology::morphology(grid.view(), kernel, Morphology::Close)?;
        let mut view = PathView::from_grid(context, cell, grid)?;
        view.excluded_pixels = excluded;
        Ok(view)
    }
    pub fn from_mask(
        &self,
        mask: GrayView<'_>,
        context: PathContext,
    ) -> Result<PathView, ObservationError> {
        let cell = self.settings.cell_pixels;
        let [columns, rows] = [mask.size().width() / cell, mask.size().height() / cell];
        let mut grid = GrayFrame::zeros(Size::new(columns, rows)?)?;
        for y in 0..rows {
            for x in 0..columns {
                let mut sum = 0u64;
                for sy in y * cell..(y + 1) * cell {
                    let row = mask
                        .row(sy)
                        .ok_or_else(|| ObservationError::invalid("path mask row missing"))?;
                    sum += row[x * cell..(x + 1) * cell]
                        .iter()
                        .map(|v| u64::from(*v))
                        .sum::<u64>();
                }
                grid.pixels_mut()[y * columns + x] =
                    u8::from(sum as f64 / (cell * cell) as f64 >= self.settings.floor_fraction);
            }
        }
        PathView::from_grid(context, cell, grid)
    }
    pub fn choose(
        &self,
        view: &PathView,
        wing: Wing,
        options: ChooseOptions<'_>,
    ) -> Result<PathChoice, ObservationError> {
        let mut floor = view.floor.clone();
        let width = floor.size().width();
        for y in 0..floor.size().height() {
            for x in 0..width {
                let p = view.point([x, y]);
                if options
                    .excluded
                    .iter()
                    .any(|[l, t, r, b]| *l <= p[0] && p[0] < *r && *t <= p[1] && p[1] < *b)
                {
                    floor.pixels_mut()[y * width + x] = 0;
                }
            }
        }
        let parts = components::connected_components(floor.view(), Connectivity::Four)?;
        for (i, label) in parts.labels.pixels().iter().enumerate() {
            if *label == 0
                || parts.stats[*label as usize].area < self.settings.minimum_component_cells
            {
                floor.pixels_mut()[i] = 0;
            }
        }
        match self.teleport {
            Some(geometry) => teleport::choose(self, view, &floor, wing, geometry, options),
            None => connected::choose(self, view, &floor, wing, options),
        }
    }
}
