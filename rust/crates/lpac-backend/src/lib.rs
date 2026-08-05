use anyhow::{Context, Result, anyhow};
use lpac_core::ActivationCode;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use zeroize::Zeroizing;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LpacPayload {
    pub code: i32,
    pub message: String,
    #[serde(default)]
    pub data: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum LpacEvent {
    #[serde(rename = "progress")]
    Progress { payload: LpacPayload },
    #[serde(rename = "lpa")]
    Result { payload: LpacPayload },
}

#[derive(Debug, Clone, Serialize)]
pub struct LpacRun {
    pub progress: Vec<LpacPayload>,
    pub result: LpacPayload,
}

impl LpacRun {
    pub fn pretty_log(&self) -> String {
        let mut lines = self
            .progress
            .iter()
            .map(|event| format!("[progress] {}: {}", event.message, event.data))
            .collect::<Vec<_>>();
        lines.push(format!("[result] {}", self.result.message));
        if !self.result.data.is_null() {
            lines.push(
                serde_json::to_string_pretty(&self.result.data)
                    .unwrap_or_else(|_| self.result.data.to_string()),
            );
        }
        lines.join("\n")
    }
}

pub fn parse_lpac_ndjson(output: &[u8]) -> Result<LpacRun> {
    let text = std::str::from_utf8(output).context("lpac output is not UTF-8")?;
    let mut progress = Vec::new();
    let mut result = None;

    for (line_index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let event: LpacEvent = serde_json::from_str(line)
            .with_context(|| format!("invalid lpac JSON event on line {}", line_index + 1))?;
        match event {
            LpacEvent::Progress { payload } => progress.push(payload),
            LpacEvent::Result { payload } => {
                if result.replace(payload).is_some() {
                    return Err(anyhow!("lpac returned more than one final event"));
                }
            }
        }
    }

    let result = result.context("lpac output did not contain a final lpa event")?;
    if result.code != 0 {
        return Err(anyhow!(
            "lpac {}: {}",
            result.message,
            if result.data.is_null() {
                String::new()
            } else {
                result.data.to_string()
            }
        ));
    }

    Ok(LpacRun { progress, result })
}

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

    pub fn chip_info(&self) -> Result<LpacRun> {
        self.run(["chip", "info"], None)
    }

    pub fn profiles(&self) -> Result<LpacRun> {
        self.run(["profile", "list"], None)
    }

    /// Uses the companion lpac stdin contract. A `-` value for `-a` or `-c`
    /// consumes one line from stdin, keeping both secrets out of argv.
    pub fn download(
        &self,
        code: &ActivationCode,
        confirmation_code: Option<&str>,
    ) -> Result<LpacRun> {
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

        self.run(args, Some(stdin.as_bytes()))
    }

    /// A successful ES10b load is not accepted as final proof on its own.
    /// The installed ICCID must be returned by a fresh `profile list` call.
    pub fn download_and_verify(
        &self,
        code: &ActivationCode,
        confirmation_code: Option<&str>,
    ) -> Result<LpacRun> {
        let mut download = self.download(code, confirmation_code)?;
        let iccid = download
            .result
            .data
            .get("iccid")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .context("lpac download result did not contain a non-empty ICCID")?
            .to_owned();

        let profiles = self.profiles()?;
        let profile_list = profiles
            .result
            .data
            .as_array()
            .context("lpac profile list result was not an array")?;
        let installed = profile_list.iter().any(|profile| {
            profile
                .get("iccid")
                .and_then(Value::as_str)
                .is_some_and(|value| value == iccid)
        });
        if !installed {
            return Err(anyhow!(
                "download reported ICCID {iccid}, but a fresh profile list did not contain it"
            ));
        }

        download.progress.extend(profiles.progress);
        download.progress.push(LpacPayload {
            code: 0,
            message: "post_install_profile_list_verified".into(),
            data: json!({ "iccid": iccid }),
        });
        Ok(download)
    }

    fn run<I, S>(&self, args: I, stdin: Option<&[u8]>) -> Result<LpacRun>
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
        let run = parse_lpac_ndjson(&output.stdout).map_err(|error| {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.trim().is_empty() {
                error
            } else {
                anyhow!("{error:#}; lpac stderr: {}", stderr.trim())
            }
        })?;
        if !output.status.success() {
            return Err(anyhow!(
                "lpac exited with status {} after returning a success event",
                output.status
            ));
        }
        Ok(run)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_progress_and_final_event() {
        let output = br#"{"type":"progress","payload":{"code":0,"message":"step","data":"reader"}}
{"type":"lpa","payload":{"code":0,"message":"success","data":{"eidValue":"8901"}}}
"#;

        let run = parse_lpac_ndjson(output).unwrap();
        assert_eq!(run.progress.len(), 1);
        assert_eq!(run.progress[0].message, "step");
        assert_eq!(run.result.data["eidValue"], "8901");
    }

    #[test]
    fn rejects_lpac_error_event() {
        let output = br#"{"type":"lpa","payload":{"code":-1,"message":"es9p_initiate_authentication","data":"network"}}
"#;

        let error = parse_lpac_ndjson(output).unwrap_err().to_string();
        assert!(error.contains("es9p_initiate_authentication"));
        assert!(error.contains("network"));
    }

    #[test]
    fn rejects_missing_final_event() {
        let output = br#"{"type":"progress","payload":{"code":0,"message":"step","data":null}}
"#;

        assert!(parse_lpac_ndjson(output).is_err());
    }

    #[test]
    fn rejects_malformed_event() {
        assert!(parse_lpac_ndjson(b"not-json\n").is_err());
    }
}
