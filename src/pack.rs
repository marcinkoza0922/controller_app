//! Packs: a game written to a `.padpack` file (TOML, in the config's own shapes), and what
//! importing, updating and exporting one does to the config.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::config::{
    ButtonAction, Config, Game, GyroMode, Indicator, InfoOverlay, ItemKind, Layer, LogOverlay, Macro, Menu, MacroStep, Origin,
    OverlayStyle, PackInfo, PackRef, Profile, Rule, Shared, free_name,
};

/// The pack format this app writes, and the newest it reads. 2 added layers; 3, toggles
/// that start on; 4, the keyboard and numpad styles; 5, the overlay font; 6, grid menus; 7,
/// the Guide shift, screenshot, recording and force-quit actions and layer indicators
/// with generated bindings, extra info overlays and a delay; 8, profiles stating what
/// controller features they need (replacing the pack-wide list); 9, the media controls and
/// in-game menu looks.
pub const FORMAT: u32 = 9;
pub const EXTENSION: &str = "padpack";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pack {
    pub format: u32,
    pub pack: Header,
    #[serde(default)]
    pub rules: Vec<Rule>,
    pub profiles: Vec<Profile>,
    #[serde(default)]
    pub macros: Vec<Macro>,
    #[serde(default)]
    pub menus: Vec<Menu>,
    #[serde(default)]
    pub info_overlays: Vec<InfoOverlay>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub log_overlays: Vec<LogOverlay>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<Layer>,
    /// How the on-screen keyboard and numpad look in this game, if the author set that.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyboard_style: Option<OverlayStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub numpad_style: Option<OverlayStyle>,
    /// The look of the media controls and the in-game menu, if the author set those.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_style: Option<OverlayStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub menu_style: Option<OverlayStyle>,
    /// The font of this game's overlays, menus and keyboards, if the author set one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overlay_font: Option<String>,
}

/// What a pack says about itself.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Header {
    /// Stable across versions of the same pack.
    pub id: String,
    /// Becomes the game's name.
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    /// The controller it was made with (free text).
    #[serde(default)]
    pub made_with: String,
    /// Read from packs made before profiles declared their own needs (format 7 and older),
    /// which `parse` moves onto the profiles. Never written.
    #[serde(default, skip_serializing)]
    pub requires: Vec<Feature>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub based_on: Option<PackRef>,
}

pub use crate::config::Feature;

/// A feature and where it's used, e.g. "profile “Play”".
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FeatureUse {
    pub feature: Feature,
    pub place: String,
    /// Whether the author declared that the profile can't be played without it. Otherwise it
    /// only adds to a scheme that works on a plain pad.
    pub required: bool,
}

impl Pack {
    pub fn info(&self) -> PackInfo {
        let h = &self.pack;
        PackInfo {
            id: h.id.clone(),
            version: h.version.clone(),
            author: h.author.clone(),
            description: h.description.clone(),
            made_with: h.made_with.clone(),
            based_on: h.based_on.clone(),
        }
    }

    /// The game this pack makes, before any clash handling.
    pub fn to_game(&self) -> Game {
        Game {
            name: self.pack.name.clone(),
            pack: self.info(),
            origin: None,
            rules: self.rules.clone(),
            profiles: self.profiles.clone(),
            macros: self.macros.clone(),
            menus: self.menus.clone(),
            info: self.info_overlays.clone(),
            logs: self.log_overlays.clone(),
            layers: self.layers.clone(),
            keyboard_style: self.keyboard_style.clone(),
            numpad_style: self.numpad_style.clone(),
            media_style: self.media_style.clone(),
            menu_style: self.menu_style.clone(),
            overlay_font: self.overlay_font.clone(),
        }
    }

    pub fn to_toml(&self) -> Result<String> {
        Ok(toml::to_string_pretty(self)?)
    }
}

/// Reads a pack, refusing one from a newer app and anything malformed (whole or nothing).
pub fn parse(text: &str) -> Result<Pack> {
    let text = crate::config::migrate_text(text);
    let text = text.as_str();
    let table: toml::Table = toml::from_str(text).context("not a valid pack file")?;
    let format = table.get("format").and_then(toml::Value::as_integer).context("not a pack file (no format number)")?;
    if format > FORMAT as i64 {
        bail!("This pack was made with a newer version of the app. Update to import it.");
    }
    if format < 1 {
        bail!("unknown pack format {format}");
    }
    let mut pack: Pack = toml::from_str(text).context("reading the pack")?;
    // Before format 8 the pack listed what its gyro-aiming profiles needed, as a whole.
    for feature in std::mem::take(&mut pack.pack.requires) {
        for p in pack.profiles.iter_mut().filter(|p| feature == Feature::Gyro && p.gyro.mode != GyroMode::Off) {
            p.requires.push(feature);
        }
    }
    check(&pack)?;
    Ok(pack)
}

