use anyhow::Result;
use arboard::Clipboard;
use crossterm::{
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEventKind,
        KeyModifiers,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    io,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::{mpsc, Mutex},
};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const GITHUB_REPO: &str = "Dilip-gif/rust_media_downloader";
const BROWSER_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

// Headless Chrome (--dump-dom) + Shreddit DOM Scraper with Binary Image Magic-Byte Verification
const REDDIT_HEADLESS_DOM_PY: &str = r#"
import sys, os, re, json, html, urllib.parse, subprocess, tempfile, shutil

url = sys.argv[1].split('?')[0].rstrip('/')
out_dir = sys.argv[2]
os.makedirs(out_dir, exist_ok=True)

post_id_match = re.search(r'/comments/([a-z0-9]+)', url, re.I)
post_id = post_id_match.group(1) if post_id_match else "post"

slug_match = re.search(r'/comments/[a-z0-9]+/([^/]+)', url, re.I)
safe_title = re.sub(r'[^a-zA-Z0-9_\- ]', '', slug_match.group(1) if slug_match else "reddit_media").strip()[:60] or "reddit_media"

dom_html = ""
cookie_jar = None

try:
    from yt_dlp.cookies import extract_cookies_from_browser
    cookie_jar = extract_cookies_from_browser("chrome")
except Exception:
    pass

try:
    from curl_cffi import requests as cffi_requests
    page_headers = {
        "Accept": "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        "Accept-Language": "en-US,en;q=0.9",
    }
    r = cffi_requests.get(url, impersonate="chrome", cookies=cookie_jar, headers=page_headers, timeout=20)
    if r.status_code == 200 and ("shreddit" in r.text or "redd.it" in r.text):
        dom_html = r.text
except Exception:
    pass

def dump_dom_with_headless_browser(target_url):
    browsers = ["google-chrome", "google-chrome-stable", "chromium", "chromium-browser", "brave-browser", "msedge", "chrome"]
    for b in browsers:
        bin_path = shutil.which(b)
        if not bin_path:
            continue
        tmp_profile = tempfile.mkdtemp(prefix="md_headless_")
        try:
            proc = subprocess.run(
                [
                    bin_path,
                    "--headless=new",
                    "--disable-gpu",
                    "--no-sandbox",
                    "--disable-blink-features=AutomationControlled",
                    f"--user-agent={sys.argv[3]}",
                    f"--user-data-dir={tmp_profile}",
                    "--virtual-time-budget=5000",
                    "--dump-dom",
                    target_url,
                ],
                capture_output=True,
                text=True,
                timeout=30,
            )
            if proc.stdout and ("redd.it" in proc.stdout or "shreddit" in proc.stdout):
                return proc.stdout
        except Exception:
            pass
        finally:
            shutil.rmtree(tmp_profile, ignore_errors=True)
    return ""

if not dom_html or ("i.redd.it" not in dom_html and "preview.redd.it" not in dom_html and "v.redd.it" not in dom_html):
    headless_out = dump_dom_with_headless_browser(url)
    if headless_out:
        dom_html = headless_out

title_m = re.search(r'post-title="([^"]+)"', dom_html)
if title_m:
    t = html.unescape(title_m.group(1))
    cleaned = re.sub(r'[^a-zA-Z0-9_\- ]', '', t).strip()[:60]
    if cleaned:
        safe_title = cleaned

vid_m = re.search(r'https://v\.redd\.it/([a-zA-Z0-9]+)', dom_html)
if vid_m:
    vid_id = vid_m.group(1)
    hls_url = f"https://v.redd.it/{vid_id}/HLSPlaylist.m3u8"
    print(f"REDDIT_VIDEO::{safe_title}_{post_id}::{hls_url}", flush=True)
    sys.exit(0)

raw_candidates = []
for m in re.finditer(r'content-href="(https://(?:i|preview)\.redd\.it/[^"]+)"', dom_html):
    raw_candidates.append(html.unescape(m.group(1)))

for m in re.finditer(r'srcset="([^"]+)"', dom_html):
    srcset_val = html.unescape(m.group(1))
    for part in srcset_val.split(","):
        tokens = part.strip().split()
        if tokens and tokens[0].startswith("https://") and "redd.it/" in tokens[0]:
            raw_candidates.append(tokens[0])

for m in re.finditer(r'(?:src|href|lazy-src)="(https://(?:i|preview)\.redd\.it/[^"]+)"', dom_html):
    u = html.unescape(m.group(1))
    raw_candidates.append(u)

def score_url(u):
    if "i.redd.it/" in u:
        return 100000
    w_m = re.search(r'width=(\d+)', u)
    if w_m:
        return int(w_m.group(1))
    return 500

best_signed_by_key = {}
ordered_keys = []

for u in raw_candidates:
    if any(skip in u for skip in ("award_images", "snoovatar", "styles.redditmedia", "avatar", "width=64", "width=32", "width=96", "project-avatar")):
        continue
    parsed = urllib.parse.urlparse(u)
    fname = os.path.basename(parsed.path)
    id_m = re.search(r'-v0-([a-zA-Z0-9]+\.(?:jpg|jpeg|png|webp|gif))$', fname, re.I)
    key = id_m.group(1) if id_m else fname
    if not key or '.' not in key:
        continue

    if key not in ordered_keys:
        ordered_keys.append(key)

    if "preview.redd.it" in u:
        if key not in best_signed_by_key or score_url(u) > score_url(best_signed_by_key[key]):
            best_signed_by_key[key] = u
    elif key not in best_signed_by_key:
        best_signed_by_key[key] = u

if not ordered_keys:
    sys.stderr.write("ERROR: Headless DOM could not find media on this Reddit page.\n")
    sys.exit(1)

def detect_image_magic_ext(data):
    if not data or len(data) < 64:
        return None
    head = data[:64].lstrip().lower()
    if head.startswith(b"<!doctype") or head.startswith(b"<html") or head.startswith(b"<?xml") or b"<head>" in head:
        return None
    if data.startswith(b"\xff\xd8\xff"):
        return ".jpg"
    if data.startswith(b"\x89PNG\r\n\x1a\n"):
        return ".png"
    if data.startswith((b"GIF87a", b"GIF89a")):
        return ".gif"
    if data.startswith(b"RIFF") and data[8:12] == b"WEBP":
        return ".webp"
    return None

def fetch_verified_image_bytes(target_url):
    img_headers = {
        "Accept": "image/jpeg,image/png,image/gif,image/*;q=0.8",
        "Sec-Fetch-Dest": "image",
        "Sec-Fetch-Mode": "no-cors",
        "Sec-Fetch-Site": "cross-site",
        "Referer": "https://www.reddit.com/",
        "User-Agent": sys.argv[3],
    }
    try:
        from curl_cffi import requests as cffi_requests
        ir = cffi_requests.get(target_url, impersonate="chrome", headers=img_headers, cookies=cookie_jar, timeout=30)
        ctype = ir.headers.get("content-type", "").lower()
        if ir.status_code == 200 and "text/html" not in ctype:
            real_ext = detect_image_magic_ext(ir.content)
            if real_ext:
                return ir.content, real_ext
    except Exception:
        pass
    try:
        import urllib.request
        req = urllib.request.Request(target_url, headers=img_headers)
        with urllib.request.urlopen(req, timeout=30) as resp:
            ctype = (resp.headers.get("Content-Type") or "").lower()
            data = resp.read()
            if "text/html" not in ctype:
                real_ext = detect_image_magic_ext(data)
                if real_ext:
                    return data, real_ext
    except Exception:
        pass
    return None, None

downloaded = 0
for idx, key in enumerate(ordered_keys, 1):
    signed_url = best_signed_by_key.get(key, "")
    direct_cdn_url = f"https://i.redd.it/{key}"

    img_bytes, real_ext = fetch_verified_image_bytes(direct_cdn_url)
    if not img_bytes and signed_url and signed_url != direct_cdn_url:
        img_bytes, real_ext = fetch_verified_image_bytes(signed_url)

    if img_bytes and real_ext:
        out_name = f"{safe_title}_{post_id}_{idx}{real_ext}" if len(ordered_keys) > 1 else f"{safe_title}_{post_id}{real_ext}"
        out_path = os.path.join(out_dir, out_name)
        with open(out_path, "wb") as f:
            f.write(img_bytes)
        downloaded += 1
        print(out_path, flush=True)

if downloaded == 0:
    sys.stderr.write("ERROR: Failed to download verified image binaries from Reddit CDN.\n")
    sys.exit(1)
"#;

// --- GITHUB AUTO-UPDATE CHECKER ---

fn version_is_newer(latest_tag: &str, current_ver: &str) -> bool {
    let parse_ver = |v: &str| -> Vec<u32> {
        v.trim_start_matches('v')
            .split('.')
            .filter_map(|s| s.parse::<u32>().ok())
            .collect()
    };
    let l = parse_ver(latest_tag);
    let c = parse_ver(current_ver);
    l > c
}

