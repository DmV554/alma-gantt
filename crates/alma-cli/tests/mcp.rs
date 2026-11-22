//! `alma mcp` over stdio, spoken as raw JSON-RPC lines like an MCP client would.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/hybrid-terminal.alma.toml"
);

/// Sends `requests` and returns one response per request that has an `id`.
fn session(requests: &[Value]) -> Vec<Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_alma"))
        .args(["--file", FIXTURE, "mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut responses = Vec::new();
    for r in requests {
        writeln!(stdin, "{r}").unwrap();
        stdin.flush().unwrap();
        if r.get("id").is_none() {
            continue;
        }
        // Read until the response to this id (skip notifications from the server).
        loop {
            let mut line = String::new();
            assert!(
                out.read_line(&mut line).unwrap() > 0,
                "server closed the stream"
            );
            let v: Value = serde_json::from_str(&line).unwrap();
            if v.get("id") == r.get("id") {
                responses.push(v);
                break;
            }
        }
    }
    drop(stdin);
    assert!(
        child.wait().unwrap().success(),
        "server exits cleanly on EOF"
    );
    responses
}

fn handshake(version: &str) -> Vec<Value> {
    vec![
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": version, "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } } }),
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
    ]
}

#[test]
fn handshake_versions_list_and_call() {
    for version in ["2024-11-05", "2025-06-18", "2025-11-25"] {
        let mut reqs = handshake(version);
        reqs.push(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }));
        reqs.push(json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": { "name": "critical_path", "arguments": {} } }));
        reqs.push(json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call",
            "params": { "name": "task_set", "arguments": { "id": "yard-sw", "fields": { "duration": "60d" }, "dry_run": true } } }));
        let r = session(&reqs);
        assert_eq!(
            r[0]["result"]["protocolVersion"], version,
            "negotiates {version}"
        );
        assert_eq!(r[0]["result"]["serverInfo"]["name"], "alma");
        let tools = r[1]["result"]["tools"].as_array().unwrap();
        assert!(tools.len() >= 25);
        assert_eq!(r[2]["result"]["isError"], false);
        assert_eq!(
            r[2]["result"]["structuredContent"]["project_finish"],
            "2027-07-30"
        );
        assert_eq!(
            r[3]["result"]["structuredContent"]["end_date_delta"],
            "+10d"
        );
    }
}

#[test]
fn tool_errors_are_results_not_protocol_errors() {
    let mut reqs = handshake("2025-06-18");
    reqs.push(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": { "name": "dep_add", "arguments": { "pred": "h4", "succ": "discovery", "dry_run": true } } }));
    reqs.push(json!({ "jsonrpc": "2.0", "id": 3, "method": "no/such/method" }));
    let r = session(&reqs);
    assert_eq!(r[1]["result"]["isError"], true);
    let text = r[1]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("E-DEP-01"), "{text}");
    assert!(
        r[2].get("error").is_some(),
        "unknown methods are JSON-RPC errors"
    );
}

#[test]
fn render_sends_an_image() {
    let mut reqs = handshake("2025-06-18");
    reqs.push(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": { "name": "render", "arguments": { "depth": 1 } } }));
    let r = session(&reqs);
    let content = r[1]["result"]["content"].as_array().unwrap();
    let image = content
        .iter()
        .find(|c| c["type"] == "image")
        .expect("image content");
    assert_eq!(image["mimeType"], "image/png");
    assert!(image["data"].as_str().unwrap().len() > 1000);
}

/// The official Rust SDK as the client, in both lifecycles: the `initialize` handshake (newest
/// version with it is 2025-11-25) and stateless `server/discover` (2026-07-28).
#[test]
fn official_sdk_client_in_both_lifecycles() {
    use rmcp::model::{CallToolRequestParams, ProtocolVersion};
    use rmcp::service::{ClientLifecycleMode, ClientServiceExt};
    use rmcp::transport::TokioChildProcess;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let modes = [
        (ClientLifecycleMode::Initialize, "2025-11-25"),
        (
            ClientLifecycleMode::Discover {
                preferred_versions: vec![ProtocolVersion::V_2026_07_28],
            },
            "2026-07-28",
        ),
    ];
    for (mode, expected) in modes {
        rt.block_on(async {
            let mut cmd = tokio::process::Command::new(env!("CARGO_BIN_EXE_alma"));
            cmd.args(["--file", FIXTURE, "mcp"]);
            let transport = TokioChildProcess::new(cmd).unwrap();
            let client = ().serve_with_lifecycle(transport, mode).await.unwrap();
            let version = client.peer_info().map(|i| i.protocol_version.to_string());
            assert_eq!(version.as_deref(), Some(expected));
            let tools = client.list_all_tools().await.unwrap();
            assert!(tools.iter().any(|t| t.name == "render"), "{expected}");
            let mut params = CallToolRequestParams::new("show_task");
            params.arguments = Some(
                serde_json::json!({ "id": "gate-sw" })
                    .as_object()
                    .cloned()
                    .unwrap(),
            );
            let result = client.call_tool(params).await.unwrap();
            let v = serde_json::to_value(&result).unwrap();
            assert_eq!(v["structuredContent"]["tf"], 10.0, "{expected}: {v}");
            client.cancel().await.unwrap();
        });
    }
}
