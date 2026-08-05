use clap::Parser;
#[derive(Parser)] struct Args { #[arg(long, env="SERVICE_ENDPOINT", default_value="http://localhost:8080")] endpoint: String }
fn main() { println!("{}", Args::parse().endpoint); }
