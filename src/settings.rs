use crate::config::ApiConfig;
use serde_json::{json, Map, Value};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// The 4 environment variable keys managed by CCS.
const MANAGED_KEYS: &[&str] = &[
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_MODEL",
    "ANTHROPIC_SMALL_FAST_MODEL",
];

/// Returns the path to the global Claude Code settings.json: ~/.claude/settings.json
pub fn global_settings_path() -> Result<PathBuf, String> {
    dirs::home_dir()
        .map(|d| d.join(".claude").join("settings.json"))
        .ok_or_else(|| "Cannot determine home directory".to_string())
}

/// Returns the path to the local (project) Claude Code settings.json: .claude/settings.json
pub fn local_settings_path() -> PathBuf {
    PathBuf::from(".claude").join("settings.json")
}

/// Read settings.json from the given path, returning a JSON Value.
/// If the file doesn't exist, returns an empty object.
fn read_settings(path: &Path) -> Result<Value, String> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let content =
        fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    if content.trim().is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse {}: {}", path.display(), e))
}

/// Write settings JSON to the given path atomically (write to temp, then rename).
fn write_settings(path: &Path, value: &Value) -> Result<(), String> {
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory {}: {}", parent.display(), e))?;
    }

    let json_str = serde_json::to_string_pretty(value)
        .map_err(|e| format!("Failed to serialize JSON: {}", e))?;

    // Write to a temp file in the same directory, then rename for atomicity
    let temp_path = path.with_extension("json.tmp");
    {
        let mut file = fs::File::create(&temp_path)
            .map_err(|e| format!("Failed to create temp file: {}", e))?;
        file.write_all(json_str.as_bytes())
            .map_err(|e| format!("Failed to write temp file: {}", e))?;
        file.write_all(b"\n")
            .map_err(|e| format!("Failed to write temp file: {}", e))?;
    }

    fs::rename(&temp_path, path)
        .map_err(|e| format!("Failed to rename temp file to {}: {}", path.display(), e))?;

    Ok(())
}

/// Apply an ApiConfig to a settings.json file at the given path.
/// Only modifies the `env` sub-object, preserving all other fields.
/// If a CCS field is None, the corresponding env key is removed.
pub fn apply_config(path: &Path, api: &ApiConfig) -> Result<(), String> {
    let mut settings = read_settings(path)?;

    // Ensure settings is an object
    let obj = settings
        .as_object_mut()
        .ok_or_else(|| format!("{} is not a JSON object", path.display()))?;

    // Get or create the "env" sub-object
    if !obj.contains_key("env") {
        obj.insert("env".to_string(), json!({}));
    }
    let env = obj
        .get_mut("env")
        .and_then(|v| v.as_object_mut())
        .ok_or_else(|| format!("'env' field in {} is not an object", path.display()))?;

    // Apply each CCS field
    for (env_key, value) in api.env_pairs() {
        match value {
            Some(v) => {
                env.insert(env_key.to_string(), Value::String(v.to_string()));
            }
            None => {
                env.remove(env_key);
            }
        }
    }

    write_settings(path, &settings)
}

/// Read the current CCS-managed env vars from a settings.json file.
/// Returns a map of env_key -> value for managed keys that are present.
pub fn read_current_env(path: &Path) -> Result<Map<String, Value>, String> {
    let settings = read_settings(path)?;
    let mut result = Map::new();

    if let Some(env) = settings.get("env").and_then(|v| v.as_object()) {
        for &key in MANAGED_KEYS {
            if let Some(val) = env.get(key) {
                result.insert(key.to_string(), val.clone());
            }
        }
    }

    Ok(result)
}
