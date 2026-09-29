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
pub use state::{glasses_on, load_state, record_intake, save_state, DailyCount, PersistedState};
pub use units::{
    format_drink_ack, format_drink_prompt, format_intake, parse_drink_unit, DrinkUnit,
};
