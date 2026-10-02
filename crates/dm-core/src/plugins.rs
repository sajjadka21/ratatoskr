//! Plugins: small JSON files that adjust how links and files are handled,
//! so anyone can tune the app without changing it and new behaviour can be
//! shipped without a new release.
//!
//! A plugin is data, never code. It can only do four things, each limited to
//! a link, a file name or a request header, and every result is checked:
//!
//! ```json
//! { "schema": 1, "id": "my-site", "name": "My site", "version": "1.0.0",
//!   "rules": [
//!     { "type": "rewrite_url", "match": "https://example.com/view/*", "replace": "https://cdn.example.com/files/{1}.zip" },
//!     { "type": "rename", "match": "*.mp4.part", "replace": "{1}.mp4" },
//!     { "type": "referer", "host": "cdn.example.com", "value": "https://example.com/" },
//!     { "type": "user_agent", "host": "cdn.example.com", "value": "Mozilla/5.0" }
//!   ] }
//! ```
//!
//! Patterns use `*` for "anything" (up to four per pattern); `{1}`..`{4}` in
//! a replacement put back what the matching `*` caught. There are no regular
//! expressions, so a pattern cannot be made to run for a long time.

use http::Uri;
use serde::Deserialize;

pub const SCHEMA: u32 = 1;
pub const MAX_PLUGIN_BYTES: usize = 64 * 1024;
const MAX_RULES: usize = 50;
const MAX_WILDCARDS: usize = 4;
const MAX_PATTERN: usize = 512;
const MAX_INPUT: usize = 4096;
const MAX_HEADER_VALUE: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PluginError {
    #[error("the plugin file is too large")]
    TooLarge,
    #[error("the plugin is not valid JSON: {0}")]
    Json(String),
    #[error("this plugin needs a newer version of the app")]
    UnsupportedSchema,
    #[error("plugin id must be 1-40 characters: a-z, 0-9 and -")]
    BadId,
    #[error("a plugin can have at most {MAX_RULES} rules")]
    TooManyRules,
    #[error("invalid rule: {0}")]
    BadRule(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rule {
    RewriteUrl { pattern: String, replace: String },
    Rename { pattern: String, replace: String },
    Referer { host: String, value: String },
    UserAgent { host: String, value: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub rules: Vec<Rule>,
}

#[derive(Deserialize)]
struct RawPlugin {
    schema: u32,
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    rules: Vec<RawRule>,
}

#[derive(Deserialize)]
struct RawRule {
    #[serde(rename = "type")]
    kind: String,
    #[serde(rename = "match", default)]
    pattern: String,
    #[serde(default)]
    replace: String,
    #[serde(default)]
    host: String,
    #[serde(default)]
    value: String,
}

impl Plugin {
    pub fn parse(json: &str) -> Result<Self, PluginError> {
        if json.len() > MAX_PLUGIN_BYTES {
            return Err(PluginError::TooLarge);
        }
        let raw: RawPlugin =
            serde_json::from_str(json).map_err(|error| PluginError::Json(error.to_string()))?;
        if raw.schema != SCHEMA {
            return Err(PluginError::UnsupportedSchema);
        }
        let id_ok = (1..=40).contains(&raw.id.len())
            && raw
                .id
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
        if !id_ok {
            return Err(PluginError::BadId);
        }
        if raw.rules.len() > MAX_RULES {
            return Err(PluginError::TooManyRules);
        }
        let rules = raw
            .rules
            .into_iter()
            .map(parse_rule)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            name: if raw.name.trim().is_empty() {
                raw.id.clone()
            } else {
                raw.name.trim().chars().take(80).collect()
            },
            version: raw.version.trim().chars().take(20).collect(),
            id: raw.id,
            rules,
        })
    }
}

fn parse_rule(raw: RawRule) -> Result<Rule, PluginError> {
    let bad = |why: &str| PluginError::BadRule(format!("{}: {why}", raw.kind));
    match raw.kind.as_str() {
        "rewrite_url" | "rename" => {
            if raw.pattern.is_empty()
                || raw.pattern.len() > MAX_PATTERN
                || raw.replace.len() > MAX_PATTERN
            {
                return Err(bad("pattern or replacement is empty or too long"));
            }
            let wildcards = raw.pattern.matches('*').count();
            if wildcards > MAX_WILDCARDS {
                return Err(bad("too many *"));
            }
            if let Some(slot) =
                placeholders(&raw.replace).find(|slot| *slot == 0 || *slot > wildcards)
            {
                return Err(bad(&format!("{{{slot}}} has no matching *")));
            }
            Ok(if raw.kind == "rewrite_url" {
                Rule::RewriteUrl {
                    pattern: raw.pattern,
                    replace: raw.replace,
                }
            } else {
                Rule::Rename {
                    pattern: raw.pattern,
                    replace: raw.replace,
                }
            })
        }
        "referer" | "user_agent" => {
            let host = raw.host.trim().to_ascii_lowercase();
            let value_ok = !raw.value.is_empty()
                && raw.value.len() <= MAX_HEADER_VALUE
                && raw.value.bytes().all(|byte| (0x20..0x7f).contains(&byte));
            if host.is_empty() || host.contains(['/', ' ', '*']) || !value_ok {
                return Err(bad("needs a plain host and a printable value"));
            }
            if raw.kind == "referer" && !is_http(&raw.value) {
                return Err(bad("the referer must be an http(s) address"));
            }
            Ok(if raw.kind == "referer" {
                Rule::Referer {
                    host,
                    value: raw.value,
                }
            } else {
                Rule::UserAgent {
                    host,
                    value: raw.value,
                }
            })
        }
        other => Err(PluginError::BadRule(format!("unknown rule type {other}"))),
    }
}

/// The numbers inside `{n}` markers of a replacement.
fn placeholders(replace: &str) -> impl Iterator<Item = usize> + '_ {
    replace.split('{').skip(1).filter_map(|rest| {
        let (number, _) = rest.split_once('}')?;
        number.parse().ok()
    })
}

