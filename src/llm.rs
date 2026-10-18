// OpenAI-compatible chat completion client with SSE streaming
// Streams tokens for text responses, accumulates tool call deltas

use std::collections::BTreeMap;
use std::io::Write;
use anyhow::{Context, Result};
use reqwest::Client;
use crate::types::*;

pub async fn call(
    client: &Client,
    base_url: &str,
    api_key: &str,
    model: &str,
    messages: &[Message],
    tools: &[ToolDefinition],
    quiet: bool,
) -> Result<Message> {
    let url = format!("{}/v1/chat/completions", base_url.trim_end_matches('/'));
    let req = ChatRequest {
        model: model.to_string(),
        messages: messages.to_vec(),
        tools: if tools.is_empty() { None } else { Some(tools.to_vec()) },
        stream: Some(true),
    };

    let mut resp = client.post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&req)
        .send()
        .await
        .context("API request failed")?
        .error_for_status()
        .context("API returned error")?;

    let mut content = String::new();
    let mut tcs: BTreeMap<usize, (Option<String>, Option<String>, Option<String>, String)> = BTreeMap::new();
    let mut buf = String::new();
    let mut done = false;
    let mut started = false;
    let mut thinking = false;

    while let Some(chunk) = resp.chunk().await? {
        if done { break; }
        buf.push_str(&String::from_utf8_lossy(&chunk));
        loop {
            let pos = match buf.find('\n') { Some(p) => p, None => break };
            let ln = buf[..pos].trim_end_matches('\r').to_string();
            buf = buf[pos + 1..].to_string();
            if ln.is_empty() || !ln.starts_with("data: ") { continue; }
            if ln == "data: [DONE]" { done = true; break; }
            let raw = &ln[6..];
            let sk: StreamChunk = match serde_json::from_str(raw) { Ok(c) => c, Err(_) => continue };
            for c in sk.choices {
                if !quiet {
                    if let Some(r) = &c.delta.reasoning_content {
                        if !r.is_empty() {
                            if !started { started = true; }
                            if !thinking { thinking = true; print!("\n[think] "); }
                            print!("{r}");
                            std::io::stdout().flush()?;
                        }
                    }
                }
                if let Some(t) = &c.delta.content {
                    if thinking { thinking = false; println!(); }
                    if !t.is_empty() {
                        if !started { started = true; println!(); }
                        print!("{t}");
                        std::io::stdout().flush()?;
                    }
                    content.push_str(t);
                }
                if let Some(td) = &c.delta.tool_calls {
                    for td in td {
                        let e = tcs.entry(td.index).or_default();
                        if let Some(id) = &td.id { e.0 = Some(id.clone()); }
                        if let Some(ty) = &td.type_ { e.1 = Some(ty.clone()); }
                        if let Some(fn_name) = td.function.as_ref().and_then(|f| f.name.as_ref()) { e.2 = Some(fn_name.clone()); }
                        if let Some(fn_args) = td.function.as_ref().and_then(|f| f.arguments.as_ref()) { e.3.push_str(fn_args); }
                    }
                }
            }
        }
    }

    if !tcs.is_empty() {
        let mut calls = Vec::new();
        for (_, (id, ty, name, args)) in tcs {
            calls.push(ToolCall {
                id: id.unwrap_or_default(),
                type_: ty.unwrap_or_else(|| "function".into()),
                function: ToolCallFunction { name: name.unwrap_or_default(), arguments: args },
            });
        }
        Ok(Message { role: "assistant".into(), content: None, tool_calls: Some(calls), tool_call_id: None })
    } else {
        println!();
        Ok(Message { role: "assistant".into(), content: Some(content), tool_calls: None, tool_call_id: None })
    }
}
