//! Custom Handlebars helpers used by the Rust example.

use handlebars::{
    Context, Handlebars, Helper, HelperResult, Output, RenderContext, RenderError,
    RenderErrorReason,
};

fn param_not_found(helper_name: &'static str, index: usize) -> RenderError {
    RenderError::from(RenderErrorReason::ParamNotFoundForIndex(helper_name, index))
}

fn invalid_param_type(
    helper_name: &'static str,
    param_name: &str,
    expected_type: &str,
) -> RenderError {
    RenderError::from(RenderErrorReason::ParamTypeMismatchForName(
        helper_name,
        param_name.to_string(),
        expected_type.to_string(),
    ))
}

/// Name of the registered Handlebars helper used for pluralization.
pub const PLURALIZE_HELPER_NAME: &str = "pluralize";

/// Pluralizes a string based on a numeric count.
///
/// The first parameter is the value to pluralize, the second parameter is
/// the numeric count determining whether the singular or plural form is
/// emitted.
///
/// # Errors
///
/// Returns an error if the required parameters are missing or if rendering fails.
pub fn pluralize(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let value = h
        .param(0)
        .ok_or(param_not_found(PLURALIZE_HELPER_NAME, 0))?
        .value();
    let value =
        value
            .as_str()
            .ok_or(invalid_param_type(PLURALIZE_HELPER_NAME, "value", "string"))?;

    let count = h
        .param(1)
        .ok_or(param_not_found(PLURALIZE_HELPER_NAME, 1))?
        .value();
    let count = count
        .as_number()
        .and_then(serde_json::Number::as_i64)
        .ok_or(invalid_param_type(PLURALIZE_HELPER_NAME, "count", "number"))?;

    if count == 1 {
        out.write(value)?;
    } else {
        out.write(&format!("{value}s"))?;
    }
    Ok(())
}
