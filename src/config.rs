use crate::preserver::PreserveConfig;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const CONFIG_FILENAME: &str = ".cjk-token.json";

/// Cache configuration with serde defaults
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CacheConfig {
    pub enabled: bool,
    pub ttl_days: u32,
    pub max_size_mb: u32,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            ttl_days: 30,
            max_size_mb: 10,
        }
    }
}

/// Resilience configuration for retry, timeout, and circuit breaker
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ResilienceConfig {
    /// Request timeout in seconds (default: 30)
    pub timeout_secs: u64,
    /// Connection timeout in seconds (default: 5)
    pub connect_timeout_secs: u64,
    /// Maximum retry attempts for transient failures (default: 3)
    pub max_retries: u32,
    /// Base delay for exponential backoff in milliseconds (default: 200)
    pub retry_base_delay_ms: u64,
    /// Circuit breaker failure threshold before opening (default: 5)
    pub circuit_breaker_threshold: u32,
    /// Circuit breaker reset timeout in seconds (default: 60)
    pub circuit_breaker_reset_secs: u64,
    /// Enable graceful fallback to passthrough on failure (default: true)
    pub fallback_to_passthrough: bool,
}

impl Default for ResilienceConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 30,
            connect_timeout_secs: 5,
            max_retries: 3,
            retry_base_delay_ms: 200,
            circuit_breaker_threshold: 5,
            circuit_breaker_reset_secs: 60,
            fallback_to_passthrough: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub output_language: String,
    pub enable_stats: bool,
    pub threshold: f64,
    /// Translation backend: "google" (default) or "opus-mt" (local, requires local-translate feature)
    pub translation_backend: String,
    /// Collapse internal whitespace to single spaces for token reduction.
    /// WARNING: This destroys code indentation. Only enable for non-code prompts.
    /// Default: false (safe)
    pub normalize_whitespace: bool,
    pub cache: CacheConfig,
    pub preserve: PreserveConfig,
    pub resilience: ResilienceConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            output_language: "en".into(),
            enable_stats: true,
            threshold: 0.1,
            translation_backend: "google".into(),
            normalize_whitespace: false,
            cache: CacheConfig::default(),
            preserve: PreserveConfig::default(),
            resilience: ResilienceConfig::default(),
        }
    }
}

/// Load configuration from file, applying environment variable overrides
pub fn load_config() -> Config {
    let mut config: Config = find_config_file()
        .and_then(|path| {
            let content = std::fs::read_to_string(&path).ok()?;
            match serde_json::from_str(&content) {
                Ok(config) => Some(config),
                Err(e) => {
                    crate::output::print_error(&format!("Config parse error: {e}"));
                    None
                }
            }
        })
        .unwrap_or_default();

    // Apply environment variable overrides
    if let Ok(val) = std::env::var("CJK_TOKEN_OUTPUT_LANG") {
        config.output_language = val;
    }
    if let Ok(val) = std::env::var("CJK_TOKEN_THRESHOLD") {
        if let Ok(threshold) = val.parse::<f64>() {
            config.threshold = threshold;
        }
    }
    if let Ok(val) = std::env::var("CJK_TOKEN_CACHE_ENABLED") {
        config.cache.enabled = val.to_lowercase() == "true" || val == "1";
    }
    if let Ok(val) = std::env::var("CJK_TOKEN_BACKEND") {
        config.translation_backend = val;
    }

    config
}

