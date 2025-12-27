use crate::error::Result;
use std::time::Duration;

/// Parsed LRC file containing metadata and synchronized lines
#[derive(Debug, Clone, Default)]
pub struct LrcFile {
    pub metadata: LrcMetadata,
    pub lines: Vec<LrcLine>,
}

/// LRC metadata from ID tags
#[derive(Debug, Clone, Default)]
pub struct LrcMetadata {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub author: Option<String>,
    pub length: Option<Duration>,
    pub offset: i64, // milliseconds, can be negative
}

/// A single line of lyrics with timing
#[derive(Debug, Clone)]
pub struct LrcLine {
    pub start_time: Duration,
    pub text: String,
    /// Word-level timing for enhanced LRC
    pub words: Option<Vec<LrcWord>>,
}

/// Word-level timing for enhanced LRC format
#[derive(Debug, Clone)]
pub struct LrcWord {
    pub start_time: Duration,
    pub end_time: Option<Duration>,
    pub text: String,
}

impl LrcFile {
    /// Parse an LRC string into an LrcFile
    pub fn parse(input: &str) -> Result<Self> {
        let mut metadata = LrcMetadata::default();
        let mut lines = Vec::new();

        for line in input.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            // Try to parse as ID tag first
            if let Some(tag) = parse_id_tag(line) {
                match tag.0.to_lowercase().as_str() {
                    "ti" => metadata.title = Some(tag.1),
                    "ar" => metadata.artist = Some(tag.1),
                    "al" => metadata.album = Some(tag.1),
                    "au" => metadata.author = Some(tag.1),
                    "length" => metadata.length = parse_duration_tag(&tag.1),
                    "offset" => {
                        if let Ok(offset) = tag.1.parse::<i64>() {
                            metadata.offset = offset;
                        }
                    }
                    _ => {} // Ignore unknown tags
                }
                continue;
            }

            // Try to parse as lyric line(s)
            if let Some(parsed_lines) = parse_lyric_line(line) {
                lines.extend(parsed_lines);
            }
        }

        // Apply offset to all lines
        if metadata.offset != 0 {
            for line in &mut lines {
                line.start_time = apply_offset(line.start_time, metadata.offset);
                if let Some(ref mut words) = line.words {
                    for word in words {
                        word.start_time = apply_offset(word.start_time, metadata.offset);
                        if let Some(end) = word.end_time {
                            word.end_time = Some(apply_offset(end, metadata.offset));
                        }
                    }
                }
            }
        }

        // Sort lines by start time
        lines.sort_by_key(|l| l.start_time);

        Ok(LrcFile { metadata, lines })
    }

    /// Find the current line for a given playback position
    pub fn current_line(&self, position: Duration) -> Option<&LrcLine> {
        // Find the last line that started before or at the current position
        self.lines
            .iter()
            .rev()
            .find(|line| line.start_time <= position)
    }

    /// Find the current line index for a given playback position
    pub fn current_line_index(&self, position: Duration) -> Option<usize> {
        self.lines
            .iter()
            .enumerate()
            .rev()
            .find(|(_, line)| line.start_time <= position)
            .map(|(i, _)| i)
    }

    /// Get lines around the current position for display
    pub fn visible_lines(&self, position: Duration, before: usize, after: usize) -> Vec<&LrcLine> {
        let current_idx = self.current_line_index(position).unwrap_or(0);

        let start = current_idx.saturating_sub(before);
        let end = (current_idx + after + 1).min(self.lines.len());

        self.lines[start..end].iter().collect()
    }
}

impl LrcLine {
    /// Calculate progress through this line (0.0 to 1.0) based on word timing or duration estimate
    pub fn progress(&self, position: Duration, next_line_start: Option<Duration>) -> f32 {
        if position < self.start_time {
            return 0.0;
        }

        // If we have word timing, calculate based on that
        if let Some(ref words) = self.words {
            if let Some(last_word) = words.last() {
                let end_time = last_word
                    .end_time
                    .or(next_line_start)
                    .unwrap_or(self.start_time + Duration::from_secs(5));

                if position >= end_time {
                    return 1.0;
                }

                let total_duration = end_time.saturating_sub(self.start_time);
                let elapsed = position.saturating_sub(self.start_time);

                if total_duration.is_zero() {
                    return 1.0;
                }

                return (elapsed.as_secs_f32() / total_duration.as_secs_f32()).clamp(0.0, 1.0);
            }
        }

        // Estimate based on next line start or default duration
        let end_time = next_line_start.unwrap_or(self.start_time + Duration::from_secs(5));

        if position >= end_time {
            return 1.0;
        }

        let total_duration = end_time.saturating_sub(self.start_time);
        let elapsed = position.saturating_sub(self.start_time);

        if total_duration.is_zero() {
            return 1.0;
        }

        (elapsed.as_secs_f32() / total_duration.as_secs_f32()).clamp(0.0, 1.0)
    }

