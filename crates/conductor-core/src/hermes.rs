use std::{
    collections::HashMap,
    io::Write,
    process::{Command, Stdio},
};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::{Value, json};

const DEFAULT_CAPTURE_LINES: usize = 40;
const DEFAULT_SSH_CONNECT_TIMEOUT_SECS: u64 = 10;
const DEFAULT_SMOKE_MESSAGE: &str =
    "Health ping only. Do not run tools. Reply with exactly: HERMES_SUPERVISION_SMOKE_OK";
const DEFAULT_SMOKE_EXPECTED: &str = "HERMES_SUPERVISION_SMOKE_OK";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HermesProbeStatus {
    Online,
    Blocked,
    Offline,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HermesProbeConfig {
    pub ssh_target: String,
    pub tmux_target: String,
    pub capture_lines: usize,
    pub smoke: bool,
    pub smoke_message: String,
    pub smoke_expected: String,
    pub connect_timeout_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HermesProbeReport {
    pub status: HermesProbeStatus,
    pub host_label: Option<String>,
    pub tmux_target: String,
    pub tmux_target_exists: bool,
    pub tmux_session_count: Option<i64>,
    pub hermes_process_count: Option<i64>,
    pub disk_root_available_kb: Option<i64>,
    pub mem_available_kb: Option<i64>,
    pub smoke_sent: bool,
    pub smoke_ok: Option<bool>,
    pub blocked_reason: Option<String>,
}

impl HermesProbeConfig {
    pub fn from_inputs(
        ssh_target: Option<String>,
        tmux_target: Option<String>,
        capture_lines: usize,
        smoke: bool,
        smoke_message: Option<String>,
        smoke_expected: Option<String>,
    ) -> Result<Self> {
        let ssh_target = ssh_target
            .or_else(|| std::env::var("HERMES_SSH_TARGET").ok())
            .context("HERMES_SSH_TARGET or --ssh-target is required")?;
        let tmux_target = tmux_target
            .or_else(|| std::env::var("HERMES_TMUX_TARGET").ok())
            .context("HERMES_TMUX_TARGET or --tmux-target is required")?;

        if ssh_target.trim().is_empty() {
            bail!("Hermes SSH target cannot be empty");
        }
        if tmux_target.trim().is_empty() {
            bail!("Hermes tmux target cannot be empty");
        }

        Ok(Self {
            ssh_target,
            tmux_target,
            capture_lines: if capture_lines == 0 {
                DEFAULT_CAPTURE_LINES
            } else {
                capture_lines
            },
            smoke,
            smoke_message: smoke_message.unwrap_or_else(|| DEFAULT_SMOKE_MESSAGE.to_string()),
            smoke_expected: smoke_expected.unwrap_or_else(|| DEFAULT_SMOKE_EXPECTED.to_string()),
            connect_timeout_secs: DEFAULT_SSH_CONNECT_TIMEOUT_SECS,
        })
    }
}

impl HermesProbeReport {
    pub fn to_redacted_payload(&self) -> Value {
        json!({
            "status": self.status,
            "host_label": self.host_label,
            "tmux_target": self.tmux_target,
            "tmux_target_exists": self.tmux_target_exists,
            "tmux_session_count": self.tmux_session_count,
            "hermes_process_count": self.hermes_process_count,
            "disk_root_available_kb": self.disk_root_available_kb,
            "mem_available_kb": self.mem_available_kb,
            "smoke_sent": self.smoke_sent,
            "smoke_ok": self.smoke_ok,
            "blocked_reason": self.blocked_reason,
        })
    }

    pub fn severity(&self) -> &'static str {
        match self.status {
            HermesProbeStatus::Online if self.smoke_ok == Some(false) => "warn",
            HermesProbeStatus::Online => "info",
            HermesProbeStatus::Blocked | HermesProbeStatus::Offline => "warn",
        }
    }

    pub fn message(&self) -> &'static str {
        match self.status {
            HermesProbeStatus::Online if self.smoke_ok == Some(true) => {
                "remote hermes supervision smoke ok"
            }
            HermesProbeStatus::Online if self.smoke_ok == Some(false) => {
                "remote hermes supervision smoke failed"
            }
            HermesProbeStatus::Online => "remote hermes health online",
            HermesProbeStatus::Blocked => "remote hermes health blocked",
            HermesProbeStatus::Offline => "remote hermes health offline",
        }
    }
}

