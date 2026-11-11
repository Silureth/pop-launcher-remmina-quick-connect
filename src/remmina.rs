use crate::config::Config;
use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Matcher, Utf32Str};
use std::cmp::Reverse;
use std::collections::HashSet;
use std::fs;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::SystemTime;

const FLATPAK_ID: &str = "org.remmina.Remmina";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub group: String,
    pub protocol: String,
    pub server: String,
    pub username: String,
    /// The `.remmina` file passed to `remmina -c`.
    pub file: PathBuf,
}

impl Entry {
    /// `Group · RDP · user@server`, skipping whatever is empty.
    pub fn description(&self) -> String {
        let target = match (self.username.is_empty(), self.server.is_empty()) {
            (false, false) => format!("{}@{}", self.username, self.server),
            (true, _) => self.server.clone(),
            (false, true) => self.username.clone(),
        };
        [self.group.as_str(), self.protocol.as_str(), target.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" · ")
    }

    /// Text searched besides the name: group, server and username.
    fn details(&self) -> String {
        format!("{}/{} {} {}", self.group, self.name, self.server, self.username)
    }
}

/// Folders that may hold `.remmina` files: the configured one, or else each
/// `datadir_path` from Remmina's settings plus the default locations for the
/// native, Flatpak and legacy installs. Only folders that exist are returned.
pub fn data_dirs(config: &Config, home: &Path) -> Vec<PathBuf> {
    if let Some(dir) = &config.data_dir {
        return vec![dir.clone()];
    }

    let flatpak = home.join(".var/app").join(FLATPAK_ID);
    let prefs = [
        home.join(".config/remmina/remmina.pref"),
        flatpak.join("config/remmina/remmina.pref"),
    ];
    let mut dirs: Vec<PathBuf> = prefs
        .iter()
        .filter_map(|pref| pref_datadir(&fs::read_to_string(pref).ok()?))
        .collect();
    dirs.extend([
        home.join(".local/share/remmina"),
        flatpak.join("data/remmina"),
        home.join(".remmina"),
    ]);

    let mut seen = HashSet::new();
    dirs.into_iter()
        .filter(|dir| dir.is_dir())
        .filter(|dir| seen.insert(fs::canonicalize(dir).unwrap_or_else(|_| dir.clone())))
        .collect()
}

/// The custom data folder set in `remmina.pref`, if any.
fn pref_datadir(text: &str) -> Option<PathBuf> {
    let value = section_value(text, "remmina_pref", "datadir_path")?;
    (!value.is_empty()).then(|| PathBuf::from(value))
}

/// All saved connections in `dirs`, most recently modified first. Remmina
/// rewrites a profile when it is used, so this puts recent connections on top.
pub fn saved_connections(dirs: &[PathBuf]) -> Vec<Entry> {
    let mut found: Vec<(SystemTime, Entry)> = dirs
        .iter()
        .filter_map(|dir| fs::read_dir(dir).ok())
        .flat_map(|files| files.flatten())
        .filter_map(|file| {
            let path = file.path();
            if path.extension()? != "remmina" {
                return None;
            }
            let mtime = file.metadata().ok()?.modified().ok()?;
            let text = fs::read_to_string(&path).ok()?;
            Some((mtime, parse(&text, path)?))
        })
        .collect();

    found.sort_by_key(|(mtime, _)| Reverse(*mtime));
    found.into_iter().map(|(_, entry)| entry).collect()
}

/// Reads the `[remmina]` section of a profile. Only display fields are read;
/// passwords and other settings are never touched.
fn parse(text: &str, file: PathBuf) -> Option<Entry> {
    let get = |key| {
        section_value(text, "remmina", key)
            .map(|value| value.trim().to_string())
            .unwrap_or_default()
    };
    let mut entry = Entry {
        name: get("name"),
        group: get("group"),
        protocol: get("protocol"),
        server: get("server"),
        username: get("username"),
        file,
    };
    if entry.name.is_empty() && entry.server.is_empty() {
        return None;
    }
    if entry.name.is_empty() {
        entry.name = entry.server.clone();
    }
    Some(entry)
}

/// Looks up `key` in `[section]` of a GKeyFile (INI) document.
fn section_value(text: &str, section: &str, key: &str) -> Option<String> {
    let mut current = "";
    text.lines().map(str::trim).find_map(|line| {
        if let Some(header) = line.strip_prefix('[') {
            current = header.strip_suffix(']').unwrap_or_default();
            return None;
        }
        let (k, v) = line.split_once('=')?;
        (current == section && k.trim_end() == key).then(|| unescape(v.trim_start()))
    })
}

/// Undoes GKeyFile's escaping (`\s`, `\n`, `\t`, `\r`, `\\`).
fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// How literally the query appears in an entry; higher ranks first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum MatchTier {
    /// Letters appear in order, but scattered.
    Fuzzy,
    /// Every query word appears unbroken in the name, group, server or user.
    ExactInDetails,
    /// Every query word appears unbroken in the connection name.
    ExactInName,
}

