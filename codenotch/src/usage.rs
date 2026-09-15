//! Claude usage adapter (official), implemented from the upstream Codenotch's documented behaviour.
//! Endpoint: GET https://api.anthropic.com/api/oauth/usage
//! Headers: Authorization: Bearer <token>; anthropic-beta: oauth-2025-04-20; 15 s timeout
//! Rules (upstream's discipline):
//!   - the credential comes from Claude Code's own store (Windows: ~/.claude/.credentials.json), read only
//!   - 401/403 → re-read the credential once and retry (Claude Code may have just refreshed the token) → still failing means needsAuth
//!   - 429 → back off 60 s × 2^n capped at 15 min, Retry-After only raises it, even past the cap; the deadline is persisted
//!   - an expired token is never sent: the endpoint answers it with 429 + Retry-After ≈ 3600, not 401, so sending it
//!     reads as "rate limited" for as long as the token stays stale (upstream's credentialExpired, no network)
//!   - the token is renewed by running the standalone `claude -p` with an empty stdin shortly before it expires
//!     (upstream's ClaudeTokenRefresher). Only that CLI writes ~/.claude/.credentials.json — Claude Code inside the
//!     desktop app renews its own copy elsewhere — so without this the file rots eight hours after the last CLI run
//!   - never invent a percentage on failure: keep the last reading marked stale, and the UI shows how old it is
//!
//! Reply (snake_case): { limits:[{kind,percent,resets_at}], five_hour:{utilization,resets_at}, seven_day:{...} }
//! limits is the forward-compatible main shape; five_hour/seven_day are merged in as a fallback (a window that just rolled over disappears from limits).

use crate::AppState;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};

const ENDPOINT: &str = "https://api.anthropic.com/api/oauth/usage";
const POLL_ACTIVE_SECS: u64 = 60;
const POLL_IDLE_SECS: u64 = 300;
const BACKOFF_BASE_SECS: u64 = 60;
const BACKOFF_CAP_SECS: u64 = 900;
/// Renew when this close to expiry. Must stay under Claude Code's own five minutes: its start-up renews the token
/// only when now + 300 s >= expiresAt, so launching any earlier is a no-op that would be judged a failure
const RENEW_MARGIN_MS: u64 = 4 * 60 * 1000;
const RENEW_COOLDOWN_MS: u64 = 10 * 60 * 1000;
const RENEW_TIMEOUT_SECS: u64 = 30;
const EXPIRED_NOTE: &str = "Credential expired — run claude once in a terminal to renew it";

