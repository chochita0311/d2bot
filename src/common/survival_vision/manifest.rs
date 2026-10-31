use super::image::HudObserverOptions;
use crate::common::{
    error::ObservationError,
    frame::{BgrFrame, GrayFrame, PixelRect, Size},
    observation_assets::AssetOwner,
    vision_ops::{codec, color},
};
use serde::Deserialize;
use std::{collections::HashMap, path::Path};
#[derive(Deserialize)]
struct Manifest {
    schema_version: u32,
    layout_id: String,
    revision: String,
    frame_size: [usize; 2],
    matching: Matching,
    resources: HashMap<String, RawResource>,
    glyphs: HashMap<String, String>,
    #[serde(default)]
    glyph_variants: HashMap<String, Vec<String>>,
    belt: RawBelt,
    #[serde(default)]
    high_orb_bound: serde_json::Value,
    #[serde(default)]
    partial_orb_bound: serde_json::Value,
    #[serde(default)]
    mercenary_portrait: serde_json::Value,
}
#[derive(Deserialize)]
pub(super) struct Matching {
    pub label_score: f64,
    pub glyph_score: f64,
    pub glyph_margin: f64,
    pub belt_anchor_distance: f64,
    pub closed_anchor_distance: f64,
    pub closed_anchor_score: f64,
    pub item_distance: f64,
    pub item_margin: f64,
    pub minimum_glyph_height: usize,
    pub number_max_gap: usize,
    pub number_outline_dark_limit: u8,
}
#[derive(Deserialize)]
struct RawResource {
    label_box: [usize; 4],
    number_box: [usize; 4],
    label_search_box: [usize; 4],
    template: String,
    #[serde(default)]
    label_variants: Vec<String>,
}
#[derive(Deserialize)]
struct RawBelt {
    anchor_box: [usize; 4],
    anchor_template: String,
    closed_anchor_box: [usize; 4],
    closed_anchor_template: String,
    origin: [usize; 2],
    step: [usize; 2],
    slot_size: [usize; 2],
    items: HashMap<String, Vec<String>>,
}
#[derive(Deserialize)]
struct RawOrb {
    minimum_ratio: f64,
    minimum_row_coverage: f64,
    minimum_value: u8,
    channel_margin: u8,
    resources: HashMap<String, RawLiquid>,
}
#[derive(Deserialize)]
struct RawLiquid {
    r#box: [usize; 4],
    channel: usize,
}
#[derive(Deserialize)]
struct RawPortrait {
    r#box: [usize; 4],
    template: String,
    minimum_score: f64,
    minimum_visible: f64,
}
pub(super) struct Resource {
    pub search: PixelRect,
    pub gap: isize,
    pub labels: Vec<GrayFrame>,
}
pub(super) struct Belt {
    pub anchor_box: PixelRect,
    pub anchor: BgrFrame,
    pub closed_box: PixelRect,
    pub closed: BgrFrame,
    pub slots: Vec<PixelRect>,
    pub items: Vec<(String, Vec<BgrFrame>)>,
}
pub(super) struct OrbBound {
    pub minimum_ratio: f64,
    pub minimum_row_coverage: f64,
    pub minimum_value: u8,
    pub channel_margin: u8,
    pub resources: [(PixelRect, usize); 2],
    pub reason: &'static str,
}
pub(super) struct Portrait {
    pub region: PixelRect,
    pub image: GrayFrame,
    pub minimum_score: f64,
    pub minimum_visible: f64,
}
pub(super) struct HudAssets {
    pub size: Size,
    pub layout_id: String,
    pub revision: String,
    pub matching: Matching,
    pub resources: [Resource; 2],
    pub glyphs: Vec<(char, Vec<GrayFrame>)>,
    pub belt: Belt,
    pub bounds: Vec<OrbBound>,
    pub portrait: Option<Portrait>,
}
pub(super) fn rect([l, t, r, b]: [usize; 4], size: Size) -> Result<PixelRect, ObservationError> {
    Ok(PixelRect::new(l, t, r, b)?.checked_in(size)?)
}
fn same_size(image: Size, r: PixelRect, message: &str) -> Result<(), ObservationError> {
    if image.width() != r.width() || image.height() != r.height() {
        return Err(ObservationError::invalid(message));
    }
    Ok(())
}
impl HudAssets {
    pub fn load(directory: &Path, options: HudObserverOptions) -> Result<Self, ObservationError> {
        let raw: Manifest =
            serde_json::from_slice(&std::fs::read(directory.join("manifest.json"))?)?;
        if raw.schema_version != 1 {
            return Err(ObservationError::invalid(
                "Unsupported survival vision schema",
            ));
        }
        let size = Size::new(raw.frame_size[0], raw.frame_size[1])?;
        if raw.layout_id.trim().is_empty() || raw.revision.trim().is_empty() {
            return Err(ObservationError::invalid(
                "Layout and revision must be explicit",
            ));
        }
        let m = &raw.matching;
        if [
            m.label_score,
            m.glyph_score,
            m.glyph_margin,
            m.belt_anchor_distance,
            m.closed_anchor_distance,
            m.closed_anchor_score,
            m.item_distance,
            m.item_margin,
        ]
        .into_iter()
        .any(|v| !v.is_finite() || v <= 0.0 || v >= 1.0)
        {
            return Err(ObservationError::invalid(
                "Vision matching limits must be finite ratios in (0,1)",
            ));
        }
        if m.minimum_glyph_height == 0
            || m.number_max_gap == 0
            || m.number_outline_dark_limit == 0
            || m.number_outline_dark_limit == 255
        {
            return Err(ObservationError::invalid(
                "Positive reviewed glyph geometry and darkness required",
            ));
        }
        if raw.resources.len() != 2
            || !raw.resources.contains_key("life")
            || !raw.resources.contains_key("mana")
        {
            return Err(ObservationError::invalid(
                "Both HUD resource references required",
            ));
        }
        let owner = AssetOwner::new(directory)?;
        let gray = |name: &str| -> Result<GrayFrame, ObservationError> {
            let mut image = codec::load_gray(owner.path(name)?)?;
            for v in image.pixels_mut() {
                *v = u8::from(*v > 0);
            }
            Ok(image)
        };
        let color = |name: &str| -> Result<BgrFrame, ObservationError> {
            Ok(codec::load_color(owner.path(name)?)?)
        };
        let resource = |key: &str| -> Result<Resource, ObservationError> {
            let r = raw
                .resources
                .get(key)
                .ok_or_else(|| ObservationError::invalid("missing resource"))?;
            let label = rect(r.label_box, size)?;
            let number = rect(r.number_box, size)?;
            let search = rect(r.label_search_box, size)?;
            let labels = std::iter::once(&r.template)
                .chain(&r.label_variants)
                .map(|n| gray(n))
                .collect::<Result<Vec<_>, _>>()?;
            for image in &labels {
                same_size(
                    image.size(),
                    label,
                    "Label dimensions must match the reference box",
                )?;
            }
            if search.width() < label.width() || search.height() < label.height() {
                return Err(ObservationError::invalid(
                    "Label dimensions must match the reference box",
                ));
            }
            let gap = isize::try_from(number.left())
                .ok()
                .zip(isize::try_from(label.right()).ok())
                .and_then(|(a, b)| a.checked_sub(b))
                .ok_or_else(|| ObservationError::invalid("numeric gap overflow"))?;
            Ok(Resource {
                search,
                gap,
                labels,
            })
        };
        let resources = [resource("life")?, resource("mana")?];
        if raw.glyphs.len() != 11
            || "0123456789/"
                .chars()
                .any(|c| !raw.glyphs.contains_key(&c.to_string()))
            || raw
                .glyph_variants
                .keys()
                .any(|key| !raw.glyphs.contains_key(key))
        {
            return Err(ObservationError::invalid(
                "Complete normalized digit/slash atlas required",
            ));
        }
        let mut glyphs = Vec::new();
        for c in "0123456789/".chars() {
            let key = c.to_string();
            let name = raw
                .glyphs
                .get(&key)
                .ok_or_else(|| ObservationError::invalid("missing glyph"))?;
            let images = std::iter::once(name)
                .chain(raw.glyph_variants.get(&key).into_iter().flatten())
                .map(|n| gray(n))
                .collect::<Result<Vec<_>, _>>()?;
            if images
                .iter()
                .any(|i| i.size().width() != 40 || i.size().height() != 32)
            {
                return Err(ObservationError::invalid(
                    "Normalized glyph variant dimensions required",
                ));
            }
            glyphs.push((c, images));
        }
        let belt = raw.belt;
        if belt.step.contains(&0) || belt.slot_size.contains(&0) {
            return Err(ObservationError::invalid("Invalid belt geometry"));
        }
        let anchor_box = rect(belt.anchor_box, size)?;
        let closed_box = rect(belt.closed_anchor_box, size)?;
        let anchor = color(&belt.anchor_template)?;
        let closed = color(&belt.closed_anchor_template)?;
        same_size(
            anchor.size(),
            anchor_box,
            "Belt anchor dimensions must match its box",
        )?;
        same_size(
            closed.size(),
            closed_box,
            "Closed belt anchor dimensions must match its box",
        )?;
        let mut slots = Vec::new();
        for row in 0_usize..4 {
            for column in 0_usize..4 {
                let left = column
                    .checked_mul(belt.step[0])
                    .and_then(|n| n.checked_add(belt.origin[0]));
                let top = row
                    .checked_mul(belt.step[1])
                    .and_then(|n| n.checked_add(belt.origin[1]));
                let (left, top) = left
                    .zip(top)
                    .ok_or_else(|| ObservationError::invalid("belt geometry overflow"))?;
                let right = left
                    .checked_add(belt.slot_size[0])
                    .ok_or_else(|| ObservationError::invalid("belt geometry overflow"))?;
                let bottom = top
                    .checked_add(belt.slot_size[1])
                    .ok_or_else(|| ObservationError::invalid("belt geometry overflow"))?;
                slots.push(rect([left, top, right, bottom], size)?);
            }
        }
        if belt.items.len() != 3
            || ["purple_potion", "town_portal_scroll", "empty"]
                .into_iter()
                .any(|k| !belt.items.contains_key(k))
        {
            return Err(ObservationError::invalid(
                "Complete belt reference classes required",
            ));
        }
        let mut items = Vec::new();
        for (kind, names) in belt.items {
            if names.is_empty() {
                return Err(ObservationError::invalid(
                    "Complete belt reference classes required",
                ));
            }
            let images = names
                .iter()
                .map(|name| color(name))
                .collect::<Result<Vec<_>, _>>()?;
            for image in &images {
                same_size(image.size(), slots[0], "Item dimensions must match slots")?;
            }
            items.push((kind, images));
        }
        let mut bounds = Vec::new();
        for (enabled, value, reason) in [
            (
                options.allow_high_orb_fallback,
                raw.high_orb_bound,
                "orb_high_bound",
            ),
            (
                options.allow_partial_orb_fallback,
                raw.partial_orb_bound,
                "orb_partial_bound",
            ),
        ] {
            if !enabled {
                continue;
            }
            let bound: RawOrb = serde_json::from_value(value)?;
            if bound.resources.len() != 2
                || ["life", "mana"]
                    .into_iter()
                    .any(|k| !bound.resources.contains_key(k))
            {
                return Err(ObservationError::invalid("Reviewed orb bounds required"));
            }
            if [bound.minimum_ratio, bound.minimum_row_coverage]
                .into_iter()
                .any(|v| !v.is_finite() || v <= 0.0 || v > 1.0)
                || [bound.minimum_value, bound.channel_margin]
                    .into_iter()
                    .any(|v| v == 0 || v == 255)
            {
                return Err(ObservationError::invalid(
                    "Positive orb ratio and color bounds required",
                ));
            }
            let liquid = |key: &str| -> Result<(PixelRect, usize), ObservationError> {
                let r = bound
                    .resources
                    .get(key)
                    .ok_or_else(|| ObservationError::invalid("missing liquid"))?;
                if !matches!(r.channel, 0 | 2) {
                    return Err(ObservationError::invalid(
                        "Reviewed liquid channel required",
                    ));
                }
                Ok((rect(r.r#box, size)?, r.channel))
            };
            bounds.push(OrbBound {
                resources: [liquid("life")?, liquid("mana")?],
                minimum_ratio: bound.minimum_ratio,
                minimum_row_coverage: bound.minimum_row_coverage,
                minimum_value: bound.minimum_value,
                channel_margin: bound.channel_margin,
                reason,
            });
        }
        let portrait = if options.observe_mercenary {
            let p: RawPortrait = serde_json::from_value(raw.mercenary_portrait)?;
            if !p.minimum_score.is_finite()
                || p.minimum_score <= 0.0
                || p.minimum_score >= 1.0
                || !p.minimum_visible.is_finite()
                || !(0.5..=1.0).contains(&p.minimum_visible)
            {
                return Err(ObservationError::invalid(
                    "Reviewed mercenary portrait score and visible face fraction required",
                ));
            }
            let region = rect(p.r#box, size)?;
            let image = color::gray(color(&p.template)?.view())?;
            same_size(
                image.size(),
                region,
                "Mercenary portrait dimensions must match its box",
            )?;
            Some(Portrait {
                region,
                image,
                minimum_score: p.minimum_score,
                minimum_visible: p.minimum_visible,
            })
        } else {
            None
        };
        Ok(Self {
            size,
            layout_id: raw.layout_id,
            revision: raw.revision,
            matching: raw.matching,
            resources,
            glyphs,
            belt: Belt {
                anchor_box,
                anchor,
                closed_box,
                closed,
                slots,
                items,
            },
            bounds,
            portrait,
        })
    }
}
