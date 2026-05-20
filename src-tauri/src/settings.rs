use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default)]
    pub theme: Option<String>, // "light" | "dark" | "system"
    #[serde(default)]
    pub locale: Option<String>, // "en" | "zh-Hant"
    #[serde(default)]
    pub output_dir: Option<String>,
    #[serde(default)]
    pub default_format: Option<String>, // "Best" | "1080p" | "720p" | "480p" | "Audio"
    #[serde(default)]
    pub concurrency: Option<u32>,
    #[serde(default)]
    pub filename_template: Option<String>,
    #[serde(default)]
    pub auto_clear_days: Option<u32>,
    #[serde(default)]
    pub remember_history: Option<bool>,
    #[serde(default)]
    pub playlist_mode: Option<String>, // "single" | "ask" | "all"
    #[serde(default)]
    pub write_subtitles: Option<bool>,
    #[serde(default)]
    pub subtitle_langs: Option<String>,
    #[serde(default)]
    pub sponsor_block: Option<bool>,
    #[serde(default)]
    pub embed_thumbnail: Option<bool>,
    #[serde(default)]
    pub embed_metadata: Option<bool>,
    #[serde(default)]
    pub embed_chapters: Option<bool>,
    #[serde(default)]
    pub restrict_filenames: Option<bool>,
    #[serde(default)]
    pub write_thumbnail: Option<bool>,
    #[serde(default)]
    pub write_info_json: Option<bool>,
    #[serde(default)]
    pub write_description: Option<bool>,
    #[serde(default)]
    pub cookies_from_browser: Option<String>, // "" | "chrome" | "firefox" | "edge" | "brave" | "opera" | "vivaldi"
    #[serde(default)]
    pub rate_limit: Option<String>, // e.g. "5M"
    #[serde(default)]
    pub retries: Option<u32>,
    #[serde(default)]
    pub keep_video: Option<bool>, // keep video file when extracting audio
    #[serde(default)]
    pub extra_args: Option<String>, // power-user passthrough, whitespace-split
    #[serde(default)]
    pub url_history: Vec<String>,
    #[serde(default)]
    pub output_dir_history: Vec<String>,
    #[serde(default)]
    pub window_w: Option<u32>,
    #[serde(default)]
    pub window_h: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jobs_snapshot: Option<Vec<crate::jobs::Job>>,
}

fn defaults() -> Settings {
    Settings {
        theme: Some("system".into()),
        locale: Some("en".into()),
        output_dir: dirs::download_dir().map(|p| p.to_string_lossy().into_owned()),
        default_format: Some("1080p".into()),
        concurrency: Some(3),
        filename_template: Some("%(title)s [%(id)s].%(ext)s".into()),
        auto_clear_days: Some(0),
        remember_history: Some(true),
        playlist_mode: Some("single".into()),
        write_subtitles: Some(false),
        subtitle_langs: Some("en.*".into()),
        sponsor_block: Some(false),
        embed_thumbnail: Some(false),
        embed_metadata: Some(false),
        embed_chapters: Some(false),
        restrict_filenames: Some(false),
        write_thumbnail: Some(false),
        write_info_json: Some(false),
        write_description: Some(false),
        cookies_from_browser: Some("".into()),
        rate_limit: Some("".into()),
        retries: Some(10),
        keep_video: Some(false),
        extra_args: Some("".into()),
        url_history: vec![],
        output_dir_history: vec![],
        window_w: Some(960),
        window_h: Some(640),
        jobs_snapshot: None,
    }
}

pub struct SettingsStore {
    path: PathBuf,
    current: Settings,
}

impl SettingsStore {
    fn settings_path() -> PathBuf {
        let exe = std::env::current_exe().expect("current_exe");
        let dir = exe
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        dir.join("settings.json")
    }

    pub fn new_default() -> Self {
        Self {
            path: Self::settings_path(),
            current: defaults(),
        }
    }

    pub fn load() -> Result<Self, String> {
        let path = Self::settings_path();
        if !path.exists() {
            return Ok(Self {
                path,
                current: defaults(),
            });
        }
        let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let parsed: Settings = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
        // Merge with defaults to fill missing
        let d = defaults();
        let merged = Settings {
            theme: parsed.theme.or(d.theme),
            locale: parsed.locale.or(d.locale),
            output_dir: parsed.output_dir.or(d.output_dir),
            default_format: parsed.default_format.or(d.default_format),
            concurrency: parsed.concurrency.or(d.concurrency),
            filename_template: parsed.filename_template.or(d.filename_template),
            auto_clear_days: parsed.auto_clear_days.or(d.auto_clear_days),
            remember_history: parsed.remember_history.or(d.remember_history),
            playlist_mode: parsed.playlist_mode.or(d.playlist_mode),
            write_subtitles: parsed.write_subtitles.or(d.write_subtitles),
            subtitle_langs: parsed.subtitle_langs.or(d.subtitle_langs),
            sponsor_block: parsed.sponsor_block.or(d.sponsor_block),
            embed_thumbnail: parsed.embed_thumbnail.or(d.embed_thumbnail),
            embed_metadata: parsed.embed_metadata.or(d.embed_metadata),
            embed_chapters: parsed.embed_chapters.or(d.embed_chapters),
            restrict_filenames: parsed.restrict_filenames.or(d.restrict_filenames),
            write_thumbnail: parsed.write_thumbnail.or(d.write_thumbnail),
            write_info_json: parsed.write_info_json.or(d.write_info_json),
            write_description: parsed.write_description.or(d.write_description),
            cookies_from_browser: parsed.cookies_from_browser.or(d.cookies_from_browser),
            rate_limit: parsed.rate_limit.or(d.rate_limit),
            retries: parsed.retries.or(d.retries),
            keep_video: parsed.keep_video.or(d.keep_video),
            extra_args: parsed.extra_args.or(d.extra_args),
            url_history: parsed.url_history,
            output_dir_history: parsed.output_dir_history,
            window_w: parsed.window_w.or(d.window_w),
            window_h: parsed.window_h.or(d.window_h),
            jobs_snapshot: parsed.jobs_snapshot,
        };
        Ok(Self {
            path,
            current: merged,
        })
    }

    pub fn get(&self) -> &Settings {
        &self.current
    }

    pub fn update(&mut self, new: Settings) {
        self.current = new;
    }

    pub fn persist(&self) -> Result<(), String> {
        let raw = serde_json::to_string_pretty(&self.current).map_err(|e| e.to_string())?;
        fs::write(&self.path, raw).map_err(|e| e.to_string())
    }
}
