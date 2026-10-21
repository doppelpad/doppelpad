//! Radial menus. A menu is open while its layer is active; a stick, the mouse or commands
//! point at a sector, and leaving the layer (or the apply command) runs the pointed one or
//! opens its sub-menu.

use std::f32::consts::TAU;

use crate::profile::{RadialApply, RadialMenu, RadialSelect};

/// Stick deflection that points at a sector.
const STICK_POINT: f32 = 0.5;
/// Mouse travel (counts) that points at a sector; longer travel is cut to this.
const MOUSE_POINT: f32 = 40.0;

/// What an open menu shows: the menu (an index into the profile's menus) and the pointed
/// sector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RadialView {
    pub menu: usize,
    pub pointed: Option<usize>,
}

#[derive(Clone, Debug)]
pub(super) struct RadialState {
    /// The menu shown now; `parents` are the menus it was opened from.
    pub menu: usize,
    parents: Vec<usize>,
    pub pointed: Option<usize>,
    /// When the menu's layer became active.
    since: u64,
    /// Mouse travel collected for pointing.
    mouse: (f32, f32),
    /// Closed by applying or going back; it opens again with its layer.
    pub closed: bool,
}

/// What a command or a closing layer asks the engine to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RadialRun {
    /// Play sector `sector` of menu `menu`.
    Sector { menu: usize, sector: usize },
}

/// The sector a direction points at: sector 0 at the top, the rest clockwise. `y` is up.
pub(super) fn sector_at(x: f32, y: f32, n: usize) -> Option<usize> {
    if n == 0 {
        return None;
    }
    let angle = x.atan2(y).rem_euclid(TAU);
    let width = TAU / n as f32;
    Some(((angle + width / 2.0) / width) as usize % n)
}

impl RadialState {
    pub(super) fn open(menu: usize, now: u64) -> RadialState {
        RadialState {
            menu,
            parents: Vec::new(),
            pointed: None,
            since: now,
            mouse: (0.0, 0.0),
            closed: false,
        }
    }

    /// The root menu: the one whose layer keeps this state open.
    pub(super) fn root(&self) -> usize {
        self.parents.first().copied().unwrap_or(self.menu)
    }

    /// What to draw now, if anything.
    pub(super) fn view(&self, menus: &[RadialMenu], now: u64) -> Option<RadialView> {
        let delay = u64::from(menus.get(self.root())?.delay_ms);
        (!self.closed && now.saturating_sub(self.since) >= delay).then_some(RadialView {
            menu: self.menu,
            pointed: self.pointed,
        })
    }

    /// Point with the menu's stick (`x`, `y` up positive) or mouse travel (`dx`, `dy` down).
    pub(super) fn point(
        &mut self,
        menus: &[RadialMenu],
        stick: Option<(f32, f32)>,
        mouse: (f32, f32),
    ) {
        let Some(m) = menus.get(self.menu) else {
            return;
        };
        if self.closed {
            return;
        }
        let n = m.sectors.len();
        match menus[self.root()].select {
            RadialSelect::LeftStick | RadialSelect::RightStick => {
                if let Some((x, y)) = stick
                    && x.hypot(y) >= STICK_POINT
                {
                    self.pointed = sector_at(x, y, n);
                }
            }
            RadialSelect::Mouse => {
                self.mouse.0 += mouse.0;
                self.mouse.1 += mouse.1;
                let len = self.mouse.0.hypot(self.mouse.1);
                if len >= MOUSE_POINT {
                    self.pointed = sector_at(self.mouse.0, -self.mouse.1, n);
                    let k = MOUSE_POINT / len;
                    self.mouse = (self.mouse.0 * k, self.mouse.1 * k);
                }
            }
            RadialSelect::Commands => {}
        }
    }

    /// Step the pointer `by` sectors clockwise.
    pub(super) fn step(&mut self, menus: &[RadialMenu], by: isize) {
        let n = menus.get(self.menu).map_or(0, |m| m.sectors.len()) as isize;
        if n == 0 || self.closed {
            return;
        }
        let next = match self.pointed {
            Some(p) => (p as isize + by).rem_euclid(n),
            None if by > 0 => 0,
            None => n - 1,
        };
        self.pointed = Some(next as usize);
    }

