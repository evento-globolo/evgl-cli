use flags2env::BundledFlags2Env;
use futures_util::StreamExt;
use serde::Deserialize;
use std::collections::HashMap;
use tokio_tungstenite::connect_async;

#[derive(Debug, Deserialize)]
struct Config {
    #[serde(rename = "EVGL_API_URL")]
    api_url: String,
    #[serde(rename = "EVGL_TIMEOUT_SECONDS")]
    timeout_seconds: u64,
    #[serde(rename = "EVGL_OUTPUT")]
    output: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let parser = BundledFlags2Env::new();
    parser.audit_config(Some(".cli-flags.toml"))?;
    let argv = std::env::args().collect::<Vec<_>>();
    let parsed = parser.parse_structured(&argv, Some(".cli-flags.toml"))?;
    if !parsed.unknown_options.is_empty() || !parsed.errors.is_empty() {
        return Err(format!("invalid arguments: unknown={:?} errors={:?}", parsed.unknown_options, parsed.errors).into());
    }
    let mut values: HashMap<String, String> = std::env::vars().collect();
    values.extend(parsed.provided_flags);
    let config: Config = parser.coerce(&values, Some(".cli-flags.toml"))?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(config.timeout_seconds))
        .build()?;
    match parsed.command.as_str() {
        "health" => print_response(client.get(format!("{}/healthz", config.api_url.trim_end_matches('/'))).send().await?, &config.output).await?,
        "list" => print_response(client.get(format!("{}/api/v1/events", config.api_url.trim_end_matches('/'))).send().await?, &config.output).await?,
        "watch" => watch(&config.api_url).await?,
        _ => {
            eprintln!("usage: evgl-cli [--api-url URL] <health|list|watch>");
            std::process::exit(2);
        }
    }
    Ok(())
}

async fn print_response(response: reqwest::Response, output: &str) -> Result<(), Box<dyn std::error::Error>> {
    let status = response.status();
    let text = response.text().await?;
    if !status.is_success() { return Err(format!("HTTP {status}: {text}").into()); }
    if output == "json" {
        let value: serde_json::Value = serde_json::from_str(&text)?;
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else { println!("{text}"); }
    Ok(())
}

async fn watch(api_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let ws_url = api_url.replacen("http://", "ws://", 1).replacen("https://", "wss://", 1);
    let (socket, _) = connect_async(format!("{}/ws", ws_url.trim_end_matches('/'))).await?;
    let (_, mut incoming) = socket.split();
    while let Some(message) = incoming.next().await { println!("{}", message?.into_text()?); }
    Ok(())
}
