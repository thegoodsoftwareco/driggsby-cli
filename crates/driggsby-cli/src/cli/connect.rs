use std::{
    io::{self, IsTerminal, Write as _},
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{Result, bail};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command as TokioCommand,
    task::JoinHandle,
};

use crate::{
    cli::McpScope,
    cli::client_id,
    cli::known_client::KnownClient,
    cli::supported_mcp_config::{
        CliMcpClient, DRIGGSBY_MCP_URL, McpConfigCommand, build_installer_command,
        build_scoped_remover_command, render_shell_command,
    },
};

const CLIENT_CONFIG_COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const LOOPBACK_REDIRECT_NEEDLE: &str = "redirect_uri=http%3A%2F%2F127.0.0.1%3A";
const BROWSER_LAUNCH_FAILED_NEEDLE: &str = "Browser launch failed";
const REMOTE_SIGN_IN_HINT_RECENT_CHARS: usize = 512;
const REMOTE_SIGN_IN_HINT: &str = "
Remote sign-in note:
  The URL above redirects to 127.0.0.1 on this machine.
  If you opened this terminal over SSH, use SSH local port forwarding before
  opening the URL in your local browser.

If sign-in times out, run:
  codex mcp login driggsby
";

pub async fn run_setup_command(
    requested_client: Option<String>,
    print: bool,
    mcp_scope: Option<McpScope>,
) -> Result<()> {
    let client = resolve_client(requested_client)?;
    validate_mcp_scope(client, mcp_scope)?;

    let Some(cli_client) = client.cli_mcp_client() else {
        print_other_client_instructions();
        return Ok(());
    };

    let installer = build_installer_command(cli_client, mcp_scope);
    if print {
        print_manual_command(client, &installer);
        return Ok(());
    }

    println!("Adding Driggsby to {} MCP config...", client.display_name());
    flush_stdout()?;

    match run_config_command(&installer, stream_config_output(client)).await {
        Ok(Ok(output)) if output.status.success() => {
            print_success(client);
            Ok(())
        }
        Ok(Ok(output)) if command_reports_existing_config(&output) => {
            reinstall_existing_client(client, cli_client, mcp_scope).await
        }
        Ok(Ok(_)) => {
            print_auto_setup_failure(client, "The client command returned an error.", &installer);
            Ok(())
        }
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            print_auto_setup_failure(
                client,
                &format!("{} is not installed or not on PATH.", client.display_name()),
                &installer,
            );
            Ok(())
        }
        Ok(Err(_)) => {
            print_auto_setup_failure(client, "Could not start the client command.", &installer);
            Ok(())
        }
        Err(_) => {
            print_auto_setup_failure(client, "The client command timed out.", &installer);
            Ok(())
        }
    }
}

fn resolve_client(requested_client: Option<String>) -> Result<KnownClient> {
    match requested_client {
        Some(value) => parse_client(&value),
        None => prompt_for_client(),
    }
}

pub(super) fn parse_client(value: &str) -> Result<KnownClient> {
    let canonical = client_id::canonicalize(value);
    if canonical.is_empty() {
        bail!("Client is required.\n\nSupported clients:\n  claude-code\n  codex\n  other");
    }
    let Some(client) = KnownClient::from_client_id(&canonical) else {
        bail!(
            "Unsupported client: {canonical}\n\nSupported clients:\n  claude-code\n  codex\n  other"
        );
    };
    Ok(client)
}

pub(super) fn validate_mcp_scope(client: KnownClient, mcp_scope: Option<McpScope>) -> Result<()> {
    if mcp_scope.is_none() || matches!(client, KnownClient::ClaudeCode) {
        return Ok(());
    }
    bail!("-s is supported only for Claude Code.");
}

fn prompt_for_client() -> Result<KnownClient> {
    if !io::stdin().is_terminal() {
        bail!(
            "Pass a client name.\n\nExamples:\n  npx driggsby@latest mcp setup claude-code\n  npx driggsby@latest mcp setup codex\n  npx driggsby@latest mcp setup other"
        );
    }

    println!("Which client are you setting up?");
    println!();
    println!("  1. Claude Code");
    println!("  2. Codex");
    println!("  3. Other");
    println!();
    print!("Choose 1-3: ");
    flush_stdout()?;

    let choice = read_trimmed_line()?;
    match choice.as_str() {
        "1" => Ok(KnownClient::ClaudeCode),
        "2" => Ok(KnownClient::Codex),
        "3" => Ok(KnownClient::Other),
        _ => bail!("Choose 1, 2, or 3."),
    }
}

