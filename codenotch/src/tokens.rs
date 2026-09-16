//! Token usage read straight from the tools' own transcripts, the way PokeTokenBar does it, and
//! nothing else: no CLI, no network. This is what the companion grows on.
//!
//!   - Claude Code: every `*.jsonl` under `~/.claude/projects` and the desktop app's session mirrors.
//!     One entry per assistant message carrying `message.usage`; the same message is logged again on
//!     resume and in sidechains, so entries are deduplicated by `message.id|requestId` keeping the one
//!     with the largest total (the completed one).
//!   - Codex: every rollout under `~/.codex/sessions`. One entry per `token_count` event, taken from
//!     `last_token_usage` (the turn's delta); a repeated identical usage state is skipped.
//!
//! Files are read incrementally: the byte offset already parsed is remembered per file, and only what
//! was appended since is read on the next pass. The per-file state is persisted beside the config so a
//! restart does not re-read a month of transcripts.

use chrono::Datelike;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager};

/// Days of history kept (the month view needs up to 31)
const KEEP_DAYS: i64 = 35;
const SCAN_EVERY_SECS: u64 = 60;
/// Window the burn rate is measured over
const BURN_WINDOW_SECS: u64 = 10 * 60;

static REFRESH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn request_refresh() {
    REFRESH.store(true, std::sync::atomic::Ordering::Relaxed);
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct Totals {
    #[serde(rename = "in")]
    pub input: u64,
    pub out: u64,
    pub cw: u64,
    pub cr: u64,
}

impl Totals {
    pub fn total(&self) -> u64 {
        self.input + self.out + self.cw + self.cr
    }
    fn add(&mut self, o: &Totals) {
        self.input += o.input;
        self.out += o.out;
        self.cw += o.cw;
        self.cr += o.cr;
    }
}

/// One API call. `id` is a hash of the vendor's own identifiers, `ts` seconds since the epoch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Entry {
    pub id: u64,
    pub ts: u64,
    pub model: String,
    pub t: Totals,
}

/// What has been read of one file so far
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct FileState {
    offset: u64,
    size: u64,
    mtime: u64,
    /// codex: the last usage state seen, to skip a repeated snapshot
    #[serde(default)]
    last_fp: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    turn: u64,
    /// id → entry, the largest total per id within this file
    entries: HashMap<u64, Entry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Cache {
    /// provider → path → state
    files: BTreeMap<String, BTreeMap<String, FileState>>,
}

/// One tool's totals over the periods the pages show
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ProviderTokens {
    pub today: Totals,
    pub week: Totals,
    pub month: Totals,
    /// YYYY-MM-DD (local) → totals, the last KEEP_DAYS days
    pub days: BTreeMap<String, Totals>,
    /// model → total tokens today
    pub models_today: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Snapshot {
    /// "claude" | "codex" → totals. A tool with no transcripts at all is absent.
    pub providers: BTreeMap<String, ProviderTokens>,
    /// Local calendar day the `today` figures belong to
    pub today: String,
    /// Tokens per minute over the last ten minutes, every tool together
    pub burn_per_min: f64,
    pub updated_at: u64,
}

impl Snapshot {
    /// provider → tokens today, the companion's input
    pub fn today_by_provider(&self) -> BTreeMap<String, u64> {
        self.providers.iter().map(|(k, v)| (k.clone(), v.today.total())).collect()
    }
}

fn now_secs() -> u64 {
    chrono::Utc::now().timestamp().max(0) as u64
}

fn fnv(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    h
}

fn cache_path() -> PathBuf {
    crate::config::config_path().with_file_name("tokens-cache.json")
}

// ---------------- roots and files ----------------

pub fn claude_roots() -> Vec<PathBuf> {
    crate::watcher::roots()
}

pub fn codex_roots() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(h) = std::env::var_os("CODEX_HOME") {
        v.push(PathBuf::from(h).join("sessions"));
    }
    if let Some(h) = dirs::home_dir() {
        v.push(h.join(".codex").join("sessions"));
    }
    v
}

/// Every transcript under `dir` touched within the kept window: (path, size, mtime secs). Audit logs
/// are left out; sub-agent transcripts are counted, since their calls are billed like any other.
fn recent_jsonl(dir: &Path, since: u64, out: &mut Vec<(PathBuf, u64, u64)>, depth: usize) {
    if depth > 9 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let Ok(md) = e.metadata() else { continue };
        if md.is_dir() {
            recent_jsonl(&p, since, out, depth + 1);
            continue;
        }
        if p.extension().map(|x| x == "jsonl").unwrap_or(false) && p.file_name().map(|n| n != "audit.jsonl").unwrap_or(false) {
            let mtime = md.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
            if mtime >= since {
                out.push((p, md.len(), mtime));
            }
        }
    }
}

