use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ApiConfig {
    #[serde(rename = "BASE_URL")]
    pub base_url: Option<String>,
    #[serde(rename = "AUTH_TOKEN")]
    pub auth_token: Option<String>,
    #[serde(rename = "MODEL")]
    pub model: Option<String>,
    #[serde(rename = "SMALL_FAST_MODEL")]
    pub small_fast_model: Option<String>,
}

impl ApiConfig {
    /// Returns an iterator of (env_var_name, Option<value>) for all managed fields.
    pub fn env_pairs(&self) -> Vec<(&'static str, Option<&str>)> {
        vec![
            ("ANTHROPIC_BASE_URL", self.base_url.as_deref()),
            ("ANTHROPIC_AUTH_TOKEN", self.auth_token.as_deref()),
            ("ANTHROPIC_MODEL", self.model.as_deref()),
            ("ANTHROPIC_SMALL_FAST_MODEL", self.small_fast_model.as_deref()),
        ]
    }
}

pub type ConfigMap = HashMap<String, ApiConfig>;

/// Returns the path to the CCS config file: ~/.config/ccs/config.toml
pub fn config_path() -> Result<PathBuf, String> {
    dirs::home_dir()
        .map(|d| d.join(".config").join("ccs").join("config.toml"))
        .ok_or_else(|| "Cannot determine home directory".to_string())
}

/// Load and parse the CCS config file.
pub fn load_config() -> Result<ConfigMap, String> {
    let path = config_path()?;
    if !path.exists() {
        return Err(format!(
            "Config file not found: {}\nCreate it with your API configurations.",
            path.display()
        ));
    }
    let content = fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    let config: ConfigMap =
        toml::from_str(&content).map_err(|e| format!("Failed to parse config: {}", e))?;
    Ok(config)
}

/// Get a specific API config by name.
pub fn get_api_config(name: &str) -> Result<(ConfigMap, ApiConfig), String> {
    let configs = load_config()?;
    let api = configs
        .get(name)
        .cloned()
        .ok_or_else(|| {
            let available: Vec<&str> = configs.keys().map(|s| s.as_str()).collect();
            format!(
                "API config '{}' not found. Available: {}",
                name,
                available.join(", ")
            )
        })?;
    Ok((configs, api))
}

/// Check if a configuration with the given name already exists.
pub fn config_exists(name: &str) -> Result<bool, String> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(false);
    }
    let content = fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    let config: ConfigMap =
        toml::from_str(&content).map_err(|e| format!("Failed to parse config: {}", e))?;
    Ok(config.contains_key(name))
}

/// Save an API configuration to the config file.
/// If `force` is false and the name already exists, returns an error.
pub fn save_config(name: &str, api: ApiConfig, force: bool) -> Result<(), String> {
    let path = config_path()?;

    // Read existing config or start with empty map
    let mut config: ConfigMap = if path.exists() {
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
        toml::from_str(&content).map_err(|e| format!("Failed to parse config: {}", e))?
    } else {
        HashMap::new()
    };

    // Check for existing entry
    if !force && config.contains_key(name) {
        return Err(format!("Configuration '{}' already exists.", name));
    }

    // Insert the new config
    config.insert(name.to_string(), api);

    // Serialize and write back
    let content =
        toml::to_string_pretty(&config).map_err(|e| format!("Failed to serialize config: {}", e))?;

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config directory: {}", e))?;
    }

    fs::write(&path, content)
        .map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;

    Ok(())
}
