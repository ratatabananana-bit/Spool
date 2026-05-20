mod debug;
mod jobs;
mod settings;
mod updater;

use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{Manager, WindowEvent};

use jobs::JobsState;
use settings::{Settings, SettingsStore};

#[tauri::command]
fn get_settings(store: tauri::State<Arc<Mutex<SettingsStore>>>) -> Settings {
    store.lock().get().clone()
}

#[tauri::command]
fn save_settings(
    new: Settings,
    store: tauri::State<Arc<Mutex<SettingsStore>>>,
) -> Result<(), String> {
    let mut s = store.lock();
    s.update(new);
    s.persist().map_err(|e| e.to_string())
}

#[tauri::command]
fn window_minimize(window: tauri::Window) {
    let _ = window.minimize();
}

#[tauri::command]
fn window_toggle_max(window: tauri::Window) {
    let maximized = window.is_maximized().unwrap_or(false);
    if maximized {
        let _ = window.unmaximize();
    } else {
        let _ = window.maximize();
    }
}

#[tauri::command]
fn window_close(window: tauri::Window) {
    let _ = window.close();
}

#[tauri::command]
fn window_start_drag(window: tauri::Window) {
    let _ = window.start_dragging();
}

#[tauri::command]
async fn check_yt_dlp_update(app: tauri::AppHandle) {
    updater::check_updates(app).await;
}

