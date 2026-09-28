mod audio;

#[cfg(target_os = "macos")]
mod panel;

use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

use chrono::{DateTime, FixedOffset, Local, TimeDelta};
use rundi_core::{
    apply_pause, apply_snooze, clear_pause, config_file_path, end_of_local_day, evaluate,
    glasses_on, load_config, load_state, mark_shown, record_glass, save_config, save_state,
    state_file_path, weekday_code, AppConfig, DailyCount, Decision, RuntimeState,
};
use serde::{Deserialize, Serialize};
use tauri::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::window::Color;
use tauri::{
    AppHandle, Emitter, Manager, RunEvent, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};
use tauri_plugin_autostart::ManagerExt;

#[cfg(target_os = "macos")]
const TRAY_ICON: &[u8] = include_bytes!("../icons/trayTemplate.png");
#[cfg(not(target_os = "macos"))]
const TRAY_ICON: &[u8] = include_bytes!("../icons/32x32.png");

struct Inner {
    config: AppConfig,
    runtime: RuntimeState,
    daily: DailyCount,
    config_path: std::path::PathBuf,
    state_path: std::path::PathBuf,
    force_show: bool,
    config_warning: Option<String>,
    active: Option<ReminderPayload>,
    token: u64,
    shutdown: bool,
}

struct InnerSync {
    mu: Mutex<Inner>,
    cv: Condvar,
}

#[derive(Clone)]
struct AppState {
    inner: Arc<InnerSync>,
}

