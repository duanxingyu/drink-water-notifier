use std::fs;
use std::path::{Path, PathBuf};

use chrono::{NaiveTime, Timelike, Weekday};

use crate::schedule::Schedule;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("提醒间隔须在 1 到 240 分钟之间")]
    IntervalOutOfRange,
    #[error("至少选择一个工作日")]
    EmptyWorkdays,
    #[error("开始时间必须早于结束时间")]
    InvalidWindow,
    #[error("无法识别时间「{0}」，请使用 HH:MM")]
    InvalidTime(String),
    #[error("无法识别星期「{0}」")]
    InvalidWeekday(String),
    #[error("音量须在 0 到 1 之间")]
    VolumeOutOfRange,
    #[error("自动关闭时间须在 5 到 180 秒之间")]
    DismissOutOfRange,
    #[error("稍后提醒须在 1 到 60 分钟之间")]
    SnoozeOutOfRange,
    #[error("{0}")]
    Io(String),
    #[error("配置格式不正确：{0}")]
    Parse(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppConfig {
    pub schedule: Schedule,
    pub sound_enabled: bool,
    pub volume: f32,
    pub auto_dismiss_seconds: u32,
    pub snooze_minutes: u32,
    pub launch_at_login: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schedule: Schedule::default(),
            sound_enabled: true,
            volume: 0.7,
            auto_dismiss_seconds: 20,
            snooze_minutes: 5,
            launch_at_login: false,
        }
    }
}

