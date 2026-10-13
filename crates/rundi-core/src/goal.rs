//! 工位高摄入（结石友好）目标进度与追赶间隔。
//!
//! 仅覆盖电脑在线时段，不声称全天饮水管理。

use chrono::{DateTime, FixedOffset};

use crate::config::{ConfigError, DESK_GOAL_ML_MAX, DESK_GOAL_ML_MIN};
use crate::schedule::Schedule;
use crate::units::{to_ml, DrinkUnit};

pub fn validate_desk_goal_ml(ml: u32) -> Result<(), ConfigError> {
    if (DESK_GOAL_ML_MIN..=DESK_GOAL_ML_MAX).contains(&ml) {
        Ok(())
    } else {
        Err(ConfigError::DeskGoalOutOfRange)
    }
}

pub fn intake_ml(amount: u32, unit: DrinkUnit) -> u32 {
    to_ml(amount, unit)
}

pub fn goal_remaining_ml(intake_ml: u32, desk_goal_ml: u32) -> u32 {
    desk_goal_ml.saturating_sub(intake_ml)
}

/// 0.0–1.0；目标为 0 时视为已满。
pub fn goal_progress(intake_ml: u32, desk_goal_ml: u32) -> f32 {
    if desk_goal_ml == 0 {
        return 1.0;
    }
    (intake_ml as f32 / desk_goal_ml as f32).clamp(0.0, 1.0)
}

/// 按工位时段进度估算「此刻应已喝多少毫升」。不在时间窗内时返回 0。
pub fn expected_desk_intake_ml(
    now: DateTime<FixedOffset>,
    schedule: &Schedule,
    desk_goal_ml: u32,
) -> u32 {
    match schedule.window_progress(now) {
        Some(progress) => (f64::from(desk_goal_ml) * progress).round() as u32,
        None => 0,
    }
}

/// 高摄入模式下，落后于匀速进度时缩短间隔（不低于 10 分钟，且不少于基础间隔的一半）。
pub fn paced_interval_minutes(
    base_interval: u32,
    high_intake_mode: bool,
    desk_goal_ml: u32,
    intake_ml: u32,
    now: DateTime<FixedOffset>,
    schedule: &Schedule,
) -> u32 {
    let base = base_interval.max(1);
    if !high_intake_mode || desk_goal_ml == 0 {
        return base;
    }
    let Some(progress) = schedule.window_progress(now) else {
        return base;
    };
    // 刚进入时段时不急着追赶。
    if progress < 0.08 {
        return base;
    }
    let expected = expected_desk_intake_ml(now, schedule, desk_goal_ml);
    if expected == 0 || intake_ml >= expected {
        return base;
    }
    let ratio = f64::from(intake_ml) / f64::from(expected);
    let factor = (0.5 + 0.5 * ratio).clamp(0.5, 1.0);
    let paced = (f64::from(base) * factor).round() as u32;
    let floor = (base / 2).max(10).min(base);
    paced.clamp(floor, base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        RECOMMENDED_DESK_GOAL_ML, RECOMMENDED_HIGH_INTAKE_AMOUNT_ML,
        RECOMMENDED_HIGH_INTAKE_INTERVAL_MINUTES,
    };
    use chrono::{Datelike, NaiveDate, NaiveTime, TimeZone, Weekday};

    fn offset() -> FixedOffset {
        FixedOffset::east_opt(8 * 3600).unwrap()
    }

    fn monday_at(hour: u32, min: u32) -> DateTime<FixedOffset> {
        let date = NaiveDate::from_ymd_opt(2026, 9, 28).unwrap(); // Monday
        assert_eq!(date.weekday(), Weekday::Mon);
        offset()
            .from_local_datetime(
                &date.and_time(NaiveTime::from_hms_opt(hour, min, 0).unwrap()),
            )
            .single()
            .unwrap()
    }

    fn desk_schedule() -> Schedule {
        Schedule {
            respect_chinese_holidays: false,
            ..Schedule::default()
        }
    }

    #[test]
    fn recommended_defaults_are_sensible() {
        assert_eq!(RECOMMENDED_DESK_GOAL_ML, 1500);
        assert_eq!(RECOMMENDED_HIGH_INTAKE_INTERVAL_MINUTES, 25);
        assert_eq!(RECOMMENDED_HIGH_INTAKE_AMOUNT_ML, 200);
        assert!(validate_desk_goal_ml(RECOMMENDED_DESK_GOAL_ML).is_ok());
    }

    #[test]
    fn progress_and_remaining() {
        assert!((goal_progress(750, 1500) - 0.5).abs() < f32::EPSILON);
        assert_eq!(goal_remaining_ml(750, 1500), 750);
        assert_eq!(goal_remaining_ml(2000, 1500), 0);
    }

    #[test]
    fn expected_intake_mid_window() {
        // 09:30–18:30 → 9h；14:00 为过了 4.5h → 50% → 750 ml
        let now = monday_at(14, 0);
        let expected = expected_desk_intake_ml(now, &desk_schedule(), 1500);
        assert!((700..=800).contains(&expected), "got {expected}");
    }

    #[test]
    fn paced_interval_shortens_when_behind() {
        let now = monday_at(14, 0);
        let schedule = desk_schedule();
        let paced = paced_interval_minutes(30, true, 1500, 200, now, &schedule);
        assert!(paced < 30, "got {paced}");
        assert!(paced >= 10, "got {paced}");
        let on_track = paced_interval_minutes(30, true, 1500, 1500, now, &schedule);
        assert_eq!(on_track, 30);
        let off = paced_interval_minutes(30, false, 1500, 0, now, &schedule);
        assert_eq!(off, 30);
    }
}