    /// Get word progress for character-level fill mode
    pub fn word_progress(&self, position: Duration, char_index: usize) -> f32 {
        let total_chars = self.text.chars().count();
        if total_chars == 0 {
            return 1.0;
        }

        // If we have word timing, use it for more accurate progress
        if let Some(ref words) = self.words {
            let mut current_char = 0;
            for word in words {
                let word_len = word.text.chars().count();
                let word_end_char = current_char + word_len;

                if char_index < word_end_char {
                    // This character is in this word
                    if position < word.start_time {
                        return 0.0;
                    }
                    if let Some(end) = word.end_time {
                        if position >= end {
                            return 1.0;
                        }
                        let word_duration = end.saturating_sub(word.start_time);
                        let elapsed = position.saturating_sub(word.start_time);
                        if word_duration.is_zero() {
                            return 1.0;
                        }
                        // Interpolate within the word
                        let char_in_word = char_index - current_char;
                        let char_progress = char_in_word as f32 / word_len as f32;
                        let time_progress =
                            elapsed.as_secs_f32() / word_duration.as_secs_f32();
                        return if time_progress >= char_progress {
                            1.0
                        } else {
                            0.0
                        };
                    }
                    return 1.0;
                }
                current_char = word_end_char;
                // Account for space between words
                if current_char < total_chars {
                    current_char += 1;
                }
            }
        }

        // Fallback to linear interpolation based on character position
        let line_progress = self.progress(position, None);
        let char_threshold = char_index as f32 / total_chars as f32;

        if line_progress >= char_threshold {
            1.0
        } else {
            0.0
        }
    }
}

/// Parse an ID tag like [ti:Title] or [ar:Artist]
fn parse_id_tag(line: &str) -> Option<(String, String)> {
    if !line.starts_with('[') || !line.contains(':') {
        return None;
    }

    let end = line.find(']')?;
    let content = &line[1..end];

    // Check if this looks like a timestamp (contains only digits and colons/dots)
    let first_colon = content.find(':')?;
    let tag = &content[..first_colon];

    // If the tag part looks like a number, it's a timestamp, not an ID tag
    if tag.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }

    let value = content[first_colon + 1..].trim().to_string();
    Some((tag.to_string(), value))
}

/// Parse a duration string like "mm:ss" or "mm:ss.xx"
fn parse_duration_tag(s: &str) -> Option<Duration> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return None;
    }

    let minutes: u64 = parts[0].parse().ok()?;
    let seconds: f64 = parts[1].parse().ok()?;

    Some(Duration::from_secs_f64(minutes as f64 * 60.0 + seconds))
}

/// Parse a lyric line like [00:12.34]Hello world or [00:12.34][00:15.67]Same lyrics
fn parse_lyric_line(line: &str) -> Option<Vec<LrcLine>> {
    let mut results = Vec::new();
    let mut remaining = line;
    let mut timestamps = Vec::new();

    // Extract all timestamps at the beginning
    while remaining.starts_with('[') {
        if let Some(end) = remaining.find(']') {
            let bracket_content = &remaining[1..end];

            // Try to parse as timestamp
            if let Some(time) = parse_timestamp(bracket_content) {
                timestamps.push(time);
                remaining = &remaining[end + 1..];
            } else {
                // Not a timestamp, might be an ID tag or something else
                break;
            }
        } else {
            break;
        }
    }

    if timestamps.is_empty() {
        return None;
    }

    let text = remaining.trim();

    // Check for enhanced LRC format with word timing
    let words = parse_enhanced_words(text);

    // Create a line for each timestamp (handles multi-timestamp lines)
    for timestamp in timestamps {
        results.push(LrcLine {
            start_time: timestamp,
            text: if words.is_some() {
                // Reconstruct text from words for enhanced format
                words
                    .as_ref()
                    .map(|w| w.iter().map(|word| word.text.as_str()).collect::<Vec<_>>().join(" "))
                    .unwrap_or_else(|| text.to_string())
            } else {
                text.to_string()
            },
            words: words.clone(),
        });
    }

    Some(results)
}

