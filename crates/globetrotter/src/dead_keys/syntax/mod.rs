//! Syntax-aware scanning includes scripts and expressions embedded in web templates.

use super::source::{Dialect, DynamicReference, References};
use literals::{constructs_string, first_interpolation, literal, literal_prefix};
use web::visit_template;

mod angular;
mod css;
#[cfg(test)]
mod lexical_tests;
mod literals;
#[cfg(test)]
mod tests;
mod web;
use std::io;
use std::path::Path;
use tree_sitter::{Language, Node, Parser, Range};

impl Dialect {
    fn language(self) -> Language {
        match self {
            Self::Javascript => tree_sitter_javascript::LANGUAGE.into(),
            Self::Typescript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Self::AngularExpression => tree_sitter_angular::language(),
            Self::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
            Self::Go => tree_sitter_go::LANGUAGE.into(),
            Self::Python => tree_sitter_python::LANGUAGE.into(),
            Self::Ruby => tree_sitter_ruby::LANGUAGE.into(),
            Self::Php => tree_sitter_php::LANGUAGE_PHP.into(),
            Self::Java => tree_sitter_java::LANGUAGE.into(),
            Self::Kotlin => tree_sitter_kotlin_ng::LANGUAGE.into(),
            Self::Swift => tree_sitter_swift::LANGUAGE.into(),
            Self::Dart => tree_sitter_dart::LANGUAGE.into(),
            Self::Elixir => tree_sitter_elixir::LANGUAGE.into(),
            Self::Lua => tree_sitter_lua::LANGUAGE.into(),
            Self::Zig => tree_sitter_zig::LANGUAGE.into(),
            Self::CSharp => tree_sitter_c_sharp::LANGUAGE.into(),
            Self::Html => tree_sitter_html::LANGUAGE.into(),
            Self::Vue => tree_sitter_vue_next::LANGUAGE.into(),
            Self::Svelte => tree_sitter_svelte_next::LANGUAGE.into(),
            Self::Astro => tree_sitter_astro_next::LANGUAGE.into(),
        }
    }

    fn is_web_template(self) -> bool {
        matches!(self, Self::Html | Self::Vue | Self::Svelte | Self::Astro)
    }
}

pub(super) fn scan(path: &Path, content: &str, functions: &[String]) -> io::Result<References> {
    let mut references = References::default();
    if let Some(dialect) = Dialect::for_path(path) {
        parse_region(content, dialect, None, functions, &mut references)?;
    }
    Ok(references)
}

#[derive(Debug)]
struct SyntaxError {
    offset: usize,
    row: usize,
    column: usize,
}

impl SyntaxError {
    fn at(source: &str, offset: usize) -> Self {
        let before = source.get(..offset).unwrap_or("");
        Self {
            offset,
            row: before.bytes().filter(|byte| *byte == b'\n').count() + 1,
            column: before.rsplit('\n').next().unwrap_or("").len() + 1,
        }
    }
}

impl std::fmt::Display for SyntaxError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "cannot reliably scan source with a syntax error at line {}, column {}",
            self.row, self.column
        )
    }
}

impl std::error::Error for SyntaxError {}

fn parse_region(
    source: &str,
    dialect: Dialect,
    range: Option<Range>,
    functions: &[String],
    references: &mut References,
) -> io::Result<()> {
    let mut parser = Parser::new();
    parser
        .set_language(&dialect.language())
        .map_err(io::Error::other)?;
    if let Some(range) = range {
        parser
            .set_included_ranges(&[range])
            .map_err(io::Error::other)?;
    }
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| io::Error::other("source parsing was cancelled"))?;
    // Included ranges keep every candidate's offsets in the original file,
    // including calls inside script blocks and template attributes.
    visit(tree.root_node(), source, dialect, functions, references)
}

fn text<'a>(node: Node<'_>, source: &'a str) -> io::Result<&'a str> {
    node.utf8_text(source.as_bytes()).map_err(io::Error::other)
}

fn is_non_runtime(node: Node<'_>, source: &str, dialect: Dialect) -> bool {
    if matches!(node.kind(), "string" | "string_literal")
        && node.parent().is_some_and(|parent| {
            matches!(parent.kind(), "import_statement" | "export_statement")
                && parent.child_by_field_name("source") == Some(node)
        })
    {
        return true;
    }
    if dialect == Dialect::Rust
        && matches!(node.kind(), "attribute_item" | "inner_attribute_item")
        && node
            .named_child(0)
            .and_then(|attribute| attribute.named_child(0))
            .is_some_and(|name| text(name, source).is_ok_and(|name| name == "doc"))
    {
        return true;
    }
    if dialect == Dialect::Elixir
        && node.kind() == "unary_operator"
        && node
            .child_by_field_name("operator")
            .is_some_and(|operator| operator.kind() == "@")
        && node
            .child_by_field_name("operand")
            .and_then(|operand| operand.child_by_field_name("target"))
            .is_some_and(|target| {
                text(target, source).is_ok_and(|name| {
                    matches!(
                        name,
                        "doc" | "moduledoc" | "typedoc" | "spec" | "type" | "typep" | "callback"
                    )
                })
            })
    {
        return true;
    }

    let kind = node.kind();
    if kind.contains("comment") || matches!(kind, "regex" | "regex_pattern") {
        return true;
    }
    if matches!(
        dialect,
        Dialect::Typescript | Dialect::Tsx | Dialect::AngularExpression
    ) {
        return kind.ends_with("_type")
            || matches!(
                kind,
                "type_annotation"
                    | "type_alias_declaration"
                    | "interface_declaration"
                    | "type_arguments"
                    | "type_parameters"
                    | "type_query"
                    | "ambient_declaration"
                    | "import_statement"
            );
    }
    dialect == Dialect::Python
        && kind == "expression_statement"
        && node.named_child(0).is_some_and(|child| {
            let child = unwrap_expression(child);
            matches!(child.kind(), "string" | "concatenated_string")
                && first_interpolation(child).is_none()
        })
}

