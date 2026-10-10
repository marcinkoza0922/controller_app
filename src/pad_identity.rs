//! Which controller the virtual pad presents itself as. It's an Xbox 360 pad unless the game
//! says it supports the physical controller's kind, in which case games see that kind instead.

use serde::{Deserialize, Serialize};

use crate::info::{PadFamily, PadModel};

/// The other controllers a game supports, as its author ticked them on the Details tab. A
/// physical controller of a ticked kind makes the daemon present itself as that kind while the
/// game is active.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ControllerSupport {
    pub dualshock: bool,
    pub dualsense: bool,
    pub switch_pro: bool,
}

/// The controller the virtual pad presents itself as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PadIdentity {
    Xbox360,
    DualShock4,
    DualSense,
    SwitchPro,
}

impl PadIdentity {
    /// The identity for a physical controller of `model` under a game's `support`. Anything the
    /// game doesn't list, or a controller that isn't one of the three, gives the Xbox 360 pad.
    pub fn choose(model: Option<PadModel>, support: ControllerSupport) -> Self {
        match model {
            Some(PadModel::DualShock4) if support.dualshock => Self::DualShock4,
            Some(PadModel::DualSense | PadModel::DualSenseEdge) if support.dualsense => Self::DualSense,
            Some(PadModel::ProController | PadModel::Switch2Pro) if support.switch_pro => Self::SwitchPro,
            _ => Self::Xbox360,
        }
    }

    /// What the pad presents as: `forced` when a debug session forces one, else [`Self::choose`].
    pub fn resolve(forced: Option<Self>, model: Option<PadModel>, support: ControllerSupport) -> Self {
        forced.unwrap_or_else(|| Self::choose(model, support))
    }

