use chrono::TimeZone;

pub fn short_display_id(value: &str) -> String {
    if value.starts_with("0x") && value.len() > 10 {
        return format!("{}....{}", &value[..6], &value[value.len() - 4..]);
    }
    if value.len() <= 8 {
        return value.to_owned();
    }
    format!("{}....{}", &value[..4], &value[value.len() - 4..])
}

pub fn format_day_tag(sent_at_ns: i64) -> String {
    if sent_at_ns <= 0 {
        return "-- -- --".to_owned();
    }
    let secs = sent_at_ns / 1_000_000_000;
    chrono::Local
        .timestamp_opt(secs, 0)
        .single()
        .map(|value| value.format("%m-%d").to_string())
        .unwrap_or_else(|| "-- --".to_owned())
}

pub fn format_clock(sent_at_ns: i64) -> String {
    if sent_at_ns <= 0 {
        return "--:--:--".to_owned();
    }
    let secs = sent_at_ns / 1_000_000_000;
    chrono::Local
        .timestamp_opt(secs, 0)
        .single()
        .map(|value| value.format("%H:%M:%S").to_string())
        .unwrap_or_else(|| "--:--:--".to_owned())
}

#[cfg(test)]
mod tests {
    use super::{format_clock, format_day_tag, short_display_id};

    #[test]
    fn short_display_id_formats_hex_address() {
        assert_eq!(short_display_id("0x1234567890abcdef"), "0x1234....cdef");
    }

    #[test]
    fn short_display_id_formats_other_ids() {
        assert_eq!(short_display_id("abcdef1234567890"), "abcd....7890");
    }

    #[test]
    fn format_day_tag_handles_zero() {
        assert_eq!(format_day_tag(0), "-- -- --");
    }

    #[test]
    fn format_clock_handles_zero() {
        assert_eq!(format_clock(0), "--:--:--");
    }
}
