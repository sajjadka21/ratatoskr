use dm_common::{CategoryRecord, DownloadPriority, DownloadRule};
use reqwest::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleDecision {
    pub rule_id: Option<String>,
    pub explanation: String,
    pub category_id: Option<String>,
    pub queue_id: Option<String>,
    pub priority: Option<DownloadPriority>,
    pub destination_directory: Option<String>,
    pub max_connections: Option<u32>,
    pub max_host_concurrency: Option<u32>,
    pub speed_cap: Option<u64>,
}

pub fn evaluate_rules(
    source_url: &str,
    mime_type: Option<&str>,
    size: Option<u64>,
    rules: &[DownloadRule],
    categories: &[CategoryRecord],
) -> Option<RuleDecision> {
    let url = Url::parse(source_url).ok()?;
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    let extension = url
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .and_then(|name| name.rsplit_once('.'))
        .map(|(_, extension)| extension.to_ascii_lowercase());

    let matched = rules
        .iter()
        .filter(|rule| {
            rule.enabled
                && rule.domain.as_deref().is_none_or(|domain| {
                    host == domain.to_ascii_lowercase() || host.ends_with(&format!(".{domain}"))
                })
                && rule
                    .url_pattern
                    .as_deref()
                    .is_none_or(|pattern| wildcard_match(pattern, source_url))
                && rule.extension.as_deref().is_none_or(|expected| {
                    extension.as_deref()
                        == Some(
                            expected
                                .trim_start_matches('.')
                                .to_ascii_lowercase()
                                .as_str(),
                        )
                })
                && rule.mime_pattern.as_deref().is_none_or(|pattern| {
                    mime_type.is_some_and(|mime| wildcard_match(pattern, mime))
                })
                && rule
                    .min_size
                    .is_none_or(|minimum| size.is_some_and(|value| value >= minimum))
                && rule
                    .max_size
                    .is_none_or(|maximum| size.is_some_and(|value| value <= maximum))
        })
        .min_by_key(|rule| (rule.sort_order, rule.id.as_str()));

    let (category_id, explanation) = if let Some(rule) = matched {
        (
            rule.category_id.clone(),
            format!("rule '{}' matched", rule.name),
        )
    } else {
        let category = categories
            .iter()
            .find(|category| {
                mime_type.is_some_and(|mime| {
                    category
                        .mime_patterns
                        .iter()
                        .any(|pattern| wildcard_match(pattern, mime))
                })
            })
            .or_else(|| {
                categories.iter().find(|category| {
                    extension.as_deref().is_some_and(|value| {
                        category.extensions.iter().any(|candidate| {
                            candidate
                                .trim_start_matches('.')
                                .eq_ignore_ascii_case(value)
                        })
                    })
                })
            })
            .or_else(|| {
                categories.iter().find(|category| {
                    category
                        .host_patterns
                        .iter()
                        .any(|pattern| wildcard_match(pattern, &host))
                })
            })
            .or_else(|| categories.iter().find(|category| category.id == "other"));
        (
            category.map(|value| value.id.clone()),
            category.map_or_else(
                || "no category matched".to_owned(),
                |value| format!("category '{}' matched", value.name),
            ),
        )
    };

    Some(RuleDecision {
        rule_id: matched.map(|rule| rule.id.clone()),
        explanation,
        category_id,
        queue_id: matched.and_then(|rule| rule.queue_id.clone()),
        priority: matched.and_then(|rule| rule.priority),
        destination_directory: matched.and_then(|rule| rule.destination_directory.clone()),
        max_connections: matched.and_then(|rule| rule.max_connections),
        max_host_concurrency: matched.and_then(|rule| rule.max_host_concurrency),
        speed_cap: matched.and_then(|rule| rule.speed_cap),
    })
}

fn wildcard_match(pattern: &str, value: &str) -> bool {
    let pattern = pattern.to_ascii_lowercase();
    let value = value.to_ascii_lowercase();
    let parts = pattern.split('*').collect::<Vec<_>>();
    if parts.len() == 1 {
        return value == pattern;
    }
    let mut cursor = 0;
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        let Some(found) = value[cursor..].find(part) else {
            return false;
        };
        if index == 0 && found != 0 {
            return false;
        }
        cursor += found + part.len();
    }
    pattern.ends_with('*') || cursor == value.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn category(id: &str, extension: &str, mime: &str, host: &str) -> CategoryRecord {
        CategoryRecord {
            id: id.to_owned(),
            name: id.to_owned(),
            extensions: vec![extension.to_owned()],
            mime_patterns: vec![mime.to_owned()],
            default_directory: None,
            host_patterns: vec![host.to_owned()],
            priority: DownloadPriority::Normal,
            queue_id: None,
        }
    }

    #[test]
    fn ordered_rule_wins_and_explains_the_match() {
        let rules = vec![
            DownloadRule {
                id: "later".to_owned(),
                name: "later".to_owned(),
                enabled: true,
                sort_order: 5,
                domain: None,
                url_pattern: Some("*.example.com/*".to_owned()),
                extension: None,
                mime_pattern: None,
                min_size: None,
                max_size: None,
                category_id: None,
                destination_directory: None,
                queue_id: None,
                priority: None,
                max_connections: None,
                max_host_concurrency: None,
                speed_cap: None,
                browser_takeover_allowed: None,
            },
            DownloadRule {
                id: "first".to_owned(),
                name: "first".to_owned(),
                enabled: true,
                sort_order: 1,
                domain: Some("example.com".to_owned()),
                url_pattern: None,
                extension: None,
                mime_pattern: None,
                min_size: None,
                max_size: None,
                category_id: Some("docs".to_owned()),
                destination_directory: Some("C:\\Docs".to_owned()),
                queue_id: Some("default".to_owned()),
                priority: Some(DownloadPriority::High),
                max_connections: Some(2),
                max_host_concurrency: None,
                speed_cap: None,
                browser_takeover_allowed: None,
            },
        ];
        let decision = evaluate_rules(
            "https://example.com/file.pdf",
            Some("application/pdf"),
            Some(5),
            &rules,
            &[],
        )
        .unwrap();
        assert_eq!(decision.rule_id.as_deref(), Some("first"));
        assert_eq!(decision.priority, Some(DownloadPriority::High));
        assert!(decision.explanation.contains("first"));
    }

    #[test]
    fn category_precedence_is_mime_then_extension_then_host_then_other() {
        let categories = vec![
            category("video", "mp4", "video/*", "video.example.com"),
            category("other", "", "", ""),
        ];
        let decision = evaluate_rules(
            "https://example.com/file.mp4",
            Some("video/mp4"),
            None,
            &[],
            &categories,
        )
        .unwrap();
        assert_eq!(decision.category_id.as_deref(), Some("video"));
        assert!(decision.explanation.contains("video"));
    }
}
