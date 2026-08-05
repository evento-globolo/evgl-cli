use anyhow::{Context, bail};
use flags2env::BundledFlags2Env;
use futures_util::StreamExt;
use tokio_tungstenite::connect_async;

fn apply_flags() -> anyhow::Result<String> {
    let parser = BundledFlags2Env::new();
    parser.audit_config(Some(".cli-flags.toml"))?;
    let argv = std::env::args().collect::<Vec<_>>();
    let parsed = parser.parse_structured(&argv, Some(".cli-flags.toml"))?;
    if !parsed.unknown_options.is_empty() || !parsed.errors.is_empty() {
        bail!(
            "invalid CLI arguments: unknown={:?}, errors={:?}",
            parsed.unknown_options,
            parsed.errors
        );
    }
    let command = parsed.command.clone();
    for (key, value) in parsed.provided_flags {
        unsafe { std::env::set_var(key, value) };
    }
    Ok(command)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let command = apply_flags()?;
    let base = std::env::var("EVGL_BASE_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8080".into());
    let timeout = std::env::var("EVGL_TIMEOUT_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(20);
    let output = std::env::var("EVGL_OUTPUT").unwrap_or_else(|_| "json".into());
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout))
        .build()?;
    let base = base.trim_end_matches('/');

    match command.as_str() {
        "health" => print_response(client.get(format!("{base}/healthz")).send().await?, &output).await,
        "list" => print_response(client.get(format!("{base}/v1/events")).send().await?, &output).await,
        "get" => {
            let id = std::env::var("EVGL_ID").context("--id is required")?;
            print_response(client.get(format!("{base}/v1/events/{id}")).send().await?, &output).await
        }
        "watch" => watch(base).await,
        _ => bail!("choose one command: health, list, get, watch"),
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

async fn watch(base: &str) -> anyhow::Result<()> {
    let ws_base = base
        .replacen("http://", "ws://", 1)
        .replacen("https://", "wss://", 1);
    let (socket, _) = connect_async(format!("{ws_base}/v1/ws")).await?;
    let (_, mut incoming) = socket.split();
    while let Some(message) = incoming.next().await {
        println!("{}", message?.into_text()?);
    }
    Ok(())
}
