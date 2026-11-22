//! M0 acceptance test (SPEC.md §5): replays a recorded session in which an agent, given only
//! `alma guide` and `tests/acceptance/brief.md`, built the hybrid terminal plan with the CLI.
//!
//! The session was recorded on 2026-10-29 through a wrapper that logged every `alma` call. The
//! replay checks that every call that succeeded still succeeds, that every call that failed
//! either works now or fails with a coded diagnostic (no unexplained failures), and that the
//! final plan is byte-identical to the one the agent produced and lints cleanly.
//!
//! Since M2 the over-allocation check (L-RES-01) sees what M0 could not: the brief's team is
//! too small for its parallel work (seven developers for six, five technicians for four). The
//! replay asserts exactly those two warnings, then that `alma level` resolves them.

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/acceptance")
}

#[test]
fn recorded_agent_session_replays() {
    let work = std::env::temp_dir().join(format!("alma-acceptance-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    for f in ["brief.md", "ops1.json", "ops2.json"] {
        std::fs::copy(dir().join(f), work.join(f)).unwrap();
    }
    let session = std::fs::read_to_string(dir().join("session.jsonl")).unwrap();
    let mut replayed = 0;
    for line in session.lines() {
        let rec: Value = serde_json::from_str(line).unwrap();
        let args: Vec<String> = rec["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a.as_str().unwrap().to_string())
            .collect();
        // Batches read from stdin were not recorded; that one was a dry run.
        if args.iter().any(|a| a == "-") {
            continue;
        }
        let out = Command::new(env!("CARGO_BIN_EXE_alma"))
            .args(&args)
            .current_dir(&work)
            .env("ALMA_FILE", work.join("terminal.alma.toml"))
            .output()
            .unwrap();
        let code = out.status.code().unwrap();
        let text = String::from_utf8_lossy(&out.stdout).to_string()
            + &String::from_utf8_lossy(&out.stderr);
        if rec["exit"] == 0 {
            assert_eq!(
                code, 0,
                "{args:?} succeeded in the session but fails now:\n{text}"
            );
        } else {
            let explained = text.contains(" E-") || text.contains(" L-");
            assert!(
                code == 0 || (code == 1 && explained),
                "{args:?} fails unexplained:\n{text}"
            );
        }
        replayed += 1;
    }
    assert!(replayed >= 20);
    let produced = std::fs::read_to_string(work.join("terminal.alma.toml")).unwrap();
    let expected = std::fs::read_to_string(dir().join("expected.alma.toml")).unwrap();
    assert_eq!(produced, expected, "same commands must give the same bytes");
    let out = Command::new(env!("CARGO_BIN_EXE_alma"))
        .args(["--json", "check"])
        .env("ALMA_FILE", work.join("terminal.alma.toml"))
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["counts"]["error"].as_u64(), Some(0), "{v}");
    let warns: Vec<(&str, &str)> = v["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["severity"] == "warn")
        .map(|d| (d["code"].as_str().unwrap(), d["ids"][0].as_str().unwrap()))
        .collect();
    assert_eq!(warns, [("L-RES-01", "dev"), ("L-RES-01", "tech")], "{v}");

    // The fix the hint suggests.
    let level = Command::new(env!("CARGO_BIN_EXE_alma"))
        .args(["--json", "level"])
        .env("ALMA_FILE", work.join("terminal.alma.toml"))
        .output()
        .unwrap();
    assert!(level.status.success());
    let out = Command::new(env!("CARGO_BIN_EXE_alma"))
        .args(["--json", "check"])
        .env("ALMA_FILE", work.join("terminal.alma.toml"))
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        (v["counts"]["error"].as_u64(), v["counts"]["warn"].as_u64()),
        (Some(0), Some(0)),
        "{v}"
    );
    let _ = std::fs::remove_dir_all(&work);
}