/// What a pack must hold to make a usable game.
fn check(pack: &Pack) -> Result<()> {
    if pack.pack.id.trim().is_empty() {
        bail!("the pack has no id");
    }
    if pack.pack.name.trim().is_empty() {
        bail!("the pack has no name");
    }
    if pack.profiles.is_empty() {
        bail!("the pack has no profiles");
    }
    let game = pack.to_game();
    for kind in ItemKind::ALL {
        let names = game.names(kind);
        let unique: BTreeSet<&str> = names.iter().copied().collect();
        if unique.len() != names.len() {
            bail!("the pack has two {}s with the same name", kind.noun());
        }
    }
    let profiles: BTreeSet<&String> = pack.profiles.iter().map(|p| &p.name).collect();
    if profiles.len() != pack.profiles.len() {
        bail!("the pack has two profiles with the same name");
    }
    if let Some(r) = pack.rules.iter().find(|r| !profiles.contains(&r.profile)) {
        bail!("a rule points to missing profile {:?}", r.profile);
    }
    Ok(())
}

/// A new random pack ID (a version 4 UUID).
pub fn new_id() -> String {
    use std::io::Read;
    let mut b = [0u8; 16];
    let read = std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b));
    if read.is_err() {
        // Not cryptographic, only unique enough: the clock and this process.
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        let seed = now.as_nanos() ^ ((std::process::id() as u128) << 64);
        b = seed.to_le_bytes();
    }
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..])
}

/// Every (kind, name) reference in these profiles, menus, macros and layers, nested ones
/// included, and the info overlays layers use as their indicator.
fn references(profiles: &[Profile], menus: &[Menu], macros: &[Macro], layers: &[Layer]) -> BTreeSet<(ItemKind, String)> {
    let mut refs = BTreeSet::new();
    let mut visit = |a: &ButtonAction| {
        a.walk(&mut |a| {
            for kind in ItemKind::ALL {
                if let Some(name) = kind.name_in(a) {
                    refs.insert((kind, name.clone()));
                }
            }
        })
    };
    for p in profiles {
        p.actions().into_iter().for_each(&mut visit);
    }
    for m in menus {
        m.items.iter().for_each(|i| visit(&i.action));
    }
    for m in macros {
        m.steps.iter().filter_map(MacroStep::action).for_each(&mut visit);
    }
    for l in layers {
        l.actions().into_iter().for_each(&mut visit);
    }
    for l in layers {
        if let Indicator::Info(name) = &l.indicator {
            refs.insert((ItemKind::Info, name.clone()));
        }
    }
    refs
}

/// Every (kind, name) a game's own items refer to.
pub(crate) fn game_references(g: &Game) -> BTreeSet<(ItemKind, String)> {
    references(&g.profiles, &g.menus, &g.macros, &g.layers)
}

/// What copying the `kind` item `name` out of `source` has to bring: the item itself (first)
/// and, followed through menu items and macro steps, every item of `source` it refers to
/// that `resolves` can't already find where it's going.
pub fn dependencies(source: &Game, kind: ItemKind, name: &str, resolves: impl Fn(ItemKind, &str) -> bool) -> Vec<(ItemKind, String)> {
    if !source.names(kind).contains(&name) {
        return Vec::new();
    }
    let mut needed = vec![(kind, name.to_string())];
    let mut i = 0;
    while i < needed.len() {
        let (k, n) = needed[i].clone();
        let refs = match k {
            ItemKind::Macro => source.macros.iter().find(|m| m.name == n).map(|m| references(&[], &[], std::slice::from_ref(m), &[])),
            ItemKind::Menu => source.menus.iter().find(|m| m.name == n).map(|m| references(&[], std::slice::from_ref(m), &[], &[])),
            ItemKind::Layer => source.layers.iter().find(|l| l.name == n).map(|l| references(&[], &[], &[], std::slice::from_ref(l))),
            ItemKind::Info | ItemKind::Log => None,
        };
        for (rk, rn) in refs.unwrap_or_default() {
            let item = (rk, rn);
            if !needed.contains(&item) && !resolves(rk, &item.1) && source.names(rk).contains(&item.1.as_str()) {
                needed.push(item);
            }
        }
        i += 1;
    }
    needed
}

/// Inputs beyond the XInput baseline that a game's profiles and layers use or demand, and
/// where. What a profile demands is only what its author declared.
pub fn features(game: &Game) -> Vec<FeatureUse> {
    let profiles = game.profiles.iter().flat_map(|p| {
        let uses_gyro = p.gyro.mode != GyroMode::Off;
        Feature::ALL.into_iter().filter_map(move |feature| {
            let required = p.requires.contains(&feature);
            (required || (feature == Feature::Gyro && uses_gyro))
                .then(|| FeatureUse { feature, place: format!("profile “{}”", p.name), required })
        })
    });
    let layers = game
        .layers
        .iter()
        .filter(|l| l.gyro.as_ref().is_some_and(|g| g.mode != GyroMode::Off))
        .map(|l| FeatureUse { feature: Feature::Gyro, place: format!("layer “{}”", l.name), required: false });
    profiles.chain(layers).collect()
}

