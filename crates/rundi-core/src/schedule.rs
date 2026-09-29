use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, NaiveTime, TimeDelta, TimeZone, Weekday};

use crate::holiday::china_day_override;

/// 半开区间 `[start, end)`。结束时刻本身不再提醒。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schedule {
    pub workdays: Vec<Weekday>,
    pub start: NaiveTime,
    pub end: NaiveTime,
    pub interval: TimeDelta,
    /// 结合国务院放假安排：法定节假日不提醒，调休补班日提醒。
    pub respect_chinese_holidays: bool,
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            workdays: normalize([
                Weekday::Mon,
                Weekday::Tue,
                Weekday::Wed,
                Weekday::Thu,
                Weekday::Fri,
            ]),
            start: NaiveTime::from_hms_opt(9, 30, 0).expect("09:30"),
            end: NaiveTime::from_hms_opt(18, 30, 0).expect("18:30"),
            interval: TimeDelta::minutes(30),
            respect_chinese_holidays: true,
        }
    }
}

impl Schedule {
    pub fn try_new(
        workdays: impl IntoIterator<Item = Weekday>,
        start: NaiveTime,
        end: NaiveTime,
        interval_minutes: u32,
        respect_chinese_holidays: bool,
    ) -> Result<Self, crate::config::ConfigError> {
        let workdays = normalize(workdays);
        if workdays.is_empty() {
            return Err(crate::config::ConfigError::EmptyWorkdays);
        }
        if !(1..=240).contains(&interval_minutes) {
            return Err(crate::config::ConfigError::IntervalOutOfRange);
        }
        if start >= end {
            return Err(crate::config::ConfigError::InvalidWindow);
        }
        Ok(Self {
            workdays,
            start,
            end,
            interval: TimeDelta::minutes(i64::from(interval_minutes)),
            respect_chinese_holidays,
        })
    }

    pub fn interval_minutes(&self) -> u32 {
        u32::try_from(self.interval.num_minutes().max(0)).unwrap_or(u32::MAX)
    }
}

/// 上次提醒、稍后和暂停。时间都带偏移，避免测试依赖机器时区。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuntimeState {
    pub last_shown: Option<DateTime<FixedOffset>>,
    pub snooze_until: Option<DateTime<FixedOffset>>,
    pub paused_until: Option<DateTime<FixedOffset>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// 现在就弹出提醒。
    Show,
    /// 在这个时刻之前什么都不用做。
    WaitUntil(DateTime<FixedOffset>),
}

pub fn apply_pause(state: &mut RuntimeState, until: DateTime<FixedOffset>) {
    state.paused_until = Some(until);
    state.snooze_until = None;
}

pub fn clear_pause(state: &mut RuntimeState) {
    state.paused_until = None;
}

pub fn apply_snooze(state: &mut RuntimeState, until: DateTime<FixedOffset>) {
    state.snooze_until = Some(until);
}

pub fn mark_shown(state: &mut RuntimeState, now: DateTime<FixedOffset>) {
    state.last_shown = Some(now);
    state.snooze_until = None;
}

/// 本地日历的次日 00:00，用作「今日暂停」。
pub fn end_of_local_day(now: DateTime<FixedOffset>) -> DateTime<FixedOffset> {
    let next = now.date_naive() + TimeDelta::days(1);
    at_time(
        next,
        NaiveTime::from_hms_opt(0, 0, 0).expect("midnight"),
        *now.offset(),
    )
}

/// 决定此刻该弹出，还是睡到下一个检查点。
///
/// 优先级：暂停 > 稍后提醒 > 工作日时间窗 > 间隔。
/// 「稍后提醒」到点后即使已经离开时间窗，也会再响一次。
pub fn evaluate(now: DateTime<FixedOffset>, schedule: &Schedule, state: &RuntimeState) -> Decision {
    if let Some(until) = state.paused_until {
        if now < until {
            return Decision::WaitUntil(until);
        }
    }

    if let Some(until) = state.snooze_until {
        if now < until {
            return Decision::WaitUntil(until);
        }
        return Decision::Show;
    }

    if !in_window(now, schedule) {
        return Decision::WaitUntil(next_window_start(now, schedule));
    }

    match state.last_shown {
        None => Decision::Show,
        Some(last) => {
            let due = last + schedule.interval;
            if now >= due {
                Decision::Show
            } else if in_window(due, schedule) {
                Decision::WaitUntil(due)
            } else {
                Decision::WaitUntil(next_window_start(now, schedule))
            }
        }
    }
}

