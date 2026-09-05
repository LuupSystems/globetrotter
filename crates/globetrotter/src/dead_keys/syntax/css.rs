//! Vue CSS bindings provide executable references; ordinary CSS text does not.

use super::{Dialect, References, text, web::parse_expression};
use std::io;
use tree_sitter::{Node, Parser};

pub(super) fn visit_bindings(
    node: Node<'_>,
    source: &str,
    functions: &[String],
    references: &mut References,
) -> io::Result<()> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_css::LANGUAGE.into())
        .map_err(io::Error::other)?;
    parser
        .set_included_ranges(&[node.range()])
        .map_err(io::Error::other)?;
    let mut tree = parser
        .parse(source, None)
        .ok_or_else(|| io::Error::other("CSS parsing was cancelled"))?;
    if tree.root_node().has_error() {
        // The CSS 0.25 scanner mistakes braces inside values for selector blocks.
        // Recover the parser input without changing binding source or byte offsets.
        let mut parser_source = source.as_bytes().to_vec();
        if mask_value_braces(tree.root_node(), &mut parser_source) {
            tree = parser
                .parse(&parser_source, None)
                .ok_or_else(|| io::Error::other("CSS parsing was cancelled"))?;
        }
    }
    visit(tree.root_node(), source, functions, references)
}

fn comment_ranges(node: Node<'_>, ranges: &mut Vec<std::ops::Range<usize>>) {
    if matches!(node.kind(), "comment" | "js_comment") {
        ranges.push(node.byte_range());
        return;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        comment_ranges(child, ranges);
    }
}

fn mask_value_braces(node: Node<'_>, source: &mut [u8]) -> bool {
    let mut comments = Vec::new();
    comment_ranges(node, &mut comments);
    let mut comments = comments.iter().peekable();
    let mut bytes = source
        .iter()
        .copied()
        .enumerate()
        .skip(node.start_byte())
        .take(node.end_byte() - node.start_byte())
        .peekable();
    let mut quote = None;
    let mut depth = 0usize;
    let mut pending = Vec::new();
    let mut braces = Vec::new();
    while let Some((offset, byte)) = bytes.next() {
        if let Some(delimiter) = quote {
            if byte == delimiter {
                quote = None;
            } else if byte == b'\\' {
                if let Some((offset, b'{' | b'}')) = bytes.next() {
                    braces.push(offset);
                }
            } else if matches!(byte, b'{' | b'}') {
                braces.push(offset);
            }
            continue;
        }
        while comments.next_if(|comment| comment.end <= offset).is_some() {}
        if let Some(comment) = comments.peek()
            && comment.start <= offset
        {
            while bytes.next_if(|(offset, _)| *offset < comment.end).is_some() {}
            continue;
        }
        match byte {
            b'\'' | b'"' => quote = Some(byte),
            b'`' if depth > 0 => quote = Some(byte),
            b'\\' => {
                if let Some((offset, b'{' | b'}')) = bytes.next()
                    && depth > 0
                {
                    pending.push(offset);
                }
            }
            b'(' => depth += 1,
            b')' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    braces.append(&mut pending);
                }
            }
            b'{' | b'}' if depth > 0 => pending.push(offset),
            _ => {}
        }
    }

    // Unclosed parentheses cannot authorize masking later structural rule braces.
    // Binding discovery still requires a call node in the recovered CSS tree.
    for offset in &braces {
        if let Some(byte) = source.get_mut(*offset) {
            *byte = b' ';
        }
    }
    !braces.is_empty()
}