fn read_trimmed_line() -> Result<String> {
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(line.trim().to_string())
}

async fn reinstall_existing_client(
    client: KnownClient,
    cli_client: CliMcpClient,
    mcp_scope: Option<McpScope>,
) -> Result<()> {
    let remover = build_scoped_remover_command(cli_client, mcp_scope);
    let installer = build_installer_command(cli_client, mcp_scope);

    let stream_output = stream_config_output(client);

    match run_config_command(&remover, stream_output).await {
        Ok(Ok(output)) if output.status.success() || command_reports_missing_config(&output) => {
            match run_config_command(&installer, stream_output).await {
                Ok(Ok(output)) if output.status.success() => {
                    print_success(client);
                    Ok(())
                }
                _ => {
                    print_auto_setup_failure(
                        client,
                        "Could not replace the existing Driggsby MCP config.",
                        &installer,
                    );
                    Ok(())
                }
            }
        }
        _ => {
            print_auto_setup_failure(
                client,
                "Could not remove the existing Driggsby MCP config.",
                &installer,
            );
            Ok(())
        }
    }
}

async fn run_config_command(
    command: &McpConfigCommand,
    stream_output: bool,
) -> Result<std::io::Result<std::process::Output>, tokio::time::error::Elapsed> {
    tokio::time::timeout(
        CLIENT_CONFIG_COMMAND_TIMEOUT,
        run_config_command_inner(command, stream_output),
    )
    .await
}

async fn run_config_command_inner(
    command: &McpConfigCommand,
    stream_output: bool,
) -> std::io::Result<std::process::Output> {
    let mut process = TokioCommand::new(&command.program);
    process
        .args(&command.args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = process.spawn()?;
    let remote_sign_in_hint =
        stream_output.then(|| Arc::new(Mutex::new(RemoteSignInHintState::default())));

    let stdout_task = child.stdout.take().map(|stdout| {
        tokio::spawn(read_child_output(
            stdout,
            OutputStream::Stdout,
            stream_output,
            remote_sign_in_hint.clone(),
        ))
    });
    let stderr_task = child.stderr.take().map(|stderr| {
        tokio::spawn(read_child_output(
            stderr,
            OutputStream::Stderr,
            stream_output,
            remote_sign_in_hint.clone(),
        ))
    });

    let status = child.wait().await?;
    let stdout = collect_child_output(stdout_task).await?;
    let stderr = collect_child_output(stderr_task).await?;

    Ok(std::process::Output {
        status,
        stdout,
        stderr,
    })
}

#[derive(Debug, Clone, Copy)]
enum OutputStream {
    Stdout,
    Stderr,
}

async fn read_child_output(
    mut reader: impl AsyncRead + Unpin,
    stream: OutputStream,
    stream_output: bool,
    remote_sign_in_hint: Option<Arc<Mutex<RemoteSignInHintState>>>,
) -> std::io::Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut buffer = [0; 8192];

    loop {
        let bytes_read = reader.read(&mut buffer).await?;
        if bytes_read == 0 {
            return Ok(output);
        }

        let chunk = &buffer[..bytes_read];
        output.extend_from_slice(chunk);
        if stream_output {
            write_child_output(stream, chunk)?;
            maybe_print_remote_sign_in_hint(&remote_sign_in_hint, chunk)?;
        }
    }
}

async fn collect_child_output(
    task: Option<JoinHandle<std::io::Result<Vec<u8>>>>,
) -> std::io::Result<Vec<u8>> {
    let Some(task) = task else {
        return Ok(Vec::new());
    };
    task.await
        .map_err(|error| std::io::Error::other(format!("Could not read client output: {error}")))?
}

fn write_child_output(stream: OutputStream, bytes: &[u8]) -> std::io::Result<()> {
    match stream {
        OutputStream::Stdout => {
            let mut stdout = io::stdout().lock();
            stdout.write_all(bytes)?;
            stdout.flush()
        }
        OutputStream::Stderr => {
            let mut stderr = io::stderr().lock();
            stderr.write_all(bytes)?;
            stderr.flush()
        }
    }
}

#[derive(Default)]
struct RemoteSignInHintState {
    recent_output: String,
    saw_loopback_redirect: bool,
    saw_browser_launch_failed: bool,
    printed: bool,
}

