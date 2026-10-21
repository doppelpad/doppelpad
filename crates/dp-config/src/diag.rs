//! Diagnostics produced while loading, importing or validating profiles.

use std::fmt;

use serde::Serialize;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Informational: something was converted approximately.
    Info,
    /// A feature was dropped or could not be represented.
    Warning,
    /// The profile is unusable as written.
    Error,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    pub severity: Severity,
    /// Location in the source, e.g. `mappings[3].macros[1]`.
    pub path: String,
    pub message: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = self.severity.as_str();
        if self.path.is_empty() {
            write!(f, "{s}: {}", self.message)
        } else {
            write!(f, "{s}: {}: {}", self.path, self.message)
        }
    }
}

#[derive(Default, Debug)]
pub struct Diagnostics(pub Vec<Diagnostic>);

impl Diagnostics {
    pub fn push(
        &mut self,
        severity: Severity,
        path: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.0.push(Diagnostic {
            severity,
            path: path.into(),
            message: message.into(),
        });
    }
    pub fn info(&mut self, path: impl Into<String>, message: impl Into<String>) {
        self.push(Severity::Info, path, message);
    }
    pub fn warn(&mut self, path: impl Into<String>, message: impl Into<String>) {
        self.push(Severity::Warning, path, message);
    }
    pub fn error(&mut self, path: impl Into<String>, message: impl Into<String>) {
        self.push(Severity::Error, path, message);
    }
    pub fn has_errors(&self) -> bool {
        self.0.iter().any(|d| d.severity == Severity::Error)
    }
    pub fn iter(&self) -> impl Iterator<Item = &Diagnostic> {
        self.0.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The editor shows diagnostics in this form.
    #[test]
    fn json_and_text() {
        let d = Diagnostic {
            severity: Severity::Warning,
            path: "mappings[0]".into(),
            message: "pause longer than a minute".into(),
        };
        assert_eq!(
            serde_json::to_string(&d).unwrap(),
            r#"{"severity":"warning","path":"mappings[0]","message":"pause longer than a minute"}"#
        );
        assert_eq!(
            d.to_string(),
            "warning: mappings[0]: pause longer than a minute"
        );
    }
}