// ---------------- parsing ----------------

fn parse_ts(s: &str) -> Option<u64> {
    chrono::DateTime::parse_from_rfc3339(s).ok().map(|d| d.timestamp().max(0) as u64)
}

fn n(v: &serde_json::Value, key: &str) -> u64 {
    v.get(key).and_then(|x| x.as_u64()).unwrap_or(0)
}

/// One Claude Code transcript line → an entry, when it is an assistant message with usage
pub fn parse_claude_line(line: &str) -> Option<Entry> {
    if !line.contains("\"usage\"") {
        return None;
    }
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    if v.get("type").and_then(|t| t.as_str()) != Some("assistant") {
        return None;
    }
    let msg = v.get("message")?;
    let usage = msg.get("usage")?;
    let ts = v.get("timestamp").and_then(|t| t.as_str()).and_then(parse_ts)?;
    let id = format!("{}|{}", msg.get("id").and_then(|x| x.as_str()).unwrap_or(""), v.get("requestId").and_then(|x| x.as_str()).unwrap_or(""));
    if id == "|" {
        return None;
    }
    let model = msg.get("model").and_then(|m| m.as_str()).unwrap_or("").to_string();
    Some(Entry {
        id: fnv(&id),
        ts,
        model,
        t: Totals { input: n(usage, "input_tokens"), out: n(usage, "output_tokens"), cw: n(usage, "cache_creation_input_tokens"), cr: n(usage, "cache_read_input_tokens") },
    })
}

/// One Codex rollout line. Returns the entry plus the usage-state fingerprint that a repeated
/// snapshot would share; `turn_context` lines only update the model.
fn parse_codex_line(line: &str, st: &mut FileState, path_key: &str) -> Option<Entry> {
    if line.contains("\"turn_context\"") {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(m) = v.get("payload").and_then(|p| p.get("model")).and_then(|m| m.as_str()) {
                st.model = m.to_string();
            }
        }
        return None;
    }
    if !line.contains("\"token_count\"") {
        return None;
    }
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    let payload = v.get("payload")?;
    if payload.get("type").and_then(|t| t.as_str()) != Some("token_count") {
        return None;
    }
    let ts = v.get("timestamp").and_then(|t| t.as_str()).and_then(parse_ts)?;
    let info = payload.get("info")?;
    let last = info.get("last_token_usage")?;
    let total = info.get("total_token_usage");
    let fp = format!("{}|{}", last, total.map(|t| t.to_string()).unwrap_or_default());
    if fp == st.last_fp {
        return None; // the same state logged again: no new tokens
    }
    st.last_fp = fp;
    st.turn += 1;
    let input = n(last, "input_tokens");
    let cached = n(last, "cached_input_tokens").min(input);
    Some(Entry {
        id: fnv(&format!("{path_key}#{}", st.turn)),
        ts,
        model: if st.model.is_empty() { "codex".into() } else { st.model.clone() },
        t: Totals { input: input - cached, out: n(last, "output_tokens"), cw: 0, cr: cached },
    })
}

/// Reads what was appended to `path` since the last pass into `st`
fn ingest(provider: &str, path: &Path, size: u64, mtime: u64, st: &mut FileState) {
    if size < st.offset {
        // truncated or rewritten: start over
        *st = FileState::default();
    }
    if size == st.offset {
        st.mtime = mtime;
        return;
    }
    let Ok(mut f) = std::fs::File::open(path) else { return };
    if f.seek(SeekFrom::Start(st.offset)).is_err() {
        return;
    }
    let mut buf = Vec::with_capacity((size - st.offset).min(64 << 20) as usize);
    let mut reader = std::io::BufReader::new(&mut f);
    let mut consumed = st.offset;
    let key = path.to_string_lossy().to_string();
    loop {
        buf.clear();
        let Ok(nread) = reader.read_until(b'\n', &mut buf) else { break };
        if nread == 0 {
            break;
        }
        if buf.last() != Some(&b'\n') {
            break; // a line still being written: read it whole next time
        }
        consumed += nread as u64;
        let line = String::from_utf8_lossy(&buf);
        let e = if provider == "codex" { parse_codex_line(&line, st, &key) } else { parse_claude_line(&line) };
        if let Some(e) = e {
            match st.entries.get(&e.id) {
                Some(old) if old.t.total() >= e.t.total() => {}
                _ => {
                    st.entries.insert(e.id, e);
                }
            }
        }
    }
    let _ = reader.read(&mut [0u8; 0]);
    st.offset = consumed;
    st.size = size;
    st.mtime = mtime;
}

