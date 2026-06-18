use crate::options::{FormatOptions, SortOrder};
use color_eyre::eyre::{self, WrapErr};
use std::cmp::Ordering;
use std::path::PathBuf;
use toml_edit::{Item, Table};

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
/// Only header tables are reordered; inline values such as `arguments = { .. }`
/// are left untouched. Reordering can shuffle the blank lines that separate
/// header blocks (in format-preserving TOML, the blank line before a header
/// belongs to that header), so block separation is renormalized afterwards.
fn format_str(input: &str, order: SortOrder) -> eyre::Result<String> {
    let mut doc: toml_edit::DocumentMut = input.parse()?;
    let mut state = SortState {
        input,
        order,
        // the renderer emits header tables in `position` order, so positions
        // are reassigned to match the new key order.
        position: 0,
        first_header: true,
    };
    sort_table(doc.as_table_mut(), &mut state);
    Ok(doc.to_string())
}

struct SortState<'a> {
    input: &'a str,
    order: SortOrder,
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
    for (_key, item) in table.iter_mut() {
        sort_item(item, state);
    }
}

fn sort_item(item: &mut Item, state: &mut SortState<'_>) {
    match item {
        Item::Table(table) => sort_header(table, state),
        Item::ArrayOfTables(tables) => {
            for table in tables.iter_mut() {
                sort_header(table, state);
            }
        }
        _ => {}
    }
}

fn sort_header(table: &mut Table, state: &mut SortState<'_>) {
    table.set_position(Some(state.position));
    state.position += 1;

    // implicit (e.g. the `a` in `[a.b]`) and dotted tables do not render a
    // header line, so their leading whitespace is not what separates blocks.
    if !table.is_implicit() && !table.is_dotted() {
        let comments = header_comments(table, state.input);
        let prefix = if state.first_header {
            comments
        } else {
            format!("\n{comments}")
        };
        table.decor_mut().set_prefix(prefix);
        state.first_header = false;
    }

    sort_table(table, state);
}

/// The comment lines preceding a header, with leading blank lines stripped.
fn header_comments(table: &Table, input: &str) -> String {
    let Some(raw) = table.decor().prefix() else {
        return String::new();
    };
    let prefix = raw
        .as_str()
        .or_else(|| raw.span().and_then(|span| input.get(span)))
        .unwrap_or("");
    prefix.trim_start_matches(char::is_whitespace).to_string()
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
}
