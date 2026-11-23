//! Files EQaura keeps on this computer, and API keys in the system key store.

pub mod files;
pub mod keys;

use eq_core::profile::Profile;
use files::Dir;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    System,
    Dark,
    Light,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiChoice {
    #[default]
    Rules,
    Anthropic,
    OpenAi,
    Ollama,
}

impl AiChoice {
    pub const ALL: [AiChoice; 4] =
        [AiChoice::Rules, AiChoice::Anthropic, AiChoice::OpenAi, AiChoice::Ollama];

    pub fn id(self) -> &'static str {
        match self {
            AiChoice::Rules => "rules",
            AiChoice::Anthropic => "anthropic",
            AiChoice::OpenAi => "openai",
            AiChoice::Ollama => "ollama",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AiSettings {
    pub url: String,
    pub model: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub schema: u32,
    pub theme: Theme,
    pub enabled: bool,
    pub slider_view: bool,
    pub ai: AiChoice,
    /// Address and model per provider, so switching back keeps them.
    pub ai_settings: BTreeMap<String, AiSettings>,
    pub send_hearing: bool,
    pub start_at_login: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema: 1,
            theme: Theme::System,
            enabled: true,
            slider_view: false,
            ai: AiChoice::Rules,
            ai_settings: BTreeMap::new(),
            send_hearing: false,
            start_at_login: false,
        }
    }
}

/// The sound of each output device.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Devices {
    pub schema: u32,
    pub devices: BTreeMap<String, DeviceState>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DeviceState {
    pub name: String,
    pub profile: Profile,
    /// The preset this sound was loaded from, if any.
    #[serde(default)]
    pub preset: Option<String>,
}

pub struct Store {
    dir: Dir,
}

impl Store {
    pub fn open() -> Self {
        Self::at(default_dir())
    }

    pub fn at(path: PathBuf) -> Self {
        Self { dir: Dir::new(path) }
    }

    pub fn path(&self) -> &std::path::Path {
        self.dir.path()
    }

    pub fn settings(&self) -> Settings {
        self.dir.read_json("settings.json").unwrap_or_default()
    }

    pub fn save_settings(&self, s: &Settings) -> std::io::Result<()> {
        self.dir.write_json("settings.json", s)
    }

    pub fn devices(&self) -> Devices {
        self.dir.read_json("devices.json").unwrap_or_default()
    }

    pub fn save_devices(&self, d: &Devices) -> std::io::Result<()> {
        self.dir.write_json("devices.json", d)
    }

    /// Saved presets, sorted by name.
    pub fn presets(&self) -> Vec<Profile> {
        let mut out: Vec<Profile> = self
            .dir
            .list("presets", "json")
            .iter()
            .filter_map(|p| std::fs::read_to_string(p).ok())
            .filter_map(|t| Profile::from_json(&t).ok())
            .collect();
        out.sort_by_key(|p| p.name.to_lowercase());
        out
    }

    /// Finds the file that holds a preset of this name.
    fn preset_file(&self, name: &str) -> Option<PathBuf> {
        self.dir.list("presets", "json").into_iter().find(|p| {
            std::fs::read_to_string(p)
                .ok()
                .and_then(|t| Profile::from_json(&t).ok())
                .is_some_and(|x| x.name == name)
        })
    }

    pub fn save_preset(&self, p: &Profile) -> std::io::Result<()> {
        let file = match self.preset_file(&p.name) {
            Some(f) => f,
            None => self.dir.free_name("presets", &p.name, "json"),
        };
        files::write_atomic(&file, p.to_json().as_bytes())
    }

    pub fn delete_preset(&self, name: &str) -> std::io::Result<()> {
        match self.preset_file(name) {
            Some(f) => std::fs::remove_file(f),
            None => Ok(()),
        }
    }

    pub fn rename_preset(&self, old: &str, new: &str) -> std::io::Result<()> {
        let Some(file) = self.preset_file(old) else { return Ok(()) };
        let text = std::fs::read_to_string(&file)?;
        let Ok(mut p) = Profile::from_json(&text) else { return Ok(()) };
        p.name = new.into();
        // Write the new file first, so a failure never loses the preset.
        let target = self.dir.free_name("presets", new, "json");
        files::write_atomic(&target, p.to_json().as_bytes())?;
        std::fs::remove_file(file)
    }

    /// Adds a line to the local log. Keeps five files of at most 1 MB.
    pub fn log(&self, line: &str) {
        self.dir.log(line);
    }

    pub fn log_text(&self) -> String {
        std::fs::read_to_string(self.dir.path().join("eqaura.log")).unwrap_or_default()
    }
}

/// Where EQaura keeps its files. `EQAURA_HOME` overrides it.
pub fn default_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("EQAURA_HOME").filter(|p| !p.is_empty()) {
        return p.into();
    }
    let var = |n: &str| std::env::var_os(n).filter(|v| !v.is_empty()).map(PathBuf::from);
    if cfg!(target_os = "windows") {
        var("APPDATA").unwrap_or_else(std::env::temp_dir).join("EQaura")
    } else if cfg!(target_os = "macos") {
        var("HOME").unwrap_or_else(std::env::temp_dir).join("Library/Application Support/EQaura")
    } else {
        var("XDG_CONFIG_HOME")
            .or_else(|| var("HOME").map(|h| h.join(".config")))
            .unwrap_or_else(std::env::temp_dir)
            .join("eqaura")
    }
}
