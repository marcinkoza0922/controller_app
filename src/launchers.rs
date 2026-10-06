//! Which games are installed (Steam, Heroic, Lutris) or running, so the library picker can
//! put the user's own games first. Games are matched through their auto-switch rules.

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use crate::{
    config::{Rule, RuleKind},
    ipc::WindowInfo,
};

/// How deep to look inside a game's install folder for its executables, and how many
/// entries to look at per game at most.
const SEARCH_DEPTH: usize = 4;
const SEARCH_BUDGET: usize = 20_000;

#[derive(Debug, Clone, Default)]
pub struct Installed {
    /// Steam App IDs with an install manifest.
    pub steam: HashSet<String>,
    /// File names (lowercase) found in installed games' folders, e.g. `eldenring.exe`.
    pub files: HashSet<String>,
    pub running: Vec<WindowInfo>,
}

impl Installed {
    /// True if any of a game's rules points at something installed or running.
    pub fn has(&self, rules: &[Rule]) -> bool {
        rules.iter().any(|r| {
            let value = r.value.trim();
            !value.is_empty()
                && match r.kind {
                    RuleKind::SteamAppId => self.steam.contains(value),
                    RuleKind::Executable => self.files.contains(&value.to_lowercase()),
                    RuleKind::WindowClass => false,
                }
        }) || rules.iter().any(|r| crate::focus::rule_matches_any(r, &self.running))
    }
}

/// Looks through the launchers' files and the running processes. Reads the disk: run it off
/// the UI thread.
pub fn scan() -> Installed {
    let mut found = Installed { running: crate::focus::running_processes(), ..Installed::default() };
    let Some(home) = dirs::home_dir() else { return found };
    let mut folders = Vec::new();

    for root in steam_roots(&home) {
        for library in steam_libraries(&root) {
            let apps = library.join("steamapps");
            let Ok(entries) = fs::read_dir(&apps) else { continue };
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !(name.starts_with("appmanifest_") && name.ends_with(".acf")) {
                    continue;
                }
                let Ok(text) = fs::read_to_string(entry.path()) else { continue };
                let (id, dir) = parse_manifest(&text);
                if let Some(id) = id {
                    found.steam.insert(id);
                }
                if let Some(dir) = dir {
                    folders.push(apps.join("common").join(dir));
                }
            }
        }
    }

    for heroic in [home.join(".config/heroic"), home.join(".var/app/com.heroicgameslauncher.hgl/config/heroic")] {
        let legendary = heroic.join("legendaryConfig/legendary/installed.json");
        let gog = heroic.join("gog_store/installed.json");
        for file in [legendary, gog] {
            if let Ok(text) = fs::read_to_string(file) {
                let (dirs, exes) = parse_heroic(&text);
                folders.extend(dirs);
                found.files.extend(exes);
            }
        }
    }

    for lutris in [
        home.join(".local/share/lutris/games"),
        home.join(".config/lutris/games"),
        home.join(".var/app/net.lutris.Lutris/data/lutris/games"),
    ] {
        let Ok(entries) = fs::read_dir(lutris) else { continue };
        for entry in entries.flatten() {
            if let Ok(text) = fs::read_to_string(entry.path()) {
                found.files.extend(parse_lutris(&text));
            }
        }
    }

    for folder in folders {
        let mut budget = SEARCH_BUDGET;
        collect_files(&folder, SEARCH_DEPTH, &mut budget, &mut found.files);
    }
    found
}

fn steam_roots(home: &Path) -> Vec<PathBuf> {
    let candidates = [
        home.join(".local/share/Steam"),
        home.join(".steam/steam"),
        home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
    ];
    // ~/.steam/steam is usually a link to the first; look at each folder once.
    let mut seen = HashSet::new();
    candidates.into_iter().filter(|p| p.is_dir() && seen.insert(fs::canonicalize(p).unwrap_or(p.clone()))).collect()
}

/// A Steam install's library folders: its own, plus those in `libraryfolders.vdf`.
fn steam_libraries(root: &Path) -> Vec<PathBuf> {
    let mut libraries = vec![root.to_path_buf()];
    if let Ok(text) = fs::read_to_string(root.join("steamapps/libraryfolders.vdf")) {
        libraries.extend(vdf_values(&text, "path").into_iter().map(PathBuf::from));
    }
    let mut seen = HashSet::new();
    libraries.retain(|p| seen.insert(fs::canonicalize(p).unwrap_or(p.clone())));
    libraries
}

/// Every value of `key` in Valve's KeyValues text (`"key"  "value"` lines).
fn vdf_values(text: &str, key: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('"').collect();
            // ["\t", key, "\t\t", value, ""]
            (parts.len() >= 5 && parts[1].eq_ignore_ascii_case(key)).then(|| parts[3].replace("\\\\", "\\"))
        })
        .collect()
}

/// A Steam app manifest's App ID and install folder name.
fn parse_manifest(text: &str) -> (Option<String>, Option<String>) {
    (vdf_values(text, "appid").into_iter().next(), vdf_values(text, "installdir").into_iter().next())
}

