use crate::{Error, Model};
use candle_core::{DType, Device, IndexOp, Tensor};
use candle_nn::{Linear, Module, VarBuilder};
use candle_transformers::models::bert::{BertModel, Config, DTYPE};
use tokenizers::{PaddingParams, Tokenizer, TruncationParams};

/// Inputs longer than this many tokens are truncated before embedding. Bounds
/// per-sentence compute; translation strings are almost always far shorter.
const MAX_TOKENS: usize = 256;

/// How a model turns per-token hidden states into one sentence vector. The
/// choice is model-specific: using the wrong head badly degrades similarity.
enum Head {
    /// Mean-pool the tokens (attention-weighted), then L2-normalize. Used by
    /// the e5 family.
    Mean,
    /// Take the `CLS` token, apply a dense layer with `tanh`, then L2-normalize.
    /// This is `LaBSE`'s trained sentence-embedding head.
    ClsDense(Linear),
}

/// A loaded sentence-embedding model ready to turn text into vectors.
pub struct Embedder {
    model: BertModel,
    tokenizer: Tokenizer,
    device: Device,
    input_prefix: Option<&'static str>,
    head: Head,
}

impl Embedder {
    /// Download (or load from cache) the given model and prepare it for
    /// embedding on the CPU.
    ///
    /// `cache_dir` overrides the Hugging Face cache location; when `None` the
    /// default (`~/.cache/huggingface`) is used.
    ///
    /// # Errors
    ///
    /// Returns an error if the model files cannot be downloaded, the config or
    /// tokenizer cannot be parsed, or the weights cannot be loaded.
    pub fn load(model: Model, cache_dir: Option<std::path::PathBuf>) -> Result<Self, Error> {
        let device = Device::Cpu;
        let input_prefix = model.input_prefix();

        let api = {
            let mut builder = hf_hub::api::sync::ApiBuilder::new();
            if let Some(dir) = cache_dir {
                builder = builder.with_cache_dir(dir);
            }
            builder.build()?
        };
        let repo = api.model(model.repo_id().to_string());

        tracing::debug!(repo = model.repo_id(), "resolving model files");
        let config_path = repo.get("config.json")?;
        let tokenizer_path = repo.get("tokenizer.json")?;
        let weights_path = repo.get("model.safetensors")?;

        let config: Config = serde_json::from_slice(&std::fs::read(&config_path)?)?;

        let mut tokenizer = Tokenizer::from_file(&tokenizer_path).map_err(Error::tokenizer)?;
        tokenizer.with_padding(Some(PaddingParams::default()));
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: MAX_TOKENS,
                ..Default::default()
            }))
            .map_err(Error::tokenizer)?;

        tracing::debug!(?weights_path, "loading weights");
        let tensors = candle_core::safetensors::load(&weights_path, &device)?;
        let vb = VarBuilder::from_tensors(tensors, DTYPE, &device);

        // Build the model-specific pooling head before the model consumes `vb`.
        let head = match model {
            Model::Labse => {
                let dense = candle_nn::linear(
                    config.hidden_size,
                    config.hidden_size,
                    vb.pp("pooler").pp("dense"),
                )?;
                Head::ClsDense(dense)
            }
            // e5 mean-pools; the cross-encoder models never load as an Embedder.
            Model::MultilingualE5Small
            | Model::BgeRerankerV2M3
            | Model::MultilingualMiniLmNli
            | Model::MdebertaV3Nli => Head::Mean,
        };

        let model = BertModel::load(vb, &config)?;

        Ok(Self {
            model,
            tokenizer,
            device,
            input_prefix,
            head,
        })
    }

    /// Embed a batch of texts into L2-normalized vectors (one per input).
    ///
    /// The cosine similarity of two such vectors is simply their dot product.
    ///
    /// # Errors
    ///
    /// Returns an error if tokenization or the forward pass fails.
    pub fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, Error> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let prepared: Vec<String> = texts
            .iter()
            .map(|text| match self.input_prefix {
                Some(prefix) => format!("{prefix}{text}"),
                None => text.clone(),
            })
            .collect();

        let encodings = self
            .tokenizer
            .encode_batch(prepared, true)
            .map_err(Error::tokenizer)?;

        let batch = encodings.len();
        let seq_len = encodings
            .iter()
            .map(|encoding| encoding.get_ids().len())
            .max()
            .unwrap_or(0);

        let mut ids: Vec<u32> = Vec::with_capacity(batch * seq_len);
        let mut mask: Vec<u32> = Vec::with_capacity(batch * seq_len);
        for encoding in &encodings {
            ids.extend_from_slice(encoding.get_ids());
            mask.extend_from_slice(encoding.get_attention_mask());
        }

        let token_ids = Tensor::from_vec(ids, (batch, seq_len), &self.device)?;
        let attention = Tensor::from_vec(mask, (batch, seq_len), &self.device)?;
        let token_type_ids = token_ids.zeros_like()?;

        let hidden = self
            .model
            .forward(&token_ids, &token_type_ids, Some(&attention))?;

        let pooled = match &self.head {
            Head::Mean => mean_pool(&hidden, &attention)?,
            Head::ClsDense(dense) => {
                let cls = hidden.i((.., 0))?;
                dense.forward(&cls)?.tanh()?
            }
        };
        let normalized = l2_normalize(&pooled)?;
        Ok(normalized.to_vec2::<f32>()?)
    }
}

/// Mean-pool a `[batch, seq, hidden]` tensor over the sequence dimension,
/// weighting by the attention mask so padding tokens do not contribute.
fn mean_pool(hidden: &Tensor, attention: &Tensor) -> candle_core::Result<Tensor> {
    let mask = attention.to_dtype(DType::F32)?.unsqueeze(2)?;
    let summed = hidden.broadcast_mul(&mask)?.sum(1)?;
    let counts = mask.sum(1)?;
    summed.broadcast_div(&counts)
}

/// L2-normalize each row of a `[batch, hidden]` tensor.
fn l2_normalize(vectors: &Tensor) -> candle_core::Result<Tensor> {
    let norm = vectors.sqr()?.sum_keepdim(1)?.sqrt()?;
    vectors.broadcast_div(&norm)
}
