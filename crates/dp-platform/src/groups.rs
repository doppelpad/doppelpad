//! Which controllers Doppelpad uses: a block list of devices it ignores completely, and user
//! groups whose members act as one controller with one profile (two pads, a pad and a
//! stick, ...). [`Grouping`] wraps every gamepad backend: polling drops blocked devices and
//! merges each group into one snapshot (`group:<name>`); output sent to a group goes to all
//! of its members.

use std::sync::Arc;

use dp_core::pad::{Axis, PadState, Rgb, Rumble};
use serde::{Deserialize, Serialize};

use crate::{
    Battery, DeviceId, DeviceInfo, GamepadBackend, Leds, PadSnapshot, TriggerEffects,
    VirtualIgnoreList,
};

/// Controllers that act as one.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DeviceGroup {
    pub name: String,
    /// Device ids, the first one leading (its family names the buttons).
    pub members: Vec<String>,
}

impl DeviceGroup {
    /// What is wrong with a set of groups, if anything: each needs a name of its own and at
    /// least two controllers, and a controller can be in one group only.
    pub fn problems(groups: &[DeviceGroup]) -> Option<String> {
        for (i, g) in groups.iter().enumerate() {
            if g.name.trim().is_empty() {
                return Some("a device group needs a name".into());
            }
            if groups[..i].iter().any(|o| o.name == g.name) {
                return Some(format!("there are two device groups named {}", g.name));
            }
            if g.members.len() < 2 {
                return Some(format!(
                    "device group {} needs at least two controllers",
                    g.name
                ));
            }
            if let Some(m) = g
                .members
                .iter()
                .find(|m| groups[..i].iter().any(|o| o.members.contains(m)))
            {
                return Some(format!("{m} is in two device groups"));
            }
        }
        None
    }

    /// The id the group's combined controller goes by.
    pub fn id(&self) -> DeviceId {
        DeviceId(format!("group:{}", self.name))
    }
}

/// The block list and the groups, shared with whoever changes them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DevicePolicy {
    pub blocked: Vec<String>,
    pub groups: Vec<DeviceGroup>,
}

/// A connected device as the backends report it, before blocking and grouping.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SeenDevice {
    pub id: String,
    pub name: String,
}

/// Several controllers' states as one: buttons of any, each axis from the member that pushes
/// it furthest, motion and touch from the first member that has them.
fn merge<'a>(states: impl IntoIterator<Item = &'a PadState>) -> PadState {
    let mut out = PadState::default();
    for s in states {
        out.buttons |= s.buttons;
        for (o, v) in out.axes.iter_mut().zip(s.axes) {
            if v.abs() > o.abs() {
                *o = v;
            }
        }
        out.motion = out.motion.or(s.motion);
        if out.touch.iter().all(Option::is_none) {
            out.touch = s.touch;
        }
    }
    // Triggers never go negative.
    for a in [Axis::LeftTrigger, Axis::RightTrigger] {
        let v = out.axis(a).max(0.0);
        out.set_axis(a, v);
    }
    out
}

pub struct Grouping {
    inner: Vec<Box<dyn GamepadBackend>>,
    policy: DevicePolicy,
    /// What the backends reported at the last poll.
    seen: Vec<SeenDevice>,
    raw: Vec<PadSnapshot>,
}

impl Grouping {
    pub fn new(inner: Vec<Box<dyn GamepadBackend>>) -> Grouping {
        Grouping {
            inner,
            policy: DevicePolicy::default(),
            seen: Vec::new(),
            raw: Vec::new(),
        }
    }

    /// Ignore the blocked devices and play each group as one controller.
    pub fn set_policy(&mut self, policy: DevicePolicy) {
        self.policy = policy;
    }

    /// Every device the backends reported at the last poll, blocked and grouped ones
    /// included.
    pub fn seen(&self) -> &[SeenDevice] {
        &self.seen
    }

    /// Run `f` for every backend with each member of the group `id` names, else with `id`.
    fn each(&mut self, id: &DeviceId, mut f: impl FnMut(&mut dyn GamepadBackend, &DeviceId)) {
        let group =
            id.0.strip_prefix("group:")
                .and_then(|name| self.policy.groups.iter().find(|g| g.name == name));
        let Some(group) = group else {
            for b in &mut self.inner {
                f(b.as_mut(), id);
            }
            return;
        };
        for member in &group.members {
            let member = DeviceId(member.clone());
            for b in &mut self.inner {
                f(b.as_mut(), &member);
            }
        }
    }
}

/// The group's combined snapshot from the members that are present.
fn combine(group: &DeviceGroup, members: &[&PadSnapshot]) -> PadSnapshot {
    let lead = &members[0].info;
    let battery = Battery::emptiest(members.iter().filter_map(|m| m.battery));
    PadSnapshot {
        info: std::sync::Arc::new(DeviceInfo {
            id: group.id(),
            name: group.name.clone(),
            backend: "group".into(),
            instance_paths: members
                .iter()
                .flat_map(|m| m.info.instance_paths.iter().cloned())
                .collect(),
            joycon: None,
            has_motion: members.iter().any(|m| m.info.has_motion),
            has_touchpad: members.iter().any(|m| m.info.has_touchpad),
            has_rumble: members.iter().any(|m| m.info.has_rumble),
            ..(**lead).clone()
        }),
        state: merge(members.iter().map(|m| &m.state)),
        battery,
        raw: None,
    }
}