impl AppConfig {
    pub fn try_from_parts(
        workdays: &[String],
        start: &str,
        end: &str,
        interval_minutes: u32,
        sound_enabled: bool,
        volume: f32,
        auto_dismiss_seconds: u32,
        snooze_minutes: u32,
        launch_at_login: bool,
    ) -> Result<Self, ConfigError> {
        if !(5..=180).contains(&auto_dismiss_seconds) {
            return Err(ConfigError::DismissOutOfRange);
        }
        if !(1..=60).contains(&snooze_minutes) {
            return Err(ConfigError::SnoozeOutOfRange);
        }
        if !volume.is_finite() || !(0.0..=1.0).contains(&volume) {
            return Err(ConfigError::VolumeOutOfRange);
        }
        let mut days = Vec::with_capacity(workdays.len());
        for day in workdays {
            days.push(parse_weekday(day)?);
        }
        let schedule =
            Schedule::try_new(days, parse_time(start)?, parse_time(end)?, interval_minutes)?;
        Ok(Self {
            schedule,
            sound_enabled,
            volume,
            auto_dismiss_seconds,
            snooze_minutes,
            launch_at_login,
        })
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ConfigFile {
    #[serde(default = "default_workdays")]
    workdays: Vec<String>,
    #[serde(default = "default_start")]
    start: String,
    #[serde(default = "default_end")]
    end: String,
    #[serde(default = "default_interval")]
    interval_minutes: u32,
    #[serde(default = "default_sound")]
    sound_enabled: bool,
    #[serde(default = "default_volume", serialize_with = "serialize_volume")]
    volume: f32,
    #[serde(default = "default_dismiss")]
    auto_dismiss_seconds: u32,
    #[serde(default = "default_snooze")]
    snooze_minutes: u32,
    #[serde(default = "default_login")]
    launch_at_login: bool,
}

fn default_workdays() -> Vec<String> {
    ["Mon", "Tue", "Wed", "Thu", "Fri"]
        .into_iter()
        .map(str::to_string)
        .collect()
}
fn default_start() -> String {
    "09:30".into()
}
fn default_end() -> String {
    "18:30".into()
}
fn default_interval() -> u32 {
    30
}
fn default_sound() -> bool {
    true
}
fn default_volume() -> f32 {
    0.7
}

/// 音量按百分之一写入，避免 `0.7` 被存成一长串二进制小数。
fn serialize_volume<S: serde::Serializer>(volume: &f32, serializer: S) -> Result<S::Ok, S::Error> {
    let hundredths = (*volume * 100.0).round() as i32;
    serializer.serialize_f64(f64::from(hundredths) / 100.0)
}
fn default_dismiss() -> u32 {
    20
}
fn default_snooze() -> u32 {
    5
}
fn default_login() -> bool {
    false
}

impl Default for ConfigFile {
    fn default() -> Self {
        let config = AppConfig::default();
        Self::from(&config)
    }
}

impl From<&AppConfig> for ConfigFile {
    fn from(config: &AppConfig) -> Self {
        Self {
            workdays: config.schedule.workdays.iter().map(weekday_label).collect(),
            start: format_time(config.schedule.start),
            end: format_time(config.schedule.end),
            interval_minutes: config.schedule.interval_minutes(),
            sound_enabled: config.sound_enabled,
            volume: config.volume,
            auto_dismiss_seconds: config.auto_dismiss_seconds,
            snooze_minutes: config.snooze_minutes,
            launch_at_login: config.launch_at_login,
        }
    }
}

impl ConfigFile {
    fn into_app(self) -> Result<AppConfig, ConfigError> {
        AppConfig::try_from_parts(
            &self.workdays,
            &self.start,
            &self.end,
            self.interval_minutes,
            self.sound_enabled,
            self.volume,
            self.auto_dismiss_seconds,
            self.snooze_minutes,
            self.launch_at_login,
        )
    }
}

/// 操作系统约定的配置目录：`%APPDATA%/rundi`、`~/Library/Application Support/rundi` 或 `~/.config/rundi`。
pub fn app_config_dir() -> PathBuf {
    dirs::config_dir()
        .or_else(|| dirs::home_dir().map(|home| home.join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("rundi")
}

pub fn config_file_path() -> PathBuf {
    app_config_dir().join("config.toml")
}

pub fn state_file_path() -> PathBuf {
    app_config_dir().join("state.json")
}

pub fn load_config(path: &Path) -> Result<AppConfig, ConfigError> {
    if !path.exists() {
        return Ok(AppConfig::default());
    }
    let text = fs::read_to_string(path)
        .map_err(|err| ConfigError::Io(format!("无法读取 {}：{err}", path.display())))?;
    let file: ConfigFile =
        toml::from_str(&text).map_err(|err| ConfigError::Parse(err.to_string()))?;
    file.into_app()
}

pub fn save_config(path: &Path, config: &AppConfig) -> Result<(), ConfigError> {
    let text = toml::to_string_pretty(&ConfigFile::from(config))
        .map_err(|err| ConfigError::Parse(err.to_string()))?;
    write_atomic(path, text.as_bytes())
}

pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), ConfigError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|err| ConfigError::Io(format!("无法创建 {}：{err}", parent.display())))?;
        }
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)
        .map_err(|err| ConfigError::Io(format!("无法写入 {}：{err}", tmp.display())))?;
    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(err) => {
            let _ = fs::remove_file(path);
            fs::rename(&tmp, path)
                .map_err(|_| ConfigError::Io(format!("无法保存 {}：{err}", path.display())))
        }
    }
}

pub fn parse_weekday(value: &str) -> Result<Weekday, ConfigError> {
    match value.trim() {
        "Mon" | "mon" | "Monday" | "monday" | "周一" | "星期一" | "一" | "1" => {
            Ok(Weekday::Mon)
        }
        "Tue" | "tue" | "Tuesday" | "tuesday" | "周二" | "星期二" | "二" | "2" => {
            Ok(Weekday::Tue)
        }
        "Wed" | "wed" | "Wednesday" | "wednesday" | "周三" | "星期三" | "三" | "3" => {
            Ok(Weekday::Wed)
        }
        "Thu" | "thu" | "Thursday" | "thursday" | "周四" | "星期四" | "四" | "4" => {
            Ok(Weekday::Thu)
        }
        "Fri" | "fri" | "Friday" | "friday" | "周五" | "星期五" | "五" | "5" => {
            Ok(Weekday::Fri)
        }
        "Sat" | "sat" | "Saturday" | "saturday" | "周六" | "星期六" | "六" | "6" => {
            Ok(Weekday::Sat)
        }
        "Sun" | "sun" | "Sunday" | "sunday" | "周日" | "星期日" | "星期天" | "日" | "7" => {
            Ok(Weekday::Sun)
        }
        other => Err(ConfigError::InvalidWeekday(other.to_string())),
    }
}

