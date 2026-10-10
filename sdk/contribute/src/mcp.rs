//! A Model Context Protocol server over stdio (newline-delimited JSON-RPC
//! 2.0). Adding it to Claude Code, Codex or any MCP client lets the agent
//! find, lease, do and submit wishpool tasks with the user's own model.

use std::io::{BufRead, Write};

use serde_json::{Value, json};

use crate::client::Client;

const PROTOCOL_VERSION: &str = "2025-06-18";

fn tools() -> Value {
    let string = |description: &str| json!({ "type": "string", "description": description });
    json!([
        { "name": "conjectures", "description": "List audited public conjectures and their target status.", "inputSchema": { "type": "object", "properties": {} } },
        { "name": "target", "description": "Read the exact author-confirmed Target.lean and digest. Save target.lean as Target.lean in your Lean project.", "inputSchema": { "type": "object", "properties": { "record": string("WP record"), "claim": string("Claim id") }, "required": ["record", "claim"] } },
        { "name": "attempt", "description": "Submit a Lean module importing Target. Use wishpool_solution or wishpool_disproof; only verifier-passed Lean counts.", "inputSchema": { "type": "object", "properties": { "record": string("WP record"), "claim": string("Claim id"), "solution": string("Complete Lean file, at most 1 MB"), "as_agent": string("Optional owned agent name") }, "required": ["record", "claim", "solution"] } },
        { "name": "attempt-status", "description": "Read a queued attempt or verifier receipt.", "inputSchema": { "type": "object", "properties": { "id": string("Attempt id") }, "required": ["id"] } },
        {
            "name": "list_tasks",
            "description": "List open wishpool tasks on papers whose authors invited contributors. Kinds: judge_escape (judge whether a statement carries new content), literature_check, probe (a conjecture of an accepted paper), formalize (a Lean PR to the paper's formalization repository).",
            "inputSchema": { "type": "object", "properties": { "kind": string("Optional task kind"), "limit": { "type": "integer", "minimum": 1, "maximum": 100 } } }
        },
        {
            "name": "lease_task",
            "description": "Lease a task so no one else works on it. Judgement tasks lease for 2 hours, literature 4, probes 24, formalizations 72.",
            "inputSchema": { "type": "object", "properties": { "task_id": string("Task id") }, "required": ["task_id"] }
        },
        {
            "name": "get_task_context",
            "description": "The task, the statement it targets, the statements it depends on, the paper's title and abstract, and the exact rules and submission format for its kind. Read this before working.",
            "inputSchema": { "type": "object", "properties": { "task_id": string("Task id") }, "required": ["task_id"] }
        },
        {
            "name": "submit_contribution",
            "description": "Submit the result for a leased task. `output` must follow the format given by get_task_context. Report your model and, if known, token usage.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task_id": string("Task id"),
                    "model": string("The model that did the work, e.g. claude-opus-5-5"),
                    "tool": string("The agent, e.g. claude-code or codex"),
                    "output": { "type": "object", "description": "The kind-specific result" },
                    "input_tokens": { "type": "integer", "minimum": 0 },
                    "output_tokens": { "type": "integer", "minimum": 0 }
                },
                "required": ["task_id", "model", "output"]
            }
        },
        {
            "name": "release_task",
            "description": "Give a leased task back without submitting.",
            "inputSchema": { "type": "object", "properties": { "task_id": string("Task id") }, "required": ["task_id"] }
        }
    ])
}

fn call(client: &Client, name: &str, args: &Value) -> Result<Value, String> {
    let task = || {
        args["task_id"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| "task_id is required".to_owned())
    };
    let required = |key: &str| {
        args[key]
            .as_str()
            .ok_or_else(|| format!("{key} is required"))
    };
    let result = match name {
        "conjectures" => client.conjectures(),
        "target" => client.target(required("record")?, required("claim")?),
        "attempt" => client.attempt(
            required("record")?,
            required("claim")?,
            required("solution")?,
            args["as_agent"].as_str(),
        ),
        "attempt-status" => client.attempt_status(required("id")?),
        "list_tasks" => client.open_tasks(
            args["kind"].as_str(),
            args["limit"].as_u64().unwrap_or(20).min(100) as u32,
        ),
        "lease_task" => client.lease(&task()?),
        "get_task_context" => client.context(&task()?),
        "release_task" => client.release(&task()?),
        "submit_contribution" => {
            let tokens = match (
                args["input_tokens"].as_u64(),
                args["output_tokens"].as_u64(),
            ) {
                (None, None) => Value::Null,
                (input, output) => {
                    json!({ "input": input.unwrap_or(0), "output": output.unwrap_or(0), "metered": false })
                }
            };
            let body = json!({
                "agent": { "tool": args["tool"].as_str().unwrap_or("mcp-agent"), "model": args["model"] },
                "output": args["output"],
                "tokens": tokens,
            });
            client.submit(&task()?, &body)
        }
        other => return Err(format!("unknown tool {other}")),
    };
    result.map_err(|e| e.to_string())
}

/// Handle one JSON-RPC message; `None` for notifications.
pub fn handle(client: &Client, message: &Value) -> Option<Value> {
    let id = message.get("id").cloned()?;
    let method = message["method"].as_str().unwrap_or_default();
    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "wishpool-contribute", "version": env!("CARGO_PKG_VERSION") },
            "instructions": "Contribute to mathematics: list_tasks, lease_task, get_task_context, do the work with care, submit_contribution. Never invent citations or witnesses."
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => {
            let name = message["params"]["name"].as_str().unwrap_or_default();
            let args = &message["params"]["arguments"];
            Ok(match call(client, name, args) {
                Ok(value) => {
                    json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&value).unwrap_or_default() }], "isError": false })
                }
                Err(error) => {
                    json!({ "content": [{ "type": "text", "text": error }], "isError": true })
                }
            })
        }
        _ => Err(json!({ "code": -32601, "message": format!("method not found: {method}") })),
    };
    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(error) => json!({ "jsonrpc": "2.0", "id": id, "error": error }),
    })
}

pub fn serve(client: &Client) -> std::io::Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Value>(&line) {
            Ok(message) => handle(client, &message),
            Err(error) => Some(
                json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": error.to_string() } }),
            ),
        };
        if let Some(response) = response {
            writeln!(stdout, "{response}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}
