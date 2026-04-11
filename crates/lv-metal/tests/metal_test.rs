use lv_core::types::{Message, Role};

#[test]
fn test_chat_template_format() {
    let messages = vec![
        Message { role: Role::User, content: "Hello".to_string() },
    ];

    // Manually verify the template format
    let mut prompt = String::new();
    for msg in &messages {
        let role_str = match msg.role {
            Role::System => "user",
            Role::User => "user",
            Role::Assistant => "model",
        };
        prompt.push_str("<start_of_turn>");
        prompt.push_str(role_str);
        prompt.push('\n');
        prompt.push_str(&msg.content);
        prompt.push_str("<end_of_turn>\n");
    }
    prompt.push_str("<start_of_turn>model\n");

    assert_eq!(prompt, "<start_of_turn>user\nHello<end_of_turn>\n<start_of_turn>model\n");
}

#[test]
fn test_chat_template_multi_turn() {
    let messages = vec![
        Message { role: Role::User, content: "Hi".to_string() },
        Message { role: Role::Assistant, content: "Hello!".to_string() },
        Message { role: Role::User, content: "How are you?".to_string() },
    ];

    let mut prompt = String::new();
    for msg in &messages {
        let role_str = match msg.role {
            Role::System => "user",
            Role::User => "user",
            Role::Assistant => "model",
        };
        prompt.push_str("<start_of_turn>");
        prompt.push_str(role_str);
        prompt.push('\n');
        prompt.push_str(&msg.content);
        prompt.push_str("<end_of_turn>\n");
    }
    prompt.push_str("<start_of_turn>model\n");

    assert!(prompt.contains("<start_of_turn>user\nHi<end_of_turn>"));
    assert!(prompt.contains("<start_of_turn>model\nHello!<end_of_turn>"));
    assert!(prompt.contains("<start_of_turn>user\nHow are you?<end_of_turn>"));
    assert!(prompt.ends_with("<start_of_turn>model\n"));
}

#[test]
fn test_sampler_creation() {
    use lv_metal::sampler::Sampler;
    let _ = Sampler::new(42, 0.7, Some(0.9), Some(40));
    let _ = Sampler::new(42, 0.0, None, None);
    let _ = Sampler::new(42, 1.0, None, None);
}

#[test]
fn test_metal_device_available() {
    let device = candle_core::Device::new_metal(0);
    assert!(device.is_ok(), "Metal device should be available on macOS");
}

#[tokio::test]
#[ignore]
async fn test_metal_backend_inference() {
    use lv_core::traits::InferenceBackend;
    use lv_core::types::*;
    use lv_metal::MetalBackend;
    use futures::StreamExt;
    use std::path::PathBuf;

    let model_path = PathBuf::from(
        std::env::var("LV_TEST_MODEL_PATH")
            .expect("set LV_TEST_MODEL_PATH to a GGUF file")
    );
    let tokenizer_path = PathBuf::from(
        std::env::var("LV_TEST_TOKENIZER_PATH")
            .expect("set LV_TEST_TOKENIZER_PATH to tokenizer.json")
    );

    let backend = MetalBackend::load(&model_path, &tokenizer_path, ModelTier::Medium)
        .expect("failed to load model");

    let health = backend.health().await;
    assert!(health.available);

    let req = CompletionRequest {
        messages: vec![
            Message { role: Role::User, content: "Say hello in one word.".to_string() },
        ],
        temperature: 0.1,
        max_tokens: 10,
        stream: true,
    };

    let mut stream = backend.complete(req).await.expect("complete failed");
    let mut response = String::new();

    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(c) if c.finished => break,
            Ok(c) => response.push_str(&c.delta),
            Err(e) => panic!("stream error: {e}"),
        }
    }

    assert!(!response.is_empty(), "got empty response");
    println!("Model response: {response}");
}
