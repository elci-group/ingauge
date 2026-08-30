// Copyright (c) 2026 sal
// SPDX-License-Identifier: MIT
//! Interactive provider configuration without persisting keys in main config.

use crate::{
    config::{write_private, Config, CredentialSource, ProviderConfig},
    providers::ProviderError,
};
use serde::Serialize;
use std::{
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(Clone, Copy)]
struct ProviderPreset {
    name: &'static str,
    env: &'static str,
    endpoint: &'static str,
    usage_path: &'static str,
}

const PRESETS: &[ProviderPreset] = &[
    ProviderPreset {
        name: "openai",
        env: "OPENAI_API_KEY",
        endpoint: "http://127.0.0.1:3000",
        usage_path: "/usage",
    },
    ProviderPreset {
        name: "anthropic",
        env: "ANTHROPIC_API_KEY",
        endpoint: "http://127.0.0.1:3000",
        usage_path: "/usage",
    },
    ProviderPreset {
        name: "groq",
        env: "GROQ_API_KEY",
        endpoint: "https://api.groq.com",
        usage_path: "/openai/v1/models",
    },
    ProviderPreset {
        name: "gemini",
        env: "GEMINI_API_KEY",
        endpoint: "http://127.0.0.1:3000",
        usage_path: "/usage",
    },
    ProviderPreset {
        name: "openrouter",
        env: "OPENROUTER_API_KEY",
        endpoint: "http://127.0.0.1:3000",
        usage_path: "/usage",
    },
];

#[derive(Debug, Serialize)]
pub struct SetupResult {
    pub provider: String,
    pub credential_source: CredentialSource,
    pub credential_env: String,
    pub config_path: PathBuf,
    pub credential_detected: bool,
}

pub fn run(config: &mut Config, config_path: &Path) -> Result<SetupResult, ProviderError> {
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let stdout = io::stdout();
    let mut output = stdout.lock();
    render_menu(&mut output)?;

    let preset_choice = prompt(&mut input, &mut output, "Provider [1-6]")?;
    let preset = preset_choice
        .parse::<usize>()
        .ok()
        .and_then(|choice| choice.checked_sub(1))
        .and_then(|index| PRESETS.get(index).copied());
    let (provider, default_env, default_endpoint, default_usage_path) = match preset {
        Some(preset) => (
            preset.name.to_owned(),
            preset.env.to_owned(),
            preset.endpoint.to_owned(),
            preset.usage_path.to_owned(),
        ),
        None if preset_choice.trim() == "6" => (
            prompt(&mut input, &mut output, "Provider id")?,
            prompt(&mut input, &mut output, "API key environment variable")?,
            "http://127.0.0.1:3000".to_owned(),
            "/usage".to_owned(),
        ),
        None => {
            return Err(ProviderError::Configuration(
                "choose a provider from 1 to 6".into(),
            ))
        }
    };
    let endpoint = prompt_default(
        &mut input,
        &mut output,
        "Telemetry endpoint",
        &default_endpoint,
    )?;
    let usage_path = prompt_default(&mut input, &mut output, "Usage path", &default_usage_path)?;
    writeln!(output, "│  CREDENTIAL SOURCE").map_err(io_error)?;
    writeln!(output, "│  1  .bashrc            read export VARIABLE=...").map_err(io_error)?;
    writeln!(
        output,
        "│  2  .env               read a selected dotenv file"
    )
    .map_err(io_error)?;
    writeln!(
        output,
        "│  3  manual_key_entry   masked input → private vault"
    )
    .map_err(io_error)?;
    let source = match prompt(&mut input, &mut output, "Source [1-3]")?.trim() {
        "1" => CredentialSource::Bashrc,
        "2" => CredentialSource::Dotenv,
        "3" => CredentialSource::ManualKeyEntry,
        _ => {
            return Err(ProviderError::Configuration(
                "choose a credential source from 1 to 3".into(),
            ))
        }
    };
    let variable = prompt_default(
        &mut input,
        &mut output,
        "Environment variable",
        &default_env,
    )?;
    let credential_file = match source {
        CredentialSource::Bashrc => {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".bashrc"))
        }
        CredentialSource::Dotenv => {
            let default = config_path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(".env");
            Some(PathBuf::from(prompt_default(
                &mut input,
                &mut output,
                ".env path",
                &default.to_string_lossy(),
            )?))
        }
        CredentialSource::ManualKeyEntry => {
            let vault = private_vault_path(config_path);
            if let Some(parent) = vault.parent() {
                std::fs::create_dir_all(parent).map_err(io_error)?;
            }
            let key = read_secret(&mut input, &mut output, "API key (input hidden)")?;
            if key.is_empty() {
                return Err(ProviderError::Configuration(
                    "API key cannot be empty".into(),
                ));
            }
            upsert_secret(&vault, &variable, &key)?;
            Some(vault)
        }
    };
    let provider_config = ProviderConfig {
        enabled: Some(true),
        endpoint: Some(endpoint),
        api_key_env: Some(variable.clone()),
        credential_source: Some(source),
        credential_file: credential_file.clone(),
        usage_path,
    };
    let detected = provider_config.resolve_api_key().is_ok();
    config.providers.insert(provider.clone(), provider_config);
    config.validate()?;
    config.save(config_path)?;
    writeln!(
        output,
        "╰─ Saved {} · key {}",
        config_path.display(),
        if detected {
            "detected ✓"
        } else {
            "not detected"
        }
    )
    .map_err(io_error)?;
    Ok(SetupResult {
        provider,
        credential_source: source,
        credential_env: variable,
        config_path: config_path.to_path_buf(),
        credential_detected: detected,
    })
}

