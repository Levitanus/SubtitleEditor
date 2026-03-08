use crate::TimecodeRange;
use std::time::Duration;

#[derive(Debug, Clone, Copy, Default)]
pub struct TimecodeParts {
    pub hours: i64,
    pub minutes: i64,
    pub seconds: i64,
    pub millis: i64,
}

pub fn parse_sbv_timecode(raw: &str) -> Option<Duration> {
    let (h, rest) = raw.split_once(':')?;
    let (m, rest) = rest.split_once(':')?;
    let (s, ms) = rest.split_once('.')?;

    let hours: u64 = h.parse().ok()?;
    let minutes: u64 = m.parse().ok()?;
    let seconds: u64 = s.parse().ok()?;
    let millis: u64 = ms.parse().ok()?;

    if minutes > 59 || seconds > 59 || millis > 999 {
        return None;
    }

    let total_millis = hours
        .saturating_mul(3_600_000)
        .saturating_add(minutes.saturating_mul(60_000))
        .saturating_add(seconds.saturating_mul(1_000))
        .saturating_add(millis);

    Some(Duration::from_millis(total_millis))
}

pub fn parse_srt_timecode(raw: &str) -> Option<Duration> {
    let (h, rest) = raw.split_once(':')?;
    let (m, rest) = rest.split_once(':')?;
    let (s, ms) = rest.split_once(',')?;

    let hours: u64 = h.parse().ok()?;
    let minutes: u64 = m.parse().ok()?;
    let seconds: u64 = s.parse().ok()?;
    let millis: u64 = ms.parse().ok()?;

    if minutes > 59 || seconds > 59 || millis > 999 {
        return None;
    }

    let total_millis = hours
        .saturating_mul(3_600_000)
        .saturating_add(minutes.saturating_mul(60_000))
        .saturating_add(seconds.saturating_mul(1_000))
        .saturating_add(millis);

    Some(Duration::from_millis(total_millis))
}

pub fn format_sbv_timecode(duration: Duration) -> String {
    let total_millis = duration.as_millis() as u64;
    let hours = total_millis / 3_600_000;
    let rem_after_hours = total_millis % 3_600_000;
    let minutes = rem_after_hours / 60_000;
    let rem_after_minutes = rem_after_hours % 60_000;
    let seconds = rem_after_minutes / 1_000;
    let millis = rem_after_minutes % 1_000;

    format!("{}:{:02}:{:02}.{:03}", hours, minutes, seconds, millis)
}

pub fn format_srt_timecode(duration: Duration) -> String {
    let total_millis = duration.as_millis() as u64;
    let hours = total_millis / 3_600_000;
    let rem_after_hours = total_millis % 3_600_000;
    let minutes = rem_after_hours / 60_000;
    let rem_after_minutes = rem_after_hours % 60_000;
    let seconds = rem_after_minutes / 1_000;
    let millis = rem_after_minutes % 1_000;

    format!("{:02}:{:02}:{:02},{:03}", hours, minutes, seconds, millis)
}

pub fn parse_sbv_time_range(line: &str) -> Option<TimecodeRange> {
    let (start_raw, end_raw) = line.split_once(',')?;
    let start = parse_sbv_timecode(start_raw.trim())?;
    let end = parse_sbv_timecode(end_raw.trim())?;
    Some(TimecodeRange { start, end })
}

pub fn parse_srt_time_range(line: &str) -> Option<TimecodeRange> {
    let (start_raw, end_raw) = line.split_once("-->")?;
    let start = parse_srt_timecode(start_raw.trim())?;
    let end = parse_srt_timecode(end_raw.trim())?;
    Some(TimecodeRange { start, end })
}

pub fn split_timecode_range(
    range: &TimecodeRange,
    left_len: usize,
    right_len: usize,
) -> (TimecodeRange, TimecodeRange) {
    let total = if range.end > range.start {
        range.end - range.start
    } else {
        Duration::from_millis(0)
    };

    let left_units = left_len as u128;
    let right_units = right_len as u128;
    let total_units = left_units + right_units;

    let left_nanos = if total_units == 0 {
        total.as_nanos() / 2
    } else {
        total.as_nanos().saturating_mul(left_units) / total_units
    };

    let left_duration = duration_from_nanos(left_nanos);
    let mut split_point = range.start.checked_add(left_duration).unwrap_or(range.end);
    if split_point > range.end {
        split_point = range.end;
    }

    (
        TimecodeRange {
            start: range.start,
            end: split_point,
        },
        TimecodeRange {
            start: split_point,
            end: range.end,
        },
    )
}

pub fn duration_to_parts(duration: Duration) -> TimecodeParts {
    let total_millis = duration.as_millis() as u64;
    let hours = (total_millis / 3_600_000) as i64;
    let rem_after_hours = total_millis % 3_600_000;
    let minutes = (rem_after_hours / 60_000) as i64;
    let rem_after_minutes = rem_after_hours % 60_000;
    let seconds = (rem_after_minutes / 1_000) as i64;
    let millis = (rem_after_minutes % 1_000) as i64;

    TimecodeParts {
        hours,
        minutes,
        seconds,
        millis,
    }
}

pub fn parts_to_duration(parts: TimecodeParts) -> Duration {
    let hours = parts.hours.max(0) as u64;
    let minutes = parts.minutes.clamp(0, 59) as u64;
    let seconds = parts.seconds.clamp(0, 59) as u64;
    let millis = parts.millis.clamp(0, 999) as u64;

    let total_millis = hours
        .saturating_mul(3_600_000)
        .saturating_add(minutes.saturating_mul(60_000))
        .saturating_add(seconds.saturating_mul(1_000))
        .saturating_add(millis);

    Duration::from_millis(total_millis)
}

fn duration_from_nanos(nanos: u128) -> Duration {
    let secs_u128 = nanos / 1_000_000_000;
    let sub_nanos = (nanos % 1_000_000_000) as u32;
    let secs = secs_u128.min(u64::MAX as u128) as u64;
    Duration::new(secs, sub_nanos)
}

pub mod duration_millis_serde {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(value: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(value.as_millis() as u64)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let millis = u64::deserialize(deserializer)?;
        Ok(Duration::from_millis(millis))
    }
}
