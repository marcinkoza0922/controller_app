//! Each controller model's outline, and the marks that tell the models apart. Drawn over the
//! bumpers and triggers and under the sticks and buttons, which sit at the same places on every model.

use super::{ACTIVE, BODY, BODY_EDGE, IDLE, IDLE_EDGE};
use crate::info::PadModel;

/// The body's outline. A pad of no known model gets the generic one.
pub fn outline(model: Option<PadModel>) -> String {
    let d = match model {
        Some(PadModel::DualShock4) => {
            "M104 52 C150 44 270 44 316 52 C352 60 378 84 388 128 C400 186 404 236 378 258 C352 270 326 246 306 214 \
             C290 206 130 206 114 214 C94 246 68 270 42 258 C16 236 20 186 32 128 C42 84 68 60 104 52 Z"
        }
        Some(PadModel::DualSense) => {
            "M110 58 C150 49 270 49 310 58 C356 68 382 92 392 140 C402 196 406 240 380 260 C354 272 328 246 308 216 \
             C292 207 128 207 112 216 C92 246 66 272 40 260 C14 240 18 196 28 140 C38 92 64 68 110 58 Z"
        }
        Some(PadModel::ProController) => {
            "M96 52 H324 C368 52 396 74 402 116 C408 158 404 206 390 236 C382 254 362 262 342 250 C328 242 318 224 302 212 \
             H128 C112 224 102 242 88 250 C68 262 48 254 40 236 C26 206 22 158 28 116 C34 74 52 52 96 52 Z"
        }
        None => {
            "M110 58 C150 49 270 49 310 58 C350 66 372 90 385 140 C400 200 405 245 375 258 C350 268 325 245 305 215 \
             C290 207 130 207 115 215 C95 245 70 268 45 258 C15 245 20 200 35 140 C48 90 70 66 110 58 Z"
        }
    };
    format!(r#"<path d="{d}" fill="{BODY}" stroke="{BODY_EDGE}" stroke-width="3"/>"#)
}

/// The marks on the body that belong to the model: a DualShock 4 and DualSense have a touchpad
/// (the DualSense's wider and squarer), and the Pro Controller has a capture button and no touchpad.
pub fn marks(model: Option<PadModel>) -> String {
    match model {
        Some(PadModel::DualShock4) => format!(
            r#"<rect x="160" y="52" width="100" height="22" rx="6" fill="{IDLE}" stroke="{IDLE_EDGE}" stroke-width="2"/>"#
        ),
        Some(PadModel::DualSense) => format!(
            r#"<rect x="150" y="57" width="120" height="18" rx="3" fill="{IDLE}" stroke="{IDLE_EDGE}" stroke-width="2"/>"#
        ),
        Some(PadModel::ProController) => format!(
            r#"<rect x="145" y="91" width="14" height="10" rx="3" fill="{IDLE}" stroke="{IDLE_EDGE}" stroke-width="1.5"/>
            <rect x="150" y="93" width="4" height="6" rx="1" fill="{ACTIVE}" fill-opacity="0.35"/>"#
        ),
        None => String::new(),
    }
}
