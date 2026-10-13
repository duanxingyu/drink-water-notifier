mod audio;

#[cfg(target_os = "macos")]
mod panel;

use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

use chrono::{DateTime, FixedOffset, Local, TimeDelta};
use rundi_core::{
    apply_pause, apply_snooze, clear_history, clear_pause, config_file_path, convert_daily_unit,
    end_of_local_day, evaluate, format_drink_ack, format_drink_prompt, format_intake,
    format_intake_approx, glasses_on, goal_progress, goal_remaining_ml, intake_ml, load_config,
    load_state, mark_shown, merged_history, paced_interval_minutes, record_intake, rollover_daily,
    save_config, save_state, state_file_path, weekday_code, AppConfig, DailyCount, DayIntake,
    Decision, RuntimeState, Schedule,
};
use serde::{Deserialize, Serialize};
use tauri::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::window::Color;
use tauri::{
    AppHandle, Emitter, Manager, RunEvent, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_updater::UpdaterExt;

#[cfg(target_os = "macos")]
const TRAY_ICON: &[u8] = include_bytes!("../icons/trayTemplate.png");
#[cfg(not(target_os = "macos"))]
const TRAY_ICON: &[u8] = include_bytes!("../icons/32x32.png");

struct Inner {
    config: AppConfig,
    runtime: RuntimeState,
    daily: DailyCount,
    history: Vec<DayIntake>,
    config_path: std::path::PathBuf,
    state_path: std::path::PathBuf,
    force_show: bool,
    config_warning: Option<String>,
    active: Option<ReminderPayload>,
    token: u64,
    shutdown: bool,
    update_available: Option<UpdateInfo>,
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
    drink_unit: String,
    drink_amount: u32,
    intake_label: String,
    drink_prompt: String,
    drink_ack: String,
    high_intake_mode: bool,
    desk_goal_ml: u32,
    intake_ml_today: u32,
    goal_label: Option<String>,
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
    respect_chinese_holidays: bool,
    drink_unit: String,
    drink_amount: u32,
    high_intake_mode: bool,
    desk_goal_ml: u32,
    intake_ml_today: u32,
    goal_remaining_ml: u32,
    goal_progress: f32,
    glasses_today: u32,
    intake_label: String,
    unit_label: String,
    paused: bool,
    paused_until: Option<String>,
    config_path: String,
    config_warning: Option<String>,
    app_version: String,
    update_available: Option<UpdateInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateInfo {
    version: String,
    body: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DrinkHistoryDay {
    date: String,
    amount: u32,
    unit: String,
    label: String,
    converted_label: Option<String>,
    is_today: bool,
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
    respect_chinese_holidays: bool,
    drink_unit: String,
    drink_amount: u32,
    high_intake_mode: bool,
    desk_goal_ml: u32,
}

fn local_now() -> DateTime<FixedOffset> {
    Local::now().fixed_offset()
}

fn goal_label_for(intake_ml_today: u32, desk_goal_ml: u32, high_intake: bool) -> Option<String> {
    if !high_intake {
        return None;
    }
    Some(format!(
        "工位目标 {intake_ml_today}/{desk_goal_ml} 毫升"
    ))
}

fn active_schedule(guard: &Inner, now: DateTime<FixedOffset>) -> Schedule {
    let glasses_today = glasses_on(&guard.daily, now.date_naive());
    let today_ml = intake_ml(glasses_today, guard.config.drink_unit);
    let paced = paced_interval_minutes(
        guard.config.schedule.interval_minutes(),
        guard.config.high_intake_mode,
        guard.config.desk_goal_ml,
        today_ml,
        now,
        &guard.config.schedule,
    );
    guard.config.schedule.with_interval_minutes(paced)
}

fn snapshot_from(guard: &Inner) -> Snapshot {
    let now = local_now();
    let paused_until = guard.runtime.paused_until.filter(|until| now < *until);
    let glasses_today = glasses_on(&guard.daily, now.date_naive());
    let intake_ml_today = intake_ml(glasses_today, guard.config.drink_unit);
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
        respect_chinese_holidays: guard.config.schedule.respect_chinese_holidays,
        drink_unit: guard.config.drink_unit.code().to_string(),
        drink_amount: guard.config.drink_amount,
        high_intake_mode: guard.config.high_intake_mode,
        desk_goal_ml: guard.config.desk_goal_ml,
        intake_ml_today,
        goal_remaining_ml: goal_remaining_ml(intake_ml_today, guard.config.desk_goal_ml),
        goal_progress: goal_progress(intake_ml_today, guard.config.desk_goal_ml),
        glasses_today,
        intake_label: format_intake(glasses_today, guard.config.drink_unit),
        unit_label: guard.config.drink_unit.label().to_string(),
        paused: paused_until.is_some(),
        paused_until: paused_until.map(|time| time.to_rfc3339()),
        config_path: guard.config_path.display().to_string(),
        config_warning: guard.config_warning.clone(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        update_available: guard.update_available.clone(),
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
    let (runtime, mut daily, mut history) = match load_state(&state_path) {
        Ok(triple) => triple,
        Err(err) => {
            warning = Some(format!("状态没有读出来，今日计数从 0 开始。{err}"));
            (
                RuntimeState::default(),
                DailyCount::default(),
                Vec::new(),
            )
        }
    };
    let today = local_now().date_naive();
    // 跨日启动时先归档昨日，避免历史页要等今天第一次喝水才出现。
    let mut dirty = rollover_daily(&mut daily, &mut history, today, config.drink_unit);
    // 旧状态没有 countUnit 时，与当前配置单位对齐。
    if daily.unit != config.drink_unit && daily.glasses == 0 {
        daily.unit = config.drink_unit;
    } else if daily.unit != config.drink_unit {
        convert_daily_unit(&mut daily, config.drink_unit);
        dirty = true;
    }
    let inner = Inner {
        config,
        runtime,
        daily,
        history,
        config_path,
        state_path,
        force_show: false,
        config_warning: warning,
        active: None,
        token: 0,
        shutdown: false,
        update_available: None,
    };
    if dirty {
        if let Err(err) = save_state(
            &inner.state_path,
            &inner.runtime,
            &inner.daily,
            &inner.history,
        ) {
            eprintln!("润滴：跨日归档状态失败：{err}");
        }
    }
    (inner, first_launch)
}

fn persist(guard: &Inner) -> Result<(), String> {
    save_state(
        &guard.state_path,
        &guard.runtime,
        &guard.daily,
        &guard.history,
    )
    .map_err(|err| err.to_string())
}

/// 若已跨日，把昨日累计写入 history 并重置今日计数；有变更时落盘。
fn ensure_daily_rolled(guard: &mut Inner) {
    let today = local_now().date_naive();
    let unit = guard.config.drink_unit;
    if rollover_daily(&mut guard.daily, &mut guard.history, today, unit) {
        if let Err(err) = persist(guard) {
            eprintln!("润滴：跨日归档状态失败：{err}");
        }
    }
}

fn history_days_from(guard: &Inner) -> Vec<DrinkHistoryDay> {
    let today = local_now().date_naive();
    let current_unit = guard.config.drink_unit;
    merged_history(&guard.daily, &guard.history, today, current_unit)
        .into_iter()
        .map(|day| {
            let counterpart = current_unit.counterpart_for_display();
            let converted_label = if day.unit == current_unit {
                format_intake_approx(day.amount, day.unit, counterpart)
            } else {
                format_intake_approx(day.amount, day.unit, current_unit)
                    .or_else(|| format_intake_approx(day.amount, day.unit, counterpart))
            };
            DrinkHistoryDay {
                date: day.date.format("%Y-%m-%d").to_string(),
                amount: day.amount,
                unit: day.unit.code().to_string(),
                label: format_intake(day.amount, day.unit),
                converted_label,
                is_today: day.date == today,
            }
        })
        .collect()
}

/// `cargo run` / `tauri dev` 产物路径（`target/debug|release`）。
/// 这类二进制与安装版共用 `%APPDATA%/rundi` 配置；若对它们调用 enable/disable，
/// 会把 Run 注册表写成临时路径，或清掉安装版已经写好的开机启动项。
fn executable_is_cargo_target() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return true;
    };
    let path = exe.to_string_lossy().replace('\\', "/").to_ascii_lowercase();
    path.contains("/target/debug/") || path.contains("/target/release/")
}

fn sync_autostart(app: &AppHandle, enabled: bool) -> Result<(), String> {
    if executable_is_cargo_target() {
        return if enabled {
            Err(
                "当前是开发构建，不会改开机启动注册表（避免覆盖安装版）。请在已安装的 Rundi 里打开并保存。"
                    .into(),
            )
        } else {
            // 配置可以关掉，但不要在这里 delete 注册表，以免误伤安装版的开机项。
            Ok(())
        };
    }
    let auto = app.autolaunch();
    if enabled {
        auto.enable().map_err(|err| err.to_string())?;
        match auto.is_enabled() {
            Ok(true) => Ok(()),
            Ok(false) => Err("已写入注册表，但系统仍显示未启用（可能被任务管理器禁用了启动项）".into()),
            Err(err) => Err(err.to_string()),
        }
    } else {
        auto.disable().map_err(|err| err.to_string())
    }
}

fn publish(app: &AppHandle, state: &AppState) {
    state.wake();
    refresh_tray(app, state);
    let _ = app.emit("snapshot", state.snapshot());
}

/// 保存设置时托盘文案不会变，跳过重建菜单，避免 Windows 上窗口/托盘闪一下。
fn publish_config(app: &AppHandle, state: &AppState) {
    state.wake();
    let _ = app.emit("snapshot", state.snapshot());
}

fn hide_reminder(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("reminder") {
        let _ = window.hide();
    }
}

/// 设置窗按需创建；关掉即销毁，避免隐藏 WebView 长期占内存。
fn ensure_settings_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(window) = app.get_webview_window("settings") {
        return Ok(window);
    }
    let mut builder = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("index.html".into()))
        .title("润滴设置")
        .inner_size(460.0, 700.0)
        .min_inner_size(400.0, 560.0)
        .resizable(true)
        .visible(false)
        .center()
        .background_color(Color(223, 245, 241, 255));
    #[cfg(target_os = "windows")]
    {
        // 降低 WebView2 附属 UI 开销；对功能无影响。
        builder = builder.additional_browser_args("--disable-features=msWebOOUI,msPdfOOUI");
    }
    let settings = builder.build()?;
    Ok(settings)
}

