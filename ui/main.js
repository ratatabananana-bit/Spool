// yt-portable frontend. Vanilla.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// ── i18n ─────────────────────────────────────────────────────
function t(key, args) {
  const loc = state.settings?.locale || "en";
  const dict = window.LOCALES?.[loc] || window.LOCALES?.en || {};
  let s = dict[key];
  if (s == null) {
    const en = window.LOCALES?.en || {};
    s = en[key];
  }
  if (s == null) return key;
  if (args) {
    for (const [k, v] of Object.entries(args)) {
      s = s.replaceAll(`{${k}}`, String(v));
    }
  }
  return s;
}

function applyLocale() {
  document.querySelectorAll("[data-i18n]").forEach((el) => {
    el.textContent = t(el.dataset.i18n);
  });
  document.querySelectorAll("[data-i18n-title]").forEach((el) => {
    el.title = t(el.dataset.i18nTitle);
  });
  document.querySelectorAll("[data-i18n-placeholder]").forEach((el) => {
    el.placeholder = t(el.dataset.i18nPlaceholder);
  });
  // Special: empty-state hint with inline kbd/tag tokens
  const hint = document.getElementById("emptyHint");
  if (hint) {
    let s = t("empty.hint");
    s = s
      .replace(/\{tag\}/g, '<span class="ytp-empty__tag">')
      .replace(/\{\/tag\}/g, "</span>")
      .replace(/\{kbd\}/g, "<kbd>")
      .replace(/\{\/kbd\}/g, "</kbd>");
    hint.innerHTML = s;
  }
  // Sync dynamic widgets
  if (state.settings) {
    syncDynamicLabels();
  }
}

function syncDynamicLabels() {
  // Format value label
  const fmt = state.settings.defaultFormat || "1080p";
  $("formatValue").textContent = formatLabel(fmt);
  // Playlist mode value label
  const plMode = state.settings.playlistMode || "single";
  const plVal = plMode === "single" ? t("tb.playlist.off") : plMode === "ask" ? t("tb.playlist.ask") : t("tb.playlist.all");
  $("playlistValue").textContent = plVal;
  // Queue status
  refreshQueueStatus();
  // Updater chip — re-render via current state
  if (lastUpdaterStatus) setUpdaterChip(lastUpdaterStatus);
  // Folder recent labels
  renderFolderRecent();
}

const root = document.getElementById("root");
const $ = (id) => document.getElementById(id);

const THEME_BG = { light: "#f6f5f1", dark: "#131311" };

const state = {
  settings: null,
  systemTheme: matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light",
  jobs: new Map(), // id -> job
  jobLogs: new Map(), // id -> string[] (log lines)
  jobDests: new Map(), // id -> final filepath
  openLogId: null,
  bannerIds: 0,
};

// ── Theme ─────────────────────────────────────────────────────
function applyTheme() {
  const pref = state.settings?.theme || "system";
  const eff = pref === "system" ? state.systemTheme : pref;
  root.setAttribute("data-theme", eff);
  document.documentElement.style.background = THEME_BG[eff];
  document.body.style.background = THEME_BG[eff];
  invoke("set_window_background", { hex: THEME_BG[eff] }).catch((e) => console.error(e));
}

// ── Settings persistence ─────────────────────────────────────
async function persistSetting(patch) {
  state.settings = { ...state.settings, ...patch };
  try {
    await invoke("save_settings", { new: state.settings });
  } catch (e) {
    console.error("save_settings", e);
  }
}

// ── Format select ────────────────────────────────────────────
function formatLabel(fmt) {
  if (!fmt) fmt = "1080p";
  const key = {
    Best: "fmt.best",
    "2160p": "fmt.2160.any",
    "2160p MP4": "fmt.2160.mp4",
    "2160p WebM": "fmt.2160.webm",
    "1440p": "fmt.1440.any",
    "1440p MP4": "fmt.1440.mp4",
    "1440p WebM": "fmt.1440.webm",
    "1080p": "fmt.1080.any",
    "1080p MP4": "fmt.1080.mp4",
    "1080p WebM": "fmt.1080.webm",
    "720p": "fmt.720.any",
    "720p MP4": "fmt.720.mp4",
    "720p WebM": "fmt.720.webm",
    "480p": "fmt.480.any",
    "Audio MP3": "fmt.audio.mp3",
    "Audio M4A": "fmt.audio.m4a",
    "Audio Opus": "fmt.audio.opus",
    "Audio WAV": "fmt.audio.wav",
    "Audio FLAC": "fmt.audio.flac",
    Audio: "fmt.audio.mp3",
  }[fmt];
  return key ? t(key) : fmt;
}

function setFormat(fmt) {
  state.settings.defaultFormat = fmt;
  $("formatValue").textContent = formatLabel(fmt);
  $("formatMenu")
    .querySelectorAll("button[data-fmt]")
    .forEach((b) => b.setAttribute("data-on", b.dataset.fmt === fmt));
  persistSetting({ defaultFormat: fmt });
}

function wireFormat() {
  const btn = $("formatBtn");
  const menu = $("formatMenu");
  function place() {
    const r = btn.getBoundingClientRect();
    menu.style.top = `${r.bottom + 4}px`;
    menu.style.left = `${r.left}px`;
  }
  btn.addEventListener("click", (e) => {
    e.stopPropagation();
    const open = !menu.hidden;
    if (open) {
      menu.hidden = true;
    } else {
      place();
      menu.hidden = false;
    }
  });
  menu.addEventListener("click", (e) => {
    const b = e.target.closest("button[data-fmt]");
    if (!b) return;
    setFormat(b.dataset.fmt);
    menu.hidden = true;
  });
  document.addEventListener("click", () => (menu.hidden = true));
}

// ── Playlist mode select ─────────────────────────────────────
function setPlaylistMode(mode) {
  state.settings.playlistMode = mode;
  const label = mode === "single" ? t("tb.playlist.off") : mode === "ask" ? t("tb.playlist.ask") : t("tb.playlist.all");
  $("playlistValue").textContent = label;
  $("playlistMenu")
    .querySelectorAll("button[data-pl]")
    .forEach((b) => b.setAttribute("data-on", b.dataset.pl === mode));
  persistSetting({ playlistMode: mode });
}

