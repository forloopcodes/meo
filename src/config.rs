// Config file management and interactive login for API credentials
// Stores API key, base URL, and model in a JSON config file

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

impl Config {
    fn path() -> PathBuf {
        if cfg!(windows) {
            let dir = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
            PathBuf::from(dir).join("meo").join("config.json")
        } else {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join(".config").join("meo").join("config.json")
        }
    }

    pub fn load() -> Option<Config> {
        let path = Self::path();
        if !path.exists() { return None; }
        std::fs::read_to_string(&path).ok().and_then(|s| serde_json::from_str(&s).ok())
    }

    fn save(&self) -> Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
        std::fs::write(&path, serde_json::to_string_pretty(self)?)?;
        println!("Saved to {}", path.display());
        Ok(())
    }

    pub fn delete() {
        let path = Self::path();
        if path.exists() {
            std::fs::remove_file(&path).ok();
            println!("Config removed");
        }
    }

    pub async fn interactive_login(client: &reqwest::Client) -> Result<Config> {
        println!("\nConnect to an OpenAI-compatible provider.");
        println!("Get a key from https://opencode.ai/zen or use your own.\n");

        let api_key = read_input("API key (sk-...)", None)?;
        let base_url = read_input("Base URL", Some("https://opencode.ai/zen"))?;
        let model = read_input("Model", Some("gpt-4o-mini"))?;

        print!("Testing... ");
        std::io::stdout().flush()?;
        let test_url = format!("{}/v1/models", base_url.trim_end_matches('/'));
        match client.get(&test_url).header("Authorization", format!("Bearer {}", api_key)).send().await {
            Ok(r) if r.status().is_success() => println!("OK"),
            _ => println!("unreachable (saved anyway)"),
        }

        let config = Config { api_key, base_url, model };
        config.save()?;
        Ok(config)
    }
}

fn read_input(prompt: &str, default: Option<&str>) -> Result<String> {
    use std::io::Write;
    let mut stdout = std::io::stdout();
    match default {
        Some(d) => print!("{} [{}]: ", prompt, d),
        None => print!("{}: ", prompt),
    }
    stdout.flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let input = input.trim().to_string();
    if input.is_empty() {
        default.map(|d| d.to_string()).context("Input required")
    } else {
        Ok(input)
    }
}