#[tauri::command]
fn set_window_background(window: tauri::Window, hex: String) -> Result<(), String> {
    let h = hex.trim_start_matches('#');
    if h.len() != 6 {
        return Err("bad hex".into());
    }
    let r = u8::from_str_radix(&h[0..2], 16).map_err(|e| e.to_string())?;
    let g = u8::from_str_radix(&h[2..4], 16).map_err(|e| e.to_string())?;
    let b = u8::from_str_radix(&h[4..6], 16).map_err(|e| e.to_string())?;
    window
        .set_background_color(Some(tauri::window::Color(r, g, b, 255)))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn add_jobs(
    app: tauri::AppHandle,
    urls: Vec<String>,
    format: String,
    output_dir: String,
    filename_template: String,
) -> Result<Vec<jobs::EnqueueOutcome>, String> {
    let mut out = Vec::new();
    for url in urls {
        let url = url.trim().to_string();
        if url.is_empty() {
            continue;
        }
        match jobs::enqueue_job(
            &app,
            url,
            format.clone(),
            output_dir.clone(),
            filename_template.clone(),
        ) {
            Ok(o) => out.push(o),
            Err(e) => return Err(e),
        }
    }
    Ok(out)
}

#[tauri::command]
fn cancel_job(app: tauri::AppHandle, id: String) {
    jobs::cancel_job(&app, &id);
}

#[tauri::command]
fn remove_job(app: tauri::AppHandle, id: String) {
    jobs::remove_job(&app, &id);
}

#[tauri::command]
fn retry_job(app: tauri::AppHandle, id: String) {
    jobs::retry_job(&app, &id);
}

#[tauri::command]
fn clear_completed(app: tauri::AppHandle) -> usize {
    jobs::clear_completed(&app)
}

#[tauri::command]
async fn probe_playlist(app: tauri::AppHandle, url: String) -> Result<serde_json::Value, String> {
    use tauri_plugin_shell::process::CommandEvent;
    use tauri_plugin_shell::ShellExt;
    let cmd = app
        .shell()
        .sidecar("yt-dlp")
        .map_err(|e| e.to_string())?
        .args([
            "--flat-playlist",
            "-J",
            "--no-warnings",
            "--no-call-home",
            "--ignore-config",
            &url,
        ])
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONUTF8", "1");

    let (mut rx, _child) = cmd.spawn().map_err(|e| e.to_string())?;
    let mut buf: Vec<u8> = Vec::new();
    while let Some(evt) = rx.recv().await {
        match evt {
            CommandEvent::Stdout(b) => buf.extend_from_slice(&b),
            CommandEvent::Terminated(_) => break,
            _ => {}
        }
    }
    let text = String::from_utf8_lossy(&buf);
    if text.trim().is_empty() {
        return Err("yt-dlp produced no output".into());
    }
    serde_json::from_str::<serde_json::Value>(&text).map_err(|e| format!("parse: {e}"))
}

#[tauri::command]
async fn save_log_file(
    app: tauri::AppHandle,
    suggested: String,
    contents: String,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = std::sync::mpsc::channel();
    app.dialog()
        .file()
        .set_file_name(&suggested)
        .add_filter("Log", &["log", "txt"])
        .save_file(move |path| {
            let _ = tx.send(path);
        });
    let chosen = tokio::task::spawn_blocking(move || rx.recv().ok().flatten())
        .await
        .ok()
        .flatten();
    let Some(fp) = chosen else { return Ok(None) };
    let path = fp.into_path().map_err(|e| e.to_string())?;
    std::fs::write(&path, contents.as_bytes()).map_err(|e| e.to_string())?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

#[tauri::command]
fn path_exists(path: String) -> bool {
    std::path::Path::new(&path).exists()
}

#[tauri::command]
fn ffmpeg_status() -> serde_json::Value {
    let exe = std::env::current_exe().ok();
    let dir = exe.as_ref().and_then(|p| p.parent()).map(|p| p.to_path_buf());
    let ff = dir.as_ref().map(|d| d.join("ffmpeg.exe"));
    let exists = ff.as_ref().map(|p| p.exists()).unwrap_or(false);
    serde_json::json!({
        "exists": exists,
        "path": ff.map(|p| p.to_string_lossy().into_owned()),
        "dir": dir.map(|p| p.to_string_lossy().into_owned()),
    })
}

#[tauri::command]
fn open_file(path: String) -> Result<(), String> {
    // ShellExecute via cmd's `start ""` — opens the file with its default app.
    // If the recorded path no longer exists (yt-dlp post-processing changed
    // the extension, user moved the file, etc.), fall back to opening the
    // parent directory in Explorer rather than showing a Windows "file not
    // found" dialog.
    use std::os::windows::process::CommandExt;
    use std::path::Path;
    use std::process::Command;

    let p = Path::new(&path);
    if p.is_file() {
        Command::new("cmd")
            .args(["/c", "start", "", &path])
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    } else if let Some(dir) = p.parent().filter(|d| d.exists()) {
        Command::new("explorer.exe")
            .arg(dir.as_os_str())
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    } else {
        Err(format!("path not found: {path}"))
    }
}

#[tauri::command]
fn open_in_explorer(path: String) -> Result<(), String> {
    use std::process::Command;
    let p = std::path::Path::new(&path);
    if p.is_file() {
        Command::new("explorer.exe")
            .arg(format!("/select,{}", p.display()))
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    } else {
        // Fall back to opening the directory itself.
        let dir = if p.is_dir() { p } else { p.parent().unwrap_or(p) };
        Command::new("explorer.exe")
            .arg(dir.as_os_str())
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

#[tauri::command]
fn set_concurrency(app: tauri::AppHandle, n: u32) {
    if let Some(state) = app.try_state::<Arc<jobs::JobsState>>() {
        state.set_concurrency(n);
    }
    jobs::dispatch(&app);
}

#[tauri::command]
async fn pick_folder(app: tauri::AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = std::sync::mpsc::channel();
    app.dialog().file().pick_folder(move |path| {
        let _ = tx.send(path);
    });
    let path = tokio::task::spawn_blocking(move || rx.recv().ok().flatten())
        .await
        .ok()
        .flatten();
    path.and_then(|p| p.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    debug::init();
    dlog!("run() entered");
    let store = Arc::new(Mutex::new(
        SettingsStore::load().unwrap_or_else(|_| SettingsStore::new_default()),
    ));

    let dirty = Arc::new(AtomicBool::new(false));
    {
        let store_bg = store.clone();
        let dirty_bg = dirty.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(500));
            if dirty_bg.swap(false, Ordering::SeqCst) {
                let _ = store_bg.lock().persist();
            }
        });
    }

    let jobs_state = JobsState::new();
    {
        let s = store.lock().get().clone();
        if let Some(c) = s.concurrency {
            jobs_state.set_concurrency(c);
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .manage(store.clone())
        .manage(jobs_state.clone())
        .setup(move |app| {
            let win = app.get_webview_window("main").unwrap();
            let s = store.lock().get().clone();

            if let (Some(w), Some(h)) = (s.window_w, s.window_h) {
                let _ = win.set_size(tauri::Size::Physical(tauri::PhysicalSize {
                    width: w,
                    height: h,
                }));
            }
            let _ = win.show();

            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                updater::check_updates(app_handle).await;
            });

            // Restore jobs snapshot from settings.
            let snap = store.lock().get().jobs_snapshot.clone();
            if let Some(snap) = snap {
                let app_h = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    // brief delay so frontend has time to attach listeners
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    jobs::restore_from_snapshot(&app_h, snap);
                });
            }

            let store_resize = store.clone();
            let dirty_resize = dirty.clone();
            win.on_window_event(move |evt| match evt {
                WindowEvent::Resized(size) => {
                    let mut s = store_resize.lock();
                    let mut cur = s.get().clone();
                    cur.window_w = Some(size.width);
                    cur.window_h = Some(size.height);
                    s.update(cur);
                    dirty_resize.store(true, Ordering::SeqCst);
                }
                WindowEvent::CloseRequested { .. } => {
                    let _ = store_resize.lock().persist();
                }
                _ => {}
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            window_minimize,
            window_toggle_max,
            window_close,
            window_start_drag,
            set_window_background,
            check_yt_dlp_update,
            add_jobs,
            cancel_job,
            remove_job,
            retry_job,
            clear_completed,
            set_concurrency,
            pick_folder,
            open_in_explorer,
            open_file,
            probe_playlist,
            save_log_file,
            path_exists,
            ffmpeg_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri app");
}
