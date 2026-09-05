//! Embedded scripts and template expressions retain their original source locations.

use super::{Dialect, References, parse_region, text, visit};
use std::io;
use tree_sitter::{Node, Parser, Range};

pub(super) fn visit_template(
    node: Node<'_>,
    source: &str,
    dialect: Dialect,
    functions: &[String],
    references: &mut References,
) -> io::Result<()> {
    if matches!(node.kind(), "style_element" | "style") {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "start_tag" {
                visit(child, source, dialect, functions, references, None)?;
            } else if child.kind() == "raw_text" && dialect == Dialect::Vue {
                super::css::visit_bindings(child, source, functions, references)?;
            }
        }
        return Ok(());
    }
    if dialect == Dialect::Vue && node.kind() == "element" && has_attribute(node, source, "v-pre")?
    {
        return Ok(());
    }
    if dialect == Dialect::Html && node.kind() == "text" {
        return visit_mustaches(node, source, functions, references);
    }
    if dialect == Dialect::Astro && node.kind() == "html_interpolation" {
        let raw = text(node, source)?;
        let expression = raw
            .strip_prefix('{')
            .and_then(|s| s.strip_suffix('}'))
            .unwrap_or(raw);
        return parse_expression(
            expression,
            node.start_byte() + 1,
            source,
            Dialect::Tsx,
            functions,
            references,
        );
    }
    if node.kind() == "attribute_backtick_string" {
        return parse_expression(
            text(node, source)?,
            node.start_byte(),
            source,
            Dialect::Typescript,
            functions,
            references,
        );
    }
    if node.kind() == "svelte_raw_text" {
        return parse_svelte_expression(node, source, functions, references);
    }
    if node.kind() == "attribute_js_expr" {
        return parse_expression(
            text(node, source)?,
            node.start_byte(),
            source,
            Dialect::Typescript,
            functions,
            references,
        );
    }
    let injected = injected_dialect(node, source, dialect)?;
    if let Some(injected) = injected {
        if node.kind() == "attribute_value" {
            return parse_attribute(node, source, injected, functions, references);
        }
        if node
            .parent()
            .is_some_and(|parent| parent.kind() == "interpolation")
        {
            return parse_expression(
                text(node, source)?,
                node.start_byte(),
                source,
                injected,
                functions,
                references,
            );
        }
        return parse_region(source, injected, Some(node.range()), functions, references);
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        visit(child, source, dialect, functions, references, None)?;
    }
    Ok(())
}

fn injected_dialect(node: Node<'_>, source: &str, dialect: Dialect) -> io::Result<Option<Dialect>> {
    Ok(match node.kind() {
        "frontmatter_js_block" => Some(Dialect::Typescript),
        "raw_text"
            if node
                .parent()
                .is_some_and(|parent| parent.kind() == "script_element") =>
        {
            script_dialect(node.parent(), source, dialect)?
        }
        "raw_text"
            if node
                .parent()
                .is_some_and(|parent| parent.kind() == "interpolation") =>
        {
            Some(Dialect::Typescript)
        }
        "attribute_value"
            if matches!(dialect, Dialect::Vue | Dialect::Html)
                && (is_expression_attribute(node, source)?
                    || (dialect == Dialect::Html && text(node, source)?.contains("{{"))) =>
        {
            Some(if dialect == Dialect::Html {
                Dialect::AngularExpression
            } else {
                Dialect::Typescript
            })
        }
        _ => None,
    })
}

fn has_attribute(node: Node<'_>, source: &str, name: &str) -> io::Result<bool> {
    Ok(attributes(node, source)?
        .iter()
        .any(|(key, _)| *key == name))
}

/// Maps decoded template expression offsets back to the original HTML source.
///
/// Entity decoding and parser delimiters change lengths, so a constant offset
/// cannot preserve diagnostic spans inside attributes.
struct AttributeSource {
    text: String,
    offsets: Vec<usize>,
}

