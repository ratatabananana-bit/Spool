use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;

use crate::dlog;

// CP1252 → Unicode map for the 0x80..=0x9F range (rest of high-bytes are Latin-1).
// yt-dlp.exe on Windows ignores PYTHONUTF8 when stdout is piped (PyInstaller frozen),
// so non-ASCII titles arrive as CP1252 bytes. We decode UTF-8 strict first; on failure
// fall back to a CP1252 reading so e.g. "ÉZI" round-trips instead of becoming "�ZI".
const CP1252_HIGH: [char; 32] = [
    '\u{20AC}', '\u{FFFD}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{FFFD}', '\u{017D}', '\u{FFFD}',
    '\u{FFFD}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}', '\u{0153}', '\u{FFFD}', '\u{017E}', '\u{0178}',
];

fn decode_bytes(b: &[u8]) -> String {
    match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        Err(_) => b
            .iter()
            .map(|&byte| match byte {
                0..=0x7F => char::from(byte),
                0x80..=0x9F => CP1252_HIGH[(byte - 0x80) as usize],
                _ => char::from(byte),
            })
            .collect(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub url: String,
    pub title: String,
    pub host: String,
    pub format: String,
    pub output_dir: String,
    pub filename_template: String,
    pub state: String, // "queued" | "running" | "paused" | "completed" | "failed"
    pub pct: f32,
    pub speed: String,
    pub eta: String,
    pub size: String,
    pub error_msg: Option<String>,
    #[serde(default)]
    pub rtl: bool,
    #[serde(default)]
    pub created_ms: u128,
    #[serde(default)]
    pub video_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobProgress {
    pub id: String,
    pub pct: f32,
    pub speed: String,
    pub eta: String,
    pub size: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobStateChange {
    pub id: String,
    pub state: String,
    pub error_msg: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobMeta {
    pub id: String,
    pub title: Option<String>,
    pub host: Option<String>,
    pub rtl: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobLogLine {
    pub id: String,
    pub kind: String, // "stdout" | "stderr"
    pub line: String,
}

pub struct JobsState {
    pub queue: Mutex<VecDeque<String>>,
    pub active: Mutex<HashMap<String, CommandChild>>,
    pub records: Mutex<HashMap<String, Job>>,
    pub concurrency: AtomicU32,
}

impl JobsState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            queue: Mutex::new(VecDeque::new()),
            active: Mutex::new(HashMap::new()),
            records: Mutex::new(HashMap::new()),
            concurrency: AtomicU32::new(3),
        })
    }

    pub fn set_concurrency(&self, n: u32) {
        self.concurrency.store(n.max(1), Ordering::SeqCst);
    }

    pub fn snapshot_all(&self) -> Vec<Job> {
        let r = self.records.lock();
        let mut v: Vec<Job> = r.values().cloned().collect();
        v.sort_by_key(|j| j.created_ms);
        v
    }
}

fn next_id() -> String {
    static N: AtomicU64 = AtomicU64::new(1);
    format!("j{}", N.fetch_add(1, Ordering::Relaxed))
}

fn now_ms() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

pub fn format_to_args(fmt: &str) -> Vec<String> {
    let v = |sel: &str| vec!["-f".into(), sel.into()];
    // Audio extraction with per-codec max quality. MP3 goes 320K CBR (its
    // true ceiling); other codecs use VBR-best ("0") which is already
    // perceptually transparent or lossless.
    let audio_q = |ext: &str, quality: Option<&str>| {
        let mut a: Vec<String> = vec![
            "-x".into(),
            "--audio-format".into(),
            ext.into(),
            "-f".into(),
            "ba/b".into(),
        ];
        if let Some(q) = quality {
            a.push("--audio-quality".into());
            a.push(q.into());
        }
        a
    };
    let merge_to = |target: &str, sel: &str| {
        vec![
            "-f".into(),
            sel.into(),
            "--merge-output-format".into(),
            target.into(),
        ]
    };
    match fmt {
        "Best" => v("bv*+ba/b"),
        "2160p" => v("bv*[height<=2160]+ba/b[height<=2160]"),
        "1440p" => v("bv*[height<=1440]+ba/b[height<=1440]"),
        "1080p" => v("bv*[height<=1080]+ba/b[height<=1080]"),
        "720p" => v("bv*[height<=720]+ba/b[height<=720]"),
        "480p" => v("bv*[height<=480]+ba/b[height<=480]"),
        "2160p MP4" => merge_to(
            "mp4",
            "bv*[height<=2160][ext=mp4]+ba[ext=m4a]/bv*[height<=2160]+ba/b[height<=2160]",
        ),
        "2160p WebM" => merge_to(
            "webm",
            "bv*[height<=2160][ext=webm]+ba[ext=webm]/bv*[height<=2160]+ba/b[height<=2160]",
        ),
        "1440p MP4" => merge_to(
            "mp4",
            "bv*[height<=1440][ext=mp4]+ba[ext=m4a]/bv*[height<=1440]+ba/b[height<=1440]",
        ),
        "1440p WebM" => merge_to(
            "webm",
            "bv*[height<=1440][ext=webm]+ba[ext=webm]/bv*[height<=1440]+ba/b[height<=1440]",
        ),
        "1080p MP4" => merge_to(
            "mp4",
            "bv*[height<=1080][ext=mp4]+ba[ext=m4a]/bv*[height<=1080]+ba/b[height<=1080]",
        ),
        "720p MP4" => merge_to(
            "mp4",
            "bv*[height<=720][ext=mp4]+ba[ext=m4a]/bv*[height<=720]+ba/b[height<=720]",
        ),
        "1080p WebM" => merge_to(
            "webm",
            "bv*[height<=1080][ext=webm]+ba[ext=webm]/bv*[height<=1080]+ba/b[height<=1080]",
        ),
        "720p WebM" => merge_to(
            "webm",
            "bv*[height<=720][ext=webm]+ba[ext=webm]/bv*[height<=720]+ba/b[height<=720]",
        ),
        "Audio MP3" => audio_q("mp3", Some("320K")),
        "Audio M4A" => audio_q("m4a", Some("0")),
        "Audio Opus" => audio_q("opus", Some("0")),
        "Audio WAV" => audio_q("wav", None),
        "Audio FLAC" => audio_q("flac", None),
        "Audio" => audio_q("mp3", Some("320K")),
        _ => v("bv*[height<=1080]+ba/b[height<=1080]"),
    }
}

fn ffmpeg_path(_app: &AppHandle) -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.to_path_buf();
    let p = dir.join("ffmpeg.exe");
    if p.exists() {
        Some(p.to_string_lossy().into_owned())
    } else {
        None
    }
}

