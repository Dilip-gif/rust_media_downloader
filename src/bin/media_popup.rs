#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::Result;
use arboard::Clipboard;
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::{mpsc, oneshot, Mutex},
};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const GITHUB_REPO: &str = "Dilip-gif/rust_media_downloader";
const BROWSER_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

// --- DYNAMIC QUALITY OPTION STRUCT ---
#[derive(Clone, Debug, PartialEq)]
pub struct DynamicQualityChoice {
    pub format_id: String,
    pub label: String,
    pub badge: String,
    pub is_audio: bool,
    pub is_image: bool,
    pub height: u32,
}

// --- OS-ADAPTIVE NATIVE COLOR THEME ENGINE ---
#[derive(Clone, Copy)]
struct OsTheme {
    bg_surface: egui::Color32,
    card_elevated: egui::Color32,
    card_hover: egui::Color32,
    border_subtle: egui::Color32,
    accent_primary: egui::Color32,
    accent_text_on_primary: egui::Color32,
    accent_green: egui::Color32,
    accent_amber: egui::Color32,
    accent_red: egui::Color32,
    text_primary: egui::Color32,
    text_muted: egui::Color32,
    os_badge: &'static str,
}

impl OsTheme {
    fn detect() -> Self {
        match std::env::consts::OS {
            "windows" => Self {
                bg_surface: egui::Color32::from_rgb(24, 25, 30),
                card_elevated: egui::Color32::from_rgb(38, 40, 48),
                card_hover: egui::Color32::from_rgb(52, 55, 66),
                border_subtle: egui::Color32::from_rgb(62, 66, 78),
                accent_primary: egui::Color32::from_rgb(76, 194, 255),
                accent_text_on_primary: egui::Color32::from_rgb(12, 16, 22),
                accent_green: egui::Color32::from_rgb(108, 203, 95),
                accent_amber: egui::Color32::from_rgb(252, 225, 0),
                accent_red: egui::Color32::from_rgb(255, 153, 164),
                text_primary: egui::Color32::from_rgb(245, 246, 248),
                text_muted: egui::Color32::from_rgb(156, 160, 170),
                os_badge: "Windows Quick",
            },
            "macos" => Self {
                bg_surface: egui::Color32::from_rgb(28, 28, 32),
                card_elevated: egui::Color32::from_rgb(44, 44, 49),
                card_hover: egui::Color32::from_rgb(58, 58, 66),
                border_subtle: egui::Color32::from_rgb(68, 68, 76),
                accent_primary: egui::Color32::from_rgb(48, 160, 255),
                accent_text_on_primary: egui::Color32::from_rgb(12, 14, 18),
                accent_green: egui::Color32::from_rgb(48, 209, 88),
                accent_amber: egui::Color32::from_rgb(255, 214, 10),
                accent_red: egui::Color32::from_rgb(255, 105, 97),
                text_primary: egui::Color32::from_rgb(245, 245, 247),
                text_muted: egui::Color32::from_rgb(152, 152, 157),
                os_badge: "macOS Quick",
            },
            _ => Self {
                bg_surface: egui::Color32::from_rgb(34, 34, 38),
                card_elevated: egui::Color32::from_rgb(48, 48, 55),
                card_hover: egui::Color32::from_rgb(62, 62, 72),
                border_subtle: egui::Color32::from_rgb(68, 68, 78),
                accent_primary: egui::Color32::from_rgb(154, 134, 253),
                accent_text_on_primary: egui::Color32::from_rgb(20, 20, 24),
                accent_green: egui::Color32::from_rgb(87, 227, 137),
                accent_amber: egui::Color32::from_rgb(248, 228, 92),
                accent_red: egui::Color32::from_rgb(255, 123, 99),
                text_primary: egui::Color32::from_rgb(246, 245, 244),
                text_muted: egui::Color32::from_rgb(154, 153, 150),
                os_badge: "Quick Popup",
            },
        }
    }
}

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

// --- GITHUB AUTO-UPDATE CHECKER & SELF-INSTALLER ---

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
            .arg("rust-media-downloader-updater")
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

fn trigger_self_update_and_restart() {
    match std::env::consts::OS {
        "windows" => {
            let cmd = format!(
                "irm https://raw.githubusercontent.com/{}/main/install.ps1 | iex; Start-Process media-popup",
                GITHUB_REPO
            );
            let _ = std::process::Command::new("powershell")
                .args(["-NoProfile", "-Command", &cmd])
                .spawn();
        }
        _ => {
            let cmd = format!(
                "curl -fsSL https://raw.githubusercontent.com/{}/main/install.sh | bash && ~/.local/bin/media-popup &",
                GITHUB_REPO
            );
            let _ = std::process::Command::new("bash")
                .args(["-c", &cmd])
                .spawn();
        }
    }
}

// --- MULTI-FALLBACK CLIPBOARD READER ---

fn read_system_clipboard_robust() -> String {
    if let Ok(mut clip) = Clipboard::new() {
        if let Ok(t) = clip.get_text() {
            if !t.trim().is_empty() {
                return t;
            }
        }
    }
    if std::env::consts::OS == "linux" {
        for (bin, args) in [
            ("wl-paste", vec!["--no-newline"]),
            ("xclip", vec!["-o", "-selection", "clipboard"]),
            ("xsel", vec!["-ob"]),
        ] {
            if let Ok(out) = std::process::Command::new(bin).args(&args).output() {
                if out.status.success() {
                    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    if !s.is_empty() {
                        return s;
                    }
                }
            }
        }
    }
    String::new()
}

// --- SHARED PERSISTENT HISTORY ---

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

    pub fn record_search(url: &str, platform: &str) {
        if url.is_empty() || url == "batch" {
            return;
        }
        let mut store = Self::load();
        store.search_history.retain(|e| e.url != url);
        store.search_history.insert(
            0,
            SearchHistoryEntry {
                url: url.to_string(),
                platform: platform.to_string(),
                timestamp: current_timestamp_str(),
            },
        );
        if store.search_history.len() > 300 {
            store.search_history.truncate(300);
        }
        store.save();
    }

    pub fn record_download(title: &str, url: &str, platform: &str, media_type: &str, save_path: &str) {
        if url.is_empty() {
            return;
        }
        let mut store = Self::load();
        store.download_history.retain(|e| !(e.url == url && e.media_type == media_type));
        store.download_history.insert(
            0,
            DownloadHistoryEntry {
                title: title.to_string(),
                url: url.to_string(),
                platform: platform.to_string(),
                media_type: media_type.to_string(),
                save_path: save_path.to_string(),
                timestamp: current_timestamp_str(),
            },
        );
        if store.download_history.len() > 300 {
            store.download_history.truncate(300);
        }
        store.save();
    }
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

// --- PATH & URL HELPERS ---

fn get_ytdlp_cmd() -> String {
    let base = if std::env::consts::OS == "windows" { "yt-dlp.exe" } else { "yt-dlp" };
    if Path::new(base).exists() { format!("./{}", base) } else { base.to_string() }
}

fn get_gallerydl_cmd() -> String {
    let base = if std::env::consts::OS == "windows" { "gallery-dl.exe" } else { "gallery-dl" };
    if Path::new(base).exists() { format!("./{}", base) } else { base.to_string() }
}

fn get_local_ffmpeg_dir() -> Option<String> {
    let bin = if std::env::consts::OS == "windows" { "ffmpeg.exe" } else { "ffmpeg" };
    if Path::new(bin).exists() { Some(".".to_string()) } else { None }
}

fn detect_platform_folder(url: &str) -> &'static str {
    let u = url.to_lowercase();
    if u.contains("youtube.com") || u.contains("youtu.be") {
        "YouTube"
    } else if u.contains("instagram.com") {
        "Instagram"
    } else if u.contains("reddit.com") || u.contains("redd.it") {
        "Reddit"
    } else if u.contains("x.com") || u.contains("twitter.com") {
        "X_Twitter"
    } else if u.contains("pinterest.com") || u.contains("pin.it") {
        "Pinterest"
    } else if u.contains("tiktok.com") {
        "TikTok"
    } else if u.contains("imgur.com") {
        "Imgur"
    } else {
        "Other"
    }
}