pub fn weekday_code(day: Weekday) -> &'static str {
    match day {
        Weekday::Mon => "Mon",
        Weekday::Tue => "Tue",
        Weekday::Wed => "Wed",
        Weekday::Thu => "Thu",
        Weekday::Fri => "Fri",
        Weekday::Sat => "Sat",
        Weekday::Sun => "Sun",
    }
}

fn weekday_label(day: &Weekday) -> String {
    weekday_code(*day).to_string()
}

pub fn parse_time(value: &str) -> Result<NaiveTime, ConfigError> {
    let value = value.trim();
    if let Ok(time) = NaiveTime::parse_from_str(value, "%H:%M") {
        return Ok(time);
    }
    if let Ok(time) = NaiveTime::parse_from_str(value, "%H:%M:%S") {
        return Ok(NaiveTime::from_hms_opt(time.hour(), time.minute(), 0).expect("truncated"));
    }
    Err(ConfigError::InvalidTime(value.to_string()))
}

fn format_time(time: NaiveTime) -> String {
    format!("{:02}:{:02}", time.hour(), time.minute())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;

    fn temp_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rundi-core-{}-{}-{}",
            std::process::id(),
            name,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir.join("config.toml")
    }

    #[test]
    fn defaults_match_the_brief() {
        let config = AppConfig::default();
        assert_eq!(config.schedule.interval_minutes(), 30);
        assert_eq!(config.schedule.start.hour(), 9);
        assert_eq!(config.schedule.start.minute(), 30);
        assert_eq!(config.schedule.end.hour(), 18);
        assert_eq!(config.schedule.end.minute(), 30);
        assert_eq!(config.schedule.workdays.len(), 5);
        assert!(config.sound_enabled);
        assert!((config.volume - 0.7).abs() < f32::EPSILON);
        assert_eq!(config.snooze_minutes, 5);
        assert!(!config.launch_at_login);
    }

    #[test]
    fn rejects_bad_bounds() {
        let days = ["Mon".to_string()];
        assert!(matches!(
            AppConfig::try_from_parts(&days, "09:30", "18:30", 0, true, 0.5, 20, 5, false),
            Err(ConfigError::IntervalOutOfRange)
        ));
        assert!(matches!(
            AppConfig::try_from_parts(&days, "09:30", "18:30", 241, true, 0.5, 20, 5, false),
            Err(ConfigError::IntervalOutOfRange)
        ));
        assert!(
            AppConfig::try_from_parts(&days, "09:30", "18:30", 1, true, 0.0, 5, 1, false).is_ok()
        );
        assert!(
            AppConfig::try_from_parts(&days, "09:30", "18:30", 240, true, 1.0, 180, 60, false)
                .is_ok()
        );
        assert!(matches!(
            AppConfig::try_from_parts(&days, "18:30", "09:30", 30, true, 0.5, 20, 5, false),
            Err(ConfigError::InvalidWindow)
        ));
        assert!(matches!(
            AppConfig::try_from_parts(&days, "09:30", "09:30", 30, true, 0.5, 20, 5, false),
            Err(ConfigError::InvalidWindow)
        ));
        assert!(matches!(
            AppConfig::try_from_parts(&[], "09:30", "18:30", 30, true, 0.5, 20, 5, false),
            Err(ConfigError::EmptyWorkdays)
        ));
        assert!(matches!(
            AppConfig::try_from_parts(&days, "09:30", "18:30", 30, true, 1.1, 20, 5, false),
            Err(ConfigError::VolumeOutOfRange)
        ));
        assert!(matches!(
            AppConfig::try_from_parts(&days, "09:30", "18:30", 30, true, f32::NAN, 20, 5, false),
            Err(ConfigError::VolumeOutOfRange)
        ));
        assert!(matches!(
            AppConfig::try_from_parts(&days, "09:30", "18:30", 30, true, 0.5, 4, 5, false),
            Err(ConfigError::DismissOutOfRange)
        ));
        assert!(matches!(
            AppConfig::try_from_parts(&days, "09:30", "18:30", 30, true, 0.5, 20, 0, false),
            Err(ConfigError::SnoozeOutOfRange)
        ));
        assert!(matches!(
            AppConfig::try_from_parts(
                &["昨天".into()],
                "09:30",
                "18:30",
                30,
                true,
                0.5,
                20,
                5,
                false
            ),
            Err(ConfigError::InvalidWeekday(_))
        ));
        assert!(matches!(
            AppConfig::try_from_parts(&days, "九点半", "18:30", 30, true, 0.5, 20, 5, false),
            Err(ConfigError::InvalidTime(_))
        ));
    }

    #[test]
    fn parses_chinese_weekdays_and_seconds() {
        let config = AppConfig::try_from_parts(
            &["周一".into(), "星期三".into(), "五".into(), "日".into()],
            "09:30:45",
            "18:00",
            15,
            false,
            0.25,
            30,
            8,
            true,
        )
        .unwrap();
        assert_eq!(config.schedule.workdays.len(), 4);
        assert_eq!(config.schedule.start.second(), 0);
        assert!(!config.sound_enabled);
        assert!(config.launch_at_login);
    }

    #[test]
    fn roundtrip_and_missing_file() {
        let path = temp_path("roundtrip");
        assert!(load_config(&path).unwrap() == AppConfig::default());
        let config = AppConfig::try_from_parts(
            &["Tue".into(), "Thu".into()],
            "10:00",
            "17:15",
            45,
            true,
            0.4,
            25,
            10,
            true,
        )
        .unwrap();
        save_config(&path, &config).unwrap();
        let loaded = load_config(&path).unwrap();
        assert_eq!(loaded, config);
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("interval_minutes = 45"));
        assert!(text.contains("Tue"));
        assert!(text.contains("volume = 0.4\n") || text.contains("volume = 0.40\n"));
        let pretty = toml::to_string_pretty(&ConfigFile::from(&AppConfig::default())).unwrap();
        assert!(
            pretty.contains("volume = 0.7\n") || pretty.contains("volume = 0.70\n"),
            "{pretty}"
        );
    }

    #[test]
    fn partial_file_fills_defaults_and_corrupt_file_errors() {
        let path = temp_path("partial");
        fs::write(&path, "interval_minutes = 12\n").unwrap();
        let loaded = load_config(&path).unwrap();
        assert_eq!(loaded.schedule.interval_minutes(), 12);
        assert_eq!(
            loaded.schedule.workdays,
            AppConfig::default().schedule.workdays
        );
        fs::write(&path, "interval_minutes = \"").unwrap();
        assert!(matches!(load_config(&path), Err(ConfigError::Parse(_))));
    }

    #[test]
    fn empty_workday_list_in_file_is_rejected() {
        let path = temp_path("empty-days");
        fs::write(&path, "workdays = []\n").unwrap();
        assert!(matches!(
            load_config(&path),
            Err(ConfigError::EmptyWorkdays)
        ));
    }

    #[test]
    fn config_dir_ends_with_rundi() {
        assert_eq!(app_config_dir().file_name().unwrap(), "rundi");
        assert_eq!(config_file_path().file_name().unwrap(), "config.toml");
        assert_eq!(state_file_path().file_name().unwrap(), "state.json");
    }
}
