//! Minimal UTC time handling: epoch seconds <-> civil dates, with the two
//! textual formats sarDa needs (sadf's `YYYY-MM-DD HH:MM:SS UTC` and RFC 3339).

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn num(s: &str) -> Option<i64> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

fn parse_date_time(date: &str, time: &str) -> Option<i64> {
    let mut d = date.split('-');
    let (y, m, day) = (num(d.next()?)?, num(d.next()?)?, num(d.next()?)?);
    if d.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&day) {
        return None;
    }
    let mut t = time.split(':');
    let h = num(t.next()?)?;
    let mi = num(t.next()?)?;
    let s = match t.next() {
        Some(sec) => num(sec.split('.').next()?)?,
        None => 0,
    };
    if t.next().is_some() || h > 23 || mi > 59 || s > 60 {
        return None;
    }
    Some(days_from_civil(y, m, day) * 86400 + h * 3600 + mi * 60 + s)
}

/// Parses sadf's `-d` timestamp, e.g. `2026-11-25 11:51:56 UTC`.
pub fn parse_sadf(ts: &str) -> Option<i64> {
    let ts = ts.trim().strip_suffix("UTC")?.trim_end();
    let (date, time) = ts.split_once(' ')?;
    parse_date_time(date, time)
}

/// Parses an RFC 3339 timestamp. A missing offset means UTC. The date and
/// time may be separated by `T` or a space, and `Z` is accepted.
pub fn parse_user(input: &str) -> Result<i64, String> {
    let bad = || {
        format!(
            "invalid time '{input}': expected RFC 3339, e.g. 2026-11-25T08:00:00Z or 2026-11-25T08:00:00+01:00 (no offset means UTC)"
        )
    };
    let s = input.trim();
    let (date, rest) = s
        .split_once('T')
        .or_else(|| s.split_once(' '))
        .unwrap_or((s, "00:00:00"));
    let (time, offset) = if let Some(t) = rest.strip_suffix(['Z', 'z']) {
        (t, 0)
    } else if let Some(pos) = rest.rfind(['+', '-']) {
        let (t, off) = rest.split_at(pos);
        let sign = if off.starts_with('-') { -1 } else { 1 };
        let off = &off[1..];
        let (oh, om) = off.split_once(':').unwrap_or((off, "0"));
        let secs = num(oh).ok_or_else(bad)? * 3600 + num(om).ok_or_else(bad)? * 60;
        (t, sign * secs)
    } else {
        (rest, 0)
    };
    let base = parse_date_time(date, time).ok_or_else(bad)?;
    Ok(base - offset)
}

/// Formats an instant the way `sadf -d` prints it: `2026-11-25 11:51:56 UTC`.
pub fn format_sadf(epoch: i64) -> String {
    let rfc = format_rfc3339(epoch);
    format!("{} {} UTC", &rfc[..10], &rfc[11..19])
}

pub fn format_rfc3339(epoch: i64) -> String {
    let days = epoch.div_euclid(86400);
    let secs = epoch.rem_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sadf_timestamp_round_trips() {
        let t = parse_sadf("2026-11-25 11:51:56 UTC").unwrap();
        assert_eq!(format_rfc3339(t), "2026-11-25T11:51:56Z");
    }

    #[test]
    fn sadf_formatting_round_trips_with_parsing() {
        for text in [
            "2026-11-25 11:51:56 UTC",
            "1970-01-01 00:00:00 UTC",
            "2024-02-29 23:59:59 UTC",
        ] {
            assert_eq!(format_sadf(parse_sadf(text).unwrap()), text);
        }
    }

    #[test]
    fn epoch_zero() {
        assert_eq!(parse_sadf("1970-01-01 00:00:00 UTC"), Some(0));
        assert_eq!(format_rfc3339(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn leap_day() {
        let t = parse_user("2024-02-29T12:00:00Z").unwrap();
        assert_eq!(format_rfc3339(t), "2024-02-29T12:00:00Z");
        assert_eq!(parse_user("2024-03-01T00:00:00Z").unwrap() - t, 12 * 3600);
    }

    #[test]
    fn offsets_are_applied() {
        let utc = parse_user("2026-11-25T07:00:00Z").unwrap();
        assert_eq!(parse_user("2026-11-25T08:00:00+01:00").unwrap(), utc);
        assert_eq!(parse_user("2026-11-25T02:00:00-05:00").unwrap(), utc);
    }

    #[test]
    fn missing_offset_is_utc() {
        assert_eq!(
            parse_user("2026-11-25T07:00:00").unwrap(),
            parse_user("2026-11-25T07:00:00Z").unwrap()
        );
        assert_eq!(
            parse_user("2026-11-25").unwrap(),
            parse_user("2026-11-25T00:00:00Z").unwrap()
        );
    }

    #[test]
    fn garbage_is_rejected() {
        assert!(parse_user("yesterday").is_err());
        assert!(parse_user("2026-13-01T00:00:00Z").is_err());
        assert!(parse_sadf("2026-11-25 11:51:56").is_none());
    }
}