impl AttributeSource {
    fn new(raw: &str, base: usize, prefix: &str, suffix: &str, decode_entities: bool) -> Self {
        let mut result = Self {
            text: String::new(),
            offsets: Vec::new(),
        };
        result.push(prefix, base);
        let mut remaining = raw;
        while !remaining.is_empty() {
            let offset = base + raw.len() - remaining.len();
            if decode_entities
                && remaining.starts_with('&')
                && let Some(end) = remaining
                    .char_indices()
                    .skip(1)
                    .find(|(_, c)| !c.is_ascii_alphanumeric() && *c != '#')
                    .filter(|(_, c)| *c == ';')
                    .map(|(index, _)| index)
                && let Some(entity) = remaining.get(..=end)
            {
                let decoded = html_escape::decode_html_entities(entity);
                if decoded != entity {
                    result.push(&decoded, offset);
                    remaining = remaining.strip_prefix(entity).unwrap_or("");
                    continue;
                }
            }
            let Some(c) = remaining.chars().next() else {
                break;
            };
            result.push(&c.to_string(), offset);
            remaining = remaining.strip_prefix(c).unwrap_or("");
        }
        result.push(suffix, base + raw.len());
        result.offsets.push(base + raw.len());
        result
    }

    fn push(&mut self, text: &str, offset: usize) {
        self.text.push_str(text);
        self.offsets.extend(std::iter::repeat_n(offset, text.len()));
    }

    fn original_span(&self, span: std::ops::Range<usize>) -> io::Result<std::ops::Range<usize>> {
        let start = self
            .offsets
            .get(span.start)
            .copied()
            .ok_or_else(|| io::Error::other("invalid decoded attribute start"))?;
        let end = self
            .offsets
            .get(span.end)
            .copied()
            .ok_or_else(|| io::Error::other("invalid decoded attribute end"))?;
        Ok(start..end)
    }
}

fn parse_attribute(
    node: Node<'_>,
    source: &str,
    dialect: Dialect,
    functions: &[String],
    references: &mut References,
) -> io::Result<()> {
    let name = ancestor_attribute_name(node, source)?.unwrap_or("title");
    let angular_binding =
        dialect == Dialect::AngularExpression && is_expression_attribute(node, source)?;
    let prefix = if angular_binding {
        format!("<p {name}=\"")
    } else {
        "(".to_owned()
    };
    let suffix = if angular_binding { "\"></p>" } else { ")" };
    let (prefix, suffix) = if (dialect == Dialect::AngularExpression && !angular_binding)
        || name.starts_with('@')
        || name.starts_with("v-on:")
    {
        ("", "")
    } else {
        (prefix.as_str(), suffix)
    };
    let mut attribute =
        AttributeSource::new(text(node, source)?, node.start_byte(), prefix, suffix, true);
    if dialect == Dialect::Typescript
        && ancestor_attribute_name(node, source)?.is_some_and(|name| name == "v-for")
    {
        // Vue's iterable is a runtime expression; the aliases before in/of are bindings.
        let boundary = attribute
            .text
            .match_indices("in")
            .chain(attribute.text.match_indices("of"))
            .filter(|(at, word)| {
                attribute
                    .text
                    .get(..*at)
                    .and_then(|s| s.chars().next_back())
                    .is_some_and(char::is_whitespace)
                    && attribute
                        .text
                        .get(at + word.len()..)
                        .and_then(|s| s.chars().next())
                        .is_some_and(char::is_whitespace)
            })
            .map(|(at, word)| at + word.len())
            .min();
        if let Some(boundary) = boundary {
            attribute.text = attribute
                .text
                .get(boundary..)
                .ok_or_else(|| io::Error::other("invalid v-for expression"))?
                .to_owned();
            attribute.offsets.drain(..boundary);
            attribute.text.insert(0, '(');
            attribute.offsets.insert(0, node.start_byte() + boundary);
        }
    }
    merge_expression(&attribute, source, dialect, functions, references)
}