/// Parse a timestamp string like "00:12.34" or "00:12:34"
fn parse_timestamp(s: &str) -> Option<Duration> {
    // Format: [mm:ss.xx] or [mm:ss:xx] or [mm:ss]
    let s = s.trim();

    // Split by colon
    let parts: Vec<&str> = s.split(':').collect();

    match parts.len() {
        2 => {
            // mm:ss.xx or mm:ss
            let minutes: u64 = parts[0].parse().ok()?;
            let seconds_str = parts[1];

            // Handle both . and : as decimal separator
            let seconds: f64 = if seconds_str.contains('.') {
                seconds_str.parse().ok()?
            } else {
                seconds_str.parse().ok()?
            };

            Some(Duration::from_secs_f64(minutes as f64 * 60.0 + seconds))
        }
        3 => {
            // mm:ss:xx (hundredths)
            let minutes: u64 = parts[0].parse().ok()?;
            let seconds: u64 = parts[1].parse().ok()?;
            let hundredths: u64 = parts[2].parse().ok()?;

            Some(Duration::from_millis(
                minutes * 60 * 1000 + seconds * 1000 + hundredths * 10,
            ))
        }
        _ => None,
    }
}

/// Parse enhanced LRC format with word timing
/// Format: <mm:ss.xx> word1 <mm:ss.xx> word2 ...
fn parse_enhanced_words(text: &str) -> Option<Vec<LrcWord>> {
    if !text.contains('<') {
        return None;
    }

    let mut words = Vec::new();
    let mut remaining = text.trim();

    while !remaining.is_empty() {
        // Look for timestamp
        if remaining.starts_with('<') {
            if let Some(end) = remaining.find('>') {
                let timestamp_str = &remaining[1..end];
                if let Some(start_time) = parse_timestamp(timestamp_str) {
                    remaining = &remaining[end + 1..];

                    // Find the word (until next < or end)
                    let word_end = remaining.find('<').unwrap_or(remaining.len());
                    let word_text = remaining[..word_end].trim();

                    if !word_text.is_empty() {
                        words.push(LrcWord {
                            start_time,
                            end_time: None,
                            text: word_text.to_string(),
                        });
                    }

                    remaining = &remaining[word_end..];
                } else {
                    // Invalid timestamp, skip
                    remaining = &remaining[end + 1..];
                }
            } else {
                break;
            }
        } else {
            // Skip non-timestamp content
            let next_timestamp = remaining.find('<').unwrap_or(remaining.len());
            remaining = &remaining[next_timestamp..];
        }
    }

    // Set end times based on next word start time
    for i in 0..words.len() {
        if i + 1 < words.len() {
            words[i].end_time = Some(words[i + 1].start_time);
        }
    }

    if words.is_empty() {
        None
    } else {
        Some(words)
    }
}

