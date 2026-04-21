use crate::ErrorContext;
use crate::context::RuntimeLocations;
use crate::encyclopedia::{EntryKind, EntryOverlay, KnowledgeEntry, slugify};
use anyhow::{Context, Result};
use clap::ValueEnum;
use reqwest::StatusCode;
use reqwest::blocking::Client;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const OPENDOTA_HERO_TTL_SEC: u64 = 900;
const OPENDOTA_ITEM_TTL_SEC: u64 = 900;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum SourceSelector {
    Auto,
    Opendota,
    Stratz,
    CacheOnly,
}

impl SourceSelector {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Opendota => "opendota",
            Self::Stratz => "stratz",
            Self::CacheOnly => "cache-only",
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderSourceSelector {
    Auto,
    Opendota,
    Stratz,
}

impl ProviderSourceSelector {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Opendota => "opendota",
            Self::Stratz => "stratz",
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum FreshnessMode {
    Live,
    Recent,
    CachedOk,
}

impl FreshnessMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Recent => "recent",
            Self::CachedOk => "cached-ok",
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum OverlayMode {
    Basic,
    Stats,
    Full,
}

impl OverlayMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Basic => "basic",
            Self::Stats => "stats",
            Self::Full => "full",
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum ListSort {
    Name,
    Popularity,
    Winrate,
    Updated,
}

impl ListSort {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Popularity => "popularity",
            Self::Winrate => "winrate",
            Self::Updated => "updated",
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum WarmScope {
    Indexes,
    Details,
    All,
}

impl WarmScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Indexes => "indexes",
            Self::Details => "details",
            Self::All => "all",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseSourceMetadata {
    pub requested_source: String,
    pub resolved_sources: Vec<String>,
    pub freshness: String,
    pub cache_state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_age_sec: Option<u64>,
    pub live_data_used: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fetched_at: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderDataset {
    pub entries: Vec<KnowledgeEntry>,
    pub source: ResponseSourceMetadata,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceStatusOutput {
    pub requested_source: String,
    pub freshness: String,
    pub checked_at: String,
    pub providers: Vec<ProviderStatus>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderStatus {
    pub provider: String,
    pub configured: bool,
    pub auth: String,
    pub reachability: String,
    pub cache_state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_age_sec: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fetched_at: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supported_surfaces: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceWarmOutput {
    pub requested_source: String,
    pub scope: String,
    pub force: bool,
    pub results: Vec<WarmResult>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WarmResult {
    pub provider: String,
    pub dataset: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fetched_at: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEnvelope<T> {
    fetched_at: u64,
    expires_at: u64,
    value: T,
}

#[derive(Debug, Clone)]
struct CacheRead<T> {
    value: T,
    cache_state: String,
    cache_age_sec: Option<u64>,
    fetched_at: Option<String>,
    cache_path: PathBuf,
}

#[derive(Debug, Clone)]
struct OpendotaCatalogs {
    heroes: CacheRead<Vec<OpenDotaHeroStat>>,
    items: CacheRead<BTreeMap<String, OpenDotaItem>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenDotaHeroStat {
    id: u32,
    name: String,
    localized_name: String,
    primary_attr: Option<String>,
    attack_type: Option<String>,
    #[serde(default)]
    roles: Vec<String>,
    pro_pick: Option<u64>,
    pro_win: Option<u64>,
    move_speed: Option<u32>,
    base_armor: Option<f64>,
    base_health: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenDotaItem {
    id: Option<u32>,
    dname: Option<String>,
    qual: Option<String>,
    cost: Option<u32>,
    notes: Option<String>,
    #[serde(default)]
    hint: Vec<String>,
    #[serde(default)]
    components: Option<serde_json::Value>,
    #[serde(default)]
    mc: Option<serde_json::Value>,
    #[serde(default)]
    cd: Option<serde_json::Value>,
    lore: Option<String>,
}

pub fn load_live_entries(
    runtime: &RuntimeLocations,
    requested_source: SourceSelector,
    freshness: FreshnessMode,
) -> std::result::Result<ProviderDataset, ErrorContext> {
    match requested_source {
        SourceSelector::Auto | SourceSelector::Opendota | SourceSelector::CacheOnly => {
            load_opendota_entries(runtime, requested_source, freshness)
        }
        SourceSelector::Stratz => {
            let has_token = std::env::var("STRATZ_API_TOKEN")
                .ok()
                .is_some_and(|value| !value.trim().is_empty());
            if !has_token {
                Err(ErrorContext::new(
                    "provider.auth_required",
                    "STRATZ surfaces require STRATZ_API_TOKEN before live lookups can run",
                    "provider_routing",
                )
                .with_detail("provider", "stratz"))
            } else {
                load_stratz_entries(runtime, freshness)
            }
        }
    }
}

pub fn source_status(
    runtime: &RuntimeLocations,
    requested_source: ProviderSourceSelector,
    freshness: FreshnessMode,
) -> std::result::Result<SourceStatusOutput, ErrorContext> {
    let mut providers = Vec::new();

    match requested_source {
        ProviderSourceSelector::Auto => {
            providers.push(opendota_status(runtime, freshness)?);
            providers.push(stratz_status());
        }
        ProviderSourceSelector::Opendota => providers.push(opendota_status(runtime, freshness)?),
        ProviderSourceSelector::Stratz => providers.push(stratz_status()),
    }

    Ok(SourceStatusOutput {
        requested_source: requested_source.as_str().to_string(),
        freshness: freshness.as_str().to_string(),
        checked_at: now_string(),
        providers,
    })
}

pub fn source_warm(
    runtime: &RuntimeLocations,
    requested_source: ProviderSourceSelector,
    scope: WarmScope,
    force: bool,
) -> std::result::Result<SourceWarmOutput, ErrorContext> {
    let mut results = Vec::new();
    let mut notes = Vec::new();
    let effective_freshness = if force {
        FreshnessMode::Live
    } else {
        FreshnessMode::Recent
    };

    if matches!(scope, WarmScope::Details | WarmScope::All) {
        notes.push(
            "This revision warms the shared hero/item indexes that back both discovery and detail lookups."
                .to_string(),
        );
    }

    match requested_source {
        ProviderSourceSelector::Auto | ProviderSourceSelector::Opendota => {
            let catalogs = load_opendota_catalogs(runtime, effective_freshness, false)?;
            results.push(WarmResult {
                provider: "opendota".to_string(),
                dataset: "hero_index".to_string(),
                status: catalogs.heroes.cache_state.clone(),
                record_count: Some(catalogs.heroes.value.len()),
                cache_path: Some(catalogs.heroes.cache_path.display().to_string()),
                fetched_at: catalogs.heroes.fetched_at.clone(),
                notes: Vec::new(),
            });
            results.push(WarmResult {
                provider: "opendota".to_string(),
                dataset: "item_index".to_string(),
                status: catalogs.items.cache_state.clone(),
                record_count: Some(catalogs.items.value.len()),
                cache_path: Some(catalogs.items.cache_path.display().to_string()),
                fetched_at: catalogs.items.fetched_at.clone(),
                notes: Vec::new(),
            });
        }
        ProviderSourceSelector::Stratz => {
            // Warm STRATZ encyclopedia data (heroes and items)
            match load_stratz_entries(runtime, effective_freshness) {
                Ok(dataset) => {
                    let hero_count = dataset
                        .entries
                        .iter()
                        .filter(|e| e.kind == EntryKind::Hero)
                        .count();
                    let item_count = dataset
                        .entries
                        .iter()
                        .filter(|e| e.kind == EntryKind::Item)
                        .count();
                    results.push(WarmResult {
                        provider: "stratz".to_string(),
                        dataset: "hero_index".to_string(),
                        status: dataset.source.cache_state.clone(),
                        record_count: Some(hero_count),
                        cache_path: Some(
                            runtime
                                .cache_dir
                                .join("live-providers")
                                .join("stratz-hero-stats.json")
                                .display()
                                .to_string(),
                        ),
                        fetched_at: dataset.source.fetched_at.clone(),
                        notes: Vec::new(),
                    });
                    results.push(WarmResult {
                        provider: "stratz".to_string(),
                        dataset: "item_index".to_string(),
                        status: dataset.source.cache_state.clone(),
                        record_count: Some(item_count),
                        cache_path: Some(
                            runtime
                                .cache_dir
                                .join("live-providers")
                                .join("stratz-items.json")
                                .display()
                                .to_string(),
                        ),
                        fetched_at: dataset.source.fetched_at.clone(),
                        notes: Vec::new(),
                    });
                }
                Err(e) => {
                    results.push(WarmResult {
                        provider: "stratz".to_string(),
                        dataset: "encyclopedia".to_string(),
                        status: "error".to_string(),
                        record_count: None,
                        cache_path: None,
                        fetched_at: None,
                        notes: vec![format!("STRATZ warming failed: {}", e.message())],
                    });
                }
            }
        }
    }

    if matches!(
        requested_source,
        ProviderSourceSelector::Auto | ProviderSourceSelector::Stratz
    ) {
        let mut stratz_notes = Vec::new();
        let token_present = std::env::var("STRATZ_API_TOKEN")
            .ok()
            .is_some_and(|value| !value.trim().is_empty());
        let status = if token_present {
            stratz_notes.push(
                "Provider credentials are present, but cache warming for STRATZ-backed encyclopedia surfaces is not implemented yet."
                    .to_string(),
            );
            "unsupported_surface"
        } else {
            stratz_notes.push("Set STRATZ_API_TOKEN to enable future STRATZ warming.".to_string());
            "auth_missing"
        };

        results.push(WarmResult {
            provider: "stratz".to_string(),
            dataset: "provider_metadata".to_string(),
            status: status.to_string(),
            record_count: None,
            cache_path: None,
            fetched_at: None,
            notes: stratz_notes,
        });
    }

    Ok(SourceWarmOutput {
        requested_source: requested_source.as_str().to_string(),
        scope: scope.as_str().to_string(),
        force,
        results,
        notes,
    })
}

fn load_opendota_entries(
    runtime: &RuntimeLocations,
    requested_source: SourceSelector,
    freshness: FreshnessMode,
) -> std::result::Result<ProviderDataset, ErrorContext> {
    let mut metadata_notes = Vec::new();
    let effective_freshness = if requested_source == SourceSelector::CacheOnly {
        metadata_notes
            .push("Cache-only routing forced cached reads and skipped live fetches.".to_string());
        FreshnessMode::CachedOk
    } else {
        freshness
    };

    let catalogs = load_opendota_catalogs(runtime, effective_freshness, true)?;
    let heroes_updated_at = catalogs.heroes.fetched_at.clone();
    let items_updated_at = catalogs.items.fetched_at.clone();

    let mut entries = catalogs
        .heroes
        .value
        .iter()
        .map(|hero| hero_entry(hero, heroes_updated_at.clone()))
        .collect::<Vec<_>>();
    entries.extend(
        catalogs
            .items
            .value
            .iter()
            .map(|(key, item)| item_entry(key, item, items_updated_at.clone())),
    );

    if requested_source == SourceSelector::Auto
        && std::env::var("STRATZ_API_TOKEN")
            .ok()
            .is_some_and(|value| !value.trim().is_empty())
    {
        metadata_notes.push(
            "STRATZ credentials were detected, but this revision still uses OpenDota for encyclopedia hero/item surfaces."
                .to_string(),
        );
    }

    let cache_state = combine_cache_states(&[
        catalogs.heroes.cache_state.as_str(),
        catalogs.items.cache_state.as_str(),
    ]);
    let cache_age_sec = max_age(&[catalogs.heroes.cache_age_sec, catalogs.items.cache_age_sec]);
    let fetched_at = latest_timestamp(&[
        catalogs.heroes.fetched_at.clone(),
        catalogs.items.fetched_at.clone(),
    ]);

    Ok(ProviderDataset {
        entries,
        source: ResponseSourceMetadata {
            requested_source: requested_source.as_str().to_string(),
            resolved_sources: vec!["opendota".to_string()],
            freshness: effective_freshness.as_str().to_string(),
            cache_state,
            cache_age_sec,
            live_data_used: matches!(
                catalogs.heroes.cache_state.as_str(),
                "live_fetch" | "fetched_after_cache_miss"
            ) || matches!(
                catalogs.items.cache_state.as_str(),
                "live_fetch" | "fetched_after_cache_miss"
            ),
            fetched_at,
            notes: metadata_notes,
        },
    })
}

fn opendota_status(
    runtime: &RuntimeLocations,
    freshness: FreshnessMode,
) -> std::result::Result<ProviderStatus, ErrorContext> {
    let catalogs = load_opendota_catalogs(runtime, freshness, true)?;
    Ok(ProviderStatus {
        provider: "opendota".to_string(),
        configured: true,
        auth: if std::env::var("OPENDOTA_API_KEY")
            .ok()
            .is_some_and(|value| !value.trim().is_empty())
        {
            "optional_token_present".to_string()
        } else {
            "public".to_string()
        },
        reachability: "reachable".to_string(),
        cache_state: combine_cache_states(&[
            catalogs.heroes.cache_state.as_str(),
            catalogs.items.cache_state.as_str(),
        ]),
        cache_age_sec: max_age(&[catalogs.heroes.cache_age_sec, catalogs.items.cache_age_sec]),
        fetched_at: latest_timestamp(&[
            catalogs.heroes.fetched_at.clone(),
            catalogs.items.fetched_at.clone(),
        ]),
        supported_surfaces: vec![
            "hero stats".to_string(),
            "item constants".to_string(),
            "hero and item encyclopedia indexes".to_string(),
        ],
        notes: Vec::new(),
    })
}

fn stratz_status() -> ProviderStatus {
    let configured = std::env::var("STRATZ_API_TOKEN")
        .ok()
        .is_some_and(|value| !value.trim().is_empty());

    let mut notes = vec![
        "STRATZ remains an optional richer provider for future analytics overlays.".to_string(),
    ];
    if configured {
        notes.push(
            "Provider credentials are present, but this revision does not yet fetch encyclopedia hero/item indexes from STRATZ."
                .to_string(),
        );
    } else {
        notes.push("Set STRATZ_API_TOKEN to enable richer provider routing later.".to_string());
    }

    ProviderStatus {
        provider: "stratz".to_string(),
        configured,
        auth: if configured {
            "token_present".to_string()
        } else {
            "missing".to_string()
        },
        reachability: if configured {
            "not_checked".to_string()
        } else {
            "auth_required".to_string()
        },
        cache_state: "not_warmed".to_string(),
        cache_age_sec: None,
        fetched_at: None,
        supported_surfaces: vec![
            "hero trends".to_string(),
            "guide overlays".to_string(),
            "player and match enrichments".to_string(),
        ],
        notes,
    }
}

fn load_opendota_catalogs(
    runtime: &RuntimeLocations,
    freshness: FreshnessMode,
    fail_on_cache_miss: bool,
) -> std::result::Result<OpendotaCatalogs, ErrorContext> {
    let cache_root = runtime.cache_dir.join("live-providers");
    fs::create_dir_all(&cache_root).map_err(|error| {
        ErrorContext::new(
            "provider.cache_write_failed",
            format!("failed to create cache directory: {error}"),
            "provider_cache",
        )
        .with_detail("cache_root", cache_root.display().to_string())
    })?;

    let client = build_client().map_err(|error| {
        ErrorContext::new(
            "provider.unreachable",
            format!("failed to initialize HTTP client: {error:#}"),
            "provider_client",
        )
    })?;

    let hero_url = opendota_url("heroStats");
    let item_url = opendota_url("constants/items");

    let heroes = load_or_fetch_json::<Vec<OpenDotaHeroStat>>(
        &client,
        &cache_root.join("opendota-hero-stats.json"),
        OPENDOTA_HERO_TTL_SEC,
        freshness,
        "opendota",
        &hero_url,
        fail_on_cache_miss,
    )?;
    let items = load_or_fetch_json::<BTreeMap<String, OpenDotaItem>>(
        &client,
        &cache_root.join("opendota-items.json"),
        OPENDOTA_ITEM_TTL_SEC,
        freshness,
        "opendota",
        &item_url,
        fail_on_cache_miss,
    )?;

    Ok(OpendotaCatalogs { heroes, items })
}

fn hero_entry(hero: &OpenDotaHeroStat, updated_at: Option<String>) -> KnowledgeEntry {
    let short_name = hero
        .name
        .trim_start_matches("npc_dota_hero_")
        .replace('_', " ");
    let popularity = hero.pro_pick.map(|value| value as f64);
    let win_rate = match (hero.pro_pick, hero.pro_win) {
        (Some(picks), Some(wins)) if picks > 0 => Some((wins as f64 / picks as f64) * 100.0),
        _ => None,
    };
    let mut tags = hero.roles.clone();
    if let Some(primary_attr) = &hero.primary_attr {
        tags.push(primary_attr.to_uppercase());
    }
    if let Some(attack_type) = &hero.attack_type {
        tags.push(attack_type.clone());
    }

    let mut overlay_attributes = BTreeMap::new();
    if let Some(primary_attr) = &hero.primary_attr {
        overlay_attributes.insert("primary_attr".to_string(), primary_attr.clone());
    }
    if let Some(attack_type) = &hero.attack_type {
        overlay_attributes.insert("attack_type".to_string(), attack_type.clone());
    }
    if let Some(move_speed) = hero.move_speed {
        overlay_attributes.insert("move_speed".to_string(), move_speed.to_string());
    }
    if let Some(base_armor) = hero.base_armor {
        overlay_attributes.insert("base_armor".to_string(), format!("{base_armor:.1}"));
    }
    if let Some(base_health) = hero.base_health {
        overlay_attributes.insert("base_health".to_string(), base_health.to_string());
    }

    KnowledgeEntry {
        kind: EntryKind::Hero,
        slug: slugify(&hero.localized_name),
        name: hero.localized_name.clone(),
        aliases: vec![short_name, hero.name.clone()],
        summary: format!(
            "{} {} hero with roles {}",
            humanize_primary_attr(hero.primary_attr.as_deref()),
            hero.attack_type
                .as_deref()
                .unwrap_or("unknown attack type")
                .to_ascii_lowercase(),
            comma_join(&hero.roles)
        ),
        details: vec![
            format!(
                "Primary attribute: {}.",
                humanize_primary_attr(hero.primary_attr.as_deref())
            ),
            format!(
                "Attack type: {}. Core roles: {}.",
                hero.attack_type.as_deref().unwrap_or("Unknown"),
                comma_join(&hero.roles)
            ),
            format!(
                "OpenDota pro sample: {} picks{}.",
                hero.pro_pick.unwrap_or(0),
                win_rate
                    .map(|value| format!(", {:.1}% win rate", value))
                    .unwrap_or_default()
            ),
        ],
        tags,
        related: role_related_concepts(&hero.roles),
        provider: Some("opendota".to_string()),
        provider_id: Some(hero.id.to_string()),
        popularity,
        win_rate,
        updated_at,
        overlay: Some(EntryOverlay {
            bullets: vec![
                format!(
                    "Pro pick volume: {}{}",
                    hero.pro_pick.unwrap_or(0),
                    win_rate
                        .map(|value| format!(" with {:.1}% win rate", value))
                        .unwrap_or_default()
                ),
                hero.move_speed
                    .map(|value| format!("Move speed: {value}"))
                    .unwrap_or_else(|| "Move speed unavailable".to_string()),
            ],
            popularity,
            win_rate,
            sample_size: hero.pro_pick,
            attributes: overlay_attributes,
        }),
    }
}

fn item_entry(key: &str, item: &OpenDotaItem, updated_at: Option<String>) -> KnowledgeEntry {
    let display_name = item.dname.clone().unwrap_or_else(|| titleize_item_key(key));
    let mut details = Vec::new();
    if let Some(notes) = &item.notes {
        details.push(notes.clone());
    }
    details.extend(item.hint.iter().take(2).cloned());
    if details.is_empty() {
        details.push("OpenDota returned a structured item record without extra notes.".to_string());
    }

    let mut tags = Vec::new();
    if let Some(quality) = &item.qual {
        tags.push(quality.clone());
    }
    if let Some(cost) = item.cost {
        if cost >= 4000 {
            tags.push("late-game".to_string());
        } else if cost <= 2000 {
            tags.push("early-game".to_string());
        }
    }
    if display_name.to_ascii_lowercase().contains("ward") {
        tags.push("vision".to_string());
    }
    if display_name.to_ascii_lowercase().contains("blink") {
        tags.push("initiation".to_string());
    }

    let mut overlay_attributes = BTreeMap::new();
    if let Some(cost) = item.cost {
        overlay_attributes.insert("cost".to_string(), cost.to_string());
    }
    if let Some(quality) = &item.qual {
        overlay_attributes.insert("quality".to_string(), quality.clone());
    }
    if let Some(cooldown) = &item.cd {
        overlay_attributes.insert("cooldown".to_string(), cooldown.to_string());
    }
    if let Some(mana_cost) = &item.mc {
        overlay_attributes.insert("mana_cost".to_string(), mana_cost.to_string());
    }

    KnowledgeEntry {
        kind: EntryKind::Item,
        slug: slugify(&display_name),
        name: display_name.clone(),
        aliases: vec![key.replace('_', " ")],
        summary: format!(
            "{} item{}",
            item.qual.clone().unwrap_or_else(|| "utility".to_string()),
            item.cost
                .map(|value| format!(" costing {value} gold"))
                .unwrap_or_default()
        ),
        details,
        tags,
        related: item_related_concepts(&display_name, item.qual.as_deref()),
        provider: Some("opendota".to_string()),
        provider_id: item.id.map(|value| value.to_string()),
        popularity: None,
        win_rate: None,
        updated_at,
        overlay: Some(EntryOverlay {
            bullets: vec![
                item.cost
                    .map(|value| format!("Cost: {value} gold"))
                    .unwrap_or_else(|| "Cost unavailable".to_string()),
                item.cd
                    .as_ref()
                    .map(|value| format!("Cooldown: {value} seconds"))
                    .unwrap_or_else(|| "Cooldown unavailable".to_string()),
            ],
            popularity: None,
            win_rate: None,
            sample_size: None,
            attributes: overlay_attributes,
        }),
    }
}

fn load_or_fetch_json<T>(
    client: &Client,
    cache_path: &Path,
    ttl_sec: u64,
    freshness: FreshnessMode,
    provider: &str,
    url: &str,
    fail_on_cache_miss: bool,
) -> std::result::Result<CacheRead<T>, ErrorContext>
where
    T: DeserializeOwned + Serialize,
{
    let cached = read_cache::<T>(cache_path).map_err(|error| {
        ErrorContext::new(
            "provider.cache_read_failed",
            format!("failed to read provider cache: {error:#}"),
            "provider_cache",
        )
        .with_detail("cache_path", cache_path.display().to_string())
    })?;

    if let Some(cached) = cached {
        let cache_age_sec = now().saturating_sub(cached.fetched_at);
        let fresh = cache_age_sec <= ttl_sec;

        match freshness {
            FreshnessMode::CachedOk => {
                return Ok(CacheRead {
                    value: cached.value,
                    cache_state: if fresh {
                        "fresh_cache".to_string()
                    } else {
                        "stale_cache".to_string()
                    },
                    cache_age_sec: Some(cache_age_sec),
                    fetched_at: Some(cached.fetched_at.to_string()),
                    cache_path: cache_path.to_path_buf(),
                });
            }
            FreshnessMode::Recent if fresh => {
                return Ok(CacheRead {
                    value: cached.value,
                    cache_state: "fresh_cache".to_string(),
                    cache_age_sec: Some(cache_age_sec),
                    fetched_at: Some(cached.fetched_at.to_string()),
                    cache_path: cache_path.to_path_buf(),
                });
            }
            FreshnessMode::Live | FreshnessMode::Recent => {}
        }
    } else if freshness == FreshnessMode::CachedOk && fail_on_cache_miss {
        return Err(ErrorContext::new(
            "provider.cache_miss",
            "requested cached provider data is not available yet; run `dota-agent-cli source warm` first",
            "provider_cache",
        )
        .with_detail("provider", provider)
        .with_detail("cache_path", cache_path.display().to_string()));
    }

    let value = fetch_json::<T>(client, provider, url)?;
    let fetched_at = now();
    write_cache(cache_path, ttl_sec, fetched_at, &value).map_err(|error| {
        ErrorContext::new(
            "provider.cache_write_failed",
            format!("failed to write provider cache: {error:#}"),
            "provider_cache",
        )
        .with_detail("cache_path", cache_path.display().to_string())
    })?;

    Ok(CacheRead {
        value,
        cache_state: if fail_on_cache_miss {
            "live_fetch".to_string()
        } else {
            "fetched_after_cache_miss".to_string()
        },
        cache_age_sec: Some(0),
        fetched_at: Some(fetched_at.to_string()),
        cache_path: cache_path.to_path_buf(),
    })
}

fn read_cache<T>(path: &Path) -> Result<Option<CacheEnvelope<T>>>
where
    T: DeserializeOwned,
{
    if !path.exists() {
        return Ok(None);
    }

    let raw =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let envelope = serde_json::from_str(&raw)
        .with_context(|| format!("failed to decode {}", path.display()))?;
    Ok(Some(envelope))
}

fn write_cache<T>(path: &Path, ttl_sec: u64, fetched_at: u64, value: &T) -> Result<()>
where
    T: Serialize,
{
    let envelope = CacheEnvelope {
        fetched_at,
        expires_at: fetched_at + ttl_sec,
        value,
    };
    let serialized = serde_json::to_string_pretty(&envelope)
        .with_context(|| format!("failed to encode {}", path.display()))?;
    fs::write(path, serialized).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

pub(crate) fn fetch_json<T>(
    client: &Client,
    provider: &str,
    url: &str,
) -> std::result::Result<T, ErrorContext>
where
    T: DeserializeOwned,
{
    let mut request = client.get(url);
    if provider == "opendota"
        && let Ok(api_key) = std::env::var("OPENDOTA_API_KEY")
        && !api_key.trim().is_empty()
    {
        request = request.query(&[("api_key", api_key)]);
    }

    let response = request.send().map_err(|error| {
        ErrorContext::new(
            "provider.unreachable",
            format!("failed to reach {provider}: {error}"),
            "provider_http",
        )
        .with_detail("provider", provider)
        .with_detail("url", url)
    })?;

    let status = response.status();
    if !status.is_success() {
        return Err(http_status_error(provider, url, status));
    }

    response.json::<T>().map_err(|error| {
        ErrorContext::new(
            "provider.decode_failed",
            format!("failed to decode {provider} response: {error}"),
            "provider_http",
        )
        .with_detail("provider", provider)
        .with_detail("url", url)
    })
}

pub(crate) fn build_client() -> Result<Client> {
    Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .context("failed to construct reqwest blocking client")
}

fn http_status_error(provider: &str, url: &str, status: StatusCode) -> ErrorContext {
    let code = match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => "provider.auth_required",
        StatusCode::TOO_MANY_REQUESTS => "provider.rate_limited",
        _ => "provider.unreachable",
    };
    ErrorContext::new(
        code,
        format!("{provider} returned HTTP {}", status.as_u16()),
        "provider_http",
    )
    .with_detail("provider", provider)
    .with_detail("url", url)
    .with_detail("status", status.as_u16().to_string())
}

pub(crate) fn opendota_url(path: &str) -> String {
    let base_url = std::env::var("DOTA_AGENT_CLI_OPENDOTA_BASE_URL")
        .or_else(|_| std::env::var("DOTA_CLI_OPENDOTA_BASE_URL"))
        .unwrap_or_else(|_| "https://api.opendota.com/api".to_string());
    format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

fn role_related_concepts(roles: &[String]) -> Vec<String> {
    let mut concepts = Vec::new();
    let normalized_roles = roles
        .iter()
        .map(|role| role.to_ascii_lowercase())
        .collect::<Vec<_>>();
    if normalized_roles
        .iter()
        .any(|role| role.contains("initiator") || role.contains("disabler"))
    {
        concepts.push("Initiation".to_string());
    }
    if normalized_roles
        .iter()
        .any(|role| role.contains("support") || role.contains("jungler"))
    {
        concepts.push("Vision".to_string());
    }
    if normalized_roles
        .iter()
        .any(|role| role.contains("pusher") || role.contains("escape"))
    {
        concepts.push("Map Pressure".to_string());
    }
    concepts
}

fn item_related_concepts(display_name: &str, quality: Option<&str>) -> Vec<String> {
    let lower = display_name.to_ascii_lowercase();
    let mut concepts = Vec::new();
    if lower.contains("ward") || lower.contains("gem") {
        concepts.push("Vision".to_string());
    }
    if lower.contains("blink") || lower.contains("force") {
        concepts.push("Initiation".to_string());
    }
    if lower.contains("glimmer") || lower.contains("lotus") {
        concepts.push("Save".to_string());
    }
    if quality.is_some_and(|value| value.eq_ignore_ascii_case("component")) {
        concepts.push("Power Spike".to_string());
    }
    concepts
}

fn humanize_primary_attr(value: Option<&str>) -> String {
    match value.unwrap_or_default() {
        "str" => "Strength".to_string(),
        "agi" => "Agility".to_string(),
        "int" => "Intelligence".to_string(),
        "all" => "Universal".to_string(),
        _ => "Unknown".to_string(),
    }
}

fn titleize_item_key(key: &str) -> String {
    key.split('_')
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let mut characters = segment.chars();
            match characters.next() {
                Some(first) => format!(
                    "{}{}",
                    first.to_ascii_uppercase(),
                    characters.as_str().to_ascii_lowercase()
                ),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn comma_join(values: &[String]) -> String {
    if values.is_empty() {
        "none listed".to_string()
    } else {
        values.join(", ")
    }
}

fn combine_cache_states(states: &[&str]) -> String {
    if states.iter().all(|state| *state == "live_fetch") {
        "live_fetch".to_string()
    } else if states.iter().all(|state| *state == "fresh_cache") {
        "fresh_cache".to_string()
    } else if states.contains(&"stale_cache") {
        "stale_cache".to_string()
    } else {
        "mixed".to_string()
    }
}

fn latest_timestamp(values: &[Option<String>]) -> Option<String> {
    values
        .iter()
        .filter_map(|value| value.as_ref())
        .max()
        .cloned()
}

fn max_age(values: &[Option<u64>]) -> Option<u64> {
    values.iter().flatten().max().copied()
}

pub(crate) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

pub(crate) fn now_string() -> String {
    now().to_string()
}

// ============================================================================
// STRATZ GraphQL API Integration
// ============================================================================

const STRATZ_GRAPHQL_URL: &str = "https://api.stratz.com/graphql";

fn stratz_url(path: &str) -> String {
    let base =
        std::env::var("STRATZ_API_BASE_URL").unwrap_or_else(|_| STRATZ_GRAPHQL_URL.to_string());
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

fn stratz_client() -> std::result::Result<Client, ErrorContext> {
    Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| {
            ErrorContext::new(
                "provider.unreachable",
                format!("failed to construct STRATZ client: {}", e),
                "stratz_client",
            )
        })
}

fn stratz_fetch_json<T: DeserializeOwned>(
    client: &Client,
    query: &str,
    variables: Option<BTreeMap<String, serde_json::Value>>,
) -> std::result::Result<T, ErrorContext> {
    let token = std::env::var("STRATZ_API_TOKEN").map_err(|_| {
        ErrorContext::new(
            "provider.auth_required",
            "STRATZ API token not found in STRATZ_API_TOKEN",
            "stratz_provider",
        )
    })?;

    let mut body = BTreeMap::new();
    body.insert(
        "query".to_string(),
        serde_json::Value::String(query.to_string()),
    );
    if let Some(vars) = variables {
        body.insert(
            "variables".to_string(),
            serde_json::Value::Object(vars.into_iter().collect()),
        );
    }

    let request = client
        .post(stratz_url("graphql"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .json(&body);

    let response = request.send().map_err(|error| {
        ErrorContext::new(
            "provider.unreachable",
            format!("failed to reach STRATZ: {error}"),
            "stratz_http",
        )
        .with_detail("provider", "stratz")
    })?;

    let status = response.status();
    if !status.is_success() {
        return Err(http_status_error("stratz", STRATZ_GRAPHQL_URL, status));
    }

    #[derive(Deserialize)]
    struct GraphQLResponse<T> {
        data: Option<T>,
        errors: Option<Vec<serde_json::Value>>,
    }

    let graphql_response: GraphQLResponse<T> = response.json().map_err(|error| {
        ErrorContext::new(
            "provider.decode_failed",
            format!("failed to decode STRATZ response: {error}"),
            "stratz_http",
        )
    })?;

    if let Some(errors) = graphql_response.errors {
        let error_msg = errors
            .iter()
            .filter_map(|e| e.get("message").and_then(|m| m.as_str()))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(ErrorContext::new(
            "provider.graphql_error",
            format!("STRATZ GraphQL error: {}", error_msg),
            "stratz_graphql",
        ));
    }

    graphql_response.data.ok_or_else(|| {
        ErrorContext::new(
            "provider.decode_failed",
            "STRATZ response missing data field",
            "stratz_graphql",
        )
    })
}

// STRATZ Hero stats response structure
#[derive(Debug, Clone, Deserialize)]
struct StratzHeroStatsResponse {
    #[serde(rename = "heroes")]
    heroes: Option<StratzHeroList>,
}

#[derive(Debug, Clone, Deserialize)]
struct StratzHeroList {
    #[serde(rename = "top")]
    top: Option<Vec<StratzHero>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct StratzHero {
    #[serde(rename = "id")]
    id: Option<u32>,
    #[serde(rename = "name")]
    name: Option<String>,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    #[serde(rename = "primaryAttribute")]
    primary_attribute: Option<String>,
    #[serde(rename = "type")]
    hero_type: Option<String>,
    #[serde(rename = "attackType")]
    attack_type: Option<String>,
    #[serde(rename = "roles")]
    roles: Option<Vec<String>>,
    #[serde(rename = "stats")]
    stats: Option<StratzHeroStats>,
    #[serde(rename = "alias")]
    alias: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct StratzHeroStats {
    #[serde(rename = "proPick")]
    pro_pick: Option<u64>,
    #[serde(rename = "proWin")]
    pro_win: Option<u64>,
    #[serde(rename = "proBan")]
    pro_ban: Option<u64>,
    #[serde(rename = "1Pick")]
    pick_1: Option<u64>,
    #[serde(rename = "1Win")]
    win_1: Option<u64>,
    #[serde(rename = "2Pick")]
    pick_2: Option<u64>,
    #[serde(rename = "2Win")]
    win_2: Option<u64>,
    #[serde(rename = "3Pick")]
    pick_3: Option<u64>,
    #[serde(rename = "3Win")]
    win_3: Option<u64>,
    #[serde(rename = "4Pick")]
    pick_4: Option<u64>,
    #[serde(rename = "4Win")]
    win_4: Option<u64>,
    #[serde(rename = "5Pick")]
    pick_5: Option<u64>,
    #[serde(rename = "5Win")]
    win_5: Option<u64>,
    #[serde(rename = "6Pick")]
    pick_6: Option<u64>,
    #[serde(rename = "6Win")]
    win_6: Option<u64>,
    #[serde(rename = "7Pick")]
    pick_7: Option<u64>,
    #[serde(rename = "7Win")]
    win_7: Option<u64>,
    #[serde(rename = "8Pick")]
    pick_8: Option<u64>,
    #[serde(rename = "8Win")]
    win_8: Option<u64>,
    #[serde(rename = "turboPicks")]
    turbo_picks: Option<u64>,
    #[serde(rename = "turboWins")]
    turbo_wins: Option<u64>,
}

fn stratz_load_hero_stats(client: &Client) -> std::result::Result<Vec<StratzHero>, ErrorContext> {
    // STRATZ GraphQL query for hero stats
    let query = r#"
        query HeroStats {
            heroes {
                top {
                    id
                    name
                    displayName
                    primaryAttribute
                    type
                    attackType
                    roles
                    stats {
                        proPick
                        proWin
                        proBan
                        1Pick
                        1Win
                        2Pick
                        2Win
                        3Pick
                        3Win
                        4Pick
                        4Win
                        5Pick
                        5Win
                        6Pick
                        6Win
                        7Pick
                        7Win
                        8Pick
                        8Win
                        turboPicks
                        turboWins
                    }
                    alias
                }
            }
        }
    "#;

    let response: StratzHeroStatsResponse = stratz_fetch_json(client, query, None)?;
    Ok(response.heroes.and_then(|h| h.top).unwrap_or_default())
}

// STRATZ Item constants response
#[derive(Debug, Clone, Deserialize)]
struct StratzItemResponse {
    #[serde(rename = "items")]
    items: Option<StratzItemList>,
}

#[derive(Debug, Clone, Deserialize)]
struct StratzItemList {
    #[serde(rename = "top")]
    top: Option<Vec<StratzItem>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct StratzItem {
    #[serde(rename = "id")]
    id: Option<u32>,
    #[serde(rename = "name")]
    name: Option<String>,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    #[serde(rename = "qual")]
    qual: Option<String>,
    #[serde(rename = "cost")]
    cost: Option<u32>,
    #[serde(rename = "notes")]
    notes: Option<String>,
    #[serde(rename = "hint")]
    hint: Option<Vec<String>>,
    #[serde(rename = "components")]
    components: Option<Vec<String>>,
    #[serde(rename = "cooldown")]
    cooldown: Option<f64>,
    #[serde(rename = "manaCost")]
    mana_cost: Option<u32>,
    #[serde(rename = "lore")]
    lore: Option<String>,
}

fn stratz_load_items(
    client: &Client,
) -> std::result::Result<BTreeMap<String, StratzItem>, ErrorContext> {
    // STRATZ GraphQL query for items
    let query = r#"
        query Items {
            items {
                top {
                    id
                    name
                    displayName
                    qual
                    cost
                    notes
                    hint
                    components
                    cooldown
                    manaCost
                    lore
                }
            }
        }
    "#;

    let response: StratzItemResponse = stratz_fetch_json(client, query, None)?;
    let items = response.items.and_then(|i| i.top).unwrap_or_default();
    let mut map = BTreeMap::new();
    for item in items {
        if let Some(name) = &item.name {
            map.insert(name.clone(), item);
        }
    }
    Ok(map)
}

fn load_stratz_entries(
    runtime: &RuntimeLocations,
    freshness: FreshnessMode,
) -> std::result::Result<ProviderDataset, ErrorContext> {
    let cache_root = runtime.cache_dir.join("live-providers");
    fs::create_dir_all(&cache_root).map_err(|error| {
        ErrorContext::new(
            "provider.cache_write_failed",
            format!("failed to create cache directory: {error}"),
            "stratz_cache",
        )
        .with_detail("cache_root", cache_root.display().to_string())
    })?;

    let client = stratz_client()?;
    let heroes_cache_path = cache_root.join("stratz-hero-stats.json");
    let items_cache_path = cache_root.join("stratz-items.json");

    // Load or fetch heroes
    let (heroes, heroes_cache_state, heroes_cache_age, heroes_fetched_at) =
        stratz_load_or_cache(&heroes_cache_path, OPENDOTA_HERO_TTL_SEC, freshness, || {
            stratz_load_hero_stats(&client)
        })?;

    // Load or fetch items
    let (items, items_cache_state, items_cache_age, items_fetched_at) =
        stratz_load_or_cache(&items_cache_path, OPENDOTA_ITEM_TTL_SEC, freshness, || {
            stratz_load_items(&client)
        })?;

    let mut entries = Vec::new();

    // Convert STRATZ heroes to KnowledgeEntry
    for hero in &heroes {
        if let (Some(id), Some(name)) = (
            hero.id,
            hero.display_name.clone().or_else(|| hero.name.clone()),
        ) {
            let short_name = name.trim_start_matches("npc_dota_hero_").replace('_', " ");
            let stats = hero.stats.as_ref();

            let popularity = stats.and_then(|s| s.pro_pick).map(|v| v as f64);
            let win_rate = stats
                .and_then(|s| s.pro_pick.and_then(|p| s.pro_win.map(|w| (p, w))))
                .and_then(|(p, w)| (p > 0).then_some((w as f64 / p as f64) * 100.0));

            let mut tags = hero.roles.clone().unwrap_or_default();
            if let Some(attr) = &hero.primary_attribute {
                tags.push(attr.to_uppercase());
            }
            if let Some(at) = &hero.attack_type {
                tags.push(at.clone());
            }

            let mut overlay_attributes = BTreeMap::new();
            if let Some(attr) = &hero.primary_attribute {
                overlay_attributes.insert("primary_attr".to_string(), attr.clone());
            }
            if let Some(at) = &hero.attack_type {
                overlay_attributes.insert("attack_type".to_string(), at.clone());
            }
            if let Some(stats) = stats {
                overlay_attributes.insert(
                    "pro_pick".to_string(),
                    stats.pro_pick.unwrap_or(0).to_string(),
                );
            }

            entries.push(KnowledgeEntry {
                kind: EntryKind::Hero,
                slug: slugify(&name),
                name: name.clone(),
                aliases: {
                    let mut aliases = vec![short_name.clone()];
                    if let Some(hero_alias) = &hero.alias {
                        aliases.extend(hero_alias.clone());
                    }
                    aliases
                },
                summary: format!(
                    "{} {} hero with roles {}",
                    humanize_primary_attr(hero.primary_attribute.as_deref()),
                    hero.attack_type
                        .as_deref()
                        .unwrap_or("unknown attack type")
                        .to_ascii_lowercase(),
                    hero.roles
                        .as_ref()
                        .map(|r| r.join(", "))
                        .unwrap_or_else(|| "none listed".to_string())
                ),
                details: vec![
                    format!(
                        "Primary attribute: {}.",
                        humanize_primary_attr(hero.primary_attribute.as_deref())
                    ),
                    format!(
                        "Attack type: {}.",
                        hero.attack_type.as_deref().unwrap_or("Unknown")
                    ),
                    format!(
                        "Roles: {}.",
                        hero.roles
                            .as_ref()
                            .map(|r| r.join(", "))
                            .unwrap_or_else(|| "none listed".to_string())
                    ),
                ],
                tags,
                related: vec![],
                provider: Some("stratz".to_string()),
                provider_id: Some(id.to_string()),
                popularity,
                win_rate,
                updated_at: heroes_fetched_at.clone(),
                overlay: Some(EntryOverlay {
                    bullets: vec![format!(
                        "Pro pick: {}{}",
                        stats.and_then(|s| s.pro_pick).unwrap_or(0),
                        win_rate
                            .map(|w| format!(" with {:.1}% win rate", w))
                            .unwrap_or_default()
                    )],
                    popularity,
                    win_rate,
                    sample_size: stats.and_then(|s| s.pro_pick),
                    attributes: overlay_attributes,
                }),
            });
        }
    }

    // Convert STRATZ items to KnowledgeEntry
    for (key, item) in &items {
        let display_name = item
            .display_name
            .clone()
            .unwrap_or_else(|| titleize_item_key(key));
        let mut details = Vec::new();
        if let Some(notes) = &item.notes {
            details.push(notes.clone());
        }
        if let Some(hints) = &item.hint {
            details.extend(hints.iter().take(2).cloned());
        }
        if details.is_empty() {
            details
                .push("STRATZ returned a structured item record without extra notes.".to_string());
        }

        let mut tags = Vec::new();
        if let Some(quality) = &item.qual {
            tags.push(quality.clone());
        }
        if let Some(cost) = item.cost {
            if cost >= 4000 {
                tags.push("late-game".to_string());
            } else if cost <= 2000 {
                tags.push("early-game".to_string());
            }
        }

        let mut overlay_attributes = BTreeMap::new();
        if let Some(cost) = item.cost {
            overlay_attributes.insert("cost".to_string(), cost.to_string());
        }
        if let Some(quality) = &item.qual {
            overlay_attributes.insert("quality".to_string(), quality.clone());
        }
        if let Some(cd) = item.cooldown {
            overlay_attributes.insert("cooldown".to_string(), cd.to_string());
        }
        if let Some(mc) = item.mana_cost {
            overlay_attributes.insert("mana_cost".to_string(), mc.to_string());
        }

        entries.push(KnowledgeEntry {
            kind: EntryKind::Item,
            slug: slugify(&display_name),
            name: display_name.clone(),
            aliases: vec![key.replace('_', " ")],
            summary: format!(
                "{} item{}",
                item.qual.clone().unwrap_or_else(|| "utility".to_string()),
                item.cost
                    .map(|value| format!(" costing {value} gold"))
                    .unwrap_or_default()
            ),
            details,
            tags,
            related: vec![],
            provider: Some("stratz".to_string()),
            provider_id: item.id.map(|v| v.to_string()),
            popularity: None,
            win_rate: None,
            updated_at: items_fetched_at.clone(),
            overlay: Some(EntryOverlay {
                bullets: vec![
                    item.cost
                        .map(|value| format!("Cost: {value} gold"))
                        .unwrap_or_else(|| "Cost unavailable".to_string()),
                    item.cooldown
                        .map(|value| format!("Cooldown: {value}s"))
                        .unwrap_or_else(|| "Cooldown unavailable".to_string()),
                ],
                popularity: None,
                win_rate: None,
                sample_size: None,
                attributes: overlay_attributes,
            }),
        });
    }

    let cache_state = combine_cache_states(&[&heroes_cache_state, &items_cache_state]);
    let cache_age_sec = max_age(&[heroes_cache_age, items_cache_age]);
    let fetched_at = latest_timestamp(&[heroes_fetched_at.clone(), items_fetched_at.clone()]);

    Ok(ProviderDataset {
        entries,
        source: ResponseSourceMetadata {
            requested_source: "stratz".to_string(),
            resolved_sources: vec!["stratz".to_string()],
            freshness: freshness.as_str().to_string(),
            cache_state,
            cache_age_sec,
            live_data_used: heroes_cache_state == "live_fetch" || items_cache_state == "live_fetch",
            fetched_at,
            notes: vec!["Encyclopedia surfaces resolved via STRATZ GraphQL API.".to_string()],
        },
    })
}

fn stratz_load_or_cache<T>(
    cache_path: &Path,
    ttl_sec: u64,
    freshness: FreshnessMode,
    fetcher: impl FnOnce() -> std::result::Result<T, ErrorContext>,
) -> std::result::Result<(T, String, Option<u64>, Option<String>), ErrorContext>
where
    T: DeserializeOwned + Serialize,
{
    if let Some(cached) = read_cache::<T>(cache_path).ok().flatten() {
        let cache_age_sec = now().saturating_sub(cached.fetched_at);
        let fresh = cache_age_sec <= ttl_sec;

        match freshness {
            FreshnessMode::CachedOk => {
                return Ok((
                    cached.value,
                    if fresh {
                        "fresh_cache".to_string()
                    } else {
                        "stale_cache".to_string()
                    },
                    Some(cache_age_sec),
                    Some(cached.fetched_at.to_string()),
                ));
            }
            FreshnessMode::Recent if fresh => {
                return Ok((
                    cached.value,
                    "fresh_cache".to_string(),
                    Some(cache_age_sec),
                    Some(cached.fetched_at.to_string()),
                ));
            }
            FreshnessMode::Live | FreshnessMode::Recent => {}
        }
    } else if freshness == FreshnessMode::CachedOk {
        return Err(ErrorContext::new(
            "provider.cache_miss",
            "requested cached provider data is not available yet; run `dota-agent-cli source warm` first",
            "provider_cache",
        )
        .with_detail("cache_path", cache_path.display().to_string()));
    }

    let value = fetcher()?;
    let fetched_at = now();
    let envelope = CacheEnvelope {
        fetched_at,
        expires_at: fetched_at + ttl_sec,
        value: &value,
    };
    let serialized = serde_json::to_string_pretty(&envelope).map_err(|e| {
        ErrorContext::new(
            "provider.cache_write_failed",
            format!("failed to encode cache: {}", e),
            "stratz_cache",
        )
        .with_detail("cache_path", cache_path.display().to_string())
    })?;
    let _ = fs::write(cache_path, serialized); // Best effort

    Ok((
        value,
        "live_fetch".to_string(),
        Some(0),
        Some(fetched_at.to_string()),
    ))
}