fn is_http(value: &str) -> bool {
    value.parse::<Uri>().ok().is_some_and(|uri| {
        matches!(uri.scheme_str(), Some("http" | "https")) && uri.host().is_some()
    })
}

/// Matches `text` against `pattern` where `*` stands for any text; returns what each `*` caught.
fn glob(pattern: &str, text: &str) -> Option<Vec<String>> {
    if text.len() > MAX_INPUT {
        return None;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return (pattern == text).then(Vec::new);
    }
    let first = parts[0];
    let last = parts[parts.len() - 1];
    let mut rest = text.strip_prefix(first)?;
    let mut caught = Vec::new();
    for middle in &parts[1..parts.len() - 1] {
        let at = rest.find(middle)?;
        caught.push(rest[..at].to_owned());
        rest = &rest[at + middle.len()..];
    }
    let body = rest.strip_suffix(last)?;
    caught.push(body.to_owned());
    Some(caught)
}

fn expand(replace: &str, caught: &[String]) -> String {
    let mut out = String::new();
    let mut rest = replace;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after
            .split_once('}')
            .and_then(|(n, tail)| Some((n.parse::<usize>().ok()?, tail)))
        {
            Some((slot, tail)) if slot >= 1 && slot <= caught.len() => {
                out.push_str(&caught[slot - 1]);
                rest = tail;
            }
            _ => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Every plugin the user has switched on, in the order they were added.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginSet {
    plugins: Vec<Plugin>,
}

impl PluginSet {
    pub fn new(plugins: Vec<Plugin>) -> Self {
        Self { plugins }
    }

    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    fn rules(&self) -> impl Iterator<Item = &Rule> {
        self.plugins.iter().flat_map(|plugin| plugin.rules.iter())
    }

    /// The address a link should really use. The first matching rule wins, and the
    /// result is kept only if it is still an http(s) address.
    pub fn rewrite_url(&self, url: &str) -> String {
        for rule in self.rules() {
            if let Rule::RewriteUrl { pattern, replace } = rule
                && let Some(caught) = glob(pattern, url)
            {
                let rewritten = expand(replace, &caught);
                if is_http(&rewritten) {
                    return rewritten;
                }
            }
        }
        url.to_owned()
    }

    /// The file name to save under; the first matching rule wins and the
    /// result may not become empty or contain a path separator.
    pub fn rename(&self, name: &str) -> String {
        for rule in self.rules() {
            if let Rule::Rename { pattern, replace } = rule
                && let Some(caught) = glob(pattern, name)
            {
                let renamed = expand(replace, &caught);
                if !renamed.trim().is_empty() && !renamed.contains(['/', '\\', '\0']) {
                    return renamed;
                }
            }
        }
        name.to_owned()
    }

    pub fn referer_for(&self, host: &str) -> Option<&str> {
        self.rules().find_map(|rule| match rule {
            Rule::Referer {
                host: wanted,
                value,
            } if host_matches(wanted, host) => Some(value.as_str()),
            _ => None,
        })
    }

    pub fn user_agent_for(&self, host: &str) -> Option<&str> {
        self.rules().find_map(|rule| match rule {
            Rule::UserAgent {
                host: wanted,
                value,
            } if host_matches(wanted, host) => Some(value.as_str()),
            _ => None,
        })
    }
}

/// `example.com` matches itself and its subdomains.
fn host_matches(wanted: &str, host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    host == wanted || host.ends_with(&format!(".{wanted}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{ "schema": 1, "id": "my-site", "name": "My site", "version": "1.0.0", "rules": [
        { "type": "rewrite_url", "match": "https://example.com/view/*", "replace": "https://cdn.example.com/files/{1}.zip" },
        { "type": "rename", "match": "*.mp4.part", "replace": "{1}.mp4" },
        { "type": "referer", "host": "cdn.example.com", "value": "https://example.com/" },
        { "type": "user_agent", "host": "cdn.example.com", "value": "Mozilla/5.0" } ] }"#;

    fn set() -> PluginSet {
        PluginSet::new(vec![Plugin::parse(SAMPLE).unwrap()])
    }

    #[test]
    fn a_valid_plugin_parses() {
        let plugin = Plugin::parse(SAMPLE).unwrap();
        assert_eq!(plugin.id, "my-site");
        assert_eq!(plugin.rules.len(), 4);
    }

    #[test]
    fn rewrites_a_matching_link_and_leaves_others_alone() {
        let set = set();
        assert_eq!(
            set.rewrite_url("https://example.com/view/42"),
            "https://cdn.example.com/files/42.zip"
        );
        assert_eq!(
            set.rewrite_url("https://other.org/view/42"),
            "https://other.org/view/42"
        );
    }

    #[test]
    fn renames_files_by_pattern() {
        let set = set();
        assert_eq!(set.rename("movie.mp4.part"), "movie.mp4");
        assert_eq!(set.rename("movie.mkv"), "movie.mkv");
    }

    #[test]
    fn headers_apply_to_the_host_and_its_subdomains_only() {
        let set = set();
        assert_eq!(
            set.referer_for("cdn.example.com"),
            Some("https://example.com/")
        );
        assert_eq!(set.user_agent_for("a.cdn.example.com"), Some("Mozilla/5.0"));
        assert_eq!(set.referer_for("evilcdn.example.com.attacker.net"), None);
        assert_eq!(set.referer_for("notcdn.example.com"), None);
    }

    #[test]
    fn a_rewrite_may_not_leave_http() {
        let plugin = Plugin::parse(
            r#"{ "schema": 1, "id": "x", "rules": [
            { "type": "rewrite_url", "match": "https://a.org/*", "replace": "file:///{1}" } ] }"#,
        )
        .unwrap();
        assert_eq!(
            PluginSet::new(vec![plugin]).rewrite_url("https://a.org/etc"),
            "https://a.org/etc"
        );
    }

    #[test]
    fn a_rename_may_not_become_a_path() {
        let plugin = Plugin::parse(
            r#"{ "schema": 1, "id": "x", "rules": [
            { "type": "rename", "match": "*", "replace": "../{1}" } ] }"#,
        )
        .unwrap();
        assert_eq!(PluginSet::new(vec![plugin]).rename("a.bin"), "a.bin");
    }

    #[test]
    fn bad_plugins_are_refused_with_a_reason() {
        let wrap = |rule: &str| format!(r#"{{ "schema": 1, "id": "x", "rules": [{rule}] }}"#);
        assert_eq!(
            Plugin::parse("nope").unwrap_err(),
            PluginError::Json("expected ident at line 1 column 2".into())
        );
        assert_eq!(
            Plugin::parse(r#"{"schema":2,"id":"x"}"#).unwrap_err(),
            PluginError::UnsupportedSchema
        );
        assert_eq!(
            Plugin::parse(r#"{"schema":1,"id":"Bad Id"}"#).unwrap_err(),
            PluginError::BadId
        );
        assert!(matches!(
            Plugin::parse(&wrap(r#"{"type":"run","match":"a"}"#)),
            Err(PluginError::BadRule(_))
        ));
        assert!(matches!(
            Plugin::parse(&wrap(r#"{"type":"rename","match":"a*","replace":"{2}"}"#)),
            Err(PluginError::BadRule(_))
        ));
        assert!(matches!(
            Plugin::parse(&wrap(r#"{"type":"rename","match":"*****a","replace":"x"}"#)),
            Err(PluginError::BadRule(_))
        ));
        assert!(matches!(
            Plugin::parse(&wrap(
                r#"{"type":"referer","host":"a.org","value":"javascript:1"}"#
            )),
            Err(PluginError::BadRule(_))
        ));
        assert!(matches!(
            Plugin::parse(&wrap(
                r#"{"type":"user_agent","host":"a.org","value":"a\nb"}"#
            )),
            Err(PluginError::BadRule(_))
        ));
        assert_eq!(
            Plugin::parse(&"x".repeat(MAX_PLUGIN_BYTES + 1)).unwrap_err(),
            PluginError::TooLarge
        );
    }

    #[test]
    fn glob_handles_edges_without_backtracking_blowups() {
        assert_eq!(
            glob("a*b*c", "aXXbYYc"),
            Some(vec!["XX".into(), "YY".into()])
        );
        assert_eq!(glob("*", ""), Some(vec![String::new()]));
        assert_eq!(glob("exact", "exact"), Some(vec![]));
        assert_eq!(glob("exact", "inexact"), None);
        assert_eq!(glob("a*b", &"a".repeat(MAX_INPUT + 1)), None);
        assert_eq!(glob("a*a*a*a*b", &"a".repeat(3000)), None);
    }
}