fn build_target_dir(url: &str, is_audio: bool, is_image: bool, playlist_folder: Option<&str>) -> PathBuf {
    let mut path = dirs::download_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("MediaDownloader");
    if is_audio {
        path.push("Audio");
    } else if is_image {
        path.push("Images");
    } else {
        path.push("Videos");
    }
    path.push(detect_platform_folder(url));
    if let Some(folder) = playlist_folder {
        let safe_folder = folder.replace(|c: char| !c.is_alphanumeric() && c != ' ' && c != '-', "_");
        if !safe_folder.trim().is_empty() {
            path.push(safe_folder);
        }
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
        "windows" => { let _ = std::process::Command::new("explorer").arg(&path_str).spawn(); }
        "macos" => { let _ = std::process::Command::new("open").arg(&path_str).spawn(); }
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
            if let Some(si_idx) = url.find("&si=") { url = url[..si_idx].to_string(); }
            if let Some(si_idx) = url.find("?si=") { url = url[..si_idx].to_string(); }
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

// --- CLEANUP & UNICODE NORMALIZATION HELPERS ---

fn strip_ytdlp_format_suffix(stem: &str) -> &str {
    let mut s = stem.strip_suffix(".temp").unwrap_or(stem);
    if let Some(dot_idx) = s.rfind('.') {
        let after = &s[dot_idx + 1..];
        if after.len() >= 2 && after.starts_with('f') && after[1..].chars().all(|c| c.is_ascii_digit()) {
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

fn cleanup_track_garbage(target_files: &[String], title: &str, target_dir: &Path) {
    let mut base_prefixes: Vec<String> = Vec::new();
    for file in target_files {
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

    if let Ok(entries) = std::fs::read_dir(target_dir) {
        let safe_title = title.replace(|c: char| !c.is_alphanumeric(), "").to_lowercase();
        let has_valid_title = !safe_title.is_empty()
            && safe_title != "downloading"
            && safe_title != "fetchingmedia"
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
    let s = s.trim().trim_start_matches('~');
    let mut num_str = String::new();
    let mut unit_str = String::new();
    for c in s.chars() {
        if c.is_numeric() || c == '.' { num_str.push(c); } else { unit_str.push(c); }
    }
    let val = num_str.parse::<f64>().unwrap_or(0.0);
    match unit_str.to_uppercase().as_str() {
        "GIB" | "GB" | "G" => val * 1024.0 * 1024.0 * 1024.0,
        "MIB" | "MB" | "M" => val * 1024.0 * 1024.0,
        "KIB" | "KB" | "K" => val * 1024.0,
        _ => val,
    }
}

fn parse_ytdlp_progress_line(line: &str) -> Option<(f32, f64, f64, Option<f64>, Option<String>)> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    let pct_idx = parts.iter().position(|p| p.ends_with('%'))?;
    let percent = parts[pct_idx].trim_end_matches('%').parse::<f32>().ok()?;

    let mut total_bytes = 0.0;
    if let Some(of_idx) = parts.iter().position(|&p| p == "of") {
        if of_idx + 1 < parts.len() {
            let mut size_tok = parts[of_idx + 1];
            if size_tok == "~" && of_idx + 2 < parts.len() {
                size_tok = parts[of_idx + 2];
            }
            total_bytes = parse_size(size_tok);
        }
    }

    let dl_bytes = if total_bytes > 0.0 {
        (percent as f64 / 100.0) * total_bytes
    } else {
        0.0
    };

    let mut direct_speed: Option<f64> = None;
    if let Some(at_idx) = parts.iter().position(|&p| p == "at") {
        if at_idx + 1 < parts.len() {
            let spd_tok = parts[at_idx + 1];
            if !spd_tok.contains("Unknown") && spd_tok.ends_with("/s") {
                direct_speed = Some(parse_size(spd_tok.trim_end_matches("/s")));
            }
        }
    }

    let mut direct_eta: Option<String> = None;
    if let Some(eta_idx) = parts.iter().position(|&p| p == "ETA") {
        if eta_idx + 1 < parts.len() {
            let eta_tok = parts[eta_idx + 1];
            if !eta_tok.contains("Unknown") && eta_tok.contains(':') {
                direct_eta = Some(eta_tok.to_string());
            }
        }
    }

    Some((percent, dl_bytes, total_bytes, direct_speed, direct_eta))
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

// --- DYNAMIC QUALITY & PLAYLIST INSPECTOR ---

#[derive(Deserialize)]
struct YtFormatMeta {
    height: Option<u32>,
    filesize: Option<u64>,
    filesize_approx: Option<u64>,
    vcodec: Option<String>,
}

#[derive(Deserialize)]
struct FlatPlaylistEntry {
    id: Option<String>,
    url: Option<String>,
    title: Option<String>,
}

#[derive(Deserialize)]
struct YtFullMeta {
    title: Option<String>,
    formats: Option<Vec<YtFormatMeta>>,
    entries: Option<Vec<FlatPlaylistEntry>>,
    _type: Option<String>,
}

#[derive(Clone)]
struct QueuedTrack {
    url: String,
    title: String,
    playlist_folder: Option<String>,
}

fn resolution_tag_name(h: u32) -> &'static str {
    match h {
        4320..=9999 => "8K Ultra HD",
        2160..=4319 => "4K UHD",
        1440..=2159 => "2K QHD",
        1080..=1439 => "Full HD (Recommended)",
        720..=1079 => "HD Ready",
        480..=719 => "Standard Definition",
        360..=479 => "Medium Saver",
        240..=359 => "Low Data",
        _ => "Minimal Data",
    }
}

fn build_standard_preset_heights() -> Vec<DynamicQualityChoice> {
    let mut list = Vec::new();
    for &h in &[4320u32, 2160, 1440, 1080, 720, 480, 360, 240, 144] {
        list.push(DynamicQualityChoice {
            format_id: format!("bestvideo[height<={h}]+bestaudio/best[height<={h}]/best"),
            label: format!("{}p  —  {}", h, resolution_tag_name(h)),
            badge: "MP4".into(),
            is_audio: false,
            is_image: false,
            height: h,
        });
    }
    list.push(DynamicQualityChoice {
        format_id: "bestaudio/best".into(),
        label: "Audio MP3  —  320kbps + Cover Art".into(),
        badge: "MP3".into(),
        is_audio: true,
        is_image: false,
        height: 0,
    });
    list.push(DynamicQualityChoice {
        format_id: "gallery".into(),
        label: "Images / Gallery  —  Original Quality".into(),
        badge: "IMG".into(),
        is_audio: false,
        is_image: true,
        height: 0,
    });
    list
}

// Inspects the URL(s), detects exact available resolutions & file sizes, and picks 1080p as default!
async fn inspect_urls_and_detect_qualities(
    urls: Vec<String>,
) -> (Vec<QueuedTrack>, Vec<DynamicQualityChoice>, usize, String) {
    let mut tracks = Vec::new();
    let is_single = urls.len() == 1;

    for raw_url in urls {
        let url = resolve_short_url(&raw_url).await;
        let plat = detect_platform_folder(&url);
        HistoryStore::record_search(&url, plat);

        // 1. Direct Image / Reddit Post -> Offer Image Gallery + Video/Audio fallback
        if is_single && (is_direct_image_or_gallery_url(&url) || is_reddit_url(&url)) {
            tracks.push(QueuedTrack {
                url: url.clone(),
                title: format!("{} Post", plat),
                playlist_folder: None,
            });
            let choices = vec![
                DynamicQualityChoice {
                    format_id: "gallery".into(),
                    label: "Original Quality Media (Images / Gallery / Video)".into(),
                    badge: "ORIG".into(),
                    is_audio: false,
                    is_image: true,
                    height: 1080,
                },
                DynamicQualityChoice {
                    format_id: "bestaudio/best".into(),
                    label: "Audio Only MP3 (If Post is Video)".into(),
                    badge: "MP3".into(),
                    is_audio: true,
                    is_image: false,
                    height: 0,
                },
            ];
            return (tracks, choices, 0, format!("{} Media Post", plat));
        }

        // 2. Single Video URL -> Fetch exact available resolutions & file sizes via yt-dlp -J
        if is_single && !url.contains("list=") && !url.contains("/playlist") {
            let mut auth_chain: Vec<Option<(&str, &str)>> = vec![None];
            if Path::new("cookies.txt").exists() {
                auth_chain.push(Some(("--cookies", "cookies.txt")));
            }
            auth_chain.push(Some(("--cookies-from-browser", "chrome")));
            auth_chain.push(Some(("--cookies-from-browser", "brave")));
            auth_chain.push(Some(("--cookies-from-browser", "firefox")));

            for auth in auth_chain {
                let mut cmd = Command::new(get_ytdlp_cmd());
                cmd.arg("-J")
                   .arg("--no-playlist")
                   .arg("--user-agent").arg(BROWSER_UA);
                if let Some((f, v)) = auth {
                    cmd.arg(f).arg(v);
                }
                cmd.arg(&url).stdout(Stdio::piped()).stderr(Stdio::piped());

                if let Ok(Ok(out)) = tokio::time::timeout(Duration::from_secs(25), cmd.output()).await {
                    if out.status.success() {
                        if let Ok(info) = serde_json::from_slice::<YtFullMeta>(&out.stdout) {
                            let clean_t = normalize_ytdlp_unicode(&info.title.unwrap_or_else(|| "Video".into()));
                            tracks.push(QueuedTrack {
                                url: url.clone(),
                                title: clean_t.clone(),
                                playlist_folder: None,
                            });

                            let mut choices = Vec::new();
                            let mut best_audio_bytes = 0.0;

                            if let Some(formats) = &info.formats {
                                for f in formats {
                                    let vc = f.vcodec.as_deref().unwrap_or("none");
                                    if vc == "none" || vc == "null" {
                                        let sz = f.filesize.or(f.filesize_approx).unwrap_or(0) as f64;
                                        if sz > best_audio_bytes {
                                            best_audio_bytes = sz;
                                        }
                                    }
                                }

                                let mut heights: Vec<u32> = formats
                                    .iter()
                                    .filter(|f| {
                                        let vc = f.vcodec.as_deref().unwrap_or("none");
                                        vc != "none" && vc != "null"
                                    })
                                    .filter_map(|f| f.height)
                                    .collect();
                                heights.sort_by(|a, b| b.cmp(a));
                                heights.dedup();

                                for h in heights {
                                    let mut best_vid_bytes = 0.0;
                                    for f in formats {
                                        if f.height == Some(h) {
                                            let sz = f.filesize.or(f.filesize_approx).unwrap_or(0) as f64;
                                            if sz > best_vid_bytes {
                                                best_vid_bytes = sz;
                                            }
                                        }
                                    }
                                    let est_mb = (best_vid_bytes + best_audio_bytes) / 1_048_576.0;
                                    let size_badge = if est_mb > 0.1 {
                                        format!("~{:.1} MB", est_mb)
                                    } else {
                                        "MP4".into()
                                    };
                                    choices.push(DynamicQualityChoice {
                                        format_id: format!("bestvideo[height<={h}]+bestaudio/best[height<={h}]/best"),
                                        label: format!("{}p  —  {}", h, resolution_tag_name(h)),
                                        badge: size_badge,
                                        is_audio: false,
                                        is_image: false,
                                        height: h,
                                    });
                                }
                            }

                            if choices.is_empty() {
                                choices = build_standard_preset_heights();
                            } else {
                                let audio_mb = best_audio_bytes / 1_048_576.0;
                                let audio_badge = if audio_mb > 0.1 {
                                    format!("~{:.1} MB", audio_mb)
                                } else {
                                    "MP3".into()
                                };
                                choices.push(DynamicQualityChoice {
                                    format_id: "bestaudio/best".into(),
                                    label: "Audio MP3  —  320kbps + Cover Art".into(),
                                    badge: audio_badge,
                                    is_audio: true,
                                    is_image: false,
                                    height: 0,
                                });
                            }

                            // Default selection: Pick 1080p if available (or highest resolution <= 1080p)
                            let default_idx = choices
                                .iter()
                                .position(|c| c.height == 1080)
                                .or_else(|| choices.iter().position(|c| c.height > 0 && c.height <= 1080))
                                .unwrap_or(0);

                            return (tracks, choices, default_idx, clean_t);
                        }
                    } else {
                        let err_s = String::from_utf8_lossy(&out.stderr);
                        if is_no_video_error(&err_s) {
                            tracks.push(QueuedTrack {
                                url: url.clone(),
                                title: format!("{} Gallery", plat),
                                playlist_folder: None,
                            });
                            let choices = vec![DynamicQualityChoice {
                                format_id: "gallery".into(),
                                label: "Download Images / Gallery (Original Quality)".into(),
                                badge: "IMG".into(),
                                is_audio: false,
                                is_image: true,
                                height: 1080,
                            }];
                            return (tracks, choices, 0, format!("{} Gallery", plat));
                        }
                        if !is_auth_wall_error(&err_s) {
                            break;
                        }
                    }
                }
            }
        }

        // 3. Playlist or Multi-Link Batch expansion
        if url.contains("list=") || url.contains("/playlist") {
            let mut cmd = Command::new(get_ytdlp_cmd());
            cmd.arg("-J")
               .arg("--flat-playlist")
               .arg("--user-agent").arg(BROWSER_UA)
               .arg(&url)
               .stdout(Stdio::piped())
               .stderr(Stdio::null());

            if let Ok(Ok(out)) = tokio::time::timeout(Duration::from_secs(25), cmd.output()).await {
                if out.status.success() {
                    if let Ok(info) = serde_json::from_slice::<YtFullMeta>(&out.stdout) {
                        let folder = normalize_ytdlp_unicode(&info.title.unwrap_or_else(|| "Playlist".into()));
                        if let Some(entries) = info.entries {
                            for e in entries {
                                let entry_url = e.url.or(e.id.map(|id| format!("https://youtu.be/{}", id))).unwrap_or_default();
                                if !entry_url.is_empty() {
                                    let raw_title = e.title.unwrap_or_else(|| "Playlist Track".into());
                                    tracks.push(QueuedTrack {
                                        url: entry_url,
                                        title: normalize_ytdlp_unicode(&raw_title),
                                        playlist_folder: Some(folder.clone()),
                                    });
                                }
                            }
                            continue;
                        }
                    }
                }
            }
        }

        tracks.push(QueuedTrack {
            url,
            title: "Media Track".into(),
            playlist_folder: None,
        });
    }

    let choices = build_standard_preset_heights();
    let default_idx = choices.iter().position(|c| c.height == 1080).unwrap_or(0);
    let summary_title = if tracks.len() > 1 {
        format!("Batch / Playlist ({} items)", tracks.len())
    } else {
        tracks.first().map(|t| t.title.clone()).unwrap_or_else(|| "Media".into())
    };

    (tracks, choices, default_idx, summary_title)
}

// --- SEQUENTIAL WORKER ENGINE ---

enum PopupWorkerEvent {
    UpdateAvailable { tag: String },
    ClipboardUpdated { raw_text: String, valid_joined: String },
    InspectionComplete {
        tracks: Vec<QueuedTrack>,
        choices: Vec<DynamicQualityChoice>,
        default_idx: usize,
        summary_title: String,
    },
    Filename { file_name: String },
    Progress {
        percent: f32,
        dl_bytes: f64,
        total_bytes: f64,
        direct_speed: Option<f64>,
        direct_eta: Option<String>,
    },
    TrackFinished { success: bool, error: Option<String>, was_killed: bool, final_dir: PathBuf },
}

async fn run_single_track_download(
    track: QueuedTrack,
    choice: DynamicQualityChoice,
    tx: mpsc::Sender<PopupWorkerEvent>,
    mut cancel_rx: oneshot::Receiver<()>,
) {
    let url = resolve_short_url(&track.url).await;
    let is_audio = choice.is_audio;
    let mut is_image = choice.is_image
        || (!is_audio && (is_direct_image_or_gallery_url(&url) || is_reddit_url(&url)));

    let mut auth_chain: Vec<Option<(&str, &str)>> = vec![None];
    if Path::new("cookies.txt").exists() {
        auth_chain.push(Some(("--cookies", "cookies.txt")));
    }
    auth_chain.push(Some(("--cookies-from-browser", "chrome")));
    auth_chain.push(Some(("--cookies-from-browser", "brave")));
    auth_chain.push(Some(("--cookies-from-browser", "firefox")));
    auth_chain.push(Some(("--cookies-from-browser", "edge")));

    let mut last_err = String::new();

    for auth in auth_chain {
        let base_dir = build_target_dir(&url, is_audio, is_image, track.playlist_folder.as_deref());
        let _ = std::fs::create_dir_all(&base_dir);

        let res = if is_image {
            execute_image_or_reddit_download(&url, &base_dir, auth, &tx, &mut cancel_rx).await
        } else {
            execute_ytdlp_download(&url, &base_dir, &choice, auth, &tx, &mut cancel_rx).await
        };

        match res {
            Ok(final_dir) => {
                let _ = tx.send(PopupWorkerEvent::TrackFinished {
                    success: true,
                    error: None,
                    was_killed: false,
                    final_dir,
                }).await;
                return;
            }
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("Canceled by user") {
                    let _ = tx.send(PopupWorkerEvent::TrackFinished {
                        success: false,
                        error: Some(err_str),
                        was_killed: true,
                        final_dir: base_dir,
                    }).await;
                    return;
                }
                if !is_image && is_no_video_error(&err_str) {
                    is_image = true;
                    let img_dir = build_target_dir(&url, false, true, track.playlist_folder.as_deref());
                    let _ = std::fs::create_dir_all(&img_dir);
                    if let Ok(final_dir) = execute_image_or_reddit_download(&url, &img_dir, auth, &tx, &mut cancel_rx).await {
                        let _ = tx.send(PopupWorkerEvent::TrackFinished {
                            success: true,
                            error: None,
                            was_killed: false,
                            final_dir,
                        }).await;
                        return;
                    }
                }
                last_err = err_str.clone();
                if !is_auth_wall_error(&err_str) {
                    break;
                }
            }
        }
    }

    let fallback_dir = build_target_dir(&url, is_audio, is_image, track.playlist_folder.as_deref());
    let _ = tx.send(PopupWorkerEvent::TrackFinished {
        success: false,
        error: Some(last_err),
        was_killed: false,
        final_dir: fallback_dir,
    }).await;
}

async fn execute_image_or_reddit_download(
    url: &str,
    base_dir: &Path,
    auth: Option<(&str, &str)>,
    tx: &mpsc::Sender<PopupWorkerEvent>,
    cancel_rx: &mut oneshot::Receiver<()>,
) -> Result<PathBuf> {
    if is_reddit_url(url) {
        let py_bin = if std::env::consts::OS == "windows" { "python" } else { "python3" };
        let mut cmd = Command::new(py_bin);
        cmd.arg("-c")
            .arg(REDDIT_HEADLESS_DOM_PY)
            .arg(url)
            .arg(base_dir)
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
                if e.is_empty() || line.contains("ERROR:") { *e = line; }
            }
        });

        let mut file_count = 0usize;
        let mut total_bytes = 0.0;
        let mut direct_video: Option<(String, String)> = None;

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
                                    direct_video = Some((parts[0].to_string(), parts[1].to_string()));
                                }
                            } else if !trimmed.is_empty() {
                                file_count += 1;
                                if let Ok(meta) = std::fs::metadata(trimmed) {
                                    total_bytes += meta.len() as f64;
                                } else {
                                    total_bytes += 250_000.0;
                                }
                                let _ = tx.send(PopupWorkerEvent::Filename { file_name: trimmed.to_string() }).await;
                                let _ = tx.send(PopupWorkerEvent::Progress {
                                    percent: (file_count as f32 * 25.0).min(95.0),
                                    dl_bytes: total_bytes,
                                    total_bytes,
                                    direct_speed: None,
                                    direct_eta: None,
                                }).await;
                            }
                        }
                        Ok(None) | Err(_) => break,
                    }
                }
            }
        }

        let status = child.wait().await?;

        if let Some((video_title, hls_url)) = direct_video {
            let vid_dir = build_target_dir(url, false, false, None);
            let _ = std::fs::create_dir_all(&vid_dir);
            let out_tmpl = vid_dir.join(format!("{}.%(ext)s", video_title)).to_string_lossy().to_string();

            let mut yt = Command::new(get_ytdlp_cmd());
            if let Some(ff) = get_local_ffmpeg_dir() { yt.arg("--ffmpeg-location").arg(ff); }
            yt.arg("--newline").arg("--no-colors").arg("--merge-output-format").arg("mp4").arg("-o").arg(&out_tmpl).arg(&hls_url);
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
                                            let _ = tx.send(PopupWorkerEvent::Filename { file_name: dest.trim().to_string() }).await;
                                        }
                                    } else if line.contains('%') {
                                        if let Some((percent, db, tb, spd, eta)) = parse_ytdlp_progress_line(&line) {
                                            let _ = tx.send(PopupWorkerEvent::Progress {
                                                percent,
                                                dl_bytes: db,
                                                total_bytes: tb,
                                                direct_speed: spd,
                                                direct_eta: eta,
                                            }).await;
                                        }
                                    }
                                }
                            }
                            Ok(None) | Err(_) => break,
                        }
                    }
                }
            }
            if yt_child.wait().await?.success() {
                return Ok(vid_dir);
            }
        }

        if status.success() && file_count > 0 {
            return Ok(base_dir.to_path_buf());
        }
        let e = err_msg.lock().await.clone();
        return Err(anyhow::anyhow!(if e.is_empty() { "Reddit DOM extractor found no media.".into() } else { e }));
    }

    let mut cmd = Command::new(get_gallerydl_cmd());
    if let Some((flag, val)) = auth {
        cmd.arg(flag).arg(val);
    }
    cmd.arg("--user-agent").arg(BROWSER_UA).arg("-D").arg(base_dir).arg(url);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

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
            if e.is_empty() || line.to_lowercase().contains("error") { *e = line; }
        }
    });

    let mut file_count = 0usize;
    let mut total_bytes = 0.0;

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
                                total_bytes += meta.len() as f64;
                            } else {
                                total_bytes += 250_000.0;
                            }
                            let _ = tx.send(PopupWorkerEvent::Filename { file_name: trimmed.to_string() }).await;
                            let _ = tx.send(PopupWorkerEvent::Progress {
                                percent: (file_count as f32 * 25.0).min(95.0),
                                dl_bytes: total_bytes,
                                total_bytes,
                                direct_speed: None,
                                direct_eta: None,
                            }).await;
                        }
                    }
                    Ok(None) | Err(_) => break,
                }
            }
        }
    }

    let status = child.wait().await?;
    let e = err_msg.lock().await.clone();
    if !status.success() || (file_count == 0 && !e.is_empty()) {
        return Err(anyhow::anyhow!(if e.is_empty() { "Login required".into() } else { e }));
    }
    Ok(base_dir.to_path_buf())
}

