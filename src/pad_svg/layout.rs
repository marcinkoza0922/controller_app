//! Where each controller model's parts sit in the drawing, traced from pictures of the real
//! pads and scaled to fit the 420 x 275 canvas. The generic layout is the fallback.

use crate::info::PadModel;

/// A round part: x, y and radius.
pub type Dot = (f32, f32, f32);

/// How a model's D-pad is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DpadKind {
    /// A plus-shaped cross.
    Cross,
    /// A round faceted disc (Xbox Series, Elite).
    Disc,
    /// Four separate round buttons (Joy-Con).
    Buttons,
}

/// Something drawn on the body that isn't an input: a touchpad, a capture button, a light.
#[derive(Debug, Clone, Copy)]
pub enum Mark {
    /// x, y, width, height, corner radius.
    Rect(f32, f32, f32, f32, f32),
    /// x, y, radius.
    Circle(f32, f32, f32),
    /// x, y, width, height, corner radius, fill: something the pad carries in a color of its
    /// own, like the DualSense's light strips or a Joy-Con's rail.
    Strip(f32, f32, f32, f32, f32, &'static str),
}

#[derive(Debug, Clone, Copy)]
pub struct Layout {
    pub outline: &'static str,
    pub left_stick: (f32, f32),
    pub right_stick: (f32, f32),
    /// Radius of the stick's well; the cap and its travel scale from it.
    pub stick_r: f32,
    pub dpad: (f32, f32),
    pub dpad_kind: DpadKind,
    /// How far the D-pad reaches from its center.
    pub dpad_reach: f32,
    /// The center of the face buttons' diamond.
    pub face: (f32, f32),
    /// How far each face button sits from `face`.
    pub face_gap: f32,
    pub face_r: f32,
    pub select: Dot,
    pub guide: Dot,
    pub start: Dot,
    /// The bumpers' and triggers' center x, left and right.
    pub shoulders: (f32, f32),
    /// The bumper's width; the trigger above it is half as wide.
    pub shoulder_w: f32,
    /// Where the body's top edge runs under the shoulders: the bumper is drawn sitting on it.
    pub shoulder_y: f32,
    pub marks: &'static [Mark],
}

impl Layout {
    /// The layout for `model`; a pad of no known model gets the generic one.
    pub fn of(model: Option<PadModel>) -> &'static Layout {
        match model {
            Some(PadModel::DualShock4) => &DUALSHOCK_4,
            Some(PadModel::DualSense) => &DUALSENSE,
            Some(PadModel::DualSenseEdge) => &DUALSENSE_EDGE,
            Some(PadModel::ProController) => &PRO_CONTROLLER,
            Some(PadModel::Switch2Pro) => &SWITCH_2_PRO,
            Some(PadModel::JoyCons) => &JOY_CONS,
            Some(PadModel::JoyCons2) => &JOY_CONS_2,
            Some(PadModel::Xbox360) => &XBOX_360,
            Some(PadModel::XboxOne) => &XBOX_ONE,
            Some(PadModel::XboxSeries) => &XBOX_SERIES,
            Some(PadModel::XboxElite) => &XBOX_ELITE,
            Some(PadModel::SteamController) => &STEAM_CONTROLLER,
            Some(PadModel::WiiUPro) => &WII_U_PRO,
            None => &GENERIC,
        }
    }

    /// The face button offsets from `face`: south, east, west, north.
    pub fn face_offsets(&self) -> [(f32, f32); 4] {
        let g = self.face_gap;
        [(0.0, g), (g, 0.0), (-g, 0.0), (0.0, -g)]
    }

    /// The top of the bumper bar, just above where the body's edge runs.
    pub fn bumper_top(&self) -> f32 {
        self.shoulder_y - super::BUMPER_ABOVE
    }

    /// The top of the trigger: it stands right on the bumper it belongs to.
    pub fn trigger_top(&self) -> f32 {
        self.bumper_top() - super::TRIGGER_HEIGHT
    }
}