/// Whether every whitespace-separated word of `query` occurs as an unbroken
/// substring of `text`. Case-insensitive unless the query has uppercase
/// letters, matching the fuzzy matcher's smart-case behaviour.
fn contains_all_words(text: &str, query: &str) -> bool {
    if query.chars().any(char::is_uppercase) {
        query.split_whitespace().all(|word| text.contains(word))
    } else {
        let text = text.to_lowercase();
        query
            .split_whitespace()
            .all(|word| text.contains(&word.to_lowercase()))
    }
}

/// Fuzzy-matches `query` against each connection and returns the indices of
/// the best matches, best first.
///
/// Connections whose name contains the query words unbroken come first, then
/// those whose group, server or username does, then scattered fuzzy matches.
/// Within each group the fuzzy score decides (name matches count double), and
/// ties keep recently-used order.
pub fn search(entries: &[Entry], query: &str, limit: usize) -> Vec<usize> {
    let pattern = Pattern::new(
        query,
        CaseMatching::Smart,
        Normalization::Smart,
        AtomKind::Fuzzy,
    );
    let mut matcher = Matcher::new(nucleo_matcher::Config::DEFAULT);
    let mut buf = Vec::new();

    let mut scored: Vec<(MatchTier, u32, usize)> = entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            let details = entry.details();
            let name_score = pattern.score(Utf32Str::new(&entry.name, &mut buf), &mut matcher);
            let details_score = pattern.score(Utf32Str::new(&details, &mut buf), &mut matcher);
            let score = name_score.map(|s| s * 2).max(details_score)?;
            let tier = if contains_all_words(&entry.name, query) {
                MatchTier::ExactInName
            } else if contains_all_words(&details, query) {
                MatchTier::ExactInDetails
            } else {
                MatchTier::Fuzzy
            };
            Some((tier, score, index))
        })
        .collect();

    // Stable sort, so equal ranks stay in recently-used order.
    scored.sort_by_key(|&(tier, score, _)| Reverse((tier, score)));
    scored
        .into_iter()
        .take(limit)
        .map(|(_, _, index)| index)
        .collect()
}

/// Launches Remmina on the profile, detached so it outlives the plugin.
pub fn open(entry: &Entry, config: &Config) -> std::io::Result<()> {
    let (program, args) = config
        .remmina_command
        .split_first()
        .ok_or(std::io::ErrorKind::InvalidInput)?;

    let mut child = Command::new(program)
        .args(args)
        .arg("-c")
        .arg(&entry.file)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()?;
    // Reap the child in the background so it does not linger as a zombie.
    std::thread::spawn(move || child.wait());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROFILE: &str = "\
[remmina]
password=secret
name=Office\\sDB
group=Office
server=db.example.com
protocol=RDP
username=admin

[other]
name=wrong
";

    fn entry(name: &str, group: &str, server: &str) -> Entry {
        Entry {
            name: name.into(),
            group: group.into(),
            protocol: "RDP".into(),
            server: server.into(),
            ..Default::default()
        }
    }

    #[test]
    fn parses_display_fields_only_from_remmina_section() {
        let e = parse(PROFILE, PathBuf::from("/x.remmina")).unwrap();
        assert_eq!(e.name, "Office DB");
        assert_eq!(e.group, "Office");
        assert_eq!(e.server, "db.example.com");
        assert_eq!(e.description(), "Office · RDP · admin@db.example.com");
    }

    #[test]
    fn nameless_profile_falls_back_to_server() {
        let e = parse("[remmina]\nserver=host:3389\n", PathBuf::new()).unwrap();
        assert_eq!(e.name, "host:3389");
        assert_eq!(e.description(), "host:3389");
        assert!(parse("[remmina]\nprotocol=RDP\n", PathBuf::new()).is_none());
    }

    #[test]
    fn reads_custom_datadir_from_pref() {
        let pref = "[remmina_pref]\ndatadir_path=/data/remmina\nremmina_file_name=%G_%P_%N_%h\n";
        assert_eq!(pref_datadir(pref), Some(PathBuf::from("/data/remmina")));
        assert_eq!(pref_datadir("[remmina_pref]\ndatadir_path=\n"), None);
    }

    #[test]
    fn unescapes_gkeyfile_values() {
        assert_eq!(unescape(r"a\sb\\c\t"), "a b\\c\t");
        assert_eq!(unescape("trailing\\"), "trailing\\");
    }

    fn names(entries: &[Entry], query: &str) -> Vec<String> {
        search(entries, query, 8)
            .into_iter()
            .map(|i| entries[i].name.clone())
            .collect()
    }

    #[test]
    fn name_match_beats_group_and_server_matches() {
        let entries = vec![
            entry("dc01", "lab", "10.0.1.1"),
            entry("backup", "office", "10.0.2.20"),
            entry("lab-db01", "lab", "10.0.1.3"),
        ];
        assert_eq!(names(&entries, "lab"), ["lab-db01", "dc01"]);
        assert_eq!(names(&entries, "2.20"), ["backup"]);
        assert!(search(&entries, "zzzqqq", 8).is_empty());
    }

    #[test]
    fn multi_word_query_spans_group_and_name() {
        let entries = vec![entry("db", "office", "a"), entry("db", "lab", "b")];
        let found = search(&entries, "lab db", 8);
        assert_eq!(entries[found[0]].group, "lab");
    }
}
