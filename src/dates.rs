use std::time::{SystemTime, UNIX_EPOCH};

/// 將嚴格的 YYYY-MM-DD 日期轉成 Unix epoch 日數。
pub fn parse_date(value: &str) -> Option<i64> {
    if value.len() != 10
        || value.as_bytes().get(4) != Some(&b'-')
        || value.as_bytes().get(7) != Some(&b'-')
    {
        return None;
    }
    if !value
        .bytes()
        .enumerate()
        .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
    {
        return None;
    }
    let year: i64 = value.get(0..4)?.parse().ok()?;
    let month: i64 = value.get(5..7)?.parse().ok()?;
    let day: i64 = value.get(8..10)?.parse().ok()?;
    if year < 1 || !(1..=12).contains(&month) {
        return None;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let month_days = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    if !(1..=month_days).contains(&day) {
        return None;
    }
    Some(days_from_civil(year, month, day))
}

/// 將 RFC 3339 時間戳轉成 Unix 秒數。
pub fn parse_timestamp(value: &str) -> Option<i64> {
    let (date, time) = value.split_once('T')?;
    let day = parse_date(date)?;
    let (clock, offset) = if let Some(clock) = time.strip_suffix('Z') {
        (clock, 0)
    } else {
        let split = time
            .char_indices()
            .find(|(i, c)| *i > 0 && (*c == '+' || *c == '-'))?;
        let (clock, zone) = time.split_at(split.0);
        let sign = if split.1 == '+' { 1 } else { -1 };
        let zone = zone.get(1..)?;
        let (hours, minutes) = zone.split_once(':')?;
        let hours: i64 = hours.parse().ok()?;
        let minutes: i64 = minutes.parse().ok()?;
        if hours > 23 || minutes > 59 {
            return None;
        }
        (clock, sign * (hours * 3600 + minutes * 60))
    };
    let mut fields = clock.split(':');
    let hour: i64 = fields.next()?.parse().ok()?;
    let minute: i64 = fields.next()?.parse().ok()?;
    let second_text = fields.next()?;
    if fields.next().is_some() || hour > 23 || minute > 59 {
        return None;
    }
    let second: i64 = second_text.split('.').next()?.parse().ok()?;
    if second > 60 {
        return None;
    }
    Some(day * 86_400 + hour * 3600 + minute * 60 + second - offset)
}

pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn epoch_day_from_unix(seconds: i64) -> i64 {
    seconds.div_euclid(86_400)
}

pub fn format_timestamp(seconds: i64) -> String {
    let day = epoch_day_from_unix(seconds);
    let (year, month, date) = civil_from_days(day);
    let second_of_day = seconds.rem_euclid(86_400);
    let hour = second_of_day / 3600;
    let minute = (second_of_day % 3600) / 60;
    format!("{year:04}-{month:02}-{date:02} {hour:02}:{minute:02} UTC")
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let adjusted_month = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * adjusted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let date = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month, date)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_dates_and_leap_days() {
        assert_eq!(parse_date("2026-11-20"), Some(20_777));
        assert!(parse_date("2024-02-29").is_some());
        assert!(parse_date("2023-02-29").is_none());
    }

    #[test]
    fn rejects_malformed_dates_without_error() {
        for value in [
            "2026-2-03",
            "2026/02/03",
            "2026-13-01",
            "2026-04-31",
            "no date",
        ] {
            assert_eq!(parse_date(value), None);
        }
    }

    #[test]
    fn parses_rfc3339_offsets() {
        assert_eq!(
            parse_timestamp("2026-11-12T10:30:00Z"),
            parse_timestamp("2026-11-12T18:30:00+08:00")
        );
        assert!(parse_timestamp("not a timestamp").is_none());
    }

    #[test]
    fn formats_unix_timestamps() {
        assert_eq!(format_timestamp(0), "1970-01-01 00:00 UTC");
    }
}
