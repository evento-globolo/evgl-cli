use anyhow::{Context, bail};
use flags2env::BundledFlags2Env;
use futures_util::StreamExt;
use std::collections::BTreeMap;
use tokio_tungstenite::connect_async;

type EnvMap = BTreeMap<String, String>;

const HELP: &str = "evgl-cli 0.1.0\n\nUsage: evgl-cli [options] <command>\n\nCommands:\n  health  Check the Evento Globolo API\n  list    List events\n  get     Read one event; requires --id\n  watch   Stream event updates\n\nOptions:\n  -h, --help       Print this help\n  -V, --version    Print the CLI version\n\nConfiguration flags are defined in .cli-flags.toml.\n";
const VERSION: &str = concat!(env!("CARGO_PKG_NAME"), " ", env!("CARGO_PKG_VERSION"), "\n");

fn informational_output<I, S>(arguments: I) -> Option<&'static str>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    arguments
        .into_iter()
        .find_map(|argument| match argument.as_ref() {
            "-h" | "--help" => Some(HELP),
            "-V" | "--version" => Some(VERSION),
            _ => None,
        })
}

fn merge_env(mut initial: EnvMap, overrides: impl IntoIterator<Item = (String, String)>) -> EnvMap {
    initial.extend(overrides);
    initial
}

fn env_value<'a>(env: &'a EnvMap, key: &str) -> Option<&'a str> {
    env.get(key)
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn apply_flags(argv: &[String], initial: EnvMap) -> anyhow::Result<(String, EnvMap)> {
    let parser = BundledFlags2Env::new();
    parser
        .audit_config(Some(".cli-flags.toml"))
        .map_err(|error| anyhow::anyhow!("invalid .cli-flags.toml: {error}"))?;
    let parsed = parser
        .parse_structured(argv, Some(".cli-flags.toml"))
        .map_err(|error| anyhow::anyhow!("could not parse CLI arguments: {error}"))?;
    if !parsed.unknown_options.is_empty() || !parsed.errors.is_empty() {
        bail!(
            "invalid CLI arguments: unknown={:?}, errors={:?}",
            parsed.unknown_options,
            parsed.errors
        );
    }
    let command = parsed.command.clone();
    Ok((command, merge_env(initial, parsed.provided_flags)))
}

fn env_or(env: &EnvMap, key: &str, default: &str) -> String {
    env_value(env, key)
        .map(str::to_owned)
        .unwrap_or_else(|| default.to_string())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let argv = std::env::args().collect::<Vec<_>>();
    if let Some(output) = informational_output(argv.iter().skip(1)) {
        print!("{output}");
        return Ok(());
    }

    let initial = std::env::vars().collect::<EnvMap>();
    let (command, env) = apply_flags(&argv, initial)?;
    let base = env_or(&env, "EVGL_BASE_URL", "http://127.0.0.1:8080");
    let timeout = env
        .get("EVGL_TIMEOUT_SECONDS")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(20);
    let output = env_or(&env, "EVGL_OUTPUT", "json");
    validate_output(&output)?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout))
        .build()?;
    let base = base.trim_end_matches('/');

    match command.as_str() {
        "health" => {
            print_response(client.get(format!("{base}/healthz")).send().await?, &output).await
        }
        "list" => {
            print_response(
                client.get(format!("{base}/v1/events")).send().await?,
                &output,
            )
            .await
        }
        "get" => {
            let id = env.get("EVGL_ID").context("--id is required")?;
            print_response(
                client.get(format!("{base}/v1/events/{id}")).send().await?,
                &output,
            )
            .await
        }
        "watch" => watch(base).await,
        _ => bail!("choose one command: health, list, get, watch"),
    }
}

fn validate_output(output: &str) -> anyhow::Result<()> {
    match output {
        "json" | "text" => Ok(()),
        _ => bail!("EVGL_OUTPUT must be json or text"),
    }
}

async fn print_response(response: reqwest::Response, output: &str) -> anyhow::Result<()> {
    let status = response.status();
    let text = response.text().await?;
    if !status.is_success() {
        bail!("HTTP {status}: {text}");
    }
    if output == "json" {
        let value: serde_json::Value = serde_json::from_str(&text)?;
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!("{text}");
    }
    Ok(())
}