/// Search for config file in standard locations
fn find_config_file() -> Option<PathBuf> {
    let search_paths = [
        std::env::current_dir().ok(),
        dirs::home_dir(),
        dirs::config_dir().map(|p| p.join("cjk-token-reducer")),
    ];

    for base in search_paths.into_iter().flatten() {
        let config_path = base.join(CONFIG_FILENAME);
        if config_path.exists() {
            return Some(config_path);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.output_language, "en");
        assert_eq!(config.threshold, 0.1);
        assert!(config.enable_stats);
        assert!(!config.normalize_whitespace); // default false for safety
    }

    #[test]
    fn test_normalize_whitespace_config() {
        // Default should be false (safe for code)
        let json = r#"{}"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert!(!config.normalize_whitespace);

        // Can be enabled explicitly
        let json = r#"{"normalizeWhitespace": true}"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert!(config.normalize_whitespace);
    }

    #[test]
    fn test_deserialize_partial() {
        let json = r#"{"threshold": 0.2}"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config.threshold, 0.2);
        assert_eq!(config.output_language, "en"); // default
    }

    #[test]
    fn test_preserve_config_defaults() {
        let config = PreserveConfig::default();
        assert!(config.wiki_markers);
        assert!(config.highlight_markers);
        assert!(config.english_terms);
    }

    #[test]
    fn test_preserve_config_deserialize_defaults() {
        // Empty JSON should use defaults (all true)
        let json = r#"{}"#;
        let config: PreserveConfig = serde_json::from_str(json).unwrap();
        assert!(config.wiki_markers);
        assert!(config.highlight_markers);
        assert!(config.english_terms);
    }

    #[test]
    fn test_preserve_config_partial_override() {
        // Partial config should override only specified fields
        let json = r#"{"wikiMarkers": false}"#;
        let config: PreserveConfig = serde_json::from_str(json).unwrap();
        assert!(!config.wiki_markers); // overridden
        assert!(config.highlight_markers); // default
        assert!(config.english_terms); // default
    }

    #[test]
    fn test_preserve_config_builder_methods() {
        // Test the builder methods for PreserveConfig
        let all_config = PreserveConfig::all();
        assert!(all_config.wiki_markers);
        assert!(all_config.highlight_markers);
        assert!(all_config.english_terms);

        let basic_config = PreserveConfig::basic();
        assert!(!basic_config.wiki_markers);
        assert!(!basic_config.highlight_markers);
        assert!(!basic_config.english_terms);
    }

    #[test]
    fn test_resilience_config_defaults() {
        let config = ResilienceConfig::default();
        assert_eq!(config.timeout_secs, 30);
        assert_eq!(config.connect_timeout_secs, 5);
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_base_delay_ms, 200);
        assert_eq!(config.circuit_breaker_threshold, 5);
        assert_eq!(config.circuit_breaker_reset_secs, 60);
        assert!(config.fallback_to_passthrough);
    }

    #[test]
    fn test_resilience_config_partial_override() {
        let json = r#"{"maxRetries": 5, "timeoutSecs": 60}"#;
        let config: ResilienceConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.max_retries, 5); // overridden
        assert_eq!(config.timeout_secs, 60); // overridden
        assert_eq!(config.connect_timeout_secs, 5); // default
        assert_eq!(config.retry_base_delay_ms, 200); // default
    }

    #[test]
    fn test_config_includes_resilience() {
        let config = Config::default();
        assert_eq!(config.resilience.max_retries, 3);
        assert!(config.resilience.fallback_to_passthrough);
    }

    #[test]
    fn test_cache_config_deserialization() {
        let json = r#"{"enabled": false, "ttlDays": 7, "maxSizeMb": 5}"#;
        let config: CacheConfig = serde_json::from_str(json).unwrap();
        assert!(!config.enabled);
        assert_eq!(config.ttl_days, 7);
        assert_eq!(config.max_size_mb, 5);
    }

    #[test]
    fn test_cache_config_partial_deserialization() {
        // Only override ttlDays, rest should be defaults
        let json = r#"{"ttlDays": 14}"#;
        let config: CacheConfig = serde_json::from_str(json).unwrap();
        assert!(config.enabled); // default
        assert_eq!(config.ttl_days, 14);
        assert_eq!(config.max_size_mb, 10); // default
    }

    #[test]
    fn test_config_with_nested_cache() {
        let json = r#"{"cache": {"enabled": false, "ttlDays": 1}}"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert!(!config.cache.enabled);
        assert_eq!(config.cache.ttl_days, 1);
        assert_eq!(config.cache.max_size_mb, 10); // default
    }

    #[test]
    fn test_config_serialization_roundtrip() {
        let config = Config::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.output_language, config.output_language);
        assert_eq!(deserialized.threshold, config.threshold);
        assert_eq!(deserialized.enable_stats, config.enable_stats);
        assert_eq!(deserialized.cache.enabled, config.cache.enabled);
        assert_eq!(deserialized.cache.ttl_days, config.cache.ttl_days);
        assert_eq!(
            deserialized.resilience.max_retries,
            config.resilience.max_retries
        );
    }

    #[test]
    fn test_resilience_config_fallback_override() {
        let json = r#"{"fallbackToPassthrough": false}"#;
        let config: ResilienceConfig = serde_json::from_str(json).unwrap();
        assert!(!config.fallback_to_passthrough);
        // Other fields should still be defaults
        assert_eq!(config.timeout_secs, 30);
    }
}