function wirePlaylistMode() {
  const btn = $("playlistBtn");
  const menu = $("playlistMenu");
  function place() {
    const r = btn.getBoundingClientRect();
    menu.style.top = `${r.bottom + 4}px`;
    menu.style.left = `${r.left}px`;
  }
  btn.addEventListener("click", (e) => {
    e.stopPropagation();
    if (menu.hidden) {
      place();
      menu.hidden = false;
    } else {
      menu.hidden = true;
    }
  });
  menu.addEventListener("click", (e) => {
    const b = e.target.closest("button[data-pl]");
    if (!b) return;
    setPlaylistMode(b.dataset.pl);
    menu.hidden = true;
  });
  document.addEventListener("click", () => (menu.hidden = true));
}

// ── Folder pill ──────────────────────────────────────────────
function setOutputDir(path, opts = {}) {
  state.settings.outputDir = path;
  $("folderPath").textContent = path || "—";
  $("outputPathText").textContent = path || "—";
  let history = (state.settings.outputDirHistory || []).filter((p) => p !== path);
  if (path) history.unshift(path);
  history = history.slice(0, 5);
  state.settings.outputDirHistory = history;
  persistSetting({ outputDir: path, outputDirHistory: history });
  renderFolderRecent();
  checkOutputDir();
}

function renderFolderRecent() {
  const list = state.settings.outputDirHistory || [];
  const current = state.settings.outputDir;
  const el = $("folderRecent");
  if (!el) return;
  if (!list.length) {
    el.innerHTML = `<div class="ytp-select__empty">${escapeHtml(t("folder.empty"))}</div>`;
    return;
  }
  el.innerHTML = list
    .map((p) => {
      const cur = p === current ? ' data-on="true"' : "";
      return `<button data-folder="${escapeAttr(p)}" type="button"${cur}>${escapeHtml(p)}</button>`;
    })
    .join("");
}

function escapeAttr(s) {
  return (s || "").replace(/"/g, "&quot;");
}

function wireFolder() {
  const btn = $("folderBtn");
  const menu = $("folderMenu");
  function place() {
    const r = btn.getBoundingClientRect();
    menu.style.top = `${r.bottom + 4}px`;
    menu.style.left = `${r.left}px`;
  }
  btn.addEventListener("click", (e) => {
    e.stopPropagation();
    // Click on the leading folder icon → open the current output folder in Explorer.
    // Click anywhere else on the pill → open the recent-folders dropdown.
    const iconClicked = e.target.closest("svg") !== null;
    if (iconClicked) {
      const dir = state.settings.outputDir;
      if (dir) {
        invoke("open_in_explorer", { path: dir }).catch((err) =>
          console.error("open_in_explorer", err)
        );
      } else {
        flashToast(t("enqueue.no_dir"));
      }
      return;
    }
    renderFolderRecent();
    if (menu.hidden) {
      place();
      menu.hidden = false;
    } else {
      menu.hidden = true;
    }
  });
  menu.addEventListener("click", async (e) => {
    e.stopPropagation();
    const browse = e.target.closest('button[data-act="browse"]');
    const pick = e.target.closest("button[data-folder]");
    if (browse) {
      menu.hidden = true;
      try {
        const path = await invoke("pick_folder");
        if (path) setOutputDir(path);
      } catch (err) {
        console.error("pick_folder", err);
      }
    } else if (pick) {
      menu.hidden = true;
      setOutputDir(pick.dataset.folder);
    }
  });
  document.addEventListener("click", () => (menu.hidden = true));
}

// ── Empty / queue toggle ─────────────────────────────────────
function refreshEmpty() {
  const has = state.jobs.size > 0;
  $("emptyState").hidden = has;
  $("queue").hidden = !has;
}

// ── Row render ───────────────────────────────────────────────
const ICONS = {
  check:
    '<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M3 8.5l3 3 7-7"/></svg>',
  x:
    '<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M4 4l8 8M12 4l-8 8"/></svg>',
  retry:
    '<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M13 8a5 5 0 11-1.6-3.66"/><path d="M13.5 2.5V5H11"/></svg>',
  trash:
    '<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M3 4.5h10M6.5 4.5V3h3v1.5M5 4.5l.6 8.4A1 1 0 006.6 14h2.8a1 1 0 001-1.1L11 4.5"/></svg>',
  log:
    '<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M3 3.5h10M3 6.5h10M3 9.5h7M3 12.5h5"/></svg>',
  open:
    '<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M9 3h4v4M13 3l-6 6M6.5 4H4a1 1 0 00-1 1v7a1 1 0 001 1h7a1 1 0 001-1V9.5"/></svg>',
  pause:
    '<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M6 4v8M10 4v8"/></svg>',
};

function statusGlyphHTML(stateName) {
  if (stateName === "ok") return `<span style="color:var(--ok);display:inline-flex">${ICONS.check}</span>`;
  if (stateName === "err") return `<span style="color:var(--err);display:inline-flex">${ICONS.x}</span>`;
  if (stateName === "paused") return `<span style="color:var(--warn);display:inline-flex">${ICONS.pause}</span>`;
  if (stateName === "run")
    return `<span style="color:var(--accent);display:inline-flex"><i class="ytp-spin"></i></span>`;
  return `<span style="display:inline-block;width:8px;height:8px;box-sizing:border-box;border:1.5px solid var(--text-3);border-radius:50%"></span>`;
}

function escapeHtml(s) {
  return (s || "").replace(/[&<>"']/g, (c) =>
    ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c])
  );
}

function stateLabel(job) {
  switch (job.state) {
    case "queued":
      return t("row.state.queued");
    case "run":
    case "running":
      return t("row.state.running");
    case "ok":
    case "completed":
      return t("row.state.completed");
    case "failed":
    case "err":
      return t("row.state.failed") + (job.errorMsg ? ` · ${job.errorMsg}` : "");
    case "paused":
      return t("row.state.paused");
    default:
      return job.state;
  }
}

function normalizeState(s) {
  if (s === "completed") return "ok";
  if (s === "failed") return "err";
  if (s === "running") return "run";
  return s;
}

