use lv_core::types::{Message, Role};

#[test]
fn test_chat_template_format() {
    let messages = vec![Message {
        role: Role::User,
        content: "Hello".to_string(),
    }];

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

    assert_eq!(
        prompt,
        "<start_of_turn>user\nHello<end_of_turn>\n<start_of_turn>model\n"
    );
}

#[test]
fn test_chat_template_multi_turn() {
    let messages = vec![
        Message {
            role: Role::User,
            content: "Hi".to_string(),
        },
        Message {
            role: Role::Assistant,
            content: "Hello!".to_string(),
        },
        Message {
            role: Role::User,
            content: "How are you?".to_string(),
        },
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
    use futures::StreamExt;
    use lv_core::traits::InferenceBackend;
    use lv_core::types::*;
    use lv_metal::MetalBackend;
    use std::path::PathBuf;

    let model_path = PathBuf::from(
        std::env::var("LV_TEST_MODEL_PATH").expect("set LV_TEST_MODEL_PATH to a GGUF file"),
    );
    let tokenizer_path = PathBuf::from(
        std::env::var("LV_TEST_TOKENIZER_PATH")
            .expect("set LV_TEST_TOKENIZER_PATH to tokenizer.json"),
    );

    let backend = MetalBackend::load(&model_path, &tokenizer_path, ModelTier::Medium)
        .expect("failed to load model");

    let health = backend.health().await;
    assert!(health.available);

    let req = CompletionRequest {
        messages: vec![Message {
            role: Role::User,
            content: "Say hello in one word.".to_string(),
        }],
        temperature: 0.1,
        max_tokens: 10,
        stream: true,
        session_id: None,
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

#[tokio::test]
#[ignore]
async fn bench_metal_throughput() {
    use futures::StreamExt;
    use lv_core::traits::InferenceBackend;
    use lv_core::types::*;
    use lv_metal::MetalBackend;
    use std::path::PathBuf;
    use std::time::Instant;

    let model_path = PathBuf::from(
        std::env::var("LV_TEST_MODEL_PATH").expect("set LV_TEST_MODEL_PATH to a GGUF file"),
    );
    let tokenizer_path = PathBuf::from(
        std::env::var("LV_TEST_TOKENIZER_PATH")
            .expect("set LV_TEST_TOKENIZER_PATH to tokenizer.json"),
    );

    let load_start = Instant::now();
    let backend = MetalBackend::load(&model_path, &tokenizer_path, ModelTier::Medium)
        .expect("failed to load model");
    let load_duration = load_start.elapsed();
    println!("\n=== BENCHMARK ===");
    println!("Load time: {:.2}s", load_duration.as_secs_f64());

    // Warmup
    let warmup_req = CompletionRequest {
        messages: vec![Message {
            role: Role::User,
            content: "Hi".to_string(),
        }],
        temperature: 0.7,
        max_tokens: 5,
        stream: true,
        session_id: None,
    };
    let mut stream = backend.complete(warmup_req).await.unwrap();
    while let Some(chunk) = stream.next().await {
        if let Ok(c) = chunk
            && c.finished
        {
            break;
        }
    }
    println!("Warmup complete");

    // Actual benchmark
    let req = CompletionRequest {
        messages: vec![Message {
            role: Role::User,
            content: "Write a short paragraph about the Rust programming language.".to_string(),
        }],
        temperature: 0.7,
        max_tokens: 100,
        stream: true,
        session_id: None,
    };

    let gen_start = Instant::now();
    let mut stream = backend.complete(req).await.expect("complete failed");
    let mut response = String::new();
    let mut token_count = 0;
    let mut first_token_time = None;

    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(c) if c.finished => break,
            Ok(c) => {
                if first_token_time.is_none() {
                    first_token_time = Some(gen_start.elapsed());
                }
                response.push_str(&c.delta);
                token_count += 1;
            }
            Err(e) => panic!("stream error: {e}"),
        }
    }

    let total = gen_start.elapsed();
    let ttft = first_token_time.unwrap_or(total);
    let gen_time = total - ttft;
    let tok_per_sec = if gen_time.as_secs_f64() > 0.0 {
        (token_count - 1) as f64 / gen_time.as_secs_f64()
    } else {
        0.0
    };

    println!("\n=== RESULTS ===");
    println!("Total time: {:.2}s", total.as_secs_f64());
    println!("Time to first token: {:.2}s", ttft.as_secs_f64());
    println!("Tokens generated: {}", token_count);
    println!(
        "Generation time (excl. TTFT): {:.2}s",
        gen_time.as_secs_f64()
    );
    println!("Throughput: {:.1} tok/s", tok_per_sec);
    println!("\nResponse: {response}");
}
