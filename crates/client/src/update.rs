//! Keeping the game up to date.
//!
//! At start-up and every few hours (setting "Update automatically", on by
//! default) a background thread asks GitHub for the latest release. When it
//! is newer, the file made for this computer is downloaded, checked against
//! the SHA-256 published with the release, and installed:
//!
//! - **Windows, installed with the .msi** (under Program Files): the new
//!   installer is kept; "Restart now" closes the game, runs it with a
//!   progress bar only (`msiexec /passive`) and starts the game again.
//! - **Portable copies and the one-line installers** (Windows .zip, macOS
//!   and Linux .tar.xz): the new executable is taken out of the archive,
//!   written next to the running one, tried (`--version` must answer with
//!   the new version) and swapped in with a rename, keeping the old one as
//!   `snook-legend.old` (`snook-legend.old.exe`). On Windows the running file is
//!   renamed aside first and put back if anything fails. "Restart now"
//!   starts the new one with the same arguments.
//! - **Anything else** (no archive for this platform, a folder the game
//!   can't write to, no checksum published): the menu offers the releases
//!   page instead.
//!
//! The Microsoft Store build never updates itself (the Store does it), nor
//! do development builds, unless `SNOOK_LEGEND_UPDATE_FORCE` is set or
//! `SNOOK_LEGEND_UPDATE_API` points at a test release (a URL that answers like
//! GitHub's `releases/latest`, e.g. a file served by `python3 -m http.server`).
//!
//! Nothing here interrupts a game: the notice waits in the main menu.

use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::i18n::{t, tw};

/// The latest release's page.
pub const RELEASES: &str = concat!(env!("CARGO_PKG_REPOSITORY"), "/releases/latest");

/// This version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The platform this copy was built for (from `build.rs`).
const TARGET: &str = env!("SNOOK_LEGEND_TARGET");

/// The platforms releases have archives for (`dist-workspace.toml`).
const RELEASED_TARGETS: [&str; 5] = [
    "aarch64-apple-darwin",
    "aarch64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "x86_64-unknown-linux-gnu",
    "x86_64-pc-windows-msvc",
];

/// The executable's name inside the archives.
const EXE_NAME: &str = if cfg!(windows) { "snook-legend.exe" } else { "snook-legend" };

/// How often to look again while the game runs.
const RECHECK: Duration = Duration::from_secs(6 * 3600);
/// The first look, once the game has started.
const FIRST_CHECK: Duration = Duration::from_secs(3);
/// Larger downloads are refused (the game is a few megabytes).
const MAX_DOWNLOAD: u64 = 300 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum How {
    /// The Microsoft Store updates the game.
    Store,
    /// Installed with the .msi: download the new one and run it.
    Installer,
    /// A portable copy (or the one-line installers): replace the executable.
    Replace,
    /// No archive for this platform: the releases page.
    Page,
}

pub fn how() -> How {
    static HOW: OnceLock<How> = OnceLock::new();
    *HOW.get_or_init(find_how)
}

fn find_how() -> How {
    if crate::channel::STORE {
        return How::Store;
    }
    if cfg!(windows) && installed_with_msi() {
        return How::Installer;
    }
    if RELEASED_TARGETS.contains(&TARGET) && exe().is_some() {
        return How::Replace;
    }
    How::Page
}

/// The .msi installs under Program Files.
fn installed_with_msi() -> bool {
    exe()
        .and_then(|p| p.to_str().map(|s| s.to_ascii_lowercase()))
        .is_some_and(|p| p.contains("\\program files"))
}

/// The running executable, as it was when the game started (on Linux,
/// `current_exe` names a replaced file "… (deleted)").
fn exe() -> Option<PathBuf> {
    static EXE: OnceLock<Option<PathBuf>> = OnceLock::new();
    EXE.get_or_init(|| std::env::current_exe().ok().and_then(|p| p.canonicalize().ok())).clone()
}

/// Notes where the game runs from; call before anything can replace it.
pub fn init() {
    exe();
}

/// A development build (`cargo run`): release builds started from a Cargo
/// target folder are caught by [`in_cargo_target`].
fn development_build() -> bool {
    cfg!(debug_assertions)
}

/// The address that answers with the latest release.
fn api_url() -> String {
    crate::dev::var("SNOOK_LEGEND_UPDATE_API")
        .unwrap_or_else(|| RELEASES.replace("https://github.com/", "https://api.github.com/repos/"))
}

/// The executable is a build output in a Cargo target folder: never
/// replaced, whatever the folder is called and even when updates are
/// forced.
fn in_cargo_target() -> bool {
    exe().is_some_and(|p| p.parent().is_some_and(cargo_target_dir))
}

/// `dir` is in a Cargo profile folder (`target/release` and the like), or
/// a few folders down from one: the folder Cargo locks while it builds
/// (`.cargo-lock`), or the one with its `.fingerprint` and `deps`. Cargo
/// 1.100 keeps what it builds some folders further down than before (a
/// test's executable is in `build/<package>/<hash>/out`), so the folders
/// above are looked at too.
fn cargo_target_dir(dir: &Path) -> bool {
    dir.ancestors().take(6).any(|d| {
        d.join(".cargo-lock").is_file()
            || (d.join(".fingerprint").is_dir() && d.join("deps").is_dir())
    })
}