fn detect_rtl(s: &str) -> bool {
    s.chars().any(|c| {
        let n = c as u32;
        (0x0590..=0x05FF).contains(&n)
            || (0x0600..=0x06FF).contains(&n)
            || (0x0750..=0x077F).contains(&n)
            || (0xFB50..=0xFDFF).contains(&n)
    })
}

fn host_of(url: &str) -> String {
    if let Some(rest) = url.split("://").nth(1) {
        let host_path: String = rest.chars().take_while(|c| *c != '/').collect();
        let path: String = rest.chars().skip_while(|c| *c != '/').collect();
        if path.is_empty() {
            host_path
        } else {
            let p = if path.len() > 60 { &path[..60] } else { &path };
            format!("{host_path}{}", p)
        }
    } else {
        url.to_string()
    }
}

fn human_bytes(b: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let f = b as f64;
    if f >= GB {
        format!("{:.2} GB", f / GB)
    } else if f >= MB {
        format!("{:.1} MB", f / MB)
    } else if f >= KB {
        format!("{:.0} KB", f / KB)
    } else {
        format!("{b} B")
    }
}

fn human_speed(b_per_s: f64) -> String {
    if b_per_s <= 0.0 {
        return String::new();
    }
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    if b_per_s >= MB {
        format!("{:.1} MB/s", b_per_s / MB)
    } else if b_per_s >= KB {
        format!("{:.0} KB/s", b_per_s / KB)
    } else {
        format!("{:.0} B/s", b_per_s)
    }
}

