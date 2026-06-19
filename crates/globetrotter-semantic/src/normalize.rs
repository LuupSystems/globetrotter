//! Text normalization applied before word-level matching.
//!
//! Translation strings are full of template placeholders and markup
//! (`{{name}}`, `{count}`, `${x}`, `%s`, `<b>`) that carry no translatable
//! meaning but pollute word-vector and lexicon matching. Stripping them leaves
//! only the words that actually differ between languages, which sharpens the
//! similarity signal and removes a class of false positives.

/// Remove template placeholders and inline markup, then collapse whitespace.
///
/// Handles `{...}` / `{{...}}` (ICU, Fluent, Handlebars), `${...}` (template
/// literals), `<...>` (HTML/XML tags), and `%`-style printf specifiers
/// (`%s`, `%d`, `%1$s`, `%(name)s`). For example
/// `"{passed}/{total} passed"` becomes `"/ passed"` → `"passed"`-only words.
#[must_use]
pub fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '$' if chars.peek() == Some(&'{') => {
                chars.next();
                skip_braced(&mut chars);
                out.push(' ');
            }
            '{' => {
                skip_braced(&mut chars);
                out.push(' ');
            }
            '<' => {
                for next in chars.by_ref() {
                    if next == '>' {
                        break;
                    }
                }
                out.push(' ');
            }
            '%' => {
                skip_printf(&mut chars);
                out.push(' ');
            }
            _ => out.push(c),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Consume characters up to and including the matching `}` (the opening brace
/// has already been consumed), honoring nesting so `{{x}}` is skipped whole.
fn skip_braced(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    let mut depth = 1usize;
    for next in chars.by_ref() {
        match next {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
    }
}

/// Consume a printf-style specifier after a `%` (already consumed): optional
/// argument index / mapping, then the conversion letter. Conservative: a `%`
/// not followed by a specifier (e.g. `50% off`) drops only the `%`.
fn skip_printf(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    if chars.peek() == Some(&'(') {
        for next in chars.by_ref() {
            if next == ')' {
                break;
            }
        }
    }
    while let Some(&next) = chars.peek() {
        if next.is_ascii_digit() || next == '$' || next == '.' || next == '-' {
            chars.next();
        } else {
            break;
        }
    }
    if chars
        .peek()
        .is_some_and(|c| c.is_ascii_alphabetic() || *c == '@')
    {
        chars.next();
    }
}

#[cfg(test)]
mod tests {
    use super::normalize;

    #[test]
    fn strips_braced_placeholders_keeping_words() {
        assert_eq!(normalize("{passed}/{total} passed"), "/ passed");
        assert_eq!(normalize("Hello {{name}}, welcome"), "Hello , welcome");
        assert_eq!(normalize("Upload ${count} files"), "Upload files");
    }

    #[test]
    fn strips_markup_and_printf() {
        assert_eq!(normalize("Click <b>here</b> now"), "Click here now");
        assert_eq!(normalize("Loaded %d of %1$s items"), "Loaded of items");
    }

    #[test]
    fn leaves_plain_text_and_bare_percent() {
        assert_eq!(normalize("Save your changes"), "Save your changes");
        assert_eq!(normalize("50% off"), "50 off");
    }
}