const DUALSHOCK_4: Layout = Layout {
    outline: "M 84.8 57.7 C 114.1 50.0 145.1 54.6 160.5 56.2 L 261.0 56.2 C 276.5 54.6 307.4 50.0 336.8 57.7 C 361.5 69.3 377.0 111.8 377.0 162.1 C 377.0 212.3 361.5 243.3 342.2 258.7 C 326.7 268.0 307.4 258.7 295.8 239.4 C 284.2 212.3 276.5 193.0 257.2 185.3 L 160.5 185.3 C 141.2 193.0 133.5 212.3 123.4 239.4 C 111.8 258.7 92.5 268.0 77.0 258.7 C 57.7 243.3 43.0 212.3 43.0 162.1 C 43.0 111.8 58.5 69.3 84.8 57.7Z",
    left_stick: (156.7, 152.0),
    right_stick: (267.2, 152.0),
    stick_r: 30.9,
    dpad: (98.7, 108.0),
    dpad_kind: DpadKind::Cross,
    dpad_reach: 30.9,
    face: (319.0, 108.0),
    face_gap: 27.1,
    face_r: 13.1,
    select: (139.7, 74.7, 6.2),
    guide: (210.0, 156.7, 6.2),
    start: (281.1, 74.7, 6.2),
    shoulders: (108.0, 313.6),
    shoulder_w: 54.1,
    shoulder_y: 54.0,
    // The touchpad, with the light bar glowing along the body's top edge behind it.
    marks: &[Mark::Rect(156.7, 57.7, 112.1, 61.8, 6.2), Mark::Strip(170.0, 55.0, 82.0, 3.5, 1.75, super::LIGHT)],
};

const DUALSENSE: Layout = Layout {
    outline: "M 84.8 62.2 C 94.9 50.0 122.0 52.0 140.9 55.4 L 277.7 55.4 C 298.0 52.0 325.1 50.0 335.2 62.2 C 350.1 102.8 356.9 177.3 350.8 224.7 C 348.1 248.4 342.0 263.9 328.5 266.6 C 314.9 268.0 304.8 251.8 294.6 231.4 C 281.1 201.0 254.0 180.7 210.0 180.7 C 166.0 180.7 138.9 201.0 125.4 231.4 C 115.2 251.8 105.1 268.0 91.5 266.6 C 78.0 263.9 71.9 248.4 69.2 224.7 C 63.1 177.3 69.9 102.8 84.8 62.2Z",
    left_stick: (155.8, 153.6),
    right_stick: (257.4, 153.6),
    stick_r: 28.4,
    dpad: (103.0, 109.6),
    dpad_kind: DpadKind::Cross,
    dpad_reach: 27.1,
    face: (307.5, 109.6),
    face_gap: 25.0,
    face_r: 11.0,
    select: (128.8, 74.4, 4.7),
    guide: (206.6, 155.6, 5.4),
    start: (285.8, 74.4, 4.7),
    shoulders: (116.0, 301.0),
    shoulder_w: 48.0,
    shoulder_y: 53.0,
    // The touchpad, with the light strips that run along its sides.
    marks: &[Mark::Rect(142.3, 55.4, 130.0, 67.7, 6.8), Mark::Strip(136.5, 58.0, 4.5, 62.0, 2.2, super::LIGHT), Mark::Strip(273.0, 58.0, 4.5, 62.0, 2.2, super::LIGHT)],
};

const DUALSENSE_EDGE: Layout = Layout {
    outline: "M 84.8 62.2 C 94.9 50.0 122.0 52.0 140.9 55.4 L 277.7 55.4 C 298.0 52.0 325.1 50.0 335.2 62.2 C 350.1 102.8 356.9 177.3 350.8 224.7 C 348.1 248.4 342.0 263.9 328.5 266.6 C 314.9 268.0 304.8 251.8 294.6 231.4 C 281.1 201.0 254.0 180.7 210.0 180.7 C 166.0 180.7 138.9 201.0 125.4 231.4 C 115.2 251.8 105.1 268.0 91.5 266.6 C 78.0 263.9 71.9 248.4 69.2 224.7 C 63.1 177.3 69.9 102.8 84.8 62.2Z",
    left_stick: (155.8, 153.6),
    right_stick: (257.4, 153.6),
    stick_r: 28.4,
    dpad: (103.0, 109.6),
    dpad_kind: DpadKind::Cross,
    dpad_reach: 27.1,
    face: (307.5, 109.6),
    face_gap: 25.0,
    face_r: 11.0,
    select: (128.8, 74.4, 4.7),
    guide: (206.6, 155.6, 5.4),
    start: (285.8, 74.4, 4.7),
    shoulders: (116.0, 301.0),
    shoulder_w: 48.0,
    shoulder_y: 53.0,
    marks: &[Mark::Rect(142.3, 55.4, 130.0, 67.7, 6.8), Mark::Strip(136.5, 58.0, 4.5, 62.0, 2.2, super::LIGHT), Mark::Strip(273.0, 58.0, 4.5, 62.0, 2.2, super::LIGHT), Mark::Rect(122.0, 170.5, 16.2, 8.1, 2.0), Mark::Rect(281.8, 170.5, 16.2, 8.1, 2.0)],
};