fn human_eta(secs: f64) -> String {
    if secs <= 0.0 || !secs.is_finite() {
        return String::new();
    }
    let s = secs as u64;
    let m = s / 60;
    let r = s % 60;
    if m >= 60 {
        let h = m / 60;
        let mm = m % 60;
        format!("{h}:{mm:02}:{r:02}")
    } else {
        format!("{m}:{r:02}")
    }
}

pub fn enqueue_job(
    app: &AppHandle,
    url: String,
    format: String,
    output_dir: String,
    filename_template: String,
) -> Result<EnqueueOutcome, String> {
    // Dedupe: if an active job (queued or running) already targets this URL,
    // skip — avoids race on the same destination path when yt-dlp writes/merges.
    if let Some(state) = app.try_state::<Arc<JobsState>>() {
        let r = state.records.lock();
        for j in r.values() {
            if j.url == url && (j.state == "queued" || j.state == "running") {
                return Ok(EnqueueOutcome::Skipped {
                    url,
                    existing_id: j.id.clone(),
                });
            }
        }
    }

    let id = next_id();
    let host = host_of(&url);
    let job = Job {
        id: id.clone(),
        url: url.clone(),
        title: url.clone(),
        host: host.clone(),
        format: format.clone(),
        output_dir: output_dir.clone(),
        filename_template: filename_template.clone(),
        state: "queued".into(),
        pct: 0.0,
        speed: String::new(),
        eta: String::new(),
        size: String::new(),
        error_msg: None,
        rtl: false,
        created_ms: now_ms(),
        video_id: None,
    };
    if let Some(state) = app.try_state::<Arc<JobsState>>() {
        state.records.lock().insert(id.clone(), job.clone());
        state.queue.lock().push_back(id.clone());
    }
    let _ = app.emit("job_added", job);
    dispatch(app);
    Ok(EnqueueOutcome::Added { id })
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum EnqueueOutcome {
    Added { id: String },
    Skipped { url: String, existing_id: String },
}

pub fn dispatch(app: &AppHandle) {
    let state = match app.try_state::<Arc<JobsState>>() {
        Some(s) => s.inner().clone(),
        None => return,
    };
    let conc = state.concurrency.load(Ordering::SeqCst) as usize;

    loop {
        let active_count = state.active.lock().len();
        if active_count >= conc {
            break;
        }
        let next_id = match state.queue.lock().pop_front() {
            Some(id) => id,
            None => break,
        };
        if let Err(e) = spawn_running(app, &state, &next_id) {
            dlog!("[dispatch] spawn_running({}) failed: {}", next_id, e);
            mark_failed(app, &state, &next_id, Some(e));
        }
    }
}

fn spawn_running(
    app: &AppHandle,
    state: &Arc<JobsState>,
    id: &str,
) -> Result<(), String> {
    let job = match state.records.lock().get(id).cloned() {
        Some(j) => j,
        None => return Err("job record missing".into()),
    };

    let template = if job.filename_template.trim().is_empty() {
        "%(title)s [%(id)s].%(ext)s".to_string()
    } else {
        job.filename_template.clone()
    };
    let outpath = format!("{}\\{}", job.output_dir.trim_end_matches('\\'), template);

    // Pull yt-dlp option settings.
    let settings_snap = app
        .try_state::<Arc<parking_lot::Mutex<crate::settings::SettingsStore>>>()
        .map(|s| s.lock().get().clone());

    let mut args: Vec<String> = vec![];
    args.push("--no-mtime".into());
    args.push("--newline".into());
    args.push("--no-playlist".into());
    args.push("-o".into());
    args.push(outpath);
    args.extend(format_to_args(&job.format));

    if let Some(s) = settings_snap.as_ref() {
        if s.write_subtitles.unwrap_or(false) {
            args.push("--write-subs".into());
            args.push("--write-auto-subs".into());
            let langs = s
                .subtitle_langs
                .clone()
                .filter(|v| !v.trim().is_empty())
                .unwrap_or_else(|| "en.*".into());
            args.push("--sub-langs".into());
            args.push(langs);
            args.push("--convert-subs".into());
            args.push("srt".into());
        }
        if s.sponsor_block.unwrap_or(false) {
            args.push("--sponsorblock-remove".into());
            args.push("sponsor,selfpromo,interaction".into());
        }
        if s.embed_thumbnail.unwrap_or(false) {
            args.push("--embed-thumbnail".into());
        }
        if s.embed_metadata.unwrap_or(false) {
            args.push("--embed-metadata".into());
        }
        if s.embed_chapters.unwrap_or(false) {
            args.push("--embed-chapters".into());
        }
        if s.restrict_filenames.unwrap_or(false) {
            args.push("--restrict-filenames".into());
        }
        if s.write_thumbnail.unwrap_or(false) {
            args.push("--write-thumbnail".into());
        }
        if s.write_info_json.unwrap_or(false) {
            args.push("--write-info-json".into());
        }
        if s.write_description.unwrap_or(false) {
            args.push("--write-description".into());
        }
        if s.keep_video.unwrap_or(false) {
            args.push("--keep-video".into());
        }
        if let Some(b) = s.cookies_from_browser.as_ref().filter(|v| !v.trim().is_empty()) {
            args.push("--cookies-from-browser".into());
            args.push(b.clone());
        }
        if let Some(r) = s.rate_limit.as_ref().filter(|v| !v.trim().is_empty()) {
            args.push("-r".into());
            args.push(r.clone());
        }
        if let Some(n) = s.retries {
            args.push("--retries".into());
            args.push(n.to_string());
        }
        // Audio quality is now baked into each Audio preset via format_to_args,
        // so it doesn't need a global override here.
        if let Some(extra) = s.extra_args.as_ref().filter(|v| !v.trim().is_empty()) {
            for tok in extra.split_whitespace() {
                args.push(tok.to_string());
            }
        }
    }

    args.push("--progress-template".into());
    args.push(
        "download:PROG|%(progress.downloaded_bytes)s|%(progress.total_bytes)s|%(progress.total_bytes_estimate)s|%(progress.speed)s|%(progress.eta)s|%(progress.status)s"
            .into(),
    );
    args.push("--print".into());
    args.push("before_dl:META|%(id)s|%(title)s|%(uploader)s|%(filesize,filesize_approx)d".into());
    // Three different print hooks for the final on-disk path because yt-dlp's
    // post-processing pipeline behaves differently per format: after_move may
    // fire for intermediate files; after_video doesn't fire for every audio
    // extract. We capture all three (MOVE / FINAL / VIDEO) and pick the one
    // that actually points at a real file.
    args.push("--print".into());
    args.push("after_move:MOVE|%(filepath)s".into());
    args.push("--print".into());
    args.push("after_video:FINAL|%(filepath)s".into());
    args.push("--print".into());
    args.push("post_process:DEST|%(filepath)s".into());
    match ffmpeg_path(app) {
        Some(ff) => {
            dlog!("[job {}] ffmpeg path: {}", id, ff);
            args.push("--ffmpeg-location".into());
            args.push(ff);
        }
        None => {
            dlog!("[job {}] WARN ffmpeg not found next to exe", id);
        }
    }
    args.push(job.url.clone());

    dlog!("[job {}] format={} dir={}", id, job.format, job.output_dir);
    dlog!("[job {}] yt-dlp args: {:?}", id, args);

    let cmd = app
        .shell()
        .sidecar("yt-dlp")
        .map_err(|e| {
            dlog!("[job {}] sidecar resolve error: {}", id, e);
            format!("sidecar error: {e}")
        })?
        .args(args)
        // Force UTF-8 on yt-dlp's Python stdout/stderr — without this, Windows
        // wraps bytes in cp1252 and non-ASCII titles arrive as � replacement chars.
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONUTF8", "1");

    let (mut rx, child) = cmd.spawn().map_err(|e| {
        dlog!("[job {}] spawn error: {}", id, e);
        format!("spawn error: {e}")
    })?;

    dlog!("[job {}] spawned ok", id);
    state.active.lock().insert(id.to_string(), child);

    // Mark running
    if let Some(rec) = state.records.lock().get_mut(id) {
        rec.state = "running".into();
    }
    let _ = app.emit(
        "job_state",
        JobStateChange {
            id: id.to_string(),
            state: "running".into(),
            error_msg: None,
        },
    );

    let app_handle = app.clone();
    let state_clone = state.clone();
    let job_id = id.to_string();
    tauri::async_runtime::spawn(async move {
        let mut last_total: Option<u64> = None;
        let mut last_err: Option<String> = None;

        while let Some(evt) = rx.recv().await {
            match evt {
                CommandEvent::Stdout(b) => {
                    let text = decode_bytes(&b);
                    for line in text.split('\n') {
                        if line.is_empty() {
                            continue;
                        }
                        dlog!("[job {} stdout] {}", job_id, line.trim_end_matches('\r'));
                        process_line(
                            &app_handle,
                            &state_clone,
                            &job_id,
                            line,
                            &mut last_total,
                        );
                    }
                }
                CommandEvent::Stderr(b) => {
                    let text = decode_bytes(&b);
                    for line in text.split('\n') {
                        if line.is_empty() {
                            continue;
                        }
                        let l = line.trim_end_matches('\r').to_string();
                        dlog!("[job {} stderr] {}", job_id, l);
                        if l.starts_with("ERROR:") && last_err.is_none() {
                            last_err = Some(l.trim_start_matches("ERROR:").trim().to_string());
                        }
                        let _ = app_handle.emit(
                            "job_log",
                            JobLogLine {
                                id: job_id.clone(),
                                kind: "stderr".into(),
                                line: l,
                            },
                        );
                    }
                }
                CommandEvent::Terminated(payload) => {
                    let code = payload.code.unwrap_or(-1);
                    dlog!("[job {}] terminated code={}", job_id, code);
                    let final_state = if code == 0 { "completed" } else { "failed" };

                    // Filesystem scan for the actual output file. yt-dlp's stdout
                    // mangles non-ASCII filenames (CJK, full-width brackets) on
                    // Windows even with PYTHONUTF8 because the underlying writer
                    // re-encodes to the legacy code page. The file on DISK is
                    // correct though — we resolve it by matching `[<video_id>].*`
                    // in the output directory.
                    if final_state == "completed" {
                        let (vid, outdir) = {
                            let r = state_clone.records.lock();
                            r.get(&job_id)
                                .map(|j| (j.video_id.clone(), j.output_dir.clone()))
                                .unwrap_or((None, String::new()))
                        };
                        if let Some(id) = vid {
                            let needle = format!("[{}]", id);
                            if let Ok(entries) = std::fs::read_dir(&outdir) {
                                let mut best: Option<(std::path::PathBuf, u64, std::time::SystemTime)> = None;
                                for ent in entries.flatten() {
                                    let path = ent.path();
                                    let name = path
                                        .file_name()
                                        .and_then(|n| n.to_str())
                                        .unwrap_or("");
                                    if !name.contains(&needle) {
                                        continue;
                                    }
                                    if let Ok(meta) = ent.metadata() {
                                        if !meta.is_file() {
                                            continue;
                                        }
                                        let mtime = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
                                        let len = meta.len();
                                        if best.as_ref().map_or(true, |(_, _, t)| mtime > *t) {
                                            best = Some((path, len, mtime));
                                        }
                                    }
                                }
                                if let Some((p, len, _)) = best {
                                    let real_path = p.to_string_lossy().into_owned();
                                    let size = human_bytes(len);
                                    dlog!(
                                        "[job {}] resolved via FS scan: {} ({} bytes)",
                                        job_id,
                                        real_path,
                                        len
                                    );
                                    if let Some(rec) = state_clone.records.lock().get_mut(&job_id) {
                                        rec.size = size.clone();
                                    }
                                    let _ = app_handle.emit(
                                        "job_progress",
                                        JobProgress {
                                            id: job_id.clone(),
                                            pct: 100.0,
                                            speed: String::new(),
                                            eta: String::new(),
                                            size,
                                        },
                                    );
                                    let _ = app_handle.emit(
                                        "job_dest",
                                        serde_json::json!({ "id": &job_id, "path": real_path }),
                                    );
                                }
                            }
                        }
                    }

                    let final_err = if final_state == "failed" {
                        last_err
                            .clone()
                            .or(Some(format!("exit code {code}")))
                    } else {
                        None
                    };
                    if let Some(rec) = state_clone.records.lock().get_mut(&job_id) {
                        rec.state = final_state.into();
                        rec.error_msg = final_err.clone();
                        if final_state == "completed" {
                            rec.pct = 100.0;
                            rec.speed.clear();
                            rec.eta.clear();
                        }
                    }
                    state_clone.active.lock().remove(&job_id);

                    let _ = app_handle.emit(
                        "job_state",
                        JobStateChange {
                            id: job_id.clone(),
                            state: final_state.into(),
                            error_msg: final_err,
                        },
                    );
                    if final_state == "completed" {
                        let _ = app_handle.emit(
                            "job_progress",
                            JobProgress {
                                id: job_id.clone(),
                                pct: 100.0,
                                speed: String::new(),
                                eta: String::new(),
                                size: String::new(),
                            },
                        );
                    }
                    persist_jobs(&app_handle, &state_clone);
                    dispatch(&app_handle);
                    break;
                }
                _ => {}
            }
        }
    });

    Ok(())
}

fn process_line(
    app: &AppHandle,
    state: &Arc<JobsState>,
    job_id: &str,
    line: &str,
    last_total: &mut Option<u64>,
) {
    let l = line.trim_end_matches('\r');

    if let Some(rest) = l.strip_prefix("PROG|") {
        // PROG|<dl>|<total>|<total_estimate>|<speed>|<eta>|<status>
        let parts: Vec<&str> = rest.split('|').collect();
        if parts.len() >= 5 {
            let dl: u64 = parts[0].parse().unwrap_or(0);
            let total_raw: Option<u64> = parts[1].parse().ok();
            let total_est: Option<u64> = parts[2].parse().ok();
            let total: u64 = total_raw.or(total_est).or(*last_total).unwrap_or(0);
            if total > 0 {
                *last_total = Some(total);
            }
            let speed: f64 = parts[3].parse().unwrap_or(0.0);
            let eta: f64 = parts[4].parse().unwrap_or(0.0);
            let pct = if total > 0 {
                (dl as f64 / total as f64 * 100.0) as f32
            } else {
                0.0
            };
            let size = if total > 0 {
                human_bytes(total)
            } else {
                String::new()
            };
            if let Some(rec) = state.records.lock().get_mut(job_id) {
                rec.pct = pct;
                rec.speed = human_speed(speed);
                rec.eta = human_eta(eta);
                if !size.is_empty() {
                    rec.size = size.clone();
                }
            }
            let _ = app.emit(
                "job_progress",
                JobProgress {
                    id: job_id.to_string(),
                    pct,
                    speed: human_speed(speed),
                    eta: human_eta(eta),
                    size,
                },
            );
        }
        return;
    }

    // Three possible path emissions from yt-dlp's pipeline. Prefer the one
    // that resolves to an existing file on disk; FINAL beats MOVE beats DEST
    // when multiple fire.
    for prefix in ["FINAL|", "MOVE|", "DEST|"] {
        if let Some(rest) = l.strip_prefix(prefix) {
            let path = rest.trim().to_string();
            if path.is_empty() {
                return;
            }
            if !std::path::Path::new(&path).is_file() {
                // Skip non-existent paths so we don't overwrite a good DEST
                // with an intermediate that's already been deleted.
                return;
            }
            let real_size = std::fs::metadata(&path).ok().map(|m| human_bytes(m.len()));
            if let Some(rec) = state.records.lock().get_mut(job_id) {
                if let Some(sz) = real_size.clone() {
                    rec.size = sz;
                }
            }
            if let Some(sz) = real_size {
                let _ = app.emit(
                    "job_progress",
                    JobProgress {
                        id: job_id.to_string(),
                        pct: 100.0,
                        speed: String::new(),
                        eta: String::new(),
                        size: sz,
                    },
                );
            }
            let _ = app.emit(
                "job_dest",
                serde_json::json!({ "id": job_id, "path": path }),
            );
            return;
        }
    }

    if let Some(rest) = l.strip_prefix("META|") {
        // META|<id>|<title>|<uploader>|<filesize_or_estimate>
        let parts: Vec<&str> = rest.splitn(4, '|').collect();
        let video_id = parts.get(0).map(|s| s.to_string());
        let title = parts.get(1).map(|s| s.to_string());
        let uploader = parts.get(2).map(|s| s.to_string());
        let size_str = parts
            .get(3)
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|n| *n > 0)
            .map(human_bytes);
        let rtl = title.as_deref().map(detect_rtl).unwrap_or(false);
        if let Some(rec) = state.records.lock().get_mut(job_id) {
            if let Some(t) = title.clone() {
                rec.title = t;
            }
            if let Some(u) = uploader.clone() {
                rec.host = u;
            }
            if let Some(sz) = size_str.clone() {
                rec.size = sz;
            }
            if let Some(v) = video_id.clone() {
                rec.video_id = Some(v);
            }
            rec.rtl = rtl;
        }
        if let Some(sz) = size_str {
            let _ = app.emit(
                "job_progress",
                JobProgress {
                    id: job_id.to_string(),
                    pct: 0.0,
                    speed: String::new(),
                    eta: String::new(),
                    size: sz,
                },
            );
        }
        let _ = app.emit(
            "job_meta",
            JobMeta {
                id: job_id.to_string(),
                title,
                host: uploader,
                rtl: Some(rtl),
            },
        );
        return;
    }

    let _ = app.emit(
        "job_log",
        JobLogLine {
            id: job_id.to_string(),
            kind: "stdout".into(),
            line: l.to_string(),
        },
    );
}