async fn check_github_for_updates() -> Option<String> {
    let api_url = format!("https://api.github.com/repos/{}/releases/latest", GITHUB_REPO);
    let out = tokio::time::timeout(
        Duration::from_secs(8),
        Command::new("curl")
            .arg("-fsSL")
            .arg("-A")
            .arg("rust-media-downloader-tui")
            .arg(&api_url)
            .output(),
    )
    .await
    .ok()?
    .ok()?;

    if !out.status.success() {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    let tag = v.get("tag_name")?.as_str()?;
    if version_is_newer(tag, APP_VERSION) {
        Some(tag.to_string())
    } else {
        None
    }
}

fn run_interactive_self_update() {
    println!("🚀 Launching Rust Media Downloader Auto-Updater...");
    match std::env::consts::OS {
        "windows" => {
            let cmd = format!(
                "irm https://raw.githubusercontent.com/{}/main/install.ps1 | iex",
                GITHUB_REPO
            );
            let _ = std::process::Command::new("powershell")
                .args(["-NoProfile", "-Command", &cmd])
                .status();
        }
        _ => {
            let cmd = format!(
                "curl -fsSL https://raw.githubusercontent.com/{}/main/install.sh | bash",
                GITHUB_REPO
            );
            let _ = std::process::Command::new("bash")
                .args(["-c", &cmd])
                .status();
        }
    }
}

// --- PERSISTENT HISTORY ENGINE ---

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchHistoryEntry {
    pub url: String,
    pub platform: String,
    pub timestamp: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DownloadHistoryEntry {
    pub title: String,
    pub url: String,
    pub platform: String,
    pub media_type: String,
    pub save_path: String,
    pub timestamp: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct HistoryStore {
    pub search_history: Vec<SearchHistoryEntry>,
    pub download_history: Vec<DownloadHistoryEntry>,
}

fn get_history_file_path() -> PathBuf {
    let mut dir = dirs::data_local_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    dir.push("MediaDownloader");
    let _ = std::fs::create_dir_all(&dir);
    dir.push("history.json");
    dir
}

impl HistoryStore {
    pub fn load() -> Self {
        let path = get_history_file_path();
        if let Ok(data) = std::fs::read_to_string(path) {
            if let Ok(store) = serde_json::from_str::<HistoryStore>(&data) {
                return store;
            }
        }
        Self::default()
    }

    pub fn save(&self) {
        let path = get_history_file_path();
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    pub fn record_search(&mut self, url: &str, platform: &str) {
        if url.is_empty() || url == "batch" {
            return;
        }
        self.search_history.retain(|e| e.url != url);
        self.search_history.insert(
            0,
            SearchHistoryEntry {
                url: url.to_string(),
                platform: platform.to_string(),
                timestamp: current_timestamp_str(),
            },
        );
        if self.search_history.len() > 300 {
            self.search_history.truncate(300);
        }
        self.save();
    }

    pub fn record_download(&mut self, item: &QueueItem) {
        if item.url.is_empty() || item.url == "batch" {
            return;
        }
        let media_type = if item.is_image {
            "Image"
        } else if item.is_audio {
            "Audio"
        } else {
            "Video"
        };
        let save_path = get_base_download_dir(item).to_string_lossy().to_string();
        self.download_history.retain(|e| !(e.url == item.url && e.media_type == media_type));
        self.download_history.insert(
            0,
            DownloadHistoryEntry {
                title: item.title.clone(),
                url: item.url.clone(),
                platform: item.platform.clone(),
                media_type: media_type.to_string(),
                save_path,
                timestamp: current_timestamp_str(),
            },
        );
        if self.download_history.len() > 300 {
            self.download_history.truncate(300);
        }
        self.save();
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HistoryTab {
    Downloads,
    Searches,
}

fn current_timestamp_str() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let local_secs = secs + 19_800;
    let days = local_secs.div_euclid(86_400);
    let rem = local_secs.rem_euclid(86_400);
    let hour = rem / 3600;
    let min = (rem % 3600) / 60;

    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };

    format!("{:04}-{:02}-{:02} {:02}:{:02}", year, m, d, hour, min)
}

// --- HELPER PATHS & UTF-8 SAFE UTILITIES ---

fn truncate_chars(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count > max_chars {
        let kept: String = s.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{}...", kept)
    } else {
        s.to_string()
    }
}

fn get_ytdlp_cmd() -> String {
    let base = if std::env::consts::OS == "windows" { "yt-dlp.exe" } else { "yt-dlp" };
    if Path::new(base).exists() {
        format!("./{}", base)
    } else {
        base.to_string()
    }
}

fn get_gallerydl_cmd() -> String {
    let base = if std::env::consts::OS == "windows" { "gallery-dl.exe" } else { "gallery-dl" };
    if Path::new(base).exists() {
        format!("./{}", base)
    } else {
        base.to_string()
    }
}

fn get_local_ffmpeg_dir() -> Option<String> {
    let bin = if std::env::consts::OS == "windows" { "ffmpeg.exe" } else { "ffmpeg" };
    if Path::new(bin).exists() {
        Some(".".to_string())
    } else {
        None
    }
}

fn detect_platform_folder(url: &str, extractor: &str) -> &'static str {
    let u = url.to_lowercase();
    let e = extractor.to_lowercase();
    if u.contains("youtube.com") || u.contains("youtu.be") || e.contains("youtube") {
        "YouTube"
    } else if u.contains("instagram.com") || e.contains("instagram") {
        "Instagram"
    } else if u.contains("reddit.com") || u.contains("redd.it") || e.contains("reddit") {
        "Reddit"
    } else if u.contains("x.com") || u.contains("twitter.com") || e.contains("twitter") {
        "X_Twitter"
    } else if u.contains("pinterest.com") || u.contains("pin.it") || e.contains("pinterest") {
        "Pinterest"
    } else if u.contains("tiktok.com") || e.contains("tiktok") {
        "TikTok"
    } else if u.contains("imgur.com") || e.contains("imgur") {
        "Imgur"
    } else {
        "Other"
    }
}

fn get_base_download_dir(item: &QueueItem) -> PathBuf {
    let mut path = dirs::download_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("MediaDownloader");
    
    if item.is_audio {
        path.push("Audio");
    } else if item.is_image {
        path.push("Images");
    } else {
        path.push("Videos");
    }

    let platform_folder = detect_platform_folder(&item.url, &item.platform);
    path.push(platform_folder);
    
    if let Some(folder) = &item.target_folder {
        let safe_folder = folder.replace(|c: char| !c.is_alphanumeric() && c != ' ' && c != '-', "_");
        path.push(safe_folder);
    }
    path
}

fn open_folder_in_os(path: &Path) {
    let mut current = Some(path);
    let mut target = None;
    while let Some(p) = current {
        if p.exists() {
            target = Some(p.to_path_buf());
            break;
        }
        current = p.parent();
    }

    let final_dir = target.unwrap_or_else(|| {
        let mut base = dirs::download_dir().unwrap_or_else(|| PathBuf::from("."));
        base.push("MediaDownloader");
        let _ = std::fs::create_dir_all(&base);
        base
    });

    let path_str = final_dir.to_string_lossy().to_string();
    match std::env::consts::OS {
        "windows" => {
            let _ = std::process::Command::new("explorer").arg(&path_str).spawn();
        }
        "macos" => {
            let _ = std::process::Command::new("open").arg(&path_str).spawn();
        }
        _ => {
            let _ = std::process::Command::new("xdg-open")
                .arg(&path_str)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
        }
    }
}

fn is_reddit_url(url: &str) -> bool {
    let lower = url.to_lowercase();
    lower.contains("reddit.com/") || lower.contains("redd.it/")
}

fn is_direct_image_or_gallery_url(url: &str) -> bool {
    let lower = url.to_lowercase();
    lower.contains("instagram.com/p/")
        || lower.contains("reddit.com/gallery/")
        || lower.contains("i.redd.it/")
        || lower.contains("imgur.com/a/")
        || lower.contains("imgur.com/gallery/")
        || lower.contains("pinterest.com/pin/")
        || lower.contains("pin.it/")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".png")
        || lower.ends_with(".webp")
}

fn clean_tracking_params(url: &str) -> String {
    let lower = url.to_lowercase();
    if lower.contains("reddit.com/") || lower.contains("instagram.com/") || lower.contains("pinterest.com/") || lower.contains("x.com/") {
        if let Some(idx) = url.find('?') {
            return url[..idx].to_string();
        }
    }
    url.to_string()
}

fn extract_valid_urls_from_text(text: &str) -> Vec<String> {
    let mut valid_urls = Vec::new();
    for raw in text.split_whitespace() {
        let trimmed = raw.trim_matches(|c: char| {
            matches!(c, '"' | '\'' | '(' | ')' | '[' | ']' | '<' | '>' | ',' | ';')
        });
        if let Some(idx) = trimmed.find("http://").or_else(|| trimmed.find("https://")) {
            let mut url = trimmed[idx..].to_string();
            if !url.contains('.') || url.contains('{') || url.contains('}') || url.len() < 11 {
                continue;
            }
            if let Some(si_idx) = url.find("&si=") {
                url = url[..si_idx].to_string();
            }
            if let Some(si_idx) = url.find("?si=") {
                url = url[..si_idx].to_string();
            }
            url = clean_tracking_params(&url);
            valid_urls.push(url);
        }
    }
    valid_urls
}

async fn resolve_short_url(url: &str) -> String {
    let lower = url.to_lowercase();
    if (lower.contains("reddit.com/") && lower.contains("/s/")) || lower.contains("pin.it/") {
        let null_dev = if std::env::consts::OS == "windows" { "NUL" } else { "/dev/null" };
        if let Ok(Ok(output)) = tokio::time::timeout(
            Duration::from_secs(10),
            Command::new("curl")
                .arg("-Ls")
                .arg("-A")
                .arg(BROWSER_UA)
                .arg("-o")
                .arg(null_dev)
                .arg("-w")
                .arg("%{url_effective}")
                .arg(url)
                .output(),
        )
        .await
        {
            let resolved = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if resolved.starts_with("http") {
                return clean_tracking_params(&resolved);
            }
        }
    }
    clean_tracking_params(url)
}

fn remove_sidecar_subtitles_in_dir(dir: &Path) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                if ["srt", "vtt", "lrc", "ass"].contains(&ext.to_lowercase().as_str()) {
                    let _ = std::fs::remove_file(p);
                }
            }
        }
    }
}

// --- AUTHENTICATION ENGINE ---

#[derive(Clone, Debug, PartialEq)]
pub enum AuthMode {
    SmartAuto,
    Chrome,
    Firefox,
    Brave,
    Edge,
    CookiesFile,
}

impl AuthMode {
    pub fn all() -> Vec<AuthMode> {
        vec![
            AuthMode::SmartAuto,
            AuthMode::Chrome,
            AuthMode::Firefox,
            AuthMode::Brave,
            AuthMode::Edge,
            AuthMode::CookiesFile,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            AuthMode::SmartAuto => "🛡️ Smart Auto (Guest First -> Auto-Try Browsers on Login/Age Wall)",
            AuthMode::Chrome => "🌐 Always Use Google Chrome Cookies",
            AuthMode::Firefox => "🦊 Always Use Firefox Cookies",
            AuthMode::Brave => "🦁 Always Use Brave Cookies",
            AuthMode::Edge => "🌊 Always Use Microsoft Edge Cookies",
            AuthMode::CookiesFile => "🍪 Always Use Static File (cookies.txt)",
        }
    }

    pub fn badge(&self) -> String {
        match self {
            AuthMode::SmartAuto => "🛡️ Smart Auth (Auto-Chain)".into(),
            AuthMode::Chrome => "🌐 Chrome Auth".into(),
            AuthMode::Firefox => "🦊 Firefox Auth".into(),
            AuthMode::Brave => "🦁 Brave Auth".into(),
            AuthMode::Edge => "🌊 Edge Auth".into(),
            AuthMode::CookiesFile => "🍪 Static cookies.txt".into(),
        }
    }
}

fn apply_specific_auth(cmd: &mut Command, mode: &AuthMode) {
    match mode {
        AuthMode::SmartAuto => {}
        AuthMode::CookiesFile => {
            cmd.arg("--cookies").arg("cookies.txt");
        }
        AuthMode::Chrome => {
            cmd.arg("--cookies-from-browser").arg("chrome");
        }
        AuthMode::Firefox => {
            cmd.arg("--cookies-from-browser").arg("firefox");
        }
        AuthMode::Brave => {
            cmd.arg("--cookies-from-browser").arg("brave");
        }
        AuthMode::Edge => {
            cmd.arg("--cookies-from-browser").arg("edge");
        }
    }
}

fn is_auth_wall_error(err: &str) -> bool {
    let lower = err.to_lowercase();
    lower.contains("sign in")
        || lower.contains("confirm your age")
        || lower.contains("age-restricted")
        || lower.contains("age restricted")
        || lower.contains("inappropriate for some users")
        || lower.contains("not available to everyone")
        || lower.contains("isn't available to everyone")
        || lower.contains("certain audiences")
        || lower.contains("not a bot")
        || lower.contains("private video")
        || lower.contains("private profile")
        || lower.contains("members-only")
        || lower.contains("login required")
        || lower.contains("redirect to login")
        || lower.contains("accounts/login")
        || lower.contains("authentication")
        || lower.contains("use --cookies")
        || lower.contains("401 unauthorized")
}

fn is_no_video_error(err: &str) -> bool {
    let lower = err.to_lowercase();
    lower.contains("no video formats found")
        || lower.contains("there is no video")
        || lower.contains("no video could be found")
        || lower.contains("unsupported url")
        || lower.contains("unable to extract")
}

// --- DATA STRUCTURES ---