impl Pack {
    /// The features every profile of the pack needs, so a player without them gets nothing.
    pub fn required_by_all(&self) -> Vec<Feature> {
        Feature::ALL.into_iter().filter(|f| !self.profiles.is_empty() && self.profiles.iter().all(|p| p.requires.contains(f))).collect()
    }

    /// The names of the profiles that need `feature`.
    pub fn needing(&self, feature: Feature) -> Vec<&str> {
        self.profiles.iter().filter(|p| p.requires.contains(&feature)).map(|p| p.name.as_str()).collect()
    }
}

/// A pack ready to save, and what the export dialog should point out.
#[derive(Debug, Clone, PartialEq)]
pub struct Export {
    pub pack: Pack,
    /// Shared items written into the pack because the game uses them.
    pub pulled_in: Vec<(ItemKind, String)>,
    /// References to items that exist nowhere, e.g. "macro \"Dodge\"".
    pub dangling: Vec<String>,
    pub features: Vec<FeatureUse>,
}

/// The pack metadata the export dialog starts from. Exporting a game that came from someone
/// else's pack (or the library) makes a fork: a new ID, crediting the original. `library`
/// keeps a library pack's ID instead, for the maintainer publishing a new version.
pub fn draft(game: &Game, library: bool) -> PackInfo {
    let mut info = game.pack.clone();
    if let Some(origin) = &game.origin
        && !library
        && (info.id.is_empty() || info.id == origin.id)
    {
        let name = if origin.name.is_empty() { &game.name } else { &origin.name };
        info.based_on = Some(PackRef {
            id: origin.id.clone(),
            name: name.clone(),
            author: info.author.clone(),
            version: origin.version.clone(),
        });
        info.id = new_id();
        info.author = String::new();
        info.version = String::new();
    }
    if info.id.is_empty() {
        info.id = new_id();
    }
    if info.version.is_empty() {
        info.version = "1.0".into();
    }
    info
}

/// Writes `game` as a pack with this metadata, copying in the shared items it uses (followed
/// through menus and macros) so the pack works on its own.
pub fn export(game: &Game, shared: &Shared, info: &PackInfo) -> Export {
    let mut pack_game = game.clone();
    let mut pulled_in = Vec::new();
    let mut dangling = BTreeSet::new();
    // Each pass adds the shared items the previous ones referred to.
    loop {
        let refs = game_references(&pack_game);
        let mut added = false;
        for (kind, name) in refs {
            if pack_game.names(kind).contains(&name.as_str()) {
                continue;
            }
            if !shared.names(kind).contains(&name.as_str()) {
                dangling.insert(format!("{} {name:?}", kind.noun()));
                continue;
            }
            match kind {
                ItemKind::Macro => pack_game.macros.extend(shared.macros.iter().find(|m| m.name == name).cloned()),
                ItemKind::Menu => pack_game.menus.extend(shared.menus.iter().find(|m| m.name == name).cloned()),
                ItemKind::Info => pack_game.info.extend(shared.info.iter().find(|o| o.name == name).cloned()),
                ItemKind::Log => pack_game.logs.extend(shared.logs.iter().find(|o| o.name == name).cloned()),
                // Never shared, so it was reported as dangling above.
                ItemKind::Layer => {}
            }
            pulled_in.push((kind, name));
            added = true;
        }
        if !added {
            break;
        }
    }
    let features = features(&pack_game);
    let pack = Pack {
        format: FORMAT,
        pack: Header {
            id: info.id.clone(),
            name: game.name.clone(),
            version: info.version.clone(),
            author: info.author.clone(),
            description: info.description.clone(),
            made_with: info.made_with.clone(),
            requires: Vec::new(),
            based_on: info.based_on.clone(),
        },
        rules: pack_game.rules.iter().map(|r| Rule { enabled: true, ..r.clone() }).collect(),
        profiles: pack_game.profiles,
        macros: pack_game.macros,
        menus: pack_game.menus,
        info_overlays: pack_game.info,
        log_overlays: pack_game.logs,
        layers: pack_game.layers,
        keyboard_style: pack_game.keyboard_style,
        numpad_style: pack_game.numpad_style,
        media_style: pack_game.media_style,
        menu_style: pack_game.menu_style,
        overlay_font: pack_game.overlay_font,
    };
    Export { pack, pulled_in, dangling: dangling.into_iter().collect(), features }
}

