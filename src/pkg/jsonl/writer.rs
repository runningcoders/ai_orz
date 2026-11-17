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
use std::sync::Mutex;

use anyhow::Result;
use serde::Serialize;

/// 进程内串行化 JSONL 追加，兜底保证「一条记录 = 一次 `write`」不被打断。
///
/// 并发追加时若一条记录被拆成多次 `write`，两条记录会交错粘成一行，读回时
/// `serde_json` 报 `trailing characters`。机理与主防线（单次 `write`）见
/// [`DailyJsonlWriter::write_lines`]。
static APPEND_LOCK: Mutex<()> = Mutex::new(());

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

    /// 在 JSONL 追加锁的临界区内执行闭包。
    ///
    /// 供旁车索引的「读时补齐」使用：补齐要重扫原文尾部并追加索引，必须与
    /// 写入侧的「原文 append + 索引 append」互斥，否则会读到半行或与写入交错。
    /// 注意：闭包内**不得**再调用本 writer 的 append 方法（锁不可重入）。
    pub fn with_append_lock<R>(f: impl FnOnce() -> R) -> R {
        let _guard = APPEND_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        f()
    }

    /// 追加一条，并在**同一把 `APPEND_LOCK` 临界区内**回调 `on_appended`。
    ///
    /// 供「写时维护旁车索引」使用：回调收到 (date, offset, len)，offset 是该行
    /// 在当日文件中的起始字节位置（持锁读取，与实际写入严格对应）。原文 append
    /// 与回调（写索引）被同一把锁包住，保证并发写入时索引行顺序与原文行顺序一致——
    /// 若分两次加锁，两次写入的索引行会交错乱序，watermark 回退并重复补齐。
    ///
    /// 回调内部**不得**再获取 `APPEND_LOCK`（不可重入）；写索引_directly_打开文件即可。
    pub fn append_with_index<T: Serialize, F>(&self, entry: &T, on_appended: F) -> Result<String>
    where
        F: FnOnce(&str, u64, u64) -> Result<()>,
    {
        let _guard = APPEND_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let date = self.current_date_path();
        let path = self.file_path_for_date(&date);
        let offset = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let json = serde_json::to_string(entry)?;
        let line_len = (json.len() + 1) as u64; // +1 for '\n'
        self.write_lines_inner(&path, std::slice::from_ref(entry))?;
        on_appended(&date, offset, line_len)?;
        Ok(date)
    }

    /// 追加一批，并在**同一把 `APPEND_LOCK` 临界区内**完成「数行 + 写原文 + 索引回调」。
    ///
    /// 与 [`Self::append_with_index`] 的差别：本方法额外产出 `line_number`（0-based 物理
    /// 行号），供需要「日期 + 行号」引用链的 trace（memory）使用。数行、取字节偏移、
    /// 逐行写盘、索引回调全在同一锁内，保证三者严格对应；分两次加锁会让并发写入时
    /// 行号/偏移与索引行错位。
    ///
    /// 返回 `(date, first_line_number)`：每条的坐标由回调按顺序给出
    /// `(date, line_number, offset, len)`。单条场景传 `std::slice::from_ref(entry)` 即可。
    /// 回调内部**不得**再获取 `APPEND_LOCK`（不可重入）。
    pub fn append_with_index_position<T: Serialize, F>(
        &self,
        entries: &[T],
        mut on_appended: F,
    ) -> Result<(String, usize)>
    where
        F: FnMut(&str, usize, u64, u64) -> Result<()>,
    {
        let _guard = APPEND_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let date = self.current_date_path();
        let path = self.file_path_for_date(&date);
        let first_line_number = self.count_lines(&path)?;
        let mut offset = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

        // 预序列化，逐条拼行（+'\n'），后续按序写出并用同一份长度算坐标
        let mut lines: Vec<String> = Vec::with_capacity(entries.len());
        for entry in entries {
            let mut line = serde_json::to_string(entry)?;
            line.push('\n');
            lines.push(line);
        }

        let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
        for (i, line) in lines.iter().enumerate() {
            let len = line.len() as u64;
            file.write_all(line.as_bytes())?;
            on_appended(&date, first_line_number + i, offset, len)?;
            offset += len;
        }
        file.flush()?;
        Ok((date, first_line_number))
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
    ///
    /// 【不变量】「一条记录 = 一次 `write`」。`writeln!(file, "{json}")` 会把 JSON
    /// 载荷与结尾换行拆成**两次** `write` 系统调用；多个写入方并发追加同一文件时，
    /// 两次调用之间会被别的记录插入，两条记录被粘成一行 —— 读回时 `serde_json` 报
    /// `trailing characters at line 1 column N`，表现为 trace / 记忆查询整段失败
    /// （测试并行、多线程消费者都会触发）。因此这里先把整行拼进缓冲区再单次写出：
    /// `O_APPEND` 下单次 `write` 是原子的，正常不会被交错。
    ///
    /// `APPEND_LOCK` 是兜底：万一内核返回短写（`write_all` 需要拆成多次系统调用），
    /// 进程内串行化仍能保证行完整；跨进程则依赖上面的单次 `write` 原子性。
    fn write_lines<T: Serialize>(&self, path: &Path, entries: &[T]) -> Result<()> {
        let _guard = APPEND_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        self.write_lines_inner(path, entries)
    }

    /// 不加锁的写盘内核。仅供已持有 `APPEND_LOCK` 的调用方使用（如
    /// [`Self::append_with_index`]），避免 Mutex 重入死锁。
    fn write_lines_inner<T: Serialize>(&self, path: &Path, entries: &[T]) -> Result<()> {
        // Open file in append mode, create if it doesn't exist
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;

        let mut line = String::new();
        for entry in entries {
            line.clear();
            line.push_str(&serde_json::to_string(entry)?);
            line.push('\n');
            file.write_all(line.as_bytes())?;
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
        // 数行 + 写盘绑进同一把锁：否则两个并发 append 会数到同一行号（行号重复），
        // 或批次数行后、写入前有别的写入插入导致整批行号错位。
        let _guard = APPEND_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let date = self.current_date_path();
        let path = self.file_path_for_date(&date);

        let line_number = self.count_lines(&path)?;
        self.write_lines_inner(&path, std::slice::from_ref(entry))?;

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
        // 同 append：数行与写入必须在同一把锁内，保证整批行号与写入顺序一致。
        let _guard = APPEND_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let date = self.current_date_path();
        let path = self.file_path_for_date(&date);
        let first_line_number = self.count_lines(&path)?;
        self.write_lines_inner(&path, entries)?;
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
