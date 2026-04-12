use async_trait::async_trait;
use lv_core::traits::EmbeddingBackend;
use lv_core::Result;

struct FixedEmbedder;

#[async_trait]
impl EmbeddingBackend for FixedEmbedder {
    async fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|_| vec![0.1, 0.2, 0.3]).collect())
    }
    fn dim(&self) -> usize { 3 }
    fn model_name(&self) -> &str { "fixed" }
}

#[tokio::test]
async fn fixed_embedder_returns_dim_length_vectors() {
    let e = FixedEmbedder;
    assert_eq!(e.dim(), 3);
    assert_eq!(e.model_name(), "fixed");
    let out = e.embed(&["a", "b"]).await.unwrap();
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].len(), e.dim());
}
