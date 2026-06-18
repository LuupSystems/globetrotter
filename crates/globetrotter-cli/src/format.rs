use crate::options::{FormatOptions, SortOrder};
use color_eyre::eyre::{self, WrapErr};
use std::cmp::Ordering;
use std::path::PathBuf;
use toml_edit::{ArrayOfTables, Decor, Item, KeyMut, Table};

impl crate::Globetrotter {
    /// Format translation files in place by sorting their keys.
    ///
    /// Translation files are discovered from the loaded configurations and from
    /// any paths passed via `--translation`. Comments and formatting are
    /// preserved; only the order of keys changes.
    ///
    /// # Errors
    ///
    /// Returns an error if a translation file cannot be read, parsed as TOML, or
    /// written back, or — in `--check` mode — if any file is not already
    /// formatted.
    pub async fn format(self, options: &FormatOptions) -> eyre::Result<()> {
        let strict = self.options.strict.unwrap_or(false);

        let mut diagnostics = Vec::new();
        let files =
            globetrotter::executor::resolve_input_files(&self.configs, strict, &mut diagnostics);
        for diagnostic in &diagnostics {
            self.diagnostic_printer.emit(diagnostic).await?;
        }

        // canonicalize both config-derived and explicitly passed paths so the
        // same file referenced through different patterns is only formatted once.
        let mut paths = Vec::with_capacity(files.len() + self.options.translations.len());
        for path in files.iter().chain(&self.options.translations) {
            let path = tokio::fs::canonicalize(path)
                .await
                .wrap_err_with(|| eyre::eyre!("failed to open: {path:?}"))?;
            paths.push(path);
        }
        paths.sort();
        paths.dedup();

        if paths.is_empty() {
            eyre::bail!(
                "no translation files found to format; pass --translation <FILE> or --config <FILE>"
            );
        }

        let mut unformatted: Vec<PathBuf> = Vec::new();
        for path in &paths {
            let original = tokio::fs::read_to_string(path)
                .await
                .wrap_err_with(|| eyre::eyre!("failed to read: {path:?}"))?;
            let formatted = format_str(&original, options.order)
                .wrap_err_with(|| eyre::eyre!("failed to format: {path:?}"))?;

            if formatted == original {
                continue;
            }

            unformatted.push(path.clone());
            if options.check {
                tracing::warn!(path = %path.display(), "not formatted");
            } else {
                tokio::fs::write(path, &formatted)
                    .await
                    .wrap_err_with(|| eyre::eyre!("failed to write: {path:?}"))?;
                tracing::info!(path = %path.display(), "formatted");
            }
        }

        if options.check && !unformatted.is_empty() {
            eyre::bail!(
                "{} translation file(s) are not formatted",
                unformatted.len()
            );
        }

        Ok(())
    }
}

/// Sort the keys of a TOML document, preserving comments and formatting.
///
/// Header tables (translation-key sections) and the language keys inside them
/// are reordered; inline values such as `arguments = { .. }` are left untouched.
/// A comment stays attached to the section or language key it sits above and
/// moves with it (stacked comments included). Blank lines between language keys
/// are removed and sections are separated by exactly one blank line, but a
/// single blank line between two comment paragraphs is preserved, so the result
/// is clean and idempotent.
fn format_str(input: &str, order: SortOrder) -> eyre::Result<String> {
    let mut doc: toml_edit::DocumentMut = input.parse()?;
    let mut state = SortState {
        input,
        order,
        position: 0,
        first_header: true,
    };
    sort_table(doc.as_table_mut(), &mut state);
    Ok(doc.to_string())
}

struct SortState<'a> {
    input: &'a str,
    order: SortOrder,
    // the renderer emits header tables in `position` order, so positions are
    // reassigned to match the new key order.
    position: isize,
    first_header: bool,
}

fn key_cmp(a: &str, b: &str, order: SortOrder) -> Ordering {
    // the `arguments` table holds a translation's metadata rather than a nested
    // translation key, so it is always kept last regardless of sort order.
    let pinned = |key: &str| key == "arguments" || key == "args";
    match (pinned(a), pinned(b)) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => {
            let ordering = a.cmp(b);
            match order {
                SortOrder::Ascending => ordering,
                SortOrder::Descending => ordering.reverse(),
            }
        }
    }
}

fn sort_table(table: &mut Table, state: &mut SortState<'_>) {
    let order = state.order;
    table.sort_values_by(|a, _, b, _| key_cmp(a.get(), b.get(), order));
    for (mut key, item) in table.iter_mut() {
        match item {
            Item::Table(child) => {
                child.set_position(Some(state.position));
                state.position += 1;
                // implicit (e.g. the `a` in `[a.b]`) and dotted tables do not
                // render a header line, so they carry no block separation.
                if !child.is_implicit() && !child.is_dotted() {
                    normalize_header(&mut key, child, state);
                }
                sort_table(child, state);
            }
            Item::ArrayOfTables(children) => {
                normalize_array_of_tables(&mut key, children, state);
            }
            Item::Value(_) => {
                // a comment above a language key lives in the key's leaf decor;
                // keep it (sorted with the key) but drop blank lines between
                // language keys.
                let comments = comments_of(key.leaf_decor(), state.input);
                key.leaf_decor_mut().set_prefix(comments);
            }
            Item::None => {}
        }
    }
}

