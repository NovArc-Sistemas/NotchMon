//! Claude's limits, read out of Claude Desktop's HTTP cache — one reading per signed-in organization.
//!
//! Desktop is Chromium, and its usage panel's `GET https://claude.ai/api/organizations/<org>/usage`
//! lands in the HTTP cache. Every account Desktop has been signed into leaves its own entry, so this is
//! also the only place a Desktop-only user's *second* account can be seen: no token, no request, no
//! subprocess, no write. The Mac app reads the same response out of Chromium's Simple Cache; on Windows
//! Desktop uses the older **blockfile** backend, so the container differs and the rest does not.
//!
//! Blockfile, only as far as this needs it:
//!   - `data_1`: an 8192-byte header, then 256-byte `EntryStore` blocks. The header's allocation bitmap
//!     (+80) says which blocks are live; a freed entry keeps its bytes, so it is skipped by that bit.
//!   - An entry: `state` +20, `key_len` +32, `long_key` +36, stream sizes +40, stream addresses +56,
//!     key inline from +96. Stream 0 is the response headers, stream 1 the body.
//!   - A `CacheAddr`: bit 31 initialized; bits 28-30 file type (0 = external `f_%06x`, 2/3/4 = 256 B /
//!     1 KB / 4 KB blocks in `data_<n>`); bits 16-23 the file number; bits 0-15 the first block;
//!     bits 24-25 the block count minus one.
//!
//! Desktop holds these files open with delete access, so every open shares read, write *and* delete,
//! or it fails while Desktop runs. Every size read out of a file is bounded before it is believed.

use crate::usage::{parse_response, LimitWindow};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const HEADER: usize = 8192;
const ENTRY: usize = 256;
const ALLOCATION_MAP: usize = 80;
/// Chromium keeps a key inline up to four blocks, less the fields ahead of it
const MAX_INLINE_KEY: usize = 4 * ENTRY - 96 - 1;
/// A usage body is under 1 KB compressed; this still refuses the multi-megabyte assets beside it
const MAX_BODY: usize = 512 * 1024;
const MAX_HEADERS: usize = 64 * 1024;
const MAX_DECOMPRESSED: u64 = 256 * 1024;
/// data_1 is a few MB on a month-old cache; anything past this is not a cache we understand
const MAX_ENTRY_FILE: u64 = 64 * 1024 * 1024;
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xb5, 0x2f, 0xfd];

#[derive(Debug, Clone)]
pub struct Reading {
    pub org: String,
    pub windows: Vec<LimitWindow>,
    /// ms epoch, from the response's own `Date:` — when the numbers were true, not when they were read
    pub captured_at: u64,
}

pub fn default_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("Claude").join("Cache").join("Cache_Data"))
}

/// The newest reading per organization, sorted by organization so callers get a stable order.
/// Nothing there, or nothing readable, is an empty list: the caller falls back to what it had.
pub fn read_all() -> Vec<Reading> {
    default_dir().map(|d| read_dir(&d)).unwrap_or_default()
}

/// For doctor: which organizations Desktop's cache has a reading for, and how old (no numbers, no keys)
pub fn probe() -> String {
    let Some(dir) = default_dir() else { return "claude desktop cache: no config directory".into() };
    let r = read_dir(&dir);
    if r.is_empty() {
        return format!("claude desktop cache: no usage reading in {}", dir.display());
    }
    let now = chrono::Utc::now().timestamp_millis().max(0) as u64;
    let list: Vec<String> = r
        .iter()
        .map(|x| format!("{} {} min old", x.org, now.saturating_sub(x.captured_at) / 60_000))
        .collect();
    format!("claude desktop cache: {} account(s): {}", r.len(), list.join(", "))
}

fn open_shared(path: &Path) -> std::io::Result<File> {
    let mut o = std::fs::OpenOptions::new();
    o.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        o.share_mode(0x1 | 0x2 | 0x4); // FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE
    }
    o.open(path)
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

// ponytail: scans every entry block on each poll (a few MB, tens of ms); walk the index's hash table if data_1 ever gets big
pub fn read_dir(dir: &Path) -> Vec<Reading> {
    let Some(entries) = open_shared(&dir.join("data_1"))
        .ok()
        .filter(|f| f.metadata().map(|m| m.len() <= MAX_ENTRY_FILE).unwrap_or(false))
        .and_then(|mut f| {
            let mut v = Vec::new();
            f.read_to_end(&mut v).ok().map(|_| v)
        })
    else {
        return Vec::new();
    };
    if entries.len() <= HEADER {
        return Vec::new();
    }
    let mut out: Vec<Reading> = Vec::new();
    for block in 0..(entries.len() - HEADER) / ENTRY {
        let Some(r) = entry_reading(dir, &entries, block) else { continue };
        // Two keys per organization (`/usage` and `/usage?skip_spend=1`), refreshed in turn: newest wins
        match out.iter_mut().find(|x| x.org == r.org) {
            Some(x) if x.captured_at >= r.captured_at => {}
            Some(x) => *x = r,
            None => out.push(r),
        }
    }
    out.sort_by(|a, b| a.org.cmp(&b.org));
    out
}