/// Heroic's installed-games lists (Epic via Legendary, and GOG): install folders, and
/// executable names where listed.
fn parse_heroic(text: &str) -> (Vec<PathBuf>, Vec<String>) {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(text) else { return (Vec::new(), Vec::new()) };
    let games: Vec<&serde_json::Value> = match &json {
        // GOG: {"installed": [...]}; Legendary: {"AppName": {...}, ...}
        serde_json::Value::Object(map) => match map.get("installed") {
            Some(serde_json::Value::Array(list)) => list.iter().collect(),
            _ => map.values().collect(),
        },
        serde_json::Value::Array(list) => list.iter().collect(),
        _ => Vec::new(),
    };
    let (mut dirs, mut exes) = (Vec::new(), Vec::new());
    for game in games {
        if let Some(path) = game.get("install_path").and_then(|p| p.as_str()) {
            dirs.push(PathBuf::from(path));
        }
        if let Some(exe) = game.get("executable").and_then(|e| e.as_str()) {
            exes.push(file_name(exe));
        }
    }
    (dirs, exes)
}

/// The executable a Lutris game config (YAML) launches, by file name.
fn parse_lutris(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.trim().strip_prefix("exe:"))
        .map(|v| file_name(v.trim().trim_matches(['"', '\''])))
        .filter(|n| !n.is_empty())
        .collect()
}

/// Lowercase file name of a Unix or Windows path.
fn file_name(path: &str) -> String {
    crate::focus::base_name(path).to_lowercase()
}

fn collect_files(dir: &Path, depth: usize, budget: &mut usize, out: &mut HashSet<String>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        if *budget == 0 {
            return;
        }
        *budget -= 1;
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            if depth > 0 {
                collect_files(&entry.path(), depth - 1, budget, out);
            }
        } else if kind.is_file() {
            out.insert(entry.file_name().to_string_lossy().to_lowercase());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_steam_library_folders_and_manifests() {
        let folders = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"/home/me/.local/share/Steam"
		"label"		""
	}
	"1"
	{
		"path"		"/mnt/games/SteamLibrary"
	}
}"#;
        assert_eq!(vdf_values(folders, "path"), ["/home/me/.local/share/Steam", "/mnt/games/SteamLibrary"]);
        let manifest = "\"AppState\"\n{\n\t\"appid\"\t\t\"1245620\"\n\t\"name\"\t\t\"ELDEN RING\"\n\t\"installdir\"\t\t\"ELDEN RING\"\n}";
        assert_eq!(parse_manifest(manifest), (Some("1245620".into()), Some("ELDEN RING".into())));
    }

    #[test]
    fn reads_heroic_and_lutris_lists() {
        let legendary = r#"{"Fortnite": {"install_path": "/games/Fortnite", "executable": "FortniteGame/Binaries/Win64/FortniteClient.exe"}}"#;
        assert_eq!(parse_heroic(legendary), (vec![PathBuf::from("/games/Fortnite")], vec!["fortniteclient.exe".to_string()]));
        let gog = r#"{"installed": [{"appName": "1", "install_path": "/games/Witcher 3"}]}"#;
        assert_eq!(parse_heroic(gog).0, [PathBuf::from("/games/Witcher 3")]);
        assert_eq!(parse_heroic("not json"), (Vec::new(), Vec::new()));
        let lutris = "game:\n  exe: /home/me/Games/diablo/drive_c/Diablo II/Game.exe\n  prefix: /x\nsystem: {}\n";
        assert_eq!(parse_lutris(lutris), ["game.exe"]);
    }

    #[test]
    fn games_match_through_their_rules() {
        let installed = Installed {
            steam: HashSet::from(["1245620".to_string()]),
            files: HashSet::from(["factorio".to_string()]),
            running: vec![WindowInfo { exe: "Hades.exe".into(), ..WindowInfo::default() }],
        };
        let rule = |kind, value: &str| vec![Rule::new(kind, value, "P")];
        assert!(installed.has(&rule(RuleKind::SteamAppId, "1245620")));
        assert!(installed.has(&rule(RuleKind::Executable, "Factorio")));
        assert!(installed.has(&rule(RuleKind::Executable, "hades.exe")), "running counts");
        assert!(!installed.has(&rule(RuleKind::Executable, "doom.exe")));
        assert!(!installed.has(&[]));
    }

    #[test]
    fn finds_files_in_install_folders_within_limits() {
        let dir = std::env::temp_dir().join(format!("launchers-test-{}", std::process::id()));
        fs::create_dir_all(dir.join("bin/x64")).unwrap();
        fs::write(dir.join("bin/x64/Game.EXE"), "").unwrap();
        let mut files = HashSet::new();
        let mut budget = 100;
        collect_files(&dir, 4, &mut budget, &mut files);
        assert!(files.contains("game.exe"));
        let mut shallow = HashSet::new();
        collect_files(&dir, 0, &mut 100, &mut shallow);
        assert!(shallow.is_empty(), "only one level, which holds just a folder");
        fs::remove_dir_all(dir).unwrap();
    }
}
