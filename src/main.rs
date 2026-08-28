use anyhow::{bail, Context, Result};
use evgl_domain::{ProviderKind, PublishTarget};
use flags2env::BundledFlags2Env;
use futures_util::StreamExt;
use reqwest::{Client, Method};
use serde_json::{json, Value};
use std::str::FromStr;
use tokio_tungstenite::{connect_async, tungstenite::client::IntoClientRequest};
use url::Url;
use uuid::Uuid;

mod env_map;

use env_map::{EnvMap, current_env_map, env_value, get_env_map, process_argv};

const HELP: &str = "evgl-cli 0.1.0

Usage: evgl-cli [options] <command>

Commands:
  health            Check the Evento Globolo API
  list              List events
  get               Read one event; requires --id
  providers         List supported providers
  connections       List provider connections
  connect-start     Start an OAuth connection
  connect-manual    Create a manual Craigslist or webhook connection
  create-event      Create a canonical event
  cross-post        Cross-post an event to a provider target
  job               Read a cross-post job
  watch             Stream a job (with --job-id) or the API event stream

Options:
  -h, --help       Print this help
  -V, --version    Print the CLI version

Configuration flags are defined in .cli-flags.toml.
EVGL_TOKEN is environment-only and has no CLI flag.
";
const VERSION: &str = concat!(env!("CARGO_PKG_NAME"), " ", env!("CARGO_PKG_VERSION"), "\n");

fn informational_output<I, S>(arguments: I) -> Option<&'static str>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    arguments.into_iter().find_map(|argument| match argument.as_ref() {
        "-h" | "--help" => Some(HELP),
        "-V" | "--version" => Some(VERSION),
        _ => None,
    })
}

fn apply_cli_flags(argv: &[String], initial: EnvMap) -> anyhow::Result<(String, EnvMap)> {
    let parser = BundledFlags2Env::new();
    parser
        .audit_config(Some(".cli-flags.toml"))
        .context("invalid flags-2-env contract")?;
    let parsed = parser
        .parse_structured(argv, Some(".cli-flags.toml"))
        .context("unable to parse CLI arguments")?;
    if !parsed.unknown_options.is_empty() || !parsed.errors.is_empty() {
        bail!(
            "invalid CLI arguments: unknown={:?}, errors={:?}",
            parsed.unknown_options,
            parsed.errors
        );
    }
    let command = parsed.command.clone();
    Ok((command, get_env_map(initial, parsed.provided_flags)))
}

fn env_or(env: &EnvMap, key: &str, default: &str) -> String {
    env_value(env, key)
        .map(str::to_owned)
        .unwrap_or_else(|| default.to_string())
}

fn required(env: &EnvMap, key: &'static str) -> Result<String> {
    env_value(env, key)
        .map(str::to_owned)
        .with_context(|| format!("missing {key}"))
}

fn json_env(env: &EnvMap, key: &'static str) -> Result<Value> {
    let value = env_or(env, key, "{}");
    serde_json::from_str(&value).with_context(|| format!("{key} must be valid JSON"))
}

fn api_base(env: &EnvMap) -> String {
    env_value(env, "EVGL_API_URL")
        .or_else(|| env_value(env, "EVGL_BASE_URL"))
        .unwrap_or("http://127.0.0.1:8080")
        .trim_end_matches('/')
        .to_string()
}

fn validate_output(output: &str) -> anyhow::Result<()> {
    match output {
        "json" | "text" => Ok(()),
        _ => bail!("EVGL_OUTPUT must be json or text"),
    }
}

fn print_json(value: Value) {
    println!("{}", serde_json::to_string_pretty(&value).unwrap());
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

struct Api {
    base: Url,
    token: Option<String>,
    http: Client,
}

impl Api {
    fn from_env_map(env: &EnvMap) -> Result<Self> {
        let timeout = env
            .get("EVGL_TIMEOUT_SECONDS")
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(20);
        Ok(Self {
            base: api_base(env).parse().context("API base URL")?,
            token: env_value(env, "EVGL_TOKEN").map(str::to_owned),
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(timeout))
                .build()?,
        })
    }

    async fn request(&self, method: Method, path: &str, body: Option<Value>) -> Result<Value> {
        let url = self.base.join(path.trim_start_matches('/'))?;
        let mut request = self.http.request(method, url);
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await?;
        let status = response.status();
        let text = response.text().await?;
        if !status.is_success() {
            bail!("API request failed ({status}): {text}");
        }
        if text.is_empty() {
            return Ok(Value::Null);
        }
        Ok(serde_json::from_str(&text).with_context(|| format!("invalid API JSON: {text}"))?)
    }
}