/// This copy may look for updates on its own (worked out once).
pub fn allowed() -> bool {
    static ALLOWED: OnceLock<bool> = OnceLock::new();
    *ALLOWED.get_or_init(|| {
        !crate::channel::STORE
            && !in_cargo_target()
            && (!development_build()
                || crate::dev::flag("SNOOK_LEGEND_UPDATE_FORCE")
                || crate::dev::flag("SNOOK_LEGEND_UPDATE_API"))
    })
}

// ---------------------------------------------------------------------------
// State, shared with the background thread.

/// What the updater is doing, for the menu and the settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Not checked yet (or updates are off).
    Idle,
    Checking,
    UpToDate,
    Downloading {
        version: String,
        done: u64,
        total: u64,
    },
    Installing {
        version: String,
    },
    /// Installed: the next start is the new version.
    Ready {
        version: String,
    },
    /// A newer version this copy can't install itself: the releases page.
    Available {
        version: String,
    },
    Failed(Failure),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// No connection, a timeout, or the server said no.
    Network,
    /// The download didn't match its checksum, or the archive is broken.
    Damaged,
    /// The new version couldn't be put in place (the old one is kept).
    Install,
}

impl Failure {
    pub fn message(self) -> &'static str {
        match self {
            Failure::Network => t("Couldn't reach the update server."),
            Failure::Damaged => t("The download was damaged. It will be tried again later."),
            Failure::Install => t("The new version couldn't be installed. This one stays."),
        }
    }
}

/// How a downloaded version is started.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Pending {
    /// The executable was replaced: start it again.
    Replaced,
    /// The Windows installer to run, and the SHA-256 it was checked
    /// against (checked again right before it runs: the player may restart
    /// hours after the download).
    #[cfg_attr(not(windows), allow(dead_code))]
    Installer(PathBuf, String),
}

struct State {
    status: Status,
    busy: bool,
    next_check: Option<Instant>,
    pending: Option<Pending>,
}

fn state() -> std::sync::MutexGuard<'static, State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE
        .get_or_init(|| {
            Mutex::new(State { status: Status::Idle, busy: false, next_check: None, pending: None })
        })
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

pub fn status() -> Status {
    state().status.clone()
}

fn set_status(s: Status) {
    state().status = s;
}

/// Once a frame: starts a check when one is due and updates are on.
pub fn tick(automatic: bool) {
    if !automatic || !allowed() || matches!(how(), How::Store) {
        return;
    }
    let mut s = state();
    if s.busy || s.pending.is_some() {
        return;
    }
    let now = Instant::now();
    let due = *s.next_check.get_or_insert(now + FIRST_CHECK);
    if now >= due {
        s.next_check = Some(now + RECHECK);
        drop(s);
        spawn();
    }
}

/// "Check now" (also with automatic updates off).
pub fn check_now() {
    if !allowed() {
        return;
    }
    {
        let mut s = state();
        if s.busy || s.pending.is_some() {
            return;
        }
        s.next_check = Some(Instant::now() + RECHECK);
    }
    spawn();
}

/// Whether a check or a download is under way.
pub fn busy() -> bool {
    state().busy
}

fn spawn() {
    {
        let mut s = state();
        s.busy = true;
        s.status = Status::Checking;
    }
    let started = std::thread::Builder::new().name("updater".into()).spawn(|| {
        let downloads = crate::config::config_dir().map(|d| d.join("updates"));
        let result = run(&api_url(), how(), exe(), downloads, &set_status);
        let mut s = state();
        s.busy = false;
        s.status = match result {
            Ok(Outcome::UpToDate) => Status::UpToDate,
            Ok(Outcome::Available(version)) => Status::Available { version },
            Ok(Outcome::Ready(version, pending)) => {
                s.pending = Some(pending);
                Status::Ready { version }
            }
            Err(f) => Status::Failed(f),
        };
    });
    if started.is_err() {
        let mut s = state();
        s.busy = false;
        s.status = Status::Failed(Failure::Install);
    }
}

/// Starts getting the new version when the server refuses this one (the
/// menu's and the lounge's "Update" button). Returns true when the game
/// should close.
pub fn start() -> bool {
    match how() {
        How::Store => {
            crate::util::open_store_updates();
            false
        }
        _ if state().pending.is_some() => restart(),
        How::Installer | How::Replace if allowed() => {
            if !busy() {
                match status() {
                    // Already known to need the page.
                    Status::Available { .. } => {
                        crate::util::open_url(RELEASES);
                    }
                    _ => check_now(),
                }
            }
            false
        }
        _ => {
            crate::util::open_url(RELEASES);
            false
        }
    }
}

/// What the update button says, for how this copy is updated and what the
/// updater is doing.
pub fn button_label() -> String {
    match (how(), status()) {
        (How::Store, _) => t("Update in the Microsoft Store").into(),
        (_, Status::Ready { .. }) => t("Restart now").into(),
        (_, Status::Downloading { done, total, .. }) => {
            tw("Downloading… {p}%", &[("p", &percent(done, total))])
        }
        (_, Status::Checking | Status::Installing { .. }) => t("Updating…").into(),
        (How::Installer | How::Replace, Status::Available { .. }) => {
            t("Download the new version").into()
        }
        (How::Installer | How::Replace, _) if allowed() => t("Update now").into(),
        _ => t("Download the new version").into(),
    }
}

