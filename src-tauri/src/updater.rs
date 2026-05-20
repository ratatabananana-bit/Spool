use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdaterStatus {
    pub kind: String,           // "checking" | "up-to-date" | "updated" | "offline"
    pub version: Option<String>,
    pub message: Option<String>,
}

fn emit(app: &AppHandle, status: UpdaterStatus) {
    let _ = app.emit("updater_status", status);
}

async fn read_version(app: &AppHandle) -> Option<String> {
    let cmd = app.shell().sidecar("yt-dlp").ok()?;
    let (mut rx, _child) = cmd.args(["--version"]).spawn().ok()?;
    let mut buf = String::new();
    while let Some(evt) = rx.recv().await {
        match evt {
            CommandEvent::Stdout(b) | CommandEvent::Stderr(b) => {
                if let Ok(s) = std::str::from_utf8(&b) {
                    buf.push_str(s);
                }
            }
            CommandEvent::Terminated(_) => break,
            _ => {}
        }
    }
    let trimmed = buf.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub async fn check_updates(app: AppHandle) {
    let cached_version = read_version(&app).await;

    emit(
        &app,
        UpdaterStatus {
            kind: "checking".into(),
            version: cached_version.clone(),
            message: None,
        },
    );

    let cmd = match app.shell().sidecar("yt-dlp") {
        Ok(c) => c,
        Err(e) => {
            emit(
                &app,
                UpdaterStatus {
                    kind: "offline".into(),
                    version: cached_version,
                    message: Some(format!("sidecar error: {e}")),
                },
            );
            return;
        }
    };

    let (mut rx, _child) = match cmd.args(["-U", "--no-warnings"]).spawn() {
        Ok(p) => p,
        Err(e) => {
            emit(
                &app,
                UpdaterStatus {
                    kind: "offline".into(),
                    version: cached_version,
                    message: Some(format!("spawn error: {e}")),
                },
            );
            return;
        }
    };

    let mut out = String::new();
    while let Some(evt) = rx.recv().await {
        match evt {
            CommandEvent::Stdout(b) | CommandEvent::Stderr(b) => {
                if let Ok(s) = std::str::from_utf8(&b) {
                    out.push_str(s);
                }
            }
            CommandEvent::Terminated(_) => break,
            _ => {}
        }
    }

    // Parse outcome
    let lower = out.to_lowercase();
    let looks_offline = lower.contains("unable to download")
        || lower.contains("name resolution")
        || lower.contains("no internet")
        || lower.contains("connection")
        || lower.contains("network");

    let updated_re = regex_lite_find(&out, "Updated yt-dlp to ");
    let up_to_date_re = lower.contains("up to date") || lower.contains("up-to-date");

    let final_version = read_version(&app).await.or(cached_version.clone());

    let status = if let Some(after) = updated_re {
        UpdaterStatus {
            kind: "updated".into(),
            version: Some(extract_version(&after).unwrap_or_else(|| final_version.clone().unwrap_or_default())),
            message: None,
        }
    } else if up_to_date_re {
        UpdaterStatus {
            kind: "up-to-date".into(),
            version: final_version,
            message: None,
        }
    } else if looks_offline {
        UpdaterStatus {
            kind: "offline".into(),
            version: final_version,
            message: Some(first_line(&out).to_string()),
        }
    } else {
        UpdaterStatus {
            kind: "up-to-date".into(),
            version: final_version,
            message: None,
        }
    };

    emit(&app, status);
}

fn regex_lite_find<'a>(text: &'a str, needle: &str) -> Option<&'a str> {
    text.find(needle).map(|i| &text[i + needle.len()..])
}

fn extract_version(s: &str) -> Option<String> {
    let line: String = s.chars().take_while(|c| !c.is_whitespace() && *c != '\n').collect();
    if line.is_empty() {
        None
    } else {
        Some(line.trim_end_matches('.').to_string())
    }
}

fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or("").trim()
}