#[derive(Debug, Clone, Deserialize)]
pub struct YtDlpFormat {
    pub format_id: String,
    pub ext: Option<String>,
    pub height: Option<u32>,
    pub filesize: Option<u64>,
    pub filesize_approx: Option<u64>,
    pub vcodec: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FlatEntry {
    pub id: Option<String>,
    pub url: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct YtDlpInfo {
    pub title: Option<String>,
    pub extractor_key: Option<String>,
    pub description: Option<String>,
    pub formats: Option<Vec<YtDlpFormat>>,
    pub entries: Option<Vec<FlatEntry>>,
    pub _type: Option<String>,
    pub is_live: Option<bool>,
}

#[derive(Clone, Debug)]
pub struct FormatChoice {
    pub id: String,
    pub label: String,
    pub ext: String,
    pub est_size: String,
    pub is_audio: bool,
    pub is_image: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ItemStatus {
    New,
    FetchingMeta,
    FormatSelect,
    Ready,
    Queued,
    Downloading,
    Paused,
    Completed,
    Skipped,
    Failed(String),
}

#[derive(Clone, Debug)]
pub struct DownloadStats {
    pub percent: f32,
    pub downloaded_bytes: f64,
    pub total_bytes: f64,
    pub smoothed_speed: f64,
    pub last_dl_bytes: f64,
    pub last_update: Instant,
    pub elapsed: Duration,
}

impl Default for DownloadStats {
    fn default() -> Self {
        Self {
            percent: 0.0,
            downloaded_bytes: 0.0,
            total_bytes: 0.0,
            smoothed_speed: 0.0,
            last_dl_bytes: 0.0,
            last_update: Instant::now(),
            elapsed: Duration::ZERO,
        }
    }
}

#[derive(Clone, Debug)]
pub struct QueueItem {
    pub id: usize,
    pub url: String,
    pub title: String,
    pub platform: String,
    pub description: String,
    pub status: ItemStatus,
    pub available_formats: Vec<FormatChoice>,
    pub chosen_format_id: Option<String>,
    pub is_audio: bool,
    pub is_image: bool,
    pub target_files: Vec<String>,
    pub stats: DownloadStats,
    pub started_at: Option<Instant>,
    pub playlist_entries: Option<Vec<FlatEntry>>,
    pub target_folder: Option<String>, 
    pub retries: u8,
    pub resolved_auth: Option<AuthMode>,
}

enum WorkerMessage {
    UpdateAvailable { tag: String },
    UrlResolved { id: usize, resolved_url: String },
    MetaFetched { id: usize, info: YtDlpInfo, resolved_auth: Option<AuthMode> },
    ImageDetected { id: usize, platform: String, resolved_auth: Option<AuthMode> },
    SwitchToImageEngine { id: usize },
    SwitchToVideoEngine { id: usize },
    MetaFailed { id: usize, error: String },
    Filename { id: usize, file_name: String },
    Progress { id: usize, percent: f32, dl_bytes: f64, total_bytes: f64 },
    Finished { id: usize, success: bool, error: Option<String>, was_killed: bool },
    ClipboardCaptured { url: String },
}

// --- APP STATE ---

struct App {
    input_url: String,
    input_warning: Option<(String, Instant)>,
    available_update: Option<String>,
    trigger_self_update_exit: bool,
    queue: Vec<QueueItem>,
    queue_state: ListState,
    format_state: ListState,
    auth_state: ListState,
    show_auth_modal: bool,
    auth_mode: AuthMode,
    history: HistoryStore,
    show_history_modal: bool,
    history_tab: HistoryTab,
    history_state: ListState,
    active_worker_cancel: Arc<Mutex<HashMap<usize, tokio::sync::oneshot::Sender<()>>>>,
    worker_tx: mpsc::Sender<WorkerMessage>,
    worker_rx: mpsc::Receiver<WorkerMessage>,
    active_downloads: HashSet<usize>,
    max_concurrent_downloads: usize,
    next_id: usize,
}

impl App {
    fn new(worker_tx: mpsc::Sender<WorkerMessage>, worker_rx: mpsc::Receiver<WorkerMessage>) -> Self {
        let mut queue_state = ListState::default();
        queue_state.select(Some(0));
        let mut format_state = ListState::default();
        format_state.select(Some(0));
        let mut auth_state = ListState::default();
        auth_state.select(Some(0));
        let mut history_state = ListState::default();
        history_state.select(Some(0));

        Self {
            input_url: String::new(),
            input_warning: None,
            available_update: None,
            trigger_self_update_exit: false,
            queue: Vec::new(),
            queue_state,
            format_state,
            auth_state,
            show_auth_modal: false,
            auth_mode: AuthMode::SmartAuto,
            history: HistoryStore::load(),
            show_history_modal: false,
            history_tab: HistoryTab::Downloads,
            history_state,
            active_worker_cancel: Arc::new(Mutex::new(HashMap::new())),
            worker_tx,
            worker_rx,
            active_downloads: HashSet::new(),
            max_concurrent_downloads: 4, 
            next_id: 1,
        }
    }

    fn set_input_warning(&mut self, msg: &str) {
        self.input_warning = Some((msg.to_string(), Instant::now()));
    }

    fn add_urls_to_queue(&mut self, text: String) -> bool {
        let mut valid_urls = extract_valid_urls_from_text(&text);

        if valid_urls.is_empty() {
            return false;
        }

        for u in &valid_urls {
            let plat = detect_platform_folder(u, "");
            self.history.record_search(u, plat);
        }

        let item_id = self.next_id;
        self.next_id += 1;

        let initial_auth = if self.auth_mode == AuthMode::SmartAuto {
            None
        } else {
            Some(self.auth_mode.clone())
        };

        let item = if valid_urls.len() == 1 {
            let url = valid_urls.pop().unwrap();
            if self.queue.iter().any(|item| item.url == url && item.status == ItemStatus::New) {
                return true;
            }
            let detected_platform = detect_platform_folder(&url, "").to_string();

            QueueItem {
                id: item_id, url, title: "Press [Enter] to inspect".into(), platform: detected_platform,
                description: String::new(), status: ItemStatus::New, available_formats: Vec::new(),
                chosen_format_id: None, is_audio: false, is_image: false, target_files: Vec::new(),
                stats: DownloadStats::default(), started_at: None, playlist_entries: None,
                target_folder: None, retries: 0, resolved_auth: initial_auth,
            }
        } else {
            let entries: Vec<FlatEntry> = valid_urls.into_iter().map(|u| FlatEntry { id: None, url: Some(u.clone()), title: Some(u) }).collect();
            QueueItem {
                id: item_id, url: "batch".into(), title: "Batch Download Group".into(), platform: "Multi-Link".into(),
                description: format!("Contains {} pasted links.", entries.len()), status: ItemStatus::New, 
                available_formats: Vec::new(), chosen_format_id: None, is_audio: false, is_image: false,
                target_files: Vec::new(), stats: DownloadStats::default(), started_at: None,
                playlist_entries: Some(entries), target_folder: None, retries: 0, resolved_auth: initial_auth,
            }
        };

        self.queue.insert(0, item);
        self.queue_state.select(Some(0));
        true
    }

    fn current_history_len(&self) -> usize {
        match self.history_tab {
            HistoryTab::Downloads => self.history.download_history.len(),
            HistoryTab::Searches => self.history.search_history.len(),
        }
    }

    async fn nuke_queue_and_cancel(&mut self) {
        {
            let mut cancel_guard = self.active_worker_cancel.lock().await;
            for (_, cancel) in cancel_guard.drain() { let _ = cancel.send(()); }
        }
        tokio::time::sleep(Duration::from_millis(80)).await;
        for item in &self.queue {
            if item.status == ItemStatus::Downloading || item.status == ItemStatus::Paused {
                cleanup_item_garbage(item);
            }
        }
        self.active_downloads.clear();
        self.queue.clear();
        self.queue_state.select(None);
    }

    async fn skip_active_download(&mut self) {
        let mut target_idx = None;

        if let Some(sel_idx) = self.queue_state.selected() {
            if sel_idx < self.queue.len() {
                match self.queue[sel_idx].status {
                    ItemStatus::Downloading
                    | ItemStatus::Paused
                    | ItemStatus::Queued
                    | ItemStatus::FetchingMeta
                    | ItemStatus::New
                    | ItemStatus::Ready => {
                        target_idx = Some(sel_idx);
                    }
                    _ => {}
                }
            }
        }

        if target_idx.is_none() {
            target_idx = self.queue.iter().position(|i| {
                i.status == ItemStatus::Downloading || i.status == ItemStatus::Paused
            });
        }

        let Some(idx) = target_idx else { return; };
        let id = self.queue[idx].id;

        if self.active_downloads.contains(&id) {
            if let Some(cancel) = self.active_worker_cancel.lock().await.remove(&id) {
                let _ = cancel.send(());
            }
            self.active_downloads.remove(&id);
        }

        tokio::time::sleep(Duration::from_millis(60)).await;

        cleanup_item_garbage(&self.queue[idx]);
        self.queue[idx].status = ItemStatus::Skipped;
        self.queue[idx].stats = DownloadStats::default();

        self.process_next_in_queue().await;
    }

    async fn toggle_pause_download(&mut self) {
        let mut target_idx = None;
        if let Some(sel_idx) = self.queue_state.selected() {
            if sel_idx < self.queue.len() {
                let st = &self.queue[sel_idx].status;
                if *st == ItemStatus::Downloading || *st == ItemStatus::Paused {
                    target_idx = Some(sel_idx);
                }
            }
        }
        if target_idx.is_none() { target_idx = self.queue.iter().position(|i| i.status == ItemStatus::Downloading); }
        if target_idx.is_none() { target_idx = self.queue.iter().position(|i| i.status == ItemStatus::Paused); }

        let Some(idx) = target_idx else { return; };
        let item_id = self.queue[idx].id;

        if self.queue[idx].status == ItemStatus::Downloading {
            if let Some(cancel) = self.active_worker_cancel.lock().await.remove(&item_id) { let _ = cancel.send(()); }
            self.active_downloads.remove(&item_id);
            self.queue[idx].status = ItemStatus::Paused;
            self.queue[idx].stats.smoothed_speed = 0.0;
            self.process_next_in_queue().await;
        } else if self.queue[idx].status == ItemStatus::Paused {
            self.queue[idx].status = ItemStatus::Downloading;
            self.queue[idx].stats.last_update = Instant::now();
            self.queue[idx].stats.smoothed_speed = 0.0;
            let item_clone = self.queue[idx].clone();
            self.start_download_for_item(item_clone).await;
        }
    }

    async fn start_download_for_item(&mut self, item: QueueItem) {
        self.active_downloads.insert(item.id);
        let tx = self.worker_tx.clone();
        let item_id = item.id;
        
        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
        self.active_worker_cancel.lock().await.insert(item.id, cancel_tx);

        tokio::spawn(async move {
            let res = execute_download(item, tx.clone(), cancel_rx).await;
            match res {
                Ok(_) => { let _ = tx.send(WorkerMessage::Finished { id: item_id, success: true, error: None, was_killed: false }).await; }
                Err(e) => { 
                    let err_str = e.to_string();
                    let was_killed = err_str.contains("Canceled by user");
                    let _ = tx.send(WorkerMessage::Finished { id: item_id, success: false, error: Some(err_str), was_killed }).await; 
                }
            }
        });
    }

    async fn process_next_in_queue(&mut self) {
        while self.active_downloads.len() < self.max_concurrent_downloads {
            let queued_idx = self.queue.iter().position(|i| {
                i.status == ItemStatus::Queued && (i.chosen_format_id.is_some() || i.is_image)
            });
            if let Some(idx) = queued_idx {
                self.queue[idx].status = ItemStatus::Downloading;
                self.queue[idx].started_at = Some(Instant::now());
                self.queue[idx].stats.last_update = Instant::now();
                let clone = self.queue[idx].clone();
                self.start_download_for_item(clone).await;
            } else {
                break;
            }
        }
    }
}

// --- WORKER HELPERS & AGGRESSIVE CLEANUP ---

fn strip_ytdlp_format_suffix(stem: &str) -> &str {
    let mut s = stem.strip_suffix(".temp").unwrap_or(stem);
    if let Some(dot_idx) = s.rfind('.') {
        let after = &s[dot_idx + 1..];
        if after.len() >= 2
            && after.starts_with('f')
            && after[1..].chars().all(|c| c.is_ascii_digit())
        {
            s = &s[..dot_idx];
        }
    }
    s
}

fn normalize_ytdlp_unicode(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '｜' | '│' | '┃' | '¦' => '|',
            '：' => ':',
            '？' => '?',
            '＂' | '“' | '”' => '"',
            '＇' | '‘' | '’' => '\'',
            '⧸' | '╱' | '∕' => '/',
            '⧹' | '╲' => '\\',
            '＊' => '*',
            '＜' => '<',
            '＞' => '>',
            _ => c,
        })
        .collect()
}

fn clean_title_from_filename(file_name: &str) -> String {
    let raw_stem = Path::new(file_name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let without_fmt = strip_ytdlp_format_suffix(&raw_stem);
    let stripped = if let Some(u_idx) = without_fmt.rfind('_') {
        let tail = &without_fmt[u_idx + 1..];
        let is_res = tail == "NA"
            || (tail.ends_with('p') && tail[..tail.len() - 1].chars().all(|c| c.is_ascii_digit()))
            || (tail.contains('x') && tail.chars().all(|c| c.is_ascii_digit() || c == 'x'));
        if is_res && u_idx > 0 {
            &without_fmt[..u_idx]
        } else {
            without_fmt
        }
    } else {
        without_fmt
    };
    normalize_ytdlp_unicode(stripped)
}

fn cleanup_item_garbage(item: &QueueItem) {
    let mut base_prefixes: Vec<String> = Vec::new();

    for file in &item.target_files {
        let p = Path::new(file);
        let _ = std::fs::remove_file(p);
        let _ = std::fs::remove_file(format!("{}.part", file));
        let _ = std::fs::remove_file(format!("{}.ytdl", file));
        let _ = std::fs::remove_file(format!("{}.temp", file));

        if let Some(stem_os) = p.file_stem() {
            let stem_str = stem_os.to_string_lossy();
            let clean_base = strip_ytdlp_format_suffix(&stem_str);
            if !clean_base.is_empty() {
                base_prefixes.push(clean_base.to_lowercase());
            }

            if let Some(parent) = p.parent() {
                for base_candidate in [stem_str.as_ref(), clean_base] {
                    let base_path = parent.join(base_candidate);
                    for ext in ["mp4", "webm", "mkv", "m4a", "mp3", "webp", "jpg", "png", "part", "ytdl", "srt", "lrc", "vtt"] {
                        let _ = std::fs::remove_file(base_path.with_extension(ext));
                    }
                }
            }
        }
    }

    let target_dir = get_base_download_dir(item);
    if let Ok(entries) = std::fs::read_dir(target_dir) {
        let safe_title = item.title.replace(|c: char| !c.is_alphanumeric(), "").to_lowercase();
        let has_valid_title = !safe_title.is_empty()
            && safe_title != "playlisttrack"
            && safe_title != "imagegallery"
            && safe_title != "redditpost"
            && !safe_title.starts_with("http");

        for entry in entries.flatten() {
            let p = entry.path();
            if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                let lower_name = name.to_lowercase();
                let safe_name = name.replace(|c: char| !c.is_alphanumeric(), "").to_lowercase();

                let matches_prefix = base_prefixes.iter().any(|pref| !pref.is_empty() && lower_name.starts_with(pref));
                let matches_title = has_valid_title && safe_name.contains(&safe_title);

                if matches_prefix || matches_title {
                    let _ = std::fs::remove_file(&p);
                }
            }
        }
    }
}

fn parse_size(s: &str) -> f64 {
    let s = s.trim();
    let mut num_str = String::new();
    let mut unit_str = String::new();
    for c in s.chars() {
        if c.is_numeric() || c == '.' { num_str.push(c); }
        else { unit_str.push(c); }
    }
    let val = num_str.parse::<f64>().unwrap_or(0.0);
    match unit_str.to_uppercase().as_str() {
        "GIB" | "GB" | "G" => val * 1024.0 * 1024.0 * 1024.0,
        "MIB" | "MB" | "M" => val * 1024.0 * 1024.0,
        "KIB" | "KB" | "K" => val * 1024.0,
        _ => val,
    }
}

async fn run_metadata_cmd(url: &str, specific_auth: Option<&AuthMode>) -> Result<YtDlpInfo> {
    let mut cmd = Command::new(get_ytdlp_cmd());
    cmd.arg("-J").arg("--user-agent").arg(BROWSER_UA);
    
    if let Some(mode) = specific_auth {
        apply_specific_auth(&mut cmd, mode);
    }

    if url.contains("list=") || url.contains("/playlist") { cmd.arg("--flat-playlist"); } 
    else { cmd.arg("--no-playlist"); }

    cmd.arg(url).stdout(Stdio::piped()).stderr(Stdio::piped());
    let output_res = tokio::time::timeout(Duration::from_secs(35), cmd.output()).await;
    let output = match output_res {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => return Err(anyhow::anyhow!("Failed to run yt-dlp: {}", e)),
        Err(_) => return Err(anyhow::anyhow!("Timeout: yt-dlp stalled for 35s")),
    };

    if !output.status.success() {
        return Err(anyhow::anyhow!(String::from_utf8_lossy(&output.stderr).trim().to_string()));
    }

    let info: YtDlpInfo = serde_json::from_slice(&output.stdout)?;
    Ok(info)
}

async fn fetch_metadata(url: &str, auth_mode: AuthMode) -> Result<(YtDlpInfo, Option<AuthMode>)> {
    if auth_mode == AuthMode::SmartAuto {
        match run_metadata_cmd(url, None).await {
            Ok(info) => return Ok((info, None)),
            Err(first_err) => {
                let first_err_str = first_err.to_string();
                if is_no_video_error(&first_err_str) {
                    return Err(first_err);
                }

                if is_auth_wall_error(&first_err_str) {
                    let mut candidates = Vec::new();
                    if Path::new("cookies.txt").exists() {
                        candidates.push(AuthMode::CookiesFile);
                    }
                    candidates.push(AuthMode::Chrome);
                    candidates.push(AuthMode::Brave);
                    candidates.push(AuthMode::Firefox);
                    candidates.push(AuthMode::Edge);

                    let mut last_err = first_err;
                    for candidate in candidates {
                        match run_metadata_cmd(url, Some(&candidate)).await {
                            Ok(info) => return Ok((info, Some(candidate))),
                            Err(e) => {
                                let e_str = e.to_string();
                                if is_no_video_error(&e_str) {
                                    return Err(anyhow::anyhow!("No video formats found (Auth:{:?})", candidate));
                                }
                                if candidate == AuthMode::Chrome {
                                    last_err = e;
                                }
                            }
                        }
                    }
                    return Err(last_err);
                }
                return Err(first_err);
            }
        }
    }

    let info = run_metadata_cmd(url, Some(&auth_mode)).await?;
    Ok((info, Some(auth_mode)))
}

async fn run_reddit_headless_dom_download(
    item: &mut QueueItem,
    tx: &mpsc::Sender<WorkerMessage>,
    cancel_rx: &mut tokio::sync::oneshot::Receiver<()>,
) -> Result<()> {
    let item_id = item.id;
    let img_dir = get_base_download_dir(item);
    let _ = std::fs::create_dir_all(&img_dir);

    let py_bin = if std::env::consts::OS == "windows" { "python" } else { "python3" };
    let mut cmd = Command::new(py_bin);
    cmd.arg("-c")
        .arg(REDDIT_HEADLESS_DOM_PY)
        .arg(&item.url)
        .arg(&img_dir)
        .arg(BROWSER_UA)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn()?;
    let stdout = child.stdout.take().expect("Stdout missing");
    let stderr = child.stderr.take().expect("Stderr missing");
    let mut stdout_reader = BufReader::new(stdout).lines();
    let mut stderr_reader = BufReader::new(stderr).lines();

    let err_msg = Arc::new(Mutex::new(String::new()));
    let err_clone = err_msg.clone();

    tokio::spawn(async move {
        while let Ok(Some(line)) = stderr_reader.next_line().await {
            let mut e = err_clone.lock().await;
            if e.is_empty() || line.contains("ERROR:") {
                *e = line.clone();
            }
        }
    });

    let mut file_count = 0usize;
    let mut total_bytes_downloaded = 0.0;
    let mut direct_video_stream: Option<(String, String)> = None;

    loop {
        tokio::select! {
            _ = &mut *cancel_rx => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                return Err(anyhow::anyhow!("Canceled by user"));
            }
            line_res = stdout_reader.next_line() => {
                match line_res {
                    Ok(Some(line)) => {
                        let trimmed = line.trim();
                        if let Some(rest) = trimmed.strip_prefix("REDDIT_VIDEO::") {
                            let parts: Vec<&str> = rest.splitn(2, "::").collect();
                            if parts.len() == 2 {
                                direct_video_stream = Some((parts[0].to_string(), parts[1].to_string()));
                            }
                        } else if !trimmed.is_empty() {
                            file_count += 1;
                            if let Ok(meta) = std::fs::metadata(trimmed) {
                                total_bytes_downloaded += meta.len() as f64;
                            } else {
                                total_bytes_downloaded += 250_000.0;
                            }
                            let _ = tx.send(WorkerMessage::Filename { id: item_id, file_name: trimmed.to_string() }).await;
                            let progress_pct = (file_count as f32 * 25.0).min(95.0);
                            let _ = tx.send(WorkerMessage::Progress {
                                id: item_id,
                                percent: progress_pct,
                                dl_bytes: total_bytes_downloaded,
                                total_bytes: total_bytes_downloaded,
                            }).await;
                        }
                    }
                    Ok(None) => break,
                    Err(_) => break,
                }
            }
        }
    }