pub fn percent(done: u64, total: u64) -> u64 {
    (done * 100).checked_div(total).unwrap_or(0).min(100)
}

/// One line about the updater, for the settings ("" when there's nothing
/// to say), and whether it's bad news.
pub fn describe(status: &Status) -> (String, bool) {
    match status {
        _ if crate::channel::STORE => {
            (t("The Microsoft Store keeps the game up to date.").into(), false)
        }
        _ if !allowed() => (t("Development build: no automatic updates.").into(), false),
        Status::Idle => (String::new(), false),
        Status::Checking => (t("Looking for a new version…").into(), false),
        Status::UpToDate => (t("This is the latest version.").into(), false),
        Status::Downloading { version, done, total } => (
            tw("Downloading version {v}… {p}%", &[("v", version), ("p", &percent(*done, *total))]),
            false,
        ),
        Status::Installing { version } => (tw("Installing version {v}…", &[("v", version)]), false),
        Status::Ready { version } => {
            (tw("Version {v} is ready: restart the game to use it.", &[("v", version)]), false)
        }
        Status::Available { version } => (
            tw("Version {v} is out: download it from the releases page.", &[("v", version)]),
            false,
        ),
        Status::Failed(f) => (f.message().into(), true),
    }
}

/// Starts the new version and returns true (the game should close now), or
/// false if it couldn't.
pub fn restart() -> bool {
    let pending = state().pending.clone();
    let Some(exe) = exe() else { return false };
    match pending {
        Some(Pending::Replaced) => {
            let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
            std::process::Command::new(&exe).args(args).spawn().is_ok()
        }
        Some(Pending::Installer(msi, sha256)) => {
            // Never run an installer that changed since it was checked.
            if sha256_file(&msi).as_deref() != Some(sha256.as_str()) {
                let _ = std::fs::remove_file(&msi);
                state().pending = None;
                return false;
            }
            run_installer(&msi, &exe)
        }
        None => false,
    }
}

/// Waits for the game to close, installs the new version with a progress
/// bar only, then starts the game again (also if the installation was
/// cancelled: the old version is still there).
#[cfg(windows)]
fn run_installer(msi: &Path, exe: &Path) -> bool {
    use std::os::windows::process::CommandExt;
    let quote = |s: &str| ps_quote(s);
    let args: Vec<String> = std::env::args().skip(1).map(|a| quote(&a)).collect::<Vec<_>>();
    let relaunch = if args.is_empty() {
        format!("Start-Process -FilePath {}", quote(&exe.to_string_lossy()))
    } else {
        format!(
            "Start-Process -FilePath {} -ArgumentList @({})",
            quote(&exe.to_string_lossy()),
            args.join(", ")
        )
    };
    let script = format!(
        "$p = {msi}; \
         Wait-Process -Id {pid} -Timeout 60 -ErrorAction SilentlyContinue; \
         try {{ \
           $r = Start-Process msiexec -ArgumentList @('/i', ('\"' + $p + '\"'), '/passive', '/norestart') -Wait -PassThru; \
           if ($r.ExitCode -eq 0 -or $r.ExitCode -eq 3010) {{ Remove-Item -LiteralPath $p -ErrorAction SilentlyContinue }} \
         }} catch {{ }}; \
         {relaunch}",
        msi = quote(&msi.to_string_lossy()),
        pid = std::process::id(),
    );
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .is_ok()
}

/// `s` as a PowerShell string in single quotes. PowerShell takes the
/// typographic quotes ‘ ’ ‚ ‛ for single quotes too: each is doubled like
/// the plain one (as PowerShell's own escaping does), so a folder named
/// "O’Brien" stays a name.
#[cfg_attr(not(windows), allow(dead_code))]
fn ps_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}') {
            out.push(c);
        }
        out.push(c);
    }
    out.push('\'');
    out
}

#[cfg(not(windows))]
fn run_installer(_msi: &Path, _exe: &Path) -> bool {
    false
}

// ---------------------------------------------------------------------------
// The work, on the background thread.

#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    UpToDate,
    Available(String),
    Ready(String, Pending),
}

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent(concat!("snook-legend/", env!("CARGO_PKG_VERSION")))
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        // A stalled download gives up; a slow one goes on.
        .timeout_per_read(Some(Duration::from_secs(60)))
        .build()
        .into()
}

fn get_text(agent: &ureq::Agent, url: &str, limit: u64) -> Result<String, Failure> {
    let mut resp = agent
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|_| Failure::Network)?;
    resp.body_mut().with_config().limit(limit).read_to_string().map_err(|_| Failure::Network)
}

