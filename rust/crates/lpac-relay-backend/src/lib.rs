use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    env,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
};
use uuid::Uuid;

const STDERR_HISTORY_LIMIT: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayAgentRole {
    Card,
    Server,
}

impl RelayAgentRole {
    fn argument(self) -> &'static str {
        match self {
            Self::Card => "card-agent",
            Self::Server => "server-agent",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayAgentError {
    pub code: Option<String>,
    pub message: Option<String>,
    pub subject_code: Option<String>,
    pub reason_code: Option<String>,
    pub subject_identifier: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RelayAgentResponse {
    request_id: Option<String>,
    operation: Option<String>,
    ok: bool,
    #[serde(default)]
    payload: Value,
    error: Option<RelayAgentError>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RelayAgentRequest<'a> {
    request_id: &'a str,
    operation: &'a str,
    payload: &'a Value,
}

pub struct RelayAgentProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    stderr_history: Arc<Mutex<VecDeque<String>>>,
    role: RelayAgentRole,
}

impl RelayAgentProcess {
    pub fn start(
        executable: impl Into<PathBuf>,
        role: RelayAgentRole,
        reader_index: Option<u32>,
    ) -> Result<Self> {
        let executable = executable.into();
        validate_bundled_runtime(&executable)?;

        let mut command = Command::new(&executable);
        command
            .args(["relay", role.argument()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("LPAC_APDU", "pcsc")
            .env("LPAC_HTTP", "winhttp");
        configure_runtime_environment(&executable, &mut command)?;

        if role == RelayAgentRole::Card
            && let Some(index) = reader_index
        {
            command.env("LPAC_APDU_PCSC_DRV_IFID", index.to_string());
        }

        let mut child = command
            .spawn()
            .with_context(|| format!("failed to start {} relay agent", role.argument()))?;
        let stdin = child
            .stdin
            .take()
            .context("relay agent stdin is unavailable")?;
        let stdout = child
            .stdout
            .take()
            .map(BufReader::new)
            .context("relay agent stdout is unavailable")?;
        let stderr = child
            .stderr
            .take()
            .context("relay agent stderr is unavailable")?;
        let stderr_history = Arc::new(Mutex::new(VecDeque::new()));
        let stderr_sink = Arc::clone(&stderr_history);
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                if let Ok(mut history) = stderr_sink.lock() {
                    if history.len() == STDERR_HISTORY_LIMIT {
                        history.pop_front();
                    }
                    history.push_back(line);
                }
            }
        });

        Ok(Self {
            child,
            stdin,
            stdout,
            stderr_history,
            role,
        })
    }

    pub fn role(&self) -> RelayAgentRole {
        self.role
    }

    pub fn request(&mut self, operation: &str, payload: Value) -> Result<Value> {
        if let Some(status) = self
            .child
            .try_wait()
            .context("failed to inspect relay agent")?
        {
            return Err(anyhow!(
                "{} relay agent exited with {status}: {}",
                self.role.argument(),
                self.stderr_text()
            ));
        }

        let request_id = Uuid::new_v4().to_string();
        let request = RelayAgentRequest {
            request_id: &request_id,
            operation,
            payload: &payload,
        };
        serde_json::to_writer(&mut self.stdin, &request)
            .context("failed to encode relay request")?;
        self.stdin
            .write_all(b"\n")
            .context("failed to delimit relay request")?;
        self.stdin
            .flush()
            .context("failed to flush relay request")?;

        let mut response_line = String::new();
        let bytes = self
            .stdout
            .read_line(&mut response_line)
            .context("failed to read relay response")?;
        if bytes == 0 {
            return Err(anyhow!(
                "{} relay agent closed stdout: {}",
                self.role.argument(),
                self.stderr_text()
            ));
        }

        let response: RelayAgentResponse = serde_json::from_str(&response_line)
            .with_context(|| format!("invalid relay response: {}", response_line.trim()))?;
        if response.request_id.as_deref() != Some(request_id.as_str()) {
            return Err(anyhow!("relay response request ID mismatch"));
        }
        if response.operation.as_deref() != Some(operation) {
            return Err(anyhow!("relay response operation mismatch"));
        }
        if response.ok {
            return Ok(response.payload);
        }

        let error = response.error.unwrap_or(RelayAgentError {
            code: Some("UNKNOWN".into()),
            message: Some("relay agent returned an unspecified error".into()),
            subject_code: None,
            reason_code: None,
            subject_identifier: None,
        });
        Err(anyhow!(format_agent_error(&error)))
    }

    pub fn stderr_text(&self) -> String {
        self.stderr_history
            .lock()
            .map(|history| history.iter().cloned().collect::<Vec<_>>().join("\n"))
            .unwrap_or_else(|_| "stderr history is unavailable".into())
    }

    pub fn shutdown(mut self) -> Result<()> {
        let _ = self.request("shutdown", json!({}));
        drop(self.stdin);
        let status = self
            .child
            .wait()
            .context("failed to wait for relay agent")?;
        if !status.success() {
            return Err(anyhow!(
                "{} relay agent exited with {status}: {}",
                self.role.argument(),
                self.stderr_text()
            ));
        }
        Ok(())
    }
}

impl Drop for RelayAgentProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn format_agent_error(error: &RelayAgentError) -> String {
    let mut parts = Vec::new();
    if let Some(code) = error.code.as_deref() {
        parts.push(code.to_owned());
    }
    if let Some(message) = error.message.as_deref() {
        parts.push(message.to_owned());
    }
    if let Some(subject_code) = error.subject_code.as_deref()
        && subject_code != "0.0.0"
    {
        parts.push(format!("subjectCode={subject_code}"));
    }
    if let Some(reason_code) = error.reason_code.as_deref()
        && reason_code != "0.0.0"
    {
        parts.push(format!("reasonCode={reason_code}"));
    }
    if let Some(identifier) = error.subject_identifier.as_deref()
        && identifier != "unknown"
    {
        parts.push(format!("subject={identifier}"));
    }
    parts.join(": ")
}

fn runtime_root(executable: &Path) -> Option<&Path> {
    executable
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
}

fn configure_runtime_environment(executable: &Path, command: &mut Command) -> Result<()> {
    let Some(root) = runtime_root(executable) else {
        return Ok(());
    };
    command.current_dir(root);

    #[cfg(windows)]
    {
        let inherited = env::var_os("PATH").unwrap_or_default();
        let mut paths = vec![root.to_path_buf(), root.join("lib")];
        paths.extend(env::split_paths(&inherited));
        let joined = env::join_paths(paths).context("failed to construct lpac PATH")?;
        command.env("PATH", joined);
    }

    #[cfg(not(windows))]
    {
        let inherited = env::var_os("LD_LIBRARY_PATH").unwrap_or_default();
        let mut paths = vec![root.join("lib")];
        paths.extend(env::split_paths(&inherited));
        let joined = env::join_paths(paths).context("failed to construct lpac library path")?;
        command.env("LD_LIBRARY_PATH", joined);
    }

    Ok(())
}

fn validate_bundled_runtime(executable: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        let Some(root) = runtime_root(executable) else {
            return Ok(());
        };
        let looks_like_bundle = root.join("nik-lpa-gui.exe").is_file()
            || root.join("nik-lpa.exe").is_file()
            || root.join("nik-rsp-relay-lab.exe").is_file();
        if !looks_like_bundle {
            return Ok(());
        }

        let required = [
            "lpac.exe",
            "libgcc_s_seh-1.dll",
            "libwinpthread-1.dll",
            "driver/driver_apdu_pcsc.dll",
            "driver/driver_http_winhttp.dll",
        ];
        let missing = required
            .iter()
            .filter(|relative| !root.join(relative).is_file())
            .copied()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(anyhow!(
                "Windows bundle is incomplete. Missing: {}",
                missing.join(", ")
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_structured_server_error() {
        let error = RelayAgentError {
            code: Some("AUTHENTICATE_CLIENT_FAILED".into()),
            message: Some("transaction expired".into()),
            subject_code: Some("8.8.2".into()),
            reason_code: Some("3.7".into()),
            subject_identifier: Some("transactionId".into()),
        };
        let formatted = format_agent_error(&error);
        assert!(formatted.contains("AUTHENTICATE_CLIENT_FAILED"));
        assert!(formatted.contains("transactionId"));
    }
}
