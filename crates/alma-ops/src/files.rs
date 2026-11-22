//! File access shared by the CLI and the MCP server: finding the plan, reading old versions,
//! atomic writes and parsing operation batches.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::mutate::Op;

/// The usage guide for LLM agents (`alma guide`, MCP tool `guide`).
pub const GUIDE: &str = include_str!("../../../docs/agent-guide.md");

/// Finds the plan file: `explicit`, else `$ALMA_FILE`, else the only `*.alma.toml` in `dir`.
/// Relative paths are taken from `dir`.
pub fn resolve_plan(explicit: Option<&Path>, dir: &Path) -> Result<PathBuf, String> {
    let rel = |p: PathBuf| if p.is_absolute() { p } else { dir.join(p) };
    if let Some(f) = explicit {
        return Ok(rel(f.to_path_buf()));
    }
    if let Ok(f) = std::env::var("ALMA_FILE") {
        return Ok(rel(PathBuf::from(f)));
    }
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("cannot read {}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.to_string_lossy().ends_with(".alma.toml"))
        .collect();
    found.sort();
    match found.len() {
        1 => Ok(found.remove(0)),
        0 => Err(
            "no plan file: pass a file, set ALMA_FILE, or work in a directory with one *.alma.toml"
                .into(),
        ),
        _ => Err(format!(
            "several plan files here ({}); say which one",
            found
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// Reads a text file with a readable error.
pub fn read_text(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))
}

/// The text of `path` at a git revision (`git show REV:./NAME`).
pub fn git_show(path: &Path, rev: &str) -> Result<String, String> {
    let dir = parent_dir(path);
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("{} is not a file name", path.display()))?;
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .arg("show")
        .arg(format!("{rev}:./{name}"))
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git cannot show {name} at {rev}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    String::from_utf8(out.stdout).map_err(|_| "the old version is not UTF-8".into())
}

fn parent_dir(path: &Path) -> &Path {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}

/// A temporary file name next to `path`, unique within this process.
pub(crate) fn temp_next_to(path: &Path) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("alma");
    parent_dir(path).join(format!(".{name}.tmp-{}-{n}", std::process::id()))
}

/// Writes bytes through a temporary file in the same directory and a rename.
pub fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = temp_next_to(path);
    std::fs::write(&tmp, bytes)
        .and_then(|()| std::fs::rename(&tmp, path))
        .map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            format!("cannot write {}: {e}", path.display())
        })
}

/// Parses a JSON array of operations, or one operation per line (JSON lines).
pub fn parse_ops(text: &str) -> Result<Vec<Op>, String> {
    let trimmed = text.trim_start();
    if trimmed.starts_with('[') {
        return serde_json::from_str(trimmed).map_err(|e| format!("invalid operations: {e}"));
    }
    text.lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
        .map(|(n, l)| serde_json::from_str(l).map_err(|e| format!("line {}: {e}", n + 1)))
        .collect()
}

/// Today's date (UTC), for records that carry the day they were made.
pub fn today() -> jiff::civil::Date {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    alma_core::time::date_of_day(secs.div_euclid(86_400))
}