fn provider(env: &EnvMap) -> Result<ProviderKind> {
    ProviderKind::from_str(&required(env, "EVGL_PROVIDER")?).map_err(|error| anyhow::anyhow!(error))
}

async fn foundation(command: &str, env: &EnvMap) -> Result<()> {
    let output = env_or(env, "EVGL_OUTPUT", "json");
    validate_output(&output)?;
    let api = Api::from_env_map(env)?;
    let base = api.base.as_str().trim_end_matches('/');
    match command {
        "health" => print_response(api.http.get(format!("{base}/healthz")).send().await?, &output).await,
        "list" => {
            print_response(
                api.http.get(format!("{base}/v1/events")).send().await?,
                &output,
            )
            .await
        }
        "get" => {
            let id = env_value(env, "EVGL_ID")
                .or_else(|| env_value(env, "EVGL_EVENT_ID"))
                .context("--id is required")?;
            print_response(
                api.http.get(format!("{base}/v1/events/{id}")).send().await?,
                &output,
            )
            .await
        }
        _ => bail!("unknown foundation command {command}"),
    }
}

async fn connect_start(api: &Api, env: &EnvMap) -> Result<()> {
    let provider = provider(env)?;
    print_json(
        api.request(
            Method::POST,
            &format!("/v1/oauth/{provider}/start"),
            Some(json!({})),
        )
        .await?,
    );
    Ok(())
}

async fn connect_manual(api: &Api, env: &EnvMap) -> Result<()> {
    let provider = provider(env)?;
    if !matches!(
        provider,
        ProviderKind::Craigslist | ProviderKind::GenericWebhook
    ) {
        bail!("connect-manual is only valid for craigslist and generic_webhook");
    }
    let body = json!({
        "provider": provider,
        "account_key": required(env, "EVGL_ACCOUNT_KEY")?,
        "display_name": required(env, "EVGL_DISPLAY_NAME")?,
        "metadata": json_env(env, "EVGL_METADATA")?,
        "secret": env_value(env, "EVGL_CONNECTION_SECRET")
    });
    print_json(
        api.request(Method::POST, "/v1/connections/manual", Some(body))
            .await?,
    );
    Ok(())
}

async fn create_event(api: &Api, env: &EnvMap) -> Result<()> {
    let body = json!({
        "title": required(env, "EVGL_TITLE")?,
        "summary": required(env, "EVGL_SUMMARY")?,
        "description_html": env_or(env, "EVGL_DESCRIPTION", ""),
        "starts_at": required(env, "EVGL_STARTS_AT")?,
        "ends_at": required(env, "EVGL_ENDS_AT")?,
        "timezone": required(env, "EVGL_TIMEZONE")?,
        "canonical_url": required(env, "EVGL_CANONICAL_URL")?,
        "online_url": Value::Null,
        "venue": Value::Null,
        "tags": [],
        "metadata": {}
    });
    print_json(api.request(Method::POST, "/v1/events", Some(body)).await?);
    Ok(())
}

async fn cross_post(api: &Api, env: &EnvMap) -> Result<()> {
    let event_id: Uuid = required(env, "EVGL_EVENT_ID")?.parse()?;
    let target = PublishTarget {
        provider: provider(env)?,
        connection_id: required(env, "EVGL_CONNECTION_ID")?.parse()?,
        options: json_env(env, "EVGL_TARGET_OPTIONS")?,
    };
    let key = env_or(
        env,
        "EVGL_IDEMPOTENCY_KEY",
        &format!("cli:{event_id}:{}:{}", target.provider, target.connection_id),
    );
    let url = api.base.join(&format!("v1/events/{event_id}/cross-post"))?;
    let mut request = api.http.post(url).header("idempotency-key", key).json(&json!({ "targets": [target] }));
    if let Some(token) = &api.token {
        request = request.bearer_auth(token);
    }
    let response = request.send().await?;
    let status = response.status();
    let text = response.text().await?;
    if !status.is_success() {
        bail!("API request failed ({status}): {text}");
    }
    print_json(serde_json::from_str(&text)?);
    Ok(())
}