/// A stable 64-bit FNV-1a hash, as hex.
fn fnv(text: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

fn hash<T: Serialize>(item: &T) -> String {
    fnv(&toml::to_string(item).unwrap_or_default())
}

/// A content hash per profile, macro, menu and info overlay, keyed "kind:name".
pub fn item_hashes(game: &Game) -> BTreeMap<String, String> {
    let mut hashes = BTreeMap::new();
    for p in &game.profiles {
        hashes.insert(format!("profile:{}", p.name), hash(p));
    }
    for m in &game.macros {
        hashes.insert(format!("macro:{}", m.name), hash(m));
    }
    for m in &game.menus {
        hashes.insert(format!("menu:{}", m.name), hash(m));
    }
    for o in &game.info {
        hashes.insert(format!("info overlay:{}", o.name), hash(o));
    }
    for o in &game.logs {
        hashes.insert(format!("log overlay:{}", o.name), hash(o));
    }
    for l in &game.layers {
        hashes.insert(format!("layer:{}", l.name), hash(l));
    }
    hashes
}

/// Items edited, added or removed since the game was imported, as "macro “Dodge”".
pub fn edited_items(game: &Game) -> Vec<String> {
    let Some(origin) = &game.origin else { return Vec::new() };
    let now = item_hashes(game);
    let keys: BTreeSet<&String> = now.keys().chain(origin.hashes.keys()).collect();
    keys.into_iter()
        .filter(|k| now.get(*k) != origin.hashes.get(*k))
        .map(|k| {
            let (kind, name) = k.split_once(':').unwrap_or(("item", k));
            format!("{kind} “{name}”")
        })
        .collect()
}

/// How an incoming pack's version relates to the installed one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateKind {
    Update,
    Reinstall,
    Downgrade,
}

impl UpdateKind {
    pub fn verb(self) -> &'static str {
        match self {
            UpdateKind::Update => "Update",
            UpdateKind::Reinstall => "Reinstall",
            UpdateKind::Downgrade => "Downgrade",
        }
    }
}

/// Compares dotted versions number by number ("1.10" is newer than "1.9"); anything that
/// isn't a number compares as text. A pre-release ("1.0-beta") comes before its release.
pub fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let split = |v: &str| {
        let v = v.trim().trim_start_matches(['v', 'V']);
        // Build metadata ("+linux") doesn't order versions.
        let v = v.split('+').next().unwrap_or(v);
        match v.split_once('-') {
            Some((core, pre)) => (core.to_string(), Some(pre.to_string())),
            None => (v.to_string(), None),
        }
    };
    let ((a, a_pre), (b, b_pre)) = (split(a), split(b));
    let (a, b): (Vec<&str>, Vec<&str>) = (a.split('.').collect(), b.split('.').collect());
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or("0"), b.get(i).copied().unwrap_or("0"));
        let order = match (x.parse::<u64>(), y.parse::<u64>()) {
            (Ok(x), Ok(y)) => x.cmp(&y),
            _ => x.cmp(y),
        };
        if order.is_ne() {
            return order;
        }
    }
    match (a_pre, b_pre) {
        (None, None) => Ordering::Equal,
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (Some(x), Some(y)) => compare_versions(&x, &y),
    }
}

/// An imported rule matching the same window as another game's (switched-on) rule.
#[derive(Debug, Clone, PartialEq)]
pub struct RuleClash {
    /// Index in the pack's rules.
    pub rule: usize,
    pub other_game: String,
}

/// What importing a pack would do, for the preview.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub pack: Pack,
    pub library: bool,
    /// The installed game with this pack's ID, which an import updates.
    pub update_of: Option<String>,
    pub update_kind: Option<UpdateKind>,
    /// For an update: the installed game's items edited since its import.
    pub edited: Vec<String>,
    /// Another game with the pack's name (not an update).
    pub name_clash: bool,
    /// The name the game gets if renamed.
    pub renamed: String,
    /// Pack items named like shared ones, and the names they get instead.
    pub shared_clashes: Vec<(ItemKind, String, String)>,
    pub rule_clashes: Vec<RuleClash>,
}

/// What the user chose in the preview.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Choices {
    /// Replace the game that has the pack's name instead of renaming the import.
    pub replace: bool,
    /// Per rule clash: keep the other game's rule (the imported one comes in switched off).
    pub keep_mine: Vec<bool>,
}

/// Works out what importing `pack` would do to `config`.
pub fn plan(config: &Config, pack: Pack, library: bool) -> Plan {
    let installed = config.games.iter().find(|g| g.origin.as_ref().is_some_and(|o| o.id == pack.pack.id));
    let update_kind = installed.and_then(|g| g.origin.as_ref()).map(|o| match compare_versions(&pack.pack.version, &o.version) {
        std::cmp::Ordering::Greater => UpdateKind::Update,
        std::cmp::Ordering::Equal => UpdateKind::Reinstall,
        std::cmp::Ordering::Less => UpdateKind::Downgrade,
    });
    let name = &pack.pack.name;
    let name_clash = installed.is_none() && config.games.iter().any(|g| &g.name == name);
    let renamed = free_name(name, |n| config.games.iter().any(|g| g.name == n));

    let game = pack.to_game();
    let mut shared_clashes = Vec::new();
    for kind in ItemKind::ALL {
        let shared = config.shared.names(kind);
        for item in game.names(kind) {
            if shared.contains(&item) {
                let taken = |n: &str| shared.contains(&n) || game.names(kind).contains(&n);
                shared_clashes.push((kind, item.to_string(), free_name(item, taken)));
            }
        }
    }

    let target = installed.map(|g| g.name.as_str());
    let mut rule_clashes = Vec::new();
    for (i, rule) in pack.rules.iter().enumerate() {
        // An update keeps what was decided for rules the game already had.
        if installed.is_some_and(|g| g.rules.iter().any(|r| r.same_match(rule))) {
            continue;
        }
        let other = config
            .games
            .iter()
            .filter(|g| Some(g.name.as_str()) != target)
            .find(|g| g.rules.iter().any(|r| r.enabled && r.same_match(rule)));
        if let Some(other) = other {
            rule_clashes.push(RuleClash { rule: i, other_game: other.name.clone() });
        }
    }

    Plan {
        update_of: installed.map(|g| g.name.clone()),
        update_kind,
        edited: installed.map(edited_items).unwrap_or_default(),
        name_clash,
        renamed,
        shared_clashes,
        rule_clashes,
        pack,
        library,
    }
}