async fn execute_ytdlp_download(
    url: &str,
    base_dir: &Path,
    choice: &DynamicQualityChoice,
    auth: Option<(&str, &str)>,
    tx: &mpsc::Sender<PopupWorkerEvent>,
    cancel_rx: &mut oneshot::Receiver<()>,
) -> Result<PathBuf> {
    let mut cmd = Command::new(get_ytdlp_cmd());
    cmd.arg("--newline")
       .arg("--continue")
       .arg("--no-colors")
       .arg("--no-playlist")
       .arg("--user-agent").arg(BROWSER_UA);

    if let Some(ff) = get_local_ffmpeg_dir() {
        cmd.arg("--ffmpeg-location").arg(ff);
    }
    if let Some((flag, val)) = auth {
        cmd.arg(flag).arg(val);
    }

    if choice.is_audio {
        cmd.arg("-f").arg("bestaudio/best")
           .arg("-x")
           .arg("--audio-format").arg("mp3")
           .arg("--embed-thumbnail")
           .arg("--add-metadata")
           .arg("-o").arg(base_dir.join("%(title)s.%(ext)s").to_string_lossy().to_string());
    } else {
        cmd.arg("-f").arg(&choice.format_id)
           .arg("--merge-output-format").arg("mp4")
           .arg("--embed-subs")
           .arg("--sub-langs").arg("orig,en")
           .arg("--embed-thumbnail")
           .arg("-o").arg(base_dir.join("%(title)s_%(resolution)s.%(ext)s").to_string_lossy().to_string());
    }

    cmd.arg(url).stdout(Stdio::piped()).stderr(Stdio::piped());

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
            if line.contains("ERROR:") { *e = line; }
        }
    });

    let mut downloaded_any = false;

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
                        if line.starts_with("[download]") {
                            if line.contains("Destination:") {
                                downloaded_any = true;
                                if let Some(dest) = line.split("Destination:").nth(1) {
                                    let _ = tx.send(PopupWorkerEvent::Filename { file_name: dest.trim().to_string() }).await;
                                }
                            } else if line.contains("has already been downloaded") {
                                downloaded_any = true;
                                if let Some(dest) = line.strip_prefix("[download]") {
                                    if let Some(p) = dest.split("has already been downloaded").next() {
                                        let _ = tx.send(PopupWorkerEvent::Filename { file_name: p.trim().to_string() }).await;
                                    }
                                }
                            } else if line.contains('%') {
                                downloaded_any = true;
                                if let Some((percent, db, tb, spd, eta)) = parse_ytdlp_progress_line(&line) {
                                    let _ = tx.send(PopupWorkerEvent::Progress {
                                        percent,
                                        dl_bytes: db,
                                        total_bytes: tb,
                                        direct_speed: spd,
                                        direct_eta: eta,
                                    }).await;
                                }
                            }
                        } else if line.contains("Merging formats into") {
                            if let Some(dest) = line.split("into").nth(1) {
                                let clean_dest = dest.trim().trim_matches('"');
                                let _ = tx.send(PopupWorkerEvent::Filename { file_name: clean_dest.to_string() }).await;
                            }
                        }
                    }
                    Ok(None) | Err(_) => break,
                }
            }
        }
    }

    let status = child.wait().await?;
    remove_sidecar_subtitles_in_dir(base_dir);

    let e = err_msg.lock().await.clone();
    if !status.success() || (!downloaded_any && !e.is_empty()) {
        return Err(anyhow::anyhow!(if e.is_empty() { "Process exited with error".into() } else { e }));
    }
    Ok(base_dir.to_path_buf())
}

