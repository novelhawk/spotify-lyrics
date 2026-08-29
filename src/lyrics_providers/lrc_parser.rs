use std::time::Duration;

use crate::application::{LyricsLine, LyricsSegment};

/// Parses an LRC string (synchronized lyrics) into a sorted list of `LyricsLine`s.
pub fn parse_lrc(content: &str) -> Vec<LyricsLine> {
    let mut lines = Vec::new();

    for raw_line in content.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Must begin with a bracket
        if !trimmed.starts_with('[') {
            continue;
        }

        // Collect all timestamps from the line: e.g. [00:12.34][00:24.56]Text
        let mut timestamps = Vec::new();
        let mut remainder = trimmed;

        while remainder.starts_with('[') {
            if let Some(close_idx) = remainder.find(']') {
                let tag = &remainder[1..close_idx];
                if let Some(ts) = parse_timestamp(tag) {
                    timestamps.push(ts);
                    remainder = &remainder[close_idx + 1..];
                } else {
                    // Metadata tag like [ar:...], [ti:...], [offset:...], etc.
                    timestamps.clear();
                    break;
                }
            } else {
                break;
            }
        }

        if timestamps.is_empty() {
            continue;
        }

        let text = remainder.trim().to_string();

        for ts in timestamps {
            lines.push(LyricsLine {
                start: ts,
                segments: vec![LyricsSegment {
                    start: ts,
                    text: text.clone(),
                }],
            });
        }
    }

    lines.sort_by_key(|l| l.start);
    lines
}

/// Parses an LRC timestamp tag like "01:23.45", "01:23.456", or "01:23" into a `Duration`.
fn parse_timestamp(tag: &str) -> Option<Duration> {
    let parts: Vec<&str> = tag.split(':').collect();
    if parts.len() != 2 {
        return None;
    }

    let minutes: u64 = parts[0].trim().parse().ok()?;
    let sec_part = parts[1].trim();

    if let Some((secs_str, frac_str)) = sec_part.split_once('.') {
        let secs: u64 = secs_str.parse().ok()?;
        let frac_millis: u64 = match frac_str.len() {
            0 => 0,
            1 => frac_str.parse::<u64>().ok()? * 100,
            2 => frac_str.parse::<u64>().ok()? * 10,
            3 => frac_str.parse::<u64>().ok()?,
            _ => frac_str[..3].parse::<u64>().ok()?,
        };
        Some(Duration::from_millis(
            minutes * 60 * 1000 + secs * 1000 + frac_millis,
        ))
    } else {
        let secs: u64 = sec_part.parse().ok()?;
        Some(Duration::from_millis(minutes * 60 * 1000 + secs * 1000))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_lrc() {
        let lrc = r#"
[ti:Bohemian Rhapsody]
[ar:Queen]
[00:12.43] Thunderbolt and lightning
[00:16.02] Galileo Figaro
[01:05.500] Mama, just killed a man
[01:10] Put a gun against his head
"#;
        let lines = parse_lrc(lrc);
        assert_eq!(lines.len(), 4);

        assert_eq!(lines[0].start, Duration::from_millis(12430));
        assert_eq!(lines[0].segments[0].text, "Thunderbolt and lightning");

        assert_eq!(lines[1].start, Duration::from_millis(16020));
        assert_eq!(lines[1].segments[0].text, "Galileo Figaro");

        assert_eq!(lines[2].start, Duration::from_millis(65500));
        assert_eq!(lines[2].segments[0].text, "Mama, just killed a man");

        assert_eq!(lines[3].start, Duration::from_millis(70000));
        assert_eq!(lines[3].segments[0].text, "Put a gun against his head");
    }

    #[test]
    fn test_multiple_timestamps_per_line() {
        let lrc = "[00:10.00][00:20.00] Chorus line";
        let lines = parse_lrc(lrc);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].start, Duration::from_secs(10));
        assert_eq!(lines[0].segments[0].text, "Chorus line");
        assert_eq!(lines[1].start, Duration::from_secs(20));
        assert_eq!(lines[1].segments[0].text, "Chorus line");
    }

    #[test]
    fn test_empty_lines_and_silence() {
        let lrc = "[00:05.00] \n[00:10.00]";
        let lines = parse_lrc(lrc);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].segments[0].text, "");
        assert_eq!(lines[1].segments[0].text, "");
    }
}