/// Rule clashes that still matter: not with a game the import replaces.
pub fn live_clashes<'a>(plan: &'a Plan, choices: &Choices) -> impl Iterator<Item = (usize, &'a RuleClash)> {
    let replaced = (plan.name_clash && choices.replace).then_some(plan.pack.pack.name.as_str());
    plan.rule_clashes.iter().enumerate().filter(move |(_, c)| Some(c.other_game.as_str()) != replaced)
}

/// Imports the planned pack into `config` and returns the new game's name.
pub fn apply(config: &mut Config, plan: &Plan, choices: &Choices) -> String {
    let mut game = plan.pack.to_game();
    for (kind, old, new) in &plan.shared_clashes {
        game.rename_item(*kind, old, new);
        game.rename_refs(*kind, old, new);
    }

    // Rules: an update keeps each existing rule's on/off; clashes go as chosen.
    let previous = plan.update_of.as_deref().and_then(|n| config.games.iter().find(|g| g.name == n));
    for (i, rule) in game.rules.iter_mut().enumerate() {
        if let Some(old) = previous.and_then(|g| g.rules.iter().find(|r| r.same_match(rule))) {
            rule.enabled = old.enabled;
        }
        if let Some((c, _)) = live_clashes(plan, choices).find(|(_, c)| c.rule == i)
            && choices.keep_mine.get(c).copied().unwrap_or(false)
        {
            rule.enabled = false;
        }
    }
    for (c, clash) in live_clashes(plan, choices) {
        if choices.keep_mine.get(c).copied().unwrap_or(false) {
            continue;
        }
        let rule = &plan.pack.rules[clash.rule];
        if let Some(other) = config.games.iter_mut().find(|g| g.name == clash.other_game) {
            other.rules.iter_mut().filter(|r| r.same_match(rule)).for_each(|r| r.enabled = false);
        }
    }

    game.origin = Some(Origin {
        id: plan.pack.pack.id.clone(),
        name: plan.pack.pack.name.clone(),
        version: plan.pack.pack.version.clone(),
        library: plan.library,
        hashes: item_hashes(&game),
    });

    let replace = match &plan.update_of {
        Some(name) => {
            game.name = name.clone();
            Some(name.clone())
        }
        None if plan.name_clash && choices.replace => Some(game.name.clone()),
        None if plan.name_clash => {
            game.name = plan.renamed.clone();
            None
        }
        None => None,
    };
    let name = game.name.clone();
    match replace.and_then(|n| config.games.iter().position(|g| g.name == n)) {
        Some(i) => {
            // A pack that sets no look leaves the player's own in place.
            let old = &config.games[i];
            game.keyboard_style = game.keyboard_style.or_else(|| old.keyboard_style.clone());
            game.numpad_style = game.numpad_style.or_else(|| old.numpad_style.clone());
            game.media_style = game.media_style.or_else(|| old.media_style.clone());
            game.menu_style = game.menu_style.or_else(|| old.menu_style.clone());
            game.overlay_font = game.overlay_font.or_else(|| old.overlay_font.clone());
            config.games[i] = game;
        }
        None => config.games.push(game),
    }
    name
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Button, MenuItem, MenuKind, OverlayStyle, ProfileRef, RuleKind};

    fn mac(name: &str) -> Macro {
        Macro { name: name.into(), steps: vec![MacroStep::Wait(10)] }
    }

    fn menu(name: &str, items: Vec<ButtonAction>) -> Menu {
        Menu {
            name: name.into(),
            kind: MenuKind::List,
            items: items.into_iter().map(|action| MenuItem { label: "x".into(), action, button: None }).collect(),
            cancel: None,
            style: OverlayStyle::default(),
        }
    }

    fn macro_ref(name: &str) -> ButtonAction {
        ButtonAction::Macro { name: name.into(), repeat: false }
    }

    /// A game whose profile opens shared menu "Wheel", which plays shared macro "Heal".
    fn setup() -> Config {
        let mut config = Config::default();
        config.shared.macros = vec![mac("Heal"), mac("Unused")];
        config.shared.menus = vec![menu("Wheel", vec![macro_ref("Heal")])];
        let mut game = Game::new("Doom", vec![Profile::pc_action("Play")]);
        game.profiles[0].set_button(Button::Select, ButtonAction::OpenMenu("Wheel".into()));
        game.profiles[0].set_button(Button::North, macro_ref("Gone"));
        game.macros = vec![mac("Dodge")];
        game.rules = vec![Rule::new(RuleKind::Executable, "doom.exe", "Play")];
        config.games.push(game);
        config
    }

    #[test]
    fn profiles_state_their_own_needs_and_old_packs_are_upgraded() {
        let mut config = setup();
        let game = &mut config.games[0];
        game.profiles[0].requires = vec![Feature::Gyro];
        game.profiles.push(Profile::passthrough("Plain"));
        let out = export(game, &config.shared, &draft(game, false));
        assert_eq!(out.features, [FeatureUse { feature: Feature::Gyro, place: "profile “Play”".into(), required: true }]);
        assert_eq!(out.pack.required_by_all(), []);
        assert_eq!(out.pack.needing(Feature::Gyro), ["Play"]);
        let back = parse(&out.pack.to_toml().unwrap()).unwrap();
        assert_eq!(back.profiles[0].requires, [Feature::Gyro]);
        assert!(back.profiles[1].requires.is_empty());

        // Format 7 listed the need for the whole pack: it lands on the gyro profiles.
        let mut old = out.pack.clone();
        old.format = 7;
        old.profiles.iter_mut().for_each(|p| p.requires.clear());
        let text = old.to_toml().unwrap().replacen("[pack]\n", "[pack]\nrequires = [\"gyro\"]\n", 1);
        let upgraded = parse(&text).unwrap();
        assert_eq!(upgraded.profiles[0].requires, [Feature::Gyro]);
        assert!(upgraded.profiles[1].requires.is_empty());
        assert!(upgraded.pack.requires.is_empty());
    }

    #[test]
    fn export_copies_in_used_shared_items_and_reports_problems() {
        let config = setup();
        let game = &config.games[0];
        let info = draft(game, false);
        let out = export(game, &config.shared, &info);
        assert_eq!(out.pulled_in, [(ItemKind::Menu, "Wheel".to_string()), (ItemKind::Macro, "Heal".to_string())]);
        assert_eq!(out.dangling, ["macro \"Gone\""]);
        assert!(out.pack.pack.requires.is_empty(), "a pack doesn't state needs; its profiles do");
        let uses = FeatureUse { feature: Feature::Gyro, place: "profile “Play”".into(), required: false };
        assert_eq!(out.features, [uses], "the action template aims with gyro, which it can do without");
        assert!(!out.pack.macros.iter().any(|m| m.name == "Unused"));
        assert_eq!(out.pack.pack.name, "Doom");
    }

    #[test]
    fn packs_roundtrip_and_newer_formats_are_refused() {
        let config = setup();
        let out = export(&config.games[0], &config.shared, &draft(&config.games[0], false));
        let text = out.pack.to_toml().unwrap();
        assert_eq!(parse(&text).unwrap(), out.pack);

        let newer = text.replacen(&format!("format = {FORMAT}"), &format!("format = {}", FORMAT + 1), 1);
        assert!(parse(&newer).unwrap_err().to_string().contains("newer version of the app"));
        let extra = format!("surprise = 1\n{text}");
        assert!(parse(&extra).is_err(), "strict within a format");
        assert!(parse("[pack]\nid = 'x'").unwrap_err().to_string().contains("no format number"));
    }

    #[test]
    #[expect(clippy::cognitive_complexity, reason = "predates the size lints")]
    fn importing_twice_renames_then_updates_by_id() {
        let mut config = setup();
        let mut pack = export(&config.games[0], &config.shared, &draft(&config.games[0], false)).pack;
        pack.pack.version = "1.0".into();

        // "Doom" exists (made by the user, no origin): a name clash, renamed by default.
        let p = plan(&config, pack.clone(), false);
        assert!(p.name_clash && p.update_of.is_none());
        assert_eq!(p.renamed, "Doom (2)");
        // Its rule matches the user's own Doom rule.
        assert_eq!(p.rule_clashes, [RuleClash { rule: 0, other_game: "Doom".into() }]);
        let name = apply(&mut config, &p, &Choices::default());
        assert_eq!(name, "Doom (2)");
        assert!(!config.games[0].rules[0].enabled, "the import's rule took over");
        assert!(config.games[1].rules[0].enabled);
        // Shared items it carries clash with the user's shared ones and get renamed.
        let imported = &config.games[1];
        assert!(imported.menus.iter().any(|m| m.name == "Wheel (2)"));
        assert_eq!(imported.menus[0].items[0].action, macro_ref("Heal (2)"));
        assert_eq!(imported.profiles[0].button(Button::Select), &ButtonAction::OpenMenu("Wheel (2)".into()));

        // The same ID again is an update of "Doom (2)", which the user has since edited.
        config.games[1].macros[0].steps.push(MacroStep::Wait(5));
        config.games[1].rules[0].enabled = false;
        pack.pack.version = "1.1".into();
        let p = plan(&config, pack.clone(), false);
        assert_eq!(p.update_of.as_deref(), Some("Doom (2)"));
        assert_eq!(p.update_kind, Some(UpdateKind::Update));
        assert_eq!(p.edited, ["macro “Dodge”"]);
        assert!(p.rule_clashes.is_empty(), "rules the setup had keep their decision");
        apply(&mut config, &p, &Choices::default());
        assert_eq!(config.games.len(), 2);
        assert_eq!(config.games[1].origin.as_ref().unwrap().version, "1.1");
        assert!(!config.games[1].rules[0].enabled, "switched off before, stays off");
        assert!(edited_items(&config.games[1]).is_empty());

        pack.pack.version = "1.0.9".into();
        assert_eq!(plan(&config, pack, false).update_kind, Some(UpdateKind::Downgrade));
    }

    #[test]
    fn replacing_and_keeping_my_rule() {
        let mut config = setup();
        let mut pack = export(&config.games[0], &config.shared, &draft(&config.games[0], false)).pack;
        pack.pack.id = new_id();
        let p = plan(&config, pack, false);
        assert_eq!(p.rule_clashes.len(), 1, "with the same-named setup, until it's replaced");
        let choices = Choices { replace: true, keep_mine: vec![true] };
        assert_eq!(live_clashes(&p, &choices).count(), 0);
        assert_eq!(apply(&mut config, &p, &choices), "Doom");
        assert_eq!(config.games.len(), 1, "replaced in place");
        assert!(config.games[0].rules[0].enabled, "no clash left with the setup it replaced");
        assert!(config.games[0].origin.is_some());
    }

    #[test]
    fn forks_get_a_new_id_and_credit_the_original() {
        let mut game = Game::new("Doom", vec![Profile::passthrough("P")]);
        let own = draft(&game, false);
        assert!(!own.id.is_empty() && own.based_on.is_none());
        game.pack = own.clone();
        assert_eq!(draft(&game, false).id, own.id, "my own pack keeps its ID");

        game.pack = PackInfo { id: "abc".into(), author: "them".into(), version: "2.0".into(), ..PackInfo::default() };
        game.origin = Some(Origin { id: "abc".into(), version: "2.0".into(), ..Origin::default() });
        let fork = draft(&game, false);
        assert_ne!(fork.id, "abc");
        assert_eq!(fork.based_on, Some(PackRef { id: "abc".into(), name: "Doom".into(), author: "them".into(), version: "2.0".into() }));
        assert_eq!(fork.author, "");
        assert_eq!(draft(&game, true).id, "abc", "the library keeps its pack IDs");

        // Imported next to another Doom, it was renamed; the credit uses the pack's own name.
        game.name = "Doom (2)".into();
        game.origin.as_mut().unwrap().name = "Doom".into();
        assert_eq!(draft(&game, false).based_on.unwrap().name, "Doom");
    }

    #[test]
    fn copies_bring_what_they_refer_to_unless_already_there() {
        let mut source = Game::new("Source", Vec::new());
        source.macros = vec![mac("Heal"), mac("Dodge")];
        source.menus = vec![menu("Wheel", vec![macro_ref("Heal"), ButtonAction::OpenMenu("More".into())]), menu("More", vec![macro_ref("Dodge")])];
        let none = |_: ItemKind, _: &str| false;
        let all = dependencies(&source, ItemKind::Menu, "Wheel", none);
        let want = |list: &[(ItemKind, &str)]| list.iter().map(|(k, n)| (*k, n.to_string())).collect::<Vec<_>>();
        assert_eq!(all, want(&[(ItemKind::Menu, "Wheel"), (ItemKind::Macro, "Heal"), (ItemKind::Menu, "More"), (ItemKind::Macro, "Dodge")]));
        let has_heal = |k: ItemKind, n: &str| k == ItemKind::Macro && n == "Heal";
        assert_eq!(
            dependencies(&source, ItemKind::Menu, "Wheel", has_heal),
            want(&[(ItemKind::Menu, "Wheel"), (ItemKind::Menu, "More"), (ItemKind::Macro, "Dodge")])
        );
        assert!(dependencies(&source, ItemKind::Macro, "Nope", none).is_empty());
    }

    #[test]
    fn layers_travel_in_packs_with_what_they_use() {
        let mut config = setup();
        config.games[0].profiles[0].gyro = crate::config::GyroConfig::default();
        let mut layer = Layer::new("Hotkeys");
        layer.buttons.insert(Button::South, macro_ref("Heal"));
        layer.buttons.insert(Button::East, ButtonAction::Layer("Deeper".into()));
        layer.indicator = Indicator::Info("Cheat sheet".into());
        let mut deeper = Layer::new("Deeper");
        deeper.gyro = Some(crate::config::GyroConfig { mode: crate::config::GyroMode::Mouse { sensitivity: 10.0 }, ..Default::default() });
        config.games[0].layers = vec![layer.clone(), deeper];
        config.shared.info.push(InfoOverlay { name: "Cheat sheet".into(), always: false, on_start: None, linger: None, title: None, current_input: Default::default(), style: Default::default(), rows: vec![] });

        let out = export(&config.games[0], &config.shared, &draft(&config.games[0], false));
        assert!(out.pulled_in.contains(&(ItemKind::Info, "Cheat sheet".into())), "an indicator's info overlay comes along");
        assert_eq!(out.features, [FeatureUse { feature: Feature::Gyro, place: "layer “Deeper”".into(), required: false }]);
        let text = out.pack.to_toml().unwrap();
        assert!(text.contains(&format!("format = {FORMAT}")));
        let back = parse(&text).unwrap();
        assert_eq!(back.layers.len(), 2);
        assert_eq!(back.to_game().layers[0], layer);

        // Copying the layer brings the layer it holds and the macro it plays.
        // ("Heal" is shared rather than the game's own, so it isn't the source's to copy.)
        let needs = dependencies(&config.games[0], ItemKind::Layer, "Hotkeys", |_, _| false);
        assert_eq!(needs, [(ItemKind::Layer, "Hotkeys".to_string()), (ItemKind::Layer, "Deeper".to_string())]);
    }

    #[test]
    fn the_overlay_font_travels_in_packs() {
        let mut config = setup();
        config.games[0].overlay_font = Some("Comfortaa".into());
        let out = export(&config.games[0], &config.shared, &draft(&config.games[0], false));
        let pack = parse(&out.pack.to_toml().unwrap()).unwrap();
        assert_eq!(pack.to_game().overlay_font.as_deref(), Some("Comfortaa"));
    }

    #[test]
    fn keyboard_and_numpad_styles_travel_in_packs() {
        let mut config = setup();
        let style = OverlayStyle { scale: 1.5, ..OverlayStyle::keyboard() };
        config.games[0].keyboard_style = Some(style.clone());
        let out = export(&config.games[0], &config.shared, &draft(&config.games[0], false));
        let pack = parse(&out.pack.to_toml().unwrap()).unwrap();
        assert_eq!(pack.to_game().keyboard_style, Some(style));
        assert_eq!(pack.to_game().numpad_style, None);

        // Importing over a game that has its own numpad look keeps it when the pack sets none.
        let mut config = setup();
        let mut mine = pack.to_game();
        mine.name = "Mine".into();
        mine.numpad_style = Some(OverlayStyle { scale: 0.5, ..OverlayStyle::numpad() });
        config.games.push(mine.clone());
        let mut plan = plan(&config, pack, false);
        plan.update_of = Some("Mine".into());
        apply(&mut config, &plan, &Choices::default());
        let updated = config.games.iter().find(|g| g.name == "Mine").unwrap();
        assert_eq!(updated.numpad_style, mine.numpad_style);
        assert_eq!(updated.keyboard_style.as_ref().unwrap().scale, 1.5);
    }

    #[test]
    fn media_and_menu_styles_travel_in_packs() {
        let mut config = setup();
        let style = OverlayStyle { scale: 1.2, ..OverlayStyle::media() };
        config.games[0].media_style = Some(style.clone());
        config.games[0].menu_style = Some(OverlayStyle::default());
        let out = export(&config.games[0], &config.shared, &draft(&config.games[0], false));
        let pack = parse(&out.pack.to_toml().unwrap()).unwrap();
        assert_eq!(pack.to_game().media_style, Some(style));
        assert_eq!(pack.to_game().menu_style, Some(OverlayStyle::default()));
        assert_eq!(pack.to_game().keyboard_style, None);
    }

    #[test]
    fn version_order_and_ids() {
        use std::cmp::Ordering::*;
        assert_eq!(compare_versions("1.10", "1.9"), Greater);
        assert_eq!(compare_versions("1.0", "1"), Equal);
        assert_eq!(compare_versions("v2", "1.9.9"), Greater);
        assert_eq!(compare_versions("1.0-beta", "1.0"), Less, "pre-releases come before the release");
        assert_eq!(compare_versions("1.0-beta.2", "1.0-beta.10"), Less);
        assert_eq!(compare_versions("1.1-rc1", "1.0"), Greater);
        assert_eq!(compare_versions("1.0+linux", "1.0"), Equal);
        let id = new_id();
        assert_eq!(id.len(), 36);
        assert_eq!(&id[14..15], "4");
        assert_ne!(id, new_id());
    }

    #[test]
    fn imported_games_can_become_active() {
        let mut config = setup();
        let mut pack = export(&config.games[0], &config.shared, &draft(&config.games[0], false)).pack;
        pack.pack.name = "Quake".into();
        pack.rules.clear();
        let plan = plan(&config, pack, false);
        let name = apply(&mut config, &plan, &Choices::default());
        config.active = ProfileRef::new(Some(&name), "Play");
        assert_eq!(config.active_ref(), ProfileRef::new(Some("Quake"), "Play"));
        assert!(config.scope().menus.iter().any(|m| m.name == "Wheel (2)"));
    }
}