function rowHTML(job) {
  const ns = normalizeState(job.state);
  const stateCls = `ytp-row__sub-state ytp-row__sub-state--${ns}`;
  const progressKind =
    ns === "ok" ? "ok" : ns === "err" ? "err" : ns === "queued" || ns === "paused" ? "idle" : "";
  const pct = ns === "ok" ? 100 : ns === "err" || ns === "queued" ? 0 : job.pct || 0;
  const pctLabel =
    ns === "ok" ? "100%" : ns === "err" || ns === "queued" ? "—" : `${Math.floor(pct)}%`;
  const dir = job.rtl ? "rtl" : "ltr";

  const ttOpen = escapeAttr(t("row.action.open"));
  const ttCancel = escapeAttr(t("row.action.cancel"));
  const ttRetry = escapeAttr(t("row.action.retry"));
  const ttLog = escapeAttr(t("row.action.log"));
  const ttRemove = escapeAttr(t("row.action.remove"));

  let actions = "";
  if (ns === "ok") {
    actions += `<button class="ytp-iconbtn" data-act="open" title="${ttOpen}">${ICONS.open}</button>`;
    actions += `<button class="ytp-iconbtn" data-act="log" title="${ttLog}">${ICONS.log}</button>`;
    actions += `<button class="ytp-iconbtn ytp-iconbtn--danger" data-act="remove" title="${ttRemove}">${ICONS.trash}</button>`;
  } else if (ns === "err") {
    actions += `<button class="ytp-iconbtn" data-act="retry" title="${ttRetry}">${ICONS.retry}</button>`;
    actions += `<button class="ytp-iconbtn" data-act="log" title="${ttLog}">${ICONS.log}</button>`;
    actions += `<button class="ytp-iconbtn ytp-iconbtn--danger" data-act="remove" title="${ttRemove}">${ICONS.trash}</button>`;
  } else if (ns === "run") {
    actions += `<button class="ytp-iconbtn" data-act="log" title="${ttLog}">${ICONS.log}</button>`;
    actions += `<button class="ytp-iconbtn ytp-iconbtn--danger" data-act="cancel" title="${ttCancel}">${ICONS.x}</button>`;
  } else if (ns === "queued") {
    actions += `<button class="ytp-iconbtn ytp-iconbtn--danger" data-act="cancel" title="${ttCancel}">${ICONS.x}</button>`;
  } else if (ns === "paused") {
    actions += `<button class="ytp-iconbtn" data-act="log" title="${ttLog}">${ICONS.log}</button>`;
    actions += `<button class="ytp-iconbtn ytp-iconbtn--danger" data-act="remove" title="${ttRemove}">${ICONS.trash}</button>`;
  }

  return `
  <div class="ytp-row" data-id="${escapeHtml(job.id)}">
    <div class="ytp-row__status">${statusGlyphHTML(ns)}</div>
    <div class="ytp-row__title-wrap">
      <div class="ytp-row__title" dir="${dir}">${escapeHtml(job.title || job.url)}</div>
      <div class="ytp-row__sub">
        <span class="${stateCls}">
          ${ns === "run" ? '<span class="ytp-row__dot ytp-row__dot--pulse"></span>' : ""}
          ${escapeHtml(stateLabel(job))}
        </span>
        <span class="ytp-row__sep">·</span>
        <span>${escapeHtml(job.format || "")}</span>
        <span class="ytp-row__sep">·</span>
        <span style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap;direction:ltr">${escapeHtml(
          job.host || ""
        )}</span>
      </div>
    </div>
    <div class="ytp-row__meta ytp-row__meta--muted">${escapeHtml(job.size || "—")}</div>
    <div class="ytp-row__meta">${escapeHtml(job.speed || (ns === "ok" ? "—" : ""))}</div>
    <div class="ytp-row__progress-wrap">
      <div class="ytp-row__progress${progressKind ? " ytp-row__progress--" + progressKind : ""}">
        <i style="width:${pct}%"></i>
      </div>
      <span class="ytp-row__pct">${pctLabel}</span>
    </div>
    <div class="ytp-row__actions">${actions}</div>
  </div>`;
}

function rowGroupHTML(job) {
  return `<div class="ytp-row-group" data-group-id="${escapeAttr(job.id)}">${rowHTML(job)}<div class="ytp-log" data-log-id="${escapeAttr(job.id)}"></div></div>`;
}

function renderRow(id) {
  const job = state.jobs.get(id);
  if (!job) return;
  const list = $("queueList");
  const existing = list.querySelector(`[data-group-id="${CSS.escape(id)}"]`);
  if (existing) {
    // Preserve log panel state — replace only the row, keep the log if open.
    const rowEl = existing.querySelector(`.ytp-row[data-id="${CSS.escape(id)}"]`);
    if (rowEl) rowEl.outerHTML = rowHTML(job);
  } else {
    list.insertAdjacentHTML("beforeend", rowGroupHTML(job));
  }
  if (state.openLogId === id) {
    renderLogPanel(id);
  }
  refreshEmpty();
  refreshQueueStatus();
}

function classifyLine(line) {
  const l = line.toLowerCase();
  if (l.includes("[error]") || l.startsWith("[stderr] error") || l.includes("error:")) return "err";
  if (l.includes("[stderr] warning")) return "info";
  if (l.includes("[info]")) return "info";
  return "";
}

function renderLogPanel(id) {
  const group = $("queueList").querySelector(`[data-group-id="${CSS.escape(id)}"]`);
  if (!group) return;
  const panel = group.querySelector(".ytp-log");
  if (!panel) return;
  const job = state.jobs.get(id);
  const lines = state.jobLogs.get(id) || [];
  const lineHTML = lines
    .slice(-2000)
    .map((l) => {
      const cls = classifyLine(l);
      return `<div class="ytp-log__line${cls ? " ytp-log__line--" + cls : ""}">${escapeHtml(l)}</div>`;
    })
    .join("");
  panel.innerHTML = `
    <div class="ytp-log__head">
      <span>${escapeHtml(t("log.head", { label: job?.title || job?.url || id }))}</span>
      <span class="ytp-log__sp"></span>
      <button class="ytp-btn" data-log-act="copy">${escapeHtml(t("log.copy"))}</button>
      <button class="ytp-btn" data-log-act="save">${escapeHtml(t("log.save"))}</button>
      <button class="ytp-btn" data-log-act="close">${escapeHtml(t("log.close"))}</button>
    </div>
    <div class="ytp-log__body" data-log-body>${lineHTML || `<div class="ytp-log__line" style="color:var(--text-3)">${escapeHtml(t("log.empty"))}</div>`}</div>`;
  panel.classList.add("ytp-log--open");
  const body = panel.querySelector("[data-log-body]");
  if (body) body.scrollTop = body.scrollHeight;
}

function closeLogPanel(id) {
  const group = $("queueList").querySelector(`[data-group-id="${CSS.escape(id)}"]`);
  if (!group) return;
  const panel = group.querySelector(".ytp-log");
  if (!panel) return;
  panel.classList.remove("ytp-log--open");
  setTimeout(() => {
    if (state.openLogId !== id) panel.innerHTML = "";
  }, 200);
}

