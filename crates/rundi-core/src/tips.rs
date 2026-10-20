//! 提醒副文案与下班小结文案（纯规则，无网络）。

use chrono::{DateTime, FixedOffset, NaiveDate};

use crate::goal::{expected_desk_intake_ml, goal_progress};
use crate::schedule::{is_working_day, RuntimeState, Schedule};
use crate::state::DayIntake;
use crate::units::{format_drink_prompt, to_ml, DrinkUnit};

/// 生成动态提醒副文案所需上下文。
#[derive(Debug, Clone)]
pub struct PromptContext<'a> {
    pub unit: DrinkUnit,
    pub intake_ml_today: u32,
    pub desk_goal_ml: u32,
    pub high_intake_mode: bool,
    /// 时间窗进度 0–1；窗外为 `None`。
    pub window_progress: Option<f64>,
    pub expected_ml: u32,
    /// 不含今天的近期历史（越新越靠前亦可）。
    pub recent_history: &'a [DayIntake],
}

/// 按优先级选一句提醒副文案。
pub fn format_contextual_prompt(ctx: &PromptContext<'_>) -> String {
    if ctx.high_intake_mode && ctx.desk_goal_ml > 0 && ctx.expected_ml > 0 {
        let behind = ctx.intake_ml_today < ctx.expected_ml
            && f64::from(ctx.expected_ml.saturating_sub(ctx.intake_ml_today))
                >= f64::from(ctx.expected_ml) * 0.3;
        if behind {
            return match ctx.unit {
                DrinkUnit::Cup => "进度偏慢，这杯先补上。".into(),
                DrinkUnit::Ml => "进度偏慢，先补一点水。".into(),
                DrinkUnit::Sip => "进度偏慢，先喝一口。".into(),
            };
        }
    }

    if ctx.intake_ml_today == 0 {
        if let Some(progress) = ctx.window_progress {
            if progress >= 0.7 {
                return match ctx.unit {
                    DrinkUnit::Cup => "下午了还没记过，先喝一杯。".into(),
                    DrinkUnit::Ml => "下午了还没记过，先喝一点。".into(),
                    DrinkUnit::Sip => "下午了还没记过，先喝一口。".into(),
                };
            }
        }
    }

    if recent_days_low(ctx) {
        return match ctx.unit {
            DrinkUnit::Cup => "这几天喝得偏少，今天多记一杯。".into(),
            DrinkUnit::Ml => "这几天喝得偏少，今天多记一点。".into(),
            DrinkUnit::Sip => "这几天喝得偏少，今天多记几口。".into(),
        };
    }

    if ctx.desk_goal_ml > 0 && ctx.intake_ml_today >= ctx.desk_goal_ml {
        return "目标到了，再润一下也不错。".into();
    }

    format_drink_prompt(ctx.unit).to_string()
}

fn recent_days_low(ctx: &PromptContext<'_>) -> bool {
    let threshold = if ctx.desk_goal_ml > 0 {
        (f64::from(ctx.desk_goal_ml) * 0.5).round() as u32
    } else {
        350 // 约一杯
    };
    let mut lows = 0u32;
    for day in ctx.recent_history.iter().take(3) {
        let ml = to_ml(day.amount, day.unit);
        if ml < threshold && ml > 0 {
            lows += 1;
        } else if ml == 0 {
            lows += 1;
        } else {
            return false;
        }
    }
    lows >= 3
}

/// 是否该弹下班小结（不含「已弹出」的落盘，由调用方写入 `summary_shown_on`）。
pub fn summary_due(
    now: DateTime<FixedOffset>,
    schedule: &Schedule,
    state: &RuntimeState,
    enabled: bool,
) -> bool {
    if !enabled {
        return false;
    }
    if let Some(until) = state.paused_until {
        if now < until {
            return false;
        }
    }
    let today = now.date_naive();
    if !is_working_day(today, schedule) {
        return false;
    }
    if now.time() < schedule.end {
        return false;
    }
    if state.summary_shown_on == Some(today) {
        return false;
    }
    true
}

/// 下班小结鼓励语（摄入数字由界面单独展示，这里不重复报量）。
pub fn format_day_summary(
    _intake_label: &str,
    intake_ml_today: u32,
    desk_goal_ml: u32,
    high_intake_mode: bool,
) -> String {
    if intake_ml_today == 0 {
        return "工作时段结束了。今天还没记下喝水，明天到点再轻轻叫你。".into();
    }
    if high_intake_mode && desk_goal_ml > 0 {
        let progress = (goal_progress(intake_ml_today, desk_goal_ml) * 100.0).round() as u32;
        if intake_ml_today >= desk_goal_ml {
            return "工位目标已达成，很稳。下班后也可以继续润一润。".into();
        }
        if progress >= 70 {
            return format!("工位进度约 {progress}%，就差一口气。通勤或晚饭时再补一点。");
        }
        return format!("工位进度约 {progress}%。下班后记得把缺口补一补。");
    }
    if intake_ml_today >= 1000 {
        return "今天喝得不错。工作辛苦了，路上也别忘了润一润。".into();
    }
    "工作时段告一段落。今晚再记几口，身体会更舒服。".into()
}

