//! Angular pipe inputs are classified before their constituent literals are visited.

use super::{Dialect, DynamicReference, References, literal_prefix, text};
use std::io;
use std::ops::Range;
use tree_sitter::Node;

pub(super) fn classify_pipes(
    node: Node<'_>,
    source: &str,
    functions: &[String],
    references: &mut References,
) -> io::Result<Option<Range<usize>>> {
    let Some(expression) = pipe_expression(node) else {
        return Ok(None);
    };
    if node
        .parent()
        .is_some_and(|parent| is_calculation(parent) && parent.end_byte() == node.end_byte())
    {
        return Ok(None);
    }
    let Some(sequence) = expression.child_by_field_name("pipes") else {
        return Ok(None);
    };
    let mut cursor = sequence.walk();
    let mut suppressed = None;
    for (index, pipe) in sequence
        .named_children(&mut cursor)
        .filter(|child| child.kind() == "pipe_call")
        .enumerate()
    {
        let Some(name) = pipe.child_by_field_name("name") else {
            continue;
        };
        if !functions
            .iter()
            .any(|function| text(name, source).is_ok_and(|name| name == function))
        {
            continue;
        }
        let input = if node == expression {
            expression.named_child(0).unwrap_or(node)
        } else {
            node
        };
        if index == 0
            && super::alternatives::collect(input, source, Dialect::AngularExpression, references)?
        {
            continue;
        }

        // The grammar attaches a pipe to the right operand of a calculation.
        // Angular applies it to the complete calculation; earlier pipes also
        // transform the value, so later translation pipes have no known prefix.
        let prefix = if index == 0 {
            literal_prefix(input, source, Dialect::AngularExpression)?.filter(|prefix| {
                prefix.contains('.') && prefix.chars().all(super::super::is_key_char)
            })
        } else {
            None
        };
        let end = pipe
            .prev_named_sibling()
            .map_or(sequence.start_byte(), |operator| operator.start_byte());
        let raw = source
            .get(node.start_byte()..end)
            .ok_or_else(|| io::Error::other("invalid Angular pipe input span"))?;
        let span = node.start_byte()..node.start_byte() + raw.trim_end().len();
        suppressed = Some(span.clone());
        references.dynamic.push(DynamicReference { prefix, span });
    }
    Ok(suppressed)
}

fn is_calculation(node: Node<'_>) -> bool {
    matches!(
        node.kind(),
        "concatenation_expression" | "binary_expression"
    )
}

fn pipe_expression(node: Node<'_>) -> Option<Node<'_>> {
    if node.kind() == "expression" && node.child_by_field_name("pipes").is_some() {
        return Some(node);
    }
    if is_calculation(node) {
        let mut cursor = node.walk();
        return node
            .named_children(&mut cursor)
            .last()
            .and_then(pipe_expression);
    }
    None
}
