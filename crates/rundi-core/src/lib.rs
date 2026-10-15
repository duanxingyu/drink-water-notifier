//! 喝水提醒的纯逻辑：工作日时间窗、间隔、暂停/稍后，以及配置文件读写。
//! 不依赖任何界面框架，方便在 Linux CI 上单测。

mod config;
mod goal;
mod holiday;
mod schedule;
mod state;
mod tips;
mod units;

pub use config::{
    app_config_dir, config_file_path, load_config, save_config, state_file_path, weekday_code,
    AppConfig, ConfigError, DESK_GOAL_ML_MAX, DESK_GOAL_ML_MIN, RECOMMENDED_DESK_GOAL_ML,
    RECOMMENDED_HIGH_INTAKE_AMOUNT_ML, RECOMMENDED_HIGH_INTAKE_INTERVAL_MINUTES,
};
pub use goal::{
    expected_desk_intake_ml, goal_progress, goal_remaining_ml, intake_ml, paced_interval_minutes,
    validate_desk_goal_ml,
};
pub use schedule::{
    apply_pause, apply_snooze, clear_pause, end_of_local_day, evaluate, is_working_day, mark_shown,
    Decision, RuntimeState, Schedule,
};
pub use state::{
    clear_history, convert_daily_unit, glasses_on, load_state, merged_history, record_intake,
    rollover_daily, save_state, trim_history, DailyCount, DayIntake, HISTORY_RETENTION_DAYS,
};
pub use tips::{
    expected_for_prompt, format_contextual_prompt, format_day_summary, history_excluding_today,
    summary_due, PromptContext,
};
pub use units::{
    convert_drink_amount, convert_intake_total, format_drink_ack, format_drink_prompt,
    format_intake, format_intake_approx, from_ml, parse_drink_unit, to_ml, DrinkUnit, ML_PER_CUP,
    ML_PER_SIP,
};
