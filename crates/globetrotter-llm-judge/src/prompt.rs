//! The judge prompt: a strict "meaning only" template and its rendering.
//!
//! The template was tuned on a real-world corpus: naive "find inconsistencies"
//! prompts flood the output with style, register, and loanword nitpicks, so the
//! load-bearing part is the explicit NEVER-report list. Prompt behavior varies
//! sharply *per model* (the same wording can discipline one model and give
//! another license to over-flag), which is why the template is overridable
//! rather than baked in.

use crate::KeyInput;

/// The default judge prompt template.
///
/// Placeholders: `{key}` is replaced with the dotted key path, `{languages}`
/// with one `code: text` line per language. All other braces are passed through
/// verbatim, so JSON examples and `{{placeholder}}` samples need no escaping.
pub const DEFAULT_TEMPLATE: &str = "\
These are the translations of ONE string of an application, shown to users in \
their own language:

Translation key: {key}

{languages}

Report a language ONLY if its users are told a genuinely different fact or \
action than users of the other languages — you must be able to complete the \
sentence \"users of <lang> are told <A>, everyone else is told <B>\" where A \
and B would lead a user to do or believe something different. Examples of real \
issues: an opposite or negated meaning, a different action (save vs discard), \
a different object (bank statement vs rental contract), a different quantity, \
timeframe, or unit, or text that clearly belongs to a completely different UI \
string.

Everything below is normal translation practice. NEVER report:
- restructured sentences: noun phrase vs verb phrase, active vs passive, \
different word order, added or dropped minor words — same content, different \
shape
- each market's standard terminology or abbreviations for the same concept, \
even when they look nothing like the source term
- spelling, grammar, accents, or word forms — assume every text is written \
correctly in its own language; you are not a proofreader
- formality or register (du/Sie, tu/vous), tone, idioms, more or less literal \
phrasing
- English loanwords kept in the target language, or translated vs untranslated \
product terms
- locale-adapted examples and placeholders: phone formats, example email \
domains, currencies

If in doubt, or your concern is about HOW something is phrased rather than \
WHAT the user is told, answer consistent: true.

Answer with JSON: {\"consistent\": true/false, \"issues\": [{\"language\": \
\"xx\", \"problem\": \"users of xx are told ... while the others say ...\", \
\"confidence\": 0.0-1.0}]} — issues empty when consistent. confidence is how \
certain you are that the difference changes what the user is told: 1.0 only \
for unmistakable contradictions, lower when the texts could still mean the \
same thing.";

/// Renders `template` for one key, substituting the `{key}` and `{languages}`
/// placeholders.
///
/// Substitution is plain string replacement — not `format!` — so every other
/// brace in the template (JSON examples, `{{count}}`-style samples in the
/// translations themselves) passes through untouched.
#[must_use]
pub fn render(template: &str, key: &KeyInput<'_>) -> String {
    let languages = key
        .languages
        .iter()
        .map(|language| format!("{}: {}", language.language, language.text))
        .collect::<Vec<_>>()
        .join("\n");
    template
        .replace("{key}", key.key)
        .replace("{languages}", &languages)
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_TEMPLATE, render};
    use crate::{KeyInput, LanguageText};

    #[test_util::test]
    fn renders_key_and_languages() {
        let key = KeyInput {
            key: "actions.save",
            languages: vec![
                LanguageText {
                    language: "en",
                    text: "Save {{count}} files",
                },
                LanguageText {
                    language: "de",
                    text: "{{count}} Dateien speichern",
                },
            ],
        };
        let rendered = render(DEFAULT_TEMPLATE, &key);
        assert!(rendered.contains("Translation key: actions.save"));
        assert!(rendered.contains("en: Save {{count}} files"));
        // Translation placeholders and the JSON example survive verbatim.
        assert!(rendered.contains("de: {{count}} Dateien speichern"));
        assert!(rendered.contains(r#"{"consistent": true/false"#));
    }
}