fn entry_reading(dir: &Path, entries: &[u8], block: usize) -> Option<Reading> {
    let at = HEADER + block * ENTRY;
    let e = &entries[at..];
    let key_len = u32_at(e, 32) as usize;
    // Cheapest rejections first: this runs for every block in the file
    if key_len == 0 || key_len > MAX_INLINE_KEY || 96 + key_len > e.len() {
        return None;
    }
    let key = &e[96..96 + key_len];
    find(key, b"/api/organizations/")?;
    let org = usage_org(std::str::from_utf8(key).ok()?)?;
    let live = u32_at(entries, ALLOCATION_MAP + block / 32 * 4) >> (block % 32) & 1 == 1;
    if !live || u32_at(e, 20) != 0 || u32_at(e, 36) != 0 {
        return None; // freed, not ENTRY_NORMAL, or a key stored elsewhere (a usage key never is)
    }
    let headers = stream(dir, u32_at(e, 56), u32_at(e, 40) as usize, MAX_HEADERS)?;
    let body = stream(dir, u32_at(e, 60), u32_at(e, 44) as usize, MAX_BODY)?;
    let captured_at = header(&headers, b"date:")
        .and_then(|d| chrono::DateTime::parse_from_rfc2822(d.trim()).ok())
        .map(|d| d.timestamp_millis().max(0) as u64)?; // undated is unageable: not shown as a reading
    let json = decode(&body)?;
    let windows = parse_response(&serde_json::from_slice(&json).ok()?);
    // A response naming no window is not a reading: it would draw a ring with a hole in it
    (!windows.is_empty()).then_some(Reading { org, windows, captured_at })
}

/// `…/api/organizations/<org>/usage`, query aside, on Anthropic's hosts only. The key carries a
/// partition prefix (`1/0/https://claude.ai/…`), so the URL is found inside rather than anchored.
fn usage_org(key: &str) -> Option<String> {
    if !key.contains("claude.ai") && !key.contains("anthropic.com") {
        return None;
    }
    let rest = &key[key.find("/api/organizations/")? + "/api/organizations/".len()..];
    let path = rest.split(['?', '#']).next()?;
    match path.split('/').collect::<Vec<_>>()[..] {
        [org, "usage"] if !org.is_empty() => Some(org.to_string()),
        _ => None,
    }
}

/// One stream's bytes, from a block file or an external file, bounded twice: by the caller's cap and
/// by the blocks the address actually spans.
fn stream(dir: &Path, addr: u32, size: usize, cap: usize) -> Option<Vec<u8>> {
    if addr & 0x8000_0000 == 0 || size == 0 || size > cap {
        return None;
    }
    let (path, offset) = match (addr >> 28) & 7 {
        0 => (dir.join(format!("f_{:06x}", addr & 0x0fff_ffff)), 0),
        t @ 2..=4 => {
            let block_size = [256usize, 1024, 4096][t as usize - 2];
            if size > (((addr >> 24) & 3) as usize + 1) * block_size {
                return None;
            }
            let file = (addr >> 16) & 0xff;
            (dir.join(format!("data_{file}")), HEADER + (addr & 0xffff) as usize * block_size)
        }
        _ => return None,
    };
    let mut f = open_shared(&path).ok()?;
    f.seek(SeekFrom::Start(offset as u64)).ok()?;
    let mut v = vec![0; size];
    f.read_exact(&mut v).ok()?;
    Some(v)
}

/// A header's value out of the stored response info: NUL-separated `name:value` strings. Names are
/// lower-case over HTTP/2 and HTTP/3 and original-case over HTTP/1.1, so the match ignores case.
fn header<'a>(headers: &'a [u8], name: &[u8]) -> Option<&'a str> {
    let lower: Vec<u8> = headers.to_ascii_lowercase();
    let mut needle = vec![0u8];
    needle.extend_from_slice(name);
    let start = find(&lower, &needle)? + needle.len();
    let end = start + headers[start..].iter().position(|&b| b == 0).unwrap_or(headers.len() - start);
    std::str::from_utf8(&headers[start..end]).ok()
}

