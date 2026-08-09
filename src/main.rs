use anyhow::{bail, Context, Result};
use evgl_domain::{ProviderKind, PublishTarget};
use flags2env::BundledFlags2Env;
use futures_util::StreamExt;
use reqwest::{Client, Method};
use serde_json::{json, Value};
use std::{env, str::FromStr};
use tokio_tungstenite::{connect_async, tungstenite::client::IntoClientRequest};
use url::Url;
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<()> {
    apply_flags()?;
    let command = env::args().nth(1).unwrap_or_else(|| "help".into());
    if matches!(command.as_str(), "help" | "--help" | "-h") {
        help();
        return Ok(());
    }
    let api = Api::from_env()?;
    match command.as_str() {
        "providers" => print_json(api.request(Method::GET, "/v1/providers", None).await?),
        "connections" => print_json(api.request(Method::GET, "/v1/connections", None).await?),
        "connect-start" => connect_start(&api).await?,
        "connect-manual" => connect_manual(&api).await?,
        "create-event" => create_event(&api).await?,
        "cross-post" => cross_post(&api).await?,
        "job" => job(&api).await?,
        "watch" => watch(&api).await?,
        other => bail!("unknown command {other}; run `evgl-cli help`"),
    }
    Ok(())
}

fn apply_flags() -> Result<()> {
    let parser = BundledFlags2Env::new();
    parser.audit_config(Some(".cli-flags.toml"))?;
    let argv = env::args().collect::<Vec<_>>();
    let parsed = parser.parse_structured(&argv, Some(".cli-flags.toml"))?;
    if !parsed.unknown_options.is_empty() || !parsed.errors.is_empty() {
        bail!(
            "invalid CLI arguments: unknown={:?}, errors={:?}",
            parsed.unknown_options, parsed.errors
        );
    }
    for (key, value) in parsed.provided_flags {
        unsafe { env::set_var(key, value) };
    }
    Ok(())
}

struct Api {
    base: Url,
    token: String,
    http: Client,
}

impl Api {
    fn from_env() -> Result<Self> {
        Ok(Self {
            base: required("EVGL_API_URL")?.parse().context("EVGL_API_URL")?,
            token: required("EVGL_TOKEN")
                .context("EVGL_TOKEN must be provided through the environment")?,
            http: Client::new(),
        })
    }

    async fn request(&self, method: Method, path: &str, body: Option<Value>) -> Result<Value> {
        let url = self.base.join(path.trim_start_matches('/'))?;
        let mut request = self.http.request(method, url).bearer_auth(&self.token);
        if let Some(body) = body { request = request.json(&body); }
        let response = request.send().await?;
        let status = response.status();
        let text = response.text().await?;
        if !status.is_success() {
            bail!("API request failed ({status}): {text}");
        }
        if text.is_empty() { return Ok(Value::Null); }
        Ok(serde_json::from_str(&text).with_context(|| format!("invalid API JSON: {text}"))?)
    }
}

async fn connect_start(api: &Api) -> Result<()> {
    let provider = provider()?;
    print_json(api.request(
        Method::POST,
        &format!("/v1/oauth/{provider}/start"),
        Some(json!({})),
    ).await?);
    Ok(())
}

async fn connect_manual(api: &Api) -> Result<()> {
    let provider = provider()?;
    if !matches!(provider, ProviderKind::Craigslist | ProviderKind::GenericWebhook) {
        bail!("connect-manual is only valid for craigslist and generic_webhook");
    }
    let body = json!({
        "provider": provider,
        "account_key": required("EVGL_ACCOUNT_KEY")?,
        "display_name": required("EVGL_DISPLAY_NAME")?,
        "metadata": json_env("EVGL_METADATA")?,
        "secret": env::var("EVGL_CONNECTION_SECRET").ok()
    });
    print_json(api.request(Method::POST, "/v1/connections/manual", Some(body)).await?);
    Ok(())
}

async fn create_event(api: &Api) -> Result<()> {
    let body = json!({
        "title": required("EVGL_TITLE")?,
        "summary": required("EVGL_SUMMARY")?,
        "description_html": env::var("EVGL_DESCRIPTION").unwrap_or_default(),
        "starts_at": required("EVGL_STARTS_AT")?,
        "ends_at": required("EVGL_ENDS_AT")?,
        "timezone": required("EVGL_TIMEZONE")?,
        "canonical_url": required("EVGL_CANONICAL_URL")?,
        "online_url": Value::Null,
        "venue": Value::Null,
        "tags": [],
        "metadata": {}
    });
    print_json(api.request(Method::POST, "/v1/events", Some(body)).await?);
    Ok(())
}

async fn cross_post(api: &Api) -> Result<()> {
    let event_id: Uuid = required("EVGL_EVENT_ID")?.parse()?;
    let target = PublishTarget {
        provider: provider()?,
        connection_id: required("EVGL_CONNECTION_ID")?.parse()?,
        options: json_env("EVGL_TARGET_OPTIONS")?,
    };
    let key = env::var("EVGL_IDEMPOTENCY_KEY")
        .unwrap_or_else(|_| format!("cli:{event_id}:{}:{}", target.provider, target.connection_id));
    let url = api.base.join(&format!("v1/events/{event_id}/cross-post"))?;
    let response = api.http.post(url)
        .bearer_auth(&api.token)
        .header("idempotency-key", key)
        .json(&json!({ "targets": [target] }))
        .send().await?;
    let status = response.status();
    let text = response.text().await?;
    if !status.is_success() { bail!("API request failed ({status}): {text}"); }
    print_json(serde_json::from_str(&text)?);
    Ok(())
}

async fn job(api: &Api) -> Result<()> {
    let id = required("EVGL_JOB_ID")?;
    print_json(api.request(Method::GET, &format!("/v1/jobs/{id}"), None).await?);
    Ok(())
}

async fn watch(api: &Api) -> Result<()> {
    let id = required("EVGL_JOB_ID")?;
    let mut url = api.base.join(&format!("v1/jobs/{id}/ws"))?;
    url.set_scheme(if url.scheme() == "https" { "wss" } else { "ws" })
        .map_err(|_| anyhow::anyhow!("could not construct WebSocket URL"))?;
    let mut request = url.as_str().into_client_request()?;
    request.headers_mut().insert(
        "authorization",
        format!("Bearer {}", api.token).parse()?,
    );
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

fn provider() -> Result<ProviderKind> {
    ProviderKind::from_str(&required("EVGL_PROVIDER")?)
        .map_err(|error| anyhow::anyhow!(error))
}

fn json_env(key: &'static str) -> Result<Value> {
    let value = env::var(key).unwrap_or_else(|_| "{}".into());
    serde_json::from_str(&value).with_context(|| format!("{key} must be valid JSON"))
}

fn required(key: &'static str) -> Result<String> {
    env::var(key).with_context(|| format!("missing {key}"))
}

fn print_json(value: Value) {
    println!("{}", serde_json::to_string_pretty(&value).unwrap());
}

fn help() {
    println!(
        "evgl-cli <providers|connections|connect-start|connect-manual|create-event|cross-post|job|watch> [flags]"
    );
}
