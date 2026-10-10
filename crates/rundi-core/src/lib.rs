//! 喝水提醒的纯逻辑：工作日时间窗、间隔、暂停/稍后，以及配置文件读写。
//! 不依赖任何界面框架，方便在 Linux CI 上单测。

mod config;
mod holiday;
mod schedule;
mod state;
mod units;

pub use config::{
    app_config_dir, config_file_path, load_config, save_config, state_file_path, weekday_code,
    AppConfig, ConfigError,
};
pub use schedule::{
    apply_pause, apply_snooze, clear_pause, end_of_local_day, evaluate, mark_shown, Decision,
    RuntimeState, Schedule,
};
pub use state::{
    clear_history, convert_daily_unit, glasses_on, load_state, merged_history, record_intake,
    save_state, trim_history, DailyCount, DayIntake, HISTORY_RETENTION_DAYS,
};
pub use units::{
    convert_drink_amount, convert_intake_total, format_drink_ack, format_drink_prompt,
    format_intake, format_intake_approx, from_ml, parse_drink_unit, to_ml, DrinkUnit, ML_PER_CUP,
    ML_PER_SIP,
};