fn visit(
    node: Node<'_>,
    source: &str,
    dialect: Dialect,
    functions: &[String],
    references: &mut References,
) -> io::Result<()> {
    if is_non_runtime(node, source, dialect) {
        return Ok(());
    }
    if node.is_error() || node.is_missing() {
        return Err(io::Error::other(SyntaxError::at(source, node.start_byte())));
    }
    if dialect.is_web_template() {
        return visit_template(node, source, dialect, functions, references);
    }

    if dialect == Dialect::AngularExpression {
        angular::classify_pipes(node, source, functions, references)?;
    }
    if let Some(argument) = translation_argument(node, source, dialect, functions)?
        && constructs_string(argument, source, dialect)?
    {
        references.dynamic.push(DynamicReference {
            prefix: literal_prefix(argument, source, dialect)?
                .filter(|prefix| prefix.contains('.') && prefix.chars().all(super::is_key_char)),
            span: argument.byte_range(),
        });
    }
    // Literal evidence stands on its own, including inside opaque call arguments.
    if let Some(value) = literal(node, source, dialect)? {
        references.literals.insert(value);
        return Ok(());
    }
    if dialect == Dialect::Rust
        && matches!(node.kind(), "identifier" | "struct_expression")
        && let Some(identifier) = rust_identifier(node, source)?
    {
        references.identifiers.insert(identifier);
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        visit(child, source, dialect, functions, references)?;
    }
    Ok(())
}

fn rust_identifier(node: Node<'_>, source: &str) -> io::Result<Option<String>> {
    let node = unwrap_expression(node);
    match node.kind() {
        "identifier" | "type_identifier" => Ok(Some(text(node, source)?.to_owned())),
        "scoped_identifier" | "scoped_type_identifier" | "struct_expression" => {
            match node.child_by_field_name("name") {
                Some(name) => rust_identifier(name, source),
                None => Ok(None),
            }
        }
        _ => Ok(None),
    }
}

fn translation_argument<'a>(
    node: Node<'a>,
    source: &str,
    dialect: Dialect,
    functions: &[String],
) -> io::Result<Option<Node<'a>>> {
    if !matches!(
        node.kind(),
        "call_expression"
            | "call"
            | "function_call"
            | "function_call_expression"
            | "member_call_expression"
            | "scoped_call_expression"
            | "nullsafe_member_call_expression"
            | "method_invocation"
            | "invocation_expression"
    ) {
        return Ok(None);
    }
    let callee = node
        .child_by_field_name("function")
        .or_else(|| node.child_by_field_name("method"))
        .or_else(|| node.child_by_field_name("name"))
        .or_else(|| node.named_child(0));
    let Some(callee) = callee.map(unwrap_expression) else {
        return Ok(None);
    };
    let mut callee = text(callee, source)?.to_owned();
    if dialect == Dialect::AngularExpression
        && let Some(object) = node
            .parent()
            .filter(|parent| {
                parent.kind() == "member_expression"
                    && parent.child_by_field_name("call") == Some(node)
            })
            .and_then(|parent| parent.child_by_field_name("object"))
    {
        callee = format!("{}.{callee}", text(object, source)?);
    }
    let callee: String = callee.chars().filter(|c| !c.is_whitespace()).collect();
    let callee = callee.replace("?.", ".");
    if !functions.iter().any(|function| {
        function == &callee
            || (!function.contains('.')
                && callee.rsplit(['.', ':']).next() == Some(function.as_str()))
    }) {
        return Ok(None);
    }
    let mut cursor = node.walk();
    let arguments = node.child_by_field_name("arguments").or_else(|| {
        node.named_children(&mut cursor).find(|child| {
            matches!(
                child.kind(),
                "arguments" | "argument_list" | "value_arguments" | "call_suffix"
            )
        })
    });
    if dialect == Dialect::Zig && arguments.is_none() {
        let mut cursor = node.walk();
        return Ok(node
            .named_children(&mut cursor)
            .skip(1)
            .find(|child| !child.kind().contains("comment"))
            .map(unwrap_expression));
    }
    Ok(arguments
        .and_then(|arguments| {
            if arguments.kind() == "template_string" {
                return Some(arguments);
            }
            let mut cursor = arguments.walk();
            arguments
                .named_children(&mut cursor)
                .find(|child| !child.kind().contains("comment"))
        })
        .map(unwrap_expression))
}

fn unwrap_expression(mut node: Node<'_>) -> Node<'_> {
    loop {
        let inner = match node.kind() {
            "type_assertion" | "value_argument" | "argument" => {
                let mut cursor = node.walk();
                node.child_by_field_name("value").or_else(|| {
                    node.named_children(&mut cursor)
                        .filter(|child| !child.kind().contains("comment"))
                        .last()
                })
            }
            "keyword_argument"
            | "cast_expression"
            | "type_cast_expression"
            | "reference_expression" => node
                .child_by_field_name("value")
                .or_else(|| node.child_by_field_name("expression"))
                .or_else(|| node.named_child(0)),
            "parenthesized_expression"
            | "as_expression"
            | "satisfies_expression"
            | "non_null_expression"
            | "value_arguments"
            | "group" => node.named_child(0),
            "expression" if node.child_by_field_name("pipes").is_none() => node.named_child(0),
            _ => None,
        };
        let Some(inner) = inner else { return node };
        node = inner;
    }
}