fn normalize_header(key: &mut KeyMut<'_>, table: &mut Table, state: &mut SortState<'_>) {
    // a section's leading comment may sit in the key's leaf decor (rendered
    // before the `[` of a dotted header) and/or in the table's own decor.
    let mut comments = comments_of(key.leaf_decor(), state.input);
    comments.push_str(&comments_of(table.decor(), state.input));
    key.leaf_decor_mut().set_prefix("");
    table
        .decor_mut()
        .set_prefix(block_separator(&comments, state));
}

fn normalize_array_of_tables(
    key: &mut KeyMut<'_>,
    tables: &mut ArrayOfTables,
    state: &mut SortState<'_>,
) {
    let lead = comments_of(key.leaf_decor(), state.input);
    key.leaf_decor_mut().set_prefix("");
    for (index, table) in tables.iter_mut().enumerate() {
        table.set_position(Some(state.position));
        state.position += 1;
        let mut comments = comments_of(table.decor(), state.input);
        if index == 0 {
            comments = format!("{lead}{comments}");
        }
        table
            .decor_mut()
            .set_prefix(block_separator(&comments, state));
        sort_table(table, state);
    }
}

/// The decor prefix for a header: its comments, preceded by one blank line
/// unless it is the first header in the document.
fn block_separator(comments: &str, state: &mut SortState<'_>) -> String {
    let prefix = if state.first_header {
        comments.to_string()
    } else {
        format!("\n{comments}")
    };
    state.first_header = false;
    prefix
}

fn comments_of(decor: &Decor, input: &str) -> String {
    let raw = decor
        .prefix()
        .and_then(|prefix| {
            prefix
                .as_str()
                .or_else(|| prefix.span().and_then(|span| input.get(span)))
        })
        .unwrap_or("");
    comment_lines(raw)
}