impl AppState {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.mu.lock().unwrap_or_else(|err| err.into_inner())
    }

    fn wake(&self) {
        self.inner.cv.notify_all();
    }

    fn shutdown(&self) {
        {
            let mut guard = self.lock();
            guard.shutdown = true;
        }
        self.wake();
    }

    fn snapshot(&self) -> Snapshot {
        let guard = self.lock();
        snapshot_from(&guard)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReminderPayload {
    token: u64,
    auto_dismiss_seconds: u32,
    snooze_minutes: u32,
    glasses_today: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    workdays: Vec<String>,
    start: String,
    end: String,
    interval_minutes: u32,
    sound_enabled: bool,
    volume: f32,
    auto_dismiss_seconds: u32,
    snooze_minutes: u32,
    launch_at_login: bool,
    glasses_today: u32,
    paused: bool,
    paused_until: Option<String>,
    config_path: String,
    config_warning: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsInput {
    workdays: Vec<String>,
    start: String,
    end: String,
    interval_minutes: u32,
    sound_enabled: bool,
    volume: f32,
    auto_dismiss_seconds: u32,
    snooze_minutes: u32,
    launch_at_login: bool,
}

fn local_now() -> DateTime<FixedOffset> {
    Local::now().fixed_offset()
}

fn snapshot_from(guard: &Inner) -> Snapshot {
    let now = local_now();
    let paused_until = guard.runtime.paused_until.filter(|until| now < *until);
    Snapshot {
        workdays: guard
            .config
            .schedule
            .workdays
            .iter()
            .copied()
            .map(weekday_code)
            .map(str::to_string)
            .collect(),
        start: format_hhmm(guard.config.schedule.start),
        end: format_hhmm(guard.config.schedule.end),
        interval_minutes: guard.config.schedule.interval_minutes(),
        sound_enabled: guard.config.sound_enabled,
        volume: guard.config.volume,
        auto_dismiss_seconds: guard.config.auto_dismiss_seconds,
        snooze_minutes: guard.config.snooze_minutes,
        launch_at_login: guard.config.launch_at_login,
        glasses_today: glasses_on(&guard.daily, now.date_naive()),
        paused: paused_until.is_some(),
        paused_until: paused_until.map(|time| time.to_rfc3339()),
        config_path: guard.config_path.display().to_string(),
        config_warning: guard.config_warning.clone(),
    }
}

fn format_hhmm(time: chrono::NaiveTime) -> String {
    use chrono::Timelike;
    format!("{:02}:{:02}", time.hour(), time.minute())
}

fn load_inner() -> (Inner, bool) {
    let config_path = config_file_path();
    let state_path = state_file_path();
    let first_launch = !config_path.exists();
    let (config, mut warning) = match load_config(&config_path) {
        Ok(config) => (config, None),
        Err(err) => (
            AppConfig::default(),
            Some(format!("配置没有读出来，先用默认值。{err}")),
        ),
    };
    if first_launch {
        if let Err(err) = save_config(&config_path, &config) {
            warning = Some(format!("没能写下默认配置：{err}"));
        }
    }
    let (runtime, daily) = match load_state(&state_path) {
        Ok(pair) => pair,
        Err(err) => {
            warning = Some(format!("状态没有读出来，今日杯数从 0 开始。{err}"));
            (RuntimeState::default(), DailyCount::default())
        }
    };
    (
        Inner {
            config,
            runtime,
            daily,
            config_path,
            state_path,
            force_show: false,
            config_warning: warning,
            active: None,
            token: 0,
            shutdown: false,
        },
        first_launch,
    )
}

fn persist(guard: &Inner) -> Result<(), String> {
    save_state(&guard.state_path, &guard.runtime, &guard.daily).map_err(|err| err.to_string())
}

fn sync_autostart(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let auto = app.autolaunch();
    if enabled {
        auto.enable().map_err(|err| err.to_string())
    } else {
        auto.disable().map_err(|err| err.to_string())
    }
}

fn publish(app: &AppHandle, state: &AppState) {
    state.wake();
    refresh_tray(app, state);
    let _ = app.emit("snapshot", state.snapshot());
}

fn hide_reminder(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("reminder") {
        let _ = window.hide();
    }
}

fn show_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn present_reminder(app: &AppHandle, payload: &ReminderPayload) {
    let Some(window) = app.get_webview_window("reminder") else {
        eprintln!("润滴：提醒窗口还没建好");
        return;
    };
    let _ = window.emit("reminder", payload);
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.center();
    let _ = window.set_always_on_top(true);
    #[cfg(target_os = "macos")]
    panel::raise_above_fullscreen(&window);
    let _ = window.set_focus();
}

fn build_menu(app: &AppHandle, state: &AppState) -> tauri::Result<Menu<tauri::Wry>> {
    let snap = state.snapshot();
    let count = MenuItem::with_id(
        app,
        "count",
        format!("今日已喝 {} 杯", snap.glasses_today),
        false,
        None::<&str>,
    )?;
    let settings = MenuItem::with_id(app, "settings", "打开设置", true, None::<&str>)?;
    let now = MenuItem::with_id(app, "now", "立即提醒", true, None::<&str>)?;
    let sep_a = PredefinedMenuItem::separator(app)?;
    let sep_b = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

    if snap.paused {
        let resume = MenuItem::with_id(app, "resume", "继续提醒", true, None::<&str>)?;
        Menu::with_items(
            app,
            &[&count, &sep_a, &settings, &now, &resume, &sep_b, &quit],
        )
    } else {
        let hour = MenuItem::with_id(app, "pause-hour", "暂停 1 小时", true, None::<&str>)?;
        let today = MenuItem::with_id(app, "pause-today", "今日暂停", true, None::<&str>)?;
        Menu::with_items(
            app,
            &[
                &count, &sep_a, &settings, &now, &hour, &today, &sep_b, &quit,
            ],
        )
    }
}

fn refresh_tray(app: &AppHandle, state: &AppState) {
    let Some(tray) = app.tray_by_id("main") else {
        return;
    };
    match build_menu(app, state) {
        Ok(menu) => {
            if let Err(err) = tray.set_menu(Some(menu)) {
                eprintln!("润滴：更新托盘菜单失败：{err}");
            }
        }
        Err(err) => eprintln!("润滴：生成托盘菜单失败：{err}"),
    }
}

fn on_menu(app: &AppHandle, event: MenuEvent) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let result = match event.id.as_ref() {
        "settings" => {
            show_settings(app);
            Ok(())
        }
        "now" => queue_show(&state),
        "pause-hour" => pause_for(&state, 60),
        "pause-today" => pause_for_today(&state),
        "resume" => resume_pause(&state),
        "quit" => {
            state.shutdown();
            app.exit(0);
            Ok(())
        }
        _ => Ok(()),
    };
    if let Err(err) = result {
        eprintln!("润滴：菜单操作失败：{err}");
        return;
    }
    if event.id.as_ref() != "quit" {
        hide_if_pausing(app, event.id.as_ref());
        publish(app, &state);
    }
}

fn hide_if_pausing(app: &AppHandle, id: &str) {
    if matches!(id, "pause-hour" | "pause-today" | "resume") {
        hide_reminder(app);
    }
}

fn queue_show(state: &AppState) -> Result<(), String> {
    {
        let mut guard = state.lock();
        guard.force_show = true;
    }
    state.wake();
    Ok(())
}

fn pause_for(state: &AppState, minutes: i64) -> Result<(), String> {
    let mut guard = state.lock();
    let now = local_now();
    let previous = guard.runtime.clone();
    apply_pause(&mut guard.runtime, now + TimeDelta::minutes(minutes));
    guard.active = None;
    if let Err(err) = persist(&guard) {
        guard.runtime = previous;
        return Err(err);
    }
    Ok(())
}

fn pause_for_today(state: &AppState) -> Result<(), String> {
    let mut guard = state.lock();
    let now = local_now();
    let previous = guard.runtime.clone();
    apply_pause(&mut guard.runtime, end_of_local_day(now));
    guard.active = None;
    if let Err(err) = persist(&guard) {
        guard.runtime = previous;
        return Err(err);
    }
    Ok(())
}

fn resume_pause(state: &AppState) -> Result<(), String> {
    let mut guard = state.lock();
    let previous = guard.runtime.clone();
    clear_pause(&mut guard.runtime);
    if let Err(err) = persist(&guard) {
        guard.runtime = previous;
        return Err(err);
    }
    Ok(())
}

fn wait_duration(now: DateTime<FixedOffset>, target: DateTime<FixedOffset>) -> Duration {
    let delta = target.signed_duration_since(now);
    if delta <= TimeDelta::zero() {
        return Duration::from_millis(400);
    }
    delta
        .to_std()
        .unwrap_or(Duration::from_secs(30))
        .min(Duration::from_secs(30))
        .max(Duration::from_millis(200))
}

fn scheduler_loop(app: AppHandle, sync: Arc<InnerSync>) {
    let mut guard = sync.mu.lock().unwrap_or_else(|err| err.into_inner());
    loop {
        if guard.shutdown {
            break;
        }
        let now = local_now();
        let should_show = if guard.force_show {
            guard.force_show = false;
            true
        } else {
            matches!(
                evaluate(now, &guard.config.schedule, &guard.runtime),
                Decision::Show
            )
        };

        if should_show {
            mark_shown(&mut guard.runtime, now);
            guard.token = guard.token.saturating_add(1);
            let payload = ReminderPayload {
                token: guard.token,
                auto_dismiss_seconds: guard.config.auto_dismiss_seconds,
                snooze_minutes: guard.config.snooze_minutes,
                glasses_today: glasses_on(&guard.daily, now.date_naive()),
            };
            guard.active = Some(payload.clone());
            let sound = guard.config.sound_enabled;
            let volume = guard.config.volume;
            if let Err(err) = save_state(&guard.state_path, &guard.runtime, &guard.daily) {
                eprintln!("润滴：保存提醒状态失败：{err}");
            }
            drop(guard);
            if sound {
                audio::play_drop(volume);
            }
            present_reminder(&app, &payload);
            guard = sync.mu.lock().unwrap_or_else(|err| err.into_inner());
            continue;
        }

        let target = match evaluate(now, &guard.config.schedule, &guard.runtime) {
            Decision::WaitUntil(target) => target,
            Decision::Show => now + TimeDelta::milliseconds(400),
        };
        let wait = wait_duration(now, target);
        let (next, _) = sync
            .cv
            .wait_timeout(guard, wait)
            .unwrap_or_else(|err| err.into_inner());
        guard = next;
    }
}

fn keep_in_background(window: &WebviewWindow) {
    let twin = window.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = twin.hide();
        }
    });
}

