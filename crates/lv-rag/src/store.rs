use std::sync::Arc;

use arrow_array::{
    Array, ArrayRef, FixedSizeListArray, Float32Array, RecordBatch, RecordBatchIterator,
    StringArray, UInt32Array,
};
use arrow_schema::{DataType, Field, Schema};
use async_trait::async_trait;
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::{Connection, Table as LanceTable, connect};
use tokio::sync::RwLock;

use lv_core::types::{Document, FileSummary, SearchFilter, SearchResult, StoreStats};
use lv_core::{Result, VibeError};

/// Vector store backed by LanceDB (embedded, persistent, Arrow-based).
pub struct LanceStore {
    db: Connection,
    table: RwLock<Option<LanceTable>>,
    dimension: usize,
}

fn sanitize_sql_value(value: &str) -> String {
    value.replace('\'', "''")
}

impl LanceStore {
    pub async fn new(db_path: &str, dimension: usize) -> Result<Self> {
        let db = connect(db_path)
            .execute()
            .await
            .map_err(|e| VibeError::Store(format!("Failed to connect to LanceDB: {e}")))?;

        let table = db.open_table("chunks").execute().await.ok();

        Ok(Self {
            db,
            table: RwLock::new(table),
            dimension,
        })
    }

    fn schema(&self) -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("text", DataType::Utf8, false),
            Field::new(
                "vector",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    self.dimension as i32,
                ),
                false,
            ),
            Field::new("file_path", DataType::Utf8, false),
            Field::new("file_name", DataType::Utf8, false),
            Field::new("file_hash", DataType::Utf8, false),
            Field::new("chunk_index", DataType::UInt32, false),
            Field::new("language", DataType::Utf8, true),
            Field::new("symbol_context", DataType::Utf8, true),
        ]))
    }

    fn documents_to_batch(&self, docs: &[Document]) -> std::result::Result<RecordBatch, VibeError> {
        let ids: Vec<String> = docs
            .iter()
            .map(|d| format!("{}-{}", d.file_hash, d.chunk_index))
            .collect();
        let id_refs: Vec<&str> = ids.iter().map(|s| s.as_str()).collect();
        let texts: Vec<&str> = docs.iter().map(|d| d.text.as_str()).collect();
        let file_paths: Vec<&str> = docs.iter().map(|d| d.file_path.as_str()).collect();
        let file_names: Vec<&str> = docs.iter().map(|d| d.file_name.as_str()).collect();
        let file_hashes: Vec<&str> = docs.iter().map(|d| d.file_hash.as_str()).collect();
        let chunk_indices: Vec<u32> = docs.iter().map(|d| d.chunk_index).collect();

        let all_floats: Vec<f32> = docs
            .iter()
            .flat_map(|d| d.embedding.iter().copied())
            .collect();
        let float_array = Float32Array::from(all_floats);
        let field = Arc::new(Field::new("item", DataType::Float32, true));
        let vector_array = FixedSizeListArray::try_new(
            field,
            self.dimension as i32,
            Arc::new(float_array) as ArrayRef,
            None,
        )
        .map_err(|e| VibeError::Store(format!("Failed to create vector array: {e}")))?;

        let language_values: Vec<Option<&str>> =
            docs.iter().map(|d| d.language.as_deref()).collect();
        let symbol_context_values: Vec<Option<&str>> =
            docs.iter().map(|d| d.symbol_context.as_deref()).collect();

        let batch = RecordBatch::try_new(
            self.schema(),
            vec![
                Arc::new(StringArray::from(id_refs)) as ArrayRef,
                Arc::new(StringArray::from(texts)) as ArrayRef,
                Arc::new(vector_array) as ArrayRef,
                Arc::new(StringArray::from(file_paths)) as ArrayRef,
                Arc::new(StringArray::from(file_names)) as ArrayRef,
                Arc::new(StringArray::from(file_hashes)) as ArrayRef,
                Arc::new(UInt32Array::from(chunk_indices)) as ArrayRef,
                Arc::new(StringArray::from(language_values)) as ArrayRef,
                Arc::new(StringArray::from(symbol_context_values)) as ArrayRef,
            ],
        )
        .map_err(|e| VibeError::Store(format!("Failed to create record batch: {e}")))?;

        Ok(batch)
    }
}

