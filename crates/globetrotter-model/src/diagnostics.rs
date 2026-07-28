use codespan_reporting::diagnostic::{Diagnostic, Severity};

/// Identifier for a source file, used when emitting diagnostics.
pub type FileId = usize;
/// A byte range within a source file.
pub type Span = std::ops::Range<usize>;

/// Conversion of a value into a list of diagnostics for a given source file.
pub trait ToDiagnostics {
    /// Convert `self` into diagnostics labelled with `file_id`.
    fn to_diagnostics<F: Copy + PartialEq>(&self, file_id: F) -> Vec<Diagnostic<F>>;
}

/// Convenience helpers for working with [`Diagnostic`] severities.
pub trait DiagnosticExt {
    /// Returns `true` if this diagnostic has error severity.
    fn is_error(&self) -> bool;
    /// Returns `true` if this diagnostic has warning severity.
    fn is_warning(&self) -> bool;
    /// Construct an error diagnostic when `strict`, otherwise a warning.
    fn warning_or_error(strict: bool) -> Self;
}

impl<F> DiagnosticExt for Diagnostic<F> {
    fn is_error(&self) -> bool {
        match self.severity {
            Severity::Bug | Severity::Error => true,
            Severity::Warning | Severity::Note | Severity::Help => false,
        }
    }

    fn is_warning(&self) -> bool {
        match self.severity {
            Severity::Warning => true,
            Severity::Bug | Severity::Error | Severity::Note | Severity::Help => false,
        }
    }

    fn warning_or_error(strict: bool) -> Self {
        if strict {
            Self::error()
        } else {
            Self::warning()
        }
    }
}

/// A wrapper that renders its inner value using [`std::fmt::Display`] for
/// both its `Display` and `Debug` implementations.
pub struct DisplayRepr<'a, T>(pub &'a T);

impl<T> std::fmt::Debug for DisplayRepr<'_, T>
where
    T: std::fmt::Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self.0, f)
    }
}

impl<T> std::fmt::Display for DisplayRepr<'_, T>
where
    T: std::fmt::Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self.0, f)
    }
}

/// A value paired with its source [`Span`].
#[derive(Debug, Clone)]
pub struct Spanned<T> {
    /// The wrapped value.
    pub inner: T,
    /// The source span the value originated from.
    pub span: Span,
}

impl<T> serde::Serialize for Spanned<T>
where
    T: serde::Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.inner.serialize(serializer)
    }
}

impl<T> std::ops::Deref for Spanned<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> AsRef<T> for Spanned<T> {
    fn as_ref(&self) -> &T {
        &self.inner
    }
}

impl<T> Spanned<T> {
    /// Create a new spanned value from a span and a value.
    pub fn new(span: impl Into<Span>, value: T) -> Self {
        Self {
            span: span.into(),
            inner: value,
        }
    }

    /// Create a spanned value with an empty (dummy) span.
    pub fn dummy(value: T) -> Self {
        Self {
            span: Span::default(),
            inner: value,
        }
    }

    /// Consume the wrapper and return the inner value.
    pub fn into_inner(self) -> T {
        self.inner
    }

    /// Returns a [`std::fmt::Display`] adapter for the inner value.
    pub fn display(&self) -> DisplayRepr<'_, Self> {
        DisplayRepr(self)
    }
}

impl<T> std::fmt::Display for Spanned<T>
where
    T: std::fmt::Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.inner, f)
    }
}

impl<T> PartialEq for Spanned<T>
where
    T: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl<T> PartialEq<T> for Spanned<T>
where
    T: PartialEq,
{
    fn eq(&self, other: &T) -> bool {
        (&self.inner as &dyn PartialEq<T>).eq(other)
    }
}

impl<T> PartialEq<&T> for Spanned<T>
where
    T: PartialEq,
{
    fn eq(&self, other: &&T) -> bool {
        (&self.inner as &dyn PartialEq<T>).eq(*other)
    }
}

impl<T> Ord for Spanned<T>
where
    T: Ord,
{
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        Ord::cmp(&self.inner, &other.inner)
    }
}

impl<T> PartialOrd for Spanned<T>
where
    T: PartialOrd,
{
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        PartialOrd::partial_cmp(&self.inner, &other.inner)
    }
}

impl<T> PartialOrd<T> for Spanned<T>
where
    T: PartialOrd,
{
    fn partial_cmp(&self, other: &T) -> Option<std::cmp::Ordering> {
        PartialOrd::partial_cmp(&self.inner, other)
    }
}

impl<T> Eq for Spanned<T> where T: Eq {}

impl<T> std::hash::Hash for Spanned<T>
where
    T: std::hash::Hash,
{
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.inner.hash(state);
    }
}
