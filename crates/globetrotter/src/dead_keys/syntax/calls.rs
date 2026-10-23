//! Template arguments of translation calls, where the call shape exposes them statically.
//!
//! Typst calls pass arguments by name, JavaScript-family calls pass an object literal as the
//! second argument, and Python calls pass keyword arguments.
//! Every other language, and any call whose arguments are spread or computed, reports an opaque
//! argument set so only its key is checked.

use super::super::source::{ArgumentValue, CallArgument};
use super::{Dialect, constructs_string, literal, text, unwrap_expression};
use std::io;
use tree_sitter::Node;

/// Returns the named arguments of `call`, or `None` when they are not statically visible.
///
/// `key` is the call's key argument, which is not a template argument.
pub(super) fn arguments(
    call: Node<'_>,
    key: Node<'_>,
    source: &str,
    dialect: Dialect,
) -> io::Result<Option<Vec<CallArgument>>> {
    match dialect {
        Dialect::Typst => typst(call, key, source),
        Dialect::Javascript | Dialect::Typescript | Dialect::Tsx => javascript(call, key, source),
        Dialect::Python => python(call, key, source),
        _ => Ok(None),
    }
}

fn contains(outer: Node<'_>, inner: Node<'_>) -> bool {
    outer.start_byte() <= inner.start_byte() && inner.end_byte() <= outer.end_byte()
}

fn non_comment_children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|child| !child.kind().contains("comment"))
        .collect()
}

fn value(node: Node<'_>, source: &str, dialect: Dialect) -> io::Result<ArgumentValue> {
    let node = unwrap_expression(node);
    if literal(node, source, dialect)?.is_some() || constructs_string(node, source, dialect)? {
        return Ok(ArgumentValue::String);
    }
    Ok(match node.kind() {
        "number" | "integer" | "float" => ArgumentValue::Number,
        "bool" | "true" | "false" => ArgumentValue::Boolean,
        _ => ArgumentValue::Other,
    })
}

/// Typst passes template arguments by name: `t("key", name: value)`.
fn typst(call: Node<'_>, key: Node<'_>, source: &str) -> io::Result<Option<Vec<CallArgument>>> {
    // `t("key")[body]` parses as an outer call whose callee is `t("key")`.
    if call.parent().is_some_and(|parent| {
        parent.kind() == "call" && parent.child_by_field_name("item") == Some(call)
    }) {
        return Ok(None);
    }
    let callee = call.child_by_field_name("item");
    let mut groups = non_comment_children(call)
        .into_iter()
        .filter(|child| Some(*child) != callee);
    // A trailing content block is a positional argument, which the runtime rejects.
    let (Some(group), None) = (groups.next(), groups.next()) else {
        return Ok(None);
    };
    if group.kind() != "group" {
        return Ok(None);
    }
    let mut arguments = Vec::new();
    for child in non_comment_children(group) {
        if contains(child, key) {
            continue;
        }
        // Positional values and `..spread` arguments make the argument set opaque.
        if child.kind() != "tagged" {
            return Ok(None);
        }
        let (Some(name), Some(argument)) = (
            child.child_by_field_name("field"),
            non_comment_children(child).last().copied(),
        ) else {
            return Ok(None);
        };
        arguments.push(CallArgument {
            name: text(name, source)?.to_owned(),
            span: child.byte_range(),
            value: value(argument, source, Dialect::Typst)?,
        });
    }
    Ok(Some(arguments))
}

/// JavaScript-family calls pass template arguments as an object literal after the key:
/// `t("key", { name: value })`.
fn javascript(
    call: Node<'_>,
    key: Node<'_>,
    source: &str,
) -> io::Result<Option<Vec<CallArgument>>> {
    let Some(list) = call.child_by_field_name("arguments") else {
        return Ok(None);
    };
    if list.kind() != "arguments" {
        return Ok(None);
    }
    let positional = non_comment_children(list);
    let Some(first) = positional.first() else {
        return Ok(None);
    };
    if !contains(*first, key) {
        return Ok(None);
    }
    let Some(object) = positional.get(1).map(|node| unwrap_expression(*node)) else {
        return Ok(Some(Vec::new()));
    };
    if object.kind() != "object" {
        return Ok(None);
    }
    let mut arguments = Vec::new();
    for property in non_comment_children(object) {
        let (name, argument) = match property.kind() {
            "pair" => {
                let (Some(name), Some(argument)) = (
                    property.child_by_field_name("key"),
                    property.child_by_field_name("value"),
                ) else {
                    return Ok(None);
                };
                let name = match name.kind() {
                    "property_identifier" | "number" => text(name, source)?.to_owned(),
                    "string" => match literal(name, source, Dialect::Javascript)? {
                        Some(name) => name,
                        None => return Ok(None),
                    },
                    // Computed property names are opaque.
                    _ => return Ok(None),
                };
                (name, value(argument, source, Dialect::Javascript)?)
            }
            "shorthand_property_identifier" => {
                (text(property, source)?.to_owned(), ArgumentValue::Other)
            }
            "method_definition" => match property.child_by_field_name("name") {
                Some(name) if name.kind() == "property_identifier" => {
                    (text(name, source)?.to_owned(), ArgumentValue::Other)
                }
                _ => return Ok(None),
            },
            // Spreads and anything else hide the complete argument set.
            _ => return Ok(None),
        };
        arguments.push(CallArgument {
            name,
            span: property.byte_range(),
            value: argument,
        });
    }
    Ok(Some(arguments))
}

/// Python passes template arguments as keyword arguments: `t("key", name=value)`.
fn python(call: Node<'_>, key: Node<'_>, source: &str) -> io::Result<Option<Vec<CallArgument>>> {
    let Some(list) = call.child_by_field_name("arguments") else {
        return Ok(None);
    };
    if list.kind() != "argument_list" {
        return Ok(None);
    }
    let mut arguments = Vec::new();
    for child in non_comment_children(list) {
        if contains(child, key) {
            continue;
        }
        // Extra positional values and `*args` or `**kwargs` make the argument set opaque.
        if child.kind() != "keyword_argument" {
            return Ok(None);
        }
        let (Some(name), Some(argument)) = (
            child.child_by_field_name("name"),
            child.child_by_field_name("value"),
        ) else {
            return Ok(None);
        };
        arguments.push(CallArgument {
            name: text(name, source)?.to_owned(),
            span: child.byte_range(),
            value: value(argument, source, Dialect::Python)?,
        });
    }
    Ok(Some(arguments))
}
