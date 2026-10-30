//! Radial menus and crosshairs.

use std::collections::HashMap;

use dp_core::profile::{
    Action, Command, Crosshair, CrosshairShape, Magnifier, Profile, RadialApply, RadialMenu,
    RadialPosition, RadialSector, RadialSelect,
};
use serde_json::Value;

use super::Importer;
use super::json::{array, boolean, enum_str, get, int, layer_id, rgb, text};

impl<'a> Importer<'a> {
    /// Circles become radial menus (in id order) and their sectors keep their order. Root
    /// circles open with the radial-menu layer; the others are sub-menus.
    pub(super) fn radial_menus(&mut self, p: &mut Profile) {
        let mut circles: Vec<&Value> = array(self.root, "radialMenuCircles").iter().collect();
        if circles.is_empty() {
            return;
        }
        circles.sort_by_key(|c| int(c, "id").unwrap_or(i64::MAX));
        let sectors = array(self.root, "radialMenuSectors");
        let layer = array(self.root, "shifts")
            .iter()
            .find(|s| enum_str(s, "type").as_deref() == Some("radialmenu"))
            .and_then(|s| layer_id(s, "id"));
        let index: HashMap<i64, usize> = circles
            .iter()
            .enumerate()
            .filter_map(|(i, c)| int(c, "id").map(|id| (id, i)))
            .collect();
        let mut icons = false;
        for (i, c) in circles.iter().enumerate() {
            let root = get(c, "parentSectorId").is_none();
            let mut menu = RadialMenu {
                name: format!("Menu {}", i + 1),
                layer: if root { layer.unwrap_or(0) as u8 } else { 0 },
                ..Default::default()
            };
            if boolean(c, "isDelayBeforeOpening") {
                let max = i64::from(RadialMenu::MAX_DELAY_MS);
                menu.delay_ms = int(c, "delayBeforeOpening").unwrap_or(500).clamp(0, max) as u32;
            }
            menu.position = position(
                enum_str(c, "horizontalAlignment").as_deref(),
                enum_str(c, "verticalAlignment").as_deref(),
            );
            if boolean(c, "isTintedBackground") {
                let t = get(c, "tintBackground")
                    .and_then(Value::as_f64)
                    .unwrap_or(40.0);
                menu.tint = (if t > 1.0 { t / 100.0 } else { t }).clamp(0.0, 1.0) as f32;
            }
            for sid in array(c, "sectors").iter().filter_map(Value::as_i64) {
                let Some(s) = sectors.iter().find(|s| int(s, "id") == Some(sid)) else {
                    continue;
                };
                icons |= get(s, "icon").is_some();
                self.radial_sectors.insert(sid, (i, menu.sectors.len()));
                menu.sectors.push(RadialSector {
                    label: text(s, "description").unwrap_or_default(),
                    color: get(s, "color").and_then(rgb).unwrap_or_default(),
                    submenu: int(s, "childCircleId")
                        .and_then(|c| index.get(&c))
                        .map(|&m| m as u8 + 1),
                    close: get(s, "isCloseOnApply")
                        .and_then(Value::as_bool)
                        .unwrap_or(true),
                    ..Default::default()
                });
            }
            p.radial_menus.push(menu);
        }
        if layer.is_none() {
            self.diags.warn(
                "radialMenuCircles",
                "no radial-menu layer; the menus only open as sub-menus",
            );
        }
        if icons {
            self.diags
                .info("radialMenuSectors", "sector icons are not imported");
        }
    }

    /// A mapping on a sector mask: its macro becomes the sector's actions.
    pub(super) fn radial_sector_mapping(&mut self, m: &Value, p: &mut Profile, path: &str) -> bool {
        let Some(&sector) = get(m, "condition")
            .and_then(|c| get(c, "mask"))
            .and_then(|k| int(k, "id"))
            .and_then(|id| self.radial_masks.get(&id))
        else {
            return false;
        };
        let Some(&(menu, k)) = self.radial_sectors.get(&sector) else {
            self.diags
                .warn(path, "mapping of an unknown radial-menu sector");
            return true;
        };
        for (j, item) in array(m, "macros").iter().enumerate() {
            if let Some(a) = self.macro_item(item, &format!("{path}.macros[{j}]")) {
                p.radial_menus[menu].sectors[k].actions.push(a);
            }
        }
        true
    }