/// Checks for a newer release and installs it in place of `exe`, through
/// the folder `downloads`. `report` hears the progress.
fn run(
    api: &str,
    how: How,
    exe: Option<PathBuf>,
    downloads: Option<PathBuf>,
    report: &dyn Fn(Status),
) -> Result<Outcome, Failure> {
    let agent = agent();
    let text = get_text(&agent, api, 4 << 20)?;
    let release: Release = serde_json::from_str(&text).map_err(|_| Failure::Network)?;
    let version = release.tag_name.trim().trim_start_matches('v').to_string();
    if release.draft || release.prerelease || !newer(&version, VERSION) {
        return Ok(Outcome::UpToDate);
    }
    let wanted = match how {
        How::Installer => format!("snook-legend-{TARGET}.msi"),
        How::Replace => {
            format!("snook-legend-{TARGET}.{}", if cfg!(windows) { "zip" } else { "tar.xz" })
        }
        How::Store | How::Page => return Ok(Outcome::Available(version)),
    };
    let Some(asset) = release.assets.iter().find(|a| a.name == wanted) else {
        return Ok(Outcome::Available(version));
    };
    // Never install what can't be checked.
    let Some(expected) = published_checksum(&agent, &release, &wanted)? else {
        return Ok(Outcome::Available(version));
    };
    let Some(dir) = downloads else {
        return Ok(Outcome::Available(version));
    };
    if std::fs::create_dir_all(&dir).is_err() {
        return Ok(Outcome::Available(version));
    }
    // Earlier downloads of other versions go.
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name != wanted && name != format!("{wanted}.part") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    let file = dir.join(&wanted);
    // Downloaded in an earlier session already?
    if sha256_file(&file).as_deref() != Some(expected.as_str()) {
        let progress = |done: u64, total: u64| {
            report(Status::Downloading { version: version.clone(), done, total })
        };
        download(&agent, &asset.browser_download_url, &file, asset.size, &expected, &progress)?;
    }
    report(Status::Installing { version: version.clone() });
    match how {
        How::Installer => Ok(Outcome::Ready(version, Pending::Installer(file, expected))),
        _ => {
            let Some(exe) = exe else { return Ok(Outcome::Available(version)) };
            let new = extract(&file)?;
            let result = replace_exe(&exe, &new, &version);
            let _ = std::fs::remove_file(&file);
            match result {
                Ok(()) => Ok(Outcome::Ready(version, Pending::Replaced)),
                Err(Replace::NotWritable) => Ok(Outcome::Available(version)),
                Err(Replace::Failed) => Err(Failure::Install),
            }
        }
    }
}

/// The SHA-256 published for `name`: its own `<name>.sha256`, or its line
/// in `sha256.sum`.
fn published_checksum(
    agent: &ureq::Agent,
    release: &Release,
    name: &str,
) -> Result<Option<String>, Failure> {
    for file in [format!("{name}.sha256"), "sha256.sum".to_string()] {
        if let Some(a) = release.assets.iter().find(|a| a.name == file) {
            let text = get_text(agent, &a.browser_download_url, 1 << 20)?;
            if let Some(sum) = parse_checksum(&text, name) {
                return Ok(Some(sum));
            }
        }
    }
    Ok(None)
}

/// Reads `sha256sum` output: `<hex>  <name>` (or `<hex> *<name>`) per line;
/// a line with only the hash counts for any name.
fn parse_checksum(text: &str, name: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let hash = parts.next()?.to_ascii_lowercase();
        let ok = hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit());
        let file = parts.next().map(|f| f.trim_start_matches('*').trim_start_matches("./"));
        let file = file.map(|f| f.rsplit(['/', '\\']).next().unwrap_or(f));
        (ok && file.is_none_or(|f| f == name)).then_some(hash)
    })
}

fn sha256_file(path: &Path) -> Option<String> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 << 10];
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(hex(&hasher.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Downloads `url` to `dest` through `dest.part`, going on from what an
/// earlier attempt left when the server allows it, and keeps it only if it
/// matches `expected`.
fn download(
    agent: &ureq::Agent,
    url: &str,
    dest: &Path,
    size: u64,
    expected: &str,
    progress: &dyn Fn(u64, u64),
) -> Result<(), Failure> {
    let part = dest
        .with_file_name(format!("{}.part", dest.file_name().unwrap_or_default().to_string_lossy()));
    for attempt in 0..2 {
        let have = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
        let resume = attempt == 0 && have > 0 && (size == 0 || have < size);
        let mut req = agent.get(url);
        if resume {
            req = req.header("Range", format!("bytes={have}-"));
        }
        let mut resp = match req.call() {
            Ok(r) => r,
            // 416: what is there is no part of this file.
            Err(ureq::Error::StatusCode(416)) if resume => {
                let _ = std::fs::remove_file(&part);
                continue;
            }
            Err(_) => return Err(Failure::Network),
        };
        let partial = resume && resp.status().as_u16() == 206;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(partial)
            .truncate(!partial)
            .open(&part)
            .map_err(|_| Failure::Install)?;
        let mut done = if partial { have } else { 0 };
        let total = resp.body().content_length().map(|n| n + done).unwrap_or(size);
        if total > MAX_DOWNLOAD {
            return Err(Failure::Damaged);
        }
        let mut reader = resp.body_mut().with_config().limit(MAX_DOWNLOAD).reader();
        let mut buf = vec![0u8; 64 << 10];
        let mut shown = Instant::now();
        progress(done, total);
        loop {
            let n = reader.read(&mut buf).map_err(|_| Failure::Network)?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n]).map_err(|_| Failure::Install)?;
            done += n as u64;
            if shown.elapsed() > Duration::from_millis(100) {
                shown = Instant::now();
                progress(done, total);
            }
        }
        file.sync_all().map_err(|_| Failure::Install)?;
        drop(file);
        progress(done, total);
        if sha256_file(&part).as_deref() == Some(expected) {
            return std::fs::rename(&part, dest).map_err(|_| Failure::Install);
        }
        // A bad file: once more from the start if this went on from an
        // earlier piece (it may belong to another file), else give up.
        let _ = std::fs::remove_file(&part);
        if !partial {
            return Err(Failure::Damaged);
        }
    }
    Err(Failure::Damaged)
}