/// 提醒窗首次需要时再创建，平时托盘空闲可不挂 WebView。
fn ensure_reminder_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(window) = app.get_webview_window("reminder") {
        return Ok(window);
    }
    let mut builder = WebviewWindowBuilder::new(app, "reminder", WebviewUrl::App("index.html".into()))
        .title("该喝水啦")
        .inner_size(300.0, 420.0)
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
        .background_color(Color(0, 0, 0, 0));
    #[cfg(target_os = "windows")]
    {
        builder = builder.additional_browser_args("--disable-features=msWebOOUI,msPdfOOUI");
    }
    let reminder = builder.build()?;
    place_reminder_bottom_right(&reminder);
    keep_in_background(&reminder);
    Ok(reminder)
}

fn show_settings(app: &AppHandle) {
    match ensure_settings_window(app) {
        Ok(window) => {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
            // 若窗口已存在（未销毁），通知前端按已保存配置重载。
            let _ = window.emit("settings-opened", ());
        }
        Err(err) => eprintln!("润滴：打开设置失败：{err}"),
    }
}

fn present_reminder(app: &AppHandle, payload: &ReminderPayload) {
    let window = match ensure_reminder_window(app) {
        Ok(window) => window,
        Err(err) => {
            eprintln!("润滴：创建提醒窗口失败：{err}");
            return;
        }
    };
    let _ = window.emit("reminder", payload);
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_always_on_top(true);
    #[cfg(target_os = "macos")]
    panel::raise_above_fullscreen(&window);
    let _ = window.set_focus();
}

