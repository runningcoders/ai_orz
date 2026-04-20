//! File type enumeration for artifacts and message attachments.
//!
//! Shared by both artifacts and messages, unified classification of file types.

use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlx")]
use sqlx::Type;

/// File type enumeration for artifacts and message attachments.
///
/// Classifies files into common categories for filtering and display.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum FileType {
    /// Text document (Markdown, PDF, TXT, etc.)
    Document = 0,
    /// Image file (PNG, JPG, GIF, etc.)
    Image = 1,
    /// Audio file (MP3, WAV, etc.)
    Audio = 2,
    /// Video file (MP4, WebM, etc.)
    Video = 3,
    /// Generic binary file (ZIP, EXE, etc.)
    Binary = 4,
}

impl From<i32> for FileType {
    fn from(v: i32) -> Self {
        match v {
            0 => FileType::Document,
            1 => FileType::Image,
            2 => FileType::Audio,
            3 => FileType::Video,
            4 => FileType::Binary,
            _ => FileType::Document,
        }
    }
}

impl From<i64> for FileType {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}

impl FileType {
    /// Convert the file type to i32 for database storage.
    pub fn to_i32(&self) -> i32 {
        *self as i32
    }
}
