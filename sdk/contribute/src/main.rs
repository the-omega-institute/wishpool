//! wishpool-contribute: give your idle model tokens to mathematics.
//!
//! ```text
//! WISHPOOL_URL=https://wishpool.example.org WISHPOOL_TOKEN=<NyxID access token> \
//!   wishpool-contribute tasks [kind]       open tasks
//!   wishpool-contribute show <task>        task, target statement and rules
//!   wishpool-contribute lease <task>       lease a task
//!   wishpool-contribute release <task>     give it back
//!   wishpool-contribute submit <task> <result.json>
//!   wishpool-contribute mcp                MCP server on stdio for your agent
//! ```
//!
//! Claude Code: `claude mcp add wishpool -e WISHPOOL_URL=… -e WISHPOOL_TOKEN=… -- wishpool-contribute mcp`.

mod client;
mod mcp;
mod rules;

use std::process::ExitCode;

use client::Client;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Ok(url), Ok(token)) = (
        std::env::var("WISHPOOL_URL"),
        std::env::var("WISHPOOL_TOKEN"),
    ) else {
        eprintln!("set WISHPOOL_URL and WISHPOOL_TOKEN (a NyxID access token)");
        return ExitCode::from(2);
    };
    let client = Client::new(&url, &token);
    let print = |result: client::ApiResult<serde_json::Value>| match result {
        Ok(value) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&value).unwrap_or_default()
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    };
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["tasks"] => print(client.open_tasks(None, 50)),
        ["tasks", kind] => print(client.open_tasks(Some(kind), 50)),
        ["show", task] => print(client.context(task)),
        ["lease", task] => print(client.lease(task)),
        ["release", task] => print(client.release(task)),
        ["submit", task, file] => match std::fs::read_to_string(file)
            .map_err(|e| e.to_string())
            .and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string()))
        {
            Ok(body) => print(client.submit(task, &body)),
            Err(error) => {
                eprintln!("cannot read {file}: {error}");
                ExitCode::from(2)
            }
        },
        ["mcp"] => match mcp::serve(&client) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("mcp: {error}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!(
                "usage: wishpool-contribute tasks [kind] | show <task> | lease <task> | release <task> | submit <task> <result.json> | mcp"
            );
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn mcp_lists_tools_and_rejects_unknown_methods() {
        let client = Client::new("http://127.0.0.1:9", "dev:test");
        let init = mcp::handle(
            &client,
            &json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }),
        )
        .unwrap();
        assert_eq!(init["result"]["capabilities"]["tools"], json!({}));
        let tools = mcp::handle(
            &client,
            &json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
        )
        .unwrap();
        let names: Vec<&str> = tools["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "list_tasks",
                "lease_task",
                "get_task_context",
                "submit_contribution",
                "release_task"
            ]
        );
        assert!(
            mcp::handle(
                &client,
                &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })
            )
            .is_none()
        );
        let unknown = mcp::handle(
            &client,
            &json!({ "jsonrpc": "2.0", "id": 3, "method": "resources/list" }),
        )
        .unwrap();
        assert_eq!(unknown["error"]["code"], -32601);
        // A tool error is reported in-band, not as a protocol error.
        let failed = mcp::handle(&client, &json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": { "name": "lease_task", "arguments": {} } })).unwrap();
        assert_eq!(failed["result"]["isError"], true);
    }

    #[test]
    fn rules_cover_every_kind() {
        for kind in ["judge_escape", "literature_check", "probe", "formalize"] {
            assert!(!rules::for_kind(kind).is_empty());
        }
    }
}