/// 便捷：从调度与今日摄入拼出 PromptContext 常用字段。
pub fn expected_for_prompt(
    now: DateTime<FixedOffset>,
    schedule: &Schedule,
    desk_goal_ml: u32,
) -> (Option<f64>, u32) {
    let progress = schedule.window_progress(now);
    let expected = expected_desk_intake_ml(now, schedule, desk_goal_ml);
    (progress, expected)
}

/// 过滤出不含今天的近期历史条目（最多取若干天，按日期倒序输入即可）。
pub fn history_excluding_today(history: &[DayIntake], today: NaiveDate) -> Vec<DayIntake> {
    history
        .iter()
        .filter(|day| day.date != today)
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, NaiveTime, TimeZone, Weekday};

    fn offset() -> FixedOffset {
        FixedOffset::east_opt(8 * 3600).unwrap()
    }

    fn monday_at(hour: u32, min: u32) -> DateTime<FixedOffset> {
        let date = NaiveDate::from_ymd_opt(2026, 9, 28).unwrap();
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
    fn behind_goal_uses_catchup_copy() {
        let now = monday_at(14, 0);
        let schedule = desk_schedule();
        let expected = expected_desk_intake_ml(now, &schedule, 1500);
        let ctx = PromptContext {
            unit: DrinkUnit::Cup,
            intake_ml_today: 200,
            desk_goal_ml: 1500,
            high_intake_mode: true,
            window_progress: schedule.window_progress(now),
            expected_ml: expected,
            recent_history: &[],
        };
        assert!(format_contextual_prompt(&ctx).contains("进度偏慢"));
    }

    #[test]
    fn late_window_zero_intake() {
        let now = monday_at(16, 30);
        let schedule = desk_schedule();
        let ctx = PromptContext {
            unit: DrinkUnit::Sip,
            intake_ml_today: 0,
            desk_goal_ml: 1500,
            high_intake_mode: false,
            window_progress: schedule.window_progress(now),
            expected_ml: 0,
            recent_history: &[],
        };
        assert!(format_contextual_prompt(&ctx).contains("下午了还没记过"));
    }

    #[test]
    fn goal_reached_copy() {
        let ctx = PromptContext {
            unit: DrinkUnit::Ml,
            intake_ml_today: 1500,
            desk_goal_ml: 1500,
            high_intake_mode: true,
            window_progress: Some(0.5),
            expected_ml: 750,
            recent_history: &[],
        };
        assert!(format_contextual_prompt(&ctx).contains("目标到了"));
    }

    #[test]
    fn default_falls_back_to_unit_prompt() {
        let ctx = PromptContext {
            unit: DrinkUnit::Cup,
            intake_ml_today: 700,
            desk_goal_ml: 1500,
            high_intake_mode: false,
            window_progress: Some(0.4),
            expected_ml: 0,
            recent_history: &[],
        };
        assert_eq!(
            format_contextual_prompt(&ctx),
            format_drink_prompt(DrinkUnit::Cup)
        );
    }

    #[test]
    fn recent_low_days_hint() {
        let history = vec![
            DayIntake {
                date: NaiveDate::from_ymd_opt(2026, 9, 25).unwrap(),
                amount: 1,
                unit: DrinkUnit::Cup,
            },
            DayIntake {
                date: NaiveDate::from_ymd_opt(2026, 9, 24).unwrap(),
                amount: 200,
                unit: DrinkUnit::Ml,
            },
            DayIntake {
                date: NaiveDate::from_ymd_opt(2026, 9, 23).unwrap(),
                amount: 1,
                unit: DrinkUnit::Sip,
            },
        ];
        let ctx = PromptContext {
            unit: DrinkUnit::Cup,
            intake_ml_today: 350,
            desk_goal_ml: 1500,
            high_intake_mode: false,
            window_progress: Some(0.3),
            expected_ml: 0,
            recent_history: &history,
        };
        assert!(format_contextual_prompt(&ctx).contains("这几天喝得偏少"));
    }

    #[test]
    fn summary_due_gates() {
        let schedule = desk_schedule();
        let mut state = RuntimeState::default();
        let before = monday_at(18, 0);
        assert!(!summary_due(before, &schedule, &state, true));
        let after = monday_at(18, 30);
        assert!(summary_due(after, &schedule, &state, true));
        assert!(!summary_due(after, &schedule, &state, false));
        state.summary_shown_on = Some(after.date_naive());
        assert!(!summary_due(after, &schedule, &state, true));
    }

    #[test]
    fn summary_skipped_when_paused() {
        let schedule = desk_schedule();
        let after = monday_at(19, 0);
        let state = RuntimeState {
            paused_until: Some(monday_at(20, 0)),
            ..RuntimeState::default()
        };
        assert!(!summary_due(after, &schedule, &state, true));
    }

    #[test]
    fn summary_copy_with_goal() {
        let text = format_day_summary("900 毫升", 900, 1500, true);
        assert!(text.contains("%"));
        assert!(!text.contains("900 毫升"));
        let done = format_day_summary("1500 毫升", 1500, 1500, true);
        assert!(done.contains("目标已达成"));
        let plain = format_day_summary("2 杯", 700, 1500, false);
        assert!(plain.contains("告一段落") || plain.contains("今晚"));
        let empty = format_day_summary("0 杯", 0, 1500, false);
        assert!(empty.contains("还没记下"));
    }
}
