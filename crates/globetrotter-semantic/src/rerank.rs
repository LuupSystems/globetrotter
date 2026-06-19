use crate::model::{CrossEncoderFamily, CrossEncoderScoring};
use crate::{Error, Model};
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::debertav2;
use candle_transformers::models::xlm_roberta::{self, XLMRobertaForSequenceClassification};
use std::collections::HashMap;
use tokenizers::{PaddingParams, Tokenizer, TruncationParams};

/// Token pairs longer than this are truncated. The longest-first truncation
/// strategy trims whichever side is longer, keeping the pair balanced.
const MAX_TOKENS: usize = 256;

/// How many pairs are scored in a single forward pass.
const SCORE_BATCH: usize = 8;

/// The loaded sequence-classification backbone behind a [`CrossEncoder`].
enum Classifier {
    XlmRoberta(Box<XLMRobertaForSequenceClassification>),
    Deberta(Box<debertav2::DebertaV2SeqClassificationModel>),
}

/// A cross-encoder that scores how well two strings correspond, by feeding both
/// to the model together (joint cross-attention) rather than embedding each
/// alone. Backs both the reranker and NLI models.
pub struct CrossEncoder {
    classifier: Classifier,
    tokenizer: Tokenizer,
    device: Device,
    scoring: CrossEncoderScoring,
}

impl CrossEncoder {
    /// Download (or load from cache) the given cross-encoder model.
    ///
    /// # Errors
    ///
    /// Returns an error if the model is not a cross-encoder, if its files
    /// cannot be downloaded, or if the config, tokenizer, or weights cannot be
    /// loaded.
    pub fn load(model: Model, cache_dir: Option<std::path::PathBuf>) -> Result<Self, Error> {
        let scoring = model
            .cross_encoder_scoring()
            .ok_or(Error::NotACrossEncoder)?;
        let family = model
            .cross_encoder_family()
            .ok_or(Error::NotACrossEncoder)?;
        let device = Device::Cpu;

        let api = {
            let mut builder = hf_hub::api::sync::ApiBuilder::new();
            if let Some(dir) = cache_dir {
                builder = builder.with_cache_dir(dir);
            }
            builder.build()?
        };
        let repo = api.model(model.repo_id().to_string());

        tracing::debug!(repo = model.repo_id(), "resolving cross-encoder files");
        let config_path = repo.get("config.json")?;
        let tokenizer_path = repo.get("tokenizer.json")?;
        let weights_path = repo.get("model.safetensors")?;
        let config_bytes = std::fs::read(&config_path)?;

        tracing::debug!(?weights_path, "loading cross-encoder weights");
        let tensors = candle_core::safetensors::load(&weights_path, &device)?;

        let (classifier, pad_id) = match family {
            CrossEncoderFamily::XlmRoberta => {
                let config: xlm_roberta::Config = serde_json::from_slice(&config_bytes)?;
                let pad_id = config.pad_token_id;
                let vb = VarBuilder::from_tensors(tensors, DType::F32, &device);
                let model =
                    XLMRobertaForSequenceClassification::new(scoring.num_labels(), &config, vb)?;
                (Classifier::XlmRoberta(Box::new(model)), pad_id)
            }
            CrossEncoderFamily::Deberta => {
                let config: debertav2::Config = serde_json::from_slice(&config_bytes)?;
                let pad_id = u32::try_from(config.pad_token_id.unwrap_or(0)).unwrap_or(0);
                // HF stores the backbone under a `deberta.` prefix; candle expects
                // it at the root (with `pooler`/`classifier` alongside).
                let tensors = strip_prefix(tensors, "deberta.");
                let vb = VarBuilder::from_tensors(tensors, DType::F32, &device);
                let model = debertav2::DebertaV2SeqClassificationModel::load(vb, &config, None)?;
                (Classifier::Deberta(Box::new(model)), pad_id)
            }
        };

        let mut tokenizer = Tokenizer::from_file(&tokenizer_path).map_err(Error::tokenizer)?;
        tokenizer.with_padding(Some(PaddingParams {
            pad_id,
            ..Default::default()
        }));
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: MAX_TOKENS,
                ..Default::default()
            }))
            .map_err(Error::tokenizer)?;

        Ok(Self {
            classifier,
            tokenizer,
            device,
            scoring,
        })
    }

    /// Score each `(a, b)` pair in `[0.0, 1.0]`, where higher means the two
    /// strings correspond more closely.
    ///
    /// # Errors
    ///
    /// Returns an error if tokenization or the forward pass fails.
    pub fn score_pairs(
        &self,
        pairs: &[(String, String)],
        progress: &dyn crate::Progress,
        on_score: &mut dyn FnMut(usize, f32),
    ) -> Result<(), Error> {
        for (chunk_index, chunk) in pairs.chunks(SCORE_BATCH).enumerate() {
            let base = chunk_index * SCORE_BATCH;
            for (offset, score) in self.score_chunk(chunk)?.into_iter().enumerate() {
                on_score(base + offset, score);
            }
            progress.inc(chunk.len() as u64);
        }
        Ok(())
    }

    fn score_chunk(&self, pairs: &[(String, String)]) -> Result<Vec<f32>, Error> {
        if pairs.is_empty() {
            return Ok(Vec::new());
        }

        let encodings = self
            .tokenizer
            .encode_batch(pairs.to_vec(), true)
            .map_err(Error::tokenizer)?;

        let batch = encodings.len();
        let seq_len = encodings
            .iter()
            .map(|encoding| encoding.get_ids().len())
            .max()
            .unwrap_or(0);

        let mut ids: Vec<u32> = Vec::with_capacity(batch * seq_len);
        let mut mask: Vec<f32> = Vec::with_capacity(batch * seq_len);
        for encoding in &encodings {
            ids.extend_from_slice(encoding.get_ids());
            mask.extend(
                encoding
                    .get_attention_mask()
                    .iter()
                    .map(|&m| if m == 0 { 0.0 } else { 1.0 }),
            );
        }

        let input_ids = Tensor::from_vec(ids, (batch, seq_len), &self.device)?;
        let attention = Tensor::from_vec(mask, (batch, seq_len), &self.device)?;

        let logits = match &self.classifier {
            Classifier::XlmRoberta(model) => {
                let token_type_ids = input_ids.zeros_like()?;
                model.forward(&input_ids, &attention, &token_type_ids)?
            }
            Classifier::Deberta(model) => model.forward(&input_ids, None, Some(attention))?,
        };

        let scores = match self.scoring {
            CrossEncoderScoring::RerankerRelevance => {
                // [batch, 1] relevance logit -> sigmoid.
                candle_nn::ops::sigmoid(&logits)?.flatten_all()?
            }
            CrossEncoderScoring::NliEntailment { entailment_index } => {
                // [batch, 3] NLI logits -> softmax, take the entailment column.
                candle_nn::ops::softmax(&logits, candle_core::D::Minus1)?
                    .narrow(1, entailment_index, 1)?
                    .flatten_all()?
            }
        };
        Ok(scores.to_vec1::<f32>()?)
    }
}

/// Strip a leading key prefix from every tensor name that has it.
fn strip_prefix(tensors: HashMap<String, Tensor>, prefix: &str) -> HashMap<String, Tensor> {
    tensors
        .into_iter()
        .map(|(key, value)| {
            let key = match key.strip_prefix(prefix) {
                Some(rest) => rest.to_string(),
                None => key,
            };
            (key, value)
        })
        .collect()
}
