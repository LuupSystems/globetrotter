//! Complete key alternatives remain static even when selected at runtime.

use super::{Dialect, References, literal, text, unwrap_expression};
use std::io;
use tree_sitter::Node;

#[cfg(test)]
#[path = "alternative_tests.rs"]
mod tests;

pub(super) fn collect(
    node: Node<'_>,
    source: &str,
    dialect: Dialect,
    references: &mut References,
) -> io::Result<bool> {
    let node = unwrap_expression(node);
    if let Some(value) = literal(node, source, dialect)? {
        references.literals.insert(value);
        return Ok(true);
    }
    let Some((branches, exhaustive)) = branches(node, source, dialect)? else {
        return Ok(false);
    };
    let mut complete = exhaustive && !branches.is_empty();
    for branch in branches {
        complete &= collect(branch, source, dialect, references)?;
    }
    Ok(complete)
}

fn children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|child| !child.kind().contains("comment"))
        .collect()
}

fn fields<'tree>(node: Node<'tree>, names: &[&str]) -> Vec<Node<'tree>> {
    names
        .iter()
        .filter_map(|name| node.child_by_field_name(name))
        .collect()
}

fn conditional_branches(node: Node<'_>, dialect: Dialect) -> Option<Vec<Node<'_>>> {
    Some(match node.kind() {
        "conditional_expression" if dialect == Dialect::Python => [0, 2]
            .into_iter()
            .filter_map(|index| children(node).get(index).copied())
            .collect(),
        "if_expression" if matches!(dialect, Dialect::Kotlin | Dialect::Zig) => children(node)
            .into_iter()
            .filter(|child| {
                Some(*child) != node.child_by_field_name("condition") && child.kind() != "payload"
            })
            .collect(),
        "conditional_expression" if dialect == Dialect::Php => [
            node.child_by_field_name("body")
                .or_else(|| node.child_by_field_name("condition")),
            node.child_by_field_name("alternative"),
        ]
        .into_iter()
        .flatten()
        .collect(),
        "conditional_expression" if dialect == Dialect::AngularExpression => {
            fields(node, &["left", "right"])
        }
        "ternary_expression" if dialect == Dialect::Swift => fields(node, &["if_true", "if_false"]),
        "ternary_expression" | "conditional_expression" | "if_expression" | "conditional" => {
            fields(node, &["consequence", "alternative"])
        }
        _ => return None,
    })
}

fn branches<'tree>(
    node: Node<'tree>,
    source: &str,
    dialect: Dialect,
) -> io::Result<Option<(Vec<Node<'tree>>, bool)>> {
    if let Some(branches) = conditional_branches(node, dialect) {
        let exhaustive = branches.len() == 2;
        return Ok(Some((branches, exhaustive)));
    }
    let branches = match node.kind() {
        "binary_expression" | "binary_operator" | "boolean_operator" | "binary"
            if node
                .child_by_field_name("operator")
                .is_some_and(|operator| {
                    text(operator, source).is_ok_and(|operator| {
                        matches!(operator, "??" | "||" | "&&" | "or" | "and")
                            || (dialect == Dialect::Kotlin && operator == "?:")
                    })
                }) =>
        {
            fields(node, &["left", "right"])
        }
        "nil_coalescing_expression" if dialect == Dialect::Swift => {
            fields(node, &["value", "if_nil"])
        }
        "nullish_coalescing_expression" if dialect == Dialect::AngularExpression => {
            fields(node, &["condition", "default"])
        }
        "if_null_expression" if dialect == Dialect::Dart => children(node),
        "array" | "array_expression" | "list" | "array_literal" | "list_literal" => children(node)
            .into_iter()
            .filter(|child| child.kind() != "type_arguments")
            .collect(),
        "block" if matches!(dialect, Dialect::Rust | Dialect::Kotlin) => {
            children(node).last().copied().into_iter().collect()
        }
        "else_clause" if dialect == Dialect::Rust => node.named_child(0).into_iter().collect(),
        "match_expression" if dialect == Dialect::Rust => {
            node.child_by_field_name("body").into_iter().collect()
        }
        "match_block" if dialect == Dialect::Rust => children(node)
            .into_iter()
            .filter(|child| child.kind() == "match_arm")
            .filter_map(|arm| arm.child_by_field_name("value"))
            .collect(),
        "when_expression" if dialect == Dialect::Kotlin => children(node)
            .into_iter()
            .filter(|child| child.kind() == "when_entry")
            .collect(),
        "when_entry" if dialect == Dialect::Kotlin => {
            children(node).last().copied().into_iter().collect()
        }
        "call" if dialect == Dialect::Elixir => return elixir_branches(node, source),
        _ => return Ok(None),
    };
    Ok(Some((branches, true)))
}

fn elixir_branches<'tree>(
    node: Node<'tree>,
    source: &str,
) -> io::Result<Option<(Vec<Node<'tree>>, bool)>> {
    let Some(target) = node.child_by_field_name("target") else {
        return Ok(None);
    };
    if !matches!(text(target, source)?, "if" | "unless") {
        return Ok(None);
    }
    if let Some(block) = children(node)
        .into_iter()
        .find(|child| child.kind() == "do_block")
    {
        let body = children(block);
        let consequent = body
            .iter()
            .rev()
            .find(|child| child.kind() != "else_block")
            .copied();
        let alternative = body
            .iter()
            .find(|child| child.kind() == "else_block")
            .and_then(|block| children(*block).last().copied());
        let exhaustive = consequent.is_some() && alternative.is_some();
        return Ok(Some((
            [consequent, alternative].into_iter().flatten().collect(),
            exhaustive,
        )));
    }
    let keywords = children(node)
        .into_iter()
        .find(|child| child.kind() == "arguments")
        .and_then(|arguments| {
            children(arguments)
                .into_iter()
                .find(|child| child.kind() == "keywords")
        });
    let Some(keywords) = keywords else {
        return Ok(None);
    };
    let mut branches = Vec::new();
    for name in ["do", "else"] {
        for pair in children(keywords) {
            if let Some(key) = pair.child_by_field_name("key")
                && text(key, source)?.trim().trim_end_matches(':') == name
                && let Some(value) = pair.child_by_field_name("value")
            {
                branches.push(value);
                break;
            }
        }
    }
    let exhaustive = branches.len() == 2;
    Ok(Some((branches, exhaustive)))
}