/// The executable inside a release archive (the shallowest file with its
/// name).
#[cfg(not(windows))]
fn extract(archive: &Path) -> Result<Vec<u8>, Failure> {
    let file = std::fs::File::open(archive).map_err(|_| Failure::Damaged)?;
    let xz = lzma_rust2::XzReader::new(BufReader::new(file), true);
    let mut tar = tar::Archive::new(xz);
    let mut best: Option<(usize, Vec<u8>)> = None;
    for entry in tar.entries().map_err(|_| Failure::Damaged)? {
        let mut entry = entry.map_err(|_| Failure::Damaged)?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path().map_err(|_| Failure::Damaged)?.into_owned();
        if path.file_name().and_then(|n| n.to_str()) != Some(EXE_NAME) {
            continue;
        }
        let depth = path.components().count();
        if best.as_ref().is_some_and(|(d, _)| *d <= depth) {
            continue;
        }
        let mut bytes = Vec::new();
        entry.by_ref().take(MAX_DOWNLOAD).read_to_end(&mut bytes).map_err(|_| Failure::Damaged)?;
        best = Some((depth, bytes));
    }
    best.map(|(_, b)| b).ok_or(Failure::Damaged)
}

#[cfg(windows)]
fn extract(archive: &Path) -> Result<Vec<u8>, Failure> {
    let file = std::fs::File::open(archive).map_err(|_| Failure::Damaged)?;
    let mut zip = zip::ZipArchive::new(BufReader::new(file)).map_err(|_| Failure::Damaged)?;
    let mut best: Option<(usize, usize)> = None;
    for i in 0..zip.len() {
        let entry = zip.by_index(i).map_err(|_| Failure::Damaged)?;
        let Some(path) = entry.enclosed_name() else { continue };
        if !entry.is_file() || path.file_name().and_then(|n| n.to_str()) != Some(EXE_NAME) {
            continue;
        }
        let depth = path.components().count();
        if best.is_none_or(|(d, _)| depth < d) {
            best = Some((depth, i));
        }
    }
    let (_, index) = best.ok_or(Failure::Damaged)?;
    let entry = zip.by_index(index).map_err(|_| Failure::Damaged)?;
    let mut bytes = Vec::new();
    entry.take(MAX_DOWNLOAD).read_to_end(&mut bytes).map_err(|_| Failure::Damaged)?;
    Ok(bytes)
}

#[derive(Debug, PartialEq, Eq)]
enum Replace {
    /// The game's folder can't be written: the releases page instead.
    NotWritable,
    /// Something went wrong; the old executable is in place.
    Failed,
}

/// Puts `new` in place of the executable `exe`, after making sure it runs
/// and is `version`, keeping the old one as a backup next to it.
fn replace_exe(exe: &Path, new: &[u8], version: &str) -> Result<(), Replace> {
    let dir = exe.parent().ok_or(Replace::Failed)?;
    // A build on a developer's computer is never overwritten.
    if cargo_target_dir(dir) {
        return Err(Replace::NotWritable);
    }
    let (stem, ext) = match exe.extension() {
        Some(e) => (
            exe.file_stem().unwrap_or_default().to_string_lossy().into_owned(),
            format!(".{}", e.to_string_lossy()),
        ),
        None => (exe.file_name().unwrap_or_default().to_string_lossy().into_owned(), String::new()),
    };
    let fresh = dir.join(format!("{stem}.new{ext}"));
    let backup = dir.join(format!("{stem}.old{ext}"));
    let _ = std::fs::remove_file(&fresh);
    if let Err(e) = write_executable(&fresh, new, exe) {
        let _ = std::fs::remove_file(&fresh);
        return Err(match e.kind() {
            std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::ReadOnlyFilesystem => {
                Replace::NotWritable
            }
            _ => Replace::Failed,
        });
    }
    if !answers_with(&fresh, version) {
        let _ = std::fs::remove_file(&fresh);
        return Err(Replace::Failed);
    }
    let result = swap(exe, &fresh, &backup);
    if result.is_err() {
        let _ = std::fs::remove_file(&fresh);
    }
    result
}

/// Writes the new executable with the old one's permissions.
fn write_executable(path: &Path, bytes: &[u8], like: &Path) -> std::io::Result<()> {
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(path)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    drop(f);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(like).map(|m| m.permissions().mode()).unwrap_or(0o755);
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode | 0o100))?;
    }
    #[cfg(not(unix))]
    let _ = like;
    Ok(())
}

