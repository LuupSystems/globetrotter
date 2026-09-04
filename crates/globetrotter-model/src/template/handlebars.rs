//! Handlebars analysis, walking the `handlebars` crate's own syntax tree.
//!
//! Names are classified by where they appear: parameters of a conditional
//! block select a wording, everything else reaches the output.

use super::{Analysis, CompileError};
use handlebars::template::{BlockParam, HelperTemplate, Parameter, Template, TemplateElement};
use handlebars::{Path, PathSeg};

/// Analyzes a Handlebars `source`.
///
/// The error carries only the parser's reason, not its multi-line rendering
/// with a source excerpt, since a diagnostic already shows the source.
pub(super) fn analyze(source: &str) -> Result<Analysis, CompileError> {
    let template = Template::compile(source).map_err(|error| CompileError {
        message: error.reason().to_string(),
    })?;
    let mut walk = Walk::default();
    walk.elements(&template.elements, Role::Substituted);
    Ok(walk.analysis)
}

/// Block helpers whose parameters select a wording instead of reaching the
/// output: `if`, `unless`, and the comparison helpers in block form.
const CONDITION_HELPERS: &[&str] = &[
    "if", "unless", "eq", "ne", "gt", "gte", "lt", "lte", "and", "or", "not",
];

/// What a referenced name is used for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Substituted,
    Condition,
}

#[derive(Default)]
struct Walk {
    analysis: Analysis,
    /// Block parameters (`as |x|`) in scope, which shadow outer names.
    locals: Vec<String>,
}

impl Walk {
    fn elements(&mut self, elements: &[TemplateElement], role: Role) {
        for element in elements {
            match element {
                TemplateElement::Expression(helper) | TemplateElement::HtmlExpression(helper) => {
                    self.helper(helper, role);
                }
                TemplateElement::HelperBlock(helper) => self.block(helper),
                _ => {}
            }
        }
    }

    fn block(&mut self, helper: &HelperTemplate) {
        let role = if is_condition_block(helper) {
            Role::Condition
        } else {
            Role::Substituted
        };
        if role == Role::Condition && branches_identical(helper) {
            self.analysis.dead_conditions.push(block_text(helper));
        }
        self.helper(helper, role);
    }

    /// Records the helper's parameters under `role`, then walks its branches.
    /// Branch contents reach the output whichever way the block was selected,
    /// so they are always substituted.
    fn helper(&mut self, helper: &HelperTemplate, role: Role) {
        self.parameter(&helper.name, role);
        for parameter in &helper.params {
            self.parameter(parameter, role);
        }
        for parameter in helper.hash.values() {
            self.parameter(parameter, role);
        }

        let depth = self.locals.len();
        match &helper.block_param {
            Some(BlockParam::Single(Parameter::Name(name))) => self.locals.push(name.clone()),
            Some(BlockParam::Pair((first, second))) => {
                for parameter in [first, second] {
                    if let Parameter::Name(name) = parameter {
                        self.locals.push(name.clone());
                    }
                }
            }
            _ => {}
        }
        if let Some(template) = &helper.template {
            self.elements(&template.elements, Role::Substituted);
        }
        if let Some(template) = &helper.inverse {
            self.elements(&template.elements, Role::Substituted);
        }
        self.locals.truncate(depth);
    }

    fn parameter(&mut self, parameter: &Parameter, role: Role) {
        match parameter {
            Parameter::Path(Path::Relative((segments, _))) => {
                if let Some(PathSeg::Named(first)) = segments.first()
                    && first != "this"
                    && !self.locals.iter().any(|local| local == first)
                {
                    let names = match role {
                        Role::Substituted => &mut self.analysis.substituted,
                        Role::Condition => &mut self.analysis.conditions,
                    };
                    names.insert(first.clone());
                }
            }
            Parameter::Subexpression(subexpression) => {
                self.elements(std::slice::from_ref(subexpression.element.as_ref()), role);
            }
            _ => {}
        }
    }
}

fn is_condition_block(helper: &HelperTemplate) -> bool {
    matches!(&helper.name, Parameter::Name(name) if CONDITION_HELPERS.contains(&name.as_str()))
}

/// Compares the branches by their elements only, since a template's line and
/// column mapping differs between them even when the text is the same.
fn branches_identical(helper: &HelperTemplate) -> bool {
    branch_elements(helper.template.as_ref()) == branch_elements(helper.inverse.as_ref())
}

fn branch_elements(template: Option<&Template>) -> &[TemplateElement] {
    template.map_or(&[], |template| template.elements.as_slice())
}

/// The block's opening tag as written, e.g. `{{#if my_condition}}`.
fn block_text(helper: &HelperTemplate) -> String {
    format!("{{{{#{}}}}}", helper_text(helper))
}

fn helper_text(helper: &HelperTemplate) -> String {
    let mut parts = vec![parameter_text(&helper.name)];
    parts.extend(helper.params.iter().map(parameter_text));
    // Hash entries have no stable order in the AST, so sort them for display.
    let mut hash: Vec<String> = helper
        .hash
        .iter()
        .map(|(key, value)| format!("{key}={}", parameter_text(value)))
        .collect();
    hash.sort_unstable();
    parts.extend(hash);
    parts.join(" ")
}