fn mark_failed(app: &AppHandle, state: &Arc<JobsState>, id: &str, err: Option<String>) {
    if let Some(rec) = state.records.lock().get_mut(id) {
        rec.state = "failed".into();
        rec.error_msg = err.clone();
    }
    let _ = app.emit(
        "job_state",
        JobStateChange {
            id: id.to_string(),
            state: "failed".into(),
            error_msg: err,
        },
    );
    persist_jobs(app, state);
}

pub fn cancel_job(app: &AppHandle, id: &str) {
    let state = match app.try_state::<Arc<JobsState>>() {
        Some(s) => s.inner().clone(),
        None => return,
    };
    // Remove from queue if queued
    {
        let mut q = state.queue.lock();
        q.retain(|qid| qid != id);
    }
    // Kill if active
    let was_active = {
        let mut a = state.active.lock();
        if let Some(child) = a.remove(id) {
            let _ = child.kill();
            true
        } else {
            false
        }
    };
    if let Some(rec) = state.records.lock().get_mut(id) {
        rec.state = "failed".into();
        rec.error_msg = Some("cancelled".into());
    }
    let _ = app.emit(
        "job_state",
        JobStateChange {
            id: id.to_string(),
            state: "failed".into(),
            error_msg: Some("cancelled".into()),
        },
    );
    if was_active {
        dispatch(app);
    }
    persist_jobs(app, &state);
}

