//! The virtual pads the daemon gives each physical controller, and the force-feedback support they copy.

use evdev::{AttributeSet, Device, FFEffectCode};

use crate::{
    monitor::log,
    output::{FfCaps, VirtualPad},
    pad_identity::PadIdentity,
};

/// The virtual pad a controller feeds, presenting itself as `identity`. It advertises the
/// controller's force-feedback support (`ff`), so games see rumble exactly when the controller has it.
pub fn virtual_pad_for(ff: Option<&FfSpec>, identity: PadIdentity) -> Option<VirtualPad> {
    let effects = ff.map(FfSpec::effect_set);
    let caps = ff.zip(effects.as_ref()).map(|(spec, effects)| FfCaps { effects, max_effects: spec.max_effects });
    match VirtualPad::with_identity(caps, identity) {
        Ok(pad) => Some(pad),
        Err(e) => {
            log!("cannot create virtual pad: {e:#}");
            None
        }
    }
}

/// A controller's force-feedback support, kept so its virtual pad can be rebuilt with the same.
pub struct FfSpec {
    effects: Vec<FFEffectCode>,
    max_effects: u32,
}

impl FfSpec {
    /// The support of `dev`, if it rumbles.
    pub fn of(dev: &Device) -> Option<Self> {
        let effects = dev.supported_ff().filter(|ff| ff.iter().next().is_some() && dev.max_ff_effects() > 0)?;
        Some(FfSpec { effects: effects.iter().collect(), max_effects: dev.max_ff_effects() as u32 })
    }

    pub fn effect_set(&self) -> AttributeSet<FFEffectCode> {
        let mut set = AttributeSet::new();
        for effect in &self.effects {
            set.insert(*effect);
        }
        set
    }
}
