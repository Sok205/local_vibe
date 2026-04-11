pub mod epub;
pub mod html;
pub mod pdf;
pub mod text;

use std::path::Path;

use lv_core::traits::Parser;
use lv_core::types::ParsedDocument;
use lv_core::{Result, VibeError};

/// Route a file to the appropriate parser based on extension.
pub fn parse_document(path: &Path, parsers: &[Box<dyn Parser>]) -> Result<ParsedDocument> {
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
        .unwrap_or_default();

    for parser in parsers {
        if parser.supported_extensions().contains(&ext.as_str()) {
            return parser.parse(path);
        }
    }

    Err(VibeError::UnsupportedFormat(ext))
}