pub(super) fn parse_expression(
    raw: &str,
    base: usize,
    source: &str,
    dialect: Dialect,
    functions: &[String],
    references: &mut References,
) -> io::Result<()> {
    if raw.trim().is_empty()
        || ((raw.trim_start().starts_with("/*") || raw.trim_start().starts_with("//"))
            && only_comments(raw, dialect)?)
    {
        return Ok(());
    }
    let spread = raw.trim_start().starts_with("...");
    let (prefix, suffix) = if spread { ("({", "})") } else { ("(", ")") };
    merge_expression(
        &AttributeSource::new(raw, base, prefix, suffix, false),
        source,
        dialect,
        functions,
        references,
    )
}

fn only_comments(raw: &str, dialect: Dialect) -> io::Result<bool> {
    let mut parser = Parser::new();
    parser
        .set_language(&dialect.language())
        .map_err(io::Error::other)?;
    let tree = parser
        .parse(raw, None)
        .ok_or_else(|| io::Error::other("expression parsing was cancelled"))?;
    let mut cursor = tree.root_node().walk();
    Ok(tree
        .root_node()
        .named_children(&mut cursor)
        .all(|child| child.kind().contains("comment")))
}

fn merge_expression(
    attribute: &AttributeSource,
    source: &str,
    dialect: Dialect,
    functions: &[String],
    references: &mut References,
) -> io::Result<()> {
    let mut parsed = References::default();
    parse_region(&attribute.text, dialect, None, functions, &mut parsed).map_err(|error| {
        if let Some(error) = error
            .get_ref()
            .and_then(|error| error.downcast_ref::<super::SyntaxError>())
        {
            let offset = attribute.offsets.get(error.offset).copied().unwrap_or(0);
            io::Error::other(super::SyntaxError::at(source, offset))
        } else {
            error
        }
    })?;
    references.literals.extend(parsed.literals);
    references.identifiers.extend(parsed.identifiers);
    for mut dynamic in parsed.dynamic {
        dynamic.span = attribute.original_span(dynamic.span)?;
        references.dynamic.push(dynamic);
    }
    Ok(())
}

fn parse_svelte_expression(
    node: Node<'_>,
    source: &str,
    functions: &[String],
    references: &mut References,
) -> io::Result<()> {
    let parent = node.parent();
    let raw = text(node, source)?;
    let base = node.start_byte();
    if parent.is_some_and(|parent| {
        parent.kind() == "each_start" && parent.child_by_field_name("parameter") == Some(node)
    }) && let Some(parts) = svelte_binding(raw, base)?
    {
        return merge_svelte_parts(parts, source, functions, references);
    }
    if parent.is_some_and(|parent| parent.kind() == "await_start")
        && svelte_candidate(raw, base, "(", ")")?.is_none()
    {
        // Validate both sides so keywords inside strings or nested expressions
        // cannot split a block.
        for (offset, _) in raw.char_indices() {
            let Some(rest) = raw.get(offset..) else {
                continue;
            };
            let Some(keyword) = ["then", "catch"].into_iter().find(|&word| {
                rest.starts_with(word)
                    && raw
                        .get(..offset)
                        .and_then(|s| s.chars().next_back())
                        .is_some_and(char::is_whitespace)
                    && rest
                        .get(word.len()..)
                        .and_then(|s| s.chars().next())
                        .is_some_and(char::is_whitespace)
            }) else {
                continue;
            };
            let (Some(value), Some(binding)) = (
                svelte_candidate(raw.get(..offset).unwrap_or(""), base, "(", ")")?,
                svelte_candidate(
                    rest.get(keyword.len()..).unwrap_or(""),
                    base + offset + keyword.len(),
                    "(",
                    ") => 0",
                )?,
            ) else {
                continue;
            };
            return merge_svelte_parts(vec![value, binding], source, functions, references);
        }
    }
    parse_expression(
        raw,
        base,
        source,
        Dialect::Typescript,
        functions,
        references,
    )
}