fn normalize(days: impl IntoIterator<Item = Weekday>) -> Vec<Weekday> {
    let mut days: Vec<_> = days.into_iter().collect();
    days.sort_by_key(|day| day.num_days_from_monday());
    days.dedup();
    days
}

fn is_working_day(date: NaiveDate, schedule: &Schedule) -> bool {
    if schedule.respect_chinese_holidays {
        if let Some(forced) = china_day_override(date) {
            return forced;
        }
    }
    schedule.workdays.contains(&date.weekday())
}

fn in_window(now: DateTime<FixedOffset>, schedule: &Schedule) -> bool {
    if schedule.workdays.is_empty() || schedule.start >= schedule.end {
        return false;
    }
    if !is_working_day(now.date_naive(), schedule) {
        return false;
    }
    let time = now.time();
    time >= schedule.start && time < schedule.end
}

fn next_window_start(now: DateTime<FixedOffset>, schedule: &Schedule) -> DateTime<FixedOffset> {
    if schedule.workdays.is_empty() || schedule.start >= schedule.end {
        return now + TimeDelta::days(1);
    }

    let offset = *now.offset();
    let today = now.date_naive();
    if is_working_day(today, schedule) {
        let start = at_time(today, schedule.start, offset);
        if now < start {
            return start;
        }
    }

    // 法定长假可能跨一周以上，多往后看几天。
    let horizon = if schedule.respect_chinese_holidays {
        60
    } else {
        7
    };
    for day in 1..=horizon {
        let date = today + TimeDelta::days(day);
        if is_working_day(date, schedule) {
            return at_time(date, schedule.start, offset);
        }
    }

    now + TimeDelta::days(1)
}