static REFRESH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Immediate refresh from the tray or a command
pub fn request_refresh() {
    REFRESH.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Sleep in slices so request_refresh can interrupt it
fn sleep_interruptible(total_secs: u64) {
    for _ in 0..total_secs {
        if REFRESH.swap(false, std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LimitWindow {
    pub id: String,
    pub label: String,
    /// 0.0–1.0 (fraction used)
    pub used: f64,
    /// Reset time, ms epoch (None = unknown)
    pub resets_at: Option<u64>,
    /// Pure count window (no published denominator, e.g. Antigravity's requests today) — the cell shows ~N and the ring draws only its track
    #[serde(default)]
    pub count: Option<i64>,
    /// The number is ours, not the vendor's (upstream fidelity=.derived) — the card adds a ~ prefix
    #[serde(default)]
    pub derived: bool,
    /// The heading the window sits under on the card, for a provider that reports the same windows
    /// for several things (Antigravity: a 5-hour and a weekly lane per model family). None = ungrouped
    #[serde(default)]
    pub group: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UsageSnapshot {
    /// ok | stale | needsAuth | backoff | error
    pub status: String,
    pub windows: Vec<LimitWindow>,
    pub fetched_at: u64,
    pub note: String,
    #[serde(default)]
    pub backoff_until: u64,
    /// "desktop" when the windows came from Claude Desktop's cache. `fetched_at` is then the response's own
    /// date and lags by Desktop's refresh interval, so the page judges staleness by `status` alone
    #[serde(default)]
    pub source: String,
}

/// A Claude account Desktop is signed into besides the one in the `claude` cell. Its limits come from
/// Desktop's cache only — there is no credential for it here, so it is never fetched
#[derive(Debug, Clone, Serialize)]
pub struct ClaudeAccount {
    /// `claude:<first 8 of the organization>`
    pub id: String,
    pub name: String,
    pub snap: UsageSnapshot,
}

/// Desktop refreshes its usage every few minutes while it runs; a reading this recent is taken as live and
/// the network is not asked. Past it, the OAuth path runs as before
const CACHE_FRESH_MS: u64 = 30 * 60 * 1000;
const CACHE_NOTE: &str = "From Claude Desktop";

/// The organization the CLI credential belongs to, so Desktop's reading lands on the right ring
// ponytail: Claude Code rewrites this file often; a torn read is None for one poll, and the ring falls back to the first org
fn cli_org() -> Option<String> {
    let text = std::fs::read_to_string(dirs::home_dir()?.join(".claude.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    v.pointer("/oauthAccount/organizationUuid")?.as_str().map(str::to_string)
}

/// The reading that feeds the `claude` cell, and the others. The CLI's organization when there is one,
/// otherwise the first in the cache — sorted by organization, so the rings never swap places
fn split_readings(
    readings: Vec<crate::desktop_cache::Reading>,
    cli_org: Option<&str>,
) -> (Option<crate::desktop_cache::Reading>, Vec<crate::desktop_cache::Reading>) {
    let primary = cli_org.map(str::to_string).or_else(|| readings.first().map(|r| r.org.clone()));
    let (mine, rest): (Vec<_>, Vec<_>) = readings.into_iter().partition(|r| Some(&r.org) == primary.as_ref());
    (mine.into_iter().next(), rest)
}

/// A cached reading as a cell shows it. A window whose reset has passed since has rolled over and its
/// number with it: ~0 with no reset time, rather than an old percentage for a window that already ended
fn snapshot_from(r: &crate::desktop_cache::Reading, now: u64) -> UsageSnapshot {
    let windows = r
        .windows
        .iter()
        .cloned()
        .map(|mut w| {
            if w.resets_at.is_some_and(|t| t <= now) {
                w.used = 0.0;
                w.resets_at = None;
                w.derived = true;
            }
            w
        })
        .collect();
    UsageSnapshot {
        status: if now.saturating_sub(r.captured_at) < CACHE_FRESH_MS { "ok" } else { "stale" }.into(),
        windows,
        fetched_at: r.captured_at,
        note: CACHE_NOTE.into(),
        backoff_until: 0,
        source: "desktop".into(),
    }
}

fn publish_accounts(app: &AppHandle, extras: &[crate::desktop_cache::Reading]) {
    let st = app.state::<AppState>();
    let names = st.cfg.lock().unwrap().claude_names.clone();
    let now = now_ms();
    let list: Vec<ClaudeAccount> = extras
        .iter()
        .map(|r| {
            let short = r.org.get(..8).unwrap_or(&r.org);
            ClaudeAccount {
                id: format!("claude:{short}"),
                name: names.get(&r.org).cloned().unwrap_or_else(|| format!("Claude ({short})")),
                snap: snapshot_from(r, now),
            }
        })
        .collect();
    *st.claude_accounts.lock().unwrap() = list.clone();
    let _ = app.emit("claude_accounts", &list);
}

fn store_path() -> std::path::PathBuf {
    crate::config::config_path().with_file_name("usage.json")
}

pub fn load_persisted() -> UsageSnapshot {
    std::fs::read_to_string(store_path())
        .ok()
        .and_then(|t| serde_json::from_str::<UsageSnapshot>(&t).ok())
        .map(|mut s| {
            if !s.windows.is_empty() {
                s.status = "stale".into(); // an old reading after a restart is labelled as such
            }
            s
        })
        .unwrap_or_default()
}

fn persist(s: &UsageSnapshot) {
    if let Ok(t) = serde_json::to_string_pretty(s) {
        let _ = std::fs::write(store_path(), t);
    }
}

struct Credential {
    token: String,
    /// ms epoch (None = the file names no expiry)
    expires_at: Option<u64>,
}

impl Credential {
    fn expired(&self, now: u64) -> bool {
        self.expires_at.map(|e| e <= now).unwrap_or(false)
    }
}

/// Reads Claude Code's OAuth credential.
fn read_credentials() -> Option<Credential> {
    let home = dirs::home_dir()?;
    for name in [".credentials.json", "credentials.json"] {
        let p = home.join(".claude").join(name);
        let Ok(text) = std::fs::read_to_string(&p) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let oauth = v.get("claudeAiOauth").unwrap_or(&v);
        if let Some(tok) = oauth.get("accessToken").and_then(|x| x.as_str()) {
            let expires_at = oauth.get("expiresAt").and_then(|x| x.as_f64()).map(|ms| ms as u64);
            return Some(Credential { token: tok.to_string(), expires_at });
        }
    }
    None
}

/// For doctor: credential probe report (prints no secret values)
pub fn probe_credentials() -> String {
    let cli = match find_cli() {
        Some(p) => format!("renews via {}", p.display()),
        None => "no standalone claude CLI found to renew it".into(),
    };
    match read_credentials() {
        Some(c) => format!(
            "credential: found (token {} chars, {}; {cli})",
            c.token.len(),
            if c.expired(now_ms()) { "expired" } else { "valid" }
        ),
        None => "credential: ~/.claude/.credentials.json not found (needsAuth; the desktop app may use another store — signing in once with the Claude Code CLI creates it)".into(),
    }
}

// ---------------- token renewal (upstream's ClaudeTokenRefresher) ----------------

/// Anything under these belongs to the desktop app: its bundled Claude Code keeps its token in the desktop app's
/// own store and never writes ~/.claude/.credentials.json, so renewing with it would change nothing here
fn is_desktop_owned(p: &std::path::Path) -> bool {
    let s = p.to_string_lossy().to_ascii_lowercase().replace('/', "\\");
    s.contains("\\anthropicclaude\\") || s.contains("\\claude\\claude-code\\") || s.contains("\\windowsapps\\")
}

/// The standalone Claude Code command: its own installer's location first, then global npm/pnpm/Volta, then PATH
fn find_cli() -> Option<std::path::PathBuf> {
    let mut v = Vec::new();
    if let Some(h) = dirs::home_dir() {
        v.push(h.join(".local").join("bin").join("claude.exe"));
    }
    if let Some(d) = dirs::config_dir() {
        v.push(d.join("npm").join("claude.cmd"));
    }
    if let Some(d) = dirs::data_local_dir() {
        v.push(d.join("pnpm").join("claude.cmd"));
    }
    if let Some(h) = dirs::home_dir() {
        v.push(h.join(".volta").join("bin").join("claude.exe"));
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            v.push(dir.join("claude.exe"));
            v.push(dir.join("claude.cmd"));
        }
    }
    v.into_iter().find(|p| p.is_file() && !is_desktop_owned(p))
}

/// Whether a launch is worth making. Pure, so every branch is testable without a clock or a subprocess
fn should_renew(expires_at: Option<u64>, now: u64, attempted_for: Option<u64>, last_attempt: Option<u64>) -> bool {
    // Nothing read yet: never launch on a guess
    let Some(exp) = expires_at else { return false };
    // Plenty of time left — also where launching would do nothing, because the CLI's own gate has not opened
    if exp > now + RENEW_MARGIN_MS {
        return false;
    }
    // One attempt per token: a launch that failed to move the expiry leaves the same value here, and never runs again
    if attempted_for == Some(exp) {
        return false;
    }
    if let Some(t) = last_attempt {
        if now.saturating_sub(t) < RENEW_COOLDOWN_MS {
            return false;
        }
    }
    true
}

/// `claude -p` with a null stdin starts up (which is where it renews an aged token), then exits non-zero for want
/// of a prompt: no conversation, no transcript. Output goes nowhere — a token could in principle be echoed into it.
fn run_renewal(cli: &std::path::Path) -> std::io::Result<()> {
    use std::process::{Command, Stdio};
    let mut cmd = Command::new(cli);
    cmd.arg("-p").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    // Launched from inside a Claude Code session, the child would take the host's auth and leave the file alone
    for (k, _) in std::env::vars_os() {
        let k = k.to_string_lossy();
        if k == "CLAUDECODE" || k.starts_with("CLAUDE_CODE_") {
            cmd.env_remove(k.as_ref());
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd.spawn()?;
    let deadline = std::time::Instant::now() + Duration::from_secs(RENEW_TIMEOUT_SECS);
    while child.try_wait()?.is_none() {
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}

#[derive(Default)]
struct Renewer {
    attempted_for: Option<u64>,
    last_attempt: Option<u64>,
}

impl Renewer {
    /// Renews if the token is about to expire. Some(true) = the expiry moved; judged on the outcome, never on the
    /// exit status, because refusing the empty prompt is a non-zero exit and a successful renewal at the same time
    fn maybe_renew(&mut self, cred: &Credential) -> Option<bool> {
        let now = now_ms();
        if !should_renew(cred.expires_at, now, self.attempted_for, self.last_attempt) {
            return None;
        }
        self.last_attempt = Some(now);
        self.attempted_for = cred.expires_at;
        let Some(cli) = find_cli() else {
            crate::applog("claude: token about to expire and no standalone claude CLI found to renew it");
            return Some(false);
        };
        if let Err(e) = run_renewal(&cli) {
            crate::applog(&format!("claude: token renewal could not start ({}): {e}", cli.display()));
            return Some(false);
        }
        let after = read_credentials().and_then(|c| c.expires_at);
        let renewed = matches!((after, cred.expires_at), (Some(a), Some(b)) if a > b);
        crate::applog(&if renewed {
            format!("claude: token renewed via {}", cli.display())
        } else {
            format!("claude: ran {} but the token expiry did not move", cli.display())
        });
        Some(renewed)
    }
}

fn parse_reset(v: &serde_json::Value) -> Option<u64> {
    v.as_str()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.timestamp_millis().max(0) as u64)
}

fn label_for(kind: &str) -> String {
    match kind {
        "session" => "Current session".into(),
        "seven_day" | "weekly_all" => "Weekly (all models)".into(),
        "seven_day_opus" | "weekly_opus" => "Weekly (Opus)".into(),
        "weekly_scoped" => "Weekly (model-scoped)".into(),
        other => {
            // Forward compatibility: an unknown kind gets a readable label
            let mut s = other.replace('_', " ");
            if let Some(c) = s.get_mut(0..1) {
                c.make_ascii_uppercase();
            }
            s
        }
    }
}

pub(crate) fn parse_response(v: &serde_json::Value) -> Vec<LimitWindow> {
    let mut out: Vec<LimitWindow> = Vec::new();
    if let Some(arr) = v.get("limits").and_then(|x| x.as_array()) {
        for l in arr {
            let Some(kind) = l.get("kind").and_then(|x| x.as_str()) else {
                continue;
            };
            let Some(pct) = l.get("percent").and_then(|x| x.as_f64()) else {
                continue;
            };
            let resets = l.get("resets_at").and_then(parse_reset);
            if resets.is_none() {
                continue; // upstream rule: a window without a reset time is not shown
            }
            out.push(LimitWindow {
                id: kind.to_string(),
                label: label_for(kind),
                used: (pct / 100.0).clamp(0.0, 1.0),
                resets_at: resets, ..Default::default()
            });
        }
    }
    // Fallback merge: a window that just rolled over disappears from limits while the named field remains.
    // In practice the kinds in limits are weekly_all/weekly_scoped, not seven_day — deduplicating by id
    // alone would add the seven_day fallback a second time (the card showed "Weekly all" and
    // "Weekly (all models)" as twins). Three dedupe rules: id alias / same resets_at and percentage / same label.
    let aliases: [(&str, &str, &[&str]); 2] = [
        ("five_hour", "session", &["session", "five_hour"]),
        ("seven_day", "seven_day", &["seven_day", "weekly_all", "weekly"]),
    ];
    for (field, id, alias) in aliases {
        let Some(w) = v.get(field) else { continue };
        let Some(u) = w.get("utilization").and_then(|x| x.as_f64()) else { continue };
        let used = (u / 100.0).clamp(0.0, 1.0);
        let resets_at = w.get("resets_at").and_then(parse_reset);
        let label = label_for(id);
        let dup = out.iter().any(|x| {
            alias.contains(&x.id.as_str())
                || x.label == label
                || (resets_at.is_some()
                    && x.resets_at.map(|r| r / 1000) == resets_at.map(|r| r / 1000)
                    && (x.used - used).abs() < 0.005)
        });
        if dup {
            continue;
        }
        out.push(LimitWindow { id: id.into(), label, used, resets_at, ..Default::default() });
    }
    // session always comes first (upstream display order)
    out.sort_by_key(|w| if w.id == "session" { 0 } else { 1 });
    out
}

enum FetchErr {
    NeedsAuth,
    RateLimited(u64), // suggested wait in seconds (the Retry-After before the floor is applied)
    Other(String),
}

fn fetch_once(token: &str) -> Result<Vec<LimitWindow>, FetchErr> {
    let resp = ureq::get(ENDPOINT)
        .set("Authorization", &format!("Bearer {token}"))
        .set("anthropic-beta", "oauth-2025-04-20")
        .timeout(Duration::from_secs(15))
        .call();
    match resp {
        Ok(r) => {
            let v: serde_json::Value = r
                .into_json()
                .map_err(|e| FetchErr::Other(format!("parse: {e}")))?;
            Ok(parse_response(&v))
        }
        Err(ureq::Error::Status(401, _)) | Err(ureq::Error::Status(403, _)) => {
            Err(FetchErr::NeedsAuth)
        }
        Err(ureq::Error::Status(429, r)) => {
            let ra = r
                .header("retry-after")
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            Err(FetchErr::RateLimited(ra))
        }
        Err(ureq::Error::Status(code, _)) => Err(FetchErr::Other(format!("HTTP {code}"))),
        Err(e) => Err(FetchErr::Other(format!("{e}"))),
    }
}

fn backoff_secs(consecutive: u32, retry_after_floor: u64) -> u64 {
    let exp = BACKOFF_BASE_SECS.saturating_mul(1u64 << consecutive.min(4));
    // The server's Retry-After is honoured in full: with expired tokens no longer
    // sent, a long one is a real rate limit, and retrying early only earns another.
    exp.clamp(BACKOFF_BASE_SECS, BACKOFF_CAP_SECS).max(retry_after_floor)
}

fn set_and_broadcast(app: &AppHandle, mutate: impl FnOnce(&mut UsageSnapshot)) {
    let st = app.state::<AppState>();
    let snap = {
        let mut u = st.usage.lock().unwrap();
        mutate(&mut u);
        u.clone()
    };
    persist(&snap);
    let _ = app.emit("usage", &snap);
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        // Broadcast the persisted old reading at startup (stale beats blank)
        {
            let st = app.state::<AppState>();
            let snap = st.usage.lock().unwrap().clone();
            let _ = app.emit("usage", &snap);
        }
        let mut consecutive_429: u32 = 0;
        let mut renewer = Renewer::default();
        loop {
            // Desktop's cache first: free, and the only source for an account the CLI is not signed into
            let (primary, extras) = split_readings(crate::desktop_cache::read_all(), cli_org().as_deref());
            publish_accounts(&app, &extras);
            if let Some(r) = primary {
                let now = now_ms();
                let newer = app.state::<AppState>().usage.lock().unwrap().fetched_at < r.captured_at;
                if newer {
                    set_and_broadcast(&app, |u| {
                        let backoff_until = u.backoff_until;
                        *u = snapshot_from(&r, now);
                        u.backoff_until = backoff_until;
                    });
                }
                // Desktop is live for this account: no request, and no token renewal either
                if now.saturating_sub(r.captured_at) < CACHE_FRESH_MS {
                    pause(&app);
                    continue;
                }
            }
            // Ahead of the back-off: renewing never touches the usage endpoint, and a fresh token deserves a fresh try
            if let Some(cred) = read_credentials() {
                if renewer.maybe_renew(&cred) == Some(true) {
                    consecutive_429 = 0;
                    set_and_broadcast(&app, |u| u.backoff_until = 0);
                }
            }
            // No requests inside the backoff window
            let bu = {
                let st = app.state::<AppState>();
                let u = st.usage.lock().unwrap();
                u.backoff_until
            };
            let now = now_ms();
            if bu > now {
                sleep_interruptible(((bu - now) / 1000).clamp(1, 30));
                continue;
            }
            match read_credentials() {
                // Keep a reading Desktop left behind, dimmed and dated, rather than blanking it
                // A Desktop reading keeps its own note: telling a Desktop user to run the CLI is no help
                None => set_and_broadcast(&app, |u| {
                    u.status = if u.windows.is_empty() { "needsAuth" } else { "stale" }.into();
                    if u.source != "desktop" {
                        u.note = "No Claude Code credential found".into();
                    }
                }),
                // Expired is not signed out: keep the last reading, dimmed and dated, and send nothing
                Some(cred) if cred.expired(now_ms()) => set_and_broadcast(&app, |u| {
                    u.status = if u.windows.is_empty() { "needsAuth" } else { "stale" }.into();
                    if u.source != "desktop" {
                        u.note = EXPIRED_NOTE.into();
                    }
                }),
                Some(cred) => {
                    let token = cred.token;
                    // On 401/403 re-read the credential and retry once (Claude Code may have just refreshed it)
                    let result = match fetch_once(&token) {
                        Err(FetchErr::NeedsAuth) => match read_credentials() {
                            Some(c2) if c2.token != token => fetch_once(&c2.token),
                            _ => Err(FetchErr::NeedsAuth),
                        },
                        other => other,
                    };
                    let auth_note = "Credential rejected (switched accounts?)";
                    match result {
                        Ok(windows) => {
                            consecutive_429 = 0;
                            set_and_broadcast(&app, |u| {
                                u.status = "ok".into();
                                u.windows = windows;
                                u.fetched_at = now_ms();
                                u.note.clear();
                                u.backoff_until = 0;
                                u.source.clear();
                            });
                        }
                        Err(FetchErr::NeedsAuth) => set_and_broadcast(&app, |u| {
                            u.status = "needsAuth".into();
                            u.note = auth_note.into();
                        }),
                        Err(FetchErr::RateLimited(ra)) => {
                            consecutive_429 += 1;
                            let wait = backoff_secs(consecutive_429 - 1, ra);
                            set_and_broadcast(&app, |u| {
                                if !u.windows.is_empty() {
                                    u.status = "stale".into();
                                }
                                u.note = format!("Rate limited, retrying in {wait}s");
                                u.backoff_until = now_ms() + wait * 1000;
                            });
                        }
                        Err(FetchErr::Other(msg)) => set_and_broadcast(&app, |u| {
                            if u.windows.is_empty() {
                                u.status = "error".into();
                            } else {
                                u.status = "stale".into();
                            }
                            u.note = msg;
                        }),
                    }
                }
            }
            pause(&app);
        }
    });
}

/// 60 s while a session is active, 300 s otherwise (upstream throttling discipline)
fn pause(app: &AppHandle) {
    let active = {
        let st = app.state::<AppState>();
        let store = st.store.lock().unwrap();
        let s = store.snapshot("en", "en", false, false);
        !s.sessions.is_empty()
    };
    sleep_interruptible(if active { POLL_ACTIVE_SECS } else { POLL_IDLE_SECS });
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXP: u64 = 1_000_000_000;

    #[test]
    fn renews_only_inside_the_margin() {
        assert!(!should_renew(None, EXP, None, None), "never launch on a guess");
        assert!(!should_renew(Some(EXP), EXP - RENEW_MARGIN_MS - 1, None, None), "plenty of time left");
        assert!(should_renew(Some(EXP), EXP - RENEW_MARGIN_MS, None, None));
        assert!(should_renew(Some(EXP), EXP + 3_600_000, None, None), "already expired still renews");
    }

    #[test]
    fn one_attempt_per_token_and_a_cooldown() {
        let now = EXP + 1;
        assert!(!should_renew(Some(EXP), now, Some(EXP), None), "same token is never retried");
        assert!(!should_renew(Some(EXP + 5), now, Some(EXP), Some(now - 1000)), "cooldown holds a new token back");
        assert!(should_renew(Some(EXP + 5), now, Some(EXP), Some(now - RENEW_COOLDOWN_MS)));
    }

    #[test]
    fn retry_after_never_exceeds_the_cap() {
        assert_eq!(backoff_secs(0, 3600), 3600);
        assert_eq!(backoff_secs(0, 0), BACKOFF_BASE_SECS);
        assert_eq!(backoff_secs(1, 300), 300);
        assert_eq!(backoff_secs(9, 0), BACKOFF_CAP_SECS);
    }

    #[test]
    fn desktop_bundled_cli_is_refused() {
        use std::path::Path;
        assert!(is_desktop_owned(Path::new(r"C:\Users\u\AppData\Local\AnthropicClaude\app-1.2.3\claude.exe")));
        assert!(is_desktop_owned(Path::new(r"C:\Users\u\AppData\Roaming\Claude\claude-code\2.1.0\claude.exe")));
        assert!(!is_desktop_owned(Path::new(r"C:\Users\u\.local\bin\claude.exe")));
        assert!(!is_desktop_owned(Path::new(r"C:\Users\u\AppData\Roaming\npm\claude.cmd")));
    }

    #[test]
    #[ignore = "Runs the installed standalone claude CLI; opt in for integration verification"]
    fn live_renewal_runs_the_standalone_cli() {
        let cli = find_cli().expect("a standalone claude CLI");
        assert!(!is_desktop_owned(&cli));
        let before = read_credentials().and_then(|c| c.expires_at);
        let t = std::time::Instant::now();
        run_renewal(&cli).expect("spawned");
        assert!(t.elapsed() < Duration::from_secs(RENEW_TIMEOUT_SECS), "returned before the timeout");
        let after = read_credentials().and_then(|c| c.expires_at);
        assert!(after >= before, "the expiry never moves backwards");
        eprintln!("cli: {}", cli.display());
    }

    #[test]
    fn expired_is_judged_against_now() {
        let c = Credential { token: "t".into(), expires_at: Some(EXP) };
        assert!(c.expired(EXP));
        assert!(!c.expired(EXP - 1));
        assert!(!Credential { token: "t".into(), expires_at: None }.expired(EXP));
    }

    fn reading(org: &str) -> crate::desktop_cache::Reading {
        crate::desktop_cache::Reading { org: org.into(), windows: Vec::new(), captured_at: 0 }
    }

    #[test]
    fn the_cli_account_keeps_the_claude_cell() {
        let (p, rest) = split_readings(vec![reading("a"), reading("b")], Some("b"));
        assert_eq!(p.unwrap().org, "b");
        assert_eq!(rest.iter().map(|r| r.org.as_str()).collect::<Vec<_>>(), ["a"]);
    }

    #[test]
    fn without_a_cli_account_the_first_org_takes_the_claude_cell() {
        let (p, rest) = split_readings(vec![reading("a"), reading("b")], None);
        assert_eq!(p.unwrap().org, "a");
        assert_eq!(rest.len(), 1);
        // A CLI account Desktop has no entry for: no cached reading, and every cached org is an extra
        let (p, rest) = split_readings(vec![reading("a")], Some("z"));
        assert!(p.is_none());
        assert_eq!(rest.len(), 1);
    }

    #[test]
    fn a_window_that_reset_since_the_reading_shows_as_rolled_over() {
        let mut r = reading("a");
        r.captured_at = EXP;
        r.windows = vec![
            LimitWindow { id: "session".into(), used: 0.4, resets_at: Some(EXP + 10), ..Default::default() },
            LimitWindow { id: "seven_day".into(), used: 0.5, resets_at: Some(EXP + CACHE_FRESH_MS * 10), ..Default::default() },
        ];
        let s = snapshot_from(&r, EXP + 20);
        assert_eq!(s.status, "ok");
        assert_eq!((s.windows[0].used, s.windows[0].resets_at, s.windows[0].derived), (0.0, None, true));
        assert_eq!(s.windows[1].used, 0.5);
        assert_eq!(snapshot_from(&r, EXP + CACHE_FRESH_MS).status, "stale");
    }
}