fn svelte_binding(raw: &str, base: usize) -> io::Result<Option<Vec<AttributeSource>>> {
    if let Some(binding) = svelte_candidate(raw, base, "(", ") => 0")? {
        return Ok(Some(vec![binding]));
    }
    // The final parenthesized expression is the each key; defaults belong to the binding.
    for (offset, _) in raw.match_indices('(').rev() {
        let (Some(binding), Some(key)) = (
            svelte_candidate(raw.get(..offset).unwrap_or(""), base, "(", ") => 0")?,
            svelte_candidate(raw.get(offset..).unwrap_or(""), base + offset, "(", ")")?,
        ) else {
            continue;
        };
        return Ok(Some(vec![binding, key]));
    }
    Ok(None)
}

fn svelte_candidate(
    raw: &str,
    base: usize,
    prefix: &str,
    suffix: &str,
) -> io::Result<Option<AttributeSource>> {
    let candidate = AttributeSource::new(raw, base, prefix, suffix, false);
    let mut parser = Parser::new();
    parser
        .set_language(&Dialect::Typescript.language())
        .map_err(io::Error::other)?;
    let tree = parser
        .parse(&candidate.text, None)
        .ok_or_else(|| io::Error::other("Svelte expression parsing was cancelled"))?;
    Ok((!tree.root_node().has_error()).then_some(candidate))
}

fn merge_svelte_parts(
    parts: Vec<AttributeSource>,
    source: &str,
    functions: &[String],
    references: &mut References,
) -> io::Result<()> {
    for part in parts {
        merge_expression(&part, source, Dialect::Typescript, functions, references)?;
    }
    Ok(())
}

fn ancestor_attribute_name<'a>(node: Node<'_>, source: &'a str) -> io::Result<Option<&'a str>> {
    let mut parent = node.parent();
    while let Some(node) = parent {
        if matches!(node.kind(), "attribute" | "directive_attribute") {
            return Ok(Some(
                text(node, source)?.split('=').next().unwrap_or("").trim(),
            ));
        }
        parent = node.parent();
    }
    Ok(None)
}

fn attributes<'a>(node: Node<'_>, source: &'a str) -> io::Result<Vec<(&'a str, &'a str)>> {
    let mut cursor = node.walk();
    let Some(tag) = node
        .named_children(&mut cursor)
        .find(|child| matches!(child.kind(), "start_tag" | "self_closing_tag"))
    else {
        return Ok(Vec::new());
    };
    let mut result = Vec::new();
    let mut cursor = tag.walk();
    for attribute in tag.named_children(&mut cursor) {
        if matches!(attribute.kind(), "attribute" | "directive_attribute") {
            let raw = text(attribute, source)?;
            let (name, value) = raw.split_once('=').unwrap_or((raw, ""));
            result.push((name.trim(), value.trim().trim_matches(['\'', '"'])));
        }
    }
    Ok(result)
}

fn script_dialect(
    node: Option<Node<'_>>,
    source: &str,
    outer: Dialect,
) -> io::Result<Option<Dialect>> {
    let Some(node) = node else { return Ok(None) };
    let attributes = attributes(node, source)?;
    if attributes.iter().any(|(name, value)| {
        *name == "type"
            && !matches!(
                *value,
                "module"
                    | "text/javascript"
                    | "application/javascript"
                    | "text/ecmascript"
                    | "application/ecmascript"
                    | ""
            )
    }) {
        return Ok(None);
    }
    match attributes
        .iter()
        .find_map(|(name, value)| (*name == "lang").then_some(*value))
    {
        Some("ts" | "typescript") => Ok(Some(Dialect::Typescript)),
        Some("tsx") => Ok(Some(Dialect::Tsx)),
        Some("js" | "javascript" | "jsx") => Ok(Some(Dialect::Javascript)),
        None => Ok(Some(if outer == Dialect::Astro {
            Dialect::Typescript
        } else {
            Dialect::Javascript
        })),
        Some(lang) => Err(io::Error::other(format!(
            "unsupported script language `{lang}` in usage source"
        ))),
    }
}