/// Collect the comment lines from a decor prefix so they sit directly above the
/// key or header they annotate. Leading and trailing blank lines are dropped,
/// but a single blank line between two comment paragraphs is preserved (runs of
/// several blank lines collapse to one) — e.g. a file-level comment separated
/// from the first key's comment. The encoder emits a newline after every
/// key/header, so no leading newline is needed.
fn comment_lines(prefix: &str) -> String {
    let mut out = String::new();
    let mut pending_blank = false;
    for line in prefix.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            // remember an interior blank, but only once a comment precedes it;
            // trailing blanks are never flushed and so are dropped.
            pending_blank = !out.is_empty();
        } else {
            if pending_blank {
                out.push('\n');
                pending_blank = false;
            }
            out.push_str(trimmed);
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{SortOrder, format_str};
    use indoc::indoc;
    use similar_asserts::assert_eq as sim_assert_eq;

    #[test]
    fn sorts_keys_and_preserves_comments() {
        crate::tests::init();

        let input = indoc! {r#"
            # leading comment
            [b.title]
            en = "B"
            de = "B (de)"

            # comment attached to a.title
            [a.title]
            fr = "A (fr)"
            en = "A"
            arguments = { name = "string" }
        "#};

        let want = indoc! {r#"
            # comment attached to a.title
            [a.title]
            en = "A"
            fr = "A (fr)"
            arguments = { name = "string" }

            # leading comment
            [b.title]
            de = "B (de)"
            en = "B"
        "#};

        let have = format_str(input, SortOrder::Ascending).unwrap();
        sim_assert_eq!(have: have, want: want);
    }

    #[test]
    fn descending_reverses_key_order_but_keeps_arguments_last() {
        crate::tests::init();

        let input = indoc! {r#"
            [a]
            arguments = { name = "string" }
            en = "A"

            [b]
            en = "B"
        "#};

        let want = indoc! {r#"
            [b]
            en = "B"

            [a]
            en = "A"
            arguments = { name = "string" }
        "#};

        let have = format_str(input, SortOrder::Descending).unwrap();
        sim_assert_eq!(have: have, want: want);
    }

    #[test]
    fn already_sorted_is_unchanged() {
        crate::tests::init();

        let input = indoc! {r#"
            [a]
            en = "A"

            [b]
            en = "B"
        "#};

        let have = format_str(input, SortOrder::Ascending).unwrap();
        sim_assert_eq!(have: have, want: input);
    }

    #[test]
    fn section_comments_move_with_their_section() {
        crate::tests::init();

        // a stack of comment lines sits on top of each section header; they must
        // travel with the section when the sections are reordered.
        let input = indoc! {r#"
            # top comment for b
            # second line for b
            [b.title]
            en = "B"

            # comment for a
            [a.title]
            en = "A"
        "#};

        let want = indoc! {r#"
            # comment for a
            [a.title]
            en = "A"

            # top comment for b
            # second line for b
            [b.title]
            en = "B"
        "#};

        let have = format_str(input, SortOrder::Ascending).unwrap();
        sim_assert_eq!(have: have, want: want);
    }

    #[test]
    fn comment_above_a_language_key_moves_with_it() {
        crate::tests::init();

        let input = indoc! {r#"
            [greeting]
            # note about en
            en = "E"
            de = "D"
        "#};

        let want = indoc! {r#"
            [greeting]
            de = "D"
            # note about en
            en = "E"
        "#};

        let have = format_str(input, SortOrder::Ascending).unwrap();
        sim_assert_eq!(have: have, want: want);
    }

    #[test]
    fn removes_blank_lines_between_language_keys() {
        crate::tests::init();

        let input = indoc! {r#"
            [greeting]
            de = "D"

            en = "E"


            fr = "F"
        "#};

        let want = indoc! {r#"
            [greeting]
            de = "D"
            en = "E"
            fr = "F"
        "#};

        let have = format_str(input, SortOrder::Ascending).unwrap();
        sim_assert_eq!(have: have, want: want);
    }

    #[test]
    fn normalizes_blank_lines_between_sections_to_one() {
        crate::tests::init();

        let input = indoc! {r#"
            [a]
            en = "A"



            [b]
            en = "B"
        "#};

        let want = indoc! {r#"
            [a]
            en = "A"

            [b]
            en = "B"
        "#};

        let have = format_str(input, SortOrder::Ascending).unwrap();
        sim_assert_eq!(have: have, want: want);
    }

    #[test]
    fn real_world_translation_file() {
        crate::tests::init();

        // mixed, unsorted real-world input: section comments, a comment on a
        // language key, blank lines between languages, oversized gaps between
        // sections, and an `arguments` table that must stay last.
        let input = indoc! {r#"
            # Upload-related strings.
            [upload.title]
            en = "Upload"
            de = "Hochladen"


            [upload.message]
            # en is the source language
            en = "{count} files"

            de = "{count} Dateien"
            arguments = { count = "number" }

            [account]
            en = "Account"
            de = "Konto"
        "#};

        let want = indoc! {r#"
            [account]
            de = "Konto"
            en = "Account"

            [upload.message]
            de = "{count} Dateien"
            # en is the source language
            en = "{count} files"
            arguments = { count = "number" }

            # Upload-related strings.
            [upload.title]
            de = "Hochladen"
            en = "Upload"
        "#};

        let have = format_str(input, SortOrder::Ascending).unwrap();
        sim_assert_eq!(have: have, want: want);

        // formatting is idempotent.
        let again = format_str(&have, SortOrder::Ascending).unwrap();
        sim_assert_eq!(have: again, want: want);
    }

    #[test]
    fn preserves_blank_line_between_comment_paragraphs() {
        crate::tests::init();

        // the common top-of-file pattern: a file-level comment, a blank line,
        // then the first key's own comment. The blank must survive formatting
        // (and the sort that reorders the languages below).
        let input = indoc! {r#"
            # This file contains greeting strings.

            # Shown to the user on login.
            [greeting]
            en = "Hello"
            de = "Hallo"

            [zzz_other]
            en = "x"
        "#};

        let want = indoc! {r#"
            # This file contains greeting strings.

            # Shown to the user on login.
            [greeting]
            de = "Hallo"
            en = "Hello"

            [zzz_other]
            en = "x"
        "#};

        let have = format_str(input, SortOrder::Ascending).unwrap();
        sim_assert_eq!(have: have, want: want);

        let again = format_str(&have, SortOrder::Ascending).unwrap();
        sim_assert_eq!(have: again, want: want);
    }

    #[test]
    fn collapses_multiple_blank_lines_in_a_comment_block() {
        crate::tests::init();

        let input = indoc! {r#"
            # paragraph one



            # paragraph two
            [a]
            en = "A"
        "#};

        let want = indoc! {r#"
            # paragraph one

            # paragraph two
            [a]
            en = "A"
        "#};

        let have = format_str(input, SortOrder::Ascending).unwrap();
        sim_assert_eq!(have: have, want: want);
    }

    #[test]
    fn multiline_strings_are_untouched() {
        crate::tests::init();

        // keys around a multi-line string value (which itself contains lines
        // that look like blanks) must sort without corrupting the value.
        let input = indoc! {r#"
            [b]
            en = "B"

            [a]
            en = """
            line one

            line three
            """
            de = "A"
        "#};

        let want = indoc! {r#"
            [a]
            de = "A"
            en = """
            line one

            line three
            """

            [b]
            en = "B"
        "#};

        let have = format_str(input, SortOrder::Ascending).unwrap();
        sim_assert_eq!(have: have, want: want);
    }
}
