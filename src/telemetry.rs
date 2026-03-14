//! Generation telemetry — append-only CSV log of pipeline runs.
//!
//! Each `skilldo generate` invocation appends one row to `~/.skilldo/runs.csv`.
//! Fields capture language, models, retries, pass/fail, and failure details.

use std::fs;
use std::io::Write;
use std::path::PathBuf;

/// A single pipeline run record.
pub struct RunRecord {
    pub language: String,
    pub library: String,
    pub library_version: String,
    pub provider: String,
    pub model: String,
    pub test_provider: Option<String>,
    pub test_model: Option<String>,
    pub review_provider: Option<String>,
    pub review_model: Option<String>,
    pub max_retries: usize,
    pub retries_used: usize,
    pub review_retries_used: usize,
    pub passed: bool,
    pub failed_stage: Option<String>,
    pub failure_reason: Option<String>,
    pub duration_secs: f64,
    pub timestamp: String,
    pub skilldo_version: String,
    /// True when review ran without container introspection (textual analysis only).
    pub review_degraded: bool,
}

impl RunRecord {
    /// Format as a CSV row (no trailing newline).
    pub fn to_csv_row(&self) -> String {
        let fields = [
            csv_escape(&self.language),
            csv_escape(&self.library),
            csv_escape(&self.library_version),
            csv_escape(&self.provider),
            csv_escape(&self.model),
            csv_escape(self.test_provider.as_deref().unwrap_or("")),
            csv_escape(self.test_model.as_deref().unwrap_or("")),
            csv_escape(self.review_provider.as_deref().unwrap_or("")),
            csv_escape(self.review_model.as_deref().unwrap_or("")),
            self.max_retries.to_string(),
            self.retries_used.to_string(),
            self.review_retries_used.to_string(),
            self.passed.to_string(),
            csv_escape(self.failed_stage.as_deref().unwrap_or("")),
            csv_escape(self.failure_reason.as_deref().unwrap_or("")),
            format!("{:.1}", self.duration_secs),
            csv_escape(&self.timestamp),
            csv_escape(&self.skilldo_version),
            self.review_degraded.to_string(),
        ];
        fields.join(",")
    }

    /// CSV header line (no trailing newline).
    pub fn csv_header() -> &'static str {
        "language,library,library_version,provider,model,test_provider,test_model,review_provider,review_model,max_retries,retries_used,review_retries_used,passed,failed_stage,failure_reason,duration_secs,timestamp,skilldo_version,review_degraded"
    }
}

/// Format current time as ISO 8601 UTC (no external crate needed).
pub fn iso8601_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    epoch_to_iso8601(secs)
}

/// Convert UNIX epoch seconds to ISO 8601 UTC string.
fn epoch_to_iso8601(secs: u64) -> String {
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Days since 1970-01-01 → year/month/day
    let mut remaining_days = days as i64;
    let mut year = 1970i32;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        year += 1;
    }
    let month_days = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 1u32;
    for &d in &month_days {
        if remaining_days < d {
            break;
        }
        remaining_days -= d;
        month += 1;
    }
    let day = remaining_days + 1;
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// Escape a field for CSV: quote if it contains comma, quote, or newline.
fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Append a run record to the CSV file. Creates `~/.skilldo/` and header if needed.
/// `path` overrides the default location (for testing).
pub fn append_run(record: &RunRecord, path: Option<PathBuf>) -> std::io::Result<()> {
    let csv_path = match path {
        Some(p) => p,
        None => {
            let home = dirs::home_dir().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, "HOME directory not found")
            })?;
            let dir = home.join(".skilldo");
            fs::create_dir_all(&dir)?;
            dir.join("runs.csv")
        }
    };

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&csv_path)?;

    let file_len = file.metadata()?.len();
    if file_len == 0 {
        writeln!(file, "{}", RunRecord::csv_header())?;
    }
    writeln!(file, "{}", record.to_csv_row())?;

    // Migrate stale header: if the first line doesn't match the current header,
    // prepend the correct header. Old rows get extra trailing empty fields on
    // parse, which is harmless for append-only telemetry.
    if file_len > 0 {
        drop(file);
        migrate_header_if_stale(&csv_path)?;
    }

    Ok(())
}

