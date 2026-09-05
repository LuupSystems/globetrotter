//! Source parsing separates references from catalog matching and per-config policy.

use globetrotter_model::diagnostics::Span;
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Dialect {
    Javascript,
    Typescript,
    Tsx,
    #[cfg(feature = "tree-sitter")]
    AngularExpression,
    Rust,
    Go,
    Python,
    Ruby,
    Php,
    Java,
    Kotlin,
    Swift,
    Dart,
    Elixir,
    Lua,
    Zig,
    CSharp,
    Html,
    Vue,
    Svelte,
    Astro,
}

impl Dialect {
    pub(super) fn for_path(path: &Path) -> Option<Self> {
        Some(
            match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
                "ts" | "mts" | "cts" => Self::Typescript,
                "tsx" => Self::Tsx,
                "js" | "jsx" | "mjs" | "cjs" => Self::Javascript,
                "rs" => Self::Rust,
                "go" => Self::Go,
                "py" => Self::Python,
                "rb" => Self::Ruby,
                "php" => Self::Php,
                "java" => Self::Java,
                "kt" => Self::Kotlin,
                "swift" => Self::Swift,
                "dart" => Self::Dart,
                "ex" | "exs" => Self::Elixir,
                "lua" => Self::Lua,
                "zig" => Self::Zig,
                "cs" => Self::CSharp,
                "html" | "htm" => Self::Html,
                "vue" => Self::Vue,
                "svelte" => Self::Svelte,
                "astro" => Self::Astro,
                _ => return None,
            },
        )
    }
}

#[derive(Default)]
pub(super) struct References {
    pub literals: BTreeSet<String>,
    pub identifiers: BTreeSet<String>,
    pub dynamic: Vec<DynamicReference>,
}

pub(crate) struct DynamicReference {
    pub prefix: Option<String>,
    pub span: Span,
}

#[cfg_attr(
    not(feature = "tree-sitter"),
    expect(
        clippy::unnecessary_wraps,
        reason = "The parser-enabled implementation is fallible; both feature configurations share this scanner interface."
    )
)]
pub(super) fn scan(
    path: &Path,
    content: &str,
    functions: &[String],
) -> std::io::Result<References> {
    #[cfg(feature = "tree-sitter")]
    {
        super::syntax::scan(path, content, functions)
    }
    #[cfg(not(feature = "tree-sitter"))]
    {
        let _ = (path, functions);
        Ok(scan_text(content))
    }
}

#[cfg(not(feature = "tree-sitter"))]
fn scan_text(content: &str) -> References {
    let literals: BTreeSet<String> = content
        .split(|c| !super::is_key_char(c))
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect();
    let mut references = References {
        identifiers: literals.clone(),
        literals,
        dynamic: Vec::new(),
    };
    for (pos, _) in content.match_indices("${") {
        let Some(before) = content.get(..pos) else {
            continue;
        };
        let prefix: String = before
            .chars()
            .rev()
            .take_while(|c| super::is_key_char(*c))
            .collect();
        let prefix: String = prefix.chars().rev().collect();
        if prefix.contains('.') {
            references.dynamic.push(DynamicReference {
                span: pos - prefix.len()..pos + 2,
                prefix: Some(prefix),
            });
        }
    }
    references
}
