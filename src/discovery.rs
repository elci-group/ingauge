use serde::Serialize;
use std::{collections::BTreeSet, env, fs, path::Path};

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum TargetCategory {
    Provider,
    Router,
    Harness,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct DiscoveredTarget {
    pub category: TargetCategory,
    pub name: String,
    pub installed: bool,
    pub configured: bool,
    pub evidence: String,
}

const PROVIDERS: &[(&str, &str)] = &[
    ("anthropic", "ANTHROPIC_API_KEY"),
    ("arcee", "ARCEEAI_API_KEY"),
    ("bedrock", "AWS_ACCESS_KEY_ID"),
    ("gemini", "GOOGLE_API_KEY"),
    ("glm", "GLM_API_KEY"),
    ("groq", "GROQ_API_KEY"),
    ("grok", "XAI_API_KEY"),
    ("huggingface", "HF_TOKEN"),
    ("kimi", "KIMI_API_KEY"),
    ("minimax", "MINIMAX_API_KEY"),
    ("novita", "NOVITA_API_KEY"),
    ("ollama", "OLLAMA_API_KEY"),
    ("opencode-go", "OPENCODE_GO_API_KEY"),
    ("opencode-zen", "OPENCODE_ZEN_API_KEY"),
    ("openai", "OPENAI_API_KEY"),
    ("openrouter", "OPENROUTER_API_KEY"),
    ("xiaomi", "XIAOMI_API_KEY"),
];

const ROUTERS: &[(&str, &str)] = &[
    ("gia-proxy", "gia-proxy"),
    ("hermes", "hermes"),
    ("ollama", "ollama"),
    ("vico", "vico-desktop"),
    ("vico-vee", "vico-vee"),
];

pub fn discover(harness_directory: Option<&Path>) -> Vec<DiscoveredTarget> {
    let mut targets = Vec::new();
    for (name, variable) in PROVIDERS {
        let configured = env::var_os(variable).is_some();
        targets.push(DiscoveredTarget {
            category: TargetCategory::Provider,
            name: (*name).into(),
            installed: configured || (*name == "ollama" && binary_exists("ollama")),
            configured,
            evidence: format!("environment:{variable}"),
        });
    }
    for (name, binary) in ROUTERS {
        let installed = binary_exists(binary);
        targets.push(DiscoveredTarget {
            category: TargetCategory::Router,
            name: (*name).into(),
            installed,
            configured: installed,
            evidence: format!("path:{binary}"),
        });
    }
    if let Some(directory) = harness_directory {
        discover_harnesses(directory, &mut targets);
    }
    targets.sort();
    targets
}

pub fn default_harness_directory() -> Option<std::path::PathBuf> {
    env::var_os("VICO_HARNESS_DIR")
        .map(Into::into)
        .or_else(|| env::var_os("HOME").map(|home| Path::new(&home).join(".vico/harnesses")))
        .filter(|path| path.is_dir())
}

fn discover_harnesses(directory: &Path, targets: &mut Vec<DiscoveredTarget>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut seen = BTreeSet::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let Ok(bytes) = fs::read(&path) else { continue };
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            continue;
        };
        let Some(name) = value.get("agent_name").and_then(|value| value.as_str()) else {
            continue;
        };
        if !seen.insert(name.to_ascii_lowercase()) {
            continue;
        }
        targets.push(DiscoveredTarget {
            category: TargetCategory::Harness,
            name: name.to_owned(),
            installed: value
                .get("installed")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
            configured: value
                .get("configured")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
            evidence: "vico_manifest".into(),
        });
    }
}

fn binary_exists(binary: &str) -> bool {
    env::var_os("PATH").is_some_and(|path| {
        env::split_paths(&path).any(|directory| {
            let candidate = directory.join(binary);
            candidate.is_file()
        })
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn discovers_every_manifest_without_exposing_settings() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("Codex.json"), br#"{"agent_name":"Codex","installed":true,"configured":false,"settings":{"secret":"never"}}"#).unwrap();
        std::fs::write(directory.path().join("bad.json"), b"not json").unwrap();
        let found = discover(Some(directory.path()));
        let codex = found.iter().find(|target| target.name == "Codex").unwrap();
        assert!(codex.installed);
        assert!(!codex.configured);
        assert!(!serde_json::to_string(&found).unwrap().contains("secret"));
        assert!(found.iter().any(|target| target.name == "openrouter"));
    }
}
