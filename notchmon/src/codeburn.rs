//! CodeBurn: API-equivalent spend, read by running the local `codeburn` CLI (npm) and nothing else.
//!
//! Three commands, all local (codeburn reads the tools' own transcripts):
//!   - `export --format json` over yesterday and today: per-call `records` with a timestamp and a cost,
//!     summed over the last 24 rolling hours for the pill's NOVARC cell
//!   - `status --format menubar-json --period <p> [--provider <x>]`: codeburn's own analysis of a period,
//!     passed through to the page trimmed to `current` and the last 14 days
//!   - `sessions --period lifetime --format json`: every session, aggregated here per day and tool for the
//!     activity grid and its per-day popup
//!
//! One worker thread runs them in turn, hidden, each with a timeout; node is started directly rather than
//! through npm's `codeburn.cmd` so a timeout kills the process doing the work, not a cmd.exe above it.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// "all" is codeburn without `--provider`: every tool it knows, summed
pub const PROVIDERS: [&str; 3] = ["all", "claude", "codex"];
pub const PERIODS: [&str; 4] = ["today", "week", "30days", "month"];
const FAST_EVERY_SECS: u64 = 10 * 60;
const SLOW_EVERY_SECS: u64 = 60 * 60;
const RUN_TIMEOUT_SECS: u64 = 120;

static REFRESH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn request_refresh() {
    REFRESH.store(true, std::sync::atomic::Ordering::Relaxed);
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Last24h {
    pub total: f64,
    pub by_provider: BTreeMap<String, f64>,
    /// ms epoch the window ended at
    pub at: u64,
}

/// One day of sessions for one tool (or "all"). Short keys: the page receives every day of every tool
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Day {
    /// sessions, and of them per tool (only filled under "all")
    pub s: u64,
    pub by: BTreeMap<String, u64>,
    #[serde(rename = "in")]
    pub input: u64,
    pub out: u64,
    pub cr: u64,
    pub cw: u64,
    /// cost, calls, turns
    pub c: f64,
    pub k: u64,
    pub t: u64,
    /// top projects by cost, and the models used
    pub p: Vec<(String, f64)>,
    pub m: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Snapshot {
    /// false = no codeburn found: the page leaves every Consumo surface out
    pub available: bool,
    pub last24h: Option<Last24h>,
    /// "<provider>:<period>" → { generated, current, daily } straight from codeburn
    pub periods: BTreeMap<String, serde_json::Value>,
    /// provider → date (YYYY-MM-DD, local) → day
    pub days: BTreeMap<String, BTreeMap<String, Day>>,
    pub updated_at: u64,
}

fn now_ms() -> u64 {
    chrono::Utc::now().timestamp_millis().max(0) as u64
}

fn store_path() -> PathBuf {
    crate::config::config_path().with_file_name("codeburn.json")
}

pub fn load_persisted() -> Snapshot {
    std::fs::read_to_string(store_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

// ---------------- running the CLI ----------------

/// node, and codeburn's entry script, from npm's global install (or PATH for node)
fn find_cli() -> Option<(PathBuf, PathBuf)> {
    let npm = dirs::config_dir()?.join("npm");
    let entry = npm.join("node_modules").join("codeburn").join("dist").join("cli.js");
    if !entry.is_file() {
        return None;
    }
    let beside = npm.join("node.exe");
    let node = if beside.is_file() {
        beside
    } else {
        std::env::split_paths(&std::env::var_os("PATH")?).map(|d| d.join("node.exe")).find(|p| p.is_file())?
    };
    Some((node, entry))
}

pub fn probe() -> String {
    match find_cli() {
        Some((node, entry)) => format!("codeburn: {} via {}", entry.display(), node.display()),
        None => "codeburn: not installed (npm i -g codeburn); the Consumo surfaces stay hidden".into(),
    }
}

/// Stdout of one codeburn run, or None on a failed start, a non-zero exit or the timeout. Stdout is
/// drained on its own thread: a sessions report is hundreds of kB and would fill the pipe otherwise.
fn run(args: &[&str]) -> Option<Vec<u8>> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let (node, entry) = find_cli()?;
    let mut cmd = Command::new(node);
    cmd.arg(entry).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut v = Vec::new();
        let _ = stdout.read_to_end(&mut v);
        v
    });
    let deadline = std::time::Instant::now() + Duration::from_secs(RUN_TIMEOUT_SECS);
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) if std::time::Instant::now() < deadline => std::thread::sleep(Duration::from_millis(200)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                crate::applog(&format!("codeburn: {args:?} timed out after {RUN_TIMEOUT_SECS}s"));
                break None;
            }
        }
    };
    let out = reader.join().ok()?;
    status.filter(|s| s.success()).map(|_| out)
}

// ---------------- the three readings ----------------

fn parse_ts(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s).ok().map(|d| d.timestamp_millis())
}

