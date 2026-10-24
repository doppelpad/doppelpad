//! Format versions of the files Doppelpad writes. Each kind of file carries its version in a
//! top-level `format = N` line, and a file without one is version 1. Reading brings an older
//! file up to date step by step, so a field can be renamed or moved without breaking the files
//! people already have, and refuses a file from a newer Doppelpad instead of misreading it.

use serde::de::{DeserializeOwned, Error as _};

/// The key that holds a file's format version.
const KEY: &str = "format";

/// One step in a kind of file's history: turns a document of version n into version n + 1.
/// It works on the plain TOML table, so it needs none of the types the file once had, and it
/// leaves what it does not recognise for the deserializer to judge.
pub type Migration = fn(&mut toml::Table);

/// A kind of file, and how its earlier versions become the current one.
#[derive(Debug)]
pub struct FileFormat {
    /// `migrations[n - 1]` turns version n into n + 1, so the current version is one more
    /// than there are migrations.
    migrations: &'static [Migration],
}

impl FileFormat {
    pub const fn new(migrations: &'static [Migration]) -> FileFormat {
        FileFormat { migrations }
    }

    /// The version this build reads and writes.
    pub const fn version(&self) -> u32 {
        self.migrations.len() as u32 + 1
    }

    /// Read `src`: check its version, bring an older document up to date and deserialize it.
    /// Errors in a document of the current version point at its lines.
    pub fn parse<T: DeserializeOwned>(&self, src: &str) -> Result<T, toml::de::Error> {
        let found = version_of(src)?;
        let current = self.version();
        if found > current {
            return Err(toml::de::Error::custom(format!(
                "made by a newer Doppelpad (format {found}; this one reads up to {current})"
            )));
        }
        if found == current {
            // The spanned table keeps the positions for error messages.
            let mut root = toml::de::DeTable::parse(src)?;
            root.get_mut().remove(KEY);
            return T::deserialize(toml::de::Deserializer::from(root)).map_err(|mut e| {
                e.set_input(Some(src));
                e
            });
        }
        let mut table: toml::Table = toml::from_str(src)?;
        table.remove(KEY);
        for migrate in &self.migrations[found as usize - 1..] {
            migrate(&mut table);
        }
        table.try_into()
    }

    /// `body`, a TOML document, with this format's version in front.
    pub fn stamp(&self, body: &str) -> String {
        format!("{KEY} = {}\n{body}", self.version())
    }
}

/// The version a document declares: its `format`, or 1 without one.
fn version_of(src: &str) -> Result<u32, toml::de::Error> {
    #[derive(serde::Deserialize)]
    struct Head {
        format: Option<u32>,
    }
    match toml::from_str::<Head>(src)?.format {
        None => Ok(1),
        Some(0) => Err(toml::de::Error::custom("format versions start at 1")),
        Some(v) => Ok(v),
    }
}

/// `e` on one line, with the line of `src` it points at.
pub(crate) fn one_line(e: &toml::de::Error, src: &str) -> String {
    let line = e
        .span()
        .and_then(|span| src.get(..span.start))
        .map(|before| before.matches('\n').count() + 1);
    match line {
        Some(line) => format!("line {line}: {}", e.message()),
        None => e.message().to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(default, deny_unknown_fields)]
    struct Doc {
        name: String,
        speed: u32,
        tags: Vec<String>,
    }

    /// Version 1 called `speed` `rate`.
    fn rename_rate(t: &mut toml::Table) {
        if let Some(v) = t.remove("rate") {
            t.insert("speed".into(), v);
        }
    }

    /// Version 2 had a single `tag`.
    fn tag_to_tags(t: &mut toml::Table) {
        if let Some(v) = t.remove("tag") {
            t.insert("tags".into(), toml::Value::Array(vec![v]));
        }
    }

    const PLAIN: FileFormat = FileFormat::new(&[]);
    const THIRD: FileFormat = FileFormat::new(&[rename_rate, tag_to_tags]);

    fn doc(speed: u32, tags: &[&str]) -> Doc {
        Doc {
            name: "x".into(),
            speed,
            tags: tags.iter().map(|&t| t.to_owned()).collect(),
        }
    }

    #[test]
    fn the_version_counts_the_migrations() {
        assert_eq!(PLAIN.version(), 1);
        assert_eq!(THIRD.version(), 3);
        assert_eq!(THIRD.stamp("name = \"x\"\n"), "format = 3\nname = \"x\"\n");
    }

    #[test]
    fn current_and_unversioned_documents_read_as_they_are() {
        let src = "format = 1\nname = \"x\"\nspeed = 4\n";
        assert_eq!(PLAIN.parse::<Doc>(src).unwrap(), doc(4, &[]));
        assert_eq!(
            PLAIN.parse::<Doc>("name = \"x\"\nspeed = 4\n").unwrap(),
            doc(4, &[])
        );
        let text = PLAIN.stamp(&toml::to_string(&doc(2, &["a"])).unwrap());
        assert_eq!(PLAIN.parse::<Doc>(&text).unwrap(), doc(2, &["a"]));
    }

    #[test]
    fn older_documents_run_the_migrations_they_lack() {
        let v1 = "name = \"x\"\nrate = 5\ntag = \"a\"\n";
        assert_eq!(THIRD.parse::<Doc>(v1).unwrap(), doc(5, &["a"]));
        let v2 = "format = 2\nname = \"x\"\nspeed = 5\ntag = \"a\"\n";
        assert_eq!(THIRD.parse::<Doc>(v2).unwrap(), doc(5, &["a"]));
        let v3 = "format = 3\nname = \"x\"\nspeed = 5\ntags = [\"a\"]\n";
        assert_eq!(THIRD.parse::<Doc>(v3).unwrap(), doc(5, &["a"]));
        // What a migration does not know stays for the deserializer to reject.
        let odd = "format = 2\nname = \"x\"\nrate = 5\n";
        assert!(THIRD.parse::<Doc>(odd).is_err());
    }

    #[test]
    fn newer_or_odd_versions_are_refused() {
        let newer = PLAIN
            .parse::<Doc>("format = 2\nname = \"x\"\n")
            .unwrap_err();
        assert_eq!(
            newer.message(),
            "made by a newer Doppelpad (format 2; this one reads up to 1)"
        );
        for bad in [
            "format = 0\n",
            "format = -1\n",
            "format = \"1\"\n",
            "[format]\n",
        ] {
            assert!(PLAIN.parse::<Doc>(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn errors_point_at_their_line() {
        let src = "format = 1\nname = \"x\"\nsped = 4\n";
        let e = PLAIN.parse::<Doc>(src).unwrap_err();
        assert!(e.to_string().contains("line 3"), "{e}");
        assert!(
            one_line(&e, src).starts_with("line 3: unknown field `sped`"),
            "{e}"
        );
        let migrated = THIRD.parse::<Doc>("rate = \"fast\"\n").unwrap_err();
        assert!(!one_line(&migrated, "").is_empty());
    }
}
