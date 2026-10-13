use std::path::Path;

use chrono::{DateTime, FixedOffset, NaiveDate, TimeDelta};

use crate::config::{write_atomic, ConfigError};
use crate::schedule::RuntimeState;
use crate::units::{parse_drink_unit, DrinkUnit};

/// 历史最多保留天数（不含今天时仍按日期窗口裁剪）。
pub const HISTORY_RETENTION_DAYS: i64 = 90;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyCount {
    pub date: NaiveDate,
    pub glasses: u32,
    /// 当日累计所用单位；切单位时会换算 glasses 并更新此项。
    pub unit: DrinkUnit,
}

impl Default for DailyCount {
    fn default() -> Self {
        Self {
            date: NaiveDate::from_ymd_opt(1970, 1, 1).expect("epoch"),
            glasses: 0,
            unit: DrinkUnit::Cup,
        }
    }
}

/// 某一天的饮水量（带当时计量单位）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayIntake {
    pub date: NaiveDate,
    pub amount: u32,
    pub unit: DrinkUnit,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedDayIntake {
    date: String,
    amount: u32,
    unit: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct PersistedState {
    pub last_shown: Option<String>,
    pub snooze_until: Option<String>,
    pub paused_until: Option<String>,
    pub count_date: Option<String>,
    pub glasses: u32,
    #[serde(default)]
    pub count_unit: Option<String>,
    #[serde(default)]
    pub history: Vec<PersistedDayIntake>,
}

impl Default for PersistedState {
    fn default() -> Self {
        Self {
            last_shown: None,
            snooze_until: None,
            paused_until: None,
            count_date: None,
            glasses: 0,
            count_unit: None,
            history: Vec::new(),
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

/// 跨日时把旧日累计写入 history，并重置为今天的空计数。
/// 返回是否发生了滚动（调用方可据此决定是否落盘）。
pub fn rollover_daily(
    count: &mut DailyCount,
    history: &mut Vec<DayIntake>,
    today: NaiveDate,
    today_unit: DrinkUnit,
) -> bool {
    if count.date == today {
        return false;
    }
    if count.glasses > 0 && count.date != DailyCount::default().date {
        push_history(history, count.date, count.glasses, count.unit);
    }
    count.date = today;
    count.glasses = 0;
    count.unit = today_unit;
    trim_history(history, today);
    true
}

/// 记录一次饮水；若跨日则把昨日（有量时）写入 history。
pub fn record_intake(
    count: &mut DailyCount,
    history: &mut Vec<DayIntake>,
    today: NaiveDate,
    amount: u32,
    unit: DrinkUnit,
) -> u32 {
    let add = amount.max(1);
    rollover_daily(count, history, today, unit);
    if count.unit != unit && count.glasses > 0 {
        // 同日但配置单位已变：先把已有累计换到新单位再累加
        count.glasses = crate::units::convert_intake_total(count.glasses, count.unit, unit);
        count.unit = unit;
    } else {
        count.unit = unit;
    }
    count.glasses = count.glasses.saturating_add(add);
    trim_history(history, today);
    count.glasses
}

/// 切换计量单位时换算今日累计。
pub fn convert_daily_unit(count: &mut DailyCount, to: DrinkUnit) {
    if count.unit == to {
        return;
    }
    count.glasses = crate::units::convert_intake_total(count.glasses, count.unit, to);
    count.unit = to;
}

pub fn clear_history(history: &mut Vec<DayIntake>) {
    history.clear();
}

pub fn push_history(history: &mut Vec<DayIntake>, date: NaiveDate, amount: u32, unit: DrinkUnit) {
    if amount == 0 {
        return;
    }
    if let Some(existing) = history.iter_mut().find(|day| day.date == date) {
        // 同日再次归档：累加（单位以本次为准，先不跨单位合并旧条）
        if existing.unit == unit {
            existing.amount = existing.amount.saturating_add(amount);
        } else {
            existing.amount = amount;
            existing.unit = unit;
        }
    } else {
        history.push(DayIntake { date, amount, unit });
    }
    history.sort_by(|a, b| b.date.cmp(&a.date));
}

pub fn trim_history(history: &mut Vec<DayIntake>, today: NaiveDate) {
    let oldest = today - TimeDelta::days(HISTORY_RETENTION_DAYS);
    history.retain(|day| day.date >= oldest);
    history.sort_by(|a, b| b.date.cmp(&a.date));
}

/// 合并今天与历史，倒序（今天在前）。
pub fn merged_history(
    daily: &DailyCount,
    history: &[DayIntake],
    today: NaiveDate,
    today_unit: DrinkUnit,
) -> Vec<DayIntake> {
    let mut days = history.to_vec();
    days.retain(|day| day.date != today);
    let today_amount = glasses_on(daily, today);
    if today_amount > 0 || daily.date == today {
        days.insert(
            0,
            DayIntake {
                date: today,
                amount: today_amount,
                unit: today_unit,
            },
        );
    }
    days.sort_by(|a, b| b.date.cmp(&a.date));
    days
}

pub fn load_state(path: &Path) -> Result<(RuntimeState, DailyCount, Vec<DayIntake>), ConfigError> {
    if !path.exists() {
        return Ok((
            RuntimeState::default(),
            DailyCount::default(),
            Vec::new(),
        ));
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
    history: &[DayIntake],
) -> Result<(), ConfigError> {
    let persisted = PersistedState::from_parts(runtime, daily, history);
    let text = serde_json::to_string_pretty(&persisted)
        .map_err(|err| ConfigError::Parse(err.to_string()))?;
    write_atomic(path, text.as_bytes())
}

impl PersistedState {
    fn from_parts(runtime: &RuntimeState, daily: &DailyCount, history: &[DayIntake]) -> Self {
        Self {
            last_shown: runtime.last_shown.map(|time| time.to_rfc3339()),
            snooze_until: runtime.snooze_until.map(|time| time.to_rfc3339()),
            paused_until: runtime.paused_until.map(|time| time.to_rfc3339()),
            count_date: Some(daily.date.format("%Y-%m-%d").to_string()),
            glasses: daily.glasses,
            count_unit: Some(daily.unit.code().to_string()),
            history: history
                .iter()
                .map(|day| PersistedDayIntake {
                    date: day.date.format("%Y-%m-%d").to_string(),
                    amount: day.amount,
                    unit: day.unit.code().to_string(),
                })
                .collect(),
        }
    }

    fn into_runtime(self) -> Result<(RuntimeState, DailyCount, Vec<DayIntake>), ConfigError> {
        let mut history = Vec::with_capacity(self.history.len());
        for day in self.history {
            let date = NaiveDate::parse_from_str(&day.date, "%Y-%m-%d")
                .map_err(|_| ConfigError::Parse(format!("无法识别日期「{}」", day.date)))?;
            let unit = parse_drink_unit(&day.unit)?;
            history.push(DayIntake {
                date,
                amount: day.amount,
                unit,
            });
        }
        history.sort_by(|a, b| b.date.cmp(&a.date));
        let unit = match self.count_unit {
            Some(ref code) => parse_drink_unit(code)?,
            None => DrinkUnit::Cup,
        };
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
                unit,
            },
            history,
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
    use chrono::{TimeZone, Timelike};

    fn sample_now() -> DateTime<FixedOffset> {
        FixedOffset::east_opt(8 * 3600)
            .unwrap()
            .with_ymd_and_hms(2026, 9, 28, 10, 15, 0)
            .unwrap()
    }

    #[test]
    fn glass_count_resets_when_the_date_changes() {
        let mut count = DailyCount::default();
        let mut history = Vec::new();
        let today = NaiveDate::from_ymd_opt(2026, 9, 28).unwrap();
        assert_eq!(glasses_on(&count, today), 0);
        assert_eq!(
            record_intake(&mut count, &mut history, today, 1, DrinkUnit::Cup),
            1
        );
        assert_eq!(
            record_intake(&mut count, &mut history, today, 1, DrinkUnit::Cup),
            2
        );
        assert_eq!(glasses_on(&count, today), 2);
        let tomorrow = today + TimeDelta::days(1);
        assert_eq!(glasses_on(&count, tomorrow), 0);
        assert_eq!(
            record_intake(&mut count, &mut history, tomorrow, 1, DrinkUnit::Cup),
            1
        );
        assert_eq!(glasses_on(&count, today), 0);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].date, today);
        assert_eq!(history[0].amount, 2);
        assert_eq!(history[0].unit, DrinkUnit::Cup);
    }

    #[test]
    fn rollover_archives_previous_day_without_new_drink() {
        let yesterday = NaiveDate::from_ymd_opt(2026, 10, 12).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 10, 13).unwrap();
        let mut count = DailyCount {
            date: yesterday,
            glasses: 2080,
            unit: DrinkUnit::Ml,
        };
        let mut history = Vec::new();
        assert!(rollover_daily(
            &mut count,
            &mut history,
            today,
            DrinkUnit::Ml
        ));
        assert_eq!(count.date, today);
        assert_eq!(count.glasses, 0);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].date, yesterday);
        assert_eq!(history[0].amount, 2080);
        assert_eq!(history[0].unit, DrinkUnit::Ml);
        assert!(!rollover_daily(
            &mut count,
            &mut history,
            today,
            DrinkUnit::Ml
        ));
        let merged = merged_history(&count, &history, today, DrinkUnit::Ml);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].date, today);
        assert_eq!(merged[1].date, yesterday);
    }

    #[test]
    fn clear_history_keeps_today() {
        let mut history = vec![DayIntake {
            date: NaiveDate::from_ymd_opt(2026, 9, 27).unwrap(),
            amount: 3,
            unit: DrinkUnit::Cup,
        }];
        clear_history(&mut history);
        assert!(history.is_empty());
    }

    #[test]
    fn old_file_without_history_still_loads() {
        let dir = std::env::temp_dir().join(format!(
            "rundi-state-old-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.json");
        std::fs::write(
            &path,
            r#"{"countDate":"2026-09-28","glasses":4,"lastShown":null,"snoozeUntil":null,"pausedUntil":null}"#,
        )
        .unwrap();
        let (_, daily, history) = load_state(&path).unwrap();
        assert_eq!(daily.glasses, 4);
        assert!(history.is_empty());
    }

    #[test]
    fn trim_drops_entries_older_than_retention() {
        let today = NaiveDate::from_ymd_opt(2026, 9, 28).unwrap();
        let mut history = vec![
            DayIntake {
                date: today - TimeDelta::days(91),
                amount: 1,
                unit: DrinkUnit::Ml,
            },
            DayIntake {
                date: today - TimeDelta::days(1),
                amount: 2,
                unit: DrinkUnit::Ml,
            },
        ];
        trim_history(&mut history, today);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].amount, 2);
    }

    #[test]
    fn state_roundtrip_keeps_pause_count_and_history() {
        let dir = std::env::temp_dir().join(format!("rundi-state-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.json");
        let _ = std::fs::remove_file(&path);

        let (runtime, daily, history) = load_state(&path).unwrap();
        assert_eq!(runtime, RuntimeState::default());
        assert_eq!(daily.glasses, 0);
        assert!(history.is_empty());

        let now = sample_now();
        let mut runtime = RuntimeState::default();
        mark_shown(&mut runtime, now);
        apply_snooze(&mut runtime, now + TimeDelta::minutes(5));
        let mut daily = DailyCount::default();
        let mut history = Vec::new();
        record_intake(&mut daily, &mut history, now.date_naive(), 1, DrinkUnit::Cup);
        record_intake(&mut daily, &mut history, now.date_naive(), 1, DrinkUnit::Cup);
        let yesterday = now.date_naive() - TimeDelta::days(1);
        push_history(&mut history, yesterday, 600, DrinkUnit::Ml);
        save_state(&path, &runtime, &daily, &history).unwrap();

        let (loaded, loaded_daily, loaded_history) = load_state(&path).unwrap();
        assert_eq!(loaded.last_shown.unwrap().hour(), 10);
        assert_eq!(loaded.snooze_until.unwrap().minute(), 20);
        assert_eq!(loaded_daily.glasses, 2);
        assert_eq!(loaded_daily.date, now.date_naive());
        assert_eq!(loaded_history.len(), 1);
        assert_eq!(loaded_history[0].amount, 600);
        assert_eq!(loaded_history[0].unit, DrinkUnit::Ml);
    }
}
