//! Template analysis for linting.
//!
//! A lint needs to know what a template does with each name it references:
//! whether the value reaches the output, or whether it only selects a wording.
//! That distinction decides which names every language of a key must share.
//!
//! The analysis result is engine-independent; producing it is not.
//! Each supported engine has a submodule that walks that engine's own syntax
//! tree, and [`crate::template::Analyzer::for_engine`] is the one place that maps a
//! [`TemplateEngine`] to it.
//! Adding an engine means adding a submodule and one match arm there.

mod handlebars;

use crate::TemplateEngine;
use std::collections::BTreeSet;

/// How a template uses the names it references.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Analysis {
    /// Names whose values reach the output: interpolated, passed to a helper,
    /// or iterated.
    /// Every language of a key must reference these.
    pub substituted: BTreeSet<String>,
    /// Names that only select a wording, such as the parameters of a
    /// conditional block.
    /// A language without the distinction may leave them out.
    pub conditions: BTreeSet<String>,
    /// Conditions whose two branches are identical and therefore have no
    /// effect, as written in the template (`{{#if my_condition}}`).
    pub dead_conditions: Vec<String>,
}

impl Analysis {
    /// Every name the template references, whichever way it uses it.
    #[must_use]
    pub fn variables(&self) -> BTreeSet<String> {
        self.substituted.union(&self.conditions).cloned().collect()
    }
}

/// A template that its engine refuses to compile.
///
/// The message is the engine's own one-line reason, suitable as a diagnostic
/// label; it never includes a source excerpt.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct CompileError {
    /// The engine's reason for rejecting the template.
    pub message: String,
}

/// The analysis function of one template engine.
///
/// Resolve it once from the configured engine; a `None` from
/// [`Analyzer::for_engine`] means the engine has no analysis support, and
/// callers skip template checks entirely instead of asking per template.
#[derive(Clone, Copy)]
pub struct Analyzer(fn(&str) -> Result<Analysis, CompileError>);

impl Analyzer {
    /// The analyzer for `engine`, if it has one.
    ///
    /// The match is exhaustive so that a new [`TemplateEngine`] variant forces
    /// a decision here.
    #[must_use]
    pub fn for_engine(engine: &TemplateEngine) -> Option<Self> {
        match engine {
            TemplateEngine::Handlebars => Some(Self(handlebars::analyze)),
            TemplateEngine::Golang
            | TemplateEngine::Mustache
            | TemplateEngine::Jinja2
            | TemplateEngine::Other(_) => None,
        }
    }

    /// Analyzes `source` as a template of this engine.
    ///
    /// # Errors
    ///
    /// Returns a [`CompileError`] if the engine rejects `source`.
    pub fn analyze(self, source: &str) -> Result<Analysis, CompileError> {
        (self.0)(source)
    }
}

#[cfg(test)]
mod tests {
    use super::Analyzer;
    use crate::TemplateEngine;

    /// Only Handlebars has analysis support; other engines are not guessed at.
    #[test_util::test]
    fn resolves_by_engine() {
        let analyzer = Analyzer::for_engine(&TemplateEngine::Handlebars);
        assert!(analyzer.is_some_and(|analyzer| analyzer.analyze("{{name}}").is_ok()));
        for engine in [
            TemplateEngine::Jinja2,
            TemplateEngine::Golang,
            TemplateEngine::Mustache,
            TemplateEngine::Other("tera".to_string()),
        ] {
            assert!(Analyzer::for_engine(&engine).is_none(), "{engine:?}");
        }
    }
}