pub fn remove_job(app: &AppHandle, id: &str) {
    let state = match app.try_state::<Arc<JobsState>>() {
        Some(s) => s.inner().clone(),
        None => return,
    };
    {
        let mut q = state.queue.lock();
        q.retain(|qid| qid != id);
    }
    {
        let mut a = state.active.lock();
        if let Some(child) = a.remove(id) {
            let _ = child.kill();
        }
    }
    state.records.lock().remove(id);
    persist_jobs(app, &state);
}

pub fn retry_job(app: &AppHandle, id: &str) {
    let state = match app.try_state::<Arc<JobsState>>() {
        Some(s) => s.inner().clone(),
        None => return,
    };
    if let Some(rec) = state.records.lock().get_mut(id) {
        rec.state = "queued".into();
        rec.error_msg = None;
        rec.pct = 0.0;
        rec.speed.clear();
        rec.eta.clear();
    }
    state.queue.lock().push_back(id.to_string());
    let _ = app.emit(
        "job_state",
        JobStateChange {
            id: id.to_string(),
            state: "queued".into(),
            error_msg: None,
        },
    );
    persist_jobs(app, &state);
    dispatch(app);
}

pub fn clear_completed(app: &AppHandle) -> usize {
    let state = match app.try_state::<Arc<JobsState>>() {
        Some(s) => s.inner().clone(),
        None => return 0,
    };
    let mut removed: Vec<String> = vec![];
    {
        let mut r = state.records.lock();
        r.retain(|id, rec| {
            let keep = rec.state != "completed";
            if !keep {
                removed.push(id.clone());
            }
            keep
        });
    }
    for id in &removed {
        let _ = app.emit(
            "job_removed",
            serde_json::json!({ "id": id }),
        );
    }
    persist_jobs(app, &state);
    removed.len()
}

