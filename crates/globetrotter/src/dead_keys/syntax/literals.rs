//! Each language's string syntax determines literal values and inferred key prefixes.

use super::{Dialect, text, unwrap_expression};
use std::io;
use tree_sitter::Node;

#[cfg(test)]
#[path = "literal_tests.rs"]
mod tests;

fn is_string(node: Node<'_>) -> bool {
    matches!(
        node.kind(),
        "string"
            | "concatenated_string"
            | "template_string"
            | "string_literal"
            | "raw_string_literal"
            | "verbatim_string_literal"
            | "interpreted_string_literal"
            | "encapsed_string"
            | "line_string_literal"
            | "multi_line_string_literal"
            | "multiline_string_literal"
            | "interpolated_string_expression"
            | "string_literal_double_quotes"
            | "string_literal_single_quotes"
            | "string_literal_double_quotes_multiple"
            | "string_literal_single_quotes_multiple"
            | "raw_string_literal_double_quotes"
            | "raw_string_literal_single_quotes"
            | "raw_string_literal_double_quotes_multiple"
            | "raw_string_literal_single_quotes_multiple"
    )
}

fn interpolation(node: Node<'_>) -> bool {
    matches!(
        node.kind(),
        "template_substitution"
            | "interpolation"
            | "string_interpolation"
            | "interpolated_expression"
            | "raw_str_interpolation"
            | "variable_name"
            | "dynamic_variable_name"
    )
}

pub(super) fn first_interpolation(node: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if interpolation(child) {
            return Some(child);
        }
        if let Some(found) = first_interpolation(child) {
            return Some(found);
        }
    }
    None
}

fn interpolation_start(
    node: Node<'_>,
    source: &str,
    dialect: Dialect,
) -> io::Result<Option<usize>> {
    let parsed = first_interpolation(node).map(|interpolation| interpolation.start_byte());
    if dialect != Dialect::Kotlin {
        return Ok(parsed);
    }

    // The Kotlin grammar can tokenize bare dollar interpolation as string content.
    // Scan only the parsed string, respecting escapes outside raw triple quotes.
    let raw = text(node, source)?;
    let multiline = node.kind() == "multiline_string_literal";
    let mut chars = raw.char_indices().peekable();
    while let Some((offset, character)) = chars.next() {
        if character == '\\' && !multiline {
            chars.next();
        } else if character == '$'
            && chars
                .peek()
                .is_some_and(|(_, next)| matches!(next, '_' | '`' | '{') || next.is_alphabetic())
        {
            let offset = node.start_byte() + offset;
            return Ok(Some(parsed.map_or(offset, |parsed| parsed.min(offset))));
        }
    }
    Ok(parsed)
}

pub(super) fn literal(
    node: Node<'_>,
    source: &str,
    dialect: Dialect,
) -> io::Result<Option<String>> {
    let node = unwrap_expression(node);
    if dialect == Dialect::Python && node.kind() == "concatenated_string" {
        let mut value = String::new();
        let mut cursor = node.walk();
        for child in node
            .named_children(&mut cursor)
            .filter(|child| !child.kind().contains("comment"))
        {
            let Some(part) = literal(child, source, dialect)? else {
                return Ok(None);
            };
            value.push_str(&part);
        }
        return Ok(Some(value));
    }
    if !is_string(node) {
        return Ok(None);
    }
    if interpolation_start(node, source, dialect)?.is_some() {
        return Ok(None);
    }
    let raw = text(node, source)?;
    if dialect == Dialect::Lua && raw.starts_with('[') {
        return Ok(Some(
            node.child_by_field_name("content")
                .map(|content| text(content, source).map(str::to_owned))
                .transpose()?
                .unwrap_or_default(),
        ));
    }
    if dialect == Dialect::Ruby && (raw.starts_with("%q") || raw.starts_with("%Q")) {
        let mut cursor = node.walk();
        let mut value = String::new();
        for child in node.named_children(&mut cursor) {
            value.push_str(text(child, source)?);
        }
        return Ok(Some(if raw.starts_with("%q") {
            value
        } else {
            decode_escapes(&value)
        }));
    }
    let Some((body, is_raw)) = string_body(raw, dialect) else {
        return Ok(None);
    };
    Ok(Some(
        if dialect == Dialect::CSharp && raw.starts_with("@\"") {
            body.replace("\"\"", "\"")
        } else if is_raw {
            body.to_owned()
        } else {
            decode_escapes(body)
        },
    ))
}

fn string_body(raw: &str, dialect: Dialect) -> Option<(&str, bool)> {
    let quote_at = raw.find(['\'', '"', '`'])?;
    let prefix = raw.get(..quote_at)?;
    let quoted = raw.get(quote_at..)?;
    let quote = quoted.chars().next()?;
    let delimiter = quote.to_string();
    let triple = delimiter.repeat(3);
    let quoted = quoted.trim_end_matches('#');
    let verbatim = dialect == Dialect::CSharp && prefix.contains('@');
    let body = if !verbatim && quoted.starts_with(&triple) && quoted.len() >= 6 {
        quoted.strip_prefix(&triple)?.strip_suffix(&triple)?
    } else {
        quoted.strip_prefix(quote)?.strip_suffix(quote)?
    };
    let is_raw = prefix.to_ascii_lowercase().contains('r')
        || verbatim
        || (dialect == Dialect::Kotlin && quoted.starts_with(&triple))
        || (quote == '`' && dialect == Dialect::Go)
        || (quote == '\'' && matches!(dialect, Dialect::Php | Dialect::Ruby));
    Some((body, is_raw))
}