async function copyJobLog(id) {
  const lines = state.jobLogs.get(id) || [];
  const text = lines.join("\n");
  try {
    await navigator.clipboard.writeText(text);
    flashToast(t("log.copied"));
  } catch (e) {
    console.error("clipboard", e);
    flashToast(t("log.copy_failed", { err: e }));
  }
}

async function saveJobLog(id) {
  const lines = state.jobLogs.get(id) || [];
  const job = state.jobs.get(id);
  const stamp = new Date().toISOString().replace(/[:.]/g, "-");
  const suggested = `${(job?.title || job?.id || "log").replace(/[\\/:*?"<>|]/g, "_")}_${stamp}.log`;
  try {
    const saved = await invoke("save_log_file", { suggested, contents: lines.join("\n") });
    if (saved) flashToast(t("log.saved", { path: saved }));
  } catch (e) {
    console.error("save_log_file", e);
    flashToast(t("log.save_failed", { err: e }));
  }
}

function refreshQueueStatus() {
  const jobs = [...state.jobs.values()];
  const total = jobs.length;
  const done = jobs.filter((j) => normalizeState(j.state) === "ok").length;
  const running = jobs.filter((j) => normalizeState(j.state) === "run").length;
  const queued = jobs.filter((j) => normalizeState(j.state) === "queued").length;
  const failed = jobs.filter((j) => normalizeState(j.state) === "err").length;
  $("queueStatus").textContent = total ? t("app.queue", { done, total }) : t("app.idle");

  const summary = $("queueSummary");
  if (summary) {
    const parts = [];
    if (running) parts.push(t("summary.running", { n: running }));
    if (queued) parts.push(t("summary.queued", { n: queued }));
    if (done) parts.push(t("summary.done", { n: done }));
    if (failed) parts.push(t("summary.failed", { n: failed }));
    summary.textContent = parts.join(" · ");
  }
}

// ── Row actions ──────────────────────────────────────────────
function wireQueueActions() {
  // Double-click the title of a completed row → open the file with its default app.
  $("queueList").addEventListener("dblclick", (e) => {
    const titleZone = e.target.closest(".ytp-row__title-wrap, .ytp-row__title");
    if (!titleZone) return;
    const row = titleZone.closest(".ytp-row");
    if (!row) return;
    const id = row.dataset.id;
    const job = state.jobs.get(id);
    if (!job) return;
    if (normalizeState(job.state) !== "ok") return;
    const dest = state.jobDests.get(id);
    if (!dest) {
      flashToast(t("log.empty"));
      return;
    }
    invoke("open_file", { path: dest }).catch((err) => console.error("open_file", err));
  });

  $("queueList").addEventListener("click", (e) => {
    const btn = e.target.closest("button[data-act]");
    if (!btn) return;
    const row = btn.closest(".ytp-row");
    if (!row) return;
    const id = row.dataset.id;
    const act = btn.dataset.act;
    if (act === "cancel") {
      invoke("cancel_job", { id });
    } else if (act === "remove") {
      invoke("remove_job", { id });
      state.jobs.delete(id);
      row.remove();
      refreshEmpty();
      refreshQueueStatus();
    } else if (act === "retry") {
      invoke("retry_job", { id });
    } else if (act === "open") {
      const job = state.jobs.get(id);
      const dest = state.jobDests.get(id) || job?.outputDir || state.settings.outputDir;
      if (dest) invoke("open_in_explorer", { path: dest });
    } else if (act === "log") {
      if (state.openLogId === id) {
        state.openLogId = null;
        closeLogPanel(id);
      } else {
        const prev = state.openLogId;
        state.openLogId = id;
        if (prev) closeLogPanel(prev);
        renderLogPanel(id);
      }
    }
  });
  $("queueList").addEventListener("click", (e) => {
    const btn = e.target.closest("button[data-log-act]");
    if (!btn) return;
    const group = btn.closest(".ytp-row-group");
    if (!group) return;
    const id = group.dataset.groupId;
    const act = btn.dataset.logAct;
    if (act === "copy") copyJobLog(id);
    else if (act === "save") saveJobLog(id);
    else if (act === "close") {
      state.openLogId = null;
      closeLogPanel(id);
    }
  });
  $("clearDoneBtn")?.addEventListener("click", async () => {
    await invoke("clear_completed");
    for (const [id, j] of [...state.jobs]) {
      if (normalizeState(j.state) === "ok") {
        state.jobs.delete(id);
        $("queueList").querySelector(`[data-id="${CSS.escape(id)}"]`)?.remove();
      }
    }
    refreshEmpty();
    refreshQueueStatus();
  });
  $("clearAllBtn")?.addEventListener("click", async () => {
    for (const id of [...state.jobs.keys()]) {
      await invoke("remove_job", { id });
    }
    state.jobs.clear();
    $("queueList").innerHTML = "";
    refreshEmpty();
    refreshQueueStatus();
  });
}

// ── Add ──────────────────────────────────────────────────────
function parseUrls(text) {
  return (text || "")
    .split(/[\r\n]+/)
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
}

// `watch?v=X&list=Y` = single video with playlist context — treat as single,
// `--no-playlist` already on yt-dlp args. Only the standalone `/playlist?list=Y`
// form triggers the banner so a force-appended list= can't snowball into a
// 200-video download.
function isStrictPlaylistUrl(url) {
  try {
    const u = new URL(url);
    const hasList = u.searchParams.has("list");
    const hasV = u.searchParams.has("v");
    return hasList && !hasV && /\/playlist\b/.test(u.pathname);
  } catch {
    return false;
  }
}

function stripPlaylistParam(url) {
  try {
    const u = new URL(url);
    u.searchParams.delete("list");
    u.searchParams.delete("index");
    u.searchParams.delete("pp");
    return u.toString();
  } catch {
    return url;
  }
}

async function enqueueRaw(urls) {
  if (!urls.length) return;
  try {
    const result = await invoke("add_jobs", {
      urls,
      format: state.settings.defaultFormat || "1080p",
      outputDir: state.settings.outputDir,
      filenameTemplate: state.settings.filenameTemplate || "%(title)s [%(id)s].%(ext)s",
    });
    const skipped = (result || []).filter((r) => r.kind === "skipped");
    if (skipped.length) {
      const list = skipped.map((s) => s.url).join("\n");
      flashToast(
        skipped.length === 1
          ? t("dup.one", { list })
          : t("dup.many", { n: skipped.length, list })
      );
    }
  } catch (e) {
    console.error("add_jobs", e);
    flashToast(t("enqueue.failed", { err: e }));
  }
}