fn parameter_text(parameter: &Parameter) -> String {
    match parameter {
        Parameter::Name(name) => name.clone(),
        Parameter::Path(Path::Relative((_, raw)) | Path::Local((_, _, raw))) => raw.clone(),
        Parameter::Literal(json) => json.to_string(),
        Parameter::Subexpression(subexpression) => match subexpression.element.as_ref() {
            TemplateElement::Expression(helper)
            | TemplateElement::HtmlExpression(helper)
            | TemplateElement::HelperBlock(helper) => format!("({})", helper_text(helper)),
            _ => String::new(),
        },
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::{Analysis, analyze};
    use color_eyre::eyre::{self, OptionExt};
    use similar_asserts::assert_eq as sim_assert_eq;
    use std::collections::BTreeSet;

    fn handlebars(source: &str) -> eyre::Result<Analysis> {
        Ok(analyze(source)?)
    }

    fn names(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(ToString::to_string).collect()
    }

    /// Interpolations and helper arguments reach the output.
    #[test_util::test]
    fn interpolations_are_substituted() -> eyre::Result<()> {
        sim_assert_eq!(have: handlebars("{{name}}")?.substituted, want: names(&["name"]));
        sim_assert_eq!(have: handlebars("{{uppercase name}}")?.substituted, want: names(&["name"]));
        sim_assert_eq!(
            have: handlebars("Hello {{name}}, you are {{age}} years old.")?.substituted,
            want: names(&["age", "name"])
        );
        sim_assert_eq!(have: handlebars("just text")?, want: Analysis::default());
        Ok(())
    }

    /// Block parameters, `this`, and `@`-variables are not template arguments.
    #[test_util::test]
    fn excludes_block_locals_and_this() -> eyre::Result<()> {
        sim_assert_eq!(
            have: handlebars("{{#each items}}{{this}} {{@index}}{{/each}}")?.substituted,
            want: names(&["items"])
        );
        sim_assert_eq!(
            have: handlebars("{{#each rows as |row|}}{{row}}{{/each}}")?.substituted,
            want: names(&["rows"])
        );
        Ok(())
    }

    /// A name that only selects a wording is a condition, not a substitution,
    /// even through a comparison subexpression; a name used both ways counts
    /// as substituted too.
    #[test_util::test]
    fn conditions_are_kept_apart_from_substitutions() -> eyre::Result<()> {
        let analysis = handlebars("{{#if my_condition}}one{{else}}other{{/if}} {{name}}")?;
        sim_assert_eq!(have: analysis.conditions, want: names(&["my_condition"]));
        sim_assert_eq!(have: analysis.substituted, want: names(&["name"]));
        sim_assert_eq!(have: analysis.variables(), want: names(&["my_condition", "name"]));

        let analysis = handlebars(r#"{{#unless (eq status "open")}}closed{{/unless}}"#)?;
        sim_assert_eq!(have: analysis.conditions, want: names(&["status"]));
        assert!(analysis.substituted.is_empty());

        let analysis = handlebars("{{#if name}}Hi {{name}}{{/if}}")?;
        sim_assert_eq!(have: analysis.conditions, want: names(&["name"]));
        sim_assert_eq!(have: analysis.substituted, want: names(&["name"]));
        Ok(())
    }

    /// `each` and `with` put their value in the output, so their parameter is
    /// substituted rather than a condition.
    #[test_util::test]
    fn iteration_is_substitution() -> eyre::Result<()> {
        let analysis =
            handlebars("{{#each items}}{{label}}{{/each}}{{#with user}}{{first}}{{/with}}")?;
        sim_assert_eq!(
            have: analysis.substituted,
            want: names(&["first", "items", "label", "user"])
        );
        assert!(analysis.conditions.is_empty());
        Ok(())
    }

    /// A condition with identical branches, both empty included, is dead;
    /// differing branches are not, and nested blocks are inspected too.
    #[test_util::test]
    fn dead_conditions_have_identical_branches() -> eyre::Result<()> {
        sim_assert_eq!(
            have: handlebars("{{#if my_condition}}{{/if}}Hello")?.dead_conditions,
            want: vec!["{{#if my_condition}}".to_string()]
        );
        sim_assert_eq!(
            have: handlebars("{{#unless a}}same{{else}}same{{/unless}}")?.dead_conditions,
            want: vec!["{{#unless a}}".to_string()]
        );
        sim_assert_eq!(
            have: handlebars(r#"{{#if (eq status "open")}}{{/if}}"#)?.dead_conditions,
            want: vec![r#"{{#if (eq status "open")}}"#.to_string()]
        );
        sim_assert_eq!(
            have: handlebars("{{#each items}}{{#if flag}}{{/if}}{{/each}}")?.dead_conditions,
            want: vec!["{{#if flag}}".to_string()]
        );
        assert!(
            handlebars("{{#if a}}yes{{else}}no{{/if}}")?
                .dead_conditions
                .is_empty()
        );
        assert!(
            handlebars("{{#if a}}yes{{/if}}")?
                .dead_conditions
                .is_empty()
        );
        // An empty loop body is not a condition and is left alone.
        assert!(
            handlebars("{{#each items}}{{/each}}")?
                .dead_conditions
                .is_empty()
        );
        Ok(())
    }

    /// A rejected template reports the parser's one-line reason.
    #[test_util::test]
    fn invalid_template_reports_the_reason() {
        let error = analyze("{{unclosed")
            .err()
            .ok_or_eyre("template compiled")?;
        assert!(
            error.message.starts_with("invalid handlebars syntax"),
            "{error}"
        );
        assert!(!error.message.contains('\n'), "{error}");
        assert!(analyze("{{#each}}").is_err());
    }
}
