use crate::providers::{ListSort, OverlayMode, ResponseSourceMetadata};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    Hero,
    Item,
}

impl EntryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hero => "hero",
            Self::Item => "item",
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum SearchType {
    All,
    Hero,
    Item,
}

impl SearchType {
    pub fn matches(self, kind: EntryKind) -> bool {
        match self {
            Self::All => true,
            Self::Hero => kind == EntryKind::Hero,
            Self::Item => kind == EntryKind::Item,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Hero => "hero",
            Self::Item => "item",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EntryOverlay {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bullets: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub popularity: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub win_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_size: Option<u64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attributes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeEntry {
    pub kind: EntryKind,
    pub slug: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub popularity: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub win_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overlay: Option<EntryOverlay>,
}

impl KnowledgeEntry {
    pub fn matches_tag(&self, raw_tag: &str) -> bool {
        let normalized_tag = normalize(raw_tag);
        if normalized_tag.is_empty() {
            return true;
        }

        self.tags.iter().any(|tag| normalize(tag) == normalized_tag)
            || self
                .aliases
                .iter()
                .any(|alias| normalize(alias) == normalized_tag)
            || normalize(&self.summary).contains(&normalized_tag)
    }

    fn lookup_keys(&self) -> Vec<String> {
        let mut keys = vec![self.name.clone(), self.slug.clone(), self.summary.clone()];
        keys.extend(self.aliases.clone());
        keys.extend(self.tags.clone());
        keys.extend(self.details.clone());
        keys
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub kind: EntryKind,
    pub name: String,
    pub slug: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub popularity: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub win_rate: Option<f64>,
    pub score: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResponse {
    pub query: String,
    pub requested_type: SearchType,
    pub match_count: usize,
    pub source: ResponseSourceMetadata,
    pub results: Vec<SearchResult>,
}

pub struct SearchRequest<'a> {
    pub query: &'a str,
    pub requested_type: SearchType,
    pub tag: Option<&'a str>,
    pub limit: usize,
    pub expand: bool,
    pub effective_context: &'a BTreeMap<String, String>,
    pub source: ResponseSourceMetadata,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListEntry {
    pub kind: EntryKind,
    pub name: String,
    pub slug: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub popularity: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub win_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

impl From<&KnowledgeEntry> for ListEntry {
    fn from(entry: &KnowledgeEntry) -> Self {
        Self {
            kind: entry.kind,
            name: entry.name.clone(),
            slug: entry.slug.clone(),
            summary: entry.summary.clone(),
            tags: entry.tags.clone(),
            provider: entry.provider.clone(),
            popularity: entry.popularity,
            win_rate: entry.win_rate,
            updated_at: entry.updated_at.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ListResponse {
    pub requested_type: EntryKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    pub sort: ListSort,
    pub returned_count: usize,
    pub source: ResponseSourceMetadata,
    pub entries: Vec<ListEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShowResponse {
    pub kind: EntryKind,
    pub name: String,
    pub slug: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<String>,
    pub overlay_mode: OverlayMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub live_overlay: Option<EntryOverlay>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_entries: Vec<ListEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub popularity: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub win_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    pub source: ResponseSourceMetadata,
}

pub fn slugify(value: &str) -> String {
    let mut out = String::new();
    let mut previous_dash = false;

    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            out.push(character.to_ascii_lowercase());
            previous_dash = false;
        } else if !previous_dash {
            out.push('-');
            previous_dash = true;
        }
    }

    out.trim_matches('-').to_string()
}

pub fn search(entries: &[KnowledgeEntry], request: SearchRequest<'_>) -> SearchResponse {
    let normalized_query = normalize(request.query);
    let query_tokens = token_set(&normalized_query);
    let limit = request.limit.max(1);
    let tag_filter = request.tag.map(normalize);

    let mut matches = entries
        .iter()
        .filter(|entry| request.requested_type.matches(entry.kind))
        .filter(|entry| {
            tag_filter
                .as_ref()
                .is_none_or(|candidate| entry.matches_tag(candidate))
        })
        .filter_map(|entry| {
            let score = score_entry(
                entry,
                &normalized_query,
                &query_tokens,
                request.effective_context,
            );
            (score > 0.0).then_some((score, entry))
        })
        .collect::<Vec<_>>();

    matches.sort_by(|left, right| {
        compare_desc(left.0, right.0)
            .then_with(|| {
                compare_desc(
                    left.1.popularity.unwrap_or_default(),
                    right.1.popularity.unwrap_or_default(),
                )
            })
            .then_with(|| left.1.name.cmp(&right.1.name))
    });

    let results = matches
        .into_iter()
        .take(limit)
        .map(|(score, entry)| SearchResult {
            kind: entry.kind,
            name: entry.name.clone(),
            slug: entry.slug.clone(),
            summary: entry.summary.clone(),
            tags: entry.tags.clone(),
            provider: entry.provider.clone(),
            popularity: entry.popularity,
            win_rate: entry.win_rate,
            score: round_score(score),
            details: expanded_search_details(entry, request.expand),
        })
        .collect::<Vec<_>>();

    SearchResponse {
        query: request.query.trim().to_string(),
        requested_type: request.requested_type,
        match_count: results.len(),
        source: request.source,
        results,
    }
}

pub fn show_entry(
    kind: EntryKind,
    name: &str,
    related: bool,
    overlay_mode: OverlayMode,
    entries: &[KnowledgeEntry],
    source: ResponseSourceMetadata,
) -> Option<ShowResponse> {
    let needle = normalize(name);
    let entry = entries
        .iter()
        .filter(|entry| entry.kind == kind)
        .find(|entry| matches_lookup(entry, &needle))?;

    let related_entries = if related {
        entry
            .related
            .iter()
            .filter_map(|related_name| {
                entries
                    .iter()
                    .find(|candidate| {
                        matches_lookup(candidate, &normalize(related_name))
                            || candidate.name.eq_ignore_ascii_case(related_name)
                    })
                    .map(ListEntry::from)
            })
            .collect()
    } else {
        Vec::new()
    };

    let live_overlay = match overlay_mode {
        OverlayMode::Basic => None,
        OverlayMode::Stats | OverlayMode::Full => entry.overlay.clone(),
    };

    Some(ShowResponse {
        kind: entry.kind,
        name: entry.name.clone(),
        slug: entry.slug.clone(),
        summary: entry.summary.clone(),
        aliases: entry.aliases.clone(),
        tags: entry.tags.clone(),
        details: entry.details.clone(),
        overlay_mode,
        live_overlay,
        related_entries,
        provider: entry.provider.clone(),
        popularity: entry.popularity,
        win_rate: entry.win_rate,
        updated_at: entry.updated_at.clone(),
        source,
    })
}

pub fn list_entries(
    kind: EntryKind,
    tag: Option<&str>,
    limit: usize,
    sort: ListSort,
    effective_context: &BTreeMap<String, String>,
    entries: &[KnowledgeEntry],
    source: ResponseSourceMetadata,
) -> ListResponse {
    let limit = limit.max(1);
    let tag_filter = tag.map(normalize);

    let mut filtered = entries
        .iter()
        .filter(|entry| entry.kind == kind)
        .filter(|entry| {
            tag_filter
                .as_ref()
                .is_none_or(|candidate| entry.matches_tag(candidate))
        })
        .cloned()
        .collect::<Vec<_>>();

    sort_entries(&mut filtered, sort, effective_context);

    let entries = filtered
        .iter()
        .take(limit)
        .map(ListEntry::from)
        .collect::<Vec<_>>();

    ListResponse {
        requested_type: kind,
        tag: tag.map(str::to_string),
        sort,
        returned_count: entries.len(),
        source,
        entries,
    }
}

fn expanded_search_details(_entry: &KnowledgeEntry, _expand: bool) -> Vec<String> {
    Vec::new()
}

fn matches_lookup(entry: &KnowledgeEntry, normalized_query: &str) -> bool {
    if normalized_query.is_empty() {
        return false;
    }

    entry.lookup_keys().iter().any(|value| {
        let normalized = normalize(value);
        normalized == normalized_query || normalized.contains(normalized_query)
    })
}

fn score_entry(
    entry: &KnowledgeEntry,
    normalized_query: &str,
    query_tokens: &BTreeSet<String>,
    effective_context: &BTreeMap<String, String>,
) -> f64 {
    if normalized_query.is_empty() {
        return 0.0;
    }

    let normalized_name = normalize(&entry.name);
    let alias_tokens = entry
        .aliases
        .iter()
        .map(|alias| normalize(alias))
        .collect::<Vec<_>>();
    let tag_tokens = entry
        .tags
        .iter()
        .map(|tag| normalize(tag))
        .collect::<Vec<_>>();
    let haystacks = entry
        .lookup_keys()
        .into_iter()
        .map(|value| normalize(&value))
        .collect::<Vec<_>>();

    let mut score = 0.0;

    if normalized_name == normalized_query {
        score += 120.0;
    } else if alias_tokens.iter().any(|alias| alias == normalized_query) {
        score += 100.0;
    } else if normalized_name.contains(normalized_query) {
        score += 75.0;
    }

    for token in query_tokens {
        if token.is_empty() {
            continue;
        }

        if normalized_name
            .split_whitespace()
            .any(|candidate| candidate == token)
        {
            score += 20.0;
        } else if normalized_name.contains(token) {
            score += 12.0;
        }

        if alias_tokens.iter().any(|alias| alias.contains(token)) {
            score += 15.0;
        }

        if tag_tokens.iter().any(|tag| tag == token) {
            score += 10.0;
        } else if tag_tokens.iter().any(|tag| tag.contains(token)) {
            score += 6.0;
        }

        if haystacks.iter().any(|value| value.contains(token)) {
            score += 3.0;
        }
    }

    for selector in effective_context.values() {
        let normalized_selector = normalize(selector);
        if !normalized_selector.is_empty()
            && tag_tokens
                .iter()
                .any(|tag| tag == &normalized_selector || tag.contains(&normalized_selector))
        {
            score += 4.0;
        }
    }

    if let Some(popularity) = entry.popularity {
        score += (popularity / 50.0).min(8.0);
    }

    if let Some(win_rate) = entry.win_rate {
        score += ((win_rate - 45.0) / 4.0).clamp(0.0, 5.0);
    }

    score
}

fn sort_entries(
    entries: &mut [KnowledgeEntry],
    sort: ListSort,
    effective_context: &BTreeMap<String, String>,
) {
    entries.sort_by(|left, right| match sort {
        ListSort::Name => left.name.cmp(&right.name),
        ListSort::Popularity => compare_desc(
            left.popularity.unwrap_or(f64::NEG_INFINITY),
            right.popularity.unwrap_or(f64::NEG_INFINITY),
        )
        .then_with(|| left.name.cmp(&right.name)),
        ListSort::Winrate => compare_desc(
            left.win_rate.unwrap_or(f64::NEG_INFINITY),
            right.win_rate.unwrap_or(f64::NEG_INFINITY),
        )
        .then_with(|| left.name.cmp(&right.name)),
        ListSort::Updated => compare_desc(
            parse_timestamp(&left.updated_at),
            parse_timestamp(&right.updated_at),
        )
        .then_with(|| {
            context_bias(right, effective_context).cmp(&context_bias(left, effective_context))
        })
        .then_with(|| left.name.cmp(&right.name)),
    });
}

fn context_bias(entry: &KnowledgeEntry, effective_context: &BTreeMap<String, String>) -> usize {
    let entry_tags = entry
        .tags
        .iter()
        .map(|tag| normalize(tag))
        .collect::<BTreeSet<_>>();
    effective_context
        .values()
        .map(|value| normalize(value))
        .filter(|value| entry_tags.contains(value))
        .count()
}

fn normalize(value: &str) -> String {
    let mut cleaned = String::new();
    let mut previous_space = false;

    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            cleaned.push(character.to_ascii_lowercase());
            previous_space = false;
        } else if !previous_space {
            cleaned.push(' ');
            previous_space = true;
        }
    }

    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn token_set(value: &str) -> BTreeSet<String> {
    value.split_whitespace().map(str::to_string).collect()
}

fn round_score(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn compare_desc(left: f64, right: f64) -> Ordering {
    right.partial_cmp(&left).unwrap_or(Ordering::Equal)
}

fn parse_timestamp(value: &Option<String>) -> f64 {
    value
        .as_deref()
        .and_then(|candidate| candidate.parse::<f64>().ok())
        .unwrap_or(f64::NEG_INFINITY)
}

pub fn find_hero_by_name(entries: &[KnowledgeEntry], name: &str) -> Option<u32> {
    let normalized_name = normalize(name);
    entries
        .iter()
        .filter(|entry| entry.kind == EntryKind::Hero)
        .find(|entry| {
            let entry_name = normalize(&entry.name);
            entry_name == normalized_name
                || normalize(&entry.slug) == normalized_name
                || entry
                    .aliases
                    .iter()
                    .any(|alias| normalize(alias) == normalized_name)
        })
        .and_then(|entry| entry.provider_id.as_ref())
        .and_then(|id| id.parse::<u32>().ok())
}
