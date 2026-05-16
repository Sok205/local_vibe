use candle_core::Tensor;
use candle_transformers::generation::LogitsProcessor;
use lv_core::Result;
use lv_core::error::VibeError;

pub struct Sampler {
    processor: LogitsProcessor,
}

impl Sampler {
    pub fn new(seed: u64, temperature: f64, top_p: Option<f64>, top_k: Option<usize>) -> Self {
        use candle_transformers::generation::Sampling;

        let sampling = match (temperature, top_p, top_k) {
            (t, _, _) if t <= 0.0 => Sampling::ArgMax,
            (t, Some(p), Some(k)) => Sampling::TopKThenTopP {
                k,
                p,
                temperature: t,
            },
            (t, Some(p), None) => Sampling::TopP { p, temperature: t },
            (t, None, Some(k)) => Sampling::TopK { k, temperature: t },
            (t, None, None) => Sampling::All { temperature: t },
        };

        Self {
            processor: LogitsProcessor::from_sampling(seed, sampling),
        }
    }

    pub fn sample(&mut self, logits: &Tensor) -> Result<u32> {
        self.processor
            .sample(logits)
            .map_err(|e| VibeError::Inference(format!("sampling failed: {e}")))
    }
}