fn persist_jobs(app: &AppHandle, state: &Arc<JobsState>) {
    // Sidecar settings store keeps the queue+records snapshot for restore on next launch.
    let snap = state.snapshot_all();
    if let Some(store) = app.try_state::<Arc<parking_lot::Mutex<crate::settings::SettingsStore>>>() {
        let mut s = store.lock();
        let mut cur = s.get().clone();
        cur.jobs_snapshot = Some(snap);
        s.update(cur);
        let _ = s.persist();
    }
}

pub fn restore_from_snapshot(app: &AppHandle, snap: Vec<Job>) {
    // Only carry forward unfinished work. Completed/failed jobs from a previous
    // session don't have actionable rows — restoring them just clutters the
    // queue and inflates the "N done" summary. Their files are still on disk.
    let state = match app.try_state::<Arc<JobsState>>() {
        Some(s) => s.inner().clone(),
        None => return,
    };
    for mut j in snap {
        let keep = match j.state.as_str() {
            "queued" => true,
            "running" => {
                // Interrupted mid-download — re-queue from scratch.
                j.state = "queued".into();
                j.error_msg = None;
                j.pct = 0.0;
                j.speed.clear();
                j.eta.clear();
                true
            }
            _ => false,
        };
        if !keep {
            continue;
        }
        let id = j.id.clone();
        state.records.lock().insert(id.clone(), j.clone());
        state.queue.lock().push_back(id.clone());
        let _ = app.emit("job_added", j);
    }
    dispatch(app);
}