fn filter_to_sql(filter: &SearchFilter) -> Option<String> {
    let mut clauses = Vec::new();
    if let Some(ref lang) = filter.language {
        let safe = sanitize_sql_value(lang);
        clauses.push(format!("language = '{safe}'"));
    }
    if let Some(ref fp) = filter.file_path {
        let safe = sanitize_sql_value(fp);
        clauses.push(format!("file_path = '{safe}'"));
    }
    if clauses.is_empty() {
        None
    } else {
        Some(clauses.join(" AND "))
    }
}

#[async_trait]
impl lv_core::traits::VectorStore for LanceStore {
    async fn add_documents(&self, docs: &[Document]) -> Result<()> {
        if docs.is_empty() {
            return Ok(());
        }

        let batch = self.documents_to_batch(docs)?;
        let schema = self.schema();

        {
            let guard = self.table.read().await;
            if let Some(table) = &*guard {
                let batches: Box<dyn arrow_array::RecordBatchReader + Send> =
                    Box::new(RecordBatchIterator::new(vec![Ok(batch)], schema));
                table
                    .add(batches)
                    .execute()
                    .await
                    .map_err(|e| VibeError::Store(format!("Failed to add documents: {e}")))?;
                return Ok(());
            }
        }

        let mut guard = self.table.write().await;
        if let Some(table) = &*guard {
            let batches: Box<dyn arrow_array::RecordBatchReader + Send> =
                Box::new(RecordBatchIterator::new(vec![Ok(batch)], schema));
            table
                .add(batches)
                .execute()
                .await
                .map_err(|e| VibeError::Store(format!("Failed to add documents: {e}")))?;
        } else {
            let batches: Box<dyn arrow_array::RecordBatchReader + Send> =
                Box::new(RecordBatchIterator::new(vec![Ok(batch)], schema));
            let table = self
                .db
                .create_table("chunks", batches)
                .execute()
                .await
                .map_err(|e| VibeError::Store(format!("Failed to create table: {e}")))?;
            *guard = Some(table);
        }

        Ok(())
    }

    async fn search(
        &self,
        query: &[f32],
        limit: usize,
        threshold: f32,
        filter: &SearchFilter,
    ) -> Result<Vec<SearchResult>> {
        let guard = self.table.read().await;
        let table = match &*guard {
            Some(t) => t,
            None => return Ok(Vec::new()),
        };

        let fetch_limit = if threshold > 0.0 { limit * 3 } else { limit };

        let mut search_builder = table
            .vector_search(query)
            .map_err(|e| VibeError::Store(format!("Search setup failed: {e}")))?;
        search_builder = search_builder.limit(fetch_limit);

        if let Some(sql_filter) = filter_to_sql(filter) {
            search_builder = search_builder.only_if(sql_filter);
        }

        let results = search_builder
            .execute()
            .await
            .map_err(|e| VibeError::Store(format!("Search failed: {e}")))?;

        use arrow_array::cast::AsArray;
        use arrow_array::types::Float32Type;

        let batches: Vec<RecordBatch> = results
            .try_collect()
            .await
            .map_err(|e| VibeError::Store(format!("Failed to collect results: {e}")))?;

        let mut search_results = Vec::new();
        for batch in &batches {
            let texts = batch.column_by_name("text").unwrap().as_string::<i32>();
            let file_paths = batch
                .column_by_name("file_path")
                .unwrap()
                .as_string::<i32>();
            let file_names = batch
                .column_by_name("file_name")
                .unwrap()
                .as_string::<i32>();
            let chunk_indices = batch
                .column_by_name("chunk_index")
                .unwrap()
                .as_primitive::<arrow_array::types::UInt32Type>();
            let distances = batch
                .column_by_name("_distance")
                .unwrap()
                .as_primitive::<Float32Type>();

            for i in 0..batch.num_rows() {
                let distance = distances.value(i);
                let score = 1.0 / (1.0 + distance);
                if threshold > 0.0 && score < threshold {
                    continue;
                }

                search_results.push(SearchResult {
                    text: texts.value(i).to_string(),
                    score,
                    file_path: file_paths.value(i).to_string(),
                    file_name: file_names.value(i).to_string(),
                    chunk_index: chunk_indices.value(i),
                });
            }
        }

        search_results.truncate(limit);
        Ok(search_results)
    }

