use std::path::Path;

use epub::doc::EpubDoc;
use scraper::Html;

use lv_core::traits::Parser;
use lv_core::types::ParsedDocument;
use lv_core::{Result, VibeError};

/// Parser for EPUB files.
#[derive(Default)]
pub struct EpubParser;

impl EpubParser {
    pub fn new() -> Self {
        Self
    }
}

impl Parser for EpubParser {
    fn supported_extensions(&self) -> &[&str] {
        &[".epub"]
    }

    fn parse(&self, path: &Path) -> Result<ParsedDocument> {
        if !path.exists() {
            return Err(VibeError::NotFound(path.to_path_buf()));
        }

        let mut doc = EpubDoc::new(path).map_err(|e| VibeError::Parse {
            path: path.to_path_buf(),
            reason: format!("Failed to open EPUB: {e}"),
        })?;

        let mut all_text = Vec::new();

        while doc.go_next() {
            if let Some((content, _mime)) = doc.get_current_str() {
                let fragment = Html::parse_fragment(&content);
                let mut chapter_text = String::new();
                for node_ref in fragment.root_element().descendants() {
                    if let scraper::Node::Text(text_node) = node_ref.value() {
                        if !chapter_text.is_empty() {
                            chapter_text.push(' ');
                        }
                        chapter_text.push_str(text_node);
                    }
                }
                let trimmed = chapter_text.trim().to_string();
                if !trimmed.is_empty() {
                    all_text.push(trimmed);
                }
            }
        }

        let text = all_text.join("\n\n").trim().to_string();

        if text.is_empty() {
            return Err(VibeError::Parse {
                path: path.to_path_buf(),
                reason: "EPUB produced empty text".to_string(),
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
            extension: ".epub".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_epub_not_found() {
        let parser = EpubParser::new();
        let result = parser.parse(Path::new("/nonexistent/file.epub"));
        assert!(matches!(result, Err(VibeError::NotFound(_))));
    }

    #[test]
    fn test_epub_supported_extensions() {
        let parser = EpubParser::new();
        assert_eq!(parser.supported_extensions(), &[".epub"]);
    }
}
