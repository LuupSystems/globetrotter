/// ColorChoice represents the color preferences of an end user.
///
/// The `Default` implementation for this type will select `Auto`, which tries
/// to do the right thing based on the current environment.
///
/// The `FromStr` implementation for this type converts a lowercase kebab-case
/// string of the variant name to the corresponding variant. Any other string
/// results in an error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorChoice {
    /// Try very hard to emit colors. This includes emitting ANSI colors
    /// on Windows if the console API is unavailable.
    Always,
    /// AlwaysAnsi is like Always, except it never tries to use anything other
    /// than emitting ANSI color codes.
    AlwaysAnsi,
    /// Try to use colors, but don't force the issue. If the console isn't
    /// available on Windows, or if TERM=dumb, or if `NO_COLOR` is defined, for
    /// example, then don't use colors.
    Auto,
    /// Never emit colors.
    Never,
}

/// The default is `Auto`.
impl Default for ColorChoice {
    fn default() -> ColorChoice {
        ColorChoice::Auto
    }
}

impl std::str::FromStr for ColorChoice {
    type Err = ColorChoiceParseError;

    fn from_str(s: &str) -> Result<ColorChoice, ColorChoiceParseError> {
        match s.to_lowercase().as_str() {
            "always" => Ok(ColorChoice::Always),
            "always-ansi" => Ok(ColorChoice::AlwaysAnsi),
            "never" => Ok(ColorChoice::Never),
            "auto" => Ok(ColorChoice::Auto),
            unknown => Err(ColorChoiceParseError {
                unknown_choice: unknown.to_string(),
            }),
        }
    }
}

impl ColorChoice {
    /// Returns true if we should attempt to write colored output.
    fn should_attempt_color(&self) -> bool {
        match *self {
            ColorChoice::Always => true,
            ColorChoice::AlwaysAnsi => true,
            ColorChoice::Never => false,
            ColorChoice::Auto => self.env_allows_color(),
        }
    }

    #[cfg(not(windows))]
    fn env_allows_color(&self) -> bool {
        match std::env::var_os("TERM") {
            // If TERM isn't set, then we are in a weird environment that
            // probably doesn't support colors.
            None => return false,
            Some(k) => {
                if k == "dumb" {
                    return false;
                }
            }
        }
        // If TERM != dumb, then the only way we don't allow colors at this
        // point is if NO_COLOR is set.
        if std::env::var_os("NO_COLOR").is_some() {
            return false;
        }
        true
    }

    #[cfg(windows)]
    fn env_allows_color(&self) -> bool {
        // On Windows, if TERM isn't set, then we shouldn't automatically
        // assume that colors aren't allowed. This is unlike Unix environments
        // where TERM is more rigorously set.
        if let Some(k) = env::var_os("TERM") {
            if k == "dumb" {
                return false;
            }
        }
        // If TERM != dumb, then the only way we don't allow colors at this
        // point is if NO_COLOR is set.
        if env::var_os("NO_COLOR").is_some() {
            return false;
        }
        true
    }

    /// Returns true if this choice should forcefully use ANSI color codes.
    ///
    /// It's possible that ANSI is still the correct choice even if this
    /// returns false.
    #[cfg(windows)]
    fn should_ansi(&self) -> bool {
        match *self {
            ColorChoice::Always => false,
            ColorChoice::AlwaysAnsi => true,
            ColorChoice::Never => false,
            ColorChoice::Auto => {
                match env::var("TERM") {
                    Err(_) => false,
                    // cygwin doesn't seem to support ANSI escape sequences
                    // and instead has its own variety. However, the Windows
                    // console API may be available.
                    Ok(k) => k != "dumb" && k != "cygwin",
                }
            }
        }
    }
}

/// An error that occurs when parsing a `ColorChoice` fails.
#[derive(Clone, Debug)]
pub struct ColorChoiceParseError {
    unknown_choice: String,
}

impl std::error::Error for ColorChoiceParseError {}

impl std::fmt::Display for ColorChoiceParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "unrecognized color choice '{}': valid choices are: \
             always, always-ansi, never, auto",
            self.unknown_choice,
        )
    }
}
