// Meo - minimal coding agent CLI
// Interactive session with /exit, /help, /clear, /login, /logout, /context commands

use clap::Parser;
use anyhow::Result;
use crate::types::Message;

mod agent;
mod config;
mod tools;
mod llm;
mod types;

#[derive(Parser)]
#[command(name = "meo", about = "Minimal coding agent", version)]
struct Cli {
    #[arg(trailing_var_arg = true, hide = true)]
    task: Vec<String>,
}

fn resolve() -> (Option<String>, String, String) {
    let cfg = config::Config::load();
    let key = std::env::var("OPENAI_API_KEY").ok()
        .or_else(|| cfg.as_ref().and_then(|c| Some(c.api_key.clone())));
    let url = std::env::var("OPENAI_BASE_URL").ok()
        .or_else(|| cfg.as_ref().map(|c| c.base_url.clone()))
        .unwrap_or_else(|| "https://api.openai.com".into());
    let model = std::env::var("MODEL_NAME").ok()
        .or_else(|| cfg.as_ref().map(|c| c.model.clone()))
        .unwrap_or_else(|| "gpt-4o-mini".into());
    (key, url, model)
}

fn show_context(history: &[Message], cache: &tools::CacheStats) {
    let total_chars: usize = history.iter().filter_map(|m| m.content.as_ref()).map(|c| c.len()).sum();
    println!("\n── context ──");
    println!("Messages: {}  Chars: ~{}  Est. tokens: ~{}", history.len(), total_chars, total_chars / 4);
    for (i, m) in history.iter().enumerate() {
        let role = &m.role;
        let is_tc = m.tool_calls.is_some();
        let clen = m.content.as_ref().map(|c| c.len()).unwrap_or(0);
        let preview = m.content.as_ref().and_then(|c| {
            let first = c.lines().next()?;
            if first.len() > 48 { Some(format!("{}...", &first[..48])) } else { Some(first.to_string()) }
        }).unwrap_or_default();
        let tc_info = if is_tc { format!(" [{} calls]", m.tool_calls.as_ref().map(|v| v.len()).unwrap_or(0)) } else { String::new() };
        println!("  #{:<2} {:<10} {:>5} chars (~{:<4} tokens){}  {}", i, role, clen, clen / 4, tc_info, preview);
    }
    println!("  ─────────────────────────────────────────────");
    println!("  Turn cache: {} hits, {} misses ({:.0}% hit rate)", cache.hits, cache.misses, cache.hit_rate() * 100.0);
    println!("  Provider cache: automatic (same prefix reused across API calls)");
    println!();
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let client = reqwest::Client::new();
    let (mut api_key, mut base_url, mut model) = resolve();

    let task = cli.task.join(" ");
    if !task.is_empty() {
        let key = api_key.as_ref().expect("No API key. Set OPENAI_API_KEY or run /login");
        let mut history = Vec::new();
        let mut cache = tools::ToolCache::new();
        agent::run(&model, key, &base_url, &task, &mut history, &mut cache, true).await?;
    } else {
        println!("Meo - Minimal coding agent");
        if api_key.is_none() { println!("No API key configured. Type /login to set one."); }
        println!("Commands: /exit /help /clear /login /logout /context");
        let mut stdout = std::io::stdout();
        let mut session: Vec<Message> = Vec::new();
        let mut session_cache = tools::ToolCache::new();
        let mut cache_acc = tools::CacheStats::default();
        loop {
            use std::io::Write;
            print!("> ");
            stdout.flush()?;
            let mut input = String::new();
            if std::io::stdin().read_line(&mut input)? == 0 { break; }
            match input.trim() {
                "/exit" => break,
                "/help" => println!("Enter tasks in natural language.\nCommands: /exit /help /clear /login /logout /context"),
                "/clear" => { session.clear(); session_cache = tools::ToolCache::new(); cache_acc = tools::CacheStats::default(); print!("\x1B[2J\x1B[1;1H"); },
                "/context" => show_context(&session, &cache_acc),
                "/login" => {
                    config::Config::interactive_login(&client).await?;
                    let c = resolve();
                    api_key = c.0; base_url = c.1; model = c.2;
                },
                "/logout" => config::Config::delete(),
                s if s.is_empty() => continue,
                s => {
                    let key = api_key.as_ref().ok_or_else(|| anyhow::anyhow!("No API key. Set OPENAI_API_KEY or run /login"))?;
                    let stats = agent::run(&model, key, &base_url, s, &mut session, &mut session_cache, false).await?;
                    cache_acc.hits += stats.hits;
                    cache_acc.misses += stats.misses;
                },
            }
        }
    }
    Ok(())
}
