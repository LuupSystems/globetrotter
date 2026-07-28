//! Terminal color policy and environment detection.

/// A user's preference for colored terminal output.
///
/// [`Self::Auto`] is the default. Parsing accepts the lowercase names
/// `always`, `always-ansi`, `auto`, and `never`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorChoice {
    /// Forces color, falling back to ANSI escapes when the Windows console API
    /// is unavailable.
    Always,
    /// Forces ANSI escape sequences without using the Windows console API.
    AlwaysAnsi,
    /// Enables color when the terminal environment supports it.
    ///
    /// `TERM=dumb` and `NO_COLOR` disable color. On Unix, an absent `TERM`
    /// also disables color.
    Auto,
    /// Disables color.
    Never,
}

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
            // Unix terminals normally set `TERM`, so its absence is not enough
            // evidence that ANSI escapes are supported.
            None => return false,
            Some(k) => {
                if k == "dumb" {
                    return false;
                }
            }
        }
        // Honor the cross-platform opt-out after terminal capability checks.
        if std::env::var_os("NO_COLOR").is_some() {
            return false;
        }
        true
    }

    #[cfg(windows)]
    fn env_allows_color(&self) -> bool {
        // Windows consoles commonly omit `TERM`, unlike Unix terminals.
        if let Some(k) = env::var_os("TERM") {
            if k == "dumb" {
                return false;
            }
        }
        // Honor the cross-platform opt-out after terminal capability checks.
        if env::var_os("NO_COLOR").is_some() {
            return false;
        }
        true
    }

    /// Returns `true` when this choice requires ANSI escape sequences.
    ///
    /// ANSI may still be selected when this returns `false`.
    #[cfg(windows)]
    fn should_ansi(&self) -> bool {
        match *self {
            ColorChoice::Always => false,
            ColorChoice::AlwaysAnsi => true,
            ColorChoice::Never => false,
            ColorChoice::Auto => {
                match env::var("TERM") {
                    Err(_) => false,
                    // Cygwin uses its own terminal handling, while the Windows
                    // console API may still be available.
                    Ok(k) => k != "dumb" && k != "cygwin",
                }
            }
        }
    }
}

/// An unsupported [`ColorChoice`] name.
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