/// Write `data` to `path` atomically: write to a sibling temp file, then rename.
/// Prevents data loss if the process is killed mid-write.
fn write_atomic(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, data)?;
    fs::rename(&tmp, path)
}

/// If the CSV header doesn't match the current schema, replace the first line.
fn migrate_header_if_stale(path: &std::path::Path) -> std::io::Result<()> {
    let content = fs::read_to_string(path)?;
    let expected = RunRecord::csv_header();
    if let Some(first_line) = content.lines().next() {
        if first_line != expected {
            let rest: String = content.lines().skip(1).collect::<Vec<_>>().join("\n");
            let new_content = if rest.is_empty() {
                format!("{expected}\n")
            } else {
                format!("{expected}\n{rest}\n")
            };
            write_atomic(path, new_content.as_bytes())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_record() -> RunRecord {
        RunRecord {
            language: "python".to_string(),
            library: "fastapi".to_string(),
            library_version: "0.115.0".to_string(),
            provider: "anthropic".to_string(),
            model: "claude-sonnet-4-5-20250929".to_string(),
            test_provider: Some("openai".to_string()),
            test_model: Some("gpt-5.2".to_string()),
            review_provider: None,
            review_model: None,
            max_retries: 5,
            retries_used: 2,
            review_retries_used: 0,
            passed: true,
            failed_stage: None,
            failure_reason: None,
            duration_secs: 198.3,
            timestamp: "2026-03-02T20:30:00-08:00".to_string(),
            skilldo_version: "0.1.9".to_string(),
            review_degraded: false,
        }
    }

    #[test]
    fn test_csv_row_roundtrip() {
        let record = sample_record();
        let row = record.to_csv_row();
        // Assert on known prefixes/suffixes to avoid fragile split on commas
        assert!(row.starts_with("python,fastapi,0.115.0,anthropic,"));
        assert!(row.contains(",true,")); // passed field
        assert!(row.ends_with(",0.1.9,false"));
    }

    #[test]
    fn test_csv_escape_comma() {
        let record = RunRecord {
            failure_reason: Some("3/5 tests failed, retries exhausted".to_string()),
            ..sample_record()
        };
        let row = record.to_csv_row();
        assert!(row.contains("\"3/5 tests failed, retries exhausted\""));
    }

    #[test]
    fn test_append_run_creates_file_with_header() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("runs.csv");

        let record = sample_record();
        append_run(&record, Some(csv_path.clone())).unwrap();

        let content = fs::read_to_string(&csv_path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2); // header + 1 row
        assert!(lines[0].starts_with("language,library,"));
        assert!(lines[1].starts_with("python,fastapi,"));
    }

    #[test]
    fn test_append_run_no_duplicate_header() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("runs.csv");

        let record = sample_record();
        append_run(&record, Some(csv_path.clone())).unwrap();
        append_run(&record, Some(csv_path.clone())).unwrap();

        let content = fs::read_to_string(&csv_path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 3); // header + 2 rows
                                    // Only first line is header
        assert!(lines[0].starts_with("language,"));
        assert!(lines[1].starts_with("python,"));
        assert!(lines[2].starts_with("python,"));
    }

    #[test]
    fn test_csv_column_count_matches_header() {
        let record = sample_record();
        let header_count = RunRecord::csv_header().split(',').count();
        let row_count = record.to_csv_row().split(',').count();
        assert_eq!(
            header_count, row_count,
            "CSV header has {header_count} columns but row has {row_count}"
        );
    }

    #[test]
    fn test_epoch_to_iso8601_unix_epoch() {
        assert_eq!(epoch_to_iso8601(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn test_epoch_to_iso8601_known_date() {
        // 2026-03-02T00:00:00Z = 1772409600
        assert_eq!(epoch_to_iso8601(1772409600), "2026-03-02T00:00:00Z");
    }

    #[test]
    fn test_iso8601_now_format() {
        let ts = iso8601_now();
        // Matches YYYY-MM-DDTHH:MM:SSZ
        assert_eq!(ts.len(), 20);
        assert!(ts.ends_with('Z'));
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[7..8], "-");
        assert_eq!(&ts[10..11], "T");
        assert_eq!(&ts[13..14], ":");
        assert_eq!(&ts[16..17], ":");
    }

    #[test]
    fn test_epoch_to_iso8601_leap_year() {
        // 2024-02-29T12:00:00Z — 2024 is a leap year
        // 2024-01-01 = epoch 1704067200
        // Jan: 31 days, so Feb 1 = 1704067200 + 31*86400 = 1706745600
        // Feb 29 = 1706745600 + 28*86400 = 1709164800
        // + 12h = 1709164800 + 43200 = 1709208000
        assert_eq!(epoch_to_iso8601(1709208000), "2024-02-29T12:00:00Z");
    }

    #[test]
    fn test_csv_escape_with_quotes() {
        let escaped = csv_escape("value with \"quotes\"");
        assert_eq!(escaped, "\"value with \"\"quotes\"\"\"");
    }

    #[test]
    fn test_csv_escape_with_newline() {
        let escaped = csv_escape("line1\nline2");
        assert_eq!(escaped, "\"line1\nline2\"");
    }

    #[test]
    fn test_csv_escape_plain() {
        let escaped = csv_escape("simple");
        assert_eq!(escaped, "simple");
    }

    #[test]
    fn test_review_degraded_appears_in_csv_row() {
        let record = RunRecord {
            review_degraded: true,
            ..sample_record()
        };
        let row = record.to_csv_row();
        assert!(
            row.ends_with(",true"),
            "review_degraded=true should be last CSV field"
        );
    }

    #[test]
    fn test_review_degraded_column_in_header() {
        let header = RunRecord::csv_header();
        assert!(
            header.ends_with(",review_degraded"),
            "review_degraded should be last header column"
        );
    }

    #[test]
    fn test_default_path_creates_dir_and_file() {
        // Use a tempdir to avoid polluting the real ~/.skilldo/runs.csv
        let dir = tempfile::tempdir().unwrap();
        let skilldo_dir = dir.path().join(".skilldo");
        let csv_path = skilldo_dir.join("runs.csv");

        // Pre-create the parent directory (the None branch does create_dir_all,
        // but with Some we must create it ourselves to mirror that behavior)
        fs::create_dir_all(&skilldo_dir).unwrap();

        let record = sample_record();
        let result = append_run(&record, Some(csv_path.clone()));
        assert!(
            result.is_ok(),
            "append_run should succeed: {:?}",
            result.err()
        );
        assert!(csv_path.exists(), "runs.csv should exist");

        let content = fs::read_to_string(&csv_path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2); // header + 1 row
        assert!(lines[0].starts_with("language,"));
    }

    #[test]
    fn test_append_run_migrates_stale_header() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("runs.csv");

        // Write an old header (missing review_degraded column)
        let old_header = "language,library,library_version,provider,model,test_provider,test_model,review_provider,review_model,max_retries,retries_used,review_retries_used,passed,failed_stage,failure_reason,duration_secs,timestamp,skilldo_version";
        let old_row = "python,fastapi,0.115.0,anthropic,claude,,,,,3,0,0,true,,,1.0,2024-01-01T00:00:00Z,0.1.8";
        fs::write(&csv_path, format!("{old_header}\n{old_row}\n")).unwrap();

        // Append a new record — should migrate the header
        let record = sample_record();
        append_run(&record, Some(csv_path.clone())).unwrap();

        let content = fs::read_to_string(&csv_path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 3, "header + old row + new row");
        assert!(
            lines[0].ends_with(",review_degraded"),
            "header should be migrated to include review_degraded"
        );
        // Old data row is preserved
        assert!(lines[1].starts_with("python,fastapi,"));
    }

    #[test]
    fn test_write_atomic_replaces_file_contents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("atomic.txt");
        fs::write(&path, "original").unwrap();
        write_atomic(&path, b"replaced").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "replaced");
        // Temp file should not linger
        assert!(!dir.path().join("atomic.tmp").exists());
    }

    #[test]
    fn test_migrate_header_noop_when_current() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("runs.csv");

        let record = sample_record();
        append_run(&record, Some(csv_path.clone())).unwrap();

        let before = fs::read_to_string(&csv_path).unwrap();

        // Append again — header should not change
        append_run(&record, Some(csv_path.clone())).unwrap();

        let after = fs::read_to_string(&csv_path).unwrap();
        let lines: Vec<&str> = after.lines().collect();
        assert_eq!(lines.len(), 3); // header + 2 rows
        assert_eq!(lines[0], before.lines().next().unwrap());
    }
}