const PRO_CONTROLLER: Layout = Layout {
    outline: "M 91.5 66.3 C 102.4 52.7 135.1 50.0 162.3 51.1 L 265.9 51.1 C 293.1 50.0 325.8 52.7 336.7 68.0 C 353.1 107.2 361.2 178.1 361.2 221.7 C 361.2 254.4 344.9 268.0 325.8 262.6 C 309.5 257.1 298.6 232.6 287.7 216.2 C 279.5 205.3 271.3 201.0 255.0 201.0 L 165.0 201.0 C 148.7 201.0 140.5 205.3 132.3 216.2 C 121.4 232.6 110.5 257.1 94.2 262.6 C 75.1 268.0 58.8 254.4 59.9 221.7 C 59.9 178.1 69.7 107.2 83.3 69.1 C 86.0 62.5 89.3 68.0 91.5 66.3Z",
    left_stick: (124.2, 112.7),
    right_stick: (249.5, 156.3),
    stick_r: 28.3,
    dpad: (161.2, 157.4),
    dpad_kind: DpadKind::Cross,
    dpad_reach: 25.1,
    face: (293.1, 113.8),
    face_gap: 22.9,
    face_r: 12.0,
    select: (171.6, 89.8, 7.6),
    guide: (233.2, 113.8, 7.6),
    start: (248.4, 89.8, 7.6),
    shoulders: (140.5, 293.1),
    shoulder_w: 70.9,
    shoulder_y: 51.0,
    marks: &[Mark::Rect(180.3, 107.2, 13.1, 13.1, 2.2)],
};

const SWITCH_2_PRO: Layout = Layout {
    outline: "M 58.9 72.1 C 86.6 50.0 130.9 50.0 158.5 52.2 L 285.8 52.2 C 319.0 50.0 352.2 52.2 376.5 74.3 C 398.7 107.5 405.3 151.8 402.0 196.1 C 398.7 240.3 385.4 268.0 357.7 268.0 C 335.6 268.0 321.2 240.3 313.5 227.1 C 302.4 209.4 285.8 201.6 263.7 201.6 L 158.5 201.6 C 130.9 201.6 114.3 212.7 103.2 229.3 C 88.8 251.4 75.5 268.0 53.4 268.0 C 25.8 268.0 14.7 240.3 14.7 196.1 C 14.7 151.8 31.3 96.5 58.9 72.1Z",
    left_stick: (103.2, 105.3),
    right_stick: (261.5, 160.7),
    stick_r: 24.3,
    dpad: (147.5, 159.6),
    dpad_kind: DpadKind::Cross,
    dpad_reach: 26.6,
    face: (321.2, 109.8),
    face_gap: 31.0,
    face_r: 15.5,
    select: (161.9, 76.6, 7.7),
    guide: (231.6, 109.8, 8.9),
    start: (254.8, 76.6, 7.7),
    shoulders: (110.0, 308.0),
    shoulder_w: 72.0,
    shoulder_y: 53.0,
    marks: &[Mark::Rect(150.8, 100.9, 17.7, 17.7, 3.3), Mark::Circle(206.1, 193.9, 8.9)],
};

