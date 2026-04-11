use std::io::Read;
use std::path::Path;

use scraper::{Html, Selector};

use lv_core::traits::Parser;
use lv_core::types::ParsedDocument;
use lv_core::{Result, VibeError};

/// Parser for HTML files using the scraper crate.
#[derive(Default)]
pub struct HtmlParser;

impl HtmlParser {
    pub fn new() -> Self {
        Self
    }
}

impl Parser for HtmlParser {
    fn supported_extensions(&self) -> &[&str] {
        &[".html", ".htm", ".xhtml"]
    }

    fn parse(&self, path: &Path) -> Result<ParsedDocument> {
        if !path.exists() {
            return Err(VibeError::NotFound(path.to_path_buf()));
        }

        let file = std::fs::File::open(path)?;
        let mut reader = std::io::BufReader::new(file);
        let mut html_content = String::new();
        reader.read_to_string(&mut html_content)?;

        let document = Html::parse_document(&html_content);
        let script_selector = Selector::parse("script, style, noscript").unwrap();
        let body_selector = Selector::parse("body").unwrap();

        let root = document
            .select(&body_selector)
            .next()
            .map(|el| el.html())
            .unwrap_or_else(|| html_content.clone());

        let body_doc = Html::parse_fragment(&root);
        let mut text_parts: Vec<String> = Vec::new();

        let skip_node_ids: rustc_hash::FxHashSet<_> = body_doc
            .select(&script_selector)
            .flat_map(|el| el.descendants().map(|node_ref| node_ref.id()))
            .collect();

        for node_ref in body_doc.tree.nodes() {
            if skip_node_ids.contains(&node_ref.id()) {
                continue;
            }
            if let scraper::Node::Text(text_node) = node_ref.value() {
                let trimmed = text_node.trim();
                if !trimmed.is_empty() {
                    text_parts.push(trimmed.to_string());
                }
            }
        }

        let text = text_parts.join(" ").trim().to_string();

        if text.is_empty() {
            return Err(VibeError::Parse {
                path: path.to_path_buf(),
                reason: "HTML produced empty text".to_string(),
            });
        }

        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let extension = path
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy()))
            .unwrap_or_default();

        Ok(ParsedDocument {
            text,
            file_path: path.to_path_buf(),
            file_name,
            extension,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_parse_html() {
        let mut f = NamedTempFile::with_suffix(".html").unwrap();
        f.write_all(
            b"<html><body><h1>Title</h1><p>Hello world</p><script>var x=1;</script></body></html>",
        )
        .unwrap();
        let parser = HtmlParser::new();
        let doc = parser.parse(f.path()).unwrap();
        assert!(doc.text.contains("Title"));
        assert!(doc.text.contains("Hello world"));
        assert!(!doc.text.contains("var x=1"));
    }

    #[test]
    fn test_parse_html_empty() {
        let mut f = NamedTempFile::with_suffix(".html").unwrap();
        f.write_all(b"<html><body><script>only script</script></body></html>")
            .unwrap();
        let parser = HtmlParser::new();
        let _ = parser.parse(f.path());
    }

    #[test]
    fn test_html_not_found() {
        let parser = HtmlParser::new();
        let result = parser.parse(Path::new("/nonexistent/file.html"));
        assert!(matches!(result, Err(VibeError::NotFound(_))));
    }
}