/// Apply the block list and the groups to what the backends report.
fn arrange(raw: &[PadSnapshot], policy: &DevicePolicy, out: &mut Vec<PadSnapshot>) {
    let blocked = |id: &DeviceId| policy.blocked.contains(&id.0);
    let grouped = |id: &DeviceId| policy.groups.iter().any(|g| g.members.contains(&id.0));
    for g in &policy.groups {
        let members: Vec<&PadSnapshot> = g
            .members
            .iter()
            .filter_map(|m| {
                raw.iter()
                    .find(|s| s.info.id.0 == *m && !blocked(&s.info.id))
            })
            .collect();
        if !members.is_empty() {
            out.push(combine(g, &members));
        }
    }
    out.extend(
        raw.iter()
            .filter(|s| !blocked(&s.info.id) && !grouped(&s.info.id))
            .cloned(),
    );
}

impl GamepadBackend for Grouping {
    fn name(&self) -> &'static str {
        "grouping"
    }

    fn poll(&mut self, out: &mut Vec<PadSnapshot>) {
        self.raw.clear();
        for b in &mut self.inner {
            b.poll(&mut self.raw);
        }
        let same = self
            .raw
            .iter()
            .map(|s| &s.info.id.0)
            .eq(self.seen.iter().map(|d| &d.id));
        if !same {
            self.seen = self
                .raw
                .iter()
                .map(|s| SeenDevice {
                    id: s.info.id.0.clone(),
                    name: s.info.name.clone(),
                })
                .collect();
        }
        arrange(&self.raw, &self.policy, out);
    }

    fn set_rumble(&mut self, id: &DeviceId, rumble: Rumble) {
        self.each(id, |b, d| b.set_rumble(d, rumble));
    }

    fn set_lightbar(&mut self, id: &DeviceId, color: Option<Rgb>) {
        self.each(id, |b, d| b.set_lightbar(d, color));
    }

    fn set_leds(&mut self, id: &DeviceId, leds: Leds) {
        self.each(id, |b, d| b.set_leds(d, leds));
    }

    fn set_trigger_effects(&mut self, id: &DeviceId, effects: TriggerEffects) {
        self.each(id, |b, d| b.set_trigger_effects(d, effects));
    }

    fn power_off(&mut self, id: &DeviceId) -> bool {
        let mut any = false;
        self.each(id, |b, d| any |= b.power_off(d));
        any
    }

    fn ignore_virtual(&mut self, ignored: &VirtualIgnoreList) {
        for b in &mut self.inner {
            b.ignore_virtual(ignored);
        }
    }

    fn set_layouts(&mut self, layouts: Arc<Vec<crate::generic::GenericLayout>>) {
        for b in &mut self.inner {
            b.set_layouts(layouts.clone());
        }
    }

    fn problems(&self) -> Vec<String> {
        self.inner.iter().flat_map(|b| b.problems()).collect()
    }

    fn rescan(&mut self) {
        for b in &mut self.inner {
            b.rescan();
        }
    }
}

#[cfg(test)]
mod tests {
    use dp_core::pad::{PadButton, Stick};

    use super::*;
    use crate::PadFamily;

    fn pad(id: &str, state: PadState) -> PadSnapshot {
        PadSnapshot {
            info: std::sync::Arc::new(DeviceInfo {
                id: DeviceId(id.into()),
                name: id.into(),
                family: PadFamily::DualSense,
                vendor_id: 0,
                product_id: 0,
                backend: "fake".into(),
                instance_paths: vec![format!("path-{id}")],
                joycon: None,
                has_motion: false,
                has_touchpad: false,
                has_rumble: true,
                mirror: false,
            }),
            state,
            battery: None,
            raw: None,
        }
    }

    #[test]
    fn groups_merge_and_blocked_devices_vanish() {
        let mut a = PadState::default();
        a.set(PadButton::A, true);
        a.set_stick(Stick::Left, (0.2, 0.0));
        let mut b = PadState::default();
        b.set(PadButton::B, true);
        b.set_stick(Stick::Left, (-0.9, 0.0));
        let raw = vec![
            pad("a", a),
            pad("b", b),
            pad("c", PadState::default()),
            pad("d", PadState::default()),
        ];
        let policy = DevicePolicy {
            blocked: vec!["d".into()],
            groups: vec![DeviceGroup {
                name: "Pair".into(),
                members: vec!["a".into(), "b".into()],
            }],
        };
        let mut out = Vec::new();
        arrange(&raw, &policy, &mut out);
        let ids: Vec<&str> = out.iter().map(|s| s.info.id.0.as_str()).collect();
        assert_eq!(ids, ["group:Pair", "c"]);
        let g = &out[0];
        assert!(g.state.pressed(PadButton::A) && g.state.pressed(PadButton::B));
        assert_eq!(g.state.stick(Stick::Left).0, -0.9, "the furthest push wins");
        assert_eq!(
            g.info.instance_paths,
            ["path-a", "path-b"],
            "both get hidden"
        );
    }

    #[test]
    fn device_groups_need_names_and_distinct_members() {
        let g = |name: &str, members: &[&str]| DeviceGroup {
            name: name.into(),
            members: members.iter().map(|m| m.to_string()).collect(),
        };
        assert_eq!(DeviceGroup::problems(&[g("Pair", &["a", "b"])]), None);
        assert!(DeviceGroup::problems(&[g("", &["a", "b"])]).is_some());
        assert!(DeviceGroup::problems(&[g("Solo", &["a"])]).is_some());
        assert!(DeviceGroup::problems(&[g("P", &["a", "b"]), g("P", &["c", "d"])]).is_some());
        assert!(DeviceGroup::problems(&[g("P", &["a", "b"]), g("Q", &["b", "c"])]).is_some());
    }
}
