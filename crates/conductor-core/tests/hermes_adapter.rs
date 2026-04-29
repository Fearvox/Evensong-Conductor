use conductor_core::hermes::{HermesProbeStatus, parse_probe_output};

#[test]
fn parses_online_read_only_probe_output() {
    let output = r#"
HOST=hermes-nyc1
TMUX_SESSIONS=7
TMUX_TARGET_EXISTS=yes
HERMES_PROCS=5
DISK_ROOT_AVAIL_KB=67108864
MEM_AVAILABLE_KB=829440
"#;

    let report = parse_probe_output(output, "hermes-evo-health-20260426:mimo-max", None)
        .expect("probe output should parse");

    assert_eq!(report.status, HermesProbeStatus::Online);
    assert_eq!(report.host_label.as_deref(), Some("hermes-nyc1"));
    assert_eq!(report.tmux_target, "hermes-evo-health-20260426:mimo-max");
    assert_eq!(report.tmux_session_count, Some(7));
    assert_eq!(report.hermes_process_count, Some(5));
    assert_eq!(report.disk_root_available_kb, Some(67_108_864));
    assert_eq!(report.mem_available_kb, Some(829_440));
    assert!(!report.smoke_sent);
    assert_eq!(report.smoke_ok, None);
}

#[test]
fn marks_missing_tmux_target_as_blocked() {
    let output = r#"
HOST=hermes-nyc1
TMUX_SESSIONS=7
TMUX_TARGET_EXISTS=no
HERMES_PROCS=5
"#;

    let report = parse_probe_output(output, "missing-session:mimo-max", None)
        .expect("probe output should parse");

    assert_eq!(report.status, HermesProbeStatus::Blocked);
    assert_eq!(
        report.blocked_reason.as_deref(),
        Some("tmux target not found")
    );
}

#[test]
fn detects_smoke_success_without_storing_raw_tail_in_payload() {
    let output = r#"
HOST=hermes-nyc1
TMUX_SESSIONS=7
TMUX_TARGET_EXISTS=yes
HERMES_PROCS=5
CAPTURE_BEGIN
Health ping only.
HERMES_SUPERVISION_SMOKE_OK
CAPTURE_END
"#;

    let report = parse_probe_output(
        output,
        "hermes-evo-health-20260426:mimo-max",
        Some("HERMES_SUPERVISION_SMOKE_OK"),
    )
    .expect("probe output should parse");
    let payload = report.to_redacted_payload();
    let payload_text = payload.to_string();

    assert_eq!(report.status, HermesProbeStatus::Online);
    assert!(report.smoke_sent);
    assert_eq!(report.smoke_ok, Some(true));
    assert!(payload_text.contains("hermes-evo-health-20260426:mimo-max"));
    assert!(!payload_text.contains("Health ping only."));
    assert!(!payload_text.contains("HERMES_SUPERVISION_SMOKE_OK"));
}