function hasListParam(url) {
  try {
    return new URL(url).searchParams.has("list");
  } catch {
    return false;
  }
}

async function addUrls(urls) {
  if (!urls.length) return;
  if (!state.settings.outputDir) {
    flashToast(t("enqueue.no_dir"));
    return;
  }

  const mode = state.settings.playlistMode || "single";

  // Mode "single": strip list param + enqueue. Never expands a playlist.
  if (mode === "single") {
    return enqueueRaw(urls.map(stripPlaylistParam));
  }

  // Modes "ask" / "all": probe URLs that have a list= param.
  const hasAnyList = urls.some(hasListParam);
  if (!hasAnyList) {
    return enqueueRaw(urls);
  }

  // For multi-paste, probe each list URL one at a time. Single URL is the common path.
  for (const url of urls) {
    if (!hasListParam(url)) {
      await enqueueRaw([url]);
      continue;
    }
    flashToast(t("playlist.probing"));
    let probe;
    try {
      probe = await invoke("probe_playlist", { url });
    } catch (e) {
      console.warn("probe_playlist failed, treating as single URL:", e);
      await enqueueRaw([stripPlaylistParam(url)]);
      continue;
    }
    const isPlaylist =
      probe && probe._type === "playlist" && Array.isArray(probe.entries) && probe.entries.length > 1;
    if (!isPlaylist) {
      await enqueueRaw([url]);
      continue;
    }
    if (mode === "all") {
      const entries = probe.entries.map(entryToUrl).filter(Boolean);
      await enqueueRaw(entries);
    } else {
      // mode === "ask"
      showPlaylistBanner(probe, url);
    }
  }
}

function entryToUrl(e) {
  if (e.url && /^https?:\/\//.test(e.url)) return e.url;
  if (e.ie_key === "Youtube" || e.url || e.id) {
    return `https://www.youtube.com/watch?v=${e.id || e.url}`;
  }
  return null;
}

function showPlaylistBanner(probe, originalUrl) {
  const banners = $("banners");
  const id = `banner-${++state.bannerIds}`;
  const count = probe.entries.length;
  const title = probe.title || "Untitled playlist";
  const div = document.createElement("div");
  div.className = "ytp-banner ytp-banner--info";
  div.id = id;
  const msgHtml = t("playlist.banner", { title: escapeHtml(title), count });
  div.innerHTML = `
    <span class="ytp-banner__icon">
      <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="8" cy="8" r="6"/><path d="M8 7.5v3.5M8 5.2h.01"/></svg>
    </span>
    <span class="ytp-banner__msg">${msgHtml}</span>
    <div class="ytp-banner__actions">
      <button class="ytp-banner__link ytp-banner__link--primary" data-act="first">${escapeHtml(t("playlist.first"))}</button>
      <button class="ytp-banner__link" data-act="all">${escapeHtml(t("playlist.all", { count }))}</button>
      <button class="ytp-banner__link" data-act="cancel">${escapeHtml(t("playlist.cancel"))}</button>
    </div>`;
  banners.appendChild(div);

  const close = () => div.remove();
  div.addEventListener("click", async (e) => {
    const btn = e.target.closest("button[data-act]");
    if (!btn) return;
    const act = btn.dataset.act;
    close();
    if (act === "cancel") return;
    if (act === "first") {
      const first = probe.entries[0];
      const u = entryToUrl(first);
      if (u) await enqueueRaw([u]);
      return;
    }
    if (act === "all") {
      const urls = probe.entries.map(entryToUrl).filter(Boolean);
      await enqueueRaw(urls);
    }
  });
}

let toastTimer = null;
function flashToast(msg) {
  let el = document.getElementById("ytpToast");
  if (!el) {
    el = document.createElement("div");
    el.id = "ytpToast";
    el.className = "ytp-toast";
    document.body.appendChild(el);
  }
  el.textContent = msg;
  el.classList.add("ytp-toast--show");
  if (toastTimer) clearTimeout(toastTimer);
  toastTimer = setTimeout(() => {
    el.classList.remove("ytp-toast--show");
  }, 3500);
}

function autosize(ta) {
  // Reset to base then grow only if content overflows a single 32px line.
  ta.style.height = "32px";
  if (!ta.value) return;
  const sh = ta.scrollHeight;
  if (sh > 32) {
    ta.style.height = Math.min(96, sh) + "px";
  }
}

function wireAdd() {
  const ta = $("urlInput");
  autosize(ta);
  ta.addEventListener("input", () => autosize(ta));
  ta.addEventListener("paste", () => setTimeout(() => autosize(ta), 0));
  $("addBtn").addEventListener("click", () => {
    addUrls(parseUrls(ta.value));
    ta.value = "";
    autosize(ta);
  });
  ta.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      addUrls(parseUrls(ta.value));
      ta.value = "";
      autosize(ta);
    }
  });
}

// ── Job events ───────────────────────────────────────────────
function wireJobEvents() {
  listen("job_added", (e) => {
    const job = e.payload;
    state.jobs.set(job.id, job);
    renderRow(job.id);
  });
  listen("job_meta", (e) => {
    const { id, title, host, rtl } = e.payload;
    const j = state.jobs.get(id);
    if (!j) return;
    if (title) j.title = title;
    if (host) j.host = host;
    if (rtl != null) j.rtl = rtl;
    renderRow(id);
  });
  listen("job_state", (e) => {
    const { id, state: s, errorMsg } = e.payload;
    const j = state.jobs.get(id);
    if (!j) return;
    j.state = s;
    if (errorMsg) j.errorMsg = errorMsg;
    renderRow(id);
  });
  listen("job_progress", (e) => {
    const { id, pct, speed, eta, size } = e.payload;
    const j = state.jobs.get(id);
    if (!j) return;
    j.pct = pct;
    j.speed = speed;
    j.eta = eta;
    if (size) j.size = size;
    renderRow(id);
  });
  listen("job_log", (e) => {
    const { id, line, kind } = e.payload;
    let arr = state.jobLogs.get(id);
    if (!arr) {
      arr = [];
      state.jobLogs.set(id, arr);
    }
    arr.push(`[${kind}] ${line}`);
    if (arr.length > 5000) arr.splice(0, arr.length - 5000);
    if (state.openLogId === id) {
      const group = $("queueList").querySelector(`[data-group-id="${CSS.escape(id)}"]`);
      const body = group?.querySelector("[data-log-body]");
      if (body) {
        const wasAtBottom = body.scrollTop + body.clientHeight >= body.scrollHeight - 8;
        const cls = classifyLine(`[${kind}] ${line}`);
        const div = document.createElement("div");
        div.className = `ytp-log__line${cls ? " ytp-log__line--" + cls : ""}`;
        div.textContent = `[${kind}] ${line}`;
        body.appendChild(div);
        if (wasAtBottom) body.scrollTop = body.scrollHeight;
      }
    }
  });
  listen("job_dest", (e) => {
    const { id, path } = e.payload;
    state.jobDests.set(id, path);
  });
  listen("job_removed", (e) => {
    const { id } = e.payload;
    state.jobs.delete(id);
    $("queueList").querySelector(`[data-id="${CSS.escape(id)}"]`)?.remove();
    refreshEmpty();
    refreshQueueStatus();
  });
}