fn visit_mustaches(
    node: Node<'_>,
    source: &str,
    functions: &[String],
    references: &mut References,
) -> io::Result<()> {
    let mut remaining = text(node, source)?;
    while let Some(start) = remaining
        .find("{{")
        .into_iter()
        .chain(remaining.find("@let "))
        .min()
    {
        let offset = node.end_byte() - remaining.len() + start;
        let prefix = source
            .get(..offset)
            .ok_or_else(|| io::Error::other("invalid interpolation offset"))?;
        let mut parser = Parser::new();
        parser
            .set_language(&Dialect::AngularExpression.language())
            .map_err(io::Error::other)?;
        parser
            .set_included_ranges(&[Range {
                start_byte: offset,
                end_byte: node.end_byte(),
                start_point: tree_sitter::Point {
                    row: prefix.bytes().filter(|b| *b == b'\n').count(),
                    column: prefix.rsplit('\n').next().unwrap_or("").len(),
                },
                end_point: node.end_position(),
            }])
            .map_err(io::Error::other)?;
        let tree = parser
            .parse(source, None)
            .ok_or_else(|| io::Error::other("interpolation parsing was cancelled"))?;
        let block = tree
            .root_node()
            .named_child(0)
            .filter(|child| {
                matches!(child.kind(), "interpolation" | "let_statement") && !child.has_error()
            })
            .ok_or_else(|| io::Error::other(super::SyntaxError::at(source, offset)))?;
        visit(
            block,
            source,
            Dialect::AngularExpression,
            functions,
            references,
            None,
        )?;
        remaining = source
            .get(block.end_byte()..node.end_byte())
            .ok_or_else(|| io::Error::other("invalid interpolation range"))?;
    }
    Ok(())
}

fn is_expression_attribute(node: Node<'_>, source: &str) -> io::Result<bool> {
    let mut parent = node.parent();
    while let Some(attribute) = parent {
        if attribute.kind() == "directive_attribute" {
            return Ok(true);
        }
        if attribute.kind() == "attribute" {
            let mut cursor = attribute.walk();
            let name = attribute
                .named_children(&mut cursor)
                .find(|child| child.kind() == "attribute_name");
            return name.map_or(Ok(false), |name| {
                let name = text(name, source)?;
                Ok(name.starts_with([':', '@', '[', '(', '*']) || name.starts_with("v-"))
            });
        }
        parent = attribute.parent();
    }
    Ok(false)
}

#[cfg(test)]
mod svelte_tests {
    use super::super::scan;
    use std::path::Path;

    #[test_util::test]
    fn each_defaults_and_keys_preserve_dynamic_source_spans() {
        let source = "{#each rows as { label = t(`app.${kind}`) }, index (t(`key.${kind}`))}<p>{label}</p>{/each}";
        let references = scan(Path::new("component.svelte"), source, &["t".into()])?;

        // Both executable regions retain their own original-file locations.
        assert!(references.literals.is_empty());
        assert_eq!(
            references
                .dynamic
                .iter()
                .map(|usage| source.get(usage.span.clone()))
                .collect::<Vec<_>>(),
            [Some("`app.${kind}`"), Some("`key.${kind}`")]
        );
    }

    #[test_util::test]
    fn await_boundaries_follow_syntax_and_accept_multiline_whitespace() {
        for source in [
            "{#await load(' then ', t('app.live')) then value}<p>{value}</p>{/await}",
            "{#await load(' catch ', t('app.live')) catch error}<p>{error}</p>{/await}",
            "{#await load(' then ').then(() => t('app.live'))}<p>Loading</p>{/await}",
            indoc::indoc! {"
                {#await load(t('app.live'))
                then value}<p>{value}</p>{/await}
            "},
            "{#await promise then {label = t('app.live')}}<p>{label}</p>{/await}",
        ] {
            let references = scan(Path::new("component.svelte"), source, &["t".into()])?;
            assert!(references.literals.contains("app.live"), "{source}");
            assert!(references.dynamic.is_empty(), "{source}");
        }
    }
}
