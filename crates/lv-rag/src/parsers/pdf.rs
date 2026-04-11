use std::path::Path;

use lv_core::traits::Parser;
use lv_core::types::ParsedDocument;
use lv_core::{Result, VibeError};

/// Parser for PDF files using pdf-extract.
#[derive(Default)]
pub struct PdfParser;

impl PdfParser {
    pub fn new() -> Self {
        Self
    }
}

impl Parser for PdfParser {
    fn supported_extensions(&self) -> &[&str] {
        &[".pdf"]
    }

    fn parse(&self, path: &Path) -> Result<ParsedDocument> {
        if !path.exists() {
            return Err(VibeError::NotFound(path.to_path_buf()));
        }

        let bytes = std::fs::read(path)?;
        let text = pdf_extract::extract_text_from_mem(&bytes).map_err(|e| VibeError::Parse {
            path: path.to_path_buf(),
            reason: format!("PDF extraction failed: {e}"),
        })?;

        let text = text.trim().to_string();
        if text.is_empty() {
            return Err(VibeError::Parse {
                path: path.to_path_buf(),
                reason: "PDF produced empty text".to_string(),
            });
        }

        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();

        Ok(ParsedDocument {
            text,
            file_path: path.to_path_buf(),
            file_name,
            extension: ".pdf".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pdf_not_found() {
        let parser = PdfParser::new();
        let result = parser.parse(Path::new("/nonexistent/file.pdf"));
        assert!(matches!(result, Err(VibeError::NotFound(_))));
    }

    #[test]
    fn test_pdf_supported_extensions() {
        let parser = PdfParser::new();
        assert_eq!(parser.supported_extensions(), &[".pdf"]);
    }
}