const JOY_CONS: Layout = Layout {
    outline: "M 93.6 50.0 L 145.5 50.0 C 152.5 50.0 156.0 54.4 156.0 61.3 L 156.0 256.7 C 156.0 263.6 152.5 268.0 145.5 268.0 L 115.4 268.0 C 89.2 268.0 71.8 246.2 71.8 215.7 L 71.8 93.6 C 71.8 67.4 76.1 50.0 93.6 50.0Z M 264.0 61.3 C 264.0 54.4 267.5 50.0 274.5 50.0 L 324.7 50.0 C 344.7 50.0 348.2 67.4 348.2 93.6 L 348.2 215.7 C 348.2 246.2 330.8 268.0 307.2 268.0 L 274.5 268.0 C 267.5 268.0 264.0 263.6 264.0 256.7Z",
    left_stick: (111.0, 104.9),
    right_stick: (309.8, 165.1),
    stick_r: 22.7,
    dpad: (112.8, 165.1),
    dpad_kind: DpadKind::Buttons,
    dpad_reach: 20.9,
    face: (309.8, 104.1),
    face_gap: 15.7,
    face_r: 7.8,
    select: (144.0, 73.5, 4.4),
    guide: (295.9, 202.6, 7.0),
    start: (276.0, 71.8, 4.4),
    shoulders: (115.4, 307.2),
    shoulder_w: 61.0,
    shoulder_y: 50.0,
    // The rail each half locks onto the console (or the other half) with.
    marks: &[Mark::Rect(150.0, 66.0, 6.0, 186.0, 2.0), Mark::Rect(264.0, 66.0, 6.0, 186.0, 2.0), Mark::Rect(114.5, 195.6, 14.0, 14.0, 2.6)],
};

const JOY_CONS_2: Layout = Layout {
    outline: "M 96.0 50.0 L 145.5 50.0 C 152.5 50.0 156.0 54.4 156.0 61.3 L 156.0 256.7 C 156.0 263.6 152.5 268.0 145.5 268.0 L 115.4 268.0 C 87.0 268.0 71.8 243.0 71.8 212.0 L 71.8 96.0 C 71.8 66.0 78.0 50.0 96.0 50.0Z M 264.0 61.3 C 264.0 54.4 267.5 50.0 274.5 50.0 L 324.0 50.0 C 342.0 50.0 348.2 66.0 348.2 96.0 L 348.2 212.0 C 348.2 243.0 333.0 268.0 304.6 268.0 L 274.5 268.0 C 267.5 268.0 264.0 263.6 264.0 256.7Z",
    left_stick: (111.0, 104.9),
    right_stick: (309.8, 165.1),
    stick_r: 22.7,
    dpad: (112.8, 165.1),
    dpad_kind: DpadKind::Buttons,
    dpad_reach: 20.9,
    face: (309.8, 104.1),
    face_gap: 15.7,
    face_r: 7.8,
    select: (144.0, 73.5, 4.4),
    guide: (295.9, 202.6, 7.0),
    start: (276.0, 71.8, 4.4),
    shoulders: (115.4, 307.2),
    shoulder_w: 61.0,
    shoulder_y: 50.0,
    // Blue and coral rails, and the C button the Switch 2 halves add below Home.
    marks: &[
        Mark::Strip(150.0, 66.0, 6.0, 186.0, 2.0, "#3ab7f0"),
        Mark::Strip(264.0, 66.0, 6.0, 186.0, 2.0, "#ff7059"),
        Mark::Rect(114.5, 195.6, 14.0, 14.0, 2.6),
        Mark::Circle(295.9, 221.8, 5.2),
    ],
};

const XBOX_360: Layout = Layout {
    outline: "M 110.0 61.6 C 127.2 50.9 148.7 57.3 170.2 67.2 C 191.7 72.4 234.7 71.5 260.5 67.2 C 286.3 54.3 307.8 50.0 325.0 61.6 C 344.4 76.7 353.0 141.2 350.0 197.1 C 348.7 231.5 345.7 259.4 329.3 265.0 C 314.3 268.0 305.7 257.3 294.9 242.2 C 282.0 222.9 260.5 206.5 230.4 205.7 L 191.7 205.7 C 157.3 206.5 135.8 222.9 122.9 242.2 C 112.2 257.3 103.6 268.0 88.5 265.0 C 73.5 261.6 70.5 231.5 69.2 197.1 C 67.0 141.2 77.8 81.0 110.0 61.6Z",
    left_stick: (129.4, 89.6),
    right_stick: (251.9, 134.7),
    stick_r: 24.9,
    dpad: (163.8, 141.2),
    dpad_kind: DpadKind::Cross,
    dpad_reach: 20.6,
    face: (297.9, 93.0),
    face_gap: 22.8,
    face_r: 10.7,
    select: (178.8, 94.7, 7.3),
    guide: (211.1, 96.0, 16.3),
    start: (242.5, 94.7, 7.3),
    shoulders: (142.3, 286.3),
    shoulder_w: 55.9,
    shoulder_y: 58.0,
    marks: &[],
};

