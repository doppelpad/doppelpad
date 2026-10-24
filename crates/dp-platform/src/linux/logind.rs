//! Asking systemd-logind about the user's session with `busctl`. No I/O here, so it is
//! unit-tested on every OS.

/// `busctl` arguments that read the `LockedHint` of the session Doppelpad runs in, or of the
/// user's graphical session when it runs outside one (`session/auto`). The desktop sets the
/// hint while its lock screen shows.
pub const LOCKED_HINT_QUERY: [&str; 5] = [
    "get-property",
    "org.freedesktop.login1",
    "/org/freedesktop/login1/session/auto",
    "org.freedesktop.login1.Session",
    "LockedHint",
];

/// The hint from `busctl`'s answer (`b true`).
pub fn locked_hint(answer: &str) -> Option<bool> {
    match answer.trim() {
        "b true" => Some(true),
        "b false" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locked_hints() {
        assert_eq!(locked_hint("b true\n"), Some(true));
        assert_eq!(locked_hint("b false\n"), Some(false));
        assert_eq!(locked_hint(""), None);
        assert_eq!(locked_hint("s \"active\""), None);
    }
}
