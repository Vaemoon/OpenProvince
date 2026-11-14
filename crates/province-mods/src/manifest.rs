use crate::error::Error;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModCategory {
    #[serde(alias = "Vehicles", alias = "vehicles", alias = "Транспорт")]
    Vehicles,
    #[serde(alias = "Skins", alias = "skins", alias = "Персонажи", alias = "Скины")]
    Skins,
    #[serde(alias = "Weapons", alias = "weapons", alias = "Оружие")]
    Weapons,
    #[serde(alias = "Tuning", alias = "tuning", alias = "Тюнинг")]
    Tuning,
    #[serde(alias = "Audio", alias = "audio", alias = "Звуки")]
    Audio,
    #[serde(alias = "Map", alias = "map", alias = "Карта")]
    Map,
    #[serde(alias = "Other", alias = "other", alias = "Разное", alias = "Прочее")]
    Other,
}

impl Default for ModCategory {
    fn default() -> Self {
        Self::Vehicles
    }
}

impl ModCategory {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Vehicles => "Транспорт",
            Self::Skins => "Персонажи",
            Self::Weapons => "Оружие",
            Self::Tuning => "Тюнинг",
            Self::Audio => "Звуки",
            Self::Map => "Карта",
            Self::Other => "Разное",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplacementEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive: Option<String>,

    pub file: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModManifest {
    #[serde(default)]
    pub format_version: Option<u32>,
    pub id: String,
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default = "default_author")]
    pub author: String,
    #[serde(default)]
    pub category: ModCategory,
    #[serde(default)]
    pub description: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_archive: Option<String>,
    #[serde(default)]
    pub replaces: Vec<ReplacementEntry>,
}

fn default_version() -> String {
    "1.0".to_string()
}

fn default_author() -> String {
    "Unknown".to_string()
}

impl ModManifest {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            format_version: Some(1),
            id: id.into(),
            name: name.into(),
            version: default_version(),
            author: default_author(),
            category: ModCategory::default(),
            description: String::new(),
            target_archive: None,
            replaces: Vec::new(),
        }
    }

    pub fn load_from_file(path: &Path) -> Result<Self, Error> {
        let content = fs::read_to_string(path)?;
        let manifest = serde_json::from_str(&content)?;
        Ok(manifest)
    }

    pub fn save_to_file(&self, path: &Path) -> Result<(), Error> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }
}