// ── Window drag ──────────────────────────────────────────────
function wireWindowDrag() {
  const titlebar = document.querySelector(".ytp-titlebar");
  if (!titlebar) return;
  titlebar.addEventListener("mousedown", (e) => {
    if (e.button !== 0) return;
    if (e.target.closest("button")) return;
    invoke("window_start_drag").catch((err) => console.error(err));
  });
  titlebar.addEventListener("dblclick", (e) => {
    if (e.target.closest("button")) return;
    invoke("window_toggle_max");
  });
}

// ── Updater chip ─────────────────────────────────────────────
let lastUpdaterTimer = null;
let lastUpdaterStatus = null;
function setUpdaterChip(status) {
  lastUpdaterStatus = status;
  if (status.version) lastYtdlpVersion = status.version;
  if (document.getElementById("setYtdlpVersion") && status.version) {
    document.getElementById("setYtdlpVersion").textContent = status.version;
  }
  const chip = $("updaterChip");
  const glyph = $("updaterGlyph");
  const text = $("updaterText");
  if (lastUpdaterTimer) {
    clearTimeout(lastUpdaterTimer);
    lastUpdaterTimer = null;
  }
  chip.classList.remove(
    "ytp-statusbar__chip--ok",
    "ytp-statusbar__chip--updated",
    "ytp-statusbar__chip--offline",
    "ytp-statusbar__chip--checking"
  );
  const v = status.version || "";
  if (status.kind === "checking") {
    chip.classList.add("ytp-statusbar__chip--checking");
    glyph.outerHTML = '<i id="updaterGlyph" class="ytp-spin"></i>';
    text.textContent = t("updater.checking");
  } else if (status.kind === "updated") {
    chip.classList.add("ytp-statusbar__chip--updated");
    glyph.outerHTML =
      '<svg id="updaterGlyph" viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M3 8.5l3 3 7-7"/></svg>';
    text.textContent = v ? t("updater.updated", { version: v }) : t("updater.updated_nover");
    lastUpdaterTimer = setTimeout(() => {
      setUpdaterChip({ kind: "up-to-date", version: v });
    }, 6000);
  } else if (status.kind === "offline") {
    chip.classList.add("ytp-statusbar__chip--offline");
    glyph.outerHTML =
      '<svg id="updaterGlyph" viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M8 2.5L1.5 13.5h13z"/><path d="M8 6.5v3M8 11.5h.01"/></svg>';
    text.textContent = v ? t("updater.offline", { version: v }) : t("updater.offline_nover");
  } else {
    chip.classList.add("ytp-statusbar__chip--ok");
    glyph.outerHTML = '<span id="updaterGlyph" class="ytp-statusbar__dot"></span>';
    text.textContent = v ? t("updater.uptodate", { version: v }) : t("updater.uptodate_nover");
  }
}

function wireUpdater() {
  setUpdaterChip({ kind: "checking" });
  listen("updater_status", (e) => setUpdaterChip(e.payload)).catch((err) =>
    console.error("listen", err)
  );
  $("updaterChip").addEventListener("click", () => {
    invoke("check_yt_dlp_update").catch((e) => console.error(e));
  });
}

// ── Banners ──────────────────────────────────────────────────
function dismissBanner(id) {
  document.getElementById(id)?.remove();
}

function ensureBanner(id, kind, html) {
  const banners = $("banners");
  let el = document.getElementById(id);
  if (!el) {
    el = document.createElement("div");
    el.id = id;
    el.className = `ytp-banner ytp-banner--${kind}`;
    el.innerHTML = html;
    banners.appendChild(el);
  } else {
    el.className = `ytp-banner ytp-banner--${kind}`;
    el.innerHTML = html;
  }
  return el;
}

const ALERT_SVG = '<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M8 2.5L1.5 13.5h13z"/><path d="M8 6.5v3M8 11.5h.01"/></svg>';

async function checkFfmpeg() {
  try {
    const s = await invoke("ffmpeg_status");
    if (!s.exists) {
      ensureBanner(
        "banner-ffmpeg",
        "err",
        `<span class="ytp-banner__icon">${ALERT_SVG}</span>
         <span class="ytp-banner__msg">${t("ffmpeg.missing.msg")}</span>
         <div class="ytp-banner__actions">
           <button class="ytp-banner__link" data-act="reveal" data-path="${escapeAttr(s.dir || "")}">${escapeHtml(t("ffmpeg.reveal"))}</button>
           <button class="ytp-banner__link" data-act="recheck">${escapeHtml(t("ffmpeg.recheck"))}</button>
         </div>`
      );
    } else {
      dismissBanner("banner-ffmpeg");
    }
  } catch (e) {
    console.error("ffmpeg_status", e);
  }
}

async function checkOutputDir() {
  const dir = state.settings.outputDir;
  if (!dir) {
    dismissBanner("banner-folder-gone");
    return;
  }
  try {
    const exists = await invoke("path_exists", { path: dir });
    if (!exists) {
      ensureBanner(
        "banner-folder-gone",
        "warn",
        `<span class="ytp-banner__icon">${ALERT_SVG}</span>
         <span class="ytp-banner__msg">${t("folder.gone.msg", { dir: escapeHtml(dir) })}</span>
         <div class="ytp-banner__actions">
           <button class="ytp-banner__link" data-act="choose">${escapeHtml(t("folder.gone.choose"))}</button>
           <button class="ytp-banner__link" data-act="dismiss">${escapeHtml(t("folder.gone.dismiss"))}</button>
         </div>`
      );
    } else {
      dismissBanner("banner-folder-gone");
    }
  } catch (e) {
    console.error("path_exists", e);
  }
}