/// Local calendar day of a timestamp
fn day_of(ts: u64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(ts as i64, 0).map(|d| d.with_timezone(&chrono::Local).date_naive().to_string()).unwrap_or_default()
}

fn aggregate(cache: &Cache, now: u64) -> Snapshot {
    let today_date = chrono::Local::now().date_naive();
    let today = today_date.to_string();
    let week_start = today_date - chrono::Duration::days(today_date.weekday().num_days_from_monday() as i64);
    let month_start = today_date.with_day(1).unwrap_or(today_date);
    let mut snap = Snapshot { today: today.clone(), updated_at: now * 1000, ..Default::default() };
    let mut burn: u64 = 0;
    for (provider, files) in &cache.files {
        // Global dedup: the same call may sit in two files (resume, sidechain)
        let mut best: HashMap<u64, &Entry> = HashMap::new();
        for st in files.values() {
            for e in st.entries.values() {
                match best.get(&e.id) {
                    Some(old) if old.t.total() >= e.t.total() => {}
                    _ => {
                        best.insert(e.id, e);
                    }
                }
            }
        }
        if best.is_empty() {
            continue;
        }
        let mut pt = ProviderTokens::default();
        for e in best.values() {
            let day = day_of(e.ts);
            if let Ok(d) = chrono::NaiveDate::parse_from_str(&day, "%Y-%m-%d") {
                if (today_date - d).num_days() > KEEP_DAYS {
                    continue;
                }
                pt.days.entry(day.clone()).or_default().add(&e.t);
                if d >= week_start && d <= today_date {
                    pt.week.add(&e.t);
                }
                if d >= month_start && d <= today_date {
                    pt.month.add(&e.t);
                }
                if day == today {
                    pt.today.add(&e.t);
                    let m = if e.model.is_empty() { provider.clone() } else { e.model.clone() };
                    *pt.models_today.entry(m).or_default() += e.t.total();
                }
            }
            if now.saturating_sub(e.ts) <= BURN_WINDOW_SECS {
                burn += e.t.total();
            }
        }
        snap.providers.insert(provider.clone(), pt);
    }
    snap.burn_per_min = burn as f64 / (BURN_WINDOW_SECS as f64 / 60.0);
    snap
}

/// One pass over every root: stat, ingest what changed, drop what left the window
fn scan(cache: &mut Cache, now: u64) -> bool {
    let since = now.saturating_sub(KEEP_DAYS as u64 * 86_400);
    let mut changed = false;
    for (provider, roots) in [("claude", claude_roots()), ("codex", codex_roots())] {
        let mut found = Vec::new();
        for r in &roots {
            recent_jsonl(r, since, &mut found, 0);
        }
        let files = cache.files.entry(provider.to_string()).or_default();
        let mut seen = std::collections::HashSet::new();
        for (p, size, mtime) in found {
            let key = p.to_string_lossy().to_string();
            seen.insert(key.clone());
            let st = files.entry(key).or_default();
            if st.size != size || st.mtime != mtime {
                ingest(provider, &p, size, mtime, st);
                changed = true;
            }
        }
        let before = files.len();
        files.retain(|k, _| seen.contains(k));
        changed |= files.len() != before;
    }
    changed
}

