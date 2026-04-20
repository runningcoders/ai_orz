//! Generic daily JSONL file writer for structured logging/tracing
//!
//! Reused by both memory trace storage and tool call logging.
//! Writes to date-partitioned files: {base_path}/{YYYYMMDD}.jsonl
//! Each append operation writes a single JSON line and returns its line number

use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Serialize;

/// A generic writer for daily JSONL files
///
/// Appends JSON-serializable entries to date-partitioned files,
/// one entry per line. Returns the line number after append.
#[derive(Debug, Clone)]
pub struct DailyJsonlWriter {
    base_path: PathBuf,
}

impl DailyJsonlWriter {
    /// Create a new DailyJsonlWriter with the given base directory
    pub fn new(base_path: impl AsRef<Path>) -> Self {
        Self {
            base_path: base_path.as_ref().to_path_buf(),
        }
    }

    /// Get the current date path in YYYYMMDD format
    fn current_date_path(&self) -> String {
        let now = chrono::Local::now();
        now.format("%Y%m%d").to_string()
    }

    /// Get the full file path for the given date string
    fn file_path_for_date(&self, date: &str) -> PathBuf {
        fs::create_dir_all(&self.base_path).ok();
        self.base_path.join(format!("{date}.jsonl"))
    }

    /// Count the number of lines in a file
    fn count_lines(&self, path: &Path) -> Result<usize> {
        if !path.exists() {
            return Ok(0);
        }
        let file = fs::File::open(path)?;
        let reader = io::BufReader::new(file);
        let lines = reader.lines().count();
        Ok(lines)
    }

    /// Append a JSON-serializable entry to the current day's file
    /// Returns (date_path, line_number) where line_number is the 0-based index
    /// of the newly appended line
    pub fn append<T: Serialize>(&self, entry: &T) -> Result<(String, usize)> {
        let date = self.current_date_path();
        let path = self.file_path_for_date(&date);

        // Count existing lines to get the next line number (0-based)
        let line_number = self.count_lines(&path)?;

        // Open file in append mode, create if it doesn't exist
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;

        // Serialize to JSON and write with newline
        let json = serde_json::to_string(entry)?;
        writeln!(file, "{json}")?;
        file.flush()?;

        Ok((date, line_number))
    }

    /// Read a specific line from the file by line number (0-based)
    pub fn read_line(&self, date: &str, line_number: usize) -> Result<String> {
        let path = self.file_path_for_date(date);
        if !path.exists() {
            return Err(anyhow::anyhow!("File not found for date {date}"));
        }

        let file = fs::File::open(path)?;
        let reader = io::BufReader::new(file);

        for (idx, line) in reader.lines().enumerate() {
            if idx == line_number {
                return line.map_err(Into::into);
            }
        }

        Err(anyhow::anyhow!("Line {line_number} not found in {date:?}"))
    }

    /// Read a specific line and deserialize it
    pub fn read_line_json<T: serde::de::DeserializeOwned>(
        &self,
        date: &str,
        line_number: usize,
    ) -> Result<T> {
        let line = self.read_line(date, line_number)?;
        let parsed = serde_json::from_str(&line)?;
        Ok(parsed)
    }

    /// Get the base path of this writer
    pub fn base_path(&self) -> &PathBuf {
        &self.base_path
    }
}