    async fn delete_by_hash(&self, file_hash: &str) -> Result<()> {
        let guard = self.table.read().await;
        let table = match &*guard {
            Some(t) => t,
            None => return Ok(()),
        };

        let safe_hash = sanitize_sql_value(file_hash);
        table
            .delete(&format!("file_hash = '{safe_hash}'"))
            .await
            .map_err(|e| VibeError::Store(format!("Delete failed: {e}")))?;

        Ok(())
    }

    async fn has_file(&self, file_hash: &str) -> Result<bool> {
        let guard = self.table.read().await;
        let table = match &*guard {
            Some(t) => t,
            None => return Ok(false),
        };

        let safe_hash = sanitize_sql_value(file_hash);
        let results = table
            .query()
            .only_if(format!("file_hash = '{safe_hash}'"))
            .limit(1)
            .execute()
            .await
            .map_err(|e| VibeError::Store(format!("Query failed: {e}")))?;

        let batches: Vec<RecordBatch> = results
            .try_collect()
            .await
            .map_err(|e| VibeError::Store(format!("Failed to collect: {e}")))?;

        Ok(batches.iter().any(|b| b.num_rows() > 0))
    }

    async fn stats(&self) -> Result<StoreStats> {
        let guard = self.table.read().await;
        let table = match &*guard {
            Some(t) => t,
            None => {
                return Ok(StoreStats {
                    total_chunks: 0,
                    unique_files: 0,
                });
            }
        };

        let count = table
            .count_rows(None)
            .await
            .map_err(|e| VibeError::Store(format!("Count failed: {e}")))?;

        let results = table
            .query()
            .select(lancedb::query::Select::columns(&["file_hash"]))
            .execute()
            .await
            .map_err(|e| VibeError::Store(format!("Query failed: {e}")))?;

        use rustc_hash::FxHashSet;

        let mut unique_hashes = FxHashSet::default();
        futures::pin_mut!(results);
        while let Some(batch) = results
            .try_next()
            .await
            .map_err(|e| VibeError::Store(format!("Failed to read batch: {e}")))?
        {
            let hashes = batch
                .column_by_name("file_hash")
                .unwrap()
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();
            for i in 0..hashes.len() {
                let val = hashes.value(i);
                if !unique_hashes.contains(val) {
                    unique_hashes.insert(val.to_string());
                }
            }
        }

        Ok(StoreStats {
            total_chunks: count,
            unique_files: unique_hashes.len(),
        })
    }

    async fn list_files(&self, limit: usize) -> Result<Vec<FileSummary>> {
        let guard = self.table.read().await;
        let table = match &*guard {
            Some(t) => t,
            None => return Ok(Vec::new()),
        };

        let results = table
            .query()
            .select(lancedb::query::Select::columns(&["file_path", "language"]))
            .execute()
            .await
            .map_err(|e| VibeError::Store(format!("Query failed: {e}")))?;

        use rustc_hash::FxHashMap;
        let mut by_path: FxHashMap<String, FileSummary> = FxHashMap::default();
        futures::pin_mut!(results);
        while let Some(batch) = results
            .try_next()
            .await
            .map_err(|e| VibeError::Store(format!("Failed to read batch: {e}")))?
        {
            let paths = batch
                .column_by_name("file_path")
                .unwrap()
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();
            let langs = batch
                .column_by_name("language")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());
            for i in 0..paths.len() {
                let path = paths.value(i).to_string();
                let lang = langs.and_then(|l| {
                    if l.is_null(i) {
                        None
                    } else {
                        Some(l.value(i).to_string())
                    }
                });
                by_path
                    .entry(path.clone())
                    .and_modify(|s| s.chunk_count += 1)
                    .or_insert(FileSummary {
                        file_path: path,
                        language: lang,
                        chunk_count: 1,
                    });
            }
        }

        let mut out: Vec<FileSummary> = by_path.into_values().collect();
        out.sort_by(|a, b| a.file_path.cmp(&b.file_path));
        if out.len() > limit {
            out.truncate(limit);
        }
        Ok(out)
    }
}
