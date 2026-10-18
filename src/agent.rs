// Core agent loop: calls the LLM, executes tool calls, loops until final answer
// Uses ToolCache to deduplicate identical tool calls within a task

use anyhow::Result;
use futures::future::join_all;
use reqwest::Client;
use crate::{llm, tools};
use crate::tools::{ToolCache, CacheStats};
use crate::types::Message;

pub async fn run(
    model: &str,
    api_key: &str,
    base_url: &str,
    task: &str,
    history: &mut Vec<Message>,
    cache: &mut ToolCache,
    quiet: bool,
) -> Result<CacheStats> {
    let client = Client::new();
    let tool_defs = tools::definitions();

    if history.is_empty() {
        history.push(Message {
            role: "system".into(), content: Some(SYSTEM.into()), tool_calls: None, tool_call_id: None,
        });
    }
    history.push(Message {
        role: "user".into(), content: Some(task.into()), tool_calls: None, tool_call_id: None,
    });

    loop {
        let response = llm::call(&client, base_url, api_key, model, history, &tool_defs, quiet).await?;

        if let Some(tool_calls) = response.tool_calls.clone() {
            history.push(response);

            let mut tasks = Vec::new();
            let mut cached: Vec<(String, String)> = Vec::new();
            for tc in &tool_calls {
                let args: serde_json::Value = serde_json::from_str(&tc.function.arguments).unwrap_or_default();
                let display = args.get("path")
                    .or_else(|| args.get("query"))
                    .or_else(|| args.get("command"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                eprintln!("  {:<6} {}", tc.function.name, display);

                if let Some(res) = cache.contains(&tc.function.name, &args) {
                    let cached_res = res.clone();
                    cache.hit();
                    cached.push((tc.id.clone(), cached_res));
                } else {
                    cache.miss();
                    let id = tc.id.clone();
                    let name = tc.function.name.clone();
                    let args_str = tc.function.arguments.clone();
                    tasks.push(async move {
                        let a: serde_json::Value = serde_json::from_str(&args_str).unwrap_or_default();
                        let res = tools::raw(&name, &a).await;
                        (id, name, a, res)
                    });
                }
            }

            for (id, content) in cached {
                history.push(Message {
                    role: "tool".into(), content: Some(content), tool_calls: None, tool_call_id: Some(id),
                });
            }

            for (id, name, args, result) in join_all(tasks).await {
                let content = match &result {
                    Ok(s) => { cache.insert(&name, &args, s.clone()); s.clone() },
                    Err(e) => format!("Error: {e}"),
                };
                history.push(Message {
                    role: "tool".into(), content: Some(content), tool_calls: None, tool_call_id: Some(id),
                });
            }
        } else {
            history.push(response);
            break;
        }
    }
    Ok(cache.stats())
}

const SYSTEM: &str = "You are Meo.

Priorities:
1. Correctness
2. Minimal tool usage
3. Minimal token usage
4. Production-grade code
5. Parallelize tool calls for faster task completion

Only read files when necessary.
Only call tools when necessary.
Avoid repeated reads.
Return concise responses.";
