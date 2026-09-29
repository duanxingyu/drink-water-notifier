//! 饮水计量单位：杯、毫升、口。

use crate::config::ConfigError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrinkUnit {
    /// 杯
    Cup,
    /// 毫升
    Ml,
    /// 口（一口、两口）
    Sip,
}

impl Default for DrinkUnit {
    fn default() -> Self {
        Self::Cup
    }
}

impl DrinkUnit {
    pub fn code(self) -> &'static str {
        match self {
            Self::Cup => "cup",
            Self::Ml => "ml",
            Self::Sip => "sip",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Cup => "杯",
            Self::Ml => "毫升",
            Self::Sip => "口",
        }
    }

    pub fn default_amount(self) -> u32 {
        match self {
            Self::Cup | Self::Sip => 1,
            Self::Ml => 200,
        }
    }
}

pub fn parse_drink_unit(value: &str) -> Result<DrinkUnit, ConfigError> {
    match value.trim() {
        "cup" | "Cup" | "杯" => Ok(DrinkUnit::Cup),
        "ml" | "mL" | "ML" | "毫升" => Ok(DrinkUnit::Ml),
        "sip" | "Sip" | "口" => Ok(DrinkUnit::Sip),
        other => Err(ConfigError::InvalidDrinkUnit(other.to_string())),
    }
}

pub fn validate_drink_amount(unit: DrinkUnit, amount: u32) -> Result<(), ConfigError> {
    let ok = match unit {
        DrinkUnit::Cup | DrinkUnit::Sip => (1..=20).contains(&amount),
        DrinkUnit::Ml => (10..=1000).contains(&amount),
    };
    if ok {
        Ok(())
    } else {
        Err(ConfigError::DrinkAmountOutOfRange)
    }
}

/// 今日总量文案，例如 `3 杯`、`600 毫升`、`两口`。
pub fn format_intake(amount: u32, unit: DrinkUnit) -> String {
    match unit {
        DrinkUnit::Cup => format!("{amount} 杯"),
        DrinkUnit::Ml => format!("{amount} 毫升"),
        DrinkUnit::Sip => match amount {
            0 => "0 口".into(),
            1 => "一口".into(),
            2 => "两口".into(),
            n => format!("{n} 口"),
        },
    }
}

/// 点「喝了」后的短确认，例如「记下这一杯」。
pub fn format_drink_ack(unit: DrinkUnit) -> &'static str {
    match unit {
        DrinkUnit::Cup => "记下这一杯",
        DrinkUnit::Ml => "记下这些水",
        DrinkUnit::Sip => "记下这一口",
    }
}

/// 提醒副文案。
pub fn format_drink_prompt(unit: DrinkUnit) -> &'static str {
    match unit {
        DrinkUnit::Cup => "离开屏幕一小会儿，喝一杯水。",
        DrinkUnit::Ml => "离开屏幕一小会儿，喝一点水。",
        DrinkUnit::Sip => "离开屏幕一小会儿，喝一口水。",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sip_uses_colloquial_one_and_two() {
        assert_eq!(format_intake(1, DrinkUnit::Sip), "一口");
        assert_eq!(format_intake(2, DrinkUnit::Sip), "两口");
        assert_eq!(format_intake(3, DrinkUnit::Sip), "3 口");
    }

    #[test]
    fn parses_chinese_and_english_codes() {
        assert_eq!(parse_drink_unit("杯").unwrap(), DrinkUnit::Cup);
        assert_eq!(parse_drink_unit("毫升").unwrap(), DrinkUnit::Ml);
        assert_eq!(parse_drink_unit("口").unwrap(), DrinkUnit::Sip);
        assert_eq!(parse_drink_unit("ml").unwrap(), DrinkUnit::Ml);
    }
}
