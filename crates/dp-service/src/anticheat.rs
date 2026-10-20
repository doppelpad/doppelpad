//! Games whose anti-cheat acts against automated or modified input, each with the public
//! statement it rests on. Doppelpad only warns while such a game is in front; it never
//! blocks anything. Add an entry only with a source that says so.

pub struct AntiCheat {
    /// Executable names, compared without regard to case.
    pub processes: &'static [&'static str],
    pub game: &'static str,
    /// What the publisher says, in short.
    pub note: &'static str,
    pub source: &'static str,
}

pub const LIST: &[AntiCheat] = &[
    AntiCheat {
        processes: &["cs2.exe"],
        game: "Counter-Strike 2",
        note: "Valve kicks players from its official servers when one input triggers several \
               actions (macros, scripted or hardware automation; August 2024)",
        source: "https://www.gamingonlinux.com/2024/08/valve-bans-keyboard-automation-in-counter-strike-2-update",
    },
    AntiCheat {
        processes: &["cod.exe"],
        game: "Call of Duty",
        note: "RICOCHET looks for input-modifying devices by how the input behaves; turbo and \
               recoil macros can be flagged (February 2026)",
        source: "https://www.callofduty.com/blog/2026/02/call-of-duty-black-ops-7-ricochet-anti-cheat-season-02",
    },
];

/// The entry for a foreground executable, if any.
pub fn lookup(exe: &str) -> Option<&'static AntiCheat> {
    let name = exe.rsplit(['\\', '/']).next().unwrap_or(exe);
    LIST.iter()
        .find(|a| a.processes.iter().any(|p| p.eq_ignore_ascii_case(name)))
}

impl AntiCheat {
    pub fn warning(&self) -> String {
        format!(
            "{}: {}. Mappings that automate input may get you kicked or banned ({})",
            self.game, self.note, self.source
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_games_by_executable() {
        assert_eq!(lookup("CS2.EXE").map(|a| a.game), Some("Counter-Strike 2"));
        assert_eq!(
            lookup(r"C:\Games\cod.exe").map(|a| a.game),
            Some("Call of Duty")
        );
        assert!(lookup("notepad.exe").is_none());
        for a in LIST {
            assert!(
                a.source.starts_with("https://"),
                "{} needs a source",
                a.game
            );
        }
    }
}
