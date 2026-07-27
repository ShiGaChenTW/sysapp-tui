
use crate::config::UserConfig;
use crate::model::{AppEntry, Category, Source, UiKind};

/// A keyword and the contexts that disqualify it.
///
/// Categories are tried in a fixed precedence order, so a broad term in an
/// early category would swallow entries a later, more specific category owns.
/// `unless` is how those rulings are expressed: `server` belongs to Network,
/// but `language server` must reach Development, which is checked afterwards.
struct Keyword {
    needle: &'static str,
    /// Match a token prefix rather than a whole token, for stems like
    /// `transpil` covering transpiler and transpile. Off by default because
    /// prefix matching is dangerous: `repl` would fire on "replace".
    stem: bool,
    unless: &'static [&'static str],
}

const fn word(needle: &'static str) -> Keyword {
    Keyword { needle, stem: false, unless: &[] }
}

const fn stem(needle: &'static str) -> Keyword {
    Keyword { needle, stem: true, unless: &[] }
}

const fn word_unless(needle: &'static str, unless: &'static [&'static str]) -> Keyword {
    Keyword { needle, stem: false, unless }
}

const fn stem_unless(needle: &'static str, unless: &'static [&'static str]) -> Keyword {
    Keyword { needle, stem: true, unless }
}

/// Deliberately absent, measured against the 8,517-formula homebrew-core
/// corpus: `library` (932 hits) and `convert` (177) span every category and
/// discriminate nothing; `terminal` belongs to the `UiKind` classifier, not
/// here; bare `key` hits keyboard/keybinding/keyframe; bare `browser` would
/// claim `ranger`, whose desc is "File browser". Each survives only as part of
/// a qualified phrase below.
const SECURITY: &[Keyword] = &[
    stem("encrypt"),
    stem("decrypt"),
    stem("cryptograph"),
    word("password manager"),
    word("certificate"),
    word("tls"),
    word("gpg"),
    word("pgp"),
    word("keychain"),
    word("vulnerability"),
    word("penetration test"),
    word("malware"),
    word("sandbox"),
    word("two factor"),
    word("totp"),
    word("cve"),
    word("exploit"),
    word("firewall"),
    word("private key"),
    word("public key"),
    word("gpg key"),
    word("secrets management"),
];

const DESIGN: &[Keyword] = &[
    word("svg"),
    word("vector graphic"),
    word("color palette"),
    word("typography"),
    word("typeface"),
    word("glyph"),
    word("kerning"),
    word("font"),
    word("wireframe"),
    word("design system"),
    word("mockup"),
    word("icon set"),
    word("bezier"),
    stem("rasteriz"),
    word("image tracing"),
    word("ui design"),
    word("layout engine"),
    word("color scheme"),
    word("dithering"),
    word("image editor"),
    word("vector editor"),
    word("font render"),
    word("vector render"),
];

const GAMING: &[Keyword] = &[
    word("roguelike"),
    word("video game"),
    word("game engine"),
    word("console emulator"),
    word("game emulator"),
    word("game server"),
    word("tetris"),
    word("sudoku"),
    word("solitaire"),
    word("arcade"),
    word("dungeon"),
    word("nethack"),
    word("sprite"),
    word("interactive fiction"),
    word("puzzle game"),
    word("chess engine"),
    word("retro game"),
    word("doom"),
    word("platformer"),
    word("game controller"),
    word("level editor"),
    word("nes"),
    word("snes"),
    word("gameboy"),
];

const MEDIA: &[Keyword] = &[
    word("codec"),
    word("transcode"),
    word("audio"),
    word("video"),
    word("subtitle"),
    word("mp3"),
    word("flac"),
    word("ffmpeg"),
    word("media player"),
    word("waveform"),
    word("spectrogram"),
    word("playlist"),
    word("exif"),
    word("thumbnail"),
    word("photo"),
    word("podcast"),
    word("streaming media"),
    word("music player"),
    word("frame rate"),
    word("demux"),
    word("video editor"),
    word("audio editor"),
    word("video render"),
    word("3d render"),
    word("animation render"),
    // 32 of 164 corpus hits for `image` are container images, and disk images
    // belong to System; both categories are checked after this one.
    word_unless(
        "image",
        &[
            "docker image",
            "container image",
            "oci image",
            "disk image",
            "iso image",
            "partition",
        ],
    ),
    stem_unless(
        "stream",
        &["byte stream", "stream processing", "streaming proxy"],
    ),
];

const DATA: &[Keyword] = &[
    word("dataframe"),
    word("csv"),
    word("parquet"),
    word("sql"),
    word("etl"),
    word("machine learning"),
    word("query engine"),
    word("dataset"),
    word("olap"),
    word("time series"),
    word("jupyter"),
    word("notebook"),
    word("tensor"),
    word("data visualization"),
    word("data warehouse"),
    word("apache arrow"),
    word("aggregation"),
    word("statistics"),
    word("schema migration"),
    word("columnar"),
    word("database server"),
    word("database client"),
    word("sql client"),
    word("log analytics"),
    word("byte stream"),
    word("stream processing"),
    word_unless("database", &["database migration"]),
    word_unless("graph", &["call graph", "dependency graph", "commit graph"]),
    word_unless("matrix", &["matrix protocol", "matrix client"]),
    word_unless(
        "analysis",
        &["static analysis", "packet analysis", "protocol analysis"],
    ),
];

const COMMUNICATION: &[Keyword] = &[
    word("irc"),
    word("xmpp"),
    word("jabber"),
    word("imap"),
    word("smtp"),
    word("email client"),
    word("chat client"),
    word("irc client"),
    word("xmpp client"),
    word("instant messaging"),
    word("mastodon"),
    word("matrix protocol"),
    word("matrix client"),
    word("mail user agent"),
    word("rss"),
    word("feed reader"),
    word("voip"),
    word("sip"),
    word("telegram"),
    word("slack"),
    word("discord"),
    word("newsgroup"),
    word("nntp"),
];

const NETWORK: &[Keyword] = &[
    word("dns"),
    word("tcp"),
    word("http"),
    word("proxy"),
    word("packet capture"),
    word("ssh"),
    word("ftp"),
    word("socket"),
    word("load balancer"),
    word("vpn"),
    word("traceroute"),
    word("bandwidth"),
    word("subnet"),
    word("dhcp"),
    word("tunnel"),
    word("port scan"),
    word("web browser"),
    word("reverse proxy"),
    word("latency"),
    word("netcat"),
    word("rest api"),
    word("http api"),
    word("protocol"),
    word("network monitor"),
    word("bandwidth monitor"),
    word("traffic monitor"),
    word("packet monitor"),
    word("packet analysis"),
    word("protocol analysis"),
    word("speed test"),
    word("network test"),
    word("bandwidth test"),
    word("streaming proxy"),
    word_unless(
        "server",
        &["language server", "database server", "game server"],
    ),
    word_unless(
        "client",
        &[
            "email client",
            "chat client",
            "irc client",
            "xmpp client",
            "database client",
            "sql client",
        ],
    ),
    stem_unless("sync", &["file sync", "note sync", "calendar sync"]),
];

const DEVELOPMENT: &[Keyword] = &[
    word("compiler"),
    word("interpreter"),
    word("debugger"),
    word("linter"),
    word("language server"),
    word("build system"),
    word("version control"),
    word("source code"),
    word("bytecode"),
    stem("refactor"),
    word("static analysis"),
    word("unit test"),
    word("package manager"),
    word("dependency"),
    stem("transpil"),
    word("scaffold"),
    word("code formatter"),
    word("syntax highlight"),
    word("repl"),
    word("sdk"),
    word("docker"),
    word("kubernetes"),
    word("container image"),
    word("docker image"),
    word("oci image"),
    word("database migration"),
    word("orm"),
    word("call graph"),
    word("dependency graph"),
    word("commit graph"),
    word("template render"),
    word("logging library"),
    word("shell completion"),
    word_unless("api", &["rest api", "http api"]),
    word_unless(
        "editor",
        &[
            "video editor",
            "audio editor",
            "image editor",
            "vector editor",
            "hex editor",
            "markdown editor",
            "level editor",
        ],
    ),
    word_unless(
        "test",
        &["speed test", "network test", "bandwidth test", "penetration test"],
    ),
];

const PRODUCTIVITY: &[Keyword] = &[
    word("todo"),
    word("task manager"),
    word("calendar"),
    word("note taking"),
    word("time tracking"),
    word("pomodoro"),
    word("reminder"),
    word("outliner"),
    word("journal"),
    word("personal organizer"),
    word("bookmark"),
    word("clipboard manager"),
    word("presentation"),
    word("knowledge base"),
    word("habit"),
    word("agenda"),
    word("markdown editor"),
    word("scratchpad"),
    word("zettelkasten"),
    word("file sync"),
    word("note sync"),
    word("calendar sync"),
];

/// Last on purpose. Its terms are the broadest in the table — bare `monitor`,
/// `emulator`, `log` and `archive` would otherwise swallow entries that
/// Network, Gaming and Data own.
const SYSTEM: &[Keyword] = &[
    word("process monitor"),
    word("resource monitor"),
    word("system monitor"),
    word("disk usage"),
    word("filesystem"),
    word("partition"),
    word("kernel"),
    word("daemon"),
    word("launchd"),
    word("cron"),
    word("uptime"),
    word("swap"),
    word("mount"),
    word("top like"),
    word("hardware information"),
    word("system information"),
    word("terminal emulator"),
    word("terminal multiplexer"),
    word("shell"),
    word("log file"),
    word("backup"),
    word("archive"),
    word("emulator"),
    word("monitor"),
    word("hex editor"),
    word("disk image"),
    word("iso image"),
    word("log"),
];

/// Rarest and most specific first; System is the catch-all and must stay last.
const CATEGORY_KEYWORDS: &[(Category, &[Keyword])] = &[
    (Category::Security, SECURITY),
    (Category::Design, DESIGN),
    (Category::Gaming, GAMING),
    (Category::Media, MEDIA),
    (Category::Data, DATA),
    (Category::Communication, COMMUNICATION),
    (Category::Network, NETWORK),
    (Category::Development, DEVELOPMENT),
    (Category::Productivity, PRODUCTIVITY),
    (Category::System, SYSTEM),
];

pub fn enrich_categories(entries: &mut [AppEntry], cfg: &UserConfig) {
    for entry in entries {
        // Override-before-rule lives here because both collapse to the same
        // "user said so" precedence once the pure classifier receives them.
        let from_config = cfg
            .override_for(&entry.name)
            .or_else(|| cfg.rule_for(entry));

        entry.category = Some(classify(
            &entry.name,
            entry.description.as_deref(),
            &entry.source,
            entry.ui_kind,
            from_config,
        ));
    }
}

/// Structural fallbacks, used only when nothing in the description spoke.
///
/// `Applications` deliberately has none. Labelling every unrecognised GUI app
/// `Productivity` produced 280 of 289 entries in that bucket on a real machine,
/// which makes the category carry no information at all — `—` is the honest
/// answer, and `C` lets the user assign a real one.
///
/// The command-line sources get the opposite treatment: a Homebrew, Gem or Pip
/// package that is a runnable command-line or terminal program is developer
/// tooling by default. Without this, `abseil` ("C++ Common Libraries") and
/// `agent-browser` ("Browser automation CLI for AI agents") sat in
/// `Uncategorized`, which is a miss rather than honesty.
fn structural_category(source: &Source, ui_kind: Option<UiKind>) -> Category {
    // Everything these three package managers ship is developer tooling unless
    // it is a windowed application, so `Gui` is the only disqualifier.
    //
    // Gating on `Cli | Tui | Library` instead would miss the case that motivated
    // this rule: 109 of 323 Homebrew formulae on a real machine install no
    // binary at all — `abseil`, `openssl@3` and every other header-only or
    // linked library — which makes them `Unknown`, not `Library`, because
    // `Library` is only inferred for Npm/Pip/Gem packages absent from PATH.
    let windowed = matches!(ui_kind, Some(UiKind::Gui));

    match source {
        Source::Cargo | Source::Go | Source::Npm => Category::Development,
        Source::Homebrew | Source::Gem | Source::Pip if !windowed => Category::Development,
        Source::Pkgutil => Category::System,
        _ => Category::Uncategorized,
    }
}

fn classify(
    name: &str,
    description: Option<&str>,
    source: &Source,
    ui_kind: Option<UiKind>,
    from_config: Option<Category>,
) -> Category {
    if let Some(category) = from_config {
        return category;
    }

    if let Some(category) = keyword_category(name, description) {
        return category;
    }

    structural_category(source, ui_kind)
}

impl Keyword {
    fn matches(&self, tokens: &[String], normalized: &str) -> bool {
        let hit = if self.needle.contains(' ') {
            normalized.contains(self.needle)
        } else if self.stem {
            tokens.iter().any(|token| token.starts_with(self.needle))
        } else {
            tokens.iter().any(|token| token == self.needle)
        };

        hit && !self
            .unless
            .iter()
            .any(|exception| normalized.contains(exception))
    }
}

fn keyword_category(name: &str, description: Option<&str>) -> Option<Category> {
    let mut haystack = String::from(name);
    if let Some(description) = description {
        haystack.push(' ');
        haystack.push_str(description);
    }

    // Punctuation is normalized away once so `terminal-based` and `terminal
    // based` are one needle, and so whole-token matching has tokens to compare.
    let lower = haystack.to_ascii_lowercase();
    let tokens: Vec<String> = lower
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect();
    let normalized = tokens.join(" ");

    CATEGORY_KEYWORDS
        .iter()
        .find(|(_, keywords)| {
            keywords
                .iter()
                .any(|keyword| keyword.matches(&tokens, &normalized))
        })
        .map(|(category, _)| category.clone())
}

#[cfg(test)]
mod tests {
    use super::{classify, enrich_categories};
    use crate::config::UserConfig;
    use crate::model::{AppEntry, Category, Source, UiKind};

    fn entry(name: &str, description: Option<&str>, source: Source) -> AppEntry {
        AppEntry {
            name: name.to_string(),
            version: None,
            source,
            language: None,
            install_date: None,
            last_used: None,
            usage_count: 0,
            path: None,
            description: description.map(str::to_string),
            ui_kind: None,
            category: None,
        }
    }

    #[test]
    fn config_category_beats_keyword_and_structural_rules() {
        let category = classify(
            "cargo-video-tool",
            Some("video player"),
            &Source::Cargo,
            None, Some(Category::Security),
        );

        assert_eq!(category, Category::Security);
    }

    #[test]
    fn config_custom_category_survives_verbatim() {
        let category = classify(
            "notes-app",
            Some("markdown editor"),
            &Source::Applications,
            None, Some(Category::Custom("Personal".to_string())),
        );

        assert_eq!(category, Category::Custom("Personal".to_string()));
    }

    #[test]
    fn description_keyword_classifies_entry() {
        let category = classify("plain-tool", Some("A fast compiler and linter"), &Source::Pip, None, None);

        assert_eq!(category, Category::Development);
    }

    #[test]
    fn name_keyword_classifies_entry() {
        let category = classify("music-player", None, &Source::Gem, None, None);

        assert_eq!(category, Category::Media);
    }

    #[test]
    fn keyword_beats_structural_fallback() {
        let category = classify("crate-tool", Some("streaming video player"), &Source::Cargo, None, None);

        assert_eq!(category, Category::Media);
    }

    #[test]
    fn cargo_falls_back_to_development() {
        let category = classify("crate-tool", Some("plain utility"), &Source::Cargo, None, None);

        assert_eq!(category, Category::Development);
    }

    #[test]
    fn go_falls_back_to_development() {
        let category = classify("go-tool", Some("plain utility"), &Source::Go, None, None);

        assert_eq!(category, Category::Development);
    }

    #[test]
    fn npm_falls_back_to_development() {
        let category = classify("npm-tool", Some("plain utility"), &Source::Npm, None, None);

        assert_eq!(category, Category::Development);
    }

    /// 280 of 289 `Productivity` entries on a real machine came from a blanket
    /// fallback on `Applications`, which made the label meaningless. An
    /// unrecognised GUI app must read `—` and wait for the user to assign one.
    #[test]
    fn applications_with_no_keyword_hit_stay_uncategorized() {
        let category = classify(
            "desktop-app",
            Some("plain utility"),
            &Source::Applications,
            Some(UiKind::Gui),
            None,
        );

        assert_eq!(category, Category::Uncategorized);
    }

    /// The keyword pass still applies to GUI apps; only the catch-all is gone.
    #[test]
    fn applications_still_match_keywords() {
        let category = classify(
            "Final Cut",
            Some("Video editor"),
            &Source::Applications,
            Some(UiKind::Gui),
            None,
        );

        assert_eq!(category, Category::Media);
    }

    /// A brew/gem/pip package that is a runnable command-line or terminal
    /// program is developer tooling by default: `abseil` and `agent-browser`
    /// sat in `Uncategorized` without this.
    #[test]
    fn runnable_command_line_packages_fall_back_to_development() {
        for source in [Source::Homebrew, Source::Gem, Source::Pip] {
            for kind in [UiKind::Cli, UiKind::Tui, UiKind::Library] {
                assert_eq!(
                    classify("abseil", Some("C++ Common Libraries"), &source, Some(kind), None),
                    Category::Development,
                    "{source:?} + {kind:?} must fall back to Development"
                );
            }
        }
    }

    /// A Homebrew formula that installs no binary is `Unknown`, not `Library`
    /// — 109 of 323 on a real machine, `abseil` among them. Gating the
    /// fallback on `Cli | Tui | Library` would miss every one of them.
    #[test]
    fn brew_libraries_without_binaries_still_reach_development() {
        for kind in [None, Some(UiKind::Unknown), Some(UiKind::Library)] {
            assert_eq!(
                classify("abseil", Some("C++ Common Libraries"), &Source::Homebrew, kind, None),
                Category::Development,
                "a binary-less formula is still developer tooling"
            );
        }
    }

    /// `Gui` is the one disqualifier: a windowed app from any source is not
    /// developer tooling by default.
    #[test]
    fn windowed_packages_are_excluded_from_the_development_fallback() {
        for source in [Source::Homebrew, Source::Gem, Source::Pip] {
            assert_eq!(
                classify("some-app", Some("plain utility"), &source, Some(UiKind::Gui), None),
                Category::Uncategorized
            );
        }
    }

    #[test]
    fn pkgutil_falls_back_to_system() {
        let category = classify("system-component", Some("plain utility"), &Source::Pkgutil, None, None);

        assert_eq!(category, Category::System);
    }

    /// `Applications` is the source with no structural fallback, so it is what
    /// isolates the keyword pass in the tests below: a Homebrew or Pip entry
    /// would default to `Development` and mask the result being asserted.
    #[test]
    fn unmatched_entries_become_uncategorized() {
        let category = classify("plain-tool", Some("plain utility"), &Source::Applications, None, None);

        assert_eq!(category, Category::Uncategorized);
    }

    #[test]
    fn video_does_not_trigger_ide_false_positive() {
        let category = classify("plain-tool", Some("video utility"), &Source::Pip, None, None);

        assert_eq!(category, Category::Media);
    }

    #[test]
    fn decode_does_not_trigger_code_false_positive() {
        let category = classify("plain-tool", Some("decode utility"), &Source::Applications, None, None);

        assert_eq!(category, Category::Uncategorized);
    }

    /// Whole-token matching: `repl` must not fire on "replace", which is why
    /// stem matching is opt-in rather than the default.
    #[test]
    fn repl_does_not_fire_on_replace() {
        assert_eq!(
            classify("sd", Some("Find and replace text"), &Source::Applications, None, None),
            Category::Uncategorized
        );
    }

    /// Stems exist for exactly this: one entry covering the whole word family.
    #[test]
    fn stems_cover_word_families() {
        let security = |desc: &str| classify("t", Some(desc), &Source::Homebrew, None, None);

        assert_eq!(security("File encryption tool"), Category::Security);
        assert_eq!(security("Encrypts your files"), Category::Security);
        assert_eq!(security("Cryptographic primitives"), Category::Security);
    }

    /// System is last precisely so its broad terms cannot pre-empt the
    /// specific categories checked before it.
    #[test]
    fn system_is_the_catch_all_not_the_first_match() {
        let kind = |desc: &str| classify("t", Some(desc), &Source::Homebrew, None, None);

        // `monitor` is System, but a network-qualified monitor is Network.
        assert_eq!(kind("Network monitor"), Category::Network);
        assert_eq!(kind("Process monitor"), Category::System);
        // `emulator` is System's; only game-qualified emulators are Gaming.
        assert_eq!(kind("Terminal emulator"), Category::System);
        assert_eq!(kind("Console emulator for retro games"), Category::Gaming);
    }

    /// The contested-keyword rulings: a bare term owned by one category must
    /// yield to a qualified phrase owned by a later one.
    #[test]
    fn qualified_phrases_beat_the_bare_term_they_contain() {
        let kind = |desc: &str| classify("t", Some(desc), &Source::Homebrew, None, None);

        assert_eq!(kind("Web server"), Category::Network);
        assert_eq!(kind("Language server for Rust"), Category::Development);
        assert_eq!(kind("Database server"), Category::Data);

        assert_eq!(kind("Photo image viewer"), Category::Media);
        assert_eq!(kind("Build a container image"), Category::Development);
        assert_eq!(kind("Write a disk image to USB"), Category::System);

        assert_eq!(kind("Dependency graph explorer"), Category::Development);
        assert_eq!(kind("Graph query engine"), Category::Data);

        assert_eq!(kind("Matrix client for chat"), Category::Communication);
        assert_eq!(kind("Static analysis for C"), Category::Development);
        assert_eq!(kind("Bandwidth test utility"), Category::Network);
    }

    /// `editor` is Development's by default, but five categories own a
    /// qualified editor and three of them are checked after Development.
    #[test]
    fn editor_ownership_follows_its_qualifier() {
        let kind = |desc: &str| classify("t", Some(desc), &Source::Homebrew, None, None);

        assert_eq!(kind("A modern text editor"), Category::Development);
        assert_eq!(kind("Video editor"), Category::Media);
        assert_eq!(kind("Vector editor"), Category::Design);
        assert_eq!(kind("Hex editor"), Category::System);
        assert_eq!(kind("Markdown editor"), Category::Productivity);
    }

    /// Terms measured as non-discriminating are absent, so they must not
    /// classify anything on their own.
    #[test]
    fn dropped_terms_do_not_classify() {
        let kind = |desc: &str| classify("t", Some(desc), &Source::Applications, None, None);

        // 932 corpus hits, zero discriminating power.
        assert_eq!(kind("A C library"), Category::Uncategorized);
        assert_eq!(kind("Convert between units"), Category::Uncategorized);
        // `ranger` is "File browser" — bare `browser` must never mean Network.
        assert_eq!(kind("File browser"), Category::Uncategorized);
        assert_eq!(kind("Web browser"), Category::Network);
        // Bare `key` hits keyboard/keybinding/keyframe.
        assert_eq!(kind("Keyboard remapper"), Category::Uncategorized);
        assert_eq!(kind("Private key management"), Category::Security);
    }

    #[test]
    fn enrich_categories_sets_every_entry_category() {
        let mut entries = vec![
            entry("music-player", None, Source::Gem),
            entry("plain-tool", Some("plain utility"), Source::Applications),
        ];
        let cfg = UserConfig::default();

        enrich_categories(&mut entries, &cfg);

        assert_eq!(entries[0].category, Some(Category::Media));
        assert_eq!(entries[1].category, Some(Category::Uncategorized));
    }
}