// --- POPUP GUI APPLICATION STATE ---

#[derive(PartialEq)]
enum PopupPhase {
    Input,
    Inspecting,
    QualitySelect,
    Downloading,
    Paused,
    Completed,
    Failed(String),
}

struct QuickPopupApp {
    rt: Arc<tokio::runtime::Runtime>,
    theme: OsTheme,
    phase: PopupPhase,
    input_url: String,
    last_seen_clipboard: String,
    input_warning: Option<(String, Instant)>,
    auto_focused: bool,
    available_update: Option<String>,

    inspected_summary_title: String,
    quality_choices: Vec<DynamicQualityChoice>,
    selected_choice_idx: usize,

    tracks: Vec<QueuedTrack>,
    current_idx: usize,
    current_title: String,
    current_platform: String,
    current_target_files: Vec<String>,
    last_saved_dir: PathBuf,

    percent: f32,
    dl_bytes: f64,
    total_bytes: f64,
    smoothed_speed: f64,
    direct_eta: Option<String>,
    is_resuming_handshake: bool,
    last_dl_bytes: f64,
    last_speed_update: Instant,

    cancel_tx: Option<oneshot::Sender<()>>,
    worker_tx: mpsc::Sender<PopupWorkerEvent>,
    worker_rx: mpsc::Receiver<PopupWorkerEvent>,
    completed_at: Option<Instant>,
}

