//! 饮水计量单位：杯、毫升、口。

use crate::config::ConfigError;

/// 1 杯 ≈ 350 毫升。
pub const ML_PER_CUP: u32 = 350;
/// 1 口 ≈ 80 毫升。
pub const ML_PER_SIP: u32 = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
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

    /// 历史/对照展示时优先换算到的「另一种体积单位」。
    pub fn counterpart_for_display(self) -> Self {
        match self {
            Self::Cup | Self::Sip => Self::Ml,
            Self::Ml => Self::Cup,
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

/// 换算到毫升。
pub fn to_ml(amount: u32, unit: DrinkUnit) -> u32 {
    match unit {
        DrinkUnit::Ml => amount,
        DrinkUnit::Cup => amount.saturating_mul(ML_PER_CUP),
        DrinkUnit::Sip => amount.saturating_mul(ML_PER_SIP),
    }
}

/// 从毫升换算到目标单位（四舍五入）。
/// `min_one` 为 true 时用于「每次喝多少」，结果至少为 1（毫升则至少 10 的合法下限由调用方 clamp）。
pub fn from_ml(ml: u32, unit: DrinkUnit) -> u32 {
    match unit {
        DrinkUnit::Ml => ml,
        DrinkUnit::Cup => ((f64::from(ml) / f64::from(ML_PER_CUP)).round() as u32).max(0),
        DrinkUnit::Sip => ((f64::from(ml) / f64::from(ML_PER_SIP)).round() as u32).max(0),
    }
}

/// 切换单位时换算「每次喝多少」，并夹到合法范围。
pub fn convert_drink_amount(amount: u32, from: DrinkUnit, to: DrinkUnit) -> u32 {
    if from == to {
        return amount;
    }
    let ml = to_ml(amount, from);
    let converted = from_ml(ml, to);
    match to {
        DrinkUnit::Ml => converted.clamp(10, 1000),
        DrinkUnit::Cup | DrinkUnit::Sip => converted.clamp(1, 20),
    }
}

/// 切换单位时换算今日/历史总量（允许为 0）。
pub fn convert_intake_total(amount: u32, from: DrinkUnit, to: DrinkUnit) -> u32 {
    if from == to {
        return amount;
    }
    from_ml(to_ml(amount, from), to)
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

/// 换算对照文案，例如 `约 700 毫升`；同单位或换算结果为 0 时返回 None。
pub fn format_intake_approx(amount: u32, from: DrinkUnit, to: DrinkUnit) -> Option<String> {
    if from == to {
        return None;
    }
    let converted = convert_intake_total(amount, from, to);
    if converted == 0 && amount > 0 {
        // 太少不足以进位时仍给出毫升对照更直观
        if to != DrinkUnit::Ml {
            let ml = to_ml(amount, from);
            if ml > 0 {
                return Some(format!("约 {ml} 毫升"));
            }
        }
        return None;
    }
    Some(format!("约 {}", format_intake(converted, to)))
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

    #[test]
    fn cup_and_sip_ml_rates() {
        assert_eq!(to_ml(1, DrinkUnit::Cup), 350);
        assert_eq!(to_ml(1, DrinkUnit::Sip), 80);
        assert_eq!(from_ml(350, DrinkUnit::Cup), 1);
        assert_eq!(from_ml(80, DrinkUnit::Sip), 1);
        assert_eq!(convert_drink_amount(1, DrinkUnit::Cup, DrinkUnit::Ml), 350);
        assert_eq!(convert_intake_total(2, DrinkUnit::Cup, DrinkUnit::Ml), 700);
        assert_eq!(convert_intake_total(700, DrinkUnit::Ml, DrinkUnit::Cup), 2);
    }
}