fn decode_escapes(body: &str) -> String {
    let mut decoded = String::new();
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            decoded.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => decoded.push('\n'),
            Some('r') => decoded.push('\r'),
            Some('t') => decoded.push('\t'),
            Some('\n') => {}
            Some(escape @ ('u' | 'x' | 'U')) => {
                let braced = chars.next_if_eq(&'{').is_some();
                let digits: String = if braced {
                    chars.by_ref().take_while(|c| *c != '}').collect()
                } else {
                    chars
                        .by_ref()
                        .take(match escape {
                            'x' => 2,
                            'U' => 8,
                            _ => 4,
                        })
                        .collect()
                };
                if let Ok(code) = u32::from_str_radix(&digits, 16)
                    && let Some(c) = char::from_u32(code)
                {
                    decoded.push(c);
                }
            }
            Some(c) => decoded.push(c),
            None => decoded.push('\\'),
        }
    }
    decoded
}

/// Recognizes string construction without resolving identifiers or function results.
pub(super) fn constructs_string(
    node: Node<'_>,
    source: &str,
    dialect: Dialect,
) -> io::Result<bool> {
    let node = unwrap_expression(node);
    if is_string(node) {
        return Ok(interpolation_start(node, source, dialect)?.is_some());
    }
    let Some(operator) = concatenation_operator(node, source, dialect)? else {
        return Ok(false);
    };
    if matches!(operator, "." | ".." | "<>") {
        return Ok(true);
    }
    // Overloaded addition needs a visible string operand; arithmetic alone is opaque.
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        // Angular stores trailing pipes on the right operand's expression wrapper.
        let child = if dialect == Dialect::AngularExpression
            && child.kind() == "expression"
            && child.child_by_field_name("pipes").is_some()
        {
            child.named_child(0).unwrap_or(child)
        } else {
            child
        };
        if literal(child, source, dialect)?.is_some() || constructs_string(child, source, dialect)?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn concatenation_operator<'source>(
    node: Node<'_>,
    source: &'source str,
    dialect: Dialect,
) -> io::Result<Option<&'source str>> {
    if dialect == Dialect::AngularExpression && node.kind() == "concatenation_expression" {
        return Ok(Some("+"));
    }
    if !matches!(
        node.kind(),
        "binary_expression" | "additive_expression" | "binary_operator" | "binary"
    ) {
        return Ok(None);
    }
    let mut cursor = node.walk();
    let operator = node
        .child_by_field_name("operator")
        .or_else(|| node.children(&mut cursor).find(|child| !child.is_named()));
    let Some(operator) = operator else {
        return Ok(None);
    };
    let operator = text(operator, source)?;
    Ok((match dialect {
        Dialect::Php => operator == ".",
        Dialect::Lua => operator == "..",
        Dialect::Elixir => operator == "<>",
        Dialect::Zig => operator == "++",
        _ => operator == "+",
    })
    .then_some(operator))
}

pub(super) fn literal_prefix(
    node: Node<'_>,
    source: &str,
    dialect: Dialect,
) -> io::Result<Option<String>> {
    let node = unwrap_expression(node);
    if let Some(value) = literal(node, source, dialect)? {
        return Ok(Some(value));
    }
    if dialect == Dialect::Python && node.kind() == "concatenated_string" {
        let mut prefix = String::new();
        let mut cursor = node.walk();
        for part in node
            .named_children(&mut cursor)
            .filter(|part| !part.kind().contains("comment"))
        {
            if let Some(value) = literal(part, source, dialect)? {
                prefix.push_str(&value);
            } else {
                if let Some(value) = literal_prefix(part, source, dialect)? {
                    prefix.push_str(&value);
                }
                break;
            }
        }
        return Ok(Some(prefix));
    }
    if is_string(node)
        && let Some(first) = interpolation_start(node, source, dialect)?
    {
        let raw = source
            .get(node.start_byte()..first)
            .ok_or_else(|| io::Error::other("invalid template prefix span"))?;
        let Some(quote_at) = raw.find(['\'', '"', '`']) else {
            return Ok(None);
        };
        let delimiter_len = if raw.get(quote_at..).is_some_and(|quoted| {
            quoted.starts_with("\"\"\"") || quoted.starts_with("'''") || quoted.starts_with("```")
        }) {
            3
        } else {
            1
        };
        let prefix = raw
            .get(quote_at + delimiter_len..)
            .ok_or_else(|| io::Error::other("invalid string prefix"))?;
        let prefix = if dialect == Dialect::Swift {
            prefix.strip_suffix("\\(").unwrap_or(prefix)
        } else {
            prefix
        };
        return Ok(Some(if dialect == Dialect::Kotlin && delimiter_len == 3 {
            prefix.to_owned()
        } else {
            decode_escapes(prefix)
        }));
    }
    if concatenation_operator(node, source, dialect)?.is_some() {
        return match node
            .child_by_field_name("left")
            .or_else(|| node.named_child(0))
        {
            Some(left) => literal_prefix(left, source, dialect),
            None => Ok(None),
        };
    }
    Ok(None)
}
