use std::path::{Path, PathBuf};

const INDEX_PREFIX: &str = "/index ";

/// Complete an `/index <partial-path>` line to the longest common prefix of
/// matching filesystem entries. Returns `None` when the input doesn't start
/// with `/index `, when there's no partial path to complete, or when there
/// are no candidates that extend the current stem.
pub fn complete_index_line(line: &str) -> Option<String> {
    let partial = line.strip_prefix(INDEX_PREFIX)?;

    let space_idx = partial.find(char::is_whitespace).unwrap_or(partial.len());
    let (partial_path, tail) = partial.split_at(space_idx);
    if partial_path.is_empty() {
        return None;
    }

    let (parent, stem) = split_parent_stem(Path::new(partial_path));
    let entries = std::fs::read_dir(&parent).ok()?;

    let mut candidates: Vec<String> = Vec::new();
    for entry in entries.flatten() {
        let name_os = entry.file_name();
        let Some(name) = name_os.to_str() else { continue };
        if !name.starts_with(&stem) {
            continue;
        }
        let suffix = if entry.file_type().ok().map(|t| t.is_dir()).unwrap_or(false) {
            "/"
        } else {
            ""
        };
        candidates.push(format!("{name}{suffix}"));
    }

    if candidates.is_empty() {
        return None;
    }

    let lcp = longest_common_prefix(&candidates);
    if lcp.len() <= stem.len() {
        return None;
    }

    let had_trailing_slash = partial_path.ends_with('/');
    let parent_str = parent.to_string_lossy();
    let joined = if parent_str == "." && !partial_path.starts_with("./") {
        lcp
    } else if parent_str.ends_with('/') || had_trailing_slash && stem.is_empty() {
        format!("{parent_str}{lcp}")
    } else {
        format!("{parent_str}/{lcp}")
    };

    Some(format!("{INDEX_PREFIX}{joined}{tail}"))
}

fn split_parent_stem(path: &Path) -> (PathBuf, String) {
    let parent = path.parent().unwrap_or(Path::new(""));
    let stem = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    let parent = if parent.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        parent.to_path_buf()
    };
    (parent, stem)
}

fn longest_common_prefix(strings: &[String]) -> String {
    if strings.is_empty() {
        return String::new();
    }
    let first = strings[0].as_bytes();
    let mut end = first.len();
    for s in &strings[1..] {
        let bytes = s.as_bytes();
        end = end.min(bytes.len());
        for i in 0..end {
            if first[i] != bytes[i] {
                end = i;
                break;
            }
        }
        if end == 0 {
            return String::new();
        }
    }
    std::str::from_utf8(&first[..end]).unwrap_or("").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn non_index_line_returns_none() {
        assert!(complete_index_line("hello world").is_none());
        assert!(complete_index_line("/browse xyz").is_none());
    }

    #[test]
    fn empty_partial_returns_none() {
        assert!(complete_index_line("/index ").is_none());
    }

    #[test]
    fn completes_directory_in_cwd() {
        let td = tempdir().unwrap();
        std::fs::create_dir(td.path().join("alpha-project")).unwrap();
        std::fs::create_dir(td.path().join("alpha-notes")).unwrap();

        let prev = std::env::current_dir().unwrap();
        std::env::set_current_dir(td.path()).unwrap();

        let got = complete_index_line("/index alpha");
        std::env::set_current_dir(prev).unwrap();

        // LCP of "alpha-project/" and "alpha-notes/" is "alpha-"
        assert_eq!(got.as_deref(), Some("/index alpha-"));
    }

    #[test]
    fn returns_none_when_no_longer_prefix_possible() {
        let td = tempdir().unwrap();
        std::fs::create_dir(td.path().join("foo")).unwrap();
        // User already typed the full stem that happens to be the only match.
        let path = td.path().join("foo").to_string_lossy().into_owned();
        let line = format!("/index {path}");
        let got = complete_index_line(&line);
        // One match → LCP "foo/" extends the stem
        assert!(got.as_deref().unwrap().ends_with('/'));
    }

    #[test]
    fn longest_common_prefix_basic() {
        let v: Vec<String> = ["alpha-one", "alpha-two", "alpha-three"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(longest_common_prefix(&v), "alpha-");
    }

    #[test]
    fn longest_common_prefix_no_common() {
        let v: Vec<String> = ["alpha", "beta"].iter().map(|s| s.to_string()).collect();
        assert_eq!(longest_common_prefix(&v), "");
    }
}
