//! 中国法定节假日与调休补班。
//!
//! 依据国务院放假安排：法定节假日不提醒；调休补班日按工作日提醒；
//! 其余日期仍尊重用户勾选的星期。

use chrono::NaiveDate;
use chinese_holiday::{chinese_holiday, DayKind};

/// 今天是否应按「中国节假日习惯」视为可提醒的工作日。
///
/// - 法定节假日（含落到周末的连休）→ 否
/// - 调休补班日 → 是
/// - 普通工作日 / 普通周末 → `None`，交给用户的星期设置
pub fn china_day_override(date: NaiveDate) -> Option<bool> {
    let kind = chinese_holiday(&date);
    if is_makeup_workday(kind) {
        return Some(true);
    }
    if is_statutory_holiday(kind) {
        return Some(false);
    }
    None
}

fn is_makeup_workday(kind: DayKind) -> bool {
    kind.is_workday() && !matches!(kind, DayKind::NormalWorkday)
}

fn is_statutory_holiday(kind: DayKind) -> bool {
    kind.is_holiday() && !matches!(kind, DayKind::NormalHoliday)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn new_years_day_is_off() {
        let date = NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
        assert_eq!(china_day_override(date), Some(false));
    }

    #[test]
    fn makeup_saturday_is_on() {
        // 2025-01-26 周日，春节调休补班
        let date = NaiveDate::from_ymd_opt(2025, 1, 26).unwrap();
        assert_eq!(china_day_override(date), Some(true));
    }

    #[test]
    fn plain_monday_defers_to_user_weekdays() {
        let date = NaiveDate::from_ymd_opt(2025, 3, 3).unwrap();
        assert_eq!(china_day_override(date), None);
    }
}