/// The last 24 rolling hours out of an export's per-call records
fn last24h_from(export: &serde_json::Value, now: u64) -> Option<Last24h> {
    let records = export.get("records")?.as_array()?;
    let cut = now as i64 - 24 * 3600 * 1000;
    let mut by_provider: BTreeMap<String, f64> = BTreeMap::new();
    for r in records {
        let Some(t) = r.get("timestamp").and_then(|v| v.as_str()).and_then(parse_ts) else { continue };
        if t < cut || t > now as i64 {
            continue;
        }
        let cost = r.get("cost").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let provider = r.get("provider").and_then(|v| v.as_str()).unwrap_or("other");
        *by_provider.entry(provider.to_string()).or_default() += cost;
    }
    Some(Last24h { total: by_provider.values().sum(), by_provider, at: now })
}

fn read_last24h() -> Option<Last24h> {
    let today = chrono::Local::now().date_naive();
    let yesterday = today.pred_opt()?;
    let file = std::env::temp_dir().join(format!("codenotch-codeburn-{}.json", std::process::id()));
    let (from, to, out) = (yesterday.to_string(), today.to_string(), file.to_string_lossy().to_string());
    run(&["export", "--format", "json", "--from", &from, "--to", &to, "-o", &out])?;
    let text = std::fs::read(&file).ok();
    let _ = std::fs::remove_file(&file);
    last24h_from(&serde_json::from_slice(&text?).ok()?, now_ms())
}

/// codeburn's status for one period, trimmed to what the page draws: the analysis and the last 14 days
fn trim_status(v: &serde_json::Value) -> Option<serde_json::Value> {
    let current = v.get("current")?.clone();
    let daily: Vec<serde_json::Value> = v
        .pointer("/history/daily")
        .and_then(|d| d.as_array())
        .map(|a| {
            a.iter()
                .rev()
                .take(14)
                .rev()
                .map(|d| serde_json::json!({ "date": d.get("date"), "cost": d.get("cost"), "calls": d.get("calls") }))
                .collect()
        })
        .unwrap_or_default();
    Some(serde_json::json!({ "generated": v.get("generated"), "current": current, "daily": daily }))
}

fn read_status(provider: &str, period: &str) -> Option<serde_json::Value> {
    let mut args = vec!["status", "--format", "menubar-json", "--period", period, "--no-optimize"];
    if provider != "all" {
        args.extend(["--provider", provider]);
    }
    trim_status(&serde_json::from_slice(&run(&args)?).ok()?)
}

/// `C--dev-Projects-Kidsy-Hub` (Claude's project slug) or a path, as a name a person would say
fn project_name(raw: &str) -> String {
    let tail = raw.rsplit(['\\', '/']).next().unwrap_or(raw);
    let tail = tail.rsplit("Projects-").next().unwrap_or(tail);
    let name = tail.replace('-', " ");
    if name.trim().is_empty() { raw.to_string() } else { name.trim().to_string() }
}

fn days_from(sessions: &[serde_json::Value]) -> BTreeMap<String, BTreeMap<String, Day>> {
    #[derive(Default)]
    struct Acc {
        day: Day,
        projects: BTreeMap<String, f64>,
        models: BTreeMap<String, u64>,
    }
    let mut acc: BTreeMap<(String, String), Acc> = BTreeMap::new();
    for r in sessions {
        let Some(date) = r
            .get("startedAt")
            .and_then(|v| v.as_str())
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|d| d.with_timezone(&chrono::Local).date_naive().to_string())
        else {
            continue;
        };
        let provider = r.get("provider").and_then(|v| v.as_str()).unwrap_or("other").to_string();
        let n = |k: &str| r.get(k).and_then(|v| v.as_u64()).unwrap_or(0);
        let cost = r.get("cost").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let project = r.get("project").and_then(|v| v.as_str()).map(project_name).unwrap_or_default();
        let models: Vec<String> = r
            .get("models")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|m| m.as_str()).filter(|m| !m.starts_with('<')).map(str::to_string).collect())
            .unwrap_or_default();
        for key in [provider.clone(), "all".to_string()] {
            let a = acc.entry((key, date.clone())).or_default();
            a.day.s += 1;
            *a.day.by.entry(provider.clone()).or_default() += 1;
            a.day.input += n("inputTokens");
            a.day.out += n("outputTokens");
            a.day.cr += n("cacheReadTokens");
            a.day.cw += n("cacheWriteTokens");
            a.day.c += cost;
            a.day.k += n("calls");
            a.day.t += n("turns");
            *a.projects.entry(project.clone()).or_default() += cost;
            for m in &models {
                *a.models.entry(m.clone()).or_default() += 1;
            }
        }
    }
    let mut out: BTreeMap<String, BTreeMap<String, Day>> = BTreeMap::new();
    for ((provider, date), mut a) in acc {
        let mut projects: Vec<(String, f64)> = a.projects.into_iter().filter(|(n, _)| !n.is_empty()).collect();
        projects.sort_by(|x, y| y.1.total_cmp(&x.1));
        projects.truncate(3);
        let mut models: Vec<(String, u64)> = a.models.into_iter().collect();
        models.sort_by_key(|m| std::cmp::Reverse(m.1));
        a.day.p = projects;
        a.day.m = models.into_iter().take(3).map(|(n, _)| n).collect();
        out.entry(provider).or_default().insert(date, a.day);
    }
    out
}

