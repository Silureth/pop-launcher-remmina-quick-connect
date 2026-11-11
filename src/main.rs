mod config;
mod remmina;

use config::Config;
use pop_launcher::{IconSource, PluginResponse, PluginSearchResult, Request};
use remmina::Entry;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

const MAX_RESULTS: usize = 8;

struct Plugin {
    entries: Vec<Entry>,
    /// Indices into `entries` for the rows currently shown.
    shown: Vec<usize>,
    config: Config,
    out: io::StdoutLock<'static>,
}

impl Plugin {
    fn send(&mut self, response: PluginResponse) {
        // If the launcher has gone away there is nobody to report to.
        if serde_json::to_writer(&mut self.out, &response).is_ok() {
            let _ = self.out.write_all(b"\n");
            let _ = self.out.flush();
        }
    }

    fn search(&mut self, query: &str) {
        let query = strip_trigger(query);
        self.shown = if query.is_empty() {
            (0..self.entries.len().min(MAX_RESULTS)).collect()
        } else {
            remmina::search(&self.entries, query, MAX_RESULTS)
        };

        self.send(PluginResponse::Clear);

        if self.shown.is_empty() {
            let (name, description) = if query.is_empty() {
                ("No saved Remmina connections.", "")
            } else {
                ("No results found.", "Try a different query.")
            };
            self.send(PluginResponse::Append(PluginSearchResult {
                id: 0,
                name: name.into(),
                description: description.into(),
                icon: Some(IconSource::Name("dialog-information".into())),
                ..Default::default()
            }));
        }

        for id in 0..self.shown.len() {
            let entry = &self.entries[self.shown[id]];
            let result = PluginSearchResult {
                id: id as u32,
                name: entry.name.clone(),
                description: entry.description(),
                icon: Some(IconSource::Name("org.remmina.Remmina".into())),
                ..Default::default()
            };
            self.send(PluginResponse::Append(result));
        }

        self.send(PluginResponse::Finished);
    }

    fn activate(&mut self, id: u32) {
        if let Some(&index) = self.shown.get(id as usize) {
            // Nothing useful to show the user if Remmina fails to start.
            let _ = remmina::open(&self.entries[index], &self.config);
        }
        self.send(PluginResponse::Close);
    }
}

/// Drops the trigger word the launcher routed on (whatever `regex` in
/// plugin.ron matches, `rem` by default), leaving just the search text.
fn strip_trigger(query: &str) -> &str {
    match query.trim().split_once(char::is_whitespace) {
        Some((_, rest)) => rest.trim(),
        None => "",
    }
}

fn main() {
    // The binary sits in the plugin folder, next to plugin.ron and config.json.
    let plugin_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(PathBuf::from))
        .unwrap_or_default();
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());

    let config = Config::load(&plugin_dir.join("config.json"));
    let entries = remmina::saved_connections(&remmina::data_dirs(&config, &home));

    let mut plugin = Plugin {
        entries,
        shown: Vec::new(),
        config,
        out: io::stdout().lock(),
    };

    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        match serde_json::from_str::<Request>(&line) {
            Ok(Request::Search(query)) => plugin.search(&query),
            Ok(Request::Activate(id)) => plugin.activate(id),
            Ok(Request::Exit) => break,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_trigger_drops_first_word() {
        assert_eq!(strip_trigger("rem"), "");
        assert_eq!(strip_trigger("  rem  office db "), "office db");
    }
}