async fn job(api: &Api, env: &EnvMap) -> Result<()> {
    let id = required(env, "EVGL_JOB_ID")?;
    print_json(api.request(Method::GET, &format!("/v1/jobs/{id}"), None).await?);
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

async fn watch_events(env: &EnvMap) -> Result<()> {
    let (socket, _) = connect_async(websocket_url(&api_base(env))?).await?;
    let (_, mut incoming) = socket.split();
    while let Some(message) = incoming.next().await {
        println!("{}", message?.into_text()?);
    }
    Ok(())
}

async fn watch_job(env: &EnvMap) -> Result<()> {
    let api = Api::from_env_map(env)?;
    let id = required(env, "EVGL_JOB_ID")?;
    let mut url = api.base.join(&format!("v1/jobs/{id}/ws"))?;
    url.set_scheme(if url.scheme() == "https" { "wss" } else { "ws" })
        .map_err(|_| anyhow::anyhow!("could not construct WebSocket URL"))?;
    let mut request = url.as_str().into_client_request()?;
    if let Some(token) = &api.token {
        request.headers_mut().insert(
            "authorization",
            format!("Bearer {token}").parse()?,
        );
    }
    let (stream, _) = connect_async(request).await?;
    let (_, mut read) = stream.split();
    while let Some(message) = read.next().await {
        let message = message?;
        if message.is_text() {
            println!("{}", message.into_text()?);
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let argv = process_argv();
    if let Some(output) = informational_output(argv.iter().skip(1)) {
        print!("{output}");
        return Ok(());
    }

    let (command, env) = apply_cli_flags(&argv, current_env_map())?;
    match command.as_str() {
        "health" | "list" | "get" => foundation(&command, &env).await,
        "providers" => {
            let api = Api::from_env_map(&env)?;
            print_json(api.request(Method::GET, "/v1/providers", None).await?);
            Ok(())
        }
        "connections" => {
            let api = Api::from_env_map(&env)?;
            print_json(api.request(Method::GET, "/v1/connections", None).await?);
            Ok(())
        }
        "connect-start" => connect_start(&Api::from_env_map(&env)?, &env).await,
        "connect-manual" => connect_manual(&Api::from_env_map(&env)?, &env).await,
        "create-event" => create_event(&Api::from_env_map(&env)?, &env).await,
        "cross-post" => cross_post(&Api::from_env_map(&env)?, &env).await,
        "job" => job(&Api::from_env_map(&env)?, &env).await,
        "watch" => {
            if env_value(&env, "EVGL_JOB_ID").is_some() {
                watch_job(&env).await
            } else {
                watch_events(&env).await
            }
        }
        _ => bail!(
            "choose one command: health, list, get, providers, connections, connect-start, connect-manual, create-event, cross-post, job, watch"
        ),
    }
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
    fn apply_cli_flags_merges_cli_over_base_env_without_mutation() {
        let before = std::env::var_os("EVGL_OUTPUT");
        let initial = EnvMap::from([("EVGL_OUTPUT".into(), "text".into())]);
        let argv = vec![
            "evgl-cli".into(),
            "health".into(),
            "--output".into(),
            "json".into(),
        ];
        let (command, env) = apply_cli_flags(&argv, initial).unwrap();
        assert_eq!(command, "health");
        assert_eq!(env.get("EVGL_OUTPUT").map(String::as_str), Some("json"));
        assert_eq!(std::env::var_os("EVGL_OUTPUT"), before);
    }

    #[test]
    fn apply_cli_flags_parse_failure_does_not_mutate_process_environment() {
        let before = std::env::var_os("EVGL_OUTPUT");
        let initial = EnvMap::from([("EVGL_OUTPUT".into(), "text".into())]);
        let argv = vec![
            "evgl-cli".into(),
            "health".into(),
            "--this-flag-is-not-declared".into(),
        ];
        assert!(apply_cli_flags(&argv, initial).is_err());
        assert_eq!(std::env::var_os("EVGL_OUTPUT"), before);
    }

    #[test]
    fn source_does_not_mutate_process_environment() {
        const SRC: &str = include_str!("main.rs");
        let production = SRC.split("#[cfg(test)]").next().unwrap_or(SRC);
        assert!(!production.contains("std::env::set_var"));
        assert!(!production.contains("env::set_var"));
    }

    #[test]
    fn api_base_prefers_recovered_api_url_over_foundation_base_url() {
        let env = EnvMap::from([
            ("EVGL_BASE_URL".into(), "http://127.0.0.1:8080".into()),
            ("EVGL_API_URL".into(), "http://localhost:9090".into()),
        ]);
        assert_eq!(api_base(&env), "http://localhost:9090");
    }
}
