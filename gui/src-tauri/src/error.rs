//! Errors as the window gets them from a command: `{ "code": …, "detail": … }`. The bridge's
//! own messages are codes, which the window words in its language (`errors.<code>` in the
//! catalogs, with `{detail}` filled in); an error from the library, the service or the system
//! keeps its own text in `detail`, under the code `failed`.

use std::fmt::Display;

use serde::Serialize;

/// What went wrong, as the catalogs name it (`errors.<code>`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// An error from the library, the service or the system: `detail` says what.
    Failed,
    /// A driver page with nothing to allow on this system.
    NothingToAllow,
    /// A bindings sheet to be saved under another extension than `.html`.
    SheetNotHtml,
    /// The service answered something else than the command asks for.
    ServiceAnswer,
    /// No update to install: there was no check, or it found none.
    NoUpdate,
    /// A portable copy cannot install an update over itself.
    PortableUpdate,
    /// A portable copy does not start with the system.
    PortableAutostart,
    /// Starting with the system is not offered on this system.
    #[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
    AutostartUnsupported,
}

impl ErrorCode {
    #[cfg(test)]
    const ALL: [ErrorCode; 8] = [
        ErrorCode::Failed,
        ErrorCode::NothingToAllow,
        ErrorCode::SheetNotHtml,
        ErrorCode::ServiceAnswer,
        ErrorCode::NoUpdate,
        ErrorCode::PortableUpdate,
        ErrorCode::PortableAutostart,
        ErrorCode::AutostartUnsupported,
    ];
}

/// A command's error.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CmdError {
    pub code: ErrorCode,
    /// What the message names (a path), or the error's own text for [`ErrorCode::Failed`];
    /// may be empty.
    pub detail: String,
}

impl CmdError {
    pub fn new(code: ErrorCode) -> CmdError {
        CmdError {
            code,
            detail: String::new(),
        }
    }

    pub fn with(code: ErrorCode, detail: impl Display) -> CmdError {
        CmdError {
            code,
            detail: detail.to_string(),
        }
    }

    /// The error for the log, which is in English: the detail, with its code unless it is
    /// [`ErrorCode::Failed`].
    pub fn describe(&self) -> String {
        match (self.code, self.detail.is_empty()) {
            (ErrorCode::Failed, _) => self.detail.clone(),
            (code, true) => format!("{code:?}"),
            (code, false) => format!("{code:?}: {}", self.detail),
        }
    }
}

/// Any other error, in its own words. (`CmdError` itself is not `Display`, which keeps this
/// apart from `From<CmdError>`.)
impl<E: Display> From<E> for CmdError {
    fn from(e: E) -> CmdError {
        CmdError::with(ErrorCode::Failed, e)
    }
}

pub type CmdResult<T> = Result<T, CmdError>;

/// `e` as a command's error, for `map_err`.
pub fn err(e: impl Display) -> CmdError {
    CmdError::from(e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_travel_as_code_and_detail() {
        let e = CmdError::from(std::io::Error::other("disk full"));
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            serde_json::json!({ "code": "failed", "detail": "disk full" })
        );
        let e = CmdError::new(ErrorCode::SheetNotHtml);
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            serde_json::json!({ "code": "sheet_not_html", "detail": "" })
        );
    }

    #[test]
    fn every_code_has_a_text_in_every_catalog() {
        for (lang, json) in crate::tray::CATALOGS {
            let catalog: serde_json::Value = serde_json::from_str(json).unwrap();
            for code in ErrorCode::ALL {
                let key = serde_json::to_value(code).unwrap();
                let key = key.as_str().unwrap();
                assert!(
                    catalog["errors"][key].is_string(),
                    "errors.{key} is missing in {lang}"
                );
            }
        }
    }
}