function wireBannerActions() {
  $("banners").addEventListener("click", async (e) => {
    const btn = e.target.closest("button[data-act]");
    if (!btn) return;
    const act = btn.dataset.act;
    if (act === "reveal") {
      const path = btn.dataset.path;
      if (path) invoke("open_in_explorer", { path });
    } else if (act === "recheck") {
      await checkFfmpeg();
      flashToast(t("ffmpeg.rechecked"));
    } else if (act === "choose") {
      try {
        const path = await invoke("pick_folder");
        if (path) {
          setOutputDir(path);
          dismissBanner("banner-folder-gone");
        }
      } catch (err) {
        console.error(err);
      }
    } else if (act === "dismiss") {
      btn.closest(".ytp-banner")?.remove();
    }
  });
}

// ── Drag-drop ────────────────────────────────────────────────
function wireDragDrop() {
  let dragDepth = 0;
  window.addEventListener("dragenter", (e) => {
    e.preventDefault();
    dragDepth++;
    root.classList.add("ytp-drag-over");
  });
  window.addEventListener("dragleave", (e) => {
    e.preventDefault();
    dragDepth--;
    if (dragDepth <= 0) {
      dragDepth = 0;
      root.classList.remove("ytp-drag-over");
    }
  });
  window.addEventListener("dragover", (e) => {
    e.preventDefault();
  });
  window.addEventListener("drop", async (e) => {
    e.preventDefault();
    dragDepth = 0;
    root.classList.remove("ytp-drag-over");
    const items = e.dataTransfer?.files;
    if (!items || !items.length) return;
    const txtFiles = [...items].filter((f) => /\.(txt|csv)$/i.test(f.name));
    if (!txtFiles.length) {
      flashToast(t("drop.wrong_type"));
      return;
    }
    let combined = [];
    for (const f of txtFiles) {
      const text = await f.text();
      combined = combined.concat(parseUrls(text));
    }
    if (!combined.length) {
      flashToast(t("drop.no_urls"));
      return;
    }
    flashToast(t("drop.add", { n: combined.length, s: combined.length === 1 ? "" : "s", name: txtFiles[0].name }));
    addUrls(combined);
  });
}

// ── Keyboard shortcuts ───────────────────────────────────────
function wireKeyboard() {
  document.addEventListener("keydown", (e) => {
    // Esc — close settings panel, log panel, dropdowns, and dismissible banners
    if (e.key === "Escape") {
      let did = false;
      const panel = $("settingsPanel");
      if (panel && !panel.hidden) {
        closeSettings();
        did = true;
      }
      if (state.openLogId) {
        const id = state.openLogId;
        state.openLogId = null;
        closeLogPanel(id);
        did = true;
      }
      for (const menuId of ["formatMenu", "playlistMenu", "folderMenu"]) {
        const m = $(menuId);
        if (m && !m.hidden) {
          m.hidden = true;
          did = true;
        }
      }
      // Close playlist confirmation banners
      document
        .querySelectorAll("#banners .ytp-banner--info")
        .forEach((b) => {
          if (b.id?.startsWith("banner-")) {
            // Only auto-dismiss playlist-style info banners (the ones created with `banner-N` id),
            // leave persistent status banners (ffmpeg, folder-gone) intact.
            if (/^banner-\d+$/.test(b.id)) {
              b.remove();
              did = true;
            }
          }
        });
      if (did) e.preventDefault();
      return;
    }

  });

  // Window-level paste handler — catches Ctrl+V (or right-click paste) when
  // focus isn't already in an input/textarea. More reliable than reading the
  // clipboard async; uses the synchronous ClipboardEvent data.
  window.addEventListener("paste", (e) => {
    const ae = document.activeElement;
    if (ae && (ae.tagName === "INPUT" || ae.tagName === "TEXTAREA")) return; // native paste
    const text = e.clipboardData?.getData("text");
    if (!text) return;
    const ta = $("urlInput");
    ta.focus();
    ta.value = (ta.value ? ta.value + "\n" : "") + text;
    autosize(ta);
    e.preventDefault();
  });
}

// ── Settings panel ───────────────────────────────────────────
let lastYtdlpVersion = null;

function setSegOn(container, key, value) {
  container.querySelectorAll(`button[data-${key}]`).forEach((b) => {
    b.setAttribute("data-on", b.dataset[key] === value);
  });
}

async function refreshSettingsFfmpeg() {
  try {
    const s = await invoke("ffmpeg_status");
    const glyph = $("setFfmpegGlyph");
    const status = $("setFfmpegStatus");
    const path = $("setFfmpegPath");
    if (s.exists) {
      status.textContent = t("set.updates.ffmpeg.found");
      path.textContent = s.path || "";
      glyph.innerHTML =
        '<span style="color:var(--ok)"><svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M3 8.5l3 3 7-7"/></svg></span>';
    } else {
      status.textContent = t("set.updates.ffmpeg.missing");
      path.textContent = `${s.dir || ""}\\ffmpeg.exe`;
      glyph.innerHTML =
        '<span style="color:var(--err)"><svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M4 4l8 8M12 4l-8 8"/></svg></span>';
    }
  } catch (e) {
    console.error("ffmpeg_status", e);
  }
}

const TOGGLE_FIELDS = {
  setWriteSubs: "writeSubtitles",
  setSponsorBlock: "sponsorBlock",
  setEmbedThumbnail: "embedThumbnail",
  setEmbedMetadata: "embedMetadata",
  setEmbedChapters: "embedChapters",
  setRestrictFilenames: "restrictFilenames",
  setWriteThumbnail: "writeThumbnail",
  setWriteInfoJson: "writeInfoJson",
  setWriteDescription: "writeDescription",
  setKeepVideo: "keepVideo",
};

