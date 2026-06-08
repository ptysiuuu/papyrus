use papyrus_lib::plugin::{PluginManifest, PluginSource, discover_plugins};
use std::fs;
use tempfile::tempdir;

#[test]
fn test_discover_plugins_empty_dir() {
    let dir = tempdir().unwrap();
    let plugins = discover_plugins(dir.path()).unwrap();
    assert!(plugins.is_empty());
}

#[test]
fn test_discover_plugins_finds_manifest() {
    let dir = tempdir().unwrap();
    let plugin_dir = dir.path().join("dblp");
    fs::create_dir_all(&plugin_dir).unwrap();

    let manifest = PluginManifest {
        name: "dblp".to_string(),
        version: "0.1.0".to_string(),
        description: "DBLP computer science bibliography".to_string(),
        binary: "papyrus-plugin-dblp".to_string(),
        sources: vec!["dblp".to_string()],
    };
    let manifest_path = plugin_dir.join("manifest.toml");
    fs::write(&manifest_path, toml::to_string(&manifest).unwrap()).unwrap();

    let plugins = discover_plugins(dir.path()).unwrap();
    assert_eq!(plugins.len(), 1);
    assert_eq!(plugins[0].name, "dblp");
}

#[test]
fn test_plugin_manifest_serialization() {
    let manifest = PluginManifest {
        name: "test".to_string(),
        version: "1.0.0".to_string(),
        description: "Test plugin".to_string(),
        binary: "papyrus-plugin-test".to_string(),
        sources: vec!["test_source".to_string()],
    };
    let toml_str = toml::to_string(&manifest).unwrap();
    let parsed: PluginManifest = toml::from_str(&toml_str).unwrap();
    assert_eq!(parsed.name, manifest.name);
    assert_eq!(parsed.binary, manifest.binary);
}

#[test]
fn test_plugin_protocol_request_format() {
    // Verify the JSON format of a plugin search request
    let request = papyrus_lib::plugin::PluginRequest {
        action: "search".to_string(),
        query: Some("transformers".to_string()),
        limit: 10,
        extra: std::collections::HashMap::new(),
    };
    let json = serde_json::to_string(&request).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["action"], "search");
    assert_eq!(parsed["query"], "transformers");
    assert_eq!(parsed["limit"], 10);
}
