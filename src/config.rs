//! User-editable category overrides.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::model::{AppEntry, Category};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UserConfig {
    overrides: HashMap<String, Category>,
    rules: Vec<Rule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Rule {
    contains: String,
    category: Category,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct RawUserConfig {
    #[serde(default)]
    overrides: HashMap<String, String>,
    #[serde(default)]
    rules: Vec<RawRule>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RawRule {
    contains: String,
    category: String,
}

/// Path to the categories file, or `None` if the platform has no home directory.
pub fn path() -> Option<PathBuf> {
    Some(
        dirs::home_dir()?
            .join(".config")
            .join("sysapp-tui")
            .join("categories.json"),
    )
}

/// A categories file this large is not a hand-edited override list.
///
/// Real configs stay in the low kilobytes even with many rules; 1 MB still
/// covers pathological but legitimate lists. Without the bound,
/// `read_to_string` will happily pull an arbitrary file into memory before
/// serde rejects it, and anything larger scales straight toward OOM.
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;

/// User edits must never crash or stall the TUI, so every read failure
/// collapses to the default empty config.
pub fn load() -> UserConfig {
    path().and_then(|path| load_from(&path)).unwrap_or_default()
}

/// The write path takes an explicit file rather than resolving `path()`
/// internally, so the TUI's category tests can point it at a scratch directory.
/// A version that resolved the real home directory could only be tested by
/// mutating the developer's own config, or by setting `$HOME` from a test —
/// and a process-global env mutation is a data race under the parallel test
/// runner, which is why the `PATH` equivalent was already reverted once here.
pub fn set_override_in(path: &Path, name: &str, cat: &Category) -> Result<()> {
    let mut config = load_from(path).unwrap_or_default();
    config.overrides.insert(name.to_owned(), cat.clone());
    save_to(path, &config)
}

/// Clearing the last override still rewrites the file: deleting it would take
/// the user's hand-written `rules` array with it.
pub fn clear_override_in(path: &Path, name: &str) -> Result<()> {
    let mut config = load_from(path).unwrap_or_default();
    config.overrides.remove(name);
    save_to(path, &config)
}

impl UserConfig {
    pub fn override_for(&self, name: &str) -> Option<Category> {
        self.overrides.get(name).cloned()
    }

    pub fn rule_for(&self, entry: &AppEntry) -> Option<Category> {
        self.rule_for_text(&entry.name, entry.description.as_deref())
    }
}

fn load_from(path: &Path) -> Option<UserConfig> {
    // Stat before opening. A fifo or device at this path would block
    // `read_to_string` forever, and a bad config must not hang the UI.
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_CONFIG_BYTES {
        return None;
    }

    let raw = std::fs::read_to_string(path).ok()?;
    let config: RawUserConfig = serde_json::from_str(&raw).ok()?;
    Some(UserConfig::from_raw(config))
}

/// Write the config, replacing any existing file.
///
/// Writes to a process-unique sibling temp file and renames it into place, so
/// an interrupted write cannot leave truncated JSON behind. The pid matters:
/// a fixed temp name would let concurrent instances clobber the same inode and
/// then rename corrupt data into place.
fn save_to(path: &Path, config: &UserConfig) -> Result<()> {
    let dir = path.parent().context("config path has no parent")?;
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;

    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    let mut data = serde_json::to_vec_pretty(&config.to_raw())?;
    data.push(b'\n');

    std::fs::write(&tmp, data).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

impl UserConfig {
    fn from_raw(raw: RawUserConfig) -> Self {
        let overrides = raw
            .overrides
            .into_iter()
            .map(|(name, category)| (name, Category::parse(&category)))
            .collect();
        let rules = raw
            .rules
            .into_iter()
            .map(|rule| Rule {
                contains: rule.contains,
                category: Category::parse(&rule.category),
            })
            .collect();

        Self { overrides, rules }
    }

    fn to_raw(&self) -> RawUserConfig {
        let overrides = self
            .overrides
            .iter()
            .map(|(name, category)| (name.clone(), category.key().to_owned()))
            .collect();
        let rules = self
            .rules
            .iter()
            .map(|rule| RawRule {
                contains: rule.contains.clone(),
                category: rule.category.key().to_owned(),
            })
            .collect();

        RawUserConfig { overrides, rules }
    }

    fn rule_for_text(&self, name: &str, description: Option<&str>) -> Option<Category> {
        let name = name.to_lowercase();
        let description = description.map(str::to_lowercase);

        self.rules.iter().find_map(|rule| {
            let needle = rule.contains.to_lowercase();
            let name_match = name.contains(&needle);
            let description_match = description
                .as_deref()
                .is_some_and(|description| description.contains(&needle));

            (name_match || description_match).then(|| rule.category.clone())
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs::File;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        clear_override_in, load_from, path, save_to, set_override_in, Rule, UserConfig,
    };
    use crate::model::Category;

    /// The directory name is namespaced to this module, not just tagged.
    ///
    /// `Drop` removes the whole parent directory, so any two modules that
    /// derive the same path delete each other's fixtures mid-run. `cache.rs`
    /// uses `sysapp-tui-test-{tag}-{pid}` and shares the tags `corrupt`,
    /// `empty` and `missing` with this module — identical tag plus identical
    /// pid produced the identical path, and Rust runs tests in parallel, so
    /// whichever dropped first failed the other. Renaming the tags would only
    /// re-arm the trap for the next module to copy this helper; the `config`
    /// segment is what makes a collision impossible. The nanosecond suffix
    /// covers reuse of one tag twice inside this module.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(tag: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let dir = std::env::temp_dir().join(format!(
                "sysapp-tui-config-tests-{tag}-{}-{nanos}",
                std::process::id()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir.join("categories.json"))
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn write(&self, contents: &str) {
            std::fs::write(&self.0, contents).unwrap();
        }

        fn tmp_path(&self) -> PathBuf {
            self.0
                .with_extension(format!("{}.tmp", std::process::id()))
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(self.0.parent().unwrap());
        }
    }

    #[test]
    fn path_points_into_user_config() {
        if let Some(path) = path() {
            assert!(path.ends_with(".config/sysapp-tui/categories.json"));
        }
    }

    #[test]
    fn missing_file_returns_default() {
        let scratch = Scratch::new("missing");
        assert_eq!(load_from(scratch.path()), None);
    }

    #[test]
    fn corrupt_json_returns_default() {
        let scratch = Scratch::new("corrupt");
        scratch.write("{");
        assert_eq!(load_from(scratch.path()), None);
    }

    #[test]
    fn wrong_types_return_default() {
        let scratch = Scratch::new("wrong-types");
        scratch.write(r#"{ "overrides": [] }"#);
        assert_eq!(load_from(scratch.path()), None);
    }

    #[test]
    fn empty_file_returns_default() {
        let scratch = Scratch::new("empty");
        scratch.write("");
        assert_eq!(load_from(scratch.path()), None);
    }

    #[test]
    fn oversized_file_returns_default() {
        let scratch = Scratch::new("oversized");
        File::create(scratch.path()).unwrap().set_len(super::MAX_CONFIG_BYTES + 1).unwrap();
        assert_eq!(load_from(scratch.path()), None);
    }

    #[test]
    fn loads_file_with_only_overrides() {
        let scratch = Scratch::new("only-overrides");
        scratch.write(r#"{ "overrides": { "ghostty": "Terminal" } }"#);

        let config = load_from(scratch.path()).unwrap();

        assert_eq!(
            config.override_for("ghostty"),
            Some(Category::Custom("Terminal".to_owned()))
        );
        assert!(config.rules.is_empty());
    }

    #[test]
    fn loads_file_with_only_rules() {
        let scratch = Scratch::new("only-rules");
        scratch.write(r#"{ "rules": [ { "contains": "kubectl", "category": "DevOps" } ] }"#);

        let config = load_from(scratch.path()).unwrap();

        assert_eq!(
            config.rule_for_text("tool", Some("wraps kubectl plugins")),
            Some(Category::Custom("DevOps".to_owned()))
        );
        assert!(config.overrides.is_empty());
    }

    #[test]
    fn save_load_round_trip_preserves_custom_categories() {
        let scratch = Scratch::new("round-trip");
        let config = UserConfig {
            overrides: HashMap::from([(
                "ghostty".to_owned(),
                Category::Custom("Terminal".to_owned()),
            )]),
            rules: vec![Rule {
                contains: "kubectl".to_owned(),
                category: Category::Custom("DevOps".to_owned()),
            }],
        };

        save_to(scratch.path(), &config).unwrap();
        let loaded = load_from(scratch.path()).unwrap();

        assert_eq!(loaded, config);
    }

    #[test]
    fn set_override_preserves_existing_rules() {
        let scratch = Scratch::new("preserve-rules");
        scratch.write(
            r#"{ "rules": [ { "contains": "kubectl", "category": "DevOps" } ] }"#,
        );

        set_override_in(scratch.path(), "ghostty", &Category::Custom("Terminal".to_owned()))
            .unwrap();

        let loaded = load_from(scratch.path()).unwrap();
        assert_eq!(
            loaded.override_for("ghostty"),
            Some(Category::Custom("Terminal".to_owned()))
        );
        assert_eq!(
            loaded.rule_for_text("tool", Some("kubectl wrapper")),
            Some(Category::Custom("DevOps".to_owned()))
        );
    }

    #[test]
    fn set_override_leaves_no_tmp_file_behind() {
        let scratch = Scratch::new("tmp-cleanup");

        set_override_in(scratch.path(), "ghostty", &Category::Development).unwrap();

        assert!(!scratch.tmp_path().exists());
    }

    #[test]
    fn clear_override_removes_existing_override_and_preserves_rules() {
        let scratch = Scratch::new("clear-preserve-rules");
        scratch.write(
            r#"{
  "overrides": { "ghostty": "Terminal" },
  "rules": [ { "contains": "kubectl", "category": "DevOps" } ]
}"#,
        );

        clear_override_in(scratch.path(), "ghostty").unwrap();

        let loaded = load_from(scratch.path()).unwrap();
        assert_eq!(loaded.override_for("ghostty"), None);
        assert_eq!(
            loaded.rule_for_text("tool", Some("kubectl wrapper")),
            Some(Category::Custom("DevOps".to_owned()))
        );
    }

    #[test]
    fn clear_override_of_missing_name_is_ok_and_leaves_config_unchanged() {
        let scratch = Scratch::new("clear-missing");
        scratch.write(
            r#"{
  "overrides": { "ghostty": "Terminal" },
  "rules": [ { "contains": "kubectl", "category": "DevOps" } ]
}"#,
        );
        let before = load_from(scratch.path()).unwrap();

        clear_override_in(scratch.path(), "wezterm").unwrap();

        let after = load_from(scratch.path()).unwrap();
        assert_eq!(after, before);
    }

    #[test]
    fn clear_override_of_last_entry_leaves_parseable_empty_overrides_file() {
        let scratch = Scratch::new("clear-last");
        scratch.write(r#"{ "overrides": { "ghostty": "Terminal" } }"#);

        clear_override_in(scratch.path(), "ghostty").unwrap();

        let loaded = load_from(scratch.path());
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().override_for("ghostty"), None);
    }

    #[test]
    fn clear_override_leaves_no_tmp_file_behind() {
        let scratch = Scratch::new("clear-tmp-cleanup");
        scratch.write(r#"{ "overrides": { "ghostty": "Terminal" } }"#);

        clear_override_in(scratch.path(), "ghostty").unwrap();

        assert!(!scratch.tmp_path().exists());
    }

    #[test]
    fn override_for_miss_returns_none() {
        let config = UserConfig::default();
        assert_eq!(config.override_for("ghostty"), None);
    }

    #[test]
    fn rule_for_matches_description_case_insensitively() {
        let config = UserConfig {
            overrides: HashMap::new(),
            rules: vec![Rule {
                contains: "kubectl".to_owned(),
                category: Category::Custom("DevOps".to_owned()),
            }],
        };

        assert_eq!(
            config.rule_for_text("tool", Some("Wraps KUBECTL plugins")),
            Some(Category::Custom("DevOps".to_owned()))
        );
    }

    #[test]
    fn rule_for_matches_name_case_insensitively() {
        let config = UserConfig {
            overrides: HashMap::new(),
            rules: vec![Rule {
                contains: "gh".to_owned(),
                category: Category::Communication,
            }],
        };

        assert_eq!(
            config.rule_for_text("GHOSTTY", Some("terminal")),
            Some(Category::Communication)
        );
    }

    #[test]
    fn rule_for_uses_first_matching_rule() {
        let config = UserConfig {
            overrides: HashMap::new(),
            rules: vec![
                Rule {
                    contains: "gh".to_owned(),
                    category: Category::Communication,
                },
                Rule {
                    contains: "ghostty".to_owned(),
                    category: Category::Productivity,
                },
            ],
        };

        assert_eq!(
            config.rule_for_text("ghostty", Some("terminal")),
            Some(Category::Communication)
        );
    }
}