    let status = child.wait().await?;

    if let Some((video_title, hls_url)) = direct_video_stream {
        item.is_image = false;
        let _ = tx.send(WorkerMessage::SwitchToVideoEngine { id: item_id }).await;
        let vid_dir = get_base_download_dir(item);
        let _ = std::fs::create_dir_all(&vid_dir);
        let out_tmpl = vid_dir.join(format!("{}.%(ext)s", video_title)).to_string_lossy().to_string();

        let mut yt = Command::new(get_ytdlp_cmd());
        if let Some(ffmpeg_dir) = get_local_ffmpeg_dir() {
            yt.arg("--ffmpeg-location").arg(ffmpeg_dir);
        }
        yt.arg("--newline")
          .arg("--no-colors")
          .arg("--merge-output-format").arg("mp4")
          .arg("-o").arg(&out_tmpl)
          .arg(&hls_url);
        yt.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut yt_child = yt.spawn()?;
        let yt_stdout = yt_child.stdout.take().expect("Stdout missing");
        let mut yt_reader = BufReader::new(yt_stdout).lines();

        loop {
            tokio::select! {
                _ = &mut *cancel_rx => {
                    let _ = yt_child.kill().await;
                    let _ = yt_child.wait().await;
                    return Err(anyhow::anyhow!("Canceled by user"));
                }
                line_res = yt_reader.next_line() => {
                    match line_res {
                        Ok(Some(line)) => {
                            if line.starts_with("[download]") {
                                if line.contains("Destination:") {
                                    if let Some(dest) = line.split("Destination:").nth(1) {
                                        let _ = tx.send(WorkerMessage::Filename { id: item_id, file_name: dest.trim().to_string() }).await;
                                    }
                                } else if line.contains('%') {
                                    let parts: Vec<&str> = line.split_whitespace().collect();
                                    if let Some(pct_idx) = parts.iter().position(|p| p.ends_with('%')) {
                                        let percent_str = &parts[pct_idx][..parts[pct_idx].len()-1];
                                        if let Ok(percent) = percent_str.parse::<f32>() {
                                            if parts.len() > pct_idx + 2 && parts[pct_idx + 1] == "of" {
                                                let mut size_str = parts[pct_idx + 2];
                                                if size_str.starts_with('~') { size_str = &size_str[1..]; }
                                                let total_bytes = parse_size(size_str);
                                                let dl_bytes = (percent as f64 / 100.0) * total_bytes;
                                                let _ = tx.send(WorkerMessage::Progress { id: item_id, percent, dl_bytes, total_bytes }).await;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Ok(None) => break,
                        Err(_) => break,
                    }
                }
            }
        }
        let yt_status = yt_child.wait().await?;
        if yt_status.success() {
            return Ok(());
        }
    }

    if status.success() && file_count > 0 {
        return Ok(());
    }

    let e = err_msg.lock().await.clone();
    Err(anyhow::anyhow!(if e.is_empty() {
        "Reddit Headless DOM extractor could not find media on this page.".to_string()
    } else {
        e
    }))
}

async fn run_gallery_dl_download(
    item: &mut QueueItem,
    base_dir: &Path,
    tx: &mpsc::Sender<WorkerMessage>,
    cancel_rx: &mut tokio::sync::oneshot::Receiver<()>,
) -> Result<()> {
    let item_id = item.id;
    let _ = std::fs::create_dir_all(base_dir);

    if is_reddit_url(&item.url) {
        return run_reddit_headless_dom_download(item, tx, cancel_rx).await;
    }

    let mut cmd = Command::new(get_gallerydl_cmd());
    if let Some(ref mode) = item.resolved_auth {
        apply_specific_auth(&mut cmd, mode);
    }
    cmd.arg("--user-agent").arg(BROWSER_UA)
       .arg("-D").arg(base_dir)
       .arg(&item.url);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                return Err(anyhow::anyhow!(
                    "gallery-dl is not installed. Run: pip3 install -U gallery-dl --break-system-packages"
                ));
            }
            return Err(e.into());
        }
    };

    let stdout = child.stdout.take().expect("Stdout missing");
    let stderr = child.stderr.take().expect("Stderr missing");
    let mut stdout_reader = BufReader::new(stdout).lines();
    let mut stderr_reader = BufReader::new(stderr).lines();

    let err_msg = Arc::new(Mutex::new(String::new()));
    let err_clone = err_msg.clone();

    tokio::spawn(async move {
        while let Ok(Some(line)) = stderr_reader.next_line().await {
            let mut e = err_clone.lock().await;
            if e.is_empty() || line.to_lowercase().contains("error") || line.to_lowercase().contains("redirect") {
                *e = line.clone();
            }
        }
    });

    let mut file_count = 0usize;
    let mut total_bytes_downloaded = 0.0;

    loop {
        tokio::select! {
            _ = &mut *cancel_rx => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                return Err(anyhow::anyhow!("Canceled by user"));
            }
            line_res = stdout_reader.next_line() => {
                match line_res {
                    Ok(Some(line)) => {
                        let trimmed = line.trim().trim_start_matches("# ").trim();
                        if !trimmed.is_empty() && !trimmed.starts_with('[') {
                            file_count += 1;
                            if let Ok(meta) = std::fs::metadata(trimmed) {
                                total_bytes_downloaded += meta.len() as f64;
                            } else {
                                total_bytes_downloaded += 250_000.0;
                            }
                            let _ = tx.send(WorkerMessage::Filename { id: item_id, file_name: trimmed.to_string() }).await;
                            let progress_pct = (file_count as f32 * 25.0).min(95.0);
                            let _ = tx.send(WorkerMessage::Progress {
                                id: item_id,
                                percent: progress_pct,
                                dl_bytes: total_bytes_downloaded,
                                total_bytes: total_bytes_downloaded,
                            }).await;
                        }
                    }
                    Ok(None) => break,
                    Err(_) => break,
                }
            }
        }
    }

    let status = child.wait().await?;
    let e = err_msg.lock().await.clone();
    if !status.success() || (file_count == 0 && !e.is_empty()) {
        if e.is_empty() {
            return Err(anyhow::anyhow!(
                "Login Required: Please log into this site in your browser (Chrome/Firefox/Brave/Edge) or add cookies.txt, then press [Enter] to retry."
            ));
        } else {
            return Err(anyhow::anyhow!(e));
        }
    }
    Ok(())
}

async fn execute_download(
    mut item: QueueItem,
    tx: mpsc::Sender<WorkerMessage>,
    mut cancel_rx: tokio::sync::oneshot::Receiver<()>,
) -> Result<()> {
    let item_id = item.id;

    let resolved_url = resolve_short_url(&item.url).await;
    if resolved_url != item.url {
        item.url = resolved_url.clone();
        let _ = tx.send(WorkerMessage::UrlResolved { id: item_id, resolved_url }).await;
    }

    if !item.is_image && (is_direct_image_or_gallery_url(&item.url) || is_reddit_url(&item.url)) {
        item.is_image = true;
        let _ = tx.send(WorkerMessage::SwitchToImageEngine { id: item_id }).await;
    }

    let base_dir = get_base_download_dir(&item);
    let _ = std::fs::create_dir_all(&base_dir);

    if item.is_image {
        return run_gallery_dl_download(&mut item, &base_dir, &tx, &mut cancel_rx).await;
    }

    let mut cmd = Command::new(get_ytdlp_cmd());
    cmd.arg("--newline")
       .arg("--continue")
       .arg("--no-colors")
       .arg("--ignore-errors")
       .arg("--user-agent").arg(BROWSER_UA);
    
    if let Some(ffmpeg_dir) = get_local_ffmpeg_dir() {
        cmd.arg("--ffmpeg-location").arg(ffmpeg_dir);
    }

    if let Some(ref mode) = item.resolved_auth {
        apply_specific_auth(&mut cmd, mode);
    }
    
    cmd.arg("--sleep-requests").arg("1").arg("--min-sleep-interval").arg("1");

    if let Some(fid) = &item.chosen_format_id {
        if item.is_audio {
            cmd.arg("-x")
               .arg("--audio-format").arg("mp3")
               .arg("--embed-thumbnail")
               .arg("--add-metadata")
               .arg("-o").arg(base_dir.join("%(title)s.%(ext)s").to_string_lossy().to_string());
        } else {
            if fid.contains('+') {
                cmd.arg("-f").arg(fid).arg("--merge-output-format").arg("mp4");
            } else {
                cmd.arg("-f").arg(fid);
            }
            cmd.arg("--embed-subs")
               .arg("--sub-langs").arg("orig,en")
               .arg("--embed-thumbnail")
               .arg("-o").arg(base_dir.join("%(title)s_%(resolution)s.%(ext)s").to_string_lossy().to_string());
        }
    } else {
        cmd.arg("-f").arg("best").arg("-o").arg(base_dir.join("%(title)s.%(ext)s").to_string_lossy().to_string());
    }
    cmd.arg(&item.url).stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = cmd.spawn()?;
    let stdout = child.stdout.take().expect("Stdout missing");
    let stderr = child.stderr.take().expect("Stderr missing");

    let mut stdout_reader = BufReader::new(stdout).lines();
    let mut stderr_reader = BufReader::new(stderr).lines();

    let err_msg = Arc::new(Mutex::new(String::new()));
    let err_clone = err_msg.clone();

    tokio::spawn(async move {
        while let Ok(Some(line)) = stderr_reader.next_line().await {
            let mut e = err_clone.lock().await;
            if line.contains("ERROR:") { *e = line.clone(); } 
            else if e.is_empty() && line.contains("WARNING:") { *e = line.clone(); }
        }
    });

    let mut downloaded_any = false;

    loop {
        tokio::select! {
            _ = &mut cancel_rx => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                return Err(anyhow::anyhow!("Canceled by user"));
            }
            line_res = stdout_reader.next_line() => {
                match line_res {
                    Ok(Some(line)) => {
                        if line.starts_with("[download]") {
                            if line.contains("Destination:") {
                                downloaded_any = true;
                                if let Some(dest) = line.split("Destination:").nth(1) {
                                    let _ = tx.send(WorkerMessage::Filename { id: item_id, file_name: dest.trim().to_string() }).await;
                                }
                            } else if line.contains("has already been downloaded") {
                                downloaded_any = true;
                                if let Some(dest) = line.strip_prefix("[download]") {
                                    if let Some(path_part) = dest.split("has already been downloaded").next() {
                                        let _ = tx.send(WorkerMessage::Filename { id: item_id, file_name: path_part.trim().to_string() }).await;
                                    }
                                }
                            } else if line.contains('%') {
                                downloaded_any = true;
                                let parts: Vec<&str> = line.split_whitespace().collect();
                                if let Some(pct_idx) = parts.iter().position(|p| p.ends_with('%')) {
                                    let percent_str = &parts[pct_idx][..parts[pct_idx].len()-1];
                                    if let Ok(percent) = percent_str.parse::<f32>() {
                                        if parts.len() > pct_idx + 2 && parts[pct_idx + 1] == "of" {
                                            let mut size_str = parts[pct_idx + 2];
                                            if size_str.starts_with('~') { size_str = &size_str[1..]; }
                                            let total_bytes = parse_size(size_str);
                                            let dl_bytes = (percent as f64 / 100.0) * total_bytes;
                                            let _ = tx.send(WorkerMessage::Progress { id: item_id, percent, dl_bytes, total_bytes }).await;
                                        }
                                    }
                                }
                            }
                        } else if line.contains("Merging formats into") {
                            if let Some(dest) = line.split("into").nth(1) {
                                let clean_dest = dest.trim().trim_matches('"');
                                let _ = tx.send(WorkerMessage::Filename { id: item_id, file_name: clean_dest.to_string() }).await;
                            }
                        } else if line.contains("Writing video thumbnail") && line.contains("to:") {
                            if let Some(dest) = line.split("to:").nth(1) {
                                let clean_dest = dest.trim().trim_matches('"');
                                let _ = tx.send(WorkerMessage::Filename { id: item_id, file_name: clean_dest.to_string() }).await;
                            }
                        }
                    }
                    Ok(None) => break,
                    Err(_) => break,
                }
            }
        }
    }

    let status = child.wait().await?;
    remove_sidecar_subtitles_in_dir(&base_dir);

    let e = err_msg.lock().await.clone();

    if !status.success() || (!downloaded_any && !e.is_empty()) {
        if is_no_video_error(&e) {
            item.is_image = true;
            let _ = tx.send(WorkerMessage::SwitchToImageEngine { id: item_id }).await;
            let img_dir = get_base_download_dir(&item);
            return run_gallery_dl_download(&mut item, &img_dir, &tx, &mut cancel_rx).await;
        }

        if e.is_empty() {
            return Err(anyhow::anyhow!("Process exited with error"));
        } else {
            return Err(anyhow::anyhow!(e));
        }
    }
    Ok(())
}

fn get_quick_presets() -> Vec<FormatChoice> {
    vec![
        FormatChoice {
            id: "bestvideo[height<=1080]+bestaudio/best[height<=1080]/best".into(),
            label: "🎬 Best Video (Max 1080p Full HD + Embedded CC)".into(),
            ext: "mp4".into(), est_size: "Auto".into(), is_audio: false, is_image: false,
        },
        FormatChoice {
            id: "bestvideo+bestaudio/best".into(),
            label: "💎 Ultra 4K / 8K Video (Unrestricted Max + CC)".into(),
            ext: "mp4".into(), est_size: "Auto".into(), is_audio: false, is_image: false,
        },
        FormatChoice {
            id: "bestvideo[height<=720]+bestaudio/best[height<=720]/best".into(),
            label: "📺 Medium Quality Video (Max 720p + Embedded CC)".into(),
            ext: "mp4".into(), est_size: "Auto".into(), is_audio: false, is_image: false,
        },
        FormatChoice {
            id: "bestvideo[height<=480]+bestaudio/best[height<=480]/best".into(),
            label: "📱 Data Saver Video (Max 480p + Embedded CC)".into(),
            ext: "mp4".into(), est_size: "Auto".into(), is_audio: false, is_image: false,
        },
        FormatChoice {
            id: "bestaudio/best".into(),
            label: "🎵 MP3 320kbps Audio (Single File + Cover Art)".into(),
            ext: "mp3".into(), est_size: "Auto".into(), is_audio: true, is_image: false,
        },
        FormatChoice {
            id: "gallery".into(),
            label: "🖼️ Download Images / Gallery (via gallery-dl)".into(),
            ext: "img".into(), est_size: "Orig".into(), is_audio: false, is_image: true,
        },
    ]
}

fn parse_available_formats(info: &YtDlpInfo) -> Vec<FormatChoice> {
    let mut results = get_quick_presets();
    let mut best_audio_size = 0.0;
    
    if let Some(formats) = &info.formats {
        for f in formats {
            if f.vcodec.as_deref().unwrap_or("none") == "none" || f.vcodec.as_deref().unwrap_or("none") == "null" {
                let size = f.filesize.or(f.filesize_approx).unwrap_or(0) as f64;
                if size > best_audio_size { best_audio_size = size; }
            }
        }
        
        let mut available_heights: Vec<u32> = formats.iter()
            .filter(|f| f.vcodec.as_deref().unwrap_or("none") != "none" && f.vcodec.as_deref().unwrap_or("none") != "null")
            .filter_map(|f| f.height).collect();
            
        available_heights.sort_by(|a, b| b.cmp(a));
        available_heights.dedup();
        
        for h in available_heights {
            let mut best_vid_size = 0.0;
            for f in formats {
                if f.height == Some(h) {
                    let size = f.filesize.or(f.filesize_approx).unwrap_or(0) as f64;
                    if size > best_vid_size { best_vid_size = size; }
                }
            }
            let total_size = (best_vid_size + best_audio_size) / 1_048_576.0;
            let size_str = if total_size > 0.1 { format!("~{:.1} MB", total_size) } else { "Dynamic".into() };
            
            results.push(FormatChoice {
                id: format!("bestvideo[height<={h}]+bestaudio/best[height<={h}]/best"),
                label: format!("   ↳ Exact {}p Stream", h),
                ext: "mp4".into(), est_size: size_str, is_audio: false, is_image: false,
            });
        }
    }
    results
}

fn format_bytes(bytes: f64) -> String {
    if bytes >= 1_048_576.0 { format!("{:.1} MB", bytes / 1_048_576.0) }
    else if bytes >= 1024.0 { format!("{:.1} KB", bytes / 1024.0) }
    else { format!("{:.0} B", bytes) }
}

fn format_time(secs: f64) -> String {
    if secs <= 0.0 || secs.is_nan() || secs.is_infinite() { return "--:--".into(); }
    let s = secs as u64; let m = s / 60; let h = m / 60;
    if h > 0 { format!("{:02}:{:02}:{:02}", h, m % 60, s % 60) }
    else { format!("{:02}:{:02}", m, s % 60) }
}

// --- MAIN RUNNER ---

#[tokio::main]
async fn main() -> Result<()> {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableBracketedPaste);
        original_hook(panic_info);
    }));

    let yt_cmd = get_ytdlp_cmd();
    if std::process::Command::new(&yt_cmd).arg("--version").output().is_err() {
        println!("CRITICAL ERROR: '{}' was not found on this system!", yt_cmd);
        println!("Please place yt-dlp inside the same folder as this application, or install it system-wide.");
        println!("Press Enter to exit...");
        let _ = std::io::stdin().read_line(&mut String::new());
        return Ok(());
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableBracketedPaste)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (worker_tx, worker_rx) = mpsc::channel(100);
    let mut app = App::new(worker_tx.clone(), worker_rx);

    // Background GitHub Release Auto-Update Checker
    let upd_tx = worker_tx.clone();
    tokio::spawn(async move {
        if let Some(tag) = check_github_for_updates().await {
            let _ = upd_tx.send(WorkerMessage::UpdateAvailable { tag }).await;
        }
    });

    let clip_tx = worker_tx.clone();
    std::thread::spawn(move || {
        if let Ok(mut clipboard) = Clipboard::new() {
            let mut last_clipboard = clipboard.get_text().unwrap_or_default();
            loop {
                std::thread::sleep(Duration::from_millis(1500));
                if let Ok(text) = clipboard.get_text() {
                    if text != last_clipboard {
                        last_clipboard = text.clone();
                        let trimmed = text.trim();
                        if (trimmed.starts_with("http://") || trimmed.starts_with("https://"))
                            && !trimmed.contains('\n')
                            && (trimmed.contains("youtube.com") || trimmed.contains("youtu.be") || 
                                trimmed.contains("instagram.com") || trimmed.contains("reddit.com") || 
                                trimmed.contains("redd.it") || trimmed.contains("imgur.com") ||
                                trimmed.contains("pinterest.com") || trimmed.contains("pin.it") ||
                                trimmed.contains("x.com") || trimmed.contains("twitter.com") ||
                                trimmed.contains("tiktok.com") || trimmed.contains("vimeo.com") ||
                                trimmed.contains("twitch.tv") || trimmed.contains("soundcloud.com"))
                        {
                            let _ = clip_tx.blocking_send(WorkerMessage::ClipboardCaptured { url: trimmed.to_string() });
                        }
                    }
                }
            }
        }
    });

    let tick_rate = Duration::from_millis(50);
    let mut last_tick = Instant::now();

    loop {
        if let Some((_, ts)) = &app.input_warning {
            if ts.elapsed() > Duration::from_secs(3) {
                app.input_warning = None;
            }
        }

        terminal.draw(|f| ui(f, &mut app))?;

        while let Ok(msg) = app.worker_rx.try_recv() {
            match msg {
                WorkerMessage::UpdateAvailable { tag } => {
                    app.available_update = Some(tag);
                }
                WorkerMessage::ClipboardCaptured { url } => {
                    let _ = app.add_urls_to_queue(url);
                }
                WorkerMessage::UrlResolved { id, resolved_url } => {
                    if let Some(item) = app.queue.iter_mut().find(|i| i.id == id) {
                        item.url = resolved_url;
                    }
                }
                WorkerMessage::SwitchToImageEngine { id } => {
                    if let Some(item) = app.queue.iter_mut().find(|i| i.id == id) {
                        item.is_image = true;
                        item.is_audio = false;
                    }
                }
                WorkerMessage::SwitchToVideoEngine { id } => {
                    if let Some(item) = app.queue.iter_mut().find(|i| i.id == id) {
                        item.is_image = false;
                        item.is_audio = false;
                    }
                }
                WorkerMessage::MetaFetched { id, info, resolved_auth } => {
                    if let Some(item) = app.queue.iter_mut().find(|i| i.id == id) {
                        if info.is_live.unwrap_or(false) {
                            item.status = ItemStatus::Failed("Live streams are not supported.".into());
                            continue;
                        }

                        item.resolved_auth = resolved_auth;
                        item.title = normalize_ytdlp_unicode(&info.title.clone().unwrap_or_else(|| "Untitled".into()));
                        let ext_key = info.extractor_key.clone().unwrap_or_else(|| "Web".into());
                        item.platform = detect_platform_folder(&item.url, &ext_key).to_string();
                        item.description = info.description.clone().unwrap_or_default();

                        if info.entries.is_some() || info._type.as_deref() == Some("playlist") {
                            item.playlist_entries = info.entries;
                            item.available_formats = get_quick_presets();
                        } else {
                            item.available_formats = parse_available_formats(&info);
                        }
                        item.status = ItemStatus::FormatSelect;
                    }
                }
                WorkerMessage::ImageDetected { id, platform, resolved_auth } => {
                    if let Some(item) = app.queue.iter_mut().find(|i| i.id == id) {
                        item.platform = platform;
                        if resolved_auth.is_some() {
                            item.resolved_auth = resolved_auth;
                        }
                        item.title = if is_reddit_url(&item.url) { "Reddit Post".into() } else { "Image Gallery".into() };
                        item.description = "Uses Headless Browser DOM & Smart Auth Engine (auto-detects images, galleries & v.redd.it video).".into();
                        item.available_formats = vec![FormatChoice {
                            id: "gallery".into(),
                            label: "🖼️ Download Post Media (Original Quality)".into(),
                            ext: "auto".into(),
                            est_size: "Auto".into(),
                            is_audio: false,
                            is_image: true,
                        }];
                        item.status = ItemStatus::FormatSelect;
                    }
                }
                WorkerMessage::MetaFailed { id, error } => {
                    if let Some(item) = app.queue.iter_mut().find(|i| i.id == id) {
                        if is_auth_wall_error(&error) {
                            item.status = ItemStatus::Failed(
                                "Login Required: Please log into this site in your browser (Chrome/Firefox/Brave/Edge) or add cookies.txt, then press [Enter] to retry.".into()
                            );
                        } else {
                            item.status = ItemStatus::Failed(error);
                        }
                    }
                }
                WorkerMessage::Filename { id, file_name } => {
                    if let Some(item) = app.queue.iter_mut().find(|i| i.id == id) { 
                        item.target_files.push(file_name.clone()); 
                        
                        if item.title.starts_with("http")
                            || item.title == "Playlist Track"
                            || item.title == "Image Gallery"
                            || item.title == "Reddit Post"
                        {
                            let cleaned = clean_title_from_filename(&file_name);
                            if !cleaned.is_empty() {
                                item.title = cleaned;
                                item.platform = detect_platform_folder(&item.url, &item.platform).to_string();
                                item.description = "Metadata dynamically updated during download...".into();
                            }
                        }
                    }
                }
                WorkerMessage::Progress { id, percent, dl_bytes, total_bytes } => {
                    if let Some(item) = app.queue.iter_mut().find(|i| i.id == id) {
                        item.stats.percent = percent;
                        item.stats.downloaded_bytes = dl_bytes;
                        item.stats.total_bytes = total_bytes;

                        let now = Instant::now();
                        let dt = now.duration_since(item.stats.last_update).as_secs_f64();
                        if dt > 0.5 {
                            let current_speed = (dl_bytes - item.stats.last_dl_bytes) / dt;
                            if item.stats.smoothed_speed == 0.0 { item.stats.smoothed_speed = current_speed.max(0.0); }
                            else { item.stats.smoothed_speed = (0.1 * current_speed + 0.9 * item.stats.smoothed_speed).max(0.0); }
                            item.stats.last_dl_bytes = dl_bytes;
                            item.stats.last_update = now;
                        }
                        if let Some(start) = item.started_at { item.stats.elapsed = start.elapsed(); }
                    }
                }
                WorkerMessage::Finished { id, success, error, was_killed } => {
                    app.active_downloads.remove(&id);
                    if let Some(idx) = app.queue.iter().position(|i| i.id == id) {
                        app.queue[idx].stats.smoothed_speed = 0.0;

                        if app.queue[idx].status == ItemStatus::Skipped {
                            cleanup_item_garbage(&app.queue[idx]);
                            app.queue[idx].target_files.clear();
                            continue;
                        }

                        if !was_killed && app.queue[idx].status == ItemStatus::Downloading {
                            if success {
                                app.queue[idx].status = ItemStatus::Completed;
                                app.queue[idx].stats.percent = 100.0;
                                let completed_item = app.queue[idx].clone();
                                app.history.record_download(&completed_item);
                            } else {
                                let err_str = error.unwrap_or_else(|| "Error".into());
                                let item_url = app.queue[idx].url.clone();
                                if !is_reddit_url(&item_url) && is_auth_wall_error(&err_str) {
                                    let next_auth = match app.queue[idx].resolved_auth {
                                        None => {
                                            if Path::new("cookies.txt").exists() {
                                                Some(AuthMode::CookiesFile)
                                            } else {
                                                Some(AuthMode::Chrome)
                                            }
                                        }
                                        Some(AuthMode::CookiesFile) => Some(AuthMode::Chrome),
                                        Some(AuthMode::Chrome) => Some(AuthMode::Brave),
                                        Some(AuthMode::Brave) => Some(AuthMode::Firefox),
                                        Some(AuthMode::Firefox) => Some(AuthMode::Edge),
                                        Some(_) => None,
                                    };

                                    if let Some(escalated) = next_auth {
                                        app.queue[idx].resolved_auth = Some(escalated);
                                        app.queue[idx].status = ItemStatus::Queued;
                                        continue;
                                    }
                                }

                                if app.queue[idx].retries < 2 && !is_auth_wall_error(&err_str) {
                                    app.queue[idx].retries += 1;
                                    app.queue[idx].status = ItemStatus::Queued;
                                } else if !is_reddit_url(&item_url) && is_auth_wall_error(&err_str) {
                                    app.queue[idx].status = ItemStatus::Failed(
                                        "Login Required: Please log into this site in your browser (Chrome/Firefox/Brave/Edge) or add cookies.txt, then press [Enter] to retry.".into()
                                    );
                                } else {
                                    app.queue[idx].status = ItemStatus::Failed(err_str);
                                }
                            }
                        }
                    }
                }
            }
        }

        app.process_next_in_queue().await;

        let timeout = tick_rate.saturating_sub(last_tick.elapsed());
        if event::poll(timeout)? {
            match event::read()? {
                Event::Paste(pasted_text) => {
                    if !app.show_auth_modal && !app.show_history_modal {
                        let combined = if app.input_url.is_empty() {
                            pasted_text
                        } else {
                            format!("{} {}", app.input_url, pasted_text)
                        };
                        app.input_url.clear();
                        if !app.add_urls_to_queue(combined) {
                            app.set_input_warning("⚠️ Ignored non-URL text! Paste a valid http:// or https:// link.");
                        } else {
                            app.input_warning = None;
                        }
                    }
                }
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    if app.show_history_modal {
                        let len = app.current_history_len();
                        match key.code {
                            KeyCode::Tab | KeyCode::Left | KeyCode::Right => {
                                app.history_tab = match app.history_tab {
                                    HistoryTab::Downloads => HistoryTab::Searches,
                                    HistoryTab::Searches => HistoryTab::Downloads,
                                };
                                app.history_state.select(Some(0));
                            }
                            KeyCode::Up => {
                                let cur = app.history_state.selected().unwrap_or(0);
                                if cur > 0 { app.history_state.select(Some(cur - 1)); }
                            }
                            KeyCode::Down => {
                                let cur = app.history_state.selected().unwrap_or(0);
                                if cur + 1 < len { app.history_state.select(Some(cur + 1)); }
                            }
                            KeyCode::Enter => {
                                let sel = app.history_state.selected().unwrap_or(0);
                                let target_url = match app.history_tab {
                                    HistoryTab::Downloads => app.history.download_history.get(sel).map(|e| e.url.clone()),
                                    HistoryTab::Searches => app.history.search_history.get(sel).map(|e| e.url.clone()),
                                };
                                if let Some(u) = target_url {
                                    let _ = app.add_urls_to_queue(u);
                                    app.show_history_modal = false;
                                }
                            }
                            KeyCode::F(9) => {
                                if app.history_tab == HistoryTab::Downloads {
                                    let sel = app.history_state.selected().unwrap_or(0);
                                    if let Some(entry) = app.history.download_history.get(sel) {
                                        open_folder_in_os(Path::new(&entry.save_path));
                                    }
                                }
                            }
                            KeyCode::Delete => {
                                let sel = app.history_state.selected().unwrap_or(0);
                                match app.history_tab {
                                    HistoryTab::Downloads => {
                                        if sel < app.history.download_history.len() {
                                            app.history.download_history.remove(sel);
                                            app.history.save();
                                        }
                                    }
                                    HistoryTab::Searches => {
                                        if sel < app.history.search_history.len() {
                                            app.history.search_history.remove(sel);
                                            app.history.save();
                                        }
                                    }
                                }
                                let new_len = app.current_history_len();
                                if new_len == 0 {
                                    app.history_state.select(Some(0));
                                } else if sel >= new_len {
                                    app.history_state.select(Some(new_len - 1));
                                }
                            }
                            KeyCode::F(7) => {
                                match app.history_tab {
                                    HistoryTab::Downloads => app.history.download_history.clear(),
                                    HistoryTab::Searches => app.history.search_history.clear(),
                                }
                                app.history.save();
                                app.history_state.select(Some(0));
                            }
                            KeyCode::Esc | KeyCode::Backspace | KeyCode::F(4) | KeyCode::F(12) => {
                                app.show_history_modal = false;
                            }
                            _ => {}
                        }
                        continue;
                    }

                    if app.show_auth_modal {
                        let modes = AuthMode::all();
                        match key.code {
                            KeyCode::Up => {
                                let cur = app.auth_state.selected().unwrap_or(0);
                                if cur > 0 { app.auth_state.select(Some(cur - 1)); }
                            }
                            KeyCode::Down => {
                                let cur = app.auth_state.selected().unwrap_or(0);
                                if cur + 1 < modes.len() { app.auth_state.select(Some(cur + 1)); }
                            }
                            KeyCode::Enter => {
                                let sel = app.auth_state.selected().unwrap_or(0);
                                if let Some(chosen) = modes.into_iter().nth(sel) {
                                    app.auth_mode = chosen;
                                }
                                app.show_auth_modal = false;
                            }
                            KeyCode::Esc | KeyCode::Backspace | KeyCode::F(8) => {
                                app.show_auth_modal = false;
                            }
                            _ => {}
                        }
                        continue;
                    }

                    let active_format_idx = app.queue.iter().position(|i| i.status == ItemStatus::FormatSelect);

                    if let Some(idx) = active_format_idx {
                        let total_formats = app.queue[idx].available_formats.len();
                        match key.code {
                            KeyCode::Up => {
                                let cur = app.format_state.selected().unwrap_or(0);
                                if cur > 0 { app.format_state.select(Some(cur - 1)); }
                            }
                            KeyCode::Down => {
                                let cur = app.format_state.selected().unwrap_or(0);
                                if cur + 1 < total_formats { app.format_state.select(Some(cur + 1)); }
                            }
                            KeyCode::Enter => {
                                let sel = app.format_state.selected().unwrap_or(0);
                                if let Some(format_choice) = app.queue[idx].available_formats.get(sel).cloned() {
                                    let chosen_format = format_choice.id;
                                    let is_audio = format_choice.is_audio;
                                    let is_image = format_choice.is_image;
                                    let parent_auth = app.queue[idx].resolved_auth.clone();

                                    if let Some(entries) = app.queue[idx].playlist_entries.take() {
                                        let folder_name = app.queue[idx].title.clone();
                                        let is_batch_paste = folder_name == "Batch Download Group";
                                        let mut new_items = Vec::new();

                                        for e in entries {
                                            let entry_url = e.url.or(e.id.map(|id| format!("https://youtu.be/{}", id))).unwrap_or_default();
                                            let entry_title = normalize_ytdlp_unicode(&e.title.unwrap_or_else(|| "Playlist Track".into()));
                                            if entry_url.is_empty() { continue; }

                                            let item_id = app.next_id;
                                            app.next_id += 1;

                                            let item_is_image = is_image || is_direct_image_or_gallery_url(&entry_url) || is_reddit_url(&entry_url);
                                            let item_platform = detect_platform_folder(&entry_url, "").to_string();

                                            new_items.push(QueueItem {
                                                id: item_id, url: entry_url, title: entry_title, platform: item_platform,
                                                description: String::new(), status: ItemStatus::Queued, 
                                                available_formats: get_quick_presets(), chosen_format_id: Some(chosen_format.clone()),
                                                is_audio: if item_is_image { false } else { is_audio },
                                                is_image: item_is_image,
                                                target_files: Vec::new(), stats: DownloadStats::default(),
                                                started_at: None, playlist_entries: None,
                                                target_folder: if is_batch_paste { None } else { Some(folder_name.clone()) },
                                                retries: 0, resolved_auth: parent_auth.clone(),
                                            });
                                        }

                                        app.queue.remove(idx);
                                        for (offset, item) in new_items.into_iter().enumerate() {
                                            app.queue.insert(idx + offset, item);
                                        }
                                        app.format_state.select(Some(0));
                                    } else {
                                        app.queue[idx].chosen_format_id = Some(chosen_format);
                                        app.queue[idx].is_audio = is_audio;
                                        app.queue[idx].is_image = is_image;
                                        app.queue[idx].status = ItemStatus::Queued;
                                        app.format_state.select(Some(0));
                                    }
                                }
                            }
                            KeyCode::Esc | KeyCode::Backspace => {
                                app.queue[idx].status = ItemStatus::Ready;
                                app.format_state.select(Some(0));
                            }
                            _ => {}
                        }
                    } else {
                        match key.code {
                            KeyCode::Esc => break,
                            KeyCode::F(3) => {
                                if app.available_update.is_some() {
                                    app.nuke_queue_and_cancel().await;
                                    app.trigger_self_update_exit = true;
                                    break;
                                }
                            }
                            KeyCode::F(4) | KeyCode::F(12) => {
                                app.show_history_modal = true;
                                app.history_state.select(Some(0));
                            }
                            KeyCode::F(5) => { app.toggle_pause_download().await; }
                            KeyCode::F(6) => { app.skip_active_download().await; }
                            KeyCode::F(7) => { app.nuke_queue_and_cancel().await; }
                            KeyCode::F(8) => { app.show_auth_modal = true; }
                            KeyCode::F(9) => {
                                if let Some(idx) = app.queue_state.selected() {
                                    if let Some(item) = app.queue.get(idx) {
                                        match item.status {
                                            ItemStatus::Completed | ItemStatus::Downloading | ItemStatus::Paused => {
                                                let target_dir = get_base_download_dir(item);
                                                open_folder_in_os(&target_dir);
                                            }
                                            ItemStatus::Skipped => {
                                                app.set_input_warning("⚠️ Cannot open folder: This item was skipped and its files were deleted.");
                                            }
                                            _ => {
                                                app.set_input_warning("⚠️ Cannot open folder: This item has not been downloaded yet.");
                                            }
                                        }
                                    }
                                }
                            }
                            KeyCode::Delete => {
                                if let Some(idx) = app.queue_state.selected() {
                                    if !app.queue.is_empty() && app.queue[idx].status != ItemStatus::Downloading {
                                        cleanup_item_garbage(&app.queue[idx]);
                                        app.queue.remove(idx);
                                        if app.queue.is_empty() { app.queue_state.select(None); }
                                        else if idx >= app.queue.len() { app.queue_state.select(Some(app.queue.len() - 1)); }
                                    }
                                }
                            }
                            KeyCode::Enter => {
                                if !app.input_url.is_empty() {
                                    let text = app.input_url.clone();
                                    app.input_url.clear();
                                    if !app.add_urls_to_queue(text) {
                                        app.set_input_warning("⚠️ Ignored non-URL text! Paste a valid http:// or https:// link.");
                                    } else {
                                        app.input_warning = None;
                                    }
                                } else if let Some(idx) = app.queue_state.selected() {
                                    let status = app.queue[idx].status.clone();
                                    match status {
                                        // Skipped items are permanently dead and cannot be restarted with [Enter]
                                        ItemStatus::New | ItemStatus::Failed(_) | ItemStatus::Ready => {
                                            let raw_url = app.queue[idx].url.clone();
                                            app.queue[idx].retries = 0;
                                            app.queue[idx].resolved_auth = if app.auth_mode == AuthMode::SmartAuto {
                                                None
                                            } else {
                                                Some(app.auth_mode.clone())
                                            };

                                            if raw_url == "batch" {
                                                app.queue[idx].available_formats = get_quick_presets();
                                                app.queue[idx].status = ItemStatus::FormatSelect;
                                            } else {
                                                let plat = detect_platform_folder(&raw_url, "");
                                                app.history.record_search(&raw_url, plat);

                                                if app.queue[idx].chosen_format_id.is_some() || app.queue[idx].is_image {
                                                    app.queue[idx].status = ItemStatus::Queued;
                                                    continue;
                                                }

                                                app.queue[idx].status = ItemStatus::FetchingMeta;
                                                app.queue[idx].title = "Inspecting...".into();
                                                let item_id = app.queue[idx].id;
                                                let tx = app.worker_tx.clone();
                                                let auth_mode = app.auth_mode.clone();
                                                let initial_auth = app.queue[idx].resolved_auth.clone();

                                                tokio::spawn(async move {
                                                    let url = resolve_short_url(&raw_url).await;
                                                    if url != raw_url {
                                                        let _ = tx.send(WorkerMessage::UrlResolved { id: item_id, resolved_url: url.clone() }).await;
                                                    }

                                                    let platform_name = detect_platform_folder(&url, "").to_string();

                                                    if is_direct_image_or_gallery_url(&url) || is_reddit_url(&url) {
                                                        let _ = tx.send(WorkerMessage::ImageDetected {
                                                            id: item_id,
                                                            platform: platform_name,
                                                            resolved_auth: initial_auth,
                                                        }).await;
                                                        return;
                                                    }

                                                    match fetch_metadata(&url, auth_mode).await {
                                                        Ok((info, resolved_auth)) => {
                                                            let _ = tx.send(WorkerMessage::MetaFetched { id: item_id, info, resolved_auth }).await;
                                                        }
                                                        Err(err) => {
                                                            let err_str = err.to_string();
                                                            if is_no_video_error(&err_str) {
                                                                let detected_auth = if err_str.contains("Auth:Chrome") {
                                                                    Some(AuthMode::Chrome)
                                                                } else if err_str.contains("Auth:Brave") {
                                                                    Some(AuthMode::Brave)
                                                                } else if err_str.contains("Auth:Firefox") {
                                                                    Some(AuthMode::Firefox)
                                                                } else if err_str.contains("Auth:Edge") {
                                                                    Some(AuthMode::Edge)
                                                                } else if err_str.contains("Auth:CookiesFile") {
                                                                    Some(AuthMode::CookiesFile)
                                                                } else {
                                                                    initial_auth
                                                                };
                                                                let _ = tx.send(WorkerMessage::ImageDetected {
                                                                    id: item_id,
                                                                    platform: platform_name,
                                                                    resolved_auth: detected_auth,
                                                                }).await;
                                                            } else {
                                                                let _ = tx.send(WorkerMessage::MetaFailed { id: item_id, error: err_str }).await;
                                                            }
                                                        }
                                                    }
                                                });
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            KeyCode::Backspace => { app.input_url.pop(); }
                            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                app.input_url.clear();
                            }
                            KeyCode::Up => {
                                let cur = app.queue_state.selected().unwrap_or(0);
                                if cur > 0 { app.queue_state.select(Some(cur - 1)); }
                            }
                            KeyCode::Down => {
                                let cur = app.queue_state.selected().unwrap_or(0);
                                if cur + 1 < app.queue.len() { app.queue_state.select(Some(cur + 1)); }
                            }
                            KeyCode::Char(ch) => {
                                app.input_warning = None;
                                app.input_url.push(ch);
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }

        if last_tick.elapsed() >= tick_rate { last_tick = Instant::now(); }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableBracketedPaste)?;
    terminal.show_cursor()?;

    if app.trigger_self_update_exit {
        run_interactive_self_update();
    }

    Ok(())
}

// --- UI RENDERING ---

fn ui(f: &mut Frame, app: &mut App) {
    let main_chunks = Layout::default()
        .direction(Direction::Vertical).margin(1)
        .constraints([ Constraint::Length(3), Constraint::Min(8), Constraint::Length(10), Constraint::Length(1) ])
        .split(f.area());

    let update_banner = if let Some(ref tag) = app.available_update {
        format!(" | ⬆ Update {} Available! Press [F3] ", tag)
    } else {
        String::new()
    };

    let (header_title, border_color, display_text, text_color) = if let Some((ref warn, _)) = app.input_warning {
        (
            format!(" Smart Input Shield v{} [{}]{} ", APP_VERSION, app.auth_mode.badge(), update_banner),
            Color::LightRed,
            warn.as_str(),
            Color::LightRed,
        )
    } else {
        (
            format!(
                " Add URL(s) to Queue v{} (Paste link/playlist + Enter | Ctrl+U Clear) [{}]{} ",
                APP_VERSION,
                app.auth_mode.badge(),
                update_banner
            ),
            if app.available_update.is_some() { Color::LightGreen } else { Color::White },
            app.input_url.as_str(),
            Color::Yellow,
        )
    };

    let input = Paragraph::new(display_text)
        .style(Style::default().fg(text_color))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .title(header_title),
        );
    f.render_widget(input, main_chunks[0]);

    let body_chunks = Layout::default().direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)]).split(main_chunks[1]);

    let queue_items: Vec<ListItem> = app.queue.iter().map(|item| {
        let (status_icon, color) = match &item.status {
            ItemStatus::New => ("🆕 Added", Color::DarkGray),
            ItemStatus::Queued => ("⏳ Queued", Color::DarkGray),
            ItemStatus::FetchingMeta => ("🔍 Inspecting...", Color::Cyan),
            ItemStatus::FormatSelect => ("⚙ Select Format", Color::Yellow),
            ItemStatus::Downloading => ("⬇ Downloading", Color::Green),
            ItemStatus::Paused => ("⏸ Paused", Color::LightYellow),
            ItemStatus::Completed => ("✔ Done", Color::Blue),
            ItemStatus::Skipped => ("⏭ Skipped", Color::Magenta),
            ItemStatus::Failed(_) => ("✖ Failed", Color::Red),
            ItemStatus::Ready => ("🟢 Ready", Color::Cyan),
        };
        
        let auth_tag = if item.resolved_auth.is_some() { "🔒 " } else { "" };
        let type_tag = if item.is_image { "🖼️ " } else if item.is_audio { "🎵 " } else { "" };
        let title_prefix = if item.retries > 0 && item.status == ItemStatus::Downloading {
            format!("[Retry {}] {}{}", item.retries, auth_tag, type_tag)
        } else {
            format!("{}{}", auth_tag, type_tag)
        };
        let full_title = format!("{}{}", title_prefix, item.title);
        let title_trunc = truncate_chars(&full_title, 30);
        
        ListItem::new(Line::from(vec![
            Span::styled(format!("{:<16}", status_icon), Style::default().fg(color)),
            Span::raw(" "), Span::styled(title_trunc, Style::default().add_modifier(Modifier::BOLD)),
        ]))
    }).collect();

    let queue_list = List::new(queue_items).block(Block::default().borders(Borders::ALL).title(" Media Queue (↑/↓ to inspect) "))
        .highlight_style(Style::default().bg(Color::Rgb(30, 30, 45)).add_modifier(Modifier::BOLD));
    f.render_stateful_widget(queue_list, body_chunks[0], &mut app.queue_state);

    let selected_idx = app.queue_state.selected().unwrap_or(0);
    if let Some(selected_item) = app.queue.get(selected_idx) {
        let auth_status_str = match &selected_item.resolved_auth {
            Some(mode) => format!("Authenticated via {}", mode.badge()),
            None => "Standard Guest / Headless DOM Session".to_string(),
        };
        let media_type_str = if selected_item.is_image {
            "Image / Gallery (Headless DOM + Verified Binary)"
        } else if selected_item.is_audio {
            "Audio MP3 (Single File + Cover Art)"
        } else {
            "Video MP4 (Single File + Embedded CC)"
        };

        let mut detail_lines = vec![
            Line::from(vec![Span::styled("Platform: ", Style::default().fg(Color::Cyan)), Span::raw(&selected_item.platform)]),
            Line::from(vec![Span::styled("Type:     ", Style::default().fg(Color::Cyan)), Span::raw(media_type_str)]),
            Line::from(vec![Span::styled("Title:    ", Style::default().fg(Color::Cyan)), Span::styled(&selected_item.title, Style::default().add_modifier(Modifier::BOLD))]),
            Line::from(vec![Span::styled("Session:  ", Style::default().fg(Color::Cyan)), Span::styled(auth_status_str, Style::default().fg(if selected_item.resolved_auth.is_some() { Color::LightGreen } else { Color::Gray }))]),
            Line::from(vec![Span::styled("Source:   ", Style::default().fg(Color::Cyan)), Span::styled(&selected_item.url, Style::default().fg(Color::DarkGray))]),
            Line::from(vec![Span::styled("Save Path:", Style::default().fg(Color::Cyan)), Span::styled(get_base_download_dir(selected_item).to_string_lossy().to_string(), Style::default().fg(Color::DarkGray))]),
        ];

        if let Some(entries) = &selected_item.playlist_entries {
            detail_lines.push(Line::from(vec![Span::styled("Tracks:   ", Style::default().fg(Color::LightBlue)), Span::raw(format!("{} items grouped", entries.len()))]));
        }

        if let ItemStatus::Failed(ref err) = selected_item.status {
            detail_lines.push(Line::from(""));
            let clean_err = err.replace('\n', " ");
            detail_lines.push(Line::from(vec![Span::styled("Error:    ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)), Span::styled(clean_err, Style::default().fg(Color::Red))]));
        }

        detail_lines.push(Line::from(""));
        detail_lines.push(Line::from(Span::styled("Description Preview:", Style::default().fg(Color::LightMagenta))));

        let desc_preview = if selected_item.description.is_empty() {
            "No description available.".to_string()
        } else {
            truncate_chars(&selected_item.description, 400)
        };
        for line in desc_preview.lines() {
            detail_lines.push(Line::from(Span::styled(line.to_string(), Style::default().fg(Color::Gray))));
        }

        let details = Paragraph::new(detail_lines)
            .block(Block::default().borders(Borders::ALL).title(" Media Details "))
            .wrap(Wrap { trim: true });
        f.render_widget(details, body_chunks[1]);
    } else {
        let empty_msg = Paragraph::new("Queue is empty. Paste any URL above, copy a link, or press [F4] for History.").block(Block::default().borders(Borders::ALL).title(" Media Details "));
        f.render_widget(empty_msg, body_chunks[1]);
    }

    let mut active_items: Vec<&QueueItem> = app.queue.iter()
        .filter(|i| i.status == ItemStatus::Downloading)
        .collect();
    
    let paused_items: Vec<&QueueItem> = app.queue.iter()
        .filter(|i| i.status == ItemStatus::Paused)
        .collect();
        
    active_items.extend(paused_items);

    let container_title = format!(" Active Downloads ({}/{}) ", app.active_downloads.len(), app.max_concurrent_downloads);
    let monitor_block = Block::default().borders(Borders::ALL).title(container_title);
    let inner_area = monitor_block.inner(main_chunks[2]);
    f.render_widget(monitor_block, main_chunks[2]);

    if active_items.is_empty() {
        let status_msg = "No active download. Waiting for queue.";
        f.render_widget(Paragraph::new(status_msg), inner_area);
    } else {
        let mut constraints = Vec::new();
        for _ in 0..active_items.len().min(4) {
            constraints.push(Constraint::Length(1));
            constraints.push(Constraint::Length(1));
        }
        constraints.push(Constraint::Min(0));
        
        let row_chunks = Layout::default().direction(Direction::Vertical).constraints(constraints).split(inner_area);
        
        for (i, item) in active_items.iter().enumerate().take(4) {
            let stats = &item.stats;
            let elapsed_sec = stats.elapsed.as_secs();
            let time_formatted = format!("{:02}:{:02}", elapsed_sec / 60, elapsed_sec % 60);

            let speed_str = format!("{}/s", format_bytes(stats.smoothed_speed));
            let eta_secs = if stats.smoothed_speed > 0.0 && stats.total_bytes > stats.downloaded_bytes {
                (stats.total_bytes - stats.downloaded_bytes) / stats.smoothed_speed
            } else { 0.0 };
            let eta_str = format_time(eta_secs);
            let dl_str = format!("{} / {}", format_bytes(stats.downloaded_bytes), format_bytes(stats.total_bytes));

            let safe_title = truncate_chars(&item.title, 20);

            let stats_info = Line::from(vec![
                Span::styled(format!("{:<20} | ", safe_title), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::styled("SPD: ", Style::default().fg(Color::Cyan)), Span::styled(format!("{:<12}", speed_str), Style::default().add_modifier(Modifier::BOLD)),
                Span::styled("ETA: ", Style::default().fg(Color::Cyan)), Span::styled(format!("{:<8}", eta_str), Style::default().add_modifier(Modifier::BOLD)),
                Span::styled("DL: ", Style::default().fg(Color::Cyan)), Span::styled(format!("{:<18}", dl_str), Style::default().add_modifier(Modifier::BOLD)),
                Span::styled("TIME: ", Style::default().fg(Color::Cyan)), Span::styled(time_formatted, Style::default().add_modifier(Modifier::BOLD)),
            ]);

            f.render_widget(Paragraph::new(stats_info), row_chunks[i * 2]);

            let gauge_color = if item.status == ItemStatus::Paused { Color::Yellow } else { Color::Green };
            let gauge = Gauge::default()
                .use_unicode(true)
                .gauge_style(Style::default().fg(gauge_color).bg(Color::Rgb(40, 40, 40)))
                .percent(stats.percent.clamp(0.0, 100.0) as u16);
            f.render_widget(gauge, row_chunks[i * 2 + 1]);
        }
    }

    let is_modal_open = app.queue.iter().any(|i| i.status == ItemStatus::FormatSelect)
        || app.show_auth_modal
        || app.show_history_modal;

    let help_text = if app.show_history_modal {
        Line::from(vec![
            Span::styled("[Tab/←/→]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)), Span::raw(" Switch Tab | "),
            Span::styled("[Enter]", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)), Span::raw(" Re-Queue URL | "),
            Span::styled("[F9]", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)), Span::raw(" Open Folder | "),
            Span::styled("[Del]", Style::default().add_modifier(Modifier::BOLD)), Span::raw(" Delete Entry | "),
            Span::styled("[F7]", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)), Span::raw(" Clear Tab | "),
            Span::styled("[Esc/F4]", Style::default().add_modifier(Modifier::BOLD)), Span::raw(" Close"),
        ])
    } else if is_modal_open {
        Line::from(vec![
            Span::styled("[Enter]", Style::default().add_modifier(Modifier::BOLD)), Span::raw(" Confirm Selection | "),
            Span::styled("[Esc/Bksp]", Style::default().add_modifier(Modifier::BOLD)), Span::raw(" Close Modal"),
        ])
    } else {
        let mut spans = vec![
            Span::styled("[Enter]", Style::default().add_modifier(Modifier::BOLD)), Span::raw(" Inspect | "),
        ];
        if let Some(ref tag) = app.available_update {
            spans.push(Span::styled("[F3]", Style::default().fg(Color::LightGreen).add_modifier(Modifier::BOLD)));
            spans.push(Span::raw(format!(" Update ({}) | ", tag)));
        }
        spans.extend(vec![
            Span::styled("[F4]", Style::default().fg(Color::LightMagenta).add_modifier(Modifier::BOLD)), Span::raw(" History | "),
            Span::styled("[F5]", Style::default().add_modifier(Modifier::BOLD)), Span::raw(" Pause | "),
            Span::styled("[F6]", Style::default().add_modifier(Modifier::BOLD)), Span::raw(" Skip | "),
            Span::styled("[F7]", Style::default().add_modifier(Modifier::BOLD)), Span::raw(" Nuke | "),
            Span::styled("[F8]", Style::default().fg(Color::LightGreen).add_modifier(Modifier::BOLD)), Span::raw(" Auth | "),
            Span::styled("[F9]", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)), Span::raw(" Folder | "),
            Span::styled("[Del]", Style::default().add_modifier(Modifier::BOLD)), Span::raw(" Del | "),
            Span::styled("[Esc]", Style::default().add_modifier(Modifier::BOLD)), Span::raw(" Exit"),
        ]);
        Line::from(spans)
    };
    f.render_widget(Paragraph::new(help_text), main_chunks[3]);

    // MODAL 1: Quality Format Selector
    if let Some(idx) = app.queue.iter().position(|i| i.status == ItemStatus::FormatSelect) {
        let area = centered_rect(65, 55, f.area());
        f.render_widget(Clear, area);

        let formats = &app.queue[idx].available_formats;
        let items: Vec<ListItem> = formats.iter().map(|opt| {
            ListItem::new(Line::from(vec![
                Span::styled(format!("{:<50}", opt.label), Style::default().fg(Color::Yellow)),
                Span::styled(format!("{:<6}", opt.ext), Style::default().fg(Color::Cyan)),
                Span::styled(&opt.est_size, Style::default().fg(Color::Green)),
            ]))
        }).collect();

        let format_list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title(format!(" Choose Quality Preset for: {} ", &app.queue[idx].title)))
            .highlight_style(Style::default().bg(Color::Blue).add_modifier(Modifier::BOLD));
        f.render_stateful_widget(format_list, area, &mut app.format_state);
    }

    // MODAL 2: [F8] Authentication Engine Selector
    if app.show_auth_modal {
        let area = centered_rect(65, 42, f.area());
        f.render_widget(Clear, area);

        let modes = AuthMode::all();
        let items: Vec<ListItem> = modes.iter().map(|m| {
            let active_marker = if *m == app.auth_mode { "✔ " } else { "  " };
            ListItem::new(Line::from(vec![
                Span::styled(active_marker, Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::styled(m.label(), Style::default().fg(Color::Yellow)),
            ]))
        }).collect();

        let auth_list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title(" [F8] Select Authentication Engine "))
            .highlight_style(Style::default().bg(Color::Blue).add_modifier(Modifier::BOLD));
        f.render_stateful_widget(auth_list, area, &mut app.auth_state);
    }

    // MODAL 3: [F4] Persistent Dual History Manager
    if app.show_history_modal {
        let area = centered_rect(82, 70, f.area());
        f.render_widget(Clear, area);

        let dl_count = app.history.download_history.len();
        let sr_count = app.history.search_history.len();
        let modal_title = match app.history_tab {
            HistoryTab::Downloads => format!(
                " [F4] History Manager | [▶ 📥 Completed Downloads ({}) ]   🔍 Search / Link History ({}) ",
                dl_count, sr_count
            ),
            HistoryTab::Searches => format!(
                " [F4] History Manager |   📥 Completed Downloads ({})   [▶ 🔍 Search / Link History ({}) ] ",
                dl_count, sr_count
            ),
        };

        let items: Vec<ListItem> = match app.history_tab {
            HistoryTab::Downloads => {
                if app.history.download_history.is_empty() {
                    vec![ListItem::new(Line::from(Span::styled(
                        "No completed downloads yet. Finished downloads will automatically appear here.",
                        Style::default().fg(Color::DarkGray),
                    )))]
                } else {
                    app.history.download_history.iter().map(|e| {
                        let title_trunc = truncate_chars(&e.title, 34);
                        ListItem::new(Line::from(vec![
                            Span::styled(format!("{:<16} ", e.timestamp), Style::default().fg(Color::DarkGray)),
                            Span::styled(format!("{:<10} ", e.platform), Style::default().fg(Color::Cyan)),
                            Span::styled(format!("{:<6} ", e.media_type), Style::default().fg(Color::LightGreen)),
                            Span::styled(format!("{:<35} ", title_trunc), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                            Span::styled(truncate_chars(&e.url, 32), Style::default().fg(Color::Gray)),
                        ]))
                    }).collect()
                }
            }
            HistoryTab::Searches => {
                if app.history.search_history.is_empty() {
                    vec![ListItem::new(Line::from(Span::styled(
                        "No search/link history yet. Every URL you paste or inspect is saved here.",
                        Style::default().fg(Color::DarkGray),
                    )))]
                } else {
                    app.history.search_history.iter().map(|e| {
                        ListItem::new(Line::from(vec![
                            Span::styled(format!("{:<16} ", e.timestamp), Style::default().fg(Color::DarkGray)),
                            Span::styled(format!("{:<12} ", e.platform), Style::default().fg(Color::Cyan)),
                            Span::styled(&e.url, Style::default().fg(Color::Yellow)),
                        ]))
                    }).collect()
                }
            }
        };

        let hist_list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::LightMagenta))
                    .title(modal_title),
            )
            .highlight_style(Style::default().bg(Color::Rgb(50, 35, 70)).add_modifier(Modifier::BOLD));
        f.render_stateful_widget(hist_list, area, &mut app.history_state);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default().direction(Direction::Vertical)
        .constraints([Constraint::Percentage((100 - percent_y) / 2), Constraint::Percentage(percent_y), Constraint::Percentage((100 - percent_y) / 2)]).split(r);
    Layout::default().direction(Direction::Horizontal)
        .constraints([Constraint::Percentage((100 - percent_x) / 2), Constraint::Percentage(percent_x), Constraint::Percentage((100 - percent_x) / 2)]).split(popup_layout[1])[1]
}