fn load_cache() -> Cache {
    std::fs::read_to_string(cache_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save_cache(c: &Cache) {
    if let Ok(t) = serde_json::to_string(c) {
        let _ = std::fs::write(cache_path(), t);
    }
}

pub fn probe() -> String {
    let count = |roots: Vec<PathBuf>| {
        let mut v = Vec::new();
        for r in roots {
            recent_jsonl(&r, now_secs().saturating_sub(KEEP_DAYS as u64 * 86_400), &mut v, 0);
        }
        v.len()
    };
    format!("tokens: {} Claude transcripts, {} Codex rollouts touched in the last {KEEP_DAYS} days", count(claude_roots()), count(codex_roots()))
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        crate::activity::lower_thread_priority();
        let mut cache = load_cache();
        let mut first = true;
        loop {
            let now = now_secs();
            let changed = scan(&mut cache, now);
            if changed || first {
                save_cache(&cache);
            }
            // Aggregate every pass: "today" moves at midnight and the burn window slides
            let snap = aggregate(&cache, now);
            let st = app.state::<crate::AppState>();
            let differs = { *st.tokens.lock().unwrap() != snap };
            if differs || first {
                *st.tokens.lock().unwrap() = snap.clone();
                let _ = app.emit("tokens", &snap);
            }
            first = false;
            for _ in 0..SCAN_EVERY_SECS {
                if REFRESH.swap(false, std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLAUDE_LINE: &str = r#"{"type":"assistant","uuid":"u1","timestamp":"2026-09-15T16:00:29.334Z","requestId":"req_1","message":{"id":"msg_1","model":"claude-opus-5","usage":{"input_tokens":2,"cache_creation_input_tokens":95135,"cache_read_input_tokens":10,"output_tokens":176}}}"#;

    #[test]
    fn claude_line_maps_the_four_token_kinds() {
        let e = parse_claude_line(CLAUDE_LINE).unwrap();
        assert_eq!(e.t, Totals { input: 2, out: 176, cw: 95135, cr: 10 });
        assert_eq!(e.model, "claude-opus-5");
        assert_eq!(e.id, fnv("msg_1|req_1"));
        assert!(parse_claude_line(r#"{"type":"user","message":{"usage":{}}}"#).is_none());
    }

    #[test]
    fn codex_repeated_state_is_counted_once_and_cached_input_is_split() {
        let mut st = FileState::default();
        let ctx = r#"{"timestamp":"2026-09-12T19:46:22.908Z","type":"turn_context","payload":{"model":"gpt-5.6"}}"#;
        assert!(parse_codex_line(ctx, &mut st, "f").is_none());
        let tc = r#"{"timestamp":"2026-09-12T19:46:23.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":15735,"cached_input_tokens":9984,"output_tokens":7,"total_tokens":15742},"last_token_usage":{"input_tokens":15735,"cached_input_tokens":9984,"output_tokens":7,"total_tokens":15742}}}}"#;
        let e = parse_codex_line(tc, &mut st, "f").unwrap();
        assert_eq!(e.t, Totals { input: 15735 - 9984, out: 7, cw: 0, cr: 9984 });
        assert_eq!(e.model, "gpt-5.6");
        assert!(parse_codex_line(tc, &mut st, "f").is_none(), "same snapshot again is not a new turn");
    }

    #[test]
    fn ingest_reads_incrementally_and_keeps_the_largest_total_per_id() {
        let dir = std::env::temp_dir().join(format!("notchmon-tokens-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("s.jsonl");
        std::fs::write(&p, format!("{CLAUDE_LINE}\n")).unwrap();
        let mut st = FileState::default();
        let md = std::fs::metadata(&p).unwrap();
        ingest("claude", &p, md.len(), 1, &mut st);
        assert_eq!(st.entries.len(), 1);
        assert_eq!(st.offset, md.len());
        // The same message streamed again with more output, plus a partial line without a newline
        let bigger = CLAUDE_LINE.replace("\"output_tokens\":176", "\"output_tokens\":300");
        let mut f = std::fs::OpenOptions::new().append(true).open(&p).unwrap();
        use std::io::Write;
        write!(f, "{bigger}\n{{\"type\":\"assistant\"").unwrap();
        let md = std::fs::metadata(&p).unwrap();
        ingest("claude", &p, md.len(), 2, &mut st);
        assert_eq!(st.entries.len(), 1);
        assert_eq!(st.entries.values().next().unwrap().t.out, 300);
        assert!(st.offset < md.len(), "the unfinished line is left for the next pass");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn aggregate_dedups_across_files_and_buckets_today() {
        let now = now_secs();
        let mut a = FileState::default();
        let mut b = FileState::default();
        let e = Entry { id: 7, ts: now - 60, model: "m".into(), t: Totals { input: 1, out: 2, cw: 3, cr: 4 } };
        a.entries.insert(7, e.clone());
        b.entries.insert(7, Entry { t: Totals { out: 9, ..e.t }, ..e.clone() });
        b.entries.insert(8, Entry { id: 8, ts: now - 40 * 86_400, ..e.clone() });
        let mut cache = Cache::default();
        cache.files.entry("claude".into()).or_default().insert("a".into(), a);
        cache.files.entry("claude".into()).or_default().insert("b".into(), b);
        let s = aggregate(&cache, now);
        let c = &s.providers["claude"];
        assert_eq!(c.today.total(), 1 + 9 + 3 + 4, "largest total wins across files");
        assert_eq!(c.days.len(), 1, "an entry older than the window is dropped");
        assert_eq!(c.models_today["m"], 17);
        assert!((s.burn_per_min - 1.7).abs() < 1e-9);
        assert_eq!(s.today_by_provider()["claude"], 17);
    }
}