function openSettings() {
  const panel = $("settingsPanel");
  $("setOutputDirText").textContent = state.settings.outputDir || "—";
  $("setFilenameTemplate").value =
    state.settings.filenameTemplate || "%(title)s [%(id)s].%(ext)s";
  $("setConcurrency").value = state.settings.concurrency || 3;
  $("setDefaultFormat").value = state.settings.defaultFormat || "1080p";
  setSegOn($("setPlaylistMode"), "pl", state.settings.playlistMode || "single");
  $("setAutoClear").value = String(state.settings.autoClearDays ?? 0);
  setSegOn($("setTheme"), "theme", state.settings.theme || "system");
  $("setRememberHistory").setAttribute(
    "data-on",
    state.settings.rememberHistory !== false ? "true" : "false"
  );

  for (const [elId, key] of Object.entries(TOGGLE_FIELDS)) {
    $(elId).setAttribute("data-on", state.settings[key] === true ? "true" : "false");
  }
  $("setSubLangs").value = state.settings.subtitleLangs || "en.*";
  $("setCookiesFrom").value = state.settings.cookiesFromBrowser || "";
  $("setRateLimit").value = state.settings.rateLimit || "";
  $("setRetries").value = state.settings.retries ?? 10;
  $("setExtraArgs").value = state.settings.extraArgs || "";

  $("setYtdlpVersion").textContent = lastYtdlpVersion || t("set.updates.ffmpeg.checking");
  $("setLocale").value = state.settings.locale || "en";
  refreshSettingsFfmpeg();
  panel.hidden = false;
}

function closeSettings() {
  $("settingsPanel").hidden = true;
}

function wireSettings() {
  $("settingsBtn").addEventListener("click", openSettings);
  $("settingsClose").addEventListener("click", closeSettings);
  // Click on scrim (panel background) but not the inner panel → close
  $("settingsPanel").addEventListener("click", (e) => {
    if (e.target.id === "settingsPanel") closeSettings();
  });

  $("setOutputDirBtn").addEventListener("click", async () => {
    try {
      const path = await invoke("pick_folder");
      if (path) {
        setOutputDir(path);
        $("setOutputDirText").textContent = path;
      }
    } catch (e) {
      console.error(e);
    }
  });

  $("setFilenameTemplate").addEventListener("change", (e) => {
    persistSetting({ filenameTemplate: e.target.value });
  });

  $("setConcurrency").addEventListener("change", (e) => {
    let n = parseInt(e.target.value, 10);
    if (isNaN(n)) n = 3;
    n = Math.max(1, Math.min(8, n));
    e.target.value = n;
    persistSetting({ concurrency: n });
    invoke("set_concurrency", { n });
  });

  $("setDefaultFormat").addEventListener("change", (e) => {
    setFormat(e.target.value);
  });

  $("setPlaylistMode").addEventListener("click", (e) => {
    const b = e.target.closest("button[data-pl]");
    if (!b) return;
    setPlaylistMode(b.dataset.pl);
    setSegOn($("setPlaylistMode"), "pl", b.dataset.pl);
  });

  $("setAutoClear").addEventListener("change", (e) => {
    persistSetting({ autoClearDays: parseInt(e.target.value, 10) || 0 });
  });

  $("setTheme").addEventListener("click", (e) => {
    const b = e.target.closest("button[data-theme]");
    if (!b) return;
    state.settings.theme = b.dataset.theme;
    setSegOn($("setTheme"), "theme", b.dataset.theme);
    applyTheme();
    persistSetting({ theme: b.dataset.theme });
  });

  $("setRememberHistory").addEventListener("click", (e) => {
    const cur = e.currentTarget.getAttribute("data-on") === "true";
    const next = !cur;
    e.currentTarget.setAttribute("data-on", next ? "true" : "false");
    persistSetting({ rememberHistory: next });
  });

  $("setCheckUpdate").addEventListener("click", () => {
    invoke("check_yt_dlp_update").catch((e) => console.error(e));
  });

  // yt-dlp option toggles
  for (const [elId, key] of Object.entries(TOGGLE_FIELDS)) {
    $(elId).addEventListener("click", (e) => {
      const cur = e.currentTarget.getAttribute("data-on") === "true";
      const next = !cur;
      e.currentTarget.setAttribute("data-on", next ? "true" : "false");
      persistSetting({ [key]: next });
    });
  }
  $("setSubLangs").addEventListener("change", (e) =>
    persistSetting({ subtitleLangs: e.target.value })
  );
  $("setCookiesFrom").addEventListener("change", (e) =>
    persistSetting({ cookiesFromBrowser: e.target.value })
  );
  $("setRateLimit").addEventListener("change", (e) =>
    persistSetting({ rateLimit: e.target.value })
  );
  $("setRetries").addEventListener("change", (e) => {
    let n = parseInt(e.target.value, 10);
    if (isNaN(n)) n = 10;
    n = Math.max(0, Math.min(100, n));
    e.target.value = n;
    persistSetting({ retries: n });
  });
  $("setExtraArgs").addEventListener("change", (e) =>
    persistSetting({ extraArgs: e.target.value })
  );

  $("setLocale").addEventListener("change", (e) => {
    state.settings.locale = e.target.value;
    persistSetting({ locale: e.target.value });
    applyLocale();
  });
}

// ── Init ─────────────────────────────────────────────────────
async function init() {
  try {
    state.settings = await invoke("get_settings");
  } catch (e) {
    console.error("get_settings failed", e);
    state.settings = {};
  }
  applyTheme();
  applyLocale();

  // Restore format + folder UI
  const fmt = state.settings.defaultFormat || "1080p";
  $("formatValue").textContent = formatLabel(fmt);
  $("formatMenu")
    .querySelectorAll("button[data-fmt]")
    .forEach((b) => b.setAttribute("data-on", b.dataset.fmt === fmt));
  const dir = state.settings.outputDir || "";
  $("folderPath").textContent = dir || "—";
  $("outputPathText").textContent = dir || "—";
  renderFolderRecent();

  const plMode = state.settings.playlistMode || "single";
  $("playlistValue").textContent = plMode === "single" ? "off" : plMode;
  $("playlistMenu")
    .querySelectorAll("button[data-pl]")
    .forEach((b) => b.setAttribute("data-on", b.dataset.pl === plMode));

  matchMedia("(prefers-color-scheme: dark)").addEventListener("change", (e) => {
    state.systemTheme = e.matches ? "dark" : "light";
    applyTheme();
  });

  wireWindowDrag();
  wireFormat();
  wirePlaylistMode();
  wireFolder();
  wireAdd();
  wireQueueActions();
  wireJobEvents();
  wireUpdater();
  wireBannerActions();
  wireDragDrop();
  wireKeyboard();
  wireSettings();
  checkFfmpeg();
  checkOutputDir();

  $("winMin").addEventListener("click", () => invoke("window_minimize"));
  $("winMax").addEventListener("click", () => invoke("window_toggle_max"));
  $("winClose").addEventListener("click", () => invoke("window_close"));

  refreshEmpty();
}

init();