impl QuickPopupApp {
    fn new(cc: &eframe::CreationContext<'_>, rt: Arc<tokio::runtime::Runtime>) -> Self {
        let theme = OsTheme::detect();

        let mut style = (*cc.egui_ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(12.0, 6.0);

        let mut visuals = egui::Visuals::dark();
        visuals.window_rounding = egui::Rounding::same(14.0);
        visuals.menu_rounding = egui::Rounding::same(10.0);
        visuals.window_fill = theme.bg_surface;
        visuals.panel_fill = theme.bg_surface;
        visuals.extreme_bg_color = theme.card_elevated;
        visuals.selection.bg_fill = theme.accent_primary.linear_multiply(0.35);
        visuals.selection.stroke = egui::Stroke::new(1.5_f32, theme.accent_primary);

        visuals.widgets.noninteractive.bg_fill = theme.bg_surface;
        visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, theme.border_subtle);
        visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0_f32, theme.text_primary);

        visuals.widgets.inactive.bg_fill = theme.card_elevated;
        visuals.widgets.inactive.weak_bg_fill = theme.card_elevated;
        visuals.widgets.inactive.rounding = egui::Rounding::same(8.0);
        visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, theme.text_primary);
        visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, theme.border_subtle);

        visuals.widgets.hovered.bg_fill = theme.card_hover;
        visuals.widgets.hovered.weak_bg_fill = theme.card_hover;
        visuals.widgets.hovered.rounding = egui::Rounding::same(8.0);
        visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0_f32, egui::Color32::WHITE);
        visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, theme.accent_primary);

        visuals.widgets.active.bg_fill = theme.accent_primary;
        visuals.widgets.active.weak_bg_fill = theme.accent_primary;
        visuals.widgets.active.rounding = egui::Rounding::same(8.0);
        visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0_f32, theme.accent_text_on_primary);

        style.visuals = visuals;
        cc.egui_ctx.set_style(style);

        let mut initial_url = String::new();
        let raw_clip = read_system_clipboard_robust();
        let valid = extract_valid_urls_from_text(&raw_clip);
        if !valid.is_empty() {
            initial_url = valid.join(" ");
        }

        let (worker_tx, worker_rx) = mpsc::channel(100);

        // 1. Check GitHub Releases in the background for new updates!
        let update_tx = worker_tx.clone();
        rt.spawn(async move {
            if let Some(tag) = check_github_for_updates().await {
                let _ = update_tx.send(PopupWorkerEvent::UpdateAvailable { tag }).await;
            }
        });

        // 2. Live Clipboard Auto-Watcher
        let clip_tx = worker_tx.clone();
        let initial_clip_clone = raw_clip.clone();
        std::thread::spawn(move || {
            let mut last_clip = initial_clip_clone;
            loop {
                std::thread::sleep(Duration::from_millis(650));
                let current = read_system_clipboard_robust();
                if !current.is_empty() && current != last_clip {
                    last_clip = current.clone();
                    let valid_urls = extract_valid_urls_from_text(&current);
                    if !valid_urls.is_empty() {
                        let _ = clip_tx.blocking_send(PopupWorkerEvent::ClipboardUpdated {
                            raw_text: current,
                            valid_joined: valid_urls.join(" "),
                        });
                    }
                }
            }
        });

        Self {
            rt,
            theme,
            phase: PopupPhase::Input,
            input_url: initial_url,
            last_seen_clipboard: raw_clip,
            input_warning: None,
            auto_focused: false,
            available_update: None,
            inspected_summary_title: String::new(),
            quality_choices: Vec::new(),
            selected_choice_idx: 0,
            tracks: Vec::new(),
            current_idx: 0,
            current_title: String::new(),
            current_platform: String::new(),
            current_target_files: Vec::new(),
            last_saved_dir: dirs::download_dir().unwrap_or_else(|| PathBuf::from(".")),
            percent: 0.0,
            dl_bytes: 0.0,
            total_bytes: 0.0,
            smoothed_speed: 0.0,
            direct_eta: None,
            is_resuming_handshake: false,
            last_dl_bytes: 0.0,
            last_speed_update: Instant::now(),
            cancel_tx: None,
            worker_tx,
            worker_rx,
            completed_at: None,
        }
    }

    fn paste_from_clipboard(&mut self) {
        let raw_clip = read_system_clipboard_robust();
        self.last_seen_clipboard = raw_clip.clone();
        let valid = extract_valid_urls_from_text(&raw_clip);
        if !valid.is_empty() {
            self.input_url = valid.join(" ");
            self.input_warning = None;
        } else if !raw_clip.trim().is_empty() {
            self.input_warning = Some((
                "Clipboard does not contain a valid http:// or https:// link.".into(),
                Instant::now(),
            ));
        }
    }

    fn submit_input(&mut self) {
        let urls = extract_valid_urls_from_text(&self.input_url);
        if urls.is_empty() {
            self.input_url.clear();
            self.input_warning = Some((
                "Ignored non-URL text. Please paste a valid http:// or https:// link.".into(),
                Instant::now(),
            ));
            return;
        }

        self.input_warning = None;
        self.phase = PopupPhase::Inspecting;
        let tx = self.worker_tx.clone();
        self.rt.spawn(async move {
            let (tracks, choices, default_idx, summary_title) = inspect_urls_and_detect_qualities(urls).await;
            let _ = tx
                .send(PopupWorkerEvent::InspectionComplete {
                    tracks,
                    choices,
                    default_idx,
                    summary_title,
                })
                .await;
        });
    }

    fn start_or_resume_current_track(&mut self, is_resume: bool) {
        if self.current_idx >= self.tracks.len() {
            self.phase = PopupPhase::Completed;
            self.completed_at = Some(Instant::now());
            return;
        }

        let Some(choice) = self.quality_choices.get(self.selected_choice_idx).cloned() else {
            return;
        };

        let track = self.tracks[self.current_idx].clone();
        self.current_platform = detect_platform_folder(&track.url).to_string();

        if !is_resume {
            self.current_title = track.title.clone();
            self.current_target_files.clear();
            self.percent = 0.0;
            self.dl_bytes = 0.0;
            self.total_bytes = 0.0;
            self.direct_eta = None;
            self.last_dl_bytes = 0.0;
            self.is_resuming_handshake = false;
        } else {
            self.is_resuming_handshake = true;
        }

        self.smoothed_speed = 0.0;
        self.last_speed_update = Instant::now();
        self.phase = PopupPhase::Downloading;

        let (cancel_tx, cancel_rx) = oneshot::channel();
        self.cancel_tx = Some(cancel_tx);

        let tx = self.worker_tx.clone();
        self.rt.spawn(async move {
            run_single_track_download(track, choice, tx, cancel_rx).await;
        });
    }

    fn toggle_pause(&mut self) {
        if self.phase == PopupPhase::Downloading {
            if let Some(cancel) = self.cancel_tx.take() {
                let _ = cancel.send(());
            }
            self.smoothed_speed = 0.0;
            self.is_resuming_handshake = false;
            self.phase = PopupPhase::Paused;
        } else if self.phase == PopupPhase::Paused {
            self.start_or_resume_current_track(true);
        }
    }

    fn skip_current_track(&mut self) {
        if let Some(cancel) = self.cancel_tx.take() {
            let _ = cancel.send(());
        }
        if let Some(track) = self.tracks.get(self.current_idx) {
            let choice = self.quality_choices.get(self.selected_choice_idx);
            let is_audio = choice.map(|c| c.is_audio).unwrap_or(false);
            let is_img = choice.map(|c| c.is_image).unwrap_or(false)
                || (!is_audio && (is_direct_image_or_gallery_url(&track.url) || is_reddit_url(&track.url)));
            let dir = build_target_dir(&track.url, is_audio, is_img, track.playlist_folder.as_deref());
            let files = self.current_target_files.clone();
            let title = self.current_title.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(80));
                cleanup_track_garbage(&files, &title, &dir);
            });
        }
        self.current_idx += 1;
        self.start_or_resume_current_track(false);
    }

    fn cancel_all_and_exit(&mut self, ctx: &egui::Context) {
        if let Some(cancel) = self.cancel_tx.take() {
            let _ = cancel.send(());
        }
        if let Some(track) = self.tracks.get(self.current_idx) {
            let choice = self.quality_choices.get(self.selected_choice_idx);
            let is_audio = choice.map(|c| c.is_audio).unwrap_or(false);
            let is_img = choice.map(|c| c.is_image).unwrap_or(false)
                || (!is_audio && (is_direct_image_or_gallery_url(&track.url) || is_reddit_url(&track.url)));
            let dir = build_target_dir(&track.url, is_audio, is_img, track.playlist_folder.as_deref());
            std::thread::sleep(Duration::from_millis(80));
            cleanup_track_garbage(&self.current_target_files, &self.current_title, &dir);
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

impl eframe::App for QuickPopupApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let theme = self.theme;

        while let Ok(ev) = self.worker_rx.try_recv() {
            match ev {
                PopupWorkerEvent::UpdateAvailable { tag } => {
                    self.available_update = Some(tag);
                }
                PopupWorkerEvent::ClipboardUpdated { raw_text, valid_joined } => {
                    if self.phase == PopupPhase::Input && raw_text != self.last_seen_clipboard {
                        self.last_seen_clipboard = raw_text;
                        self.input_url = valid_joined;
                        self.input_warning = None;
                    }
                }
                PopupWorkerEvent::InspectionComplete {
                    tracks,
                    choices,
                    default_idx,
                    summary_title,
                } => {
                    if tracks.is_empty() {
                        self.phase = PopupPhase::Failed("Could not extract any valid media from this link.".into());
                    } else {
                        self.tracks = tracks;
                        self.quality_choices = choices;
                        self.selected_choice_idx = default_idx;
                        self.inspected_summary_title = summary_title;
                        self.current_idx = 0;
                        // Resize window smoothly to fit the full scrollable quality selector list!
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(640.0, 275.0)));
                        self.phase = PopupPhase::QualitySelect;
                    }
                }
                PopupWorkerEvent::Filename { file_name } => {
                    if !self.current_target_files.contains(&file_name) {
                        self.current_target_files.push(file_name.clone());
                    }
                    let cleaned = clean_title_from_filename(&file_name);
                    if !cleaned.is_empty() {
                        self.current_title = cleaned;
                    }
                }
                PopupWorkerEvent::Progress {
                    percent,
                    dl_bytes,
                    total_bytes,
                    direct_speed,
                    direct_eta,
                } => {
                    if self.is_resuming_handshake && percent < self.percent {
                        continue;
                    }
                    self.is_resuming_handshake = false;
                    self.percent = percent;
                    if dl_bytes > 0.0 {
                        self.dl_bytes = dl_bytes;
                    }
                    if total_bytes > 0.0 {
                        self.total_bytes = total_bytes;
                    }
                    if direct_eta.is_some() {
                        self.direct_eta = direct_eta;
                    }

                    if let Some(spd) = direct_speed {
                        if self.smoothed_speed == 0.0 {
                            self.smoothed_speed = spd;
                        } else {
                            self.smoothed_speed = 0.35 * spd + 0.65 * self.smoothed_speed;
                        }
                        self.last_dl_bytes = self.dl_bytes;
                        self.last_speed_update = Instant::now();
                    } else {
                        let now = Instant::now();
                        let dt = now.duration_since(self.last_speed_update).as_secs_f64();
                        if dt > 0.4 && self.dl_bytes >= self.last_dl_bytes {
                            let inst_speed = (self.dl_bytes - self.last_dl_bytes) / dt;
                            if self.smoothed_speed == 0.0 {
                                self.smoothed_speed = inst_speed.max(0.0);
                            } else {
                                self.smoothed_speed = (0.25 * inst_speed + 0.75 * self.smoothed_speed).max(0.0);
                            }
                            self.last_dl_bytes = self.dl_bytes;
                            self.last_speed_update = now;
                        }
                    }
                }
                PopupWorkerEvent::TrackFinished { success, error, was_killed, final_dir } => {
                    self.last_saved_dir = final_dir.clone();
                    if was_killed {
                        continue;
                    }
                    if success {
                        if let Some(track) = self.tracks.get(self.current_idx) {
                            let choice = self.quality_choices.get(self.selected_choice_idx);
                            let media_type = if choice.map(|c| c.is_audio).unwrap_or(false) {
                                "Audio"
                            } else if choice.map(|c| c.is_image).unwrap_or(false)
                                || is_direct_image_or_gallery_url(&track.url)
                                || is_reddit_url(&track.url)
                            {
                                "Image"
                            } else {
                                "Video"
                            };
                            HistoryStore::record_download(
                                &self.current_title,
                                &track.url,
                                &self.current_platform,
                                media_type,
                                &final_dir.to_string_lossy(),
                            );
                        }
                        self.current_idx += 1;
                        if self.current_idx < self.tracks.len() {
                            self.start_or_resume_current_track(false);
                        } else {
                            self.percent = 100.0;
                            self.phase = PopupPhase::Completed;
                            self.completed_at = Some(Instant::now());
                        }
                    } else {
                        self.phase = PopupPhase::Failed(error.unwrap_or_else(|| "Download failed".into()));
                    }
                }
            }
        }

        if let Some((_, ts)) = &self.input_warning {
            if ts.elapsed() > Duration::from_secs(3) {
                self.input_warning = None;
            }
        }

        if self.phase == PopupPhase::Completed {
            if let Some(done_time) = self.completed_at {
                if done_time.elapsed() > Duration::from_millis(1600) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }

        // Global Keyboard Navigation
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.phase == PopupPhase::QualitySelect {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(640.0, 162.0)));
                self.phase = PopupPhase::Input;
                self.auto_focused = false;
            } else if self.phase == PopupPhase::Downloading || self.phase == PopupPhase::Paused {
                self.cancel_all_and_exit(ctx);
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }

        if self.phase == PopupPhase::QualitySelect {
            if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) && self.selected_choice_idx > 0 {
                self.selected_choice_idx -= 1;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown))
                && self.selected_choice_idx + 1 < self.quality_choices.len()
            {
                self.selected_choice_idx += 1;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(640.0, 162.0)));
                self.start_or_resume_current_track(false);
            }
        } else if (self.phase == PopupPhase::Downloading || self.phase == PopupPhase::Paused)
            && ctx.input(|i| i.key_pressed(egui::Key::Space))
        {
            self.toggle_pause();
        }

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(theme.bg_surface)
                    .stroke(egui::Stroke::new(1.0_f32, theme.border_subtle))
                    .rounding(egui::Rounding::same(14.0))
                    .inner_margin(egui::Margin::symmetric(18.0, 14.0)),
            )
            .show(ctx, |ui| {
                // --- TOP HEADER ROW ---
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Media Downloader")
                            .size(15.0)
                            .strong()
                            .color(theme.text_primary),
                    );
                    ui.label(
                        egui::RichText::new(format!("v{} • {}", APP_VERSION, theme.os_badge))
                            .size(11.8)
                            .color(theme.text_muted),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let close_btn = egui::Button::new(
                            egui::RichText::new("×").size(15.0).strong().color(theme.text_primary),
                        )
                        .rounding(egui::Rounding::same(14.0))
                        .min_size(egui::vec2(28.0, 28.0))
                        .fill(theme.card_elevated);

                        if ui.add(close_btn).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Close (Esc)").clicked() {
                            if self.phase == PopupPhase::Downloading || self.phase == PopupPhase::Paused {
                                self.cancel_all_and_exit(ctx);
                            } else {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        }

                        // Live 1-Click Self-Updater Pill (Appears automatically when you publish a new GitHub Release!)
                        if let Some(ref new_tag) = self.available_update {
                            ui.add_space(6.0);
                            let upd_btn = egui::Button::new(
                                egui::RichText::new(format!("⬆ Update to {} Now", new_tag))
                                    .size(12.0)
                                    .strong()
                                    .color(egui::Color32::from_rgb(15, 20, 15)),
                            )
                            .rounding(egui::Rounding::same(14.0))
                            .fill(theme.accent_green);

                            if ui.add(upd_btn).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                trigger_self_update_and_restart();
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        }
                    });
                });

                ui.add_space(8.0);

                match &self.phase {
                    PopupPhase::Input => {
                        let input_id = egui::Id::new("popup_url_input");

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let dl_btn = egui::Button::new(
                                egui::RichText::new("Inspect & Download")
                                    .size(13.2)
                                    .strong()
                                    .color(theme.accent_text_on_primary),
                            )
                            .rounding(egui::Rounding::same(8.0))
                            .min_size(egui::vec2(142.0, 34.0))
                            .fill(theme.accent_primary);

                            if ui.add(dl_btn).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                self.submit_input();
                            }

                            let is_empty = self.input_url.is_empty();
                            let action_label = if is_empty { "Paste" } else { "Clear" };
                            let action_btn = egui::Button::new(
                                egui::RichText::new(action_label).size(13.0).color(theme.text_primary),
                            )
                            .rounding(egui::Rounding::same(8.0))
                            .min_size(egui::vec2(64.0, 34.0))
                            .fill(theme.card_elevated);

                            if ui.add(action_btn).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                if is_empty {
                                    self.paste_from_clipboard();
                                } else {
                                    self.input_url.clear();
                                    self.input_warning = None;
                                    self.last_seen_clipboard = read_system_clipboard_robust();
                                }
                                ui.memory_mut(|m| m.request_focus(input_id));
                            }

                            let input_edit = egui::TextEdit::singleline(&mut self.input_url)
                                .id(input_id)
                                .font(egui::FontId::proportional(14.0))
                                .hint_text("Paste or copy any video, audio, gallery, or playlist link...")
                                .desired_width(ui.available_width())
                                .margin(egui::Margin::symmetric(10.0, 8.0));

                            let resp = ui.add(input_edit);
                            if !self.auto_focused {
                                resp.request_focus();
                                self.auto_focused = true;
                            }

                            if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                self.submit_input();
                            }
                        });

                        ui.add_space(6.0);

                        if let Some((ref warn, _)) = self.input_warning {
                            ui.colored_label(theme.accent_red, egui::RichText::new(warn).size(12.5));
                        } else {
                            ui.label(
                                egui::RichText::new("Auto-detects copied links  •  Press Enter to inspect available qualities (1080p default)  •  Esc to close")
                                    .size(11.8)
                                    .color(theme.text_muted),
                            );
                        }
                    }

                    PopupPhase::Inspecting => {
                        ui.vertical_centered(|ui| {
                            ui.add_space(14.0);
                            ui.spinner();
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new("Detecting available video resolutions, audio & gallery formats...")
                                    .size(13.5)
                                    .color(theme.text_primary),
                            );
                        });
                    }

                    // --- STEP 2: DYNAMIC QUALITY & FORMAT SELECTOR (Defaults to 1080p, shows ALL detected streams) ---
                    PopupPhase::QualitySelect => {
                        let safe_t: String = self.inspected_summary_title.chars().take(62).collect();
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Select Quality:").size(13.0).strong().color(theme.accent_primary));
                            ui.label(egui::RichText::new(safe_t).size(13.0).strong().color(theme.text_primary));
                        });

                        ui.add_space(4.0);

                        let mut start_clicked = false;

                        egui::ScrollArea::vertical()
                            .max_height(138.0)
                            .auto_shrink([false; 2])
                            .show(ui, |ui| {
                                for (idx, choice) in self.quality_choices.iter().enumerate() {
                                    let is_selected = idx == self.selected_choice_idx;
                                    let row_bg = if is_selected { theme.accent_primary } else { theme.card_elevated };
                                    let fg_col = if is_selected { theme.accent_text_on_primary } else { theme.text_primary };
                                    let badge_col = if is_selected { theme.accent_text_on_primary } else { theme.accent_primary };

                                    let btn_text = format!(
                                        "{} {:<44} [{}]",
                                        if is_selected { "●" } else { " " },
                                        choice.label,
                                        choice.badge
                                    );
                                    let row_btn = egui::Button::new(
                                        egui::RichText::new(btn_text).size(13.0).strong().color(fg_col),
                                    )
                                    .fill(row_bg)
                                    .min_size(egui::vec2(ui.available_width() - 8.0, 28.0));

                                    let resp = ui.add(row_btn).on_hover_cursor(egui::CursorIcon::PointingHand);
                                    if resp.clicked() {
                                        self.selected_choice_idx = idx;
                                    }
                                    if resp.double_clicked() {
                                        self.selected_choice_idx = idx;
                                        start_clicked = true;
                                    }
                                    let _ = badge_col;
                                }
                            });

                        ui.add_space(6.0);

                        ui.horizontal(|ui| {
                            if ui.button("← Back").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(640.0, 162.0)));
                                self.phase = PopupPhase::Input;
                                self.auto_focused = false;
                            }

                            ui.label(
                                egui::RichText::new("↑/↓ to choose  •  Enter or Double-Click to start")
                                    .size(11.8)
                                    .color(theme.text_muted),
                            );

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let start_btn = egui::Button::new(
                                    egui::RichText::new("⬇ Start Download")
                                        .size(13.2)
                                        .strong()
                                        .color(theme.accent_text_on_primary),
                                )
                                .fill(theme.accent_primary)
                                .min_size(egui::vec2(162.0, 30.0));

                                if ui.add(start_btn).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() || start_clicked {
                                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(640.0, 162.0)));
                                    self.start_or_resume_current_track(false);
                                }
                            });
                        });
                    }

                    PopupPhase::Downloading | PopupPhase::Paused => {
                        let is_paused = self.phase == PopupPhase::Paused;
                        let total_tracks = self.tracks.len();
                        let track_badge = if total_tracks > 1 {
                            format!("{} of {}  •  ", self.current_idx + 1, total_tracks)
                        } else {
                            String::new()
                        };

                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!("{}{}", track_badge, self.current_platform))
                                    .size(13.0)
                                    .strong()
                                    .color(theme.accent_primary),
                            );
                            ui.label(egui::RichText::new("—").color(theme.text_muted));
                            let safe_title: String = self.current_title.chars().take(52).collect();
                            ui.label(
                                egui::RichText::new(safe_title)
                                    .size(13.5)
                                    .strong()
                                    .color(theme.text_primary),
                            );
                        });

                        ui.add_space(6.0);

                        let eta_str = if let Some(ref d_eta) = self.direct_eta {
                            d_eta.clone()
                        } else if self.smoothed_speed > 0.0 && self.total_bytes > self.dl_bytes {
                            format_time((self.total_bytes - self.dl_bytes) / self.smoothed_speed)
                        } else {
                            "--:--".into()
                        };

                        let status_line = if is_paused {
                            format!(
                                "Paused  —  {:.1}%  ({} / {})",
                                self.percent,
                                format_bytes(self.dl_bytes),
                                format_bytes(self.total_bytes)
                            )
                        } else if self.is_resuming_handshake {
                            format!(
                                "Resuming from {:.1}%  ({} / {})...",
                                self.percent,
                                format_bytes(self.dl_bytes),
                                format_bytes(self.total_bytes)
                            )
                        } else {
                            format!(
                                "{:.1}%   •   {} / {}   •   {}/s   •   ETA {}",
                                self.percent,
                                format_bytes(self.dl_bytes),
                                format_bytes(self.total_bytes),
                                format_bytes(self.smoothed_speed),
                                eta_str
                            )
                        };

                        let bar_color = if is_paused { theme.accent_amber } else { theme.accent_primary };

                        let pb = egui::ProgressBar::new((self.percent / 100.0).clamp(0.0, 1.0))
                            .desired_height(22.0)
                            .rounding(egui::Rounding::same(8.0))
                            .text(
                                egui::RichText::new(status_line)
                                    .size(12.5)
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            )
                            .fill(bar_color);
                        ui.add(pb);

                        ui.add_space(6.0);

                        ui.horizontal(|ui| {
                            let pause_label = if is_paused { "▶  Resume" } else { "⏸  Pause" };
                            let pause_btn = egui::Button::new(
                                egui::RichText::new(pause_label).size(13.0).strong().color(theme.text_primary),
                            )
                            .min_size(egui::vec2(96.0, 28.0));

                            if ui.add(pause_btn).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Shortcut: Spacebar").clicked() {
                                self.toggle_pause();
                            }

                            if self.tracks.len() > 1 {
                                let skip_label = if self.current_idx + 1 == self.tracks.len() {
                                    "⏭  Skip Last Track"
                                } else {
                                    "⏭  Skip Track"
                                };
                                let skip_btn = egui::Button::new(
                                    egui::RichText::new(skip_label).size(13.0).color(theme.text_primary),
                                )
                                .min_size(egui::vec2(110.0, 28.0));

                                if ui.add(skip_btn).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                    self.skip_current_track();
                                }
                            }

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let cancel_btn = egui::Button::new(
                                    egui::RichText::new("×  Cancel").size(13.5).strong().color(theme.accent_red),
                                )
                                .min_size(egui::vec2(92.0, 28.0));

                                if ui.add(cancel_btn).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                    self.cancel_all_and_exit(ctx);
                                }
                            });
                        });
                    }

                    PopupPhase::Completed => {
                        ui.vertical_centered(|ui| {
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new("✔  Download Complete")
                                    .size(16.0)
                                    .strong()
                                    .color(theme.accent_green),
                            );
                            ui.add_space(6.0);
                            let open_btn = egui::Button::new(
                                egui::RichText::new("Open Folder").size(13.0).strong().color(theme.text_primary),
                            )
                            .min_size(egui::vec2(110.0, 30.0));

                            if ui.add(open_btn).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                open_folder_in_os(&self.last_saved_dir);
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        });
                    }

                    PopupPhase::Failed(err) => {
                        let clean_err: String = err.chars().take(95).collect();
                        ui.label(
                            egui::RichText::new("Download Failed")
                                .size(14.0)
                                .strong()
                                .color(theme.accent_red),
                        );
                        ui.label(egui::RichText::new(clean_err).size(12.2).color(theme.text_muted));
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            if ui.button("Try Another Link").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(640.0, 162.0)));
                                self.phase = PopupPhase::Input;
                                self.auto_focused = false;
                            }
                            if ui.button("Close").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        });
                    }
                }
            });

        ctx.request_repaint_after(Duration::from_millis(50));
    }
}

fn main() -> eframe::Result<()> {
    if std::env::consts::OS == "linux" {
        std::env::set_var("WINIT_UNIX_BACKEND", "x11");
    }

    let rt = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to create Tokio runtime"),
    );

    let viewport = egui::ViewportBuilder::default()
        .with_title("Media Downloader")
        .with_inner_size([640.0, 162.0])
        .with_min_inner_size([640.0, 162.0])
        .with_max_inner_size([640.0, 290.0])
        .with_decorations(false)
        .with_always_on_top()
        .with_resizable(true);

    let options = eframe::NativeOptions {
        viewport,
        centered: true,
        ..Default::default()
    };

    let rt_clone = rt.clone();
    eframe::run_native(
        "Media Downloader",
        options,
        Box::new(move |cc| Ok(Box::new(QuickPopupApp::new(cc, rt_clone)))),
    )
}