// Tool definitions and execution for file I/O, search, and shell
// Each tool maps to a short name used by the LLM via function calling

use std::collections::HashMap;
use anyhow::{Context, Result};
use serde_json::Value;
use std::path::Path;
use tokio::process::Command;
use crate::types::{ToolDefinition, ToolFunction};

#[derive(Default, Clone, Debug)]
pub struct CacheStats {
    pub hits: usize,
    pub misses: usize,
}

impl CacheStats {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 { 0.0 } else { self.hits as f64 / total as f64 }
    }
}

pub struct ToolCache {
    map: HashMap<(String, String), String>,
    stats: CacheStats,
}

impl ToolCache {
    pub fn new() -> Self {
        Self { map: HashMap::new(), stats: CacheStats::default() }
    }

    pub fn stats(&self) -> CacheStats { self.stats.clone() }

    pub fn contains(&self, name: &str, args: &Value) -> Option<&String> {
        self.map.get(&(name.to_string(), args.to_string()))
    }

    pub fn hit(&mut self) { self.stats.hits += 1; }
    pub fn miss(&mut self) { self.stats.misses += 1; }

    pub fn insert(&mut self, name: &str, args: &Value, result: String) {
        self.map.insert((name.to_string(), args.to_string()), result);
    }
}

pub(crate) async fn raw(name: &str, args: &Value) -> Result<String> {
    match name {
        "read" => read(args).await,
        "write" => write(args).await,
        "find" => find(args).await,
        "sh" => sh(args).await,
        _ => Err(anyhow::anyhow!("Unknown tool: {name}")),
    }
}

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            type_: "function".into(),
            function: ToolFunction {
                name: "read".into(),
                description: "Read file with optional line range".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string"},
                        "start": {"type": "integer", "description": "default 0"},
                        "end": {"type": "integer", "description": "default 200"}
                    },
                    "required": ["path"]
                }),
            },
        },
        ToolDefinition {
            type_: "function".into(),
            function: ToolFunction {
                name: "write".into(),
                description: "Write file, creates parent directories".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string"},
                        "content": {"type": "string"}
                    },
                    "required": ["path", "content"]
                }),
            },
        },
        ToolDefinition {
            type_: "function".into(),
            function: ToolFunction {
                name: "find".into(),
                description: "Search files with ripgrep".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {"type": "string"}
                    },
                    "required": ["query"]
                }),
            },
        },
        ToolDefinition {
            type_: "function".into(),
            function: ToolFunction {
                name: "sh".into(),
                description: "Run shell command".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": {"type": "string"}
                    },
                    "required": ["command"]
                }),
            },
        },
    ]
}

async fn read(args: &Value) -> Result<String> {
    let path = args["path"].as_str().context("read: missing path")?;
    let start = args.get("start").and_then(|v| v.as_i64()).unwrap_or(0) as usize;
    let end = args.get("end").and_then(|v| v.as_i64()).unwrap_or(200) as usize;
    let content = tokio::fs::read_to_string(path).await?;
    let lines: Vec<&str> = content.lines().collect();
    let total = lines.len();
    let s = start.min(total);
    let e = end.min(total).max(s);
    let selected = lines[s..e].join("\n");
    Ok(format!("{path} ({total} lines, {s}-{e}):\n{selected}"))
}

async fn write(args: &Value) -> Result<String> {
    let path = args["path"].as_str().context("write: missing path")?;
    let content = args["content"].as_str().context("write: missing content")?;
    if let Some(parent) = Path::new(path).parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(path, content).await?;
    Ok(format!("{path} written ({} bytes)", content.len()))
}

async fn find(args: &Value) -> Result<String> {
    let query = args["query"].as_str().context("find: missing query")?;
    let output = match Command::new("rg")
        .arg("--no-heading")
        .arg("--line-number")
        .arg(query)
        .output().await
    {
        Ok(o) => o,
        Err(e) => return Ok(format!("rg not found: {e}. Install ripgrep or use sh with grep.")),
    };
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    if !stdout.is_empty() { return Ok(stdout); }
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    Ok(if stderr.is_empty() { "No matches".into() } else { stderr })
}

fn is_dangerous(cmd: &str) -> bool {
    let t = cmd.trim();
    (t.starts_with("rm ") && (t.contains(" -rf") || t.contains(" -r") || t.contains(" -f") || t.contains(" --recursive")))
        || t.starts_with("git push")
        || t.starts_with("cargo publish")
        || t.starts_with("npm publish")
        || t.contains("DROP TABLE") || t.contains("drop table")
        || t.contains("ALTER TABLE") || t.contains("alter table")
}

async fn sh(args: &Value) -> Result<String> {
    let cmd = args["command"].as_str().context("sh: missing command")?;
    if is_dangerous(cmd) {
        return Ok("BLOCKED: dangerous command".into());
    }
    let shell = if cfg!(windows) { "cmd" } else { "sh" };
    let flag = if cfg!(windows) { "/c" } else { "-c" };
    let output = Command::new(shell).arg(flag).arg(cmd).output().await?;
    let mut result = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.is_empty() {
        if !result.is_empty() { result.push('\n'); }
        result.push_str(&stderr);
    }
    Ok(if result.is_empty() { "OK".into() } else { result })
}
