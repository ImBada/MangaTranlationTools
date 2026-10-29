//! App settings (`appSettings.ts`, `settingsStore.ts`). Values are normalized on load the
//! same way the Node server does, so a hand-edited or partial file never breaks startup.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{CoreError, CoreResult};
use crate::json_io::write_json_private;
use crate::model::JsonMap;

pub const DEFAULT_CODEX_MODEL: &str = "gpt-6.1-sol";
pub const DEFAULT_CODEX_OAUTH_PORT: u16 = 10531;
pub const DEFAULT_OPENAI_COMPATIBLE_BASE_URL: &str = "http://127.0.0.1:11434/v1";
pub const DEFAULT_OPENAI_COMPATIBLE_MODEL: &str = "gemma4:31b";
pub const TRANSLATION_PARALLEL_MIN: u32 = 1;
pub const TRANSLATION_PARALLEL_MAX: u32 = 8;
pub const DEFAULT_TRANSLATION_PARALLEL_MAX: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelProvider {
    #[serde(rename = "openai-codex")]
    OpenAiCodex,
    #[serde(rename = "openai-compatible")]
    OpenAiCompatible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffort {
    None,
    Low,
    Medium,
    High,
    Xhigh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TranslationMode {
    Fast,
    Accuracy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexSettings {
    pub model: String,
    pub reasoning_effort: ReasoningEffort,
    pub oauth_port: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAiCompatibleSettings {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationParallelSettings {
    pub enabled: bool,
    pub max_concurrency: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub model_provider: ModelProvider,
    pub codex: CodexSettings,
    #[serde(rename = "openAICompatible")]
    pub openai_compatible: OpenAiCompatibleSettings,
    pub translation_mode: TranslationMode,
    pub translation_parallel: TranslationParallelSettings,
    pub nsfw_mode: bool,
    pub one_hand_mode: bool,
    /// Interpolate page pixels when zoomed in (off: sharp pixels).
    pub zoom_smoothing: bool,
    /// Added to every translation prompt (tone, house style).
    pub translation_instructions: String,
    /// Keys written by newer versions of either app.
    #[serde(flatten)]
    pub extra: JsonMap,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            model_provider: ModelProvider::OpenAiCodex,
            codex: CodexSettings {
                model: DEFAULT_CODEX_MODEL.into(),
                reasoning_effort: ReasoningEffort::Medium,
                oauth_port: DEFAULT_CODEX_OAUTH_PORT,
            },
            openai_compatible: OpenAiCompatibleSettings {
                base_url: DEFAULT_OPENAI_COMPATIBLE_BASE_URL.into(),
                api_key: String::new(),
                model: DEFAULT_OPENAI_COMPATIBLE_MODEL.into(),
            },
            translation_mode: TranslationMode::Fast,
            translation_parallel: TranslationParallelSettings {
                enabled: false,
                max_concurrency: DEFAULT_TRANSLATION_PARALLEL_MAX,
            },
            nsfw_mode: false,
            one_hand_mode: false,
            zoom_smoothing: false,
            translation_instructions: String::new(),
            extra: JsonMap::new(),
        }
    }
}

fn field<'a>(map: Option<&'a serde_json::Map<String, Value>>, key: &str) -> Option<&'a Value> {
    map.and_then(|map| map.get(key))
}

const KNOWN_KEYS: [&str; 9] = [
    "modelProvider",
    "codex",
    "openAICompatible",
    "translationMode",
    "translationParallel",
    "nsfwMode",
    "oneHandMode",
    "zoomSmoothing",
    "translationInstructions",
];

impl AppSettings {
    /// `resolveDefaultAppSettings(env)`: built-in defaults overridden by
    /// `MANGA_TRANSLATOR_MODEL_PROVIDER`, `_CODEX_*`, `_OPENAI_COMPATIBLE_*` and `_PARALLEL_*`.
    pub fn env_defaults() -> Self {
        Self::defaults_from_env(|name| std::env::var(name).ok())
    }

    fn defaults_from_env(var: impl Fn(&str) -> Option<String>) -> Self {
        let get = |name: &str| var(&format!("MANGA_TRANSLATOR_{name}")).map(Value::String).unwrap_or(Value::Null);
        let raw = serde_json::json!({
            "modelProvider": get("MODEL_PROVIDER"),
            "codex": {
                "model": get("CODEX_MODEL"),
                "reasoningEffort": get("CODEX_REASONING_EFFORT"),
                "oauthPort": get("CODEX_OAUTH_PORT"),
            },
            "openAICompatible": {
                "baseUrl": get("OPENAI_COMPATIBLE_BASE_URL"),
                "apiKey": get("OPENAI_COMPATIBLE_API_KEY"),
                "model": get("OPENAI_COMPATIBLE_MODEL"),
            },
            "translationParallel": {
                "enabled": get("PARALLEL_ENABLED"),
                "maxConcurrency": get("PARALLEL_MAX_CONCURRENCY"),
            },
        });
        let mut settings = Self::from_json_with(&raw, &Self::default());
        // `resolveString`: an unset key keeps the empty default.
        if raw["openAICompatible"]["apiKey"].is_null() {
            settings.openai_compatible.api_key.clear();
        }
        settings
    }

    /// `normalizeAppSettings`: takes whatever JSON is present and falls back per field to
    /// the environment-aware defaults.
    pub fn from_json(raw: &Value) -> Self {
        Self::from_json_with(raw, &Self::env_defaults())
    }

    fn from_json_with(raw: &Value, defaults: &Self) -> Self {
        let defaults = defaults.clone();
        let record = raw.as_object();
        let get = |key: &str| record.and_then(|map| map.get(key));
        let codex = get("codex").and_then(Value::as_object);
        let compatible = get("openAICompatible").and_then(Value::as_object);
        let parallel = get("translationParallel").and_then(Value::as_object);

        Self {
            model_provider: enum_or(get("modelProvider"), defaults.model_provider),
            codex: CodexSettings {
                model: non_empty_string_or(field(codex, "model"), &defaults.codex.model),
                reasoning_effort: match field(codex, "reasoningEffort").and_then(Value::as_str) {
                    Some("minimal") => ReasoningEffort::Low,
                    _ => enum_or(field(codex, "reasoningEffort"), defaults.codex.reasoning_effort),
                },
                oauth_port: port_or(field(codex, "oauthPort"), defaults.codex.oauth_port),
            },
            openai_compatible: OpenAiCompatibleSettings {
                base_url: base_url_or(field(compatible, "baseUrl"), &defaults.openai_compatible.base_url),
                api_key: field(compatible, "apiKey")
                    .and_then(Value::as_str)
                    .map(|key| key.trim().to_owned())
                    .unwrap_or(defaults.openai_compatible.api_key),
                model: non_empty_string_or(field(compatible, "model"), &defaults.openai_compatible.model),
            },
            translation_mode: enum_or(get("translationMode"), defaults.translation_mode),
            translation_parallel: TranslationParallelSettings {
                enabled: bool_or(field(parallel, "enabled"), defaults.translation_parallel.enabled),
                max_concurrency: field(parallel, "maxConcurrency")
                    .and_then(integer_from)
                    .map(|value| value.clamp(TRANSLATION_PARALLEL_MIN.into(), TRANSLATION_PARALLEL_MAX.into()) as u32)
                    .unwrap_or(defaults.translation_parallel.max_concurrency),
            },
            nsfw_mode: bool_or(get("nsfwMode"), defaults.nsfw_mode),
            one_hand_mode: bool_or(get("oneHandMode"), defaults.one_hand_mode),
            zoom_smoothing: bool_or(get("zoomSmoothing"), defaults.zoom_smoothing),
            translation_instructions: get("translationInstructions")
                .and_then(Value::as_str)
                .map(|text| text.trim().to_owned())
                .unwrap_or(defaults.translation_instructions),
            extra: record
                .map(|map| {
                    map.iter()
                        .filter(|(key, _)| !KNOWN_KEYS.contains(&key.as_str()))
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    /// Re-normalizes values edited in the UI before saving.
    pub fn normalized(&self) -> Self {
        Self::from_json(&serde_json::to_value(self).unwrap_or(Value::Null))
    }

    pub fn load(path: &Path) -> CoreResult<Self> {
        match std::fs::read(path) {
            Ok(bytes) => Ok(serde_json::from_slice::<Value>(&bytes)
                .map(|raw| Self::from_json(&raw))
                .unwrap_or_else(|_| Self::env_defaults())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::env_defaults()),
            Err(source) => Err(CoreError::io(path, source)),
        }
    }

    /// Writes `settings.json` readable by the owner only: it holds the API key.
    pub fn save(&self, path: &Path) -> CoreResult<Self> {
        let normalized = self.normalized();
        write_json_private(path, &normalized)?;
        Ok(normalized)
    }

    /// Generation parameters per mode (`TRANSLATION_MODE_DEFAULTS`).
    pub fn mode_defaults(&self) -> TranslationModeDefaults {
        match self.translation_mode {
            TranslationMode::Fast => TranslationModeDefaults {
                max_tokens: 900,
                image_min_tokens: 640,
                image_max_tokens: 640,
                include_enhanced_variant: false,
            },
            TranslationMode::Accuracy => TranslationModeDefaults {
                max_tokens: 1400,
                image_min_tokens: 1120,
                image_max_tokens: 1120,
                include_enhanced_variant: true,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TranslationModeDefaults {
    pub max_tokens: u32,
    pub image_min_tokens: u32,
    pub image_max_tokens: u32,
    pub include_enhanced_variant: bool,
}

fn enum_or<T: for<'de> Deserialize<'de>>(value: Option<&Value>, fallback: T) -> T {
    value
        .and_then(Value::as_str)
        .and_then(|text| serde_json::from_value(Value::String(text.trim().to_ascii_lowercase())).ok())
        .unwrap_or(fallback)
}

fn non_empty_string_or(value: Option<&Value>, fallback: &str) -> String {
    value.and_then(Value::as_str).map(str::trim).filter(|text| !text.is_empty()).unwrap_or(fallback).to_owned()
}

fn base_url_or(value: Option<&Value>, fallback: &str) -> String {
    non_empty_string_or(value, fallback).trim_end_matches('/').to_owned()
}

fn bool_or(value: Option<&Value>, fallback: bool) -> bool {
    match value {
        Some(Value::Bool(flag)) => *flag,
        Some(Value::Number(number)) if number.as_f64() == Some(1.0) => true,
        Some(Value::Number(number)) if number.as_f64() == Some(0.0) => false,
        Some(Value::String(text)) => match text.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => true,
            "0" | "false" | "no" | "off" => false,
            _ => fallback,
        },
        _ => fallback,
    }
}

fn number_from(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
    .filter(|number: &f64| number.is_finite())
}

/// `Number.isInteger(Number(value))`.
fn integer_from(value: &Value) -> Option<i64> {
    number_from(value).filter(|number| number.fract() == 0.0 && number.abs() < 9e15).map(|number| number as i64)
}

fn port_or(value: Option<&Value>, fallback: u16) -> u16 {
    value.and_then(integer_from).map(|port| port.clamp(0, 65535) as u16).unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalizes_partial_and_invalid_values() {
        let settings = AppSettings::from_json(&json!({
            "modelProvider": "openai-compatible",
            "codex": { "model": "  ", "reasoningEffort": "HIGH", "oauthPort": 70000 },
            "openAICompatible": { "baseUrl": "https://ollama.com/v1/", "model": "kimi" },
            "translationMode": "accuracy",
            "translationParallel": { "enabled": "true", "maxConcurrency": 99 },
            "nsfwMode": true,
            "futureKey": 1
        }));
        assert_eq!(settings.model_provider, ModelProvider::OpenAiCompatible);
        assert_eq!(settings.codex.model, DEFAULT_CODEX_MODEL);
        assert_eq!(settings.codex.reasoning_effort, ReasoningEffort::High);
        assert_eq!(settings.codex.oauth_port, 65535, "ports are clamped like the TS resolver");
        assert_eq!(settings.openai_compatible.base_url, "https://ollama.com/v1");
        assert_eq!(settings.translation_mode, TranslationMode::Accuracy);
        assert!(settings.translation_parallel.enabled);
        assert_eq!(settings.translation_parallel.max_concurrency, TRANSLATION_PARALLEL_MAX);
        assert!(settings.nsfw_mode);
        assert!(!settings.one_hand_mode);
        assert!(!settings.zoom_smoothing, "zoomed pages stay sharp by default");
        assert_eq!(settings.extra.get("futureKey"), Some(&json!(1)));
    }

    #[test]
    fn legacy_and_non_integer_values() {
        let settings = AppSettings::from_json(&json!({
            "codex": { "reasoningEffort": "minimal", "oauthPort": "10532" },
            "translationParallel": { "enabled": 1, "maxConcurrency": 2.5 },
            "oneHandMode": 0
        }));
        assert_eq!(settings.codex.reasoning_effort, ReasoningEffort::Low);
        assert_eq!(settings.codex.oauth_port, 10532);
        assert!(settings.translation_parallel.enabled);
        assert_eq!(settings.translation_parallel.max_concurrency, DEFAULT_TRANSLATION_PARALLEL_MAX);
        assert!(!settings.one_hand_mode);
    }

    #[test]
    fn serializes_with_ts_key_names() {
        let value = serde_json::to_value(AppSettings::default()).unwrap();
        assert!(value.get("openAICompatible").is_some());
        assert_eq!(value["codex"]["reasoningEffort"], "medium");
        assert_eq!(value["modelProvider"], "openai-codex");
        assert_eq!(AppSettings::from_json(&value), AppSettings::default());
    }

    #[cfg(unix)]
    #[test]
    fn saved_settings_are_readable_by_the_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut settings = AppSettings::default();
        settings.openai_compatible.api_key = "sk-secret".into();
        settings.save(&path).unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(AppSettings::load(&path).unwrap().openai_compatible.api_key, "sk-secret");
    }

    #[test]
    fn garbage_file_falls_back_to_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "{not json").unwrap();
        assert_eq!(AppSettings::load(&path).unwrap(), AppSettings::default());
    }

    #[test]
    fn environment_overrides_defaults_like_ts() {
        let env = |name: &str| match name {
            "MANGA_TRANSLATOR_MODEL_PROVIDER" => Some("openai-compatible".to_owned()),
            "MANGA_TRANSLATOR_OPENAI_COMPATIBLE_BASE_URL" => Some("http://host:1/v1/".to_owned()),
            "MANGA_TRANSLATOR_CODEX_OAUTH_PORT" => Some("12000".to_owned()),
            "MANGA_TRANSLATOR_PARALLEL_ENABLED" => Some("yes".to_owned()),
            "MANGA_TRANSLATOR_PARALLEL_MAX_CONCURRENCY" => Some("99".to_owned()),
            _ => None,
        };
        let settings = AppSettings::defaults_from_env(env);
        assert_eq!(settings.model_provider, ModelProvider::OpenAiCompatible);
        assert_eq!(settings.openai_compatible.base_url, "http://host:1/v1");
        assert_eq!(settings.codex.oauth_port, 12000);
        assert!(settings.translation_parallel.enabled);
        assert_eq!(settings.translation_parallel.max_concurrency, TRANSLATION_PARALLEL_MAX);
        assert_eq!(AppSettings::defaults_from_env(|_| None), AppSettings::default());
    }
}