    /// Point at the sector in direction (`x`, `y`), `y` up.
    pub(super) fn point_dir(&mut self, menus: &[RadialMenu], x: f32, y: f32) {
        if let Some(m) = menus.get(self.menu)
            && !self.closed
        {
            self.pointed = sector_at(x, y, m.sectors.len());
        }
    }

    /// Run the pointed sector, or open its sub-menu.
    pub(super) fn apply(&mut self, menus: &[RadialMenu]) -> Option<RadialRun> {
        let p = self.pointed.filter(|_| !self.closed)?;
        let sector = menus.get(self.menu)?.sectors.get(p)?;
        if let Some(sub) = sector.submenu
            && let Some(i) = (sub as usize).checked_sub(1).filter(|&i| i < menus.len())
        {
            self.parents.push(self.menu);
            self.menu = i;
            self.pointed = None;
            self.mouse = (0.0, 0.0);
            return None;
        }
        if sector.close {
            self.closed = true;
        }
        Some(RadialRun::Sector {
            menu: self.menu,
            sector: p,
        })
    }

    /// Back to the parent menu, or close.
    pub(super) fn back(&mut self) {
        match self.parents.pop() {
            Some(m) => {
                self.menu = m;
                self.pointed = None;
                self.mouse = (0.0, 0.0);
            }
            None => self.closed = true,
        }
    }

    /// The menu's layer ended: the pointed sector of a release menu runs.
    pub(super) fn leave(&self, menus: &[RadialMenu]) -> Option<RadialRun> {
        let p = self.pointed.filter(|_| !self.closed)?;
        if menus.get(self.root())?.apply != RadialApply::Release {
            return None;
        }
        let sector = menus.get(self.menu)?.sectors.get(p)?;
        sector.submenu.is_none().then_some(RadialRun::Sector {
            menu: self.menu,
            sector: p,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::RadialSector;

    fn menu(n: usize) -> RadialMenu {
        RadialMenu {
            sectors: vec![RadialSector::default(); n],
            ..Default::default()
        }
    }

    #[test]
    fn sectors_start_at_the_top_and_go_clockwise() {
        assert_eq!(sector_at(0.0, 1.0, 4), Some(0));
        assert_eq!(sector_at(1.0, 0.0, 4), Some(1));
        assert_eq!(sector_at(0.0, -1.0, 4), Some(2));
        assert_eq!(sector_at(-1.0, 0.0, 4), Some(3));
        assert_eq!(sector_at(-0.1, 1.0, 4), Some(0), "just left of the top");
        assert_eq!(sector_at(1.0, 1.0, 8), Some(1));
    }

    #[test]
    fn submenus_and_back() {
        let mut menus = vec![menu(4), menu(2)];
        menus[0].sectors[1].submenu = Some(2);
        let mut s = RadialState::open(0, 0);
        s.point(&menus, Some((1.0, 0.0)), (0.0, 0.0));
        assert_eq!(s.pointed, Some(1));
        assert_eq!(s.apply(&menus), None, "opens the sub-menu");
        assert_eq!(s.menu, 1);
        s.step(&menus, 1);
        assert_eq!(
            s.leave(&menus),
            Some(RadialRun::Sector { menu: 1, sector: 0 })
        );
        s.back();
        assert_eq!((s.menu, s.closed), (0, false));
        s.back();
        assert!(s.closed);
        assert_eq!(s.view(&menus, 10), None);
    }

    #[test]
    fn the_mouse_points_after_some_travel() {
        let mut menus = vec![menu(4)];
        menus[0].select = RadialSelect::Mouse;
        let mut s = RadialState::open(0, 0);
        s.point(&menus, None, (10.0, 0.0));
        assert_eq!(s.pointed, None);
        s.point(&menus, None, (40.0, 0.0));
        assert_eq!(s.pointed, Some(1));
        s.point(&menus, None, (0.0, -200.0));
        assert_eq!(s.pointed, Some(0), "up is a negative mouse y");
    }
}
