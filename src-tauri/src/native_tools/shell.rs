use std::path::PathBuf;

use serde_json::Value;
use tokio::process::Command;

use super::{resolve_workspace_path, user_home_dir};

const COMMAND_DENYLIST: &[&str] = &[
    "sudo ",
    "sudo\n",
    "rm -rf /",
    "rm -rf /*",
    ":(){ :|:& };:",
    "mkfs.",
    "dd if=/dev/zero",
    "> /dev/sd",
];

pub async fn run_command(
    workspace_roots: &[String],
    default_timeout_ms: u64,
    arguments: &Value,
) -> Result<String, String> {
    let command = arguments
        .get("command")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "run_command requires command".to_string())?;

    let lowered = command.to_ascii_lowercase();
    for denied in COMMAND_DENYLIST {
        if lowered.contains(denied) {
            return Err("command is blocked by safety policy".to_string());
        }
    }

    let cwd = if let Some(cwd_arg) = arguments.get("cwd").and_then(|v| v.as_str()) {
        resolve_workspace_path(cwd_arg, workspace_roots)?
    } else {
        user_home_dir()?
    };

    if !cwd.is_dir() {
        return Err(format!("Working directory is not a directory: {}", cwd.display()));
    }

    let timeout_ms = arguments
        .get("timeout_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(default_timeout_ms)
        .clamp(1_000, 300_000);

    let output = run_shell_command(command, cwd, timeout_ms).await?;
    format_command_output(&output)
}

#[derive(Debug)]
struct CommandOutput {
    status_code: Option<i32>,
    stdout: String,
    stderr: String,
}

async fn run_shell_command(
    command: &str,
    cwd: PathBuf,
    timeout_ms: u64,
) -> Result<CommandOutput, String> {
    let mut cmd = {
        #[cfg(target_os = "windows")]
        {
            let mut c = Command::new("cmd");
            c.args(["/C", command]);
            c
        }
        #[cfg(not(target_os = "windows"))]
        {
            let mut c = Command::new("sh");
            c.args(["-c", command]);
            c
        }
    };
    cmd.current_dir(cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let child = cmd
        .spawn()
        .map_err(|err| format!("Failed to start command: {err}"))?;

    let result = tokio::time::timeout(
        std::time::Duration::from_millis(timeout_ms),
        child.wait_with_output(),
    )
    .await
    .map_err(|_| format!("Command timed out after {timeout_ms}ms"))?
    .map_err(|err| format!("Command failed: {err}"))?;

    Ok(CommandOutput {
        status_code: result.status.code(),
        stdout: String::from_utf8_lossy(&result.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&result.stderr).into_owned(),
    })
}

fn format_command_output(output: &CommandOutput) -> Result<String, String> {
    let mut out = String::new();
    if let Some(code) = output.status_code {
        out.push_str(&format!("exit_code: {code}\n"));
    }
    if !output.stdout.is_empty() {
        out.push_str("stdout:\n");
        out.push_str(&output.stdout);
        if !output.stdout.ends_with('\n') {
            out.push('\n');
        }
    }
    if !output.stderr.is_empty() {
        out.push_str("stderr:\n");
        out.push_str(&output.stderr);
        if !output.stderr.ends_with('\n') {
            out.push('\n');
        }
    }
    if out.trim().is_empty() {
        out.push_str("(no output)\n");
    }
    Ok(out)
}