/// The body as stored: zstd today. Plain JSON is taken as is; any other encoding is skipped rather
/// than guessed at — it is the part most likely to change, since Chromium negotiates it.
fn decode(body: &[u8]) -> Option<Vec<u8>> {
    if body.first() == Some(&b'{') {
        return Some(body.to_vec());
    }
    if !body.starts_with(&ZSTD_MAGIC) {
        return None;
    }
    let decoder = ruzstd::decoding::StreamingDecoder::new(body).ok()?;
    let mut out = Vec::new();
    // Take one past the cap so an oversized body is refused instead of silently cut
    decoder.take(MAX_DECOMPRESSED + 1).read_to_end(&mut out).ok()?;
    (out.len() as u64 <= MAX_DECOMPRESSED).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORG_A: &str = "00000000-0000-4000-8000-00000000000a";
    const ORG_B: &str = "00000000-0000-4000-8000-00000000000b";

    /// A synthetic cache: data_1 with entries, data_3 (1 KB blocks) with their streams
    struct Fixture {
        data_1: Vec<u8>,
        data_3: Vec<u8>,
    }

    impl Fixture {
        fn new() -> Self {
            Fixture { data_1: vec![0; HEADER + 64 * ENTRY], data_3: vec![0; HEADER + 64 * 1024] }
        }

        /// Puts `bytes` in data_3 at `block` and returns its address (file 3, type BLOCK_1K)
        fn put(&mut self, block: u32, bytes: &[u8]) -> u32 {
            let at = HEADER + block as usize * 1024;
            self.data_3[at..at + bytes.len()].copy_from_slice(bytes);
            0x8000_0000 | 3 << 28 | 3 << 16 | block
        }

        fn entry(&mut self, block: usize, live: bool, key: &str, date: &str, body: &[u8]) {
            let headers = format!("HTTP/1.1 200\0content-encoding:zstd\0date:{date}\0").into_bytes();
            let h = self.put(block as u32 * 2, &headers);
            let b = self.put(block as u32 * 2 + 1, body);
            let e = &mut self.data_1[HEADER + block * ENTRY..];
            e[32..36].copy_from_slice(&(key.len() as u32).to_le_bytes());
            e[40..44].copy_from_slice(&(headers.len() as u32).to_le_bytes());
            e[44..48].copy_from_slice(&(body.len() as u32).to_le_bytes());
            e[56..60].copy_from_slice(&h.to_le_bytes());
            e[60..64].copy_from_slice(&b.to_le_bytes());
            e[96..96 + key.len()].copy_from_slice(key.as_bytes());
            if live {
                self.data_1[ALLOCATION_MAP + block / 32 * 4] |= 1 << (block % 32);
            }
        }

        fn write(&self) -> PathBuf {
            let dir = std::env::temp_dir().join(format!("codenotch-cache-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("data_1"), &self.data_1).unwrap();
            std::fs::write(dir.join("data_3"), &self.data_3).unwrap();
            dir
        }
    }

    fn zstd(session: u32, weekly: u32) -> Vec<u8> {
        let json = format!(
            r#"{{"five_hour":{{"utilization":{session}.0,"resets_at":"2026-09-15T19:20:00+00:00"}},
               "seven_day":{{"utilization":{weekly}.0,"resets_at":"2026-09-21T10:00:00+00:00"}}}}"#
        );
        ruzstd::encoding::compress_to_vec(json.as_bytes(), ruzstd::encoding::CompressionLevel::Fastest)
    }

    #[test]
    fn reads_each_live_org_and_the_newer_of_its_two_keys() {
        let mut f = Fixture::new();
        let url = |org: &str, q: &str| format!("1/0/https://claude.ai/api/organizations/{org}/usage{q}");
        f.entry(1, true, &url(ORG_A, ""), "Tue, 15 Sep 2026 18:02:16 GMT", &zstd(15, 8));
        f.entry(2, true, &url(ORG_A, "?skip_spend=1"), "Tue, 15 Sep 2026 18:02:45 GMT", &zstd(20, 10));
        f.entry(3, true, &url(ORG_B, ""), "Mon, 14 Sep 2026 15:13:32 GMT", &zstd(12, 43));
        // Freed: newer than anything, and must still never be read
        f.entry(4, false, &url(ORG_B, ""), "Wed, 16 Sep 2026 10:00:00 GMT", &zstd(99, 99));
        // Not Anthropic's host
        f.entry(5, true, &format!("1/0/https://example.com/api/organizations/{ORG_B}/usage"), "Wed, 16 Sep 2026 10:00:00 GMT", &zstd(77, 77));
        let dir = f.write();

        let r = read_dir(&dir);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(r.iter().map(|x| x.org.as_str()).collect::<Vec<_>>(), [ORG_A, ORG_B]);
        let session = |x: &Reading| x.windows.iter().find(|w| w.id == "session").unwrap().used;
        assert!((session(&r[0]) - 0.20).abs() < 1e-9, "newest key wins");
        assert!((session(&r[1]) - 0.12).abs() < 1e-9, "freed entry ignored");
        assert_eq!(r[1].captured_at, 1_789_398_812_000);
    }

    #[test]
    fn usage_org_takes_only_the_usage_endpoint() {
        assert_eq!(usage_org(&format!("1/0/https://claude.ai/api/organizations/{ORG_A}/usage?skip_spend=1")).as_deref(), Some(ORG_A));
        assert_eq!(usage_org(&format!("https://claude.ai/api/organizations/{ORG_A}/usage/extra")), None);
        assert_eq!(usage_org(&format!("https://claude.ai/api/organizations/{ORG_A}/members")), None);
    }

    #[test]
    fn a_missing_cache_is_no_reading() {
        assert!(read_dir(Path::new("Z:\\no\\such\\cache")).is_empty());
    }
}
