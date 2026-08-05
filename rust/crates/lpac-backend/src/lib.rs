use anyhow::{Context, Result, anyhow};
use lpac_core::ActivationCode;
use serde_json::Value;
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use zeroize::Zeroizing;

#[derive(Debug, Clone)]
pub struct LegacyLpacBackend {
    executable: PathBuf,
    reader_index: Option<u32>,
}

impl LegacyLpacBackend {
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            reader_index: None,
        }
    }

    pub fn with_reader_index(mut self, reader_index: Option<u32>) -> Self {
        self.reader_index = reader_index;
        self
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }

    pub fn chip_info(&self) -> Result<Value> {
        self.run_json(["chip", "info"], None)
    }

    pub fn profiles(&self) -> Result<Value> {
        self.run_json(["profile", "list"], None)
    }

    /// Uses the companion lpac stdin contract. A `-` value for `-a` or `-c`
    /// consumes one line from stdin, keeping both secrets out of argv.
    pub fn download(
        &self,
        code: &ActivationCode,
        confirmation_code: Option<&str>,
    ) -> Result<Value> {
        let mut args = vec!["profile", "download", "-a", "-"];
        let mut stdin = Zeroizing::new(String::with_capacity(
            code.expose().len() + confirmation_code.map_or(0, str::len) + 2,
        ));
        stdin.push_str(code.expose());
        stdin.push('\n');

        if let Some(confirmation_code) = confirmation_code {
            args.extend(["-c", "-"]);
            stdin.push_str(confirmation_code);
            stdin.push('\n');
        }

        self.run_json(args, Some(stdin.as_bytes()))
    }

    fn run_json<I, S>(&self, args: I, stdin: Option<&[u8]>) -> Result<Value>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let mut command = Command::new(&self.executable);
        command
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if stdin.is_some() {
            command.stdin(Stdio::piped());
        }
        command.env("LPAC_APDU", "pcsc");
        if let Some(index) = self.reader_index {
            command.env("LPAC_APDU_PCSC_DRV_IFID", index.to_string());
        }

        let mut child = command
            .spawn()
            .with_context(|| format!("failed to start {}", self.executable.display()))?;
        if let Some(input) = stdin {
            let mut pipe = child.stdin.take().context("lpac stdin is unavailable")?;
            pipe.write_all(input)?;
        }

        let output = child.wait_with_output()?;
        if !output.status.success() {
            return Err(anyhow!(
                "lpac failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        serde_json::from_slice(&output.stdout).context("lpac returned invalid JSON")
    }
}