fn install_windows(app: &AppHandle) -> tauri::Result<()> {
    let settings = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("index.html".into()))
        .title("润滴设置")
        .inner_size(460.0, 700.0)
        .min_inner_size(400.0, 560.0)
        .resizable(true)
        .visible(false)
        .center()
        .background_color(Color(244, 239, 230, 255))
        .build()?;
    keep_in_background(&settings);

    let reminder = WebviewWindowBuilder::new(app, "reminder", WebviewUrl::App("index.html".into()))
        .title("该喝水啦")
        .inner_size(440.0, 600.0)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .skip_taskbar(true)
        .visible(false)
        .focused(false)
        .center()
        .background_color(Color(0, 0, 0, 0))
        .build()?;
    keep_in_background(&reminder);
    Ok(())
}

fn install_tray(app: &AppHandle, state: &AppState) -> tauri::Result<()> {
    let icon = tauri::image::Image::from_bytes(TRAY_ICON)?;
    let menu = build_menu(app, state)?;
    let mut builder = TrayIconBuilder::with_id("main")
        .icon(icon)
        .tooltip("润滴 · 喝水提醒")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .icon_as_template(cfg!(target_os = "macos"))
        .on_menu_event(|app, event| on_menu(app, event));
    let _ = &mut builder;
    builder.build(app)?;
    Ok(())
}

#[tauri::command]
fn get_snapshot(state: State<AppState>) -> Snapshot {
    state.snapshot()
}

#[tauri::command]
fn active_reminder(state: State<AppState>) -> Option<ReminderPayload> {
    state.lock().active.clone()
}

