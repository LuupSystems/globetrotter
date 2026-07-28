//! Layered run settings and their resolution into settled values.
//!
//! Every source of settings parses into the same [`SettingsLayer`] shape: each
//! config file entry carries one, and callers such as the CLI provide one as
//! overrides. [`Settings::resolve`] merges the layers exactly once, so
//! downstream code only ever sees settled values and cannot forget to consult
//! a layer.

use globetrotter_model::{self as model, diagnostics::Spanned};

/// One layer of run settings, where `None` means "not specified at this layer".
///
/// Keeping "not specified" (`None`) distinguishable from "explicitly disabled"
/// (`Some(false)`) is what makes lower-precedence layers and the built-in
/// defaults reachable during [`Settings::resolve`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct SettingsLayer {
    /// Whether warnings are promoted to errors.
    pub strict: Option<bool>,
    /// Whether to validate that templates render successfully.
    pub check_templates: Option<bool>,
    /// Whether outputs are computed and logged but not written to disk.
    pub dry_run: Option<bool>,
    /// Whether output paths are logged absolute rather than relative to the
    /// common base directory.
    pub print_absolute_paths: Option<bool>,
    /// The template engine used to render translation values.
    pub template_engine: Option<Spanned<model::TemplateEngine>>,
}

/// Settled run settings for one configuration, with every layer merged.
///
/// No field is left optional except [`template_engine`](Self::template_engine),
/// which is genuinely optional in the domain, so "forgot to resolve" is not
/// expressible downstream.
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent settled on/off settings, not a state machine"
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// Whether warnings are promoted to errors.
    pub strict: bool,
    /// Whether to validate that templates render successfully.
    pub check_templates: bool,
    /// Whether outputs are computed and logged but not written to disk.
    pub dry_run: bool,
    /// Whether output paths are logged absolute rather than relative to the
    /// common base directory.
    pub print_absolute_paths: bool,
    /// The template engine used to render translation values, if any.
    pub template_engine: Option<Spanned<model::TemplateEngine>>,
}

impl Settings {
    /// Merge the settings layers for one configuration in precedence order.
    ///
    /// Explicit `overrides` values win over the config file's, and built-in
    /// defaults fill in whatever neither layer specifies: generation is strict,
    /// checks templates, writes its outputs, and logs paths relative to the
    /// common base directory.
    ///
    /// Linting deliberately deviates for `strict`: the config file's value
    /// governs generation only, so only explicit overrides escalate lint
    /// warnings (see [`Executor::lint_config`]).
    ///
    /// [`Executor::lint_config`]: crate::executor::Executor::lint_config
    #[must_use]
    pub fn resolve(config: &SettingsLayer, overrides: &SettingsLayer) -> Self {
        Self {
            strict: overrides.strict.or(config.strict).unwrap_or(true),
            check_templates: overrides
                .check_templates
                .or(config.check_templates)
                .unwrap_or(true),
            dry_run: overrides.dry_run.or(config.dry_run).unwrap_or(false),
            print_absolute_paths: overrides
                .print_absolute_paths
                .or(config.print_absolute_paths)
                .unwrap_or(false),
            template_engine: overrides
                .template_engine
                .clone()
                .or_else(|| config.template_engine.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Settings, SettingsLayer};
    use globetrotter_model::{TemplateEngine, diagnostics::Spanned};
    use similar_asserts::assert_eq as sim_assert_eq;

    /// With neither layer set, the built-in generation defaults apply.
    #[test]
    fn defaults_apply_when_no_layer_specifies_a_value() {
        let settings = Settings::resolve(&SettingsLayer::default(), &SettingsLayer::default());
        sim_assert_eq!(
            have: settings,
            want: Settings {
                strict: true,
                check_templates: true,
                dry_run: false,
                print_absolute_paths: false,
                template_engine: None,
            }
        );
    }

    /// The config file's values are reachable when no override is given.
    #[test]
    fn config_layer_is_reachable_without_overrides() {
        let config = SettingsLayer {
            strict: Some(false),
            check_templates: Some(false),
            dry_run: Some(true),
            print_absolute_paths: Some(true),
            template_engine: Some(Spanned::dummy(TemplateEngine::Jinja2)),
        };
        let settings = Settings::resolve(&config, &SettingsLayer::default());
        sim_assert_eq!(
            have: settings,
            want: Settings {
                strict: false,
                check_templates: false,
                dry_run: true,
                print_absolute_paths: true,
                template_engine: Some(Spanned::dummy(TemplateEngine::Jinja2)),
            }
        );
    }

    /// Explicit overrides win over the config file, including explicit `false`
    /// over a config `true` (the regression the tri-state layers exist for).
    #[test]
    fn overrides_win_over_config_in_both_directions() {
        let config = SettingsLayer {
            strict: Some(true),
            check_templates: Some(true),
            dry_run: Some(false),
            print_absolute_paths: Some(false),
            template_engine: Some(Spanned::dummy(TemplateEngine::Jinja2)),
        };
        let overrides = SettingsLayer {
            strict: Some(false),
            check_templates: Some(false),
            dry_run: Some(true),
            print_absolute_paths: Some(true),
            template_engine: Some(Spanned::dummy(TemplateEngine::Handlebars)),
        };
        let settings = Settings::resolve(&config, &overrides);
        sim_assert_eq!(
            have: settings,
            want: Settings {
                strict: false,
                check_templates: false,
                dry_run: true,
                print_absolute_paths: true,
                template_engine: Some(Spanned::dummy(TemplateEngine::Handlebars)),
            }
        );
    }
}