fn private_vault_path(config_path: &Path) -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .map(|directory| directory.join("ingauge/secrets.env"))
        .unwrap_or_else(|| {
            config_path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(".ingauge-secrets.env")
        })
}

fn render_menu(output: &mut impl Write) -> Result<(), ProviderError> {
    writeln!(
        output,
        "╭≈╱≈╲≈╱≈╲≈ INGAUGE GARAGE · PROVIDER SETUP ≈╱≈╲≈╱≈╲≈╮"
    )
    .map_err(io_error)?;
    writeln!(output, "│  1  OpenAI       2  Anthropic     3  Groq").map_err(io_error)?;
    writeln!(
        output,
        "│  4  Gemini       5  OpenRouter    6  Custom bridge"
    )
    .map_err(io_error)?;
    Ok(())
}

fn prompt(
    input: &mut impl BufRead,
    output: &mut impl Write,
    label: &str,
) -> Result<String, ProviderError> {
    write!(output, "│  {label}: ").map_err(io_error)?;
    output.flush().map_err(io_error)?;
    let mut answer = String::new();
    input.read_line(&mut answer).map_err(io_error)?;
    Ok(answer.trim().to_owned())
}

fn prompt_default(
    input: &mut impl BufRead,
    output: &mut impl Write,
    label: &str,
    default: &str,
) -> Result<String, ProviderError> {
    let value = prompt(input, output, &format!("{label} [{default}]"))?;
    Ok(if value.is_empty() {
        default.to_owned()
    } else {
        value
    })
}

fn read_secret(
    input: &mut impl BufRead,
    output: &mut impl Write,
    label: &str,
) -> Result<String, ProviderError> {
    let terminal = Command::new("stty")
        .arg("-echo")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    let result = prompt(input, output, label);
    if terminal {
        let _ = Command::new("stty")
            .arg("echo")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        writeln!(output).map_err(io_error)?;
    }
    result
}

fn upsert_secret(path: &Path, variable: &str, key: &str) -> Result<(), ProviderError> {
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    let mut lines = existing
        .lines()
        .filter(|line| {
            line.trim()
                .strip_prefix("export ")
                .unwrap_or(line.trim())
                .split_once('=')
                .is_none_or(|(name, _)| name.trim() != variable)
        })
        .map(str::to_owned)
        .collect::<Vec<_>>();
    lines.push(format!("{variable}={key}"));
    write_private(path, format!("{}\n", lines.join("\n")).as_bytes())
}

fn io_error(error: io::Error) -> ProviderError {
    ProviderError::Configuration(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_vault_preserves_other_keys_and_replaces_selected_key() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let vault = directory.path().join("secrets.env");
        upsert_secret(&vault, "FIRST_KEY", "one").expect("write first key");
        upsert_secret(&vault, "SECOND_KEY", "two").expect("write second key");
        upsert_secret(&vault, "FIRST_KEY", "updated").expect("replace first key");
        let contents = std::fs::read_to_string(&vault).expect("read vault");
        assert!(contents.contains("FIRST_KEY=updated"));
        assert!(contents.contains("SECOND_KEY=two"));
        assert!(!contents.contains("FIRST_KEY=one"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(vault)
                .expect("vault metadata")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }
}