const XBOX_ONE: Layout = Layout {
    outline: "M 99.0 77.4 C 119.1 56.4 151.3 53.2 171.4 54.8 L 259.9 54.8 C 276.0 50.0 304.1 50.0 321.8 69.3 C 342.7 95.0 349.2 151.4 345.9 191.6 C 342.7 239.8 324.2 268.0 305.7 262.4 C 289.6 257.5 284.8 227.8 271.9 219.7 C 262.3 212.5 247.8 209.3 231.7 209.3 L 187.5 209.3 C 171.4 209.3 160.9 217.3 149.7 228.6 C 136.8 244.7 131.2 268.0 112.7 264.0 C 93.4 260.0 77.3 231.8 74.9 191.6 C 70.8 151.4 78.9 99.1 99.0 77.4Z",
    left_stick: (117.5, 115.2),
    right_stick: (258.3, 165.8),
    stick_r: 24.1,
    dpad: (155.3, 165.8),
    dpad_kind: DpadKind::Cross,
    dpad_reach: 25.7,
    face: (301.0, 112.7),
    face_gap: 24.1,
    face_r: 12.1,
    select: (181.8, 113.5, 5.6),
    guide: (209.2, 73.3, 11.3),
    start: (235.7, 113.5, 5.6),
    shoulders: (140.0, 281.0),
    shoulder_w: 44.0,
    shoulder_y: 55.0,
    marks: &[],
};

const XBOX_SERIES: Layout = Layout {
    outline: "M 111.0 65.7 C 130.2 50.0 156.4 52.6 173.8 57.0 L 259.3 57.0 C 278.5 52.6 309.0 50.0 326.4 67.4 C 348.2 93.6 361.3 154.6 358.7 198.2 C 356.9 250.6 335.1 268.0 317.7 261.0 C 297.6 254.9 291.5 224.4 278.5 207.0 C 269.7 196.5 252.3 191.3 234.9 191.3 L 186.9 191.3 C 169.5 191.3 156.4 198.2 145.9 211.3 C 130.2 228.8 125.9 254.9 104.1 261.0 C 84.9 266.3 65.7 241.8 62.2 198.2 C 58.7 154.6 77.9 93.6 111.0 65.7Z",
    left_stick: (132.8, 106.7),
    right_stick: (247.9, 154.6),
    stick_r: 26.2,
    dpad: (173.8, 159.0),
    dpad_kind: DpadKind::Disc,
    dpad_reach: 26.2,
    face: (288.9, 111.0),
    face_gap: 19.2,
    face_r: 10.5,
    select: (188.6, 111.0, 6.1),
    guide: (210.4, 78.8, 10.5),
    start: (233.1, 111.0, 6.1),
    shoulders: (138.9, 291.5),
    shoulder_w: 52.3,
    shoulder_y: 55.0,
    marks: &[Mark::Rect(200.8, 120.6, 19.2, 10.5, 3.5)],
};

const XBOX_ELITE: Layout = Layout {
    outline: "M 107.3 66.3 C 125.8 50.0 155.5 51.5 168.8 54.4 L 253.4 54.4 C 266.7 51.5 296.4 50.0 312.7 66.3 C 329.8 88.6 334.9 162.7 333.5 214.6 C 333.5 248.0 326.0 266.5 307.5 266.5 C 292.7 265.0 285.3 240.6 272.7 228.0 C 263.8 218.3 251.9 208.7 238.5 207.2 L 185.2 207.2 C 171.8 208.7 159.9 218.3 149.6 230.9 C 137.0 245.8 127.3 265.0 111.0 268.0 C 90.2 268.0 85.1 236.9 85.8 192.4 C 87.3 133.0 92.5 88.6 107.3 66.3Z",
    left_stick: (125.8, 107.1),
    right_stick: (248.2, 151.6),
    stick_r: 23.7,
    dpad: (161.4, 159.0),
    dpad_kind: DpadKind::Disc,
    dpad_reach: 24.5,
    face: (292.7, 108.6),
    face_gap: 23.0,
    face_r: 11.1,
    select: (185.2, 108.6, 5.2),
    guide: (208.9, 72.2, 10.4),
    start: (233.4, 108.6, 5.2),
    shoulders: (137.0, 281.6),
    shoulder_w: 50.0,
    shoulder_y: 53.0,
    // The pair button under View and Menu; the faceplate seam it sits in reads as clutter.
    marks: &[Mark::Rect(196.3, 127.1, 25.2, 7.4, 3.7)],
};

