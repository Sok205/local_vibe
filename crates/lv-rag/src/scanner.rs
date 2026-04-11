use std::path::{Path, PathBuf};

use walkdir::WalkDir;

/// A file discovered during scanning.
#[derive(Debug, Clone)]
pub struct ScannedFile {
    pub path: PathBuf,
    pub name: String,
    pub extension: String,
    pub size: u64,
}

/// Scan a directory for files with the given extensions.
pub fn scan_directory(
    root: &Path,
    supported_extensions: &[&str],
) -> lv_core::Result<Vec<ScannedFile>> {
    if !root.exists() {
        return Err(lv_core::VibeError::NotFound(root.to_path_buf()));
    }

    let files: Vec<ScannedFile> = WalkDir::new(root)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter_map(|entry| {
            let path = entry.path();
            let ext = path
                .extension()
                .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))?;

            if !supported_extensions.contains(&ext.as_str()) {
                return None;
            }

            let metadata = entry.metadata().ok()?;

            Some(ScannedFile {
                path: path.to_path_buf(),
                name: path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                extension: ext,
                size: metadata.len(),
            })
        })
        .collect();

    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_scan_finds_supported_files() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("doc.txt"), "hello").unwrap();
        std::fs::write(dir.path().join("data.pdf"), "fake pdf").unwrap();
        std::fs::write(dir.path().join("image.png"), "fake png").unwrap();

        let files = scan_directory(dir.path(), &[".txt", ".pdf"]).unwrap();
        assert_eq!(files.len(), 2);
        let names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"doc.txt"));
        assert!(names.contains(&"data.pdf"));
    }

    #[test]
    fn test_scan_recursive() {
        let dir = TempDir::new().unwrap();
        let sub = dir.path().join("subdir");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("nested.txt"), "nested").unwrap();

        let files = scan_directory(dir.path(), &[".txt"]).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "nested.txt");
    }

    #[test]
    fn test_scan_empty_dir() {
        let dir = TempDir::new().unwrap();
        let files = scan_directory(dir.path(), &[".txt"]).unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn test_scan_nonexistent_dir() {
        let result = scan_directory(Path::new("/nonexistent/dir"), &[".txt"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_scan_file_metadata() {
        let dir = TempDir::new().unwrap();
        let content = "hello world";
        std::fs::write(dir.path().join("test.txt"), content).unwrap();

        let files = scan_directory(dir.path(), &[".txt"]).unwrap();
        assert_eq!(files[0].size, content.len() as u64);
        assert_eq!(files[0].extension, ".txt");
    }
}
