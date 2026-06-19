/// A supported cross-lingual sentence-embedding model.
///
/// Both are BERT-architecture encoders that map text from many languages into a
/// shared vector space, so a translation and its source land close together
/// regardless of language. Weights are downloaded from the Hugging Face Hub on
/// first use and cached locally.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Model {
    /// [`intfloat/multilingual-e5-small`](https://huggingface.co/intfloat/multilingual-e5-small)
    /// — ~118M parameters (~470 MB), 100 languages. Small and fast; the default.
    #[default]
    MultilingualE5Small,
    /// [`sentence-transformers/LaBSE`](https://huggingface.co/sentence-transformers/LaBSE)
    /// — ~470M parameters (~1.9 GB), 109 languages. Purpose-built for
    /// cross-lingual matching; larger and slower to download.
    Labse,
    /// [`BAAI/bge-reranker-v2-m3`](https://huggingface.co/BAAI/bge-reranker-v2-m3)
    /// — a ~568M parameter (~2.3 GB) multilingual *cross-encoder* reranker. It
    /// scores a pair of strings jointly for retrieval relevance.
    BgeRerankerV2M3,
    /// [`MoritzLaurer/multilingual-MiniLMv2-L6-mnli-xnli`](https://huggingface.co/MoritzLaurer/multilingual-MiniLMv2-L6-mnli-xnli)
    /// — a ~107M parameter (~430 MB) multilingual *natural language inference*
    /// cross-encoder (XLM-R backbone). Scores whether one string entails the
    /// other, targeting semantic equivalence rather than retrieval relevance.
    MultilingualMiniLmNli,
    /// [`MoritzLaurer/mDeBERTa-v3-base-mnli-xnli`](https://huggingface.co/MoritzLaurer/mDeBERTa-v3-base-mnli-xnli)
    /// — a ~279M parameter (~560 MB) multilingual NLI cross-encoder on a
    /// stronger DeBERTa-v3 backbone; the reference multilingual NLI model.
    MdebertaV3Nli,
}

/// The transformer backbone behind a cross-encoder, which determines how its
/// weights are loaded and run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CrossEncoderFamily {
    /// XLM-RoBERTa sequence-classification head.
    XlmRoberta,
    /// DeBERTa-v2/v3 sequence-classification head.
    Deberta,
}

/// How a cross-encoder turns its output logits into a `[0, 1]` similarity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CrossEncoderScoring {
    /// A single relevance logit passed through a sigmoid (reranker models).
    RerankerRelevance,
    /// Three NLI logits passed through a softmax; the probability at
    /// `entailment_index` is the score (NLI models).
    NliEntailment { entailment_index: usize },
}

impl CrossEncoderScoring {
    /// Number of classification labels the underlying model produces.
    pub(crate) fn num_labels(self) -> usize {
        match self {
            Self::RerankerRelevance => 1,
            Self::NliEntailment { .. } => 3,
        }
    }
}

/// How a model produces a similarity score for a pair of strings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Architecture {
    /// Each string is embedded independently; similarity is cosine of the two
    /// vectors. Fast (embeddings can be reused) but weaker on short strings.
    BiEncoder,
    /// Both strings are fed to the model together and scored jointly. Slower
    /// (no reuse) but far more accurate, especially on short strings.
    CrossEncoder,
}

impl Model {
    /// Every selectable model, for help text and enumeration.
    pub const ALL: [Self; 5] = [
        Self::MultilingualE5Small,
        Self::Labse,
        Self::BgeRerankerV2M3,
        Self::MultilingualMiniLmNli,
        Self::MdebertaV3Nli,
    ];

    /// The Hugging Face Hub repository the model weights are fetched from.
    #[must_use]
    pub fn repo_id(self) -> &'static str {
        match self {
            Self::MultilingualE5Small => "intfloat/multilingual-e5-small",
            Self::Labse => "sentence-transformers/LaBSE",
            Self::BgeRerankerV2M3 => "BAAI/bge-reranker-v2-m3",
            Self::MultilingualMiniLmNli => "MoritzLaurer/multilingual-MiniLMv2-L6-mnli-xnli",
            Self::MdebertaV3Nli => "MoritzLaurer/mDeBERTa-v3-base-mnli-xnli",
        }
    }

    /// A short, stable identifier used on the command line.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::MultilingualE5Small => "e5-small",
            Self::Labse => "labse",
            Self::BgeRerankerV2M3 => "bge-reranker",
            Self::MultilingualMiniLmNli => "minilm-nli",
            Self::MdebertaV3Nli => "mdeberta-nli",
        }
    }

    /// Whether this model embeds strings independently or scores them jointly.
    pub(crate) fn architecture(self) -> Architecture {
        match self {
            Self::MultilingualE5Small | Self::Labse => Architecture::BiEncoder,
            Self::BgeRerankerV2M3 | Self::MultilingualMiniLmNli | Self::MdebertaV3Nli => {
                Architecture::CrossEncoder
            }
        }
    }

    /// For cross-encoder models, how their logits become a similarity score.
    pub(crate) fn cross_encoder_scoring(self) -> Option<CrossEncoderScoring> {
        match self {
            Self::MultilingualE5Small | Self::Labse => None,
            Self::BgeRerankerV2M3 => Some(CrossEncoderScoring::RerankerRelevance),
            // id2label for both NLI models is {0: entailment, 1: neutral, 2: contradiction}.
            Self::MultilingualMiniLmNli | Self::MdebertaV3Nli => {
                Some(CrossEncoderScoring::NliEntailment {
                    entailment_index: 0,
                })
            }
        }
    }

    /// For cross-encoder models, which transformer backbone they use.
    pub(crate) fn cross_encoder_family(self) -> Option<CrossEncoderFamily> {
        match self {
            Self::MultilingualE5Small | Self::Labse => None,
            Self::BgeRerankerV2M3 | Self::MultilingualMiniLmNli => {
                Some(CrossEncoderFamily::XlmRoberta)
            }
            Self::MdebertaV3Nli => Some(CrossEncoderFamily::Deberta),
        }
    }

    /// The prefix prepended to every input before embedding, if the model
    /// expects one. The e5 family is trained to embed `query: <text>` for
    /// symmetric similarity; the others take raw text.
    #[must_use]
    pub(crate) fn input_prefix(self) -> Option<&'static str> {
        match self {
            Self::MultilingualE5Small => Some("query: "),
            Self::Labse
            | Self::BgeRerankerV2M3
            | Self::MultilingualMiniLmNli
            | Self::MdebertaV3Nli => None,
        }
    }
}

impl std::fmt::Display for Model {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.id())
    }
}

impl std::str::FromStr for Model {
    type Err = UnknownModel;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "e5-small" | "e5" | "multilingual-e5-small" => Ok(Self::MultilingualE5Small),
            "labse" => Ok(Self::Labse),
            "bge-reranker" | "reranker" | "bge" | "bge-reranker-v2-m3" => Ok(Self::BgeRerankerV2M3),
            "minilm-nli" | "nli" | "minilm" => Ok(Self::MultilingualMiniLmNli),
            "mdeberta-nli" | "mdeberta" | "deberta" => Ok(Self::MdebertaV3Nli),
            _ => Err(UnknownModel(s.to_string())),
        }
    }
}

/// Error returned when a model name cannot be parsed.
#[derive(Debug, thiserror::Error)]
#[error(
    "unknown semantic model `{0}` (expected one of: e5-small, labse, bge-reranker, minilm-nli, mdeberta-nli)"
)]
pub struct UnknownModel(pub String);