fn read_days() -> Option<BTreeMap<String, BTreeMap<String, Day>>> {
    let v: serde_json::Value = serde_json::from_slice(&run(&["sessions", "--period", "lifetime", "--format", "json"])?).ok()?;
    Some(days_from(v.as_array()?))
}

// ---------------- the worker ----------------

fn publish(app: &AppHandle, mutate: impl FnOnce(&mut Snapshot)) {
    let st = app.state::<crate::AppState>();
    let snap = {
        let mut s = st.codeburn.lock().unwrap();
        mutate(&mut s);
        s.updated_at = now_ms();
        s.clone()
    };
    if let Ok(t) = serde_json::to_string(&snap) {
        let _ = std::fs::write(store_path(), t);
    }
    let _ = app.emit("codeburn", &snap);
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        let available = find_cli().is_some();
        publish(&app, |s| s.available = available);
        if !available {
            return;
        }
        let mut last_slow: Option<std::time::Instant> = None;
        loop {
            // Fast: what moves by the minute
            if let Some(l) = read_last24h() {
                publish(&app, |s| s.last24h = Some(l));
            }
            for p in PROVIDERS {
                if let Some(v) = read_status(p, "today") {
                    publish(&app, |s| {
                        s.periods.insert(format!("{p}:today"), v);
                    });
                }
            }
            // Slow: longer periods and the whole history, hourly
            if last_slow.map(|t| t.elapsed().as_secs() >= SLOW_EVERY_SECS).unwrap_or(true) {
                last_slow = Some(std::time::Instant::now());
                if let Some(d) = read_days() {
                    publish(&app, |s| s.days = d);
                }
                for period in &PERIODS[1..] {
                    for p in PROVIDERS {
                        if let Some(v) = read_status(p, period) {
                            publish(&app, |s| {
                                s.periods.insert(format!("{p}:{period}"), v);
                            });
                        }
                    }
                }
            }
            for _ in 0..FAST_EVERY_SECS {
                if REFRESH.swap(false, std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last24h_sums_only_the_rolling_window_per_tool() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-09-15T21:00:00Z").unwrap().timestamp_millis() as u64;
        let export = serde_json::json!({ "records": [
            { "timestamp": "2026-09-15T20:00:00Z", "provider": "claude", "cost": 10.0 },
            { "timestamp": "2026-09-14T21:30:00Z", "provider": "codex", "cost": 2.5 },
            { "timestamp": "2026-09-14T20:59:59Z", "provider": "claude", "cost": 99.0 },
            { "timestamp": "2026-09-15T22:00:00Z", "provider": "claude", "cost": 50.0 },
            { "provider": "claude", "cost": 7.0 }
        ]});
        let l = last24h_from(&export, now).unwrap();
        assert_eq!(l.total, 12.5);
        assert_eq!(l.by_provider.get("claude"), Some(&10.0));
        assert_eq!(l.by_provider.get("codex"), Some(&2.5));
    }

    #[test]
    fn days_group_per_tool_and_under_all() {
        let s = serde_json::json!([
            { "startedAt": "2026-09-14T15:00:00Z", "provider": "claude", "project": "C--dev-Projects-Kidsy-Hub", "models": ["Fable 5", "<synthetic>"],
              "cost": 10.0, "calls": 5, "turns": 2, "inputTokens": 100, "outputTokens": 10, "cacheReadTokens": 1000, "cacheWriteTokens": 1 },
            { "startedAt": "2026-09-14T16:00:00Z", "provider": "codex", "project": "C:\\dev\\Projects\\Acme App", "models": ["GPT-5.6 Sol"],
              "cost": 1.0, "calls": 1, "turns": 1, "inputTokens": 1, "outputTokens": 1, "cacheReadTokens": 0, "cacheWriteTokens": 0 }
        ]);
        let d = days_from(s.as_array().unwrap());
        let date = chrono::DateTime::parse_from_rfc3339("2026-09-14T15:00:00Z").unwrap().with_timezone(&chrono::Local).date_naive().to_string();
        let all = &d["all"][&date];
        assert_eq!((all.s, all.k, all.c), (2, 6, 11.0));
        assert_eq!(all.by.get("codex"), Some(&1));
        assert_eq!(all.p[0], ("Kidsy Hub".to_string(), 10.0));
        assert_eq!(d["claude"][&date].m, vec!["Fable 5".to_string()]);
        assert_eq!(d["codex"][&date].p[0].0, "Acme App");
    }

    #[test]
    fn status_is_trimmed_to_the_last_14_days() {
        let daily: Vec<_> = (1..=20).map(|i| serde_json::json!({ "date": format!("d{i}"), "cost": i, "calls": i, "inputTokens": 9 })).collect();
        let t = trim_status(&serde_json::json!({ "generated": "g", "current": { "cost": 1 }, "history": { "daily": daily } })).unwrap();
        let d = t["daily"].as_array().unwrap();
        assert_eq!((d.len(), d[0]["date"].as_str(), d[13]["date"].as_str()), (14, Some("d7"), Some("d20")));
        assert!(d[0].get("inputTokens").is_none());
    }
}
