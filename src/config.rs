use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Program (plus leading arguments) used to launch Remmina; `-c <file>`
    /// is appended. For the Flatpak: `["flatpak", "run", "org.remmina.Remmina"]`.
    pub remmina_command: Vec<String>,
    /// Folder holding the `.remmina` files. `None` detects it from Remmina's
    /// own settings.
    pub data_dir: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            remmina_command: vec!["remmina".into()],
            data_dir: None,
        }
    }
}

impl Config {
    /// Reads `config.json`, falling back to defaults if it is missing or invalid.
    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_config_keeps_defaults() {
        let config: Config = serde_json::from_str(r#"{"data_dir": "/srv/remmina"}"#).unwrap();
        assert_eq!(config.remmina_command, ["remmina"]);
        assert_eq!(config.data_dir, Some(PathBuf::from("/srv/remmina")));
    }

    #[test]
    fn null_data_dir_means_auto() {
        let config: Config = serde_json::from_str(
            r#"{"remmina_command": ["flatpak", "run", "org.remmina.Remmina"], "data_dir": null}"#,
        )
        .unwrap();
        assert_eq!(config.remmina_command.len(), 3);
        assert_eq!(config.data_dir, None);
    }
}
