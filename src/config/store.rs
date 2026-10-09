//! The config folder on disk. `config.toml` holds the app settings. Every setup is a folder of its
//! own: `setup.toml` has its name, rules and looks, and each profile, layer, macro, menu, info
//! overlay and log overlay is a file in a subfolder for its kind. General sits beside the setups in
//! the same shape, and so do the shared items, so a folder (or one subfolder of it) can be copied to
//! another machine as it is.
//!
//! Files are numbered so they load in the order they were saved in. Each save rewrites the whole
//! tree, so renamed or removed items leave nothing behind.

use std::{
    fs,
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Serialize, de::DeserializeOwned};

use super::{Config, Game, InfoOverlay, Layer, LogOverlay, Macro, Menu, Profile, Shared};

/// The file in a setup's folder that holds its name, rules and looks.
const SETUP_FILE: &str = "setup.toml";
const SETUPS: &str = "setups";
const GENERAL: &str = "general";
const SHARED: &str = "shared";
/// The keys of `config.toml` that live in the folders instead.
const APP_KEYS: [&str; 3] = ["general", "setups", "shared"];
/// The keys of a setup that live in its subfolders instead.
const SETUP_KEYS: [&str; 6] = ["profiles", "macros", "menus", "info_overlays", "log_overlays", "layers"];

/// An item that is saved as a file named after it.
trait Named {
    fn name(&self) -> &str;
}

macro_rules! named {
    ($($item:ty),*) => {
        $(impl Named for $item {
            fn name(&self) -> &str {
                &self.name
            }
        })*
    };
}

named!(Profile, Layer, Macro, Menu, InfoOverlay, LogOverlay);

/// The folder that holds `config.toml`, and with it the setups.
pub fn root_of(path: &Path) -> &Path {
    path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."))
}

/// Fills in the setups and shared items of `config` from the folder at `root`. A missing General
/// folder gives the default General.
pub fn load(root: &Path, config: &mut Config) -> Result<()> {
    let general = root.join(GENERAL);
    config.general = if general.join(SETUP_FILE).exists() { read_setup(&general)? } else { Config::default().general };
    config.games = setup_dirs(&root.join(SETUPS))?.iter().map(|dir| read_setup(dir)).collect::<Result<_>>()?;
    let shared = root.join(SHARED);
    config.shared = Shared {
        macros: read_items(&shared.join("macros"))?,
        menus: read_items(&shared.join("menus"))?,
        info: read_items(&shared.join("info"))?,
        logs: read_items(&shared.join("logs"))?,
    };
    Ok(())
}

/// Writes every setup and shared item under `root`, replacing what is there. The new tree is built
/// beside the old one and then swapped in, so a save that fails part way leaves the old setups.
pub fn save(root: &Path, config: &Config) -> Result<()> {
    let staging = root.join(".saving");
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    write_setup(&staging.join(GENERAL), &config.general)?;
    fs::create_dir_all(staging.join(SETUPS))?;
    for (i, game) in config.games.iter().enumerate() {
        write_setup(&staging.join(SETUPS).join(numbered(i, &game.name)), game)?;
    }
    let shared = staging.join(SHARED);
    write_items(&shared.join("macros"), &config.shared.macros)?;
    write_items(&shared.join("menus"), &config.shared.menus)?;
    write_items(&shared.join("info"), &config.shared.info)?;
    write_items(&shared.join("logs"), &config.shared.logs)?;
    for part in [GENERAL, SETUPS, SHARED] {
        let target = root.join(part);
        if target.exists() {
            fs::remove_dir_all(&target).with_context(|| format!("replacing {}", target.display()))?;
        }
        fs::rename(staging.join(part), &target).with_context(|| format!("writing {}", target.display()))?;
    }
    fs::remove_dir_all(&staging)?;
    Ok(())
}

/// `config.toml`: the app settings, without the setups, which have folders of their own.
pub fn app_settings(config: &Config) -> Result<String> {
    Ok(toml::to_string_pretty(&without(config, &APP_KEYS)?)?)
}

/// Writes `text` to `path`, readable only by its owner. The mode is set when the file is created,
/// and set again in case an earlier run left one with a looser mode.
pub fn write_private(path: &Path, text: &str) -> Result<()> {
    let mut file = fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)?;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    file.write_all(text.as_bytes())?;
    Ok(())
}

/// A setup's folder: `setup.toml`, then one subfolder per kind of item.
fn write_setup(dir: &Path, game: &Game) -> Result<()> {
    fs::create_dir_all(dir)?;
    write_private(&dir.join(SETUP_FILE), &toml::to_string_pretty(&without(game, &SETUP_KEYS)?)?)?;
    write_items(&dir.join("profiles"), &game.profiles)?;
    write_items(&dir.join("layers"), &game.layers)?;
    write_items(&dir.join("macros"), &game.macros)?;
    write_items(&dir.join("menus"), &game.menus)?;
    write_items(&dir.join("info"), &game.info)?;
    write_items(&dir.join("logs"), &game.logs)?;
    Ok(())
}

