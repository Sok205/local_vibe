use lv_core::error::VibeError;
use lv_core::types::{Message, Role};
use lv_core::Result;
use std::path::Path;

pub struct TokenizerWrapper {
    tokenizer: tokenizers::Tokenizer,
    bos_token_id: u32,
    eos_token_ids: Vec<u32>,
}

impl TokenizerWrapper {
    pub fn from_file(path: &Path) -> Result<Self> {
        let tokenizer = tokenizers::Tokenizer::from_file(path)
            .map_err(|e| VibeError::Config(format!("failed to load tokenizer: {e}")))?;

        let bos_token_id = tokenizer.token_to_id("<bos>").unwrap_or(2);

        let mut eos_token_ids = Vec::new();
        if let Some(id) = tokenizer.token_to_id("<eos>") {
            eos_token_ids.push(id);
        }
        if let Some(id) = tokenizer.token_to_id("<end_of_turn>") {
            eos_token_ids.push(id);
        }
        if eos_token_ids.is_empty() {
            eos_token_ids.push(1);
        }

        Ok(Self { tokenizer, bos_token_id, eos_token_ids })
    }

    pub fn encode(&self, text: &str) -> Result<Vec<u32>> {
        let encoding = self.tokenizer
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

    /// Apply Gemma 4 chat template.
    /// Format: <start_of_turn>user\n{msg}<end_of_turn>\n<start_of_turn>model\n
    pub fn apply_chat_template(&self, messages: &[Message]) -> String {
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
}