/// 默认摆在当前显示器工作区右下角，留一点边距，避免挡正中间视野。
fn place_reminder_bottom_right(window: &WebviewWindow) {
    let Ok(Some(monitor)) = window.current_monitor() else {
        return;
    };
    let Ok(size) = window.outer_size() else {
        return;
    };
    let scale = monitor.scale_factor();
    let work = monitor.work_area();
    let margin = (16.0 * scale).round() as i32;
    let x = work.position.x + work.size.width as i32 - size.width as i32 - margin;
    let y = work.position.y + work.size.height as i32 - size.height as i32 - margin;
    let _ = window.set_position(tauri::PhysicalPosition::new(x.max(work.position.x), y.max(work.position.y)));
}

fn build_menu(app: &AppHandle, state: &AppState) -> tauri::Result<Menu<tauri::Wry>> {
    let snap = state.snapshot();
    let count = MenuItem::with_id(
        app,
        "count",
        format!("今日已喝 {}", snap.intake_label),
        false,
        None::<&str>,
    )?;
    let settings = MenuItem::with_id(app, "settings", "打开设置", true, None::<&str>)?;
    let now = MenuItem::with_id(app, "now", "立即提醒", true, None::<&str>)?;
    let check_update = MenuItem::with_id(app, "check-update", "检查更新", true, None::<&str>)?;
    let sep_a = PredefinedMenuItem::separator(app)?;
    let sep_b = PredefinedMenuItem::separator(app)?;
    let sep_c = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

    let update_item = snap.update_available.as_ref().map(|info| {
        MenuItem::with_id(
            app,
            "install-update",
            format!("更新到 {}", info.version),
            true,
            None::<&str>,
        )
    });

    if snap.paused {
        let resume = MenuItem::with_id(app, "resume", "继续提醒", true, None::<&str>)?;
        if let Some(Ok(update)) = update_item {
            Menu::with_items(
                app,
                &[
                    &count,
                    &sep_a,
                    &settings,
                    &now,
                    &resume,
                    &sep_b,
                    &update,
                    &check_update,
                    &sep_c,
                    &quit,
                ],
            )
        } else {
            Menu::with_items(
                app,
                &[
                    &count,
                    &sep_a,
                    &settings,
                    &now,
                    &resume,
                    &sep_b,
                    &check_update,
                    &sep_c,
                    &quit,
                ],
            )
        }
    } else {
        let hour = MenuItem::with_id(app, "pause-hour", "暂停 1 小时", true, None::<&str>)?;
        let today = MenuItem::with_id(app, "pause-today", "今日暂停", true, None::<&str>)?;
        if let Some(Ok(update)) = update_item {
            Menu::with_items(
                app,
                &[
                    &count,
                    &sep_a,
                    &settings,
                    &now,
                    &hour,
                    &today,
                    &sep_b,
                    &update,
                    &check_update,
                    &sep_c,
                    &quit,
                ],
            )
        } else {
            Menu::with_items(
                app,
                &[
                    &count,
                    &sep_a,
                    &settings,
                    &now,
                    &hour,
                    &today,
                    &sep_b,
                    &check_update,
                    &sep_c,
                    &quit,
                ],
            )
        }
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
        "check-update" => {
            let handle = app.clone();
            tauri::async_runtime::spawn(async move {
                match run_check_update(&handle).await {
                    Ok(Some(info)) => {
                        show_settings(&handle);
                        eprintln!("润滴：发现新版本 {}", info.version);
                    }
                    Ok(None) => {
                        show_settings(&handle);
                        eprintln!("润滴：已是最新版本");
                    }
                    Err(err) => eprintln!("润滴：检查更新失败：{err}"),
                }
            });
            Ok(())
        }
        "install-update" => {
            let handle = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(err) = run_install_update(&handle).await {
                    eprintln!("润滴：安装更新失败：{err}");
                }
            });
            Ok(())
        }
        "quit" => {
            state.shutdown();
            // 提前释放单实例锁，避免退出后立刻再开时被当成第二实例而直接退出。
            tauri_plugin_single_instance::destroy(app);
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
        ensure_daily_rolled(&mut guard);
        let now = local_now();
        let schedule = active_schedule(&guard, now);
        let should_show = if guard.force_show {
            guard.force_show = false;
            true
        } else {
            matches!(evaluate(now, &schedule, &guard.runtime), Decision::Show)
        };

        if should_show {
            mark_shown(&mut guard.runtime, now);
            guard.token = guard.token.saturating_add(1);
            let glasses_today = glasses_on(&guard.daily, now.date_naive());
            let intake_ml_today = intake_ml(glasses_today, guard.config.drink_unit);
            let payload = ReminderPayload {
                token: guard.token,
                auto_dismiss_seconds: guard.config.auto_dismiss_seconds,
                snooze_minutes: guard.config.snooze_minutes,
                glasses_today,
                drink_unit: guard.config.drink_unit.code().to_string(),
                drink_amount: guard.config.drink_amount,
                intake_label: format_intake(glasses_today, guard.config.drink_unit),
                drink_prompt: format_drink_prompt(guard.config.drink_unit).to_string(),
                drink_ack: format_drink_ack(guard.config.drink_unit).to_string(),
                high_intake_mode: guard.config.high_intake_mode,
                desk_goal_ml: guard.config.desk_goal_ml,
                intake_ml_today,
                goal_label: goal_label_for(
                    intake_ml_today,
                    guard.config.desk_goal_ml,
                    guard.config.high_intake_mode,
                ),
            };
            guard.active = Some(payload.clone());
            let sound = guard.config.sound_enabled;
            let volume = guard.config.volume;
            if let Err(err) = save_state(
                &guard.state_path,
                &guard.runtime,
                &guard.daily,
                &guard.history,
            ) {
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

        let target = match evaluate(now, &schedule, &guard.runtime) {
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
    let mut config = AppConfig::try_from_parts(
        &input.workdays,
        &input.start,
        &input.end,
        input.interval_minutes,
        input.sound_enabled,
        input.volume,
        input.auto_dismiss_seconds,
        input.snooze_minutes,
        input.launch_at_login,
        input.respect_chinese_holidays,
        &input.drink_unit,
        input.drink_amount,
    )
    .map_err(|err| err.to_string())?;
    config
        .set_high_intake(input.high_intake_mode, input.desk_goal_ml)
        .map_err(|err| err.to_string())?;
    {
        let mut guard = state.lock();
        let previous_config = guard.config.clone();
        let previous_daily = guard.daily.clone();
        if previous_config.drink_unit != config.drink_unit {
            convert_daily_unit(&mut guard.daily, config.drink_unit);
        }
        guard.config = config.clone();
        if let Err(err) = save_config(&guard.config_path, &guard.config) {
            guard.config = previous_config;
            guard.daily = previous_daily;
            return Err(err.to_string());
        }
        if let Err(err) = persist(&guard) {
            guard.config = previous_config;
            guard.daily = previous_daily;
            let _ = save_config(&guard.config_path, &guard.config);
            return Err(err);
        }
    }
    let warning = sync_autostart(&app, config.launch_at_login)
        .err()
        .map(|err| format!("开机启动没能更新：{err}"));
    {
        let mut guard = state.lock();
        guard.config_warning = warning;
    }
    publish_config(&app, &state);
    Ok(state.snapshot())
}

#[tauri::command]
fn drink(app: AppHandle, state: State<AppState>) -> Result<Snapshot, String> {
    {
        let mut guard = state.lock();
        let now = local_now();
        let previous_daily = guard.daily.clone();
        let previous_history = guard.history.clone();
        let amount = guard.config.drink_amount;
        let unit = guard.config.drink_unit;
        let today = now.date_naive();
        {
            let Inner {
                daily, history, ..
            } = &mut *guard;
            record_intake(daily, history, today, amount, unit);
        }
        guard.active = None;
        if let Err(err) = persist(&guard) {
            guard.daily = previous_daily;
            guard.history = previous_history;
            return Err(err);
        }
    }
    hide_reminder(&app);
    publish(&app, &state);
    Ok(state.snapshot())
}

#[tauri::command]
fn get_drink_history(state: State<AppState>) -> Vec<DrinkHistoryDay> {
    let mut guard = state.lock();
    ensure_daily_rolled(&mut guard);
    history_days_from(&guard)
}

#[tauri::command]
fn clear_drink_history(app: AppHandle, state: State<AppState>) -> Result<Vec<DrinkHistoryDay>, String> {
    {
        let mut guard = state.lock();
        let previous = guard.history.clone();
        clear_history(&mut guard.history);
        if let Err(err) = persist(&guard) {
            guard.history = previous;
            return Err(err);
        }
    }
    publish(&app, &state);
    Ok(history_days_from(&state.lock()))
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

async fn run_check_update(app: &AppHandle) -> Result<Option<UpdateInfo>, String> {
    let updater = app.updater().map_err(|err| err.to_string())?;
    match updater.check().await {
        Ok(Some(update)) => {
            let info = UpdateInfo {
                version: update.version.clone(),
                body: update.body.clone(),
            };
            if let Some(state) = app.try_state::<AppState>() {
                state.lock().update_available = Some(info.clone());
                publish(app, &state);
            }
            Ok(Some(info))
        }
        Ok(None) => {
            if let Some(state) = app.try_state::<AppState>() {
                state.lock().update_available = None;
                publish(app, &state);
            }
            Ok(None)
        }
        Err(err) => Err(err.to_string()),
    }
}

async fn run_install_update(app: &AppHandle) -> Result<(), String> {
    let updater = app.updater().map_err(|err| err.to_string())?;
    let update = updater
        .check()
        .await
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "没有可用更新".to_string())?;
    update
        .download_and_install(|_chunk, _total| {}, || {})
        .await
        .map_err(|err| err.to_string())?;
    app.restart();
}

#[tauri::command]
async fn check_for_update(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    run_check_update(&app).await
}

#[tauri::command]
async fn install_update(app: AppHandle) -> Result<(), String> {
    run_install_update(&app).await
}

pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .app_name("Rundi")
                .args(["--autostart"])
                .build(),
        )
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_settings(app);
        }))
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let (inner, first_launch) = load_inner();
            let from_autostart = std::env::args().any(|arg| arg == "--autostart");
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
            // WebView 按需创建：托盘空闲时不预建窗口，降低常驻内存。
            install_tray(app.handle(), &state)?;
            let shared = Arc::clone(&state.inner);
            app.manage(state);
            // 开始菜单等交互启动打开设置；开机自启（带 --autostart）只进托盘。
            if first_launch || !from_autostart {
                show_settings(app.handle());
            }
            let handle = app.handle().clone();
            std::thread::Builder::new()
                .name("rundi-scheduler".into())
                .spawn(move || scheduler_loop(handle, shared))
                .map_err(|err| -> Box<dyn std::error::Error> { Box::new(err) })?;

            let update_handle = app.handle().clone();
            std::thread::Builder::new()
                .name("rundi-update-check".into())
                .spawn(move || {
                    std::thread::sleep(Duration::from_secs(3));
                    tauri::async_runtime::block_on(async move {
                        match run_check_update(&update_handle).await {
                            Ok(Some(info)) => {
                                eprintln!("润滴：发现新版本 {}", info.version);
                            }
                            Ok(None) => {}
                            Err(err) => {
                                eprintln!("润滴：后台检查更新跳过：{err}");
                            }
                        }
                    });
                })
                .map_err(|err| -> Box<dyn std::error::Error> { Box::new(err) })?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            active_reminder,
            save_settings,
            drink,
            get_drink_history,
            clear_drink_history,
            snooze,
            dismiss,
            pause_hour,
            pause_today,
            resume,
            remind_now,
            check_for_update,
            install_update,
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
