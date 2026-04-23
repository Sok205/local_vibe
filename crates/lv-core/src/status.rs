use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::{CloudConfig, Config, ModelConfig};
use crate::traits::AppHost;
use crate::types::FileSummary;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusSnapshot {
    pub models: ModelsStatus,
    pub databases: Vec<DbStatus>,
    pub db_root: Option<PathBuf>,
    pub config_path: Option<PathBuf>,
    pub runtime: Option<RuntimeStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsStatus {
    pub fast: ModelSlot,
    pub medium: ModelSlot,
    pub strong: ModelSlot,
    pub embedding: Option<ModelSlot>,
    pub cloud: Option<CloudModelSlot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSlot {
    pub name: String,
    pub backend: String,
    pub weights_path: Option<PathBuf>,
    pub tokenizer_path: Option<PathBuf>,
    pub ready: Readiness,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudModelSlot {
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Readiness {
    Ready,
    MissingWeights,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbStatus {
    pub name: String,
    pub path: PathBuf,
    pub total_chunks: usize,
    pub unique_files: usize,
    pub languages: Vec<(String, usize)>,
    pub indexed_at: Option<String>,
    pub is_current: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuntimeStatus {
    pub warm_models: Vec<String>,
    pub warm_dbs: Vec<String>,
    pub pid: u32,
    pub session_id: Option<String>,
}

pub fn slot_from_config(m: &ModelConfig) -> ModelSlot {
    let ready = match m.backend.as_str() {
        "metal" => match (m.model_path.as_ref(), m.tokenizer_path.as_ref()) {
            (Some(mp), Some(tp)) if mp.exists() && tp.exists() => Readiness::Ready,
            _ => Readiness::MissingWeights,
        },
        _ => Readiness::Unknown,
    };
    ModelSlot {
        name: m.name.clone(),
        backend: m.backend.clone(),
        weights_path: m.model_path.clone(),
        tokenizer_path: m.tokenizer_path.clone(),
        ready,
    }
}

pub fn cloud_slot_from_config(c: &CloudConfig) -> CloudModelSlot {
    CloudModelSlot {
        provider: c.provider.clone(),
        model: c.model.clone(),
    }
}

pub fn build_models_status(cfg: &Config) -> ModelsStatus {
    ModelsStatus {
        fast: slot_from_config(&cfg.models.fast),
        medium: slot_from_config(&cfg.models.medium),
        strong: slot_from_config(&cfg.models.strong),
        embedding: cfg.models.embedding.as_ref().map(slot_from_config),
        cloud: cfg.models.cloud.as_ref().map(cloud_slot_from_config),
    }
}

pub fn language_histogram_by_files(files: &[FileSummary]) -> Vec<(String, usize)> {
    let mut map: HashMap<String, usize> = HashMap::new();
    for f in files {
        let key = f.language.clone().unwrap_or_else(|| "?".to_string());
        *map.entry(key).or_insert(0) += 1;
    }
    let mut v: Vec<(String, usize)> = map.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    v
}

fn resolve_db_path(cfg: &Config, name: &str) -> PathBuf {
    match cfg.rag.db_root.as_ref() {
        Some(root) => root.join(name),
        None => cfg.rag.db_dir.clone(),
    }
}

pub async fn collect_declared_status(
    host: &dyn AppHost,
    current_db: Option<&str>,
) -> anyhow::Result<StatusSnapshot> {
    let cfg = host.config();
    let models = build_models_status(cfg);

    let db_root = cfg.rag.db_root.clone();
    let db_names: Vec<String> = if db_root.is_some() {
        host.list_dbs().await.unwrap_or_default()
    } else if cfg.rag.db_dir.exists() {
        vec!["default".to_string()]
    } else {
        Vec::new()
    };

    let mut databases = Vec::with_capacity(db_names.len());
    for name in db_names {
        let path = resolve_db_path(cfg, &name);
        let is_current = current_db == Some(name.as_str());
        let indexed_at = crate::sidecar::read(&path).map(|m| m.indexed_at);

        match host.open_store_readonly(&name).await {
            Ok(store) => {
                let (chunks, files) = match store.stats().await {
                    Ok(s) => (s.total_chunks, s.unique_files),
                    Err(_) => (0, 0),
                };
                let langs = match store.list_files(usize::MAX).await {
                    Ok(fs) => language_histogram_by_files(&fs),
                    Err(_) => Vec::new(),
                };
                databases.push(DbStatus {
                    name,
                    path,
                    total_chunks: chunks,
                    unique_files: files,
                    languages: langs,
                    indexed_at,
                    is_current,
                    error: None,
                });
            }
            Err(e) => {
                databases.push(DbStatus {
                    name,
                    path,
                    total_chunks: 0,
                    unique_files: 0,
                    languages: Vec::new(),
                    indexed_at,
                    is_current,
                    error: Some(e.to_string()),
                });
            }
        }
    }

    let runtime = Some(host.runtime_status().await);

    Ok(StatusSnapshot {
        models,
        databases,
        db_root,
        config_path: Config::discover_path(),
        runtime,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ModelConfig, ModelsConfig, RagConfig};

    fn model_cfg(name: &str, backend: &str) -> ModelConfig {
        ModelConfig {
            name: name.to_string(),
            backend: backend.to_string(),
            quantization: "4bit".into(),
            model_path: None,
            tokenizer_path: None,
        }
    }

    #[test]
    fn readiness_unknown_for_mlx_lm() {
        let slot = slot_from_config(&model_cfg("gemma", "mlx-lm"));
        assert_eq!(slot.ready, Readiness::Unknown);
    }

    #[test]
    fn readiness_missing_for_metal_without_paths() {
        let slot = slot_from_config(&model_cfg("q2.5", "metal"));
        assert_eq!(slot.ready, Readiness::MissingWeights);
    }

    #[test]
    fn readiness_ready_for_metal_with_existing_paths() {
        let td = tempfile::tempdir().unwrap();
        let mp = td.path().join("model.gguf");
        let tp = td.path().join("tokenizer.json");
        std::fs::write(&mp, b"stub").unwrap();
        std::fs::write(&tp, b"stub").unwrap();

        let cfg = ModelConfig {
            name: "q2.5".into(),
            backend: "metal".into(),
            quantization: "4bit".into(),
            model_path: Some(mp),
            tokenizer_path: Some(tp),
        };
        let slot = slot_from_config(&cfg);
        assert_eq!(slot.ready, Readiness::Ready);
    }

    #[test]
    fn build_models_status_includes_optional_slots() {
        let models = ModelsConfig {
            embedding: Some(model_cfg("bge-small", "fastembed")),
            ..Default::default()
        };
        let cfg = Config {
            models,
            rag: RagConfig::default(),
            code_graph: Default::default(),
            tui: Default::default(),
        };
        let status = build_models_status(&cfg);
        assert!(status.embedding.is_some());
        assert!(status.cloud.is_none());
    }

    #[test]
    fn language_histogram_sorts_desc_by_count_then_name() {
        let files = vec![
            FileSummary { file_path: "a.rs".into(), language: Some("rust".into()), chunk_count: 1 },
            FileSummary { file_path: "b.rs".into(), language: Some("rust".into()), chunk_count: 2 },
            FileSummary { file_path: "c.md".into(), language: Some("markdown".into()), chunk_count: 5 },
            FileSummary { file_path: "d.ts".into(), language: Some("typescript".into()), chunk_count: 1 },
            FileSummary { file_path: "e.py".into(), language: Some("python".into()), chunk_count: 1 },
        ];
        let hist = language_histogram_by_files(&files);
        assert_eq!(hist[0], ("rust".into(), 2));
        // markdown, python, typescript all tied at 1; sorted alphabetically within the tie
        let tail: Vec<&String> = hist.iter().skip(1).map(|(k, _)| k).collect();
        assert_eq!(tail, vec!["markdown", "python", "typescript"]);
    }
}