const STEAM_CONTROLLER: Layout = Layout {
    outline: "M 87.3 66.0 C 104.4 50.0 138.7 51.1 161.5 51.1 L 252.8 51.1 C 281.3 51.1 315.6 50.0 332.7 66.0 C 355.5 88.8 364.1 140.2 364.1 197.2 C 364.1 237.2 358.4 260.0 338.4 264.6 C 318.4 268.0 304.2 254.3 292.7 237.2 C 284.2 224.6 275.6 220.1 264.2 220.1 L 155.8 220.1 C 144.4 220.1 135.8 224.6 127.3 237.2 C 115.8 254.3 101.6 268.0 81.6 264.6 C 61.6 260.0 55.9 237.2 55.9 197.2 C 55.9 140.2 64.5 88.8 87.3 66.0Z",
    left_stick: (161.5, 107.1),
    right_stick: (255.7, 107.1),
    stick_r: 19.4,
    dpad: (109.0, 83.1),
    dpad_kind: DpadKind::Cross,
    dpad_reach: 19.4,
    face: (305.3, 87.7),
    face_gap: 17.7,
    face_r: 9.7,
    select: (152.9, 67.1, 8.6),
    guide: (208.3, 86.0, 9.7),
    start: (264.2, 67.1, 8.6),
    shoulders: (125.0, 295.0),
    shoulder_w: 66.0,
    shoulder_y: 53.0,
    marks: &[Mark::Rect(120.4, 137.3, 68.5, 67.3, 9.1), Mark::Rect(231.1, 137.3, 68.5, 67.3, 9.1), Mark::Rect(198.0, 164.1, 20.5, 11.4, 4.6)],
};

const WII_U_PRO: Layout = Layout {
    outline: "M 92.5 64.2 C 113.8 53.5 145.7 61.7 174.0 65.2 C 205.9 68.1 241.4 68.1 269.7 59.6 C 301.6 50.0 330.0 53.5 345.9 80.1 C 360.1 106.7 361.2 154.6 359.1 190.0 C 357.6 232.6 351.3 266.2 330.0 266.2 C 315.8 266.2 306.9 250.3 298.1 234.3 C 287.5 214.8 262.6 198.9 227.2 198.9 L 191.7 198.9 C 156.3 198.9 131.5 214.8 120.9 234.3 C 112.0 250.3 103.1 268.0 88.9 266.2 C 69.5 263.7 60.6 225.5 59.9 190.0 C 58.8 154.6 60.6 108.5 71.2 85.4 C 76.5 73.0 83.6 67.7 92.5 64.2Z",
    left_stick: (108.4, 99.6),
    right_stick: (310.5, 94.3),
    stick_r: 27.6,
    dpad: (149.9, 153.9),
    dpad_kind: DpadKind::Cross,
    dpad_reach: 26.6,
    face: (276.1, 151.0),
    face_gap: 22.0,
    face_r: 11.3,
    select: (184.7, 113.1, 7.8),
    guide: (212.3, 113.1, 8.5),
    start: (240.3, 113.1, 7.8),
    shoulders: (127.9, 283.9),
    shoulder_w: 60.3,
    shoulder_y: 58.0,
    marks: &[Mark::Circle(213.0, 153.9, 5.7)],
};

const GENERIC: Layout = Layout {
    outline: "M 78 72 C 88 55 108 52 136 52 L 284 52 C 312 52 332 55 342 72 C 364 110 366 170 357 214 C 350 249 333 266 312 264 C 293 262 281 246 267 230 C 251 212 235 204 210 204 C 185 204 169 212 153 230 C 139 246 127 262 108 264 C 87 266 70 249 63 214 C 54 170 56 110 78 72 Z",
    left_stick: (118.0, 118.0),
    right_stick: (255.0, 168.0),
    stick_r: 27.0,
    dpad: (165.0, 168.0),
    dpad_kind: DpadKind::Cross,
    dpad_reach: 26.0,
    face: (302.0, 118.0),
    face_gap: 22.0,
    face_r: 11.0,
    select: (182.0, 118.0, 7.0),
    guide: (210.0, 88.0, 13.0),
    start: (238.0, 118.0, 7.0),
    shoulders: (124.0, 296.0),
    shoulder_w: 72.0,
    shoulder_y: 52.0,
    marks: &[],
};