fn at_time(date: NaiveDate, time: NaiveTime, offset: FixedOffset) -> DateTime<FixedOffset> {
    match offset.from_local_datetime(&date.and_time(time)) {
        chrono::LocalResult::Single(dt) | chrono::LocalResult::Ambiguous(dt, _) => dt,
        chrono::LocalResult::None => date.and_time(time).and_utc().with_timezone(&offset),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offset() -> FixedOffset {
        FixedOffset::east_opt(8 * 3600).expect("cst")
    }

    fn on(weekday: Weekday) -> NaiveDate {
        let mut date = NaiveDate::from_ymd_opt(2026, 9, 28).expect("date");
        for _ in 0..7 {
            if date.weekday() == weekday {
                return date;
            }
            date += TimeDelta::days(1);
        }
        unreachable!("weekday within a week");
    }

    fn at(date: NaiveDate, hour: u32, min: u32, sec: u32) -> DateTime<FixedOffset> {
        at_time(
            date,
            NaiveTime::from_hms_opt(hour, min, sec).expect("time"),
            offset(),
        )
    }

    fn schedule() -> Schedule {
        Schedule {
            // 单测按纯星期推算，避开节假日数据年份差异。
            respect_chinese_holidays: false,
            ..Schedule::default()
        }
    }

    #[test]
    fn known_week_layout() {
        assert_eq!(on(Weekday::Mon).weekday(), Weekday::Mon);
        assert_eq!(on(Weekday::Fri) - on(Weekday::Mon), TimeDelta::days(4));
        assert_eq!(on(Weekday::Sat) - on(Weekday::Mon), TimeDelta::days(5));
        assert_eq!(on(Weekday::Sun) - on(Weekday::Mon), TimeDelta::days(6));
    }

    #[test]
    fn before_window_waits_until_start() {
        let now = at(on(Weekday::Mon), 9, 29, 59);
        let decision = evaluate(now, &schedule(), &RuntimeState::default());
        assert_eq!(
            decision,
            Decision::WaitUntil(at(on(Weekday::Mon), 9, 30, 0))
        );
    }

    #[test]
    fn exactly_at_start_shows_immediately() {
        let now = at(on(Weekday::Mon), 9, 30, 0);
        assert_eq!(
            evaluate(now, &schedule(), &RuntimeState::default()),
            Decision::Show
        );
    }

    #[test]
    fn one_second_before_end_shows_when_due() {
        let mut state = RuntimeState::default();
        state.last_shown = Some(at(on(Weekday::Mon), 17, 59, 59));
        let sched = Schedule::try_new(
            schedule().workdays,
            schedule().start,
            schedule().end,
            30,
            false,
        )
        .unwrap();
        // 间隔 30 分钟，上次 17:59:59，下一次 18:29:59，仍在 18:30 之前。
        let now = at(on(Weekday::Mon), 18, 29, 59);
        assert_eq!(evaluate(now, &sched, &state), Decision::Show);
    }

    #[test]
    fn exactly_at_end_is_outside_the_window() {
        let now = at(on(Weekday::Mon), 18, 30, 0);
        let decision = evaluate(now, &schedule(), &RuntimeState::default());
        assert_eq!(
            decision,
            Decision::WaitUntil(at(on(Weekday::Tue), 9, 30, 0))
        );
    }

    #[test]
    fn due_landing_on_end_waits_for_the_next_window() {
        let mut state = RuntimeState::default();
        state.last_shown = Some(at(on(Weekday::Mon), 18, 0, 0));
        let now = at(on(Weekday::Mon), 18, 20, 0);
        // 18:00 + 30 分 = 18:30，落在结束边界上，不再弹出。
        assert_eq!(
            evaluate(now, &schedule(), &state),
            Decision::WaitUntil(at(on(Weekday::Tue), 9, 30, 0))
        );
    }

    #[test]
    fn interval_boundary_is_inclusive_on_the_due_instant() {
        let mut state = RuntimeState::default();
        state.last_shown = Some(at(on(Weekday::Mon), 10, 0, 0));
        let early = at(on(Weekday::Mon), 10, 29, 59);
        assert_eq!(
            evaluate(early, &schedule(), &state),
            Decision::WaitUntil(at(on(Weekday::Mon), 10, 30, 0))
        );
        let due = at(on(Weekday::Mon), 10, 30, 0);
        assert_eq!(evaluate(due, &schedule(), &state), Decision::Show);
    }

    #[test]
    fn monday_shows_every_thirty_minutes_until_eighteen() {
        let sched = schedule();
        let mut state = RuntimeState::default();
        let mut now = at(on(Weekday::Mon), 0, 0, 0);
        let mut shows = Vec::new();
        for _ in 0..80 {
            match evaluate(now, &sched, &state) {
                Decision::Show => {
                    shows.push(now);
                    mark_shown(&mut state, now);
                }
                Decision::WaitUntil(next) => {
                    assert!(next > now, "wait target {next} is not after {now}");
                    if next.date_naive() != on(Weekday::Mon) {
                        break;
                    }
                    now = next;
                }
            }
        }

        assert_eq!(shows.len(), 18, "09:30 到 18:00 含首尾，每 30 分钟一次");
        assert_eq!(shows[0], at(on(Weekday::Mon), 9, 30, 0));
        assert_eq!(shows[1], at(on(Weekday::Mon), 10, 0, 0));
        assert_eq!(*shows.last().unwrap(), at(on(Weekday::Mon), 18, 0, 0));
        for pair in shows.windows(2) {
            assert_eq!(pair[1] - pair[0], TimeDelta::minutes(30));
        }
    }

    #[test]
    fn saturday_waits_until_monday() {
        let now = at(on(Weekday::Sat), 12, 0, 0);
        assert_eq!(
            evaluate(now, &schedule(), &RuntimeState::default()),
            Decision::WaitUntil(at(on(Weekday::Mon) + TimeDelta::days(7), 9, 30, 0))
        );
    }

    #[test]
    fn sunday_morning_waits_until_monday_start() {
        let now = at(on(Weekday::Sun), 9, 0, 0);
        assert_eq!(
            evaluate(now, &schedule(), &RuntimeState::default()),
            Decision::WaitUntil(at(on(Weekday::Mon) + TimeDelta::days(7), 9, 30, 0))
        );
    }

    #[test]
    fn friday_after_hours_waits_until_next_monday() {
        let now = at(on(Weekday::Fri), 18, 30, 0);
        let next_monday = on(Weekday::Fri) + TimeDelta::days(3);
        assert_eq!(next_monday.weekday(), Weekday::Mon);
        assert_eq!(
            evaluate(now, &schedule(), &RuntimeState::default()),
            Decision::WaitUntil(at(next_monday, 9, 30, 0))
        );
    }

    #[test]
    fn custom_tuesday_thursday_skips_wednesday() {
        let sched = Schedule::try_new(
            [Weekday::Tue, Weekday::Thu],
            schedule().start,
            schedule().end,
            30,
            false,
        )
        .unwrap();

        let wednesday = at(on(Weekday::Wed), 10, 0, 0);
        assert_eq!(
            evaluate(wednesday, &sched, &RuntimeState::default()),
            Decision::WaitUntil(at(on(Weekday::Thu), 9, 30, 0))
        );

        let mut state = RuntimeState::default();
        mark_shown(&mut state, at(on(Weekday::Thu), 18, 0, 0));
        let after = at(on(Weekday::Thu), 19, 0, 0);
        let next_tue = on(Weekday::Thu) + TimeDelta::days(5);
        assert_eq!(next_tue.weekday(), Weekday::Tue);
        assert_eq!(
            evaluate(after, &sched, &state),
            Decision::WaitUntil(at(next_tue, 9, 30, 0))
        );
    }

    #[test]
    fn sunday_only_schedule() {
        let sched = Schedule::try_new(
            [Weekday::Sun],
            NaiveTime::from_hms_opt(9, 30, 0).unwrap(),
            NaiveTime::from_hms_opt(18, 30, 0).unwrap(),
            240,
            false,
        )
        .unwrap();
        let friday_night = at(on(Weekday::Fri), 20, 0, 0);
        assert_eq!(
            evaluate(friday_night, &sched, &RuntimeState::default()),
            Decision::WaitUntil(at(on(Weekday::Sun), 9, 30, 0))
        );
        assert_eq!(
            evaluate(
                at(on(Weekday::Sun), 9, 30, 0),
                &sched,
                &RuntimeState::default()
            ),
            Decision::Show
        );
    }

    #[test]
    fn long_interval_still_shows_at_window_open() {
        let sched = Schedule::try_new(
            schedule().workdays,
            NaiveTime::from_hms_opt(9, 30, 0).unwrap(),
            NaiveTime::from_hms_opt(10, 0, 0).unwrap(),
            240,
            false,
        )
        .unwrap();
        let open = at(on(Weekday::Mon), 9, 30, 0);
        assert_eq!(
            evaluate(open, &sched, &RuntimeState::default()),
            Decision::Show
        );
        let mut state = RuntimeState::default();
        mark_shown(&mut state, open);
        let still_inside = at(on(Weekday::Mon), 9, 45, 0);
        assert_eq!(
            evaluate(still_inside, &sched, &state),
            Decision::WaitUntil(at(on(Weekday::Tue), 9, 30, 0))
        );
    }

    #[test]
    fn pause_blocks_until_it_expires_then_interval_applies() {
        let mut state = RuntimeState::default();
        let now = at(on(Weekday::Mon), 10, 0, 0);
        mark_shown(&mut state, now);
        apply_pause(&mut state, at(on(Weekday::Mon), 11, 0, 0));
        assert!(state.snooze_until.is_none());
        assert_eq!(
            evaluate(at(on(Weekday::Mon), 10, 30, 0), &schedule(), &state),
            Decision::WaitUntil(at(on(Weekday::Mon), 11, 0, 0))
        );
        // 暂停在 11:00 结束。上次 10:00，间隔 30 分钟，11:00 已经到期。
        assert_eq!(
            evaluate(at(on(Weekday::Mon), 11, 0, 0), &schedule(), &state),
            Decision::Show
        );
    }

    #[test]
    fn pause_past_the_window_wakes_into_the_next_window() {
        let mut state = RuntimeState::default();
        apply_pause(&mut state, at(on(Weekday::Mon), 19, 0, 0));
        assert_eq!(
            evaluate(at(on(Weekday::Mon), 18, 0, 0), &schedule(), &state),
            Decision::WaitUntil(at(on(Weekday::Mon), 19, 0, 0))
        );
        assert_eq!(
            evaluate(at(on(Weekday::Mon), 19, 0, 0), &schedule(), &state),
            Decision::WaitUntil(at(on(Weekday::Tue), 9, 30, 0))
        );
    }

    #[test]
    fn pause_today_lasts_until_local_midnight() {
        let now = at(on(Weekday::Mon), 15, 4, 0);
        let until = end_of_local_day(now);
        assert_eq!(until, at(on(Weekday::Tue), 0, 0, 0));
        let mut state = RuntimeState::default();
        apply_pause(&mut state, until);
        assert_eq!(
            evaluate(at(on(Weekday::Mon), 18, 0, 0), &schedule(), &state),
            Decision::WaitUntil(until)
        );
        assert_eq!(
            evaluate(until, &schedule(), &state),
            Decision::WaitUntil(at(on(Weekday::Tue), 9, 30, 0))
        );
    }

    #[test]
    fn snooze_fires_even_outside_the_window_and_only_once() {
        let mut state = RuntimeState::default();
        mark_shown(&mut state, at(on(Weekday::Mon), 18, 20, 0));
        apply_snooze(&mut state, at(on(Weekday::Mon), 18, 40, 0));
        assert_eq!(
            evaluate(at(on(Weekday::Mon), 18, 35, 0), &schedule(), &state),
            Decision::WaitUntil(at(on(Weekday::Mon), 18, 40, 0))
        );
        let due = at(on(Weekday::Mon), 18, 40, 0);
        assert_eq!(evaluate(due, &schedule(), &state), Decision::Show);
        mark_shown(&mut state, due);
        assert_eq!(
            evaluate(due, &schedule(), &state),
            Decision::WaitUntil(at(on(Weekday::Tue), 9, 30, 0))
        );
    }

    #[test]
    fn snooze_inside_the_window_beats_the_regular_interval() {
        let mut state = RuntimeState::default();
        let shown = at(on(Weekday::Mon), 10, 0, 0);
        mark_shown(&mut state, shown);
        apply_snooze(&mut state, at(on(Weekday::Mon), 10, 5, 0));
        assert_eq!(
            evaluate(at(on(Weekday::Mon), 10, 3, 0), &schedule(), &state),
            Decision::WaitUntil(at(on(Weekday::Mon), 10, 5, 0))
        );
        assert_eq!(
            evaluate(at(on(Weekday::Mon), 10, 5, 0), &schedule(), &state),
            Decision::Show
        );
    }

    #[test]
    fn pause_overrides_an_existing_snooze() {
        let mut state = RuntimeState::default();
        apply_snooze(&mut state, at(on(Weekday::Mon), 10, 5, 0));
        apply_pause(&mut state, at(on(Weekday::Mon), 12, 0, 0));
        assert!(state.snooze_until.is_none());
        assert_eq!(
            evaluate(at(on(Weekday::Mon), 10, 6, 0), &schedule(), &state),
            Decision::WaitUntil(at(on(Weekday::Mon), 12, 0, 0))
        );
    }

    #[test]
    fn empty_workdays_never_show_and_do_not_spin() {
        let sched = Schedule {
            workdays: Vec::new(),
            ..schedule()
        };
        let now = at(on(Weekday::Mon), 10, 0, 0);
        assert_eq!(
            evaluate(now, &sched, &RuntimeState::default()),
            Decision::WaitUntil(now + TimeDelta::days(1))
        );
    }

    #[test]
    fn clear_pause_returns_to_the_interval() {
        let mut state = RuntimeState::default();
        mark_shown(&mut state, at(on(Weekday::Mon), 10, 0, 0));
        apply_pause(&mut state, at(on(Weekday::Mon), 16, 0, 0));
        clear_pause(&mut state);
        assert_eq!(
            evaluate(at(on(Weekday::Mon), 10, 20, 0), &schedule(), &state),
            Decision::WaitUntil(at(on(Weekday::Mon), 10, 30, 0))
        );
    }

    #[test]
    fn week_scan_never_waits_in_the_past_or_shows_on_weekends() {
        let sched = schedule();
        let mut state = RuntimeState::default();
        let origin = at(on(Weekday::Mon), 0, 0, 0);
        for step in (0..8 * 24 * 60).step_by(17) {
            let now = origin + TimeDelta::minutes(step);
            match evaluate(now, &sched, &state) {
                Decision::Show => {
                    assert!(
                        in_window(now, &sched),
                        "unexpected show at {now} ({:?})",
                        now.weekday()
                    );
                    mark_shown(&mut state, now);
                }
                Decision::WaitUntil(target) => {
                    assert!(target > now, "{target} <= {now}");
                }
            }
        }
    }

    #[test]
    fn chinese_holiday_skips_statutory_leave_and_honors_makeup() {
        let sched = Schedule {
            respect_chinese_holidays: true,
            ..schedule()
        };
        // 2025-01-01 周三元旦放假
        let new_year = NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
        assert_eq!(
            evaluate(at(new_year, 10, 0, 0), &sched, &RuntimeState::default()),
            Decision::WaitUntil(at(NaiveDate::from_ymd_opt(2025, 1, 2).unwrap(), 9, 30, 0))
        );
        // 2025-01-26 周日春节调休补班
        let makeup = NaiveDate::from_ymd_opt(2025, 1, 26).unwrap();
        assert_eq!(makeup.weekday(), Weekday::Sun);
        assert_eq!(
            evaluate(at(makeup, 9, 30, 0), &sched, &RuntimeState::default()),
            Decision::Show
        );
    }
}
