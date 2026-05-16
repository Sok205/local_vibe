use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const SIDECAR_NAME: &str = ".lv-meta.toml";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LvMeta {
    pub indexed_at: String,
    #[serde(default)]
    pub lv_version: String,
}

pub fn sidecar_path(db_path: &Path) -> PathBuf {
    db_path.join(SIDECAR_NAME)
}

pub fn read(db_path: &Path) -> Option<LvMeta> {
    let content = std::fs::read_to_string(sidecar_path(db_path)).ok()?;
    toml::from_str(&content).ok()
}

pub fn write(db_path: &Path, meta: &LvMeta) -> std::io::Result<()> {
    std::fs::create_dir_all(db_path)?;
    let target = sidecar_path(db_path);
    let tmp = db_path.join(format!("{SIDECAR_NAME}.tmp"));
    let content = toml::to_string_pretty(meta).map_err(|e| std::io::Error::other(e.to_string()))?;
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, &target)?;
    Ok(())
}

pub fn write_indexed_now(db_path: &Path, lv_version: &str) -> std::io::Result<()> {
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    write(
        db_path,
        &LvMeta {
            indexed_at: now,
            lv_version: lv_version.to_string(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn round_trip_write_then_read() {
        let td = tempdir().unwrap();
        let meta = LvMeta {
            indexed_at: "2026-04-23T12:34:56Z".to_string(),
            lv_version: "0.1.0".to_string(),
        };
        write(td.path(), &meta).unwrap();

        let back = read(td.path()).expect("sidecar should be readable");
        assert_eq!(back.indexed_at, meta.indexed_at);
        assert_eq!(back.lv_version, meta.lv_version);
    }

    #[test]
    fn missing_sidecar_returns_none() {
        let td = tempdir().unwrap();
        assert!(read(td.path()).is_none());
    }

    #[test]
    fn malformed_sidecar_returns_none() {
        let td = tempdir().unwrap();
        std::fs::write(sidecar_path(td.path()), "this is not toml = = =").unwrap();
        assert!(read(td.path()).is_none());
    }

    #[test]
    fn write_indexed_now_produces_rfc3339() {
        let td = tempdir().unwrap();
        write_indexed_now(td.path(), "0.1.0").unwrap();
        let meta = read(td.path()).unwrap();
        assert!(meta.indexed_at.contains('T'));
        assert!(meta.indexed_at.ends_with('Z'));
    }
}