fn websocket_url(base: &str) -> anyhow::Result<String> {
    let base = base.trim_end_matches('/');
    if let Some(rest) = base.strip_prefix("http://") {
        return Ok(format!("ws://{rest}/v1/ws"));
    }
    if let Some(rest) = base.strip_prefix("https://") {
        return Ok(format!("wss://{rest}/v1/ws"));
    }
    bail!("EVGL_BASE_URL must start with http:// or https://")
}

async fn watch(base: &str) -> anyhow::Result<()> {
    let (socket, _) = connect_async(websocket_url(base)?).await?;
    let (_, mut incoming) = socket.split();
    while let Some(message) = incoming.next().await {
        println!("{}", message?.into_text()?);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_and_version_are_available_without_configuration_or_network() {
        assert_eq!(informational_output(["--help"]), Some(HELP));
        assert_eq!(informational_output(["-h"]), Some(HELP));
        assert_eq!(informational_output(["--version"]), Some(VERSION));
        assert_eq!(informational_output(["health"]), None);
    }

    #[test]
    fn websocket_url_maps_http_schemes_and_normalizes_trailing_slashes() {
        assert_eq!(
            websocket_url("http://127.0.0.1:8080/").unwrap(),
            "ws://127.0.0.1:8080/v1/ws"
        );
        assert_eq!(
            websocket_url("https://events.example.com").unwrap(),
            "wss://events.example.com/v1/ws"
        );
    }

    #[test]
    fn websocket_url_rejects_unexpected_schemes() {
        let error = websocket_url("file:///tmp/socket").unwrap_err().to_string();
        assert!(error.contains("http:// or https://"));
    }

    #[test]
    fn output_mode_is_explicit() {
        assert!(validate_output("json").is_ok());
        assert!(validate_output("text").is_ok());
        assert!(validate_output("yaml").is_err());
    }

    #[test]
    fn cli_overrides_win_without_mutating_process_environment() {
        let before = std::env::var_os("EVGL_OUTPUT");
        let env = merge_env(
            EnvMap::from([("EVGL_OUTPUT".into(), "text".into())]),
            [("EVGL_OUTPUT".into(), "json".into())],
        );
        assert_eq!(env.get("EVGL_OUTPUT").map(String::as_str), Some("json"));
        assert_eq!(env_or(&env, "EVGL_OUTPUT", "text"), "json");
        assert_eq!(std::env::var_os("EVGL_OUTPUT"), before);
    }

    #[test]
    fn empty_and_whitespace_env_values_are_absent() {
        for raw in ["", " ", "\t"] {
            let env = EnvMap::from([("EVGL_OUTPUT".into(), raw.into())]);
            assert_eq!(env_value(&env, "EVGL_OUTPUT"), None, "raw={raw:?}");
            assert_eq!(env_or(&env, "EVGL_OUTPUT", "json"), "json");
        }
    }

    #[test]
    fn apply_flags_merges_cli_over_base_env_without_mutation() {
        let before = std::env::var_os("EVGL_OUTPUT");
        let initial = EnvMap::from([("EVGL_OUTPUT".into(), "text".into())]);
        let argv = vec![
            "evgl-cli".into(),
            "health".into(),
            "--output".into(),
            "json".into(),
        ];
        let (command, env) = apply_flags(&argv, initial).unwrap();
        assert_eq!(command, "health");
        assert_eq!(env.get("EVGL_OUTPUT").map(String::as_str), Some("json"));
        assert_eq!(std::env::var_os("EVGL_OUTPUT"), before);
    }

    #[test]
    fn apply_flags_parse_failure_does_not_mutate_process_environment() {
        let before = std::env::var_os("EVGL_OUTPUT");
        let initial = EnvMap::from([("EVGL_OUTPUT".into(), "text".into())]);
        let argv = vec![
            "evgl-cli".into(),
            "health".into(),
            "--this-flag-is-not-declared".into(),
        ];
        assert!(apply_flags(&argv, initial).is_err());
        assert_eq!(std::env::var_os("EVGL_OUTPUT"), before);
    }

    #[test]
    fn source_does_not_mutate_process_environment() {
        const SRC: &str = include_str!("main.rs");
        let production = SRC.split("#[cfg(test)]").next().unwrap_or(SRC);
        assert!(!production.contains("std::env::set_var"));
        assert!(!production.contains("env::set_var"));
    }
}