    /// Menus no mapping navigates point with the right stick; without an apply command they
    /// run the pointed sector when their layer ends.
    pub(super) fn finish_radial(p: &mut Profile) {
        let uses = |want: &dyn Fn(&Command) -> bool| {
            p.mappings
                .iter()
                .flat_map(|m| &m.actions)
                .any(|a| matches!(a, Action::Command { command } if want(command)))
        };
        let apply = uses(&|c| *c == Command::RadialApply);
        let navigate = uses(&|c| {
            matches!(
                c,
                Command::RadialNext
                    | Command::RadialPrev
                    | Command::RadialUp
                    | Command::RadialDown
                    | Command::RadialLeft
                    | Command::RadialRight
            )
        });
        for menu in &mut p.radial_menus {
            menu.apply = if apply {
                RadialApply::Command
            } else {
                RadialApply::Release
            };
            menu.select = if navigate {
                RadialSelect::Commands
            } else {
                RadialSelect::RightStick
            };
        }
    }

    pub(super) fn crosshairs(&mut self, p: &mut Profile) {
        let mut items: Vec<&Value> = array(self.root, "crosshairs").iter().collect();
        items.sort_by_key(|c| int(c, "id").unwrap_or(i64::MAX));
        let num = |c: &Value, key: &str| get(c, key).and_then(Value::as_f64);
        let mut shapes = false;
        for (i, c) in items.into_iter().enumerate() {
            if i == Crosshair::MAX_PER_PROFILE {
                self.diags.warn(
                    "crosshairs",
                    format!(
                        "only the first {} crosshairs were kept",
                        Crosshair::MAX_PER_PROFILE
                    ),
                );
                break;
            }
            let d = Crosshair::default();
            shapes |= get(c, "crosshairType").is_some();
            let magnifier = boolean(c, "useMagnifier").then(|| Magnifier {
                zoom: within(num(c, "magnification").unwrap_or(2.0), &Magnifier::ZOOM),
                size: within(
                    num(c, "magnificationSizePercents").map_or(0.2, |v| v / 100.0),
                    &Magnifier::SIZE,
                ),
            });
            let delta = |key| int(c, key).unwrap_or(0).clamp(-10_000, 10_000) as i16;
            p.crosshairs.push(Crosshair {
                name: text(c, "name").unwrap_or_default(),
                shape: CrosshairShape::Cross,
                color: get(c, "color").and_then(rgb).unwrap_or(d.color),
                opacity: num(c, "opacity").map_or(1.0, |v| (v / 100.0).clamp(0.0, 1.0) as f32),
                size: int(c, "size").map_or(d.size, |v| {
                    let [lo, hi] = [Crosshair::SIZE.start(), Crosshair::SIZE.end()];
                    v.clamp(i64::from(*lo), i64::from(*hi)) as u16
                }),
                thickness: int(c, "thickness").map_or(d.thickness, |v| {
                    let [lo, hi] = [Crosshair::THICKNESS.start(), Crosshair::THICKNESS.end()];
                    v.clamp(i64::from(*lo), i64::from(*hi)) as u8
                }),
                outline: num(c, "edgingThickness").map_or(0, |v| {
                    (v / 5.0)
                        .round()
                        .clamp(0.0, f64::from(Crosshair::MAX_OUTLINE)) as u8
                }),
                outline_color: get(c, "edgingColor")
                    .and_then(rgb)
                    .unwrap_or(d.outline_color),
                offset: [delta("horizontalDelta"), delta("verticalDelta")],
                visible: get(c, "useCrosshair")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
                magnifier,
                layers: Vec::new(),
            });
            if let Some(g) = text(c, "guid") {
                self.crosshair_guids.insert(g, i);
            }
            self.crosshair_ids.push(int(c, "id").unwrap_or(-1));
        }
        if shapes {
            self.diags.info(
                "crosshairs",
                "crosshairs are drawn with Doppelpad's own shapes; each became a cross",
            );
        }
    }
}

/// A circle's place from its alignments (`left`, `center`, `right`, `stretch`; `top`,
/// `center`, `bottom`, `stretch`). Stretch, missing and unknown values are the centre.
fn position(horizontal: Option<&str>, vertical: Option<&str>) -> RadialPosition {
    use RadialPosition as P;
    match (horizontal, vertical) {
        (Some("left"), Some("top")) => P::TopLeft,
        (Some("right"), Some("top")) => P::TopRight,
        (Some("left"), Some("bottom")) => P::BottomLeft,
        (Some("right"), Some("bottom")) => P::BottomRight,
        (Some("left"), _) => P::Left,
        (Some("right"), _) => P::Right,
        (_, Some("top")) => P::Top,
        (_, Some("bottom")) => P::Bottom,
        _ => P::Center,
    }
}

/// `v` as an `f32` clamped into `range`.
fn within(v: f64, range: &std::ops::RangeInclusive<f32>) -> f32 {
    (v as f32).clamp(*range.start(), *range.end())
}