pub fn probe(config: &HermesProbeConfig) -> Result<HermesProbeReport> {
    let mut child = Command::new("ssh")
        .args([
            "-o",
            "BatchMode=yes",
            "-o",
            "StrictHostKeyChecking=accept-new",
            "-o",
            &format!("ConnectTimeout={}", config.connect_timeout_secs),
            &config.ssh_target,
            "sh",
            "-s",
            "--",
            &config.tmux_target,
            &config.capture_lines.to_string(),
            if config.smoke { "1" } else { "0" },
            &config.smoke_message,
            &config.smoke_expected,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to start ssh for remote Hermes probe")?;

    let mut stdin = child
        .stdin
        .take()
        .context("failed to open ssh stdin for remote Hermes probe")?;
    stdin
        .write_all(remote_probe_script().as_bytes())
        .context("failed to write remote Hermes probe script")?;
    drop(stdin);

    let output = child
        .wait_with_output()
        .context("failed to wait for remote Hermes probe")?;

    if !output.status.success() {
        return Ok(HermesProbeReport {
            status: HermesProbeStatus::Offline,
            host_label: None,
            tmux_target: config.tmux_target.clone(),
            tmux_target_exists: false,
            tmux_session_count: None,
            hermes_process_count: None,
            disk_root_available_kb: None,
            mem_available_kb: None,
            smoke_sent: config.smoke,
            smoke_ok: if config.smoke { Some(false) } else { None },
            blocked_reason: Some("ssh probe failed".to_string()),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_probe_output(
        &stdout,
        &config.tmux_target,
        config.smoke.then_some(config.smoke_expected.as_str()),
    )
}

pub fn parse_probe_output(
    output: &str,
    tmux_target: &str,
    smoke_expected: Option<&str>,
) -> Result<HermesProbeReport> {
    let fields = parse_key_values(output);
    let tmux_target_exists = fields
        .get("TMUX_TARGET_EXISTS")
        .is_some_and(|value| value == "yes");
    let smoke_ok = smoke_expected.map(|expected| output.contains(expected));

    let status = if tmux_target_exists {
        HermesProbeStatus::Online
    } else {
        HermesProbeStatus::Blocked
    };
    let blocked_reason = if tmux_target_exists {
        None
    } else {
        Some("tmux target not found".to_string())
    };

    Ok(HermesProbeReport {
        status,
        host_label: fields.get("HOST").cloned(),
        tmux_target: tmux_target.to_string(),
        tmux_target_exists,
        tmux_session_count: parse_i64(fields.get("TMUX_SESSIONS")),
        hermes_process_count: parse_i64(fields.get("HERMES_PROCS")),
        disk_root_available_kb: parse_i64(fields.get("DISK_ROOT_AVAIL_KB")),
        mem_available_kb: parse_i64(fields.get("MEM_AVAILABLE_KB")),
        smoke_sent: smoke_expected.is_some(),
        smoke_ok,
        blocked_reason,
    })
}

fn parse_key_values(output: &str) -> HashMap<String, String> {
    output
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            Some((key.trim().to_string(), value.trim().to_string()))
        })
        .collect()
}

fn parse_i64(value: Option<&String>) -> Option<i64> {
    value.and_then(|value| value.parse().ok())
}

fn remote_probe_script() -> &'static str {
    r#"set -eu
target="$1"
lines="$2"
smoke="$3"
message="$4"
expected="$5"

echo "HOST=$(hostname)"
echo "TMUX_SESSIONS=$(tmux list-sessions 2>/dev/null | wc -l | tr -d ' ')"
if tmux has-session -t "$target" 2>/dev/null; then
  echo "TMUX_TARGET_EXISTS=yes"
else
  echo "TMUX_TARGET_EXISTS=no"
fi
echo "HERMES_PROCS=$(pgrep -af 'hermes|mimo|codex' 2>/dev/null | grep -v pgrep | wc -l | tr -d ' ')"
echo "DISK_ROOT_AVAIL_KB=$(df -Pk / 2>/dev/null | awk 'NR==2 {print $4}')"
echo "MEM_AVAILABLE_KB=$(awk '/MemAvailable/ {print $2}' /proc/meminfo 2>/dev/null || echo 0)"

if tmux has-session -t "$target" 2>/dev/null; then
  if [ "$smoke" = "1" ]; then
    tmux send-keys -t "$target" "$message" Enter
    sleep 10
  fi
  echo "CAPTURE_BEGIN"
  tmux capture-pane -pt "$target" -S "-$lines" 2>/dev/null | tail -"$lines" || true
  echo "CAPTURE_END"
fi
"#
}