/// Apply a millisecond offset to a duration (can be negative)
fn apply_offset(duration: Duration, offset_ms: i64) -> Duration {
    if offset_ms >= 0 {
        duration + Duration::from_millis(offset_ms as u64)
    } else {
        duration.saturating_sub(Duration::from_millis((-offset_ms) as u64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_lrc() {
        let input = "[00:12.34]Hello world";
        let result = LrcFile::parse(input).unwrap();
        assert_eq!(result.lines.len(), 1);
        assert_eq!(result.lines[0].start_time, Duration::from_millis(12340));
        assert_eq!(result.lines[0].text, "Hello world");
    }

    #[test]
    fn test_parse_multiple_lines() {
        let input = r#"
[00:05.00]First line
[00:10.00]Second line
[00:15.00]Third line
"#;
        let result = LrcFile::parse(input).unwrap();
        assert_eq!(result.lines.len(), 3);
        assert_eq!(result.lines[0].text, "First line");
        assert_eq!(result.lines[1].text, "Second line");
        assert_eq!(result.lines[2].text, "Third line");
    }

    #[test]
    fn test_parse_id_tags() {
        let input = r#"
[ti:Song Title]
[ar:Artist Name]
[al:Album Name]
[00:05.00]Lyrics here
"#;
        let result = LrcFile::parse(input).unwrap();
        assert_eq!(result.metadata.title, Some("Song Title".to_string()));
        assert_eq!(result.metadata.artist, Some("Artist Name".to_string()));
        assert_eq!(result.metadata.album, Some("Album Name".to_string()));
    }

    #[test]
    fn test_parse_offset() {
        let input = r#"
[offset:500]
[00:10.00]Test
"#;
        let result = LrcFile::parse(input).unwrap();
        // 10.00s + 0.5s offset = 10.5s
        assert_eq!(result.lines[0].start_time, Duration::from_millis(10500));
    }

    #[test]
    fn test_parse_negative_offset() {
        let input = r#"
[offset:-500]
[00:10.00]Test
"#;
        let result = LrcFile::parse(input).unwrap();
        // 10.00s - 0.5s offset = 9.5s
        assert_eq!(result.lines[0].start_time, Duration::from_millis(9500));
    }

    #[test]
    fn test_parse_cjk_lyrics() {
        let input = "[00:05.00]你好世界";
        let result = LrcFile::parse(input).unwrap();
        assert_eq!(result.lines[0].text, "你好世界");
    }

    #[test]
    fn test_parse_enhanced_lrc() {
        let input = "[00:12.34] <00:12.34> Hello <00:13.00> world";
        let result = LrcFile::parse(input).unwrap();
        assert!(result.lines[0].words.is_some());
        let words = result.lines[0].words.as_ref().unwrap();
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].text, "Hello");
        assert_eq!(words[1].text, "world");
    }

    #[test]
    fn test_parse_multi_timestamp_line() {
        let input = "[00:05.00][00:15.00]Repeated lyric";
        let result = LrcFile::parse(input).unwrap();
        assert_eq!(result.lines.len(), 2);
        assert_eq!(result.lines[0].text, "Repeated lyric");
        assert_eq!(result.lines[1].text, "Repeated lyric");
        assert_eq!(result.lines[0].start_time, Duration::from_millis(5000));
        assert_eq!(result.lines[1].start_time, Duration::from_millis(15000));
    }

    #[test]
    fn test_current_line() {
        let input = r#"
[00:05.00]First
[00:10.00]Second
[00:15.00]Third
"#;
        let lrc = LrcFile::parse(input).unwrap();

        assert!(lrc.current_line(Duration::from_secs(0)).is_none());
        assert_eq!(
            lrc.current_line(Duration::from_secs(7)).unwrap().text,
            "First"
        );
        assert_eq!(
            lrc.current_line(Duration::from_secs(12)).unwrap().text,
            "Second"
        );
        assert_eq!(
            lrc.current_line(Duration::from_secs(20)).unwrap().text,
            "Third"
        );
    }

    #[test]
    fn test_line_progress() {
        let line = LrcLine {
            start_time: Duration::from_secs(10),
            text: "Hello world".to_string(),
            words: None,
        };

        let next_start = Some(Duration::from_secs(15));

        assert_eq!(line.progress(Duration::from_secs(8), next_start), 0.0);
        assert_eq!(line.progress(Duration::from_secs(10), next_start), 0.0);
        assert!((line.progress(Duration::from_millis(12500), next_start) - 0.5).abs() < 0.01);
        assert_eq!(line.progress(Duration::from_secs(15), next_start), 1.0);
        assert_eq!(line.progress(Duration::from_secs(20), next_start), 1.0);
    }

    #[test]
    fn test_alternative_timestamp_format() {
        // Some LRC files use mm:ss:xx format (colon instead of dot for hundredths)
        let input = "[00:12:34]Hello world";
        let result = LrcFile::parse(input).unwrap();
        assert_eq!(result.lines.len(), 1);
        // 12 seconds + 340ms
        assert_eq!(result.lines[0].start_time, Duration::from_millis(12340));
    }

    #[test]
    fn test_empty_lines_ignored() {
        let input = r#"
[00:05.00]First

[00:10.00]Second

"#;
        let result = LrcFile::parse(input).unwrap();
        assert_eq!(result.lines.len(), 2);
    }

    #[test]
    fn test_visible_lines() {
        let input = r#"
[00:05.00]Line 1
[00:10.00]Line 2
[00:15.00]Line 3
[00:20.00]Line 4
[00:25.00]Line 5
"#;
        let lrc = LrcFile::parse(input).unwrap();

        let visible = lrc.visible_lines(Duration::from_secs(12), 1, 1);
        assert_eq!(visible.len(), 3);
        assert_eq!(visible[0].text, "Line 1");
        assert_eq!(visible[1].text, "Line 2");
        assert_eq!(visible[2].text, "Line 3");
    }
}
