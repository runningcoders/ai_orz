//! Shared file metadata structure for artifacts and message attachments.
//! Used by both artifacts and messages to store file information.

use serde::{Deserialize, Serialize};

/// Shared file metadata structure.
/// Stored as JSON in database `file_meta` column.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMeta {
    /// Relative file path within attachments directory.
    pub file_path: String,
    /// MIME type of the file (e.g., "application/pdf", "image/png").
    pub mime_type: String,
    /// File size in bytes.
    pub file_size: u64,
}

impl Default for FileMeta {
    fn default() -> Self {
        Self {
            file_path: String::new(),
            mime_type: String::new(),
            file_size: 0,
        }
    }
}

impl FileMeta {
    /// Create a new FileMeta instance.
    pub fn new(file_path: String, mime_type: String, file_size: u64) -> Self {
        Self {
            file_path,
            mime_type,
            file_size,
        }
    }
}
