use std::time::Duration;

use chrono::TimeDelta;
use serde::{Deserialize, Deserializer};

pub fn time_delta_ms<'de, D>(deserializer: D) -> Result<TimeDelta, D::Error>
where
    D: Deserializer<'de>,
{
    let millis = i64::deserialize(deserializer)?;
    Ok(TimeDelta::milliseconds(millis))
}

pub fn duration_ms<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    let millis = u64::deserialize(deserializer)?;
    Ok(Duration::from_millis(millis))
}

pub fn duration_ms_str<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    let str = String::deserialize(deserializer)?;
    let millis: u64 = str.parse().map_err(serde::de::Error::custom)?;
    Ok(Duration::from_millis(millis))
}

pub fn print_hhmm(duration: Duration) -> String {
    let secs = duration.as_secs();
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

pub fn print_hhmm_timedelta(duration: TimeDelta) -> String {
    let secs = duration.num_seconds();
    format!("{:02}:{:02}", secs / 60, secs % 60)
}
