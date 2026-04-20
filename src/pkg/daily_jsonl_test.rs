//! Daily JSONL 单元测试

use crate::pkg::daily_jsonl::DailyJsonlWriter;
use serde::{Deserialize, Serialize};
use tempfile::tempdir;

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct TestLogEntry {
    id: usize,
    message: String,
    value: f64,
}

#[test]
fn test_daily_jsonl_append_and_read() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let base_path = temp_dir.path().to_path_buf();
    
    let writer = DailyJsonlWriter::new(base_path.clone());
    
    // Append multiple entries
    let entry1 = TestLogEntry {
        id: 1,
        message: "first message".to_string(),
        value: 1.5,
    };
    let (date1, line1) = writer.append(&entry1)?;
    
    let entry2 = TestLogEntry {
        id: 2,
        message: "second message".to_string(),
        value: 2.7,
    };
    let (date2, line2) = writer.append(&entry2)?;
    
    let entry3 = TestLogEntry {
        id: 3,
        message: "third message".to_string(),
        value: 3.14,
    };
    let (date3, line3) = writer.append(&entry3)?;
    
    // Check line numbers are 0-indexed, dates are same (assuming test runs within same day)
    assert_eq!(line1, 0);
    assert_eq!(line2, 1);
    assert_eq!(line3, 2);
    assert_eq!(date1, date2);
    assert_eq!(date2, date3);
    
    // Read back each line
    let json1 = writer.read_line(&date1, 0)?;
    let json2 = writer.read_line(&date1, 1)?;
    let json3 = writer.read_line(&date1, 2)?;
    
    // Parse and verify
    let parsed1: TestLogEntry = serde_json::from_str(&json1)?;
    let parsed2: TestLogEntry = serde_json::from_str(&json2)?;
    let parsed3: TestLogEntry = serde_json::from_str(&json3)?;
    
    assert_eq!(parsed1, entry1);
    assert_eq!(parsed2, entry2);
    assert_eq!(parsed3, entry3);
    
    Ok(())
}

#[test]
fn test_daily_jsonl_read_line_json() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let base_path = temp_dir.path().to_path_buf();
    
    let writer = DailyJsonlWriter::new(base_path.clone());
    
    let entry = TestLogEntry {
        id: 42,
        message: "test deserialization".to_string(),
        value: 99.9,
    };
    let (date, line) = writer.append(&entry)?;
    
    let parsed: TestLogEntry = writer.read_line_json(&date, line)?;
    assert_eq!(parsed, entry);
    
    Ok(())
}

#[test]
fn test_daily_jsonl_line_out_of_range_returns_error() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let base_path = temp_dir.path().to_path_buf();
    
    let writer = DailyJsonlWriter::new(base_path.clone());
    let (date, _line) = writer.append(&"hello world")?;
    
    // Line 0 exists, line 1 does not
    assert!(writer.read_line(&date, 0).is_ok());
    assert!(writer.read_line(&date, 1).is_err());
    
    Ok(())
}

#[test]
fn test_daily_jsonl_nonexistent_date_returns_error() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let base_path = temp_dir.path().to_path_buf();
    
    let writer = DailyJsonlWriter::new(base_path.clone());
    
    // File doesn't exist for this date
    assert!(writer.read_line("19990101", 0).is_err());
    
    Ok(())
}