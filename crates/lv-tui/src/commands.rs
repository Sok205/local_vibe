/// Single source of truth for every slash command. Consumed by the palette
/// popup (for live filtering) and the /help overlay (for rendering).
pub struct CommandSpec {
    pub name: &'static str,
    pub takes_args: bool,
    pub description: &'static str,
}

pub const COMMANDS: &[CommandSpec] = &[
    CommandSpec { name: "/help",   takes_args: false, description: "show command + key reference" },
    CommandSpec { name: "/?",      takes_args: false, description: "alias for /help" },
    CommandSpec { name: "/status", takes_args: false, description: "snapshot: models, every DB, runtime" },
    CommandSpec { name: "/models", takes_args: false, description: "load / unload / activate model tiers" },
    CommandSpec { name: "/browse", takes_args: true,  description: "browse files inside a DB" },
    CommandSpec { name: "/dbs",    takes_args: false, description: "list DB names" },
    CommandSpec { name: "/db",     takes_args: true,  description: "switch current DB" },
    CommandSpec { name: "/index",  takes_args: true,  description: "index a directory (blank = picker)" },
    CommandSpec { name: "/quit",   takes_args: false, description: "exit" },
];

/// Returns true when the current input line is in "palette mode" — starts with
/// `/`, has at least one character after it, and contains no whitespace yet.
/// (A trailing space means the user has moved past the command name into args.)
pub fn is_palette_prefix(line: &str) -> bool {
    line.starts_with('/') && !line.contains(char::is_whitespace)
}

/// Case-insensitive prefix match on the text after `/` of the input line.
pub fn filter_commands(line: &str) -> Vec<&'static CommandSpec> {
    let needle = line.trim_start_matches('/').to_ascii_lowercase();
    COMMANDS
        .iter()
        .filter(|c| {
            let name = c.name.trim_start_matches('/').to_ascii_lowercase();
            name.starts_with(&needle)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_prefix_detection() {
        assert!(is_palette_prefix("/"));
        assert!(is_palette_prefix("/s"));
        assert!(is_palette_prefix("/status"));
        assert!(!is_palette_prefix("/index foo"));
        assert!(!is_palette_prefix("hello"));
        assert!(!is_palette_prefix(""));
    }

    #[test]
    fn filter_on_empty_after_slash_returns_all() {
        let all = filter_commands("/");
        assert_eq!(all.len(), COMMANDS.len());
    }

    #[test]
    fn filter_case_insensitive_prefix() {
        let got = filter_commands("/S");
        assert!(got.iter().any(|c| c.name == "/status"));
        assert!(!got.iter().any(|c| c.name == "/db"));
    }

    #[test]
    fn filter_narrows_on_specific_prefix() {
        let got = filter_commands("/db");
        let names: Vec<&str> = got.iter().map(|c| c.name).collect();
        assert!(names.contains(&"/db"));
        assert!(names.contains(&"/dbs"));
        assert!(!names.contains(&"/status"));
    }
}