fn visit(
    node: Node<'_>,
    source: &str,
    functions: &[String],
    references: &mut References,
) -> io::Result<()> {
    if matches!(node.kind(), "comment" | "js_comment" | "string_value") {
        return Ok(());
    }
    if node.kind() == "call_expression"
        && node.named_child(0).is_some_and(|name| {
            name.kind() == "function_name" && text(name, source).is_ok_and(|name| name == "v-bind")
        })
        && let Some(arguments) = node.named_child(1)
    {
        let raw = text(arguments, source)?;
        let Some(inner) = raw.strip_prefix('(').and_then(|raw| raw.strip_suffix(')')) else {
            return Err(io::Error::other("incomplete Vue CSS binding arguments"));
        };
        let trimmed = inner.trim();
        let mut base = arguments.start_byte() + 1 + inner.len() - inner.trim_start().len();

        // Vue normalizes bindings by trimming and removing matching outer quotes.
        // CSS escapes remain intact because the expression is JavaScript source.
        let expression = if let Some(unquoted) = trimmed
            .strip_prefix('\'')
            .and_then(|raw| raw.strip_suffix('\''))
            .or_else(|| {
                trimmed
                    .strip_prefix('"')
                    .and_then(|raw| raw.strip_suffix('"'))
            }) {
            base += 1;
            unquoted
        } else {
            trimmed
        };
        return parse_expression(
            expression,
            base,
            source,
            Dialect::Typescript,
            functions,
            references,
        );
    }

    // Styles may contain preprocessor syntax outside the CSS grammar.
    // Only discovered JavaScript bindings must parse successfully.
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        visit(child, source, functions, references)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{References, mask_value_braces, visit_bindings};
    use std::io;
    use tree_sitter::{Node, Parser};

    fn find_style(node: Node<'_>) -> Option<Node<'_>> {
        if node.kind() == "raw_text"
            && node
                .parent()
                .is_some_and(|parent| parent.kind() == "style_element")
        {
            return Some(node);
        }
        let mut cursor = node.walk();
        node.named_children(&mut cursor).find_map(find_style)
    }

    fn scan(source: &str) -> io::Result<References> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_vue_next::LANGUAGE.into())
            .map_err(io::Error::other)?;
        let tree = parser
            .parse(source, None)
            .ok_or_else(|| io::Error::other("Vue parsing was cancelled"))?;
        let style = find_style(tree.root_node())
            .ok_or_else(|| io::Error::other("test source has no style content"))?;
        let mut references = References::default();
        visit_bindings(style, source, &["t".into()], &mut references)?;
        Ok(references)
    }

    #[test_util::test]
    fn quoted_and_unquoted_bindings_find_nested_translation_calls() {
        let source = indoc::indoc! {r#"
            <style>
            .label {
                color: v-bind(t('app.unquoted'));
                background: v-bind("t('app.quoted')");
                content: v-bind( 'format(t("app.nested"))' );
            }
            </style>
        "#};
        let references = scan(source)?;
        assert_eq!(
            references.literals,
            [
                "app.unquoted".into(),
                "app.quoted".into(),
                "app.nested".into()
            ]
            .into(),
        );
        assert!(references.dynamic.is_empty());
    }

    #[test_util::test]
    fn dynamic_bindings_preserve_prefixes_and_original_source_spans() {
        let source = indoc::indoc! {r#"
            <template><p>Example</p></template>
            <style>
            .label {
                content: v-bind( "t(`app.${kind}`)" );
            }
            </style>
        "#};
        let references = scan(source)?;
        assert!(references.literals.is_empty());
        assert_eq!(references.dynamic.len(), 1);
        let reference = &references.dynamic[0];
        assert_eq!(reference.prefix.as_deref(), Some("app."));
        assert_eq!(source.get(reference.span.clone()), Some("`app.${kind}`"));
    }

    #[test_util::test]
    fn unquoted_templates_and_objects_preserve_original_binding_source() {
        let source = indoc::indoc! {r#"
            <style>
            .label {
                content: v-bind(t(`app.${kind}`));
                color: v-bind(format({label: t("app.live")}));
                background: v-bind(format({label: t(`other.${kind}`)}));
            }
            </style>
        "#};
        let references = scan(source)?;
        assert_eq!(references.literals, ["app.live".into()].into());
        assert_eq!(references.dynamic.len(), 2);
        for (reference, prefix, spelling) in [
            (&references.dynamic[0], "app.", "`app.${kind}`"),
            (&references.dynamic[1], "other.", "`other.${kind}`"),
        ] {
            assert_eq!(reference.prefix.as_deref(), Some(prefix));
            assert_eq!(source.get(reference.span.clone()), Some(spelling));
        }
    }

    #[test_util::test]
    fn css_comments_and_strings_do_not_produce_usages() {
        let source = indoc::indoc! {r#"
            <style>
            /* color: v-bind(t('app.comment')); */
            /* color: v-bind(format({label: t('app.object_comment')})); */
            // color: v-bind(t('app.line_comment'));
            .label {
                content: "v-bind(t('app.string'))";
                quotes: "v-bind(t(`app.${kind}`))";
                object: 'v-bind(format({label: t("app.object_string")}))';
                background: url("v-bind(t('app.url'))");
                color: ordinary(t('app.css_data'));
            }
            </style>
        "#};
        let references = scan(source)?;
        assert!(references.literals.is_empty());
        assert!(references.dynamic.is_empty());
    }

    #[test_util::test]
    fn preprocessor_errors_are_ignored_but_invalid_bindings_fail() {
        let source = indoc::indoc! {r#"
            <style lang="scss">
            $accent: red;
            .label { color: v-bind("t('app.live')"); }
            </style>
        "#};
        let references = scan(source)?;
        assert_eq!(references.literals, ["app.live".into()].into());

        let source = indoc::indoc! {r#"
            <style>
            .label { color: v-bind("t("); }
            </style>
        "#};
        assert!(scan(source).is_err());
    }

    #[test_util::test]
    fn recovery_preserves_rule_braces_and_ignores_nonstructural_parentheses() {
        let source = indoc::indoc! {r#"
            .x { value: fn(")", /* ) */ { nested: 1 }); }
            .y { value: fn(\), { escaped: 1 }); }
            .z { value: fn(; }
        "#};
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_css::LANGUAGE.into())
            .map_err(io::Error::other)?;
        let tree = parser
            .parse(source, None)
            .ok_or_else(|| io::Error::other("CSS parsing was cancelled"))?;
        let mut recovered = source.as_bytes().to_vec();
        assert!(mask_value_braces(tree.root_node(), &mut recovered));

        // Quoted, commented, and escaped parentheses cannot close a value.
        // The final unclosed value leaves its enclosing rule's brace intact.
        let expected = source
            .replace("{ nested: 1 }", "  nested: 1  ")
            .replace("{ escaped: 1 }", "  escaped: 1  ");
        assert_eq!(String::from_utf8(recovered)?, expected);
    }
}