/// Runs `exe --version` and checks it names `version` (within a few seconds).
fn answers_with(exe: &Path, version: &str) -> bool {
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("--version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let Ok(mut child) = cmd.spawn() else { return false };
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut out = String::new();
                if let Some(mut s) = child.stdout.take() {
                    let _ = s.read_to_string(&mut out);
                }
                return status.success()
                    && out.split_whitespace().any(|w| w.trim_start_matches('v') == version);
            }
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

/// The new file takes the executable's place in one rename; the old one is
/// kept as `backup` (a second name for the same file).
#[cfg(not(windows))]
fn swap(exe: &Path, fresh: &Path, backup: &Path) -> Result<(), Replace> {
    let _ = std::fs::remove_file(backup);
    if std::fs::hard_link(exe, backup).is_err() {
        std::fs::copy(exe, backup).map_err(|_| Replace::Failed)?;
    }
    std::fs::rename(fresh, exe).map_err(|_| Replace::Failed)
}

/// Windows can't replace a running executable, but can rename it: the old
/// one moves aside (it is the backup), the new one takes its name, and the
/// old one comes back if that fails.
#[cfg(windows)]
fn swap(exe: &Path, fresh: &Path, backup: &Path) -> Result<(), Replace> {
    let _ = std::fs::remove_file(backup);
    std::fs::rename(exe, backup).map_err(|_| Replace::Failed)?;
    if std::fs::rename(fresh, exe).is_err() {
        let _ = std::fs::rename(backup, exe);
        return Err(Replace::Failed);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Versions.

/// `a` is a later version than `b` (semantic versioning; a leading `v`
/// is ignored; anything unreadable is never newer).
pub fn newer(a: &str, b: &str) -> bool {
    match (parse_version(a), parse_version(b)) {
        (Some(a), Some(b)) => compare(&a, &b) == std::cmp::Ordering::Greater,
        _ => false,
    }
}

type Version<'a> = ([u64; 3], Vec<&'a str>);

fn parse_version(s: &str) -> Option<Version<'_>> {
    let s = s.trim().trim_start_matches('v');
    let s = s.split('+').next()?;
    let (core, pre) = match s.split_once('-') {
        Some((c, p)) => (c, p.split('.').collect()),
        None => (s, Vec::new()),
    };
    let mut nums = core.split('.').map(|n| n.parse::<u64>().ok());
    let v = [nums.next()??, nums.next()??, nums.next()??];
    if nums.next().is_some() {
        return None;
    }
    Some((v, pre))
}

fn compare(a: &Version, b: &Version) -> std::cmp::Ordering {
    use std::cmp::Ordering::*;
    a.0.cmp(&b.0).then_with(|| match (a.1.is_empty(), b.1.is_empty()) {
        (true, true) => Equal,
        // A release comes after its pre-releases.
        (true, false) => Greater,
        (false, true) => Less,
        (false, false) => {
            for (x, y) in a.1.iter().zip(&b.1) {
                let o = match (x.parse::<u64>(), y.parse::<u64>()) {
                    (Ok(x), Ok(y)) => x.cmp(&y),
                    (Ok(_), Err(_)) => Less,
                    (Err(_), Ok(_)) => Greater,
                    _ => x.cmp(y),
                };
                if o != Equal {
                    return o;
                }
            }
            a.1.len().cmp(&b.1.len())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every kind of single quote PowerShell knows is doubled.
    #[test]
    fn powershell_quotes() {
        assert_eq!(ps_quote(r"C:\Users\Ana\game.exe"), r"'C:\Users\Ana\game.exe'");
        assert_eq!(ps_quote("O'Brien"), "'O''Brien'");
        assert_eq!(ps_quote("O’Brien"), "'O’’Brien'");
        assert_eq!(ps_quote("‘‚‛"), "'‘‘‚‚‛‛'");
    }

    #[test]
    fn links_and_targets() {
        assert!(RELEASES.starts_with("https://github.com/"));
        assert!(RELEASES.ends_with("/releases/latest"));
        assert_eq!(
            RELEASES.replace("https://github.com/", "https://api.github.com/repos/"),
            "https://api.github.com/repos/allansomensi/snook-legend/releases/latest"
        );
        // The platforms with archives are the ones dist builds.
        let dist = include_str!("../../../dist-workspace.toml");
        let line = dist.lines().find(|l| l.starts_with("targets = ")).unwrap();
        let mut listed: Vec<&str> = line.split('"').skip(1).step_by(2).collect();
        listed.sort();
        let mut ours = RELEASED_TARGETS.to_vec();
        ours.sort();
        assert_eq!(listed, ours, "RELEASED_TARGETS must follow dist-workspace.toml");
        assert!(!TARGET.is_empty());
        if RELEASED_TARGETS.contains(&TARGET) && !crate::channel::STORE {
            assert_eq!(how(), How::Replace);
        }
        // Debug builds never update themselves; release ones (`cargo test
        // --release`, as in CI) are kept safe by where they run from: the
        // test runs from Cargo's target folder, never replaced.
        if cfg!(debug_assertions) {
            assert!(development_build());
        }
        assert!(in_cargo_target() && !allowed());
        let me = std::env::current_exe().unwrap();
        assert_eq!(replace_exe(&me, b"x", "9.9.9"), Err(Replace::NotWritable));
        assert!(!cargo_target_dir(&std::env::temp_dir()));
    }

    #[test]
    fn versions() {
        assert!(newer("0.2.0", "0.1.4"));
        assert!(newer("v0.10.0", "0.9.9"));
        assert!(newer("1.0.0", "1.0.0-beta.2"));
        assert!(newer("1.0.0-beta.10", "1.0.0-beta.2"));
        assert!(newer("1.0.0-beta", "1.0.0-alpha.5"));
        assert!(newer("1.0.0-beta.1", "1.0.0-beta"));
        assert!(!newer("0.1.4", "0.1.4"));
        assert!(!newer("0.1.3", "0.1.4"));
        assert!(!newer("garbage", "0.1.4"));
        assert!(!newer("1.2", "0.1.4"));
        assert!(!newer("0.2.0", "garbage"));
        assert!(newer("0.2.0+build.5", "0.1.9"));
    }

    #[test]
    fn checksums() {
        let h = "a".repeat(64);
        assert_eq!(
            parse_checksum(&format!("{h}  snook-legend.tar.xz\n"), "snook-legend.tar.xz"),
            Some(h.clone())
        );
        assert_eq!(
            parse_checksum(&format!("{h} *snook-legend.tar.xz"), "snook-legend.tar.xz"),
            Some(h.clone())
        );
        assert_eq!(parse_checksum(&h.to_uppercase(), "x"), Some(h.clone()));
        let sum = format!("{}  other.zip\n{h}  ./dir/snook-legend.tar.xz\n", "b".repeat(64));
        assert_eq!(parse_checksum(&sum, "snook-legend.tar.xz"), Some(h.clone()));
        assert_eq!(parse_checksum(&sum, "missing.msi"), None);
        assert_eq!(parse_checksum("abc  snook-legend.tar.xz", "snook-legend.tar.xz"), None);
        assert_eq!(parse_checksum(&"z".repeat(64), "x"), None);
    }

    #[test]
    fn describes_every_status() {
        let all = [
            Status::Checking,
            Status::UpToDate,
            Status::Downloading { version: "0.2.0".into(), done: 5, total: 10 },
            Status::Installing { version: "0.2.0".into() },
            Status::Ready { version: "0.2.0".into() },
            Status::Available { version: "0.2.0".into() },
            Status::Failed(Failure::Network),
        ];
        for s in all {
            assert!(!describe(&s).0.is_empty());
        }
        assert_eq!(percent(5, 10), 50);
        assert_eq!(percent(5, 0), 0);
        assert_eq!(percent(50, 10), 100);
    }

    /// A folder of its own under the system's temporary folder (for the
    /// tests of replacing and unpacking, which run on Unix).
    #[cfg(unix)]
    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("snook-legend-update-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A script that answers `--version` like the game.
    #[cfg(unix)]
    fn fake_game(version: &str) -> Vec<u8> {
        format!("#!/bin/sh\nif [ \"$1\" = --version ]; then echo \"snook-legend {version}\"; fi\n")
            .into_bytes()
    }

    #[cfg(unix)]
    #[test]
    fn replaces_the_executable_and_keeps_a_backup() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("replace");
        let exe = dir.join("snook-legend");
        std::fs::write(&exe, fake_game("0.1.0")).unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        // A file that doesn't answer with the announced version is refused.
        assert_eq!(replace_exe(&exe, &fake_game("0.1.5"), "0.2.0"), Err(Replace::Failed));
        assert_eq!(std::fs::read(&exe).unwrap(), fake_game("0.1.0"));
        assert!(!dir.join("snook-legend.new").exists());
        // The right one goes in; the old one is kept.
        replace_exe(&exe, &fake_game("0.2.0"), "0.2.0").unwrap();
        assert_eq!(std::fs::read(&exe).unwrap(), fake_game("0.2.0"));
        assert_eq!(std::fs::read(dir.join("snook-legend.old")).unwrap(), fake_game("0.1.0"));
        assert!(answers_with(&exe, "0.2.0"));
        let mode = std::fs::metadata(&exe).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
        // A folder that can't be written (unless the tests run as root).
        let locked = dir.join("locked");
        std::fs::create_dir(&locked).unwrap();
        let exe2 = locked.join("snook-legend");
        std::fs::write(&exe2, fake_game("0.1.0")).unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();
        let can_write = std::fs::write(locked.join("probe"), b"").is_ok();
        if !can_write {
            assert_eq!(replace_exe(&exe2, &fake_game("0.2.0"), "0.2.0"), Err(Replace::NotWritable));
        }
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A release archive like dist's: a folder with the game and its notes.
    #[cfg(unix)]
    fn tar_xz(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut tar = tar::Builder::new(Vec::new());
        for (name, bytes) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            tar.append_data(&mut header, name, *bytes).unwrap();
        }
        let tar = tar.into_inner().unwrap();
        let mut xz =
            lzma_rust2::XzWriter::new(Vec::new(), lzma_rust2::XzOptions::with_preset(6)).unwrap();
        xz.write_all(&tar).unwrap();
        xz.finish().unwrap()
    }

    #[cfg(unix)]
    #[test]
    fn takes_the_game_out_of_the_archive() {
        let dir = temp_dir("extract");
        let archive = dir.join("snook-legend-x86_64-unknown-linux-gnu.tar.xz");
        std::fs::write(
            &archive,
            tar_xz(&[
                ("snook-legend-x86_64-unknown-linux-gnu/README.md", b"readme"),
                ("snook-legend-x86_64-unknown-linux-gnu/extra/snook-legend", b"deeper"),
                ("snook-legend-x86_64-unknown-linux-gnu/snook-legend", b"the game"),
            ]),
        )
        .unwrap();
        assert_eq!(extract(&archive).unwrap(), b"the game");
        // Without the game, or not an archive at all.
        std::fs::write(&archive, tar_xz(&[("x/README.md", b"readme")])).unwrap();
        assert_eq!(extract(&archive), Err(Failure::Damaged));
        std::fs::write(&archive, b"not xz").unwrap();
        assert_eq!(extract(&archive), Err(Failure::Damaged));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The whole thing against a local server: a release, its checksum, a
    /// download that fails its check, then one that installs.
    #[cfg(unix)]
    #[test]
    fn updates_from_a_local_release() {
        use std::net::TcpListener;
        let dir = temp_dir("release");
        let exe = dir.join("game").join("snook-legend");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, fake_game(VERSION)).unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let archive_name = format!("snook-legend-{TARGET}.tar.xz");
        let archive =
            tar_xz(&[(&format!("snook-legend-{TARGET}/snook-legend"), &fake_game("9.0.0"))]);
        let good = hex(&Sha256::digest(&archive));

        // A tiny HTTP server: path → body.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let files =
            std::sync::Arc::new(Mutex::new(std::collections::HashMap::<String, Vec<u8>>::new()));
        let served = files.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut stream = stream;
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
                let body = served.lock().unwrap().get(&path).cloned();
                let _ = match body {
                    Some(b) => {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            b.len()
                        );
                        stream.write_all(&b)
                    }
                    None => stream.write_all(
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    ),
                };
            }
        });
        let release = |tag: &str| {
            format!(
                r#"{{"tag_name":"{tag}","assets":[
                {{"name":"{archive_name}","browser_download_url":"{base}/a","size":{}}},
                {{"name":"{archive_name}.sha256","browser_download_url":"{base}/a.sha256"}}]}}"#,
                archive.len()
            )
        };
        let set = |path: &str, body: Vec<u8>| {
            files.lock().unwrap().insert(path.to_string(), body);
        };
        set("/a", archive.clone());
        let api = format!("{base}/latest");

        let downloads = dir.join("updates");
        let file = downloads.join(&archive_name);
        let go = |api: &str, report: &dyn Fn(Status)| {
            run(api, How::Replace, Some(exe.clone()), Some(downloads.clone()), report)
        };
        let quiet = |_: Status| {};

        // Up to date: nothing happens.
        set("/latest", release(&format!("v{VERSION}")).into_bytes());
        assert_eq!(go(&api, &quiet), Ok(Outcome::UpToDate));
        // Pre-releases are not offered.
        set(
            "/latest",
            release("v9.0.0").replace(r#""assets""#, r#""prerelease":true,"assets""#).into_bytes(),
        );
        assert_eq!(go(&api, &quiet), Ok(Outcome::UpToDate));

        // Can't be reached.
        assert_eq!(go(&format!("{base}/nothing"), &quiet), Err(Failure::Network));
        assert_eq!(go("http://127.0.0.1:9/latest", &quiet), Err(Failure::Network));

        // A wrong checksum: refused, the game untouched.
        set("/latest", release("v9.0.0").into_bytes());
        set("/a.sha256", format!("{}  {archive_name}\n", "0".repeat(64)).into_bytes());
        assert_eq!(go(&api, &quiet), Err(Failure::Damaged));
        assert!(!file.exists() && !file.with_file_name(format!("{archive_name}.part")).exists());
        assert!(answers_with(&exe, VERSION));

        // No checksum published: never installed, the page instead.
        set(
            "/latest",
            release("v9.0.0").replace(&format!("{archive_name}.sha256"), "notes.txt").into_bytes(),
        );
        assert_eq!(go(&api, &quiet), Ok(Outcome::Available("9.0.0".into())));
        // Published but out of reach: try again later.
        set("/latest", release("v9.0.0").into_bytes());
        files.lock().unwrap().remove("/a.sha256");
        assert_eq!(go(&api, &quiet), Err(Failure::Network));

        // No archive for this platform: the page too.
        set("/latest", release("v9.0.0").replace(&archive_name, "other.tar.xz").into_bytes());
        assert_eq!(go(&api, &quiet), Ok(Outcome::Available("9.0.0".into())));

        // The right checksum (in a sha256.sum): downloaded with progress,
        // checked, and the executable swapped for the one in the archive.
        set(
            "/latest",
            release("v9.0.0").replace(&format!("{archive_name}.sha256"), "sha256.sum").into_bytes(),
        );
        set(
            "/a.sha256",
            format!("{}  other.zip\n{good}  {archive_name}\n", "1".repeat(64)).into_bytes(),
        );
        // A piece left by an interrupted download (this server can't go
        // on from it: the file is fetched again).
        std::fs::create_dir_all(&downloads).unwrap();
        std::fs::write(file.with_file_name(format!("{archive_name}.part")), b"junk").unwrap();
        let seen = Mutex::new(Vec::new());
        let progress = |s: Status| seen.lock().unwrap().push(s);
        assert_eq!(go(&api, &progress), Ok(Outcome::Ready("9.0.0".into(), Pending::Replaced)));
        assert!(seen.lock().unwrap().iter().any(|s| matches!(s,
            Status::Downloading { done, total, .. } if *done == archive.len() as u64 && *total == *done)));
        assert!(answers_with(&exe, "9.0.0"));
        assert!(answers_with(&exe.with_file_name("snook-legend.old"), VERSION));
        // The archive is not kept.
        assert_eq!(std::fs::read_dir(&downloads).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
