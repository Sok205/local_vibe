use std::io::Read;
use std::path::Path;

use lv_core::traits::Parser;
use lv_core::types::ParsedDocument;
use lv_core::{Result, VibeError};

/// Parser for plain text and markdown files.
#[derive(Default)]
pub struct TextParser;

impl TextParser {
    pub fn new() -> Self {
        Self
    }
}

impl Parser for TextParser {
    fn supported_extensions(&self) -> &[&str] {
        &[
            ".txt",
            ".md",
            ".markdown",
            ".text",
            ".log",
            ".csv",
            ".tsv",
            ".json",
            ".xml",
            ".yaml",
            ".yml",
            ".toml",
            ".ini",
            ".cfg",
            ".conf",
            ".rst",
            ".adoc",
            ".rs",
            ".ts",
            ".js",
            ".py",
            ".go",
            ".java",
            ".c",
            ".cpp",
            ".h",
            ".hpp",
            ".css",
            ".scss",
            ".sh",
            ".bash",
            ".zsh",
            ".sql",
            ".r",
            ".rb",
            ".php",
            ".swift",
            ".kt",
        ]
    }

    fn parse(&self, path: &Path) -> Result<ParsedDocument> {
        if !path.exists() {
            return Err(VibeError::NotFound(path.to_path_buf()));
        }

        let file = std::fs::File::open(path)?;
        let mut reader = std::io::BufReader::new(file);
        let mut text = String::new();
        reader.read_to_string(&mut text)?;

        let text = text.trim().to_string();
        if text.is_empty() {
            return Err(VibeError::Parse {
                path: path.to_path_buf(),
                reason: "file is empty after trimming".to_string(),
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
    fn test_parse_txt() {
        let mut f = NamedTempFile::with_suffix(".txt").unwrap();
        f.write_all(b"Hello, world!").unwrap();
        let parser = TextParser::new();
        let doc = parser.parse(f.path()).unwrap();
        assert_eq!(doc.text, "Hello, world!");
        assert_eq!(doc.extension, ".txt");
    }

    #[test]
    fn test_parse_md() {
        let mut f = NamedTempFile::with_suffix(".md").unwrap();
        f.write_all(b"# Title\n\nSome content").unwrap();
        let parser = TextParser::new();
        let doc = parser.parse(f.path()).unwrap();
        assert!(doc.text.contains("# Title"));
        assert_eq!(doc.extension, ".md");
    }

    #[test]
    fn test_parse_empty_file() {
        let mut f = NamedTempFile::with_suffix(".txt").unwrap();
        f.write_all(b"   \n\n  ").unwrap();
        let parser = TextParser::new();
        let result = parser.parse(f.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_not_found() {
        let parser = TextParser::new();
        let result = parser.parse(Path::new("/nonexistent/file.txt"));
        assert!(matches!(result, Err(VibeError::NotFound(_))));
    }

    #[test]
    fn test_supported_extensions() {
        let parser = TextParser::new();
        let exts = parser.supported_extensions();
        assert!(exts.contains(&".txt"));
        assert!(exts.contains(&".md"));
        assert!(exts.contains(&".json"));
    }
}
