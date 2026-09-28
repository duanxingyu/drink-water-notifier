use std::path::Path;

use chrono::{DateTime, FixedOffset, NaiveDate};

use crate::config::{write_atomic, ConfigError};
use crate::schedule::RuntimeState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyCount {
    pub date: NaiveDate,
    pub glasses: u32,
}

impl Default for DailyCount {
    fn default() -> Self {
        Self {
            date: NaiveDate::from_ymd_opt(1970, 1, 1).expect("epoch"),
            glasses: 0,
        }
    }
}

pub fn glasses_on(count: &DailyCount, today: NaiveDate) -> u32 {
    if count.date == today {
        count.glasses
    } else {
        0
    }
}

pub fn record_glass(count: &mut DailyCount, today: NaiveDate) -> u32 {
    if count.date != today {
        count.date = today;
        count.glasses = 0;
    }
    count.glasses = count.glasses.saturating_add(1);
    count.glasses
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PersistedState {
    pub last_shown: Option<String>,
    pub snooze_until: Option<String>,
    pub paused_until: Option<String>,
    pub count_date: Option<String>,
    pub glasses: u32,
}

impl Default for PersistedState {
    fn default() -> Self {
        Self {
            last_shown: None,
            snooze_until: None,
            paused_until: None,
            count_date: None,
            glasses: 0,
        }
    }
}

pub fn load_state(path: &Path) -> Result<(RuntimeState, DailyCount), ConfigError> {
    if !path.exists() {
        return Ok((RuntimeState::default(), DailyCount::default()));
    }
    let text = std::fs::read_to_string(path)
        .map_err(|err| ConfigError::Io(format!("无法读取 {}：{err}", path.display())))?;
    let persisted: PersistedState =
        serde_json::from_str(&text).map_err(|err| ConfigError::Parse(err.to_string()))?;
    persisted.into_runtime()
}

pub fn save_state(
    path: &Path,
    runtime: &RuntimeState,
    daily: &DailyCount,
) -> Result<(), ConfigError> {
    let persisted = PersistedState::from_parts(runtime, daily);
    let text = serde_json::to_string_pretty(&persisted)
        .map_err(|err| ConfigError::Parse(err.to_string()))?;
    write_atomic(path, text.as_bytes())
}

impl PersistedState {
    fn from_parts(runtime: &RuntimeState, daily: &DailyCount) -> Self {
        Self {
            last_shown: runtime.last_shown.map(|time| time.to_rfc3339()),
            snooze_until: runtime.snooze_until.map(|time| time.to_rfc3339()),
            paused_until: runtime.paused_until.map(|time| time.to_rfc3339()),
            count_date: Some(daily.date.format("%Y-%m-%d").to_string()),
            glasses: daily.glasses,
        }
    }

    fn into_runtime(self) -> Result<(RuntimeState, DailyCount), ConfigError> {
        Ok((
            RuntimeState {
                last_shown: parse_optional_time(self.last_shown)?,
                snooze_until: parse_optional_time(self.snooze_until)?,
                paused_until: parse_optional_time(self.paused_until)?,
            },
            DailyCount {
                date: match self.count_date {
                    Some(value) => NaiveDate::parse_from_str(&value, "%Y-%m-%d")
                        .map_err(|_| ConfigError::Parse(format!("无法识别日期「{value}」")))?,
                    None => DailyCount::default().date,
                },
                glasses: self.glasses,
            },
        ))
    }
}

fn parse_optional_time(
    value: Option<String>,
) -> Result<Option<DateTime<FixedOffset>>, ConfigError> {
    match value {
        None => Ok(None),
        Some(text) => DateTime::parse_from_rfc3339(&text)
            .map(Some)
            .map_err(|_| ConfigError::Parse(format!("无法识别时间「{text}」"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schedule::{apply_snooze, mark_shown};
    use chrono::{TimeDelta, TimeZone, Timelike};

    fn sample_now() -> DateTime<FixedOffset> {
        FixedOffset::east_opt(8 * 3600)
            .unwrap()
            .with_ymd_and_hms(2026, 9, 28, 10, 15, 0)
            .unwrap()
    }

    #[test]
    fn glass_count_resets_when_the_date_changes() {
        let mut count = DailyCount::default();
        let today = NaiveDate::from_ymd_opt(2026, 9, 28).unwrap();
        assert_eq!(glasses_on(&count, today), 0);
        assert_eq!(record_glass(&mut count, today), 1);
        assert_eq!(record_glass(&mut count, today), 2);
        assert_eq!(glasses_on(&count, today), 2);
        let tomorrow = today + TimeDelta::days(1);
        assert_eq!(glasses_on(&count, tomorrow), 0);
        assert_eq!(record_glass(&mut count, tomorrow), 1);
        assert_eq!(glasses_on(&count, today), 0);
    }

    #[test]
    fn state_roundtrip_keeps_pause_and_count() {
        let dir = std::env::temp_dir().join(format!("rundi-state-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.json");
        let _ = std::fs::remove_file(&path);

        let (runtime, daily) = load_state(&path).unwrap();
        assert_eq!(runtime, RuntimeState::default());
        assert_eq!(daily.glasses, 0);

        let now = sample_now();
        let mut runtime = RuntimeState::default();
        mark_shown(&mut runtime, now);
        apply_snooze(&mut runtime, now + TimeDelta::minutes(5));
        let mut daily = DailyCount::default();
        record_glass(&mut daily, now.date_naive());
        record_glass(&mut daily, now.date_naive());
        save_state(&path, &runtime, &daily).unwrap();

        let (loaded, loaded_daily) = load_state(&path).unwrap();
        assert_eq!(loaded.last_shown.unwrap().hour(), 10);
        assert_eq!(loaded.snooze_until.unwrap().minute(), 20);
        assert_eq!(loaded_daily.glasses, 2);
        assert_eq!(loaded_daily.date, now.date_naive());
    }
}
