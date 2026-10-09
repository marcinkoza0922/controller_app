//! The controller models the editor draws a picture of, and how a pad is recognized as one.

use serde::{Deserialize, Serialize};

/// A controller model whose picture the editor draws differently. Models share their family's
/// button names; a pad of any other model is drawn with the generic picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PadModel {
    DualShock4,
    DualSense,
    DualSenseEdge,
    /// The Switch's Pro Controller.
    ProController,
    Switch2Pro,
    /// A pair of Switch Joy-Cons used as one pad.
    JoyCons,
    JoyCons2,
    Xbox360,
    XboxOne,
    XboxSeries,
    XboxElite,
    SteamController,
    WiiUPro,
}

impl PadModel {
    /// Recognizes the model from the USB/Bluetooth vendor and product IDs, then the device
    /// name; `None` when neither gives it away.
    pub fn detect(vendor: u16, product: u16, name: &str) -> Option<Self> {
        Self::from_ids(vendor, product).or_else(|| Self::from_name(name))
    }

    fn from_ids(vendor: u16, product: u16) -> Option<Self> {
        Some(match (vendor, product) {
            (0x054c, 0x05c4 | 0x09cc) => Self::DualShock4,
            (0x054c, 0x0ce6) => Self::DualSense,
            (0x054c, 0x0df2) => Self::DualSenseEdge,
            (0x057e, 0x2009) => Self::ProController,
            (0x057e, 0x2069) => Self::Switch2Pro,
            (0x057e, 0x2066 | 0x2067) => Self::JoyCons2,
            (0x045e, 0x028e | 0x028f | 0x0291) => Self::Xbox360,
            (0x045e, 0x02d1 | 0x02dd | 0x02ea) => Self::XboxOne,
            (0x045e, 0x02e3 | 0x0b00 | 0x0b05) => Self::XboxElite,
            (0x045e, 0x0b12 | 0x0b13) => Self::XboxSeries,
            (0x28de, 0x1302) => Self::SteamController,
            _ => return None,
        })
    }

    fn from_name(name: &str) -> Option<Self> {
        let name = name.to_lowercase();
        // Most specific first: each row matches when the name has every word in it.
        let by_name: [(&[&str], Self); 14] = [
            (&["dualsense edge"], Self::DualSenseEdge),
            (&["dualsense"], Self::DualSense),
            (&["dualshock"], Self::DualShock4),
            (&["switch 2", "pro"], Self::Switch2Pro),
            (&["joy-con 2"], Self::JoyCons2),
            (&["switch 2", "joy"], Self::JoyCons2),
            (&["joy-con"], Self::JoyCons),
            (&["joycon"], Self::JoyCons),
            (&["wii", "pro controller"], Self::WiiUPro),
            (&["pro controller"], Self::ProController),
            (&["xbox 360"], Self::Xbox360),
            (&["xbox elite"], Self::XboxElite),
            (&["xbox series"], Self::XboxSeries),
            (&["xbox one"], Self::XboxOne),
        ];
        by_name
            .into_iter()
            .find(|(words, _)| words.iter().all(|w| name.contains(w)))
            .map(|(_, model)| model)
            .or_else(|| name.contains("steam controller").then_some(Self::SteamController))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn models_from_ids_and_name() {
        assert_eq!(PadModel::detect(0x054c, 0x05c4, "Wireless Controller"), Some(PadModel::DualShock4));
        assert_eq!(PadModel::detect(0x054c, 0x09cc, "Wireless Controller"), Some(PadModel::DualShock4));
        assert_eq!(PadModel::detect(0x054c, 0x0ce6, "DualSense Wireless Controller"), Some(PadModel::DualSense));
        assert_eq!(PadModel::detect(0x054c, 0x0df2, "DualSense Edge Wireless Controller"), Some(PadModel::DualSenseEdge));
        assert_eq!(PadModel::detect(0x057e, 0x2009, "Pro Controller"), Some(PadModel::ProController));
        assert_eq!(PadModel::detect(0x0000, 0x0000, "Wireless DualShock Gamepad"), Some(PadModel::DualShock4));
    }

    #[test]
    fn xbox_and_other_models_from_ids_and_name() {
        assert_eq!(PadModel::detect(0x045e, 0x028e, "Xbox 360 Controller"), Some(PadModel::Xbox360));
        assert_eq!(PadModel::detect(0x045e, 0x0b13, "Xbox Wireless Controller"), Some(PadModel::XboxSeries));
        assert_eq!(PadModel::detect(0x045e, 0x0b00, "Xbox Elite Series 2"), Some(PadModel::XboxElite));
        assert_eq!(PadModel::detect(0x045e, 0x02ea, "Xbox One S Controller"), Some(PadModel::XboxOne));
        assert_eq!(PadModel::detect(0x057e, 0x2069, "Nintendo Switch 2 Pro Controller"), Some(PadModel::Switch2Pro));
        assert_eq!(PadModel::detect(0x0000, 0x0000, "Nintendo Switch Combined Joy-Cons"), Some(PadModel::JoyCons));
        assert_eq!(PadModel::detect(0x0000, 0x0000, "Nintendo Wii Remote Pro Controller"), Some(PadModel::WiiUPro));
        assert_eq!(PadModel::detect(0x28de, 0x1302, "Steam Controller"), Some(PadModel::SteamController));
        assert_eq!(PadModel::detect(0x045e, 0x1234, "Xbox Wireless Controller"), None, "a model it can't tell apart");
        assert_eq!(PadModel::detect(0x2dc8, 0x3106, "8BitDo Ultimate 2C"), None, "only the models with a picture of their own");
    }
}
