use lv_core::error::VibeError;
use lv_core::types::{Message, Role};
use lv_core::Result;
use std::path::Path;

#[derive(Debug, Clone, Copy)]
enum TemplateKind {
    /// Qwen / ChatML: `<|im_start|>role\n...<|im_end|>`
    ChatMl,
    /// Gemma 2 / 3 / 4: `<start_of_turn>role\n...<end_of_turn>`
    Gemma,
}

pub struct TokenizerWrapper {
    tokenizer: tokenizers::Tokenizer,
    bos_token_id: u32,
    eos_token_ids: Vec<u32>,
    template: TemplateKind,
}

impl TokenizerWrapper {
    pub fn from_file(path: &Path) -> Result<Self> {
        let tokenizer = tokenizers::Tokenizer::from_file(path)
            .map_err(|e| VibeError::Config(format!("failed to load tokenizer: {e}")))?;

        let bos_token_id = tokenizer.token_to_id("<bos>").unwrap_or(2);

        let template = if tokenizer.token_to_id("<|im_end|>").is_some() {
            TemplateKind::ChatMl
        } else {
            TemplateKind::Gemma
        };

        let mut eos_token_ids = Vec::new();
        for tok in ["<eos>", "<end_of_turn>", "<|im_end|>", "<|endoftext|>"] {
            if let Some(id) = tokenizer.token_to_id(tok) {
                eos_token_ids.push(id);
            }
        }
        if eos_token_ids.is_empty() {
            eos_token_ids.push(1);
        }

        Ok(Self {
            tokenizer,
            bos_token_id,
            eos_token_ids,
            template,
        })
    }

    pub fn encode(&self, text: &str) -> Result<Vec<u32>> {
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| VibeError::Inference(format!("tokenization failed: {e}")))?;
        Ok(encoding.get_ids().to_vec())
    }

    pub fn decode(&self, ids: &[u32]) -> Result<String> {
        self.tokenizer
            .decode(ids, true)
            .map_err(|e| VibeError::Inference(format!("detokenization failed: {e}")))
    }

    pub fn is_eos(&self, token_id: u32) -> bool {
        self.eos_token_ids.contains(&token_id)
    }

    pub fn bos_token_id(&self) -> u32 {
        self.bos_token_id
    }

    /// Apply the chat template inferred from the tokenizer vocab
    /// (ChatML for Qwen, `<start_of_turn>` for Gemma).
    pub fn apply_chat_template(&self, messages: &[Message]) -> String {
        match self.template {
            TemplateKind::ChatMl => chatml_template(messages),
            TemplateKind::Gemma => gemma_template(messages),
        }
    }
}

fn chatml_template(messages: &[Message]) -> String {
    let mut prompt = String::new();
    for msg in messages {
        let role_str = match msg.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
        };
        prompt.push_str("<|im_start|>");
        prompt.push_str(role_str);
        prompt.push('\n');
        prompt.push_str(&msg.content);
        prompt.push_str("<|im_end|>\n");
    }
    prompt.push_str("<|im_start|>assistant\n");
    prompt
}

fn gemma_template(messages: &[Message]) -> String {
    let mut prompt = String::new();
    for msg in messages {
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
    prompt
}