#[tauri::command]
fn save_settings(
    app: AppHandle,
    state: State<AppState>,
    input: SettingsInput,
) -> Result<Snapshot, String> {
    let config = AppConfig::try_from_parts(
        &input.workdays,
        &input.start,
        &input.end,
        input.interval_minutes,
        input.sound_enabled,
        input.volume,
        input.auto_dismiss_seconds,
        input.snooze_minutes,
        input.launch_at_login,
    )
    .map_err(|err| err.to_string())?;
    {
        let mut guard = state.lock();
        let previous = guard.config.clone();
        guard.config = config.clone();
        if let Err(err) = save_config(&guard.config_path, &guard.config) {
            guard.config = previous;
            return Err(err.to_string());
        }
    }
    let warning = sync_autostart(&app, config.launch_at_login)
        .err()
        .map(|err| format!("开机启动没能更新：{err}"));
    {
        let mut guard = state.lock();
        guard.config_warning = warning;
    }
    publish(&app, &state);
    Ok(state.snapshot())
}

#[tauri::command]
fn drink(app: AppHandle, state: State<AppState>) -> Result<Snapshot, String> {
    {
        let mut guard = state.lock();
        let now = local_now();
        let previous = guard.daily.clone();
        record_glass(&mut guard.daily, now.date_naive());
        guard.active = None;
        if let Err(err) = persist(&guard) {
            guard.daily = previous;
            return Err(err);
        }
    }
    hide_reminder(&app);
    publish(&app, &state);
    Ok(state.snapshot())
}

#[tauri::command]
fn snooze(app: AppHandle, state: State<AppState>) -> Result<Snapshot, String> {
    {
        let mut guard = state.lock();
        let now = local_now();
        let until = now + TimeDelta::minutes(i64::from(guard.config.snooze_minutes));
        let previous = guard.runtime.clone();
        apply_snooze(&mut guard.runtime, until);
        guard.active = None;
        if let Err(err) = persist(&guard) {
            guard.runtime = previous;
            return Err(err);
        }
    }
    hide_reminder(&app);
    publish(&app, &state);
    Ok(state.snapshot())
}

#[tauri::command]
fn dismiss(app: AppHandle, state: State<AppState>) -> Result<Snapshot, String> {
    {
        let mut guard = state.lock();
        guard.active = None;
    }
    hide_reminder(&app);
    publish(&app, &state);
    Ok(state.snapshot())
}

#[tauri::command]
fn pause_hour(app: AppHandle, state: State<AppState>) -> Result<Snapshot, String> {
    pause_for(&state, 60)?;
    hide_reminder(&app);
    publish(&app, &state);
    Ok(state.snapshot())
}

#[tauri::command]
fn pause_today(app: AppHandle, state: State<AppState>) -> Result<Snapshot, String> {
    pause_for_today(&state)?;
    hide_reminder(&app);
    publish(&app, &state);
    Ok(state.snapshot())
}

#[tauri::command]
fn resume(app: AppHandle, state: State<AppState>) -> Result<Snapshot, String> {
    resume_pause(&state)?;
    publish(&app, &state);
    Ok(state.snapshot())
}

#[tauri::command]
fn remind_now(app: AppHandle, state: State<AppState>) -> Result<Snapshot, String> {
    queue_show(&state)?;
    publish(&app, &state);
    Ok(state.snapshot())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .app_name("Rundi")
                .build(),
        )
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_settings(app);
        }))
        .setup(|app| {
            let (inner, first_launch) = load_inner();
            let state = AppState {
                inner: Arc::new(InnerSync {
                    mu: Mutex::new(inner),
                    cv: Condvar::new(),
                }),
            };
            if let Err(err) = sync_autostart(app.handle(), state.lock().config.launch_at_login) {
                state
                    .lock()
                    .config_warning
                    .get_or_insert(format!("开机启动没能同步：{err}"));
            }
            install_windows(app.handle())?;
            install_tray(app.handle(), &state)?;
            let shared = Arc::clone(&state.inner);
            app.manage(state);
            if first_launch {
                show_settings(app.handle());
            }
            let handle = app.handle().clone();
            std::thread::Builder::new()
                .name("rundi-scheduler".into())
                .spawn(move || scheduler_loop(handle, shared))
                .map_err(|err| -> Box<dyn std::error::Error> { Box::new(err) })?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            active_reminder,
            save_settings,
            drink,
            snooze,
            dismiss,
            pause_hour,
            pause_today,
            resume,
            remind_now,
        ])
        .build(tauri::generate_context!())
        .expect("启动润滴失败")
        .run(|app, event| {
            if let RunEvent::ExitRequested { api, code, .. } = event {
                if code.is_none() {
                    api.prevent_exit();
                } else if let Some(state) = app.try_state::<AppState>() {
                    state.shutdown();
                }
            }
        });
}
