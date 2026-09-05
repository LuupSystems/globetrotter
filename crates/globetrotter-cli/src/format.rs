//! In-place TOML formatting that preserves comment ownership.

use crate::options::{FormatOptions, SortOrder};
use color_eyre::eyre::{self, ContextCompat, WrapErr};
use std::cmp::Ordering;
use std::path::PathBuf;
use toml_edit::{ArrayOfTables, Decor, Item, KeyMut, Table};
use toml_parser::{Source, lexer::TokenKind};

impl crate::Globetrotter {
    /// Formats translation files in place by sorting their keys.
    ///
    /// Translation files are discovered from the loaded configurations and from
    /// any paths passed via `--translation`.
    /// Ordinary comments move with their keys; `#!` comments are collected at
    /// the top of the file in source order, followed by one blank line.
    /// Values are preserved and spacing between keys and sections is normalized.
    ///
    /// # Errors
    ///
    /// Returns an error if a translation file cannot be read, parsed as TOML, or
    /// written back, or — in `--check` mode — if any file is not already
    /// formatted.
    pub async fn format(self, options: &FormatOptions) -> eyre::Result<()> {
        let strict = self.options.strict.unwrap_or(false);

        // Resolve configured translation files before adding explicit paths.
        let mut diagnostics = Vec::new();
        let files =
            globetrotter::executor::resolve_input_files(&self.configs, strict, &mut diagnostics);
        for diagnostic in &diagnostics {
            self.diagnostic_printer.emit(diagnostic).await?;
        }

        // Canonicalize all sources so aliases and overlapping globs cannot
        // format the same file twice.
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

        // Format or verify every unique translation file.
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

        // Check mode reports every unformatted file before failing.
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
/// are reordered; inline values such as `arguments = { .. }` are not reordered.
/// A `#!` comment belongs to the file and is emitted before all keys and headers.
/// An ordinary comment stays attached to the section or language key it sits above
/// and moves with it (stacked comments included).
/// Blank lines between language keys are removed and sections are separated by
/// exactly one blank line, but a single blank line between two comment paragraphs
/// is preserved, so the result is clean and idempotent.
fn format_str(input: &str, order: SortOrder) -> eyre::Result<String> {
    // Validate the original source before extracting comments so invalid input
    // still reports errors at its original locations.
    let mut doc: toml_edit::DocumentMut = input.parse()?;
    let (file_comments, body) = extract_file_comments(input)?;
    if !file_comments.is_empty() {
        doc = body.parse()?;
    }
    let mut state = SortState {
        input: if file_comments.is_empty() {
            input
        } else {
            &body
        },
        order,
        position: 0,
        first_header: true,
    };
    sort_table(doc.as_table_mut(), &mut state);
    let formatted = doc.to_string();
    if file_comments.is_empty() {
        Ok(formatted)
    } else {
        // Document trailing decor can contain blank lines even without keys.
        // The file header owns the spacing before the rest of the document.
        Ok(format!(
            indoc::indoc! {r"
                {file_comments}
                {}"},
            formatted.trim_start_matches([' ', '\t', '\r', '\n']),
            file_comments = file_comments,
        ))
    }
}

/// Separates file comments from the source without inspecting string contents.
///
/// The lexer visits comments in source order, including those inside arrays and
/// after values or headers, which a traversal of sorted tables cannot guarantee.
/// Standalone lines retain their indentation; inline comments start at `#!`.
/// Comment text is copied verbatim and line endings are normalized to LF.
fn extract_file_comments(input: &str) -> eyre::Result<(String, String)> {
    let mut comments = String::new();
    let mut body = String::new();
    let mut remaining = input;
    for token in Source::new(input).lex() {
        if token.kind() != TokenKind::Comment {
            continue;
        }
        let offset = token.span().start() - (input.len() - remaining.len());
        let (before, tail) = remaining
            .split_at_checked(offset)
            .wrap_err("TOML comment starts outside the remaining source")?;
        let (comment, after) = tail
            .split_at_checked(token.span().len())
            .wrap_err("TOML comment ends outside the remaining source")?;
        if !comment.starts_with("#!") {
            continue;
        }

        let before_content = before.trim_end_matches([' ', '\t']);
        let standalone = before_content.is_empty() || before_content.ends_with('\n');
        if standalone {
            let indentation = before
                .get(before_content.len()..)
                .wrap_err("TOML comment indentation is outside the source")?;
            comments.push_str(indentation);
        }
        comments.push_str(comment);
        comments.push('\n');
        body.push_str(before_content);

        remaining = if standalone {
            after
                .strip_prefix("\r\n")
                .or_else(|| after.strip_prefix('\n'))
                .unwrap_or(after)
        } else {
            after
        };
    }
    if !comments.is_empty() {
        body.push_str(remaining);
    }
    Ok((comments, body))
}

struct SortState<'a> {
    input: &'a str,
    order: SortOrder,
    // `toml_edit` renders table headers by position, so sorting keys also
    // requires assigning new positions.
    position: isize,
    first_header: bool,
}

fn key_cmp(a: &str, b: &str, order: SortOrder) -> Ordering {
    // `arguments` and its alias `args` are metadata, not nested translation
    // keys, so they remain last in either sort direction.
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
                // Implicit (e.g. the `a` in `[a.b]`) and dotted tables do not
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
                // A comment above a language key lives in the key's leaf decor;
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
    // A section's leading comment may sit in the key's leaf decor (rendered
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

/// Builds a header's decor prefix.
///
/// Comments are preceded by one blank line unless this is the first rendered
/// header.
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

/// Collects comment lines from decor so they stay directly above the
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
            // Remember an interior blank only after a comment; leaving it
            // pending also drops trailing whitespace.
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

    #[test_util::test]
    fn file_header_stays_above_sorted_tables() {
        let input = indoc! {r#"
            #! Advisor-facing document groups.
            #! Keep them filesystem-friendly.

            [property]
            en = "Property Documents"

            [applicant]
            en = "Applicant Documents"
        "#};
        let want = indoc! {r#"
            #! Advisor-facing document groups.
            #! Keep them filesystem-friendly.

            [applicant]
            en = "Applicant Documents"

            [property]
            en = "Property Documents"
        "#};

        let have = format_str(input, SortOrder::Ascending)?;
        sim_assert_eq!(have: have, want: want);
        sim_assert_eq!(have: format_str(&have, SortOrder::Ascending)?, want: want);

        // Adding an earlier key must not take ownership of the file header.
        let extended = format!(
            indoc::indoc! {r#"
            {have}
            [account]
            en = "Account"
            "#},
            have = have
        );
        for order in [SortOrder::Ascending, SortOrder::Descending] {
            let formatted = format_str(&extended, order)?;
            assert!(formatted.starts_with(indoc::indoc! {r"
                    #! Advisor-facing document groups.
                    #! Keep them filesystem-friendly.

                    ["}));
            sim_assert_eq!(have: format_str(&formatted, order)?, want: formatted);
        }
    }

    #[test_util::test]
    fn collects_file_comments_in_source_order_from_every_position() {
        // Physical header order differs from tree order, and comments can live
        // in value decor, container trailing decor, or document trailing decor.
        let input = indoc! {r#"
            #! First.
            [z.last] #! Header suffix.
            en = "Z" #! Value suffix.
            #! Before a language.
            de = "Z (de)"
            arguments = [
                #! Inside an array.
                "name", #! After an array element.
                #! Array trailing comment.
            ]

            [a]
            en = "A"

            #! Before an array of tables.
            [[items]]
            en = "Item"

            #! Last.
        "#};
        let header = indoc! {"
            #! First.
            #! Header suffix.
            #! Value suffix.
            #! Before a language.
                #! Inside an array.
            #! After an array element.
                #! Array trailing comment.
            #! Before an array of tables.
            #! Last.

        "};

        for order in [SortOrder::Ascending, SortOrder::Descending] {
            let have = format_str(input, order)?;
            assert!(have.starts_with(header), "{have}");
            assert!(
                !have
                    .strip_prefix(header)
                    .ok_or_else(|| color_eyre::eyre::eyre!("missing file header"))?
                    .contains("#!")
            );
            sim_assert_eq!(have: format_str(&have, order)?, want: have);
        }
    }

    #[test_util::test]
    fn file_comments_leave_ordinary_comments_with_their_keys() {
        let input = indoc! {r#"
            # About b.
            #! File header.
            # More about b.
            [b]
            # About English.
            #! Another file paragraph.
            en = "B" # Inline note.
            de = "B (de)"

            # About a.
            [a]
            en = "A"
            # Ordinary footer.
        "#};
        let want = indoc! {r#"
            #! File header.
            #! Another file paragraph.

            # About a.
            [a]
            en = "A"

            # About b.
            # More about b.
            [b]
            de = "B (de)"
            # About English.
            en = "B" # Inline note.
            # Ordinary footer.
        "#};

        let have = format_str(input, SortOrder::Ascending)?;
        sim_assert_eq!(have: have, want: want);
        sim_assert_eq!(have: format_str(&have, SortOrder::Ascending)?, want: want);
    }

    #[test_util::test]
    fn file_comment_text_and_paragraph_breaks_are_verbatim() {
        // Explicit escapes keep significant spaces, tabs, and CRLF visible.
        let input = indoc::indoc! {"[a]\r\nen = \"A\"\r\n  #!  Grüße!  \t\r\n#!\r\n\t#!\tSecond paragraph.  "};
        assert!(input.contains("\r\n"));
        let want =
            indoc::indoc! {"  #!  Grüße!  \t\n#!\n\t#!\tSecond paragraph.  \n\n[a]\nen = \"A\"\n"};
        let have = format_str(input, SortOrder::Ascending)?;
        sim_assert_eq!(have: have, want: want);
        sim_assert_eq!(have: format_str(&have, SortOrder::Ascending)?, want: want);
    }

    #[test_util::test]
    fn file_comments_work_without_table_headers() {
        for (input, want) in [
            (
                "#! Header.",
                indoc::indoc! {r"
                #! Header.

                "},
            ),
            (
                concat!("\n", indoc::indoc! {"#!\n\n\n"}),
                indoc::indoc! {r"
                #!

                "},
            ),
            (
                indoc::indoc! {r"
                    #! Header.
                    # Ordinary footer.
                    "},
                indoc::indoc! {r"
                    #! Header.

                    # Ordinary footer.
                    "},
            ),
            (
                indoc::indoc! {r"
                z = 1
                #! Header.
                a = 2
                "},
                indoc::indoc! {r"
                #! Header.

                a = 2
                z = 1
                "},
            ),
        ] {
            let have = format_str(input, SortOrder::Ascending)?;
            sim_assert_eq!(have: have, want: want);
            sim_assert_eq!(have: format_str(&have, SortOrder::Ascending)?, want: want);
        }
    }

    #[test_util::test]
    fn file_comment_markers_in_strings_are_untouched() {
        let input = indoc! {r##"
            #! Actual file header.

            ["#! key"]
            basic = "Escaped quote: \" #! still a string"
            inline = { "#! key" = "#! value" }
            literal = '#! literal string'
            multiline_basic = """
            #! Translation text.
            Escaped delimiter: \"""
            #! Still translation text.
            """
            multiline_literal = '''
            #! Literal translation text.
            '''
            # #! Ordinary comment.
            ordinary = "Value" # #! Ordinary inline comment.
        "##};
        let have = format_str(input, SortOrder::Ascending)?;
        sim_assert_eq!(have: have, want: input);
    }

    #[test_util::test]
    fn file_comments_do_not_hide_invalid_toml() {
        for input in [
            indoc::indoc! {r#"
                #! Header.
                [a
                en = "A"
                "#},
            indoc::indoc! {"#! Invalid control character: \u{0001}\n[a]\nen = \"A\"\n"},
            indoc::indoc! {r#"
                [a]
                en = """
                #! Unterminated translation.
                "#},
        ] {
            assert!(format_str(input, SortOrder::Ascending).is_err());
        }
    }

    #[test_util::test]
    fn sorts_keys_and_preserves_comments() {
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

        let have = format_str(input, SortOrder::Ascending)?;
        sim_assert_eq!(have: have, want: want);
    }

    #[test_util::test]
    fn descending_reverses_key_order_but_keeps_arguments_last() {
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

        let have = format_str(input, SortOrder::Descending)?;
        sim_assert_eq!(have: have, want: want);
    }

    #[test_util::test]
    fn already_sorted_is_unchanged() {
        let input = indoc! {r#"
            [a]
            en = "A"

            [b]
            en = "B"
        "#};

        let have = format_str(input, SortOrder::Ascending)?;
        sim_assert_eq!(have: have, want: input);
    }

    #[test_util::test]
    fn section_comments_move_with_their_section() {
        // A stack of comment lines sits on top of each section header; they must
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

        let have = format_str(input, SortOrder::Ascending)?;
        sim_assert_eq!(have: have, want: want);
    }

    #[test_util::test]
    fn comment_above_a_language_key_moves_with_it() {
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

        let have = format_str(input, SortOrder::Ascending)?;
        sim_assert_eq!(have: have, want: want);
    }

    #[test_util::test]
    fn removes_blank_lines_between_language_keys() {
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

        let have = format_str(input, SortOrder::Ascending)?;
        sim_assert_eq!(have: have, want: want);
    }

    #[test_util::test]
    fn normalizes_blank_lines_between_sections_to_one() {
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

        let have = format_str(input, SortOrder::Ascending)?;
        sim_assert_eq!(have: have, want: want);
    }

    #[test_util::test]
    fn real_world_translation_file() {
        // Mixed, unsorted real-world input covers section and language
        // comments, blank language gaps, oversized section gaps, and argument
        // metadata that must remain last.
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

        let have = format_str(input, SortOrder::Ascending)?;
        sim_assert_eq!(have: have, want: want);

        // A second pass must not introduce additional changes.
        let again = format_str(&have, SortOrder::Ascending)?;
        sim_assert_eq!(have: again, want: want);
    }

    #[test_util::test]
    fn preserves_blank_line_between_comment_paragraphs() {
        // Keep the file-level comment and its separating blank distinct from
        // the first key's comment while languages are reordered.
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

        let have = format_str(input, SortOrder::Ascending)?;
        sim_assert_eq!(have: have, want: want);

        let again = format_str(&have, SortOrder::Ascending)?;
        sim_assert_eq!(have: again, want: want);
    }

    #[test_util::test]
    fn collapses_multiple_blank_lines_in_a_comment_block() {
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

        let have = format_str(input, SortOrder::Ascending)?;
        sim_assert_eq!(have: have, want: want);
    }

    #[test_util::test]
    fn multiline_strings_are_untouched() {
        // Sort keys around a multiline value without treating its blank-looking
        // lines as TOML decor.
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

        let have = format_str(input, SortOrder::Ascending)?;
        sim_assert_eq!(have: have, want: want);
    }
}