fn read_setup(dir: &Path) -> Result<Game> {
    let path = dir.join(SETUP_FILE);
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let mut game: Game = toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    game.profiles = read_items(&dir.join("profiles"))?;
    game.layers = read_items(&dir.join("layers"))?;
    game.macros = read_items(&dir.join("macros"))?;
    game.menus = read_items(&dir.join("menus"))?;
    game.info = read_items(&dir.join("info"))?;
    game.logs = read_items(&dir.join("logs"))?;
    Ok(game)
}

/// The setup folders under `dir` (those with a `setup.toml`), in name order.
fn setup_dirs(dir: &Path) -> Result<Vec<PathBuf>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut dirs: Vec<PathBuf> = fs::read_dir(dir)?
        .collect::<std::io::Result<Vec<_>>>()?
        .into_iter()
        .map(|entry| entry.path())
        .filter(|path| path.join(SETUP_FILE).is_file())
        .collect();
    dirs.sort();
    Ok(dirs)
}

/// The items saved in `dir`, in file name order. A missing folder has none.
fn read_items<T: DeserializeOwned>(dir: &Path) -> Result<Vec<T>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut files: Vec<PathBuf> = fs::read_dir(dir)?
        .collect::<std::io::Result<Vec<_>>>()?
        .into_iter()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
        .collect();
    files.sort();
    files
        .iter()
        .map(|path| {
            let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
            toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
        })
        .collect()
}

/// One file per item, named by its position and name, so the files sort in the list's order.
fn write_items<T: Serialize + Named>(dir: &Path, items: &[T]) -> Result<()> {
    fs::create_dir_all(dir)?;
    for (i, item) in items.iter().enumerate() {
        write_private(&dir.join(format!("{}.toml", numbered(i, item.name()))), &toml::to_string_pretty(item)?)?;
    }
    Ok(())
}

/// A file or folder name for the item at `index`: its position, then its name.
fn numbered(index: usize, name: &str) -> String {
    format!("{:03}-{}", index + 1, slug(name))
}

/// `name` with anything that isn't a letter, digit, space, dash or underscore as an underscore.
fn slug(name: &str) -> String {
    let slug: String = name.chars().map(|c| if c.is_alphanumeric() || " -_".contains(c) { c } else { '_' }).collect();
    let slug = slug.trim();
    if slug.is_empty() { "item".into() } else { slug.chars().take(60).collect() }
}

/// `value` as a TOML table, without the `keys`.
fn without<T: Serialize>(value: &T, keys: &[&str]) -> Result<toml::Table> {
    let mut table: toml::Table = toml::from_str(&toml::to_string(value)?)?;
    for key in keys {
        table.remove(*key);
    }
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Profile;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("padwight-store-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_config_is_saved_as_folders_and_loads_back_unchanged() {
        let dir = scratch("roundtrip");
        let path = dir.join("config.toml");
        let mut config = Config::default();
        config.games.push(Game::new("Doom", vec![Profile::passthrough("Gamepad"), Profile::desktop("Desktop")]));
        config.games.push(Game::new("Hades", vec![Profile::passthrough("Gamepad")]));
        config.save_to(&path).unwrap();

        // App settings stay in one file, with no setups in it.
        let app = fs::read_to_string(&path).unwrap();
        assert!(!app.contains("[general") && !app.contains("[[setups") && !app.contains("[shared"));
        // Each setup is a folder: setup.toml, then a subfolder per kind of item, numbered.
        assert!(dir.join("general/setup.toml").is_file());
        assert!(dir.join("general/profiles/001-Gamepad.toml").is_file());
        assert!(dir.join("setups/001-Doom/setup.toml").is_file());
        assert!(dir.join("setups/001-Doom/profiles/002-Desktop.toml").is_file());
        assert!(dir.join("setups/002-Hades/profiles/001-Gamepad.toml").is_file());
        assert!(dir.join("shared/macros").is_dir());

        let loaded = Config::load_from(&path).unwrap();
        assert_eq!(loaded, config);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_removed_setup_leaves_no_folder_behind() {
        let dir = scratch("removed");
        let path = dir.join("config.toml");
        let mut config = Config::default();
        config.games.push(Game::new("Doom", vec![Profile::passthrough("Gamepad")]));
        config.save_to(&path).unwrap();
        config.games.clear();
        config.save_to(&path).unwrap();
        assert!(!dir.join("setups/001-Doom").exists());
        assert_eq!(Config::load_from(&path).unwrap().games.len(), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_general_folder_gives_the_default_general() {
        let dir = scratch("missing-general");
        let path = dir.join("config.toml");
        Config::default().save_to(&path).unwrap();
        fs::remove_dir_all(dir.join("general")).unwrap();
        assert_eq!(Config::load_from(&path).unwrap().general, Config::default().general);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn names_with_slashes_make_safe_file_names() {
        assert_eq!(slug("Dark Souls: Remastered/2"), "Dark Souls_ Remastered_2");
        assert_eq!(slug("  "), "item");
        assert_eq!(numbered(0, "Doom"), "001-Doom");
    }
}