    /// The short name the debug command line uses, the same words as `model` for these kinds.
    pub fn slug(self) -> &'static str {
        match self {
            Self::Xbox360 => "xbox360",
            Self::DualShock4 => "dualshock4",
            Self::DualSense => "dualsense",
            Self::SwitchPro => "pro-controller",
        }
    }

    /// The identity a [`Self::slug`] names.
    pub fn from_slug(slug: &str) -> Option<Self> {
        [Self::Xbox360, Self::DualShock4, Self::DualSense, Self::SwitchPro].into_iter().find(|i| i.slug() == slug)
    }

    /// The family whose button glyphs this identity's buttons are drawn with.
    pub fn family(self) -> PadFamily {
        match self {
            Self::Xbox360 => PadFamily::Xbox,
            Self::DualShock4 | Self::DualSense => PadFamily::PlayStation,
            Self::SwitchPro => PadFamily::Nintendo,
        }
    }

    /// The USB vendor, product and version IDs. Games and Steam pick a controller's mapping by
    /// these, so they're the real ones.
    pub fn usb_id(self) -> (u16, u16, u16) {
        match self {
            Self::Xbox360 => (0x045e, 0x028e, 0x0110),
            Self::DualShock4 => (0x054c, 0x09cc, 0x8100),
            Self::DualSense => (0x054c, 0x0ce6, 0x8111),
            Self::SwitchPro => (0x057e, 0x2009, 0x8000),
        }
    }

    /// The device's name after the "Padwight Virtual" prefix.
    pub fn name(self) -> &'static str {
        match self {
            Self::Xbox360 => "Pad",
            Self::DualShock4 => "Wireless Controller",
            Self::DualSense => "DualSense Wireless Controller",
            Self::SwitchPro => "Pro Controller",
        }
    }

    /// Whether the face buttons use the xpad driver's labels, where the kernel's north button is
    /// the west one. The other kinds follow the gamepad spec's positions.
    pub fn xpad_labels(self) -> bool {
        self == Self::Xbox360
    }

    /// Whether the Nintendo button layout's swapped labels apply while presenting as this kind,
    /// given the `setting`. A Switch Pro presented as itself already shows its own labels, so the
    /// setting is ignored for it.
    pub fn applies_nintendo_layout(self, setting: bool) -> bool {
        setting && self != Self::SwitchPro
    }

    /// Whether the face buttons send the letters on their Nintendo labels instead of their
    /// positions: a Nintendo pad whose layout applies, and whose face buttons are all still at
    /// their defaults (`faces_default`). Any reassigned face button means the user chose the
    /// layout, so the positions are kept.
    pub fn swaps_face_output(self, nintendo_pad: bool, setting: bool, faces_default: bool) -> bool {
        nintendo_pad && faces_default && self.applies_nintendo_layout(setting)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: ControllerSupport = ControllerSupport { dualshock: true, dualsense: true, switch_pro: true };

    #[test]
    fn a_ticked_kind_is_presented_for_its_own_controller() {
        assert_eq!(PadIdentity::choose(Some(PadModel::DualShock4), ALL), PadIdentity::DualShock4);
        assert_eq!(PadIdentity::choose(Some(PadModel::DualSense), ALL), PadIdentity::DualSense);
        assert_eq!(PadIdentity::choose(Some(PadModel::DualSenseEdge), ALL), PadIdentity::DualSense);
        assert_eq!(PadIdentity::choose(Some(PadModel::ProController), ALL), PadIdentity::SwitchPro);
        assert_eq!(PadIdentity::choose(Some(PadModel::Switch2Pro), ALL), PadIdentity::SwitchPro);
    }

    #[test]
    fn a_forced_identity_wins_over_the_game_and_the_controller() {
        let forced = Some(PadIdentity::DualSense);
        assert_eq!(PadIdentity::resolve(forced, Some(PadModel::XboxSeries), ControllerSupport::default()), PadIdentity::DualSense);
        assert_eq!(PadIdentity::resolve(None, Some(PadModel::XboxSeries), ControllerSupport::default()), PadIdentity::Xbox360);
    }

    #[test]
    fn slugs_round_trip() {
        for id in [PadIdentity::Xbox360, PadIdentity::DualShock4, PadIdentity::DualSense, PadIdentity::SwitchPro] {
            assert_eq!(PadIdentity::from_slug(id.slug()), Some(id));
        }
        assert_eq!(PadIdentity::from_slug("generic"), None);
    }

    #[test]
    fn an_unticked_kind_or_another_controller_is_xinput() {
        let dualsense_only = ControllerSupport { dualsense: true, ..Default::default() };
        assert_eq!(PadIdentity::choose(Some(PadModel::DualShock4), dualsense_only), PadIdentity::Xbox360);
        assert_eq!(PadIdentity::choose(Some(PadModel::XboxSeries), ALL), PadIdentity::Xbox360);
        assert_eq!(PadIdentity::choose(None, ALL), PadIdentity::Xbox360);
        assert_eq!(PadIdentity::choose(Some(PadModel::DualSense), ControllerSupport::default()), PadIdentity::Xbox360);
    }

    #[test]
    fn the_nintendo_layout_is_ignored_only_for_a_switch_pro() {
        assert!(PadIdentity::Xbox360.applies_nintendo_layout(true));
        assert!(PadIdentity::DualSense.applies_nintendo_layout(true));
        assert!(!PadIdentity::SwitchPro.applies_nintendo_layout(true));
        assert!(!PadIdentity::Xbox360.applies_nintendo_layout(false));
    }

    #[test]
    fn the_face_swap_needs_a_nintendo_pad_the_layout_and_default_face_buttons() {
        assert!(PadIdentity::Xbox360.swaps_face_output(true, true, true));
        assert!(!PadIdentity::Xbox360.swaps_face_output(false, true, true));
        assert!(!PadIdentity::Xbox360.swaps_face_output(true, false, true));
        assert!(!PadIdentity::Xbox360.swaps_face_output(true, true, false));
        assert!(!PadIdentity::SwitchPro.swaps_face_output(true, true, true));
    }

    #[test]
    fn each_identity_has_its_family_and_label_convention() {
        assert_eq!(PadIdentity::DualSense.family(), PadFamily::PlayStation);
        assert_eq!(PadIdentity::SwitchPro.family(), PadFamily::Nintendo);
        assert_eq!(PadIdentity::Xbox360.family(), PadFamily::Xbox);
        assert!(PadIdentity::Xbox360.xpad_labels());
        assert!(!PadIdentity::DualShock4.xpad_labels());
        assert_eq!(PadIdentity::DualShock4.usb_id().0, 0x054c);
    }
}
