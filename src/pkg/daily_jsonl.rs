//! Generic daily JSONL file writer for structured logging/tracing
//!
//! Reused by both memory trace storage and tool call logging.
//! Writes to date-partitioned files: {base_path}/{YYYYMMDD}.jsonl
//!
//! 两种追加语义（按需选择）：
//! - `append`：写完返回 (日期, 行号)，代价是先数一遍当日行数 —— 只在需要行号定位时用。
//! - `append_no_position`：只返回日期，O(1) 追加 —— 按业务键检索的流水/审计场景用。

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

    /// Serialize entries and append them, one per line, in a single open/flush
    fn write_lines<T: Serialize>(&self, path: &Path, entries: &[T]) -> Result<()> {
        // Open file in append mode, create if it doesn't exist
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;

        for entry in entries {
            let json = serde_json::to_string(entry)?;
            writeln!(file, "{json}")?;
        }
        file.flush()?;
        Ok(())
    }

    /// Append a JSON-serializable entry to the current day's file
    /// Returns (date_path, line_number) where line_number is the 0-based index
    /// of the newly appended line
    ///
    /// 行号的代价是「先把当日文件整篇读一遍数行」(O(行数))。只有调用方确实需要
    /// 「日期 + 行号」这个定位坐标时才用本方法（例如记忆 trace 要把位置回写索引）；
    /// 只落盘、后续按业务键检索的场景请用 [`Self::append_no_position`]。
    pub fn append<T: Serialize>(&self, entry: &T) -> Result<(String, usize)> {
        let date = self.current_date_path();
        let path = self.file_path_for_date(&date);

        // Count existing lines to get the next line number (0-based)
        let line_number = self.count_lines(&path)?;
        self.write_lines(&path, std::slice::from_ref(entry))?;

        Ok((date, line_number))
    }

    /// Append an entry **without** computing its line number.
    ///
    /// 只返回日期。不读既有文件、不数行，写入成本恒为 O(1) 且与当日累计行数无关
    /// —— 拉平后的 call trace、审计流水这类「只追加、按业务键（`call_id` 等）检索」
    /// 的场景应当走这里；`append` 的行号对它们毫无用处，却会让每条记录都重扫一遍
    /// 当日文件。
    ///
    /// 注意：本方法不产出行号，因此不要在需要 `read_line` 定位的写入路径上使用。
    pub fn append_no_position<T: Serialize>(&self, entry: &T) -> Result<String> {
        let date = self.current_date_path();
        let path = self.file_path_for_date(&date);
        self.write_lines(&path, std::slice::from_ref(entry))?;
        Ok(date)
    }

    /// Append a batch of entries with a **single** line count.
    ///
    /// 返回 (date, first_line_number)：第 i 条的位置是 `first_line_number + i`。
    /// 相比循环调用 `append`（每条都重扫一遍当日文件 → O(批大小 × 当日行数)），
    /// 这里只数一次行、只开一次文件 → O(批大小 + 当日行数)。
    ///
    /// 约定：整批共用**同一个日期**（取批次开始时的当日），即使写入跨越午夜也不拆文件
    /// —— 位置坐标必须与 `date_filename` 自洽，拆开反而会让行号指向错误的文件。
    pub fn append_batch<T: Serialize>(&self, entries: &[T]) -> Result<(String, usize)> {
        let date = self.current_date_path();
        let path = self.file_path_for_date(&date);
        let first_line_number = self.count_lines(&path)?;
        self.write_lines(&path, entries)?;
        Ok((date, first_line_number))
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
