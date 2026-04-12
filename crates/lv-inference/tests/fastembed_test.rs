#[tokio::test]
#[ignore]
async fn fastembed_nomic_dim_is_768() {
    use lv_core::traits::EmbeddingBackend;
    use lv_inference::fastembed_backend::FastEmbedBackend;
    let backend = FastEmbedBackend::new("nomic-embed-text").unwrap();
    let out = backend.embed(&["hello world"]).await.unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(backend.dim(), 768);
    assert_eq!(out[0].len(), 768);
}
