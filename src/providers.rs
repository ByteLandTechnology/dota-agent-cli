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
                Err(ErrorContext::new(
                    "provider.unsupported_surface",
                    "STRATZ is configured, but this revision still resolves hero/item encyclopedia surfaces through OpenDota",
                    "provider_routing",
                )
                .with_detail("provider", "stratz"))
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
        ProviderSourceSelector::Stratz => {}
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