impl RemoteSignInHintState {
    fn observe(&mut self, bytes: &[u8]) -> bool {
        self.recent_output.push_str(&String::from_utf8_lossy(bytes));
        if self.recent_output.contains(LOOPBACK_REDIRECT_NEEDLE) {
            self.saw_loopback_redirect = true;
        }
        if self.recent_output.contains(BROWSER_LAUNCH_FAILED_NEEDLE) {
            self.saw_browser_launch_failed = true;
        }
        self.trim_recent_output();

        if self.printed || !self.saw_loopback_redirect || !self.saw_browser_launch_failed {
            return false;
        }

        self.printed = true;
        true
    }

    fn trim_recent_output(&mut self) {
        if self.recent_output.chars().count() <= REMOTE_SIGN_IN_HINT_RECENT_CHARS {
            return;
        }

        self.recent_output = self
            .recent_output
            .chars()
            .rev()
            .take(REMOTE_SIGN_IN_HINT_RECENT_CHARS)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
    }
}

fn maybe_print_remote_sign_in_hint(
    remote_sign_in_hint: &Option<Arc<Mutex<RemoteSignInHintState>>>,
    bytes: &[u8],
) -> std::io::Result<()> {
    let Some(remote_sign_in_hint) = remote_sign_in_hint else {
        return Ok(());
    };
    let should_print = remote_sign_in_hint
        .lock()
        .map_err(|_| std::io::Error::other("Could not inspect client output."))?
        .observe(bytes);

    if should_print {
        let mut stdout = io::stdout().lock();
        stdout.write_all(REMOTE_SIGN_IN_HINT.as_bytes())?;
        stdout.flush()?;
    }
    Ok(())
}

fn stream_config_output(client: KnownClient) -> bool {
    matches!(client, KnownClient::Codex)
}

fn print_success(client: KnownClient) {
    println!("{} is set up.", client.display_name());
    println!();
    println!("Driggsby MCP URL:");
    println!("  {DRIGGSBY_MCP_URL}");
    println!();
    println!("Next:");
    print_next_step(client);
}

fn print_next_step(client: KnownClient) {
    for line in next_step_lines(client) {
        println!("{line}");
    }
}

pub(super) fn next_step_lines(client: KnownClient) -> &'static [&'static str] {
    match client {
        KnownClient::ClaudeCode => {
            &["  Open Claude Code, run /mcp, and authenticate Driggsby to get started."]
        }
        KnownClient::Codex => &[
            "  Complete the Driggsby sign-in in the browser window opened by Codex.",
            "  If no browser window opened, run:",
            "    codex mcp login driggsby",
        ],
        KnownClient::Other => other_client_instruction_lines(),
    }
}

fn print_other_client_instructions() {
    println!("Driggsby currently supports OAuth-based remote MCP only.");
    println!();
    println!("Driggsby OAuth MCP URL:");
    println!("  {DRIGGSBY_MCP_URL}");
    println!();
    println!("For another MCP client or agent:");
    for line in other_client_instruction_lines() {
        println!("{line}");
    }
}

pub(super) fn other_client_instruction_lines() -> &'static [&'static str] {
    &[
        "  Add a remote MCP server named driggsby.",
        "  Set its URL to https://app.driggsby.com/mcp.",
        "  Choose OAuth authentication if the client asks.",
        "  Complete the Driggsby browser sign-in when prompted.",
        "",
        "Requirement:",
        "  The MCP client must support OAuth-based MCP authentication.",
    ]
}

fn print_auto_setup_failure(client: KnownClient, reason: &str, installer: &McpConfigCommand) {
    println!(
        "Could not add Driggsby to {}: {reason}",
        client.display_name()
    );
    println!();
    print_manual_command(client, installer);
}

fn print_manual_command(client: KnownClient, installer: &McpConfigCommand) {
    println!(
        "Run this command to add Driggsby to {}:",
        client.display_name()
    );
    println!();
    println!("  {}", render_shell_command(installer));
    println!();
    println!("After adding it:");
    print_next_step(client);
}

fn command_reports_existing_config(output: &std::process::Output) -> bool {
    command_output_contains(output, "already exists")
}

fn command_reports_missing_config(output: &std::process::Output) -> bool {
    command_output_contains(output, "No MCP server found")
        || command_output_contains(output, "No project-local MCP server found")
        || command_output_contains(output, "No user-scoped MCP server found")
        || command_output_contains(output, "No MCP server named 'driggsby' found")
}

fn command_output_contains(output: &std::process::Output, needle: &str) -> bool {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    stdout.contains(needle) || stderr.contains(needle)
}

fn flush_stdout() -> Result<()> {
    Ok(io::stdout().flush()?)
}

#[cfg(test)]
mod tests;
