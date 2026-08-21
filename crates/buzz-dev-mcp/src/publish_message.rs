//! Shell-free message publication for managed agents.

use crate::shell::SharedState;
use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

const PUBLISH_TIMEOUT: Duration = Duration::from_secs(60);
// Keep the MCP boundary aligned with buzz-cli's signed message limit so a
// producer cannot block the child stdin with a body the CLI will reject.
const MAX_CONTENT_BYTES: usize = 65_536;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PublishMessageParams {
    /// Channel UUID supplied by the current Buzz context.
    pub channel: String,
    /// Markdown body. Passed byte-for-byte over stdin, never through a shell.
    pub content: String,
    /// Immediate parent event ID for a threaded reply.
    #[serde(default)]
    pub reply_to: Option<String>,
    /// Explicit recipient pubkeys or npubs. Repeat entries for multiple people.
    #[serde(default)]
    pub mentions: Vec<String>,
    /// Event kind: 9, 45001, or 45003. Defaults to the channel message kind.
    #[serde(default)]
    pub kind: Option<u16>,
    /// Also publish to the wider Nostr network.
    #[serde(default)]
    pub broadcast: bool,
}

fn command_args(p: &PublishMessageParams) -> Vec<String> {
    let mut args = vec![
        "messages".to_owned(),
        "send".to_owned(),
        "--channel".to_owned(),
        p.channel.clone(),
        "--content".to_owned(),
        "-".to_owned(),
    ];
    if let Some(reply_to) = &p.reply_to {
        args.extend(["--reply-to".to_owned(), reply_to.clone()]);
    }
    if let Some(kind) = p.kind {
        args.extend(["--kind".to_owned(), kind.to_string()]);
    }
    if p.broadcast {
        args.push("--broadcast".to_owned());
    }
    for mention in &p.mentions {
        args.extend(["--mention".to_owned(), mention.clone()]);
    }
    args
}

fn tool_error(code: &str, message: impl AsRef<str>) -> CallToolResult {
    CallToolResult::error(vec![Content::text(
        json!({ "error": code, "message": message.as_ref() }).to_string(),
    )])
}

fn content_too_large(content: &str) -> bool {
    content.len() > MAX_CONTENT_BYTES
}

pub async fn run(
    state: &SharedState,
    p: PublishMessageParams,
) -> Result<CallToolResult, ErrorData> {
    if content_too_large(&p.content) {
        return Ok(tool_error(
            "content_too_large",
            format!("content exceeds {MAX_CONTENT_BYTES}-byte limit"),
        ));
    }
    let mut command = Command::new("buzz");
    command
        .args(command_args(&p))
        .current_dir(&state.cwd)
        .env("PATH", &state.shim.path_env)
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    crate::configure_no_window_async(&mut command);

    let mut child = command.spawn().map_err(|error| {
        ErrorData::internal_error(format!("failed to start buzz CLI: {error}"), None)
    })?;
    let mut stdin = child.stdin.take().ok_or_else(|| {
        ErrorData::internal_error("failed to open buzz CLI stdin".to_owned(), None)
    })?;
    let publish = async {
        stdin
            .write_all(p.content.as_bytes())
            .await
            .map_err(|error| format!("failed to write message content: {error}"))?;
        drop(stdin);
        child
            .wait_with_output()
            .await
            .map_err(|error| format!("buzz CLI failed: {error}"))
    };
    let output = match tokio::time::timeout(PUBLISH_TIMEOUT, publish).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => {
            return Ok(tool_error("publish_failed", error));
        }
        Err(_) => {
            return Ok(tool_error(
                "publish_timeout",
                "buzz CLI timed out after 60s",
            ))
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Ok(tool_error(
            "publish_failed",
            if stderr.is_empty() { stdout } else { stderr },
        ));
    }
    let value: serde_json::Value = serde_json::from_str(&stdout).map_err(|error| {
        ErrorData::internal_error(format!("buzz CLI returned invalid JSON: {error}"), None)
    })?;
    Ok(CallToolResult::success(vec![Content::text(
        value.to_string(),
    )]))
}

#[cfg(test)]
mod tests {
    use super::{command_args, content_too_large, PublishMessageParams, MAX_CONTENT_BYTES};

    #[test]
    fn content_never_enters_process_arguments() {
        let content = "first\n\n`code` $HOME $(touch nope) \"quoted\"";
        let params = PublishMessageParams {
            channel: "00000000-0000-0000-0000-000000000001".into(),
            content: content.into(),
            reply_to: Some("a".repeat(64)),
            mentions: vec!["b".repeat(64)],
            kind: Some(9),
            broadcast: true,
        };

        let args = command_args(&params);
        assert!(!args.iter().any(|arg| arg.contains("`code`")));
        assert_eq!(params.content.as_bytes(), content.as_bytes());
        assert!(args.windows(2).any(|pair| pair == ["--content", "-"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--mention", &"b".repeat(64)]));
    }

    #[test]
    fn content_limit_matches_cli_boundary() {
        assert!(!content_too_large(&"x".repeat(MAX_CONTENT_BYTES)));
        assert!(content_too_large(&"x".repeat(MAX_CONTENT_BYTES + 1)));
    }
}
