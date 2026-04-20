//! Match command module for dota-agent-cli.
//! Provides live match tracking, match detail, and recent match queries
//! backed by live providers.

use crate::ErrorContext;
use crate::context::RuntimeLocations;
use crate::providers::{
    FreshnessMode, ProviderSourceSelector, ResponseSourceMetadata, build_client, fetch_json, now,
    now_string, opendota_url,
};
use anyhow::Context;
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

const LIVE_MATCH_TTL_SEC: u64 = 30;
const MATCH_DETAIL_TTL_SEC: u64 = 300;
const RECENT_MATCHES_TTL_SEC: u64 = 120;

// ---------------------------------------------------------------------------
// Sort enum
// ---------------------------------------------------------------------------

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum MatchSort {
    Recent,
    Winrate,
    Duration,
    Kills,
}

impl MatchSort {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Recent => "recent",
            Self::Winrate => "winrate",
            Self::Duration => "duration",
            Self::Kills => "kills",
        }
    }
}

// ---------------------------------------------------------------------------
// Flexible deserializers for fields that may arrive as string or number
// ---------------------------------------------------------------------------

fn deserialize_flexible_i64<'de, D>(deserializer: D) -> std::result::Result<Option<i64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Unexpected, Visitor};
    struct I64Visitor;
    impl<'de> Visitor<'de> for I64Visitor {
        type Value = Option<i64>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("an integer, a string containing an integer, or null")
        }
        fn visit_i64<E: de::Error>(self, v: i64) -> std::result::Result<Self::Value, E> {
            Ok(Some(v))
        }
        fn visit_u64<E: de::Error>(self, v: u64) -> std::result::Result<Self::Value, E> {
            i64::try_from(v)
                .map(Some)
                .map_err(|_| E::invalid_value(Unexpected::Unsigned(v), &self))
        }
        fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<Self::Value, E> {
            v.parse::<i64>()
                .map(Some)
                .map_err(|_| E::invalid_value(Unexpected::Str(v), &self))
        }
        fn visit_none<E: de::Error>(self) -> std::result::Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_unit<E: de::Error>(self) -> std::result::Result<Self::Value, E> {
            Ok(None)
        }
    }
    deserializer.deserialize_any(I64Visitor)
}

fn deserialize_flexible_u64<'de, D>(deserializer: D) -> std::result::Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Unexpected, Visitor};
    struct U64Visitor;
    impl<'de> Visitor<'de> for U64Visitor {
        type Value = Option<u64>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("an integer, a string containing an integer, or null")
        }
        fn visit_i64<E: de::Error>(self, v: i64) -> std::result::Result<Self::Value, E> {
            u64::try_from(v)
                .map(Some)
                .map_err(|_| E::invalid_value(Unexpected::Signed(v), &self))
        }
        fn visit_u64<E: de::Error>(self, v: u64) -> std::result::Result<Self::Value, E> {
            Ok(Some(v))
        }
        fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<Self::Value, E> {
            v.parse::<u64>()
                .map(Some)
                .map_err(|_| E::invalid_value(Unexpected::Str(v), &self))
        }
        fn visit_none<E: de::Error>(self) -> std::result::Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_unit<E: de::Error>(self) -> std::result::Result<Self::Value, E> {
            Ok(None)
        }
    }
    deserializer.deserialize_any(U64Visitor)
}

// ---------------------------------------------------------------------------
// OpenDota API response types (private)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenDotaLiveGame {
    #[serde(default, deserialize_with = "deserialize_flexible_i64")]
    match_id: Option<i64>,
    #[serde(default, deserialize_with = "deserialize_flexible_u64")]
    server_steam_id: Option<u64>,
    #[serde(default)]
    game_time: Option<i64>,
    #[serde(default)]
    league_id: Option<i32>,
    #[serde(default)]
    radiant_lead: Option<i32>,
    #[serde(default)]
    average_mmr: Option<i32>,
    #[serde(default)]
    players: Vec<OpenDotaLivePlayer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenDotaLivePlayer {
    #[serde(default)]
    account_id: Option<i64>,
    #[serde(default)]
    hero_id: Option<i32>,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenDotaMatch {
    #[serde(default)]
    match_id: Option<i64>,
    #[serde(default)]
    radiant_win: Option<bool>,
    #[serde(default)]
    duration: Option<i64>,
    #[serde(default)]
    game_mode: Option<i32>,
    #[serde(default)]
    leagueid: Option<i32>,
    #[serde(default)]
    picks_bans: Option<Vec<OpenDotaPickBan>>,
    #[serde(default)]
    players: Vec<OpenDotaMatchPlayer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenDotaPickBan {
    #[serde(default)]
    is_pick: Option<bool>,
    #[serde(default)]
    hero_id: Option<i32>,
    #[serde(default)]
    side: Option<u8>,
    #[serde(default)]
    order: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenDotaMatchPlayer {
    #[serde(default)]
    account_id: Option<i64>,
    #[serde(default)]
    player_slot: Option<i32>,
    #[serde(default)]
    hero_id: Option<i32>,
    #[serde(default)]
    kills: Option<i32>,
    #[serde(default)]
    deaths: Option<i32>,
    #[serde(default)]
    assists: Option<i32>,
    #[serde(default)]
    gold_per_min: Option<i32>,
    #[serde(default)]
    xp_per_min: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenDotaPlayerMatch {
    #[serde(default)]
    match_id: Option<i64>,
    #[serde(default)]
    player_slot: Option<i32>,
    #[serde(default)]
    radiant_win: Option<bool>,
    #[serde(default)]
    hero_id: Option<i32>,
    #[serde(default)]
    duration: Option<i64>,
    #[serde(default)]
    game_mode: Option<i32>,
    #[serde(default)]
    kills: Option<i32>,
    #[serde(default)]
    deaths: Option<i32>,
    #[serde(default)]
    assists: Option<i32>,
    #[serde(default)]
    start_time: Option<i64>,
}

// ---------------------------------------------------------------------------
// Cache helpers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEnvelope<T> {
    fetched_at: u64,
    expires_at: u64,
    value: T,
}

fn ensure_cache_dir(
    runtime: &RuntimeLocations,
) -> std::result::Result<std::path::PathBuf, ErrorContext> {
    let cache_root = runtime.cache_dir.join("live-providers");
    fs::create_dir_all(&cache_root).map_err(|error| {
        ErrorContext::new(
            "provider.cache_write_failed",
            format!("failed to create cache directory: {error}"),
            "match_cache",
        )
        .with_detail("cache_root", cache_root.display().to_string())
    })?;
    Ok(cache_root)
}

fn read_cache<T>(path: &Path) -> anyhow::Result<Option<CacheEnvelope<T>>>
where
    T: serde::de::DeserializeOwned,
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

fn write_cache<T>(path: &Path, ttl_sec: u64, fetched_at: u64, value: &T) -> anyhow::Result<()>
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

fn load_or_cache<T>(
    cache_path: &Path,
    ttl_sec: u64,
    freshness: FreshnessMode,
    provider: &str,
    url: &str,
    validate: impl Fn(&T) -> bool,
) -> std::result::Result<(T, String, Option<u64>), ErrorContext>
where
    T: serde::de::DeserializeOwned + Serialize,
{
    let cached = read_cache::<T>(cache_path).map_err(|error| {
        ErrorContext::new(
            "provider.cache_read_failed",
            format!("failed to read match cache: {error:#}"),
            "match_cache",
        )
        .with_detail("cache_path", cache_path.display().to_string())
    })?;

    if let Some(cached) = cached {
        let cache_age_sec = now().saturating_sub(cached.fetched_at);
        let fresh = cache_age_sec <= ttl_sec;

        match freshness {
            FreshnessMode::CachedOk => {
                let state = if fresh { "fresh_cache" } else { "stale_cache" };
                return Ok((cached.value, state.to_string(), Some(cache_age_sec)));
            }
            FreshnessMode::Recent if fresh => {
                return Ok((cached.value, "fresh_cache".to_string(), Some(cache_age_sec)));
            }
            FreshnessMode::Live | FreshnessMode::Recent => {}
        }
    }

    let client = build_client().map_err(|error| {
        ErrorContext::new(
            "provider.unreachable",
            format!("failed to initialize HTTP client: {error:#}"),
            "match_provider",
        )
    })?;

    let value = fetch_json::<T>(&client, provider, url)?;
    let fetched_at = now();

    if validate(&value) {
        let _ = write_cache(cache_path, ttl_sec, fetched_at, &value);
    }

    Ok((value, "live_fetch".to_string(), Some(0)))
}

// ---------------------------------------------------------------------------
// Public output types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct MatchLiveOutput {
    pub match_count: usize,
    pub source: ResponseSourceMetadata,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub matches: Vec<LiveMatchEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LiveMatchEntry {
    pub match_id: Option<i64>,
    pub game_time_sec: Option<i64>,
    pub league_id: Option<i32>,
    pub radiant_lead: Option<i32>,
    pub average_mmr: Option<i32>,
    pub player_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct MatchShowOutput {
    pub match_id: i64,
    pub source: ResponseSourceMetadata,
    pub radiant_win: Option<bool>,
    pub duration_sec: Option<i64>,
    pub game_mode: Option<i32>,
    pub league_id: Option<i32>,
    pub picks_bans_count: usize,
    pub player_count: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub players: Vec<MatchPlayerSummary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub picks_bans: Vec<PickBanEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MatchPlayerSummary {
    pub hero_id: Option<i32>,
    pub kills: Option<i32>,
    pub deaths: Option<i32>,
    pub assists: Option<i32>,
    pub gpm: Option<i32>,
    pub xpm: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PickBanEntry {
    pub is_pick: bool,
    pub hero_id: Option<i32>,
    pub side: Option<u8>,
    pub order: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MatchRecentOutput {
    pub player_id: Option<i64>,
    pub match_count: usize,
    pub source: ResponseSourceMetadata,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub matches: Vec<RecentMatchEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecentMatchEntry {
    pub match_id: Option<i64>,
    pub hero_id: Option<i32>,
    pub won: Option<bool>,
    pub duration_sec: Option<i64>,
    pub kills: Option<i32>,
    pub deaths: Option<i32>,
    pub assists: Option<i32>,
}

// ---------------------------------------------------------------------------
// Provider-facing error helpers
// ---------------------------------------------------------------------------

fn match_source_metadata(
    source: ProviderSourceSelector,
    freshness: FreshnessMode,
    cache_state: &str,
    cache_age_sec: Option<u64>,
) -> ResponseSourceMetadata {
    ResponseSourceMetadata {
        requested_source: source.as_str().to_string(),
        resolved_sources: vec!["opendota".to_string()],
        freshness: freshness.as_str().to_string(),
        cache_state: cache_state.to_string(),
        cache_age_sec,
        live_data_used: cache_state == "live_fetch",
        fetched_at: Some(now_string()),
        notes: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Public match provider functions
// ---------------------------------------------------------------------------

pub fn fetch_live_matches(
    runtime: &RuntimeLocations,
    source: ProviderSourceSelector,
    freshness: FreshnessMode,
    limit: usize,
    league_id: Option<i32>,
    min_mmr: Option<i32>,
) -> std::result::Result<MatchLiveOutput, ErrorContext> {
    match source {
        ProviderSourceSelector::Stratz => {
            return Err(ErrorContext::new(
                "provider.unsupported_surface",
                "STRATZ live match feeds are not implemented yet; use OpenDota or auto routing",
                "match_provider",
            )
            .with_detail("provider", "stratz"));
        }
        ProviderSourceSelector::Auto | ProviderSourceSelector::Opendota => {}
    }

    let cache_root = ensure_cache_dir(runtime)?;
    let url = opendota_url("live");
    let (games, cache_state, cache_age_sec) = load_or_cache::<Vec<OpenDotaLiveGame>>(
        &cache_root.join("opendota-live-matches.json"),
        LIVE_MATCH_TTL_SEC,
        freshness,
        "opendota",
        &url,
        |_| true,
    )?;

    let matches: Vec<LiveMatchEntry> = games
        .into_iter()
        .filter(|game| {
            if let Some(lid) = league_id {
                game.league_id == Some(lid)
            } else {
                true
            }
        })
        .filter(|game| {
            if let Some(min) = min_mmr {
                game.average_mmr.unwrap_or(0) >= min
            } else {
                true
            }
        })
        .map(|game| LiveMatchEntry {
            match_id: game.match_id,
            game_time_sec: game.game_time,
            league_id: game.league_id,
            radiant_lead: game.radiant_lead,
            average_mmr: game.average_mmr,
            player_count: game.players.len(),
        })
        .take(limit)
        .collect();

    let match_count = matches.len();

    Ok(MatchLiveOutput {
        match_count,
        source: match_source_metadata(source, freshness, &cache_state, cache_age_sec),
        matches,
    })
}

pub fn fetch_match_detail(
    runtime: &RuntimeLocations,
    source: ProviderSourceSelector,
    freshness: FreshnessMode,
    match_id: i64,
    expand: bool,
) -> std::result::Result<MatchShowOutput, ErrorContext> {
    match source {
        ProviderSourceSelector::Stratz => {
            return Err(ErrorContext::new(
                "provider.unsupported_surface",
                "STRATZ match detail is not implemented yet; use OpenDota or auto routing",
                "match_provider",
            )
            .with_detail("provider", "stratz"));
        }
        ProviderSourceSelector::Auto | ProviderSourceSelector::Opendota => {}
    }

    let cache_root = ensure_cache_dir(runtime)?;
    let url = opendota_url(&format!("matches/{match_id}"));
    let (match_data, cache_state, cache_age_sec) = load_or_cache::<OpenDotaMatch>(
        &cache_root.join(format!("opendota-match-{match_id}.json")),
        MATCH_DETAIL_TTL_SEC,
        freshness,
        "opendota",
        &url,
        |m| m.match_id.is_some(),
    )?;

    let actual_match_id = match_data.match_id.unwrap_or(match_id);

    let mut players = Vec::new();
    let mut picks_bans = Vec::new();

    if expand {
        players = match_data
            .players
            .iter()
            .map(|p| MatchPlayerSummary {
                hero_id: p.hero_id,
                kills: p.kills,
                deaths: p.deaths,
                assists: p.assists,
                gpm: p.gold_per_min,
                xpm: p.xp_per_min,
            })
            .collect();

        if let Some(pb) = &match_data.picks_bans {
            picks_bans = pb
                .iter()
                .map(|entry| PickBanEntry {
                    is_pick: entry.is_pick.unwrap_or(false),
                    hero_id: entry.hero_id,
                    side: entry.side,
                    order: entry.order,
                })
                .collect();
        }
    }

    Ok(MatchShowOutput {
        match_id: actual_match_id,
        radiant_win: match_data.radiant_win,
        duration_sec: match_data.duration,
        game_mode: match_data.game_mode,
        league_id: match_data.leagueid,
        picks_bans_count: match_data
            .picks_bans
            .as_ref()
            .map(|pb| pb.len())
            .unwrap_or(0),
        player_count: match_data.players.len(),
        players,
        picks_bans,
        source: match_source_metadata(source, freshness, &cache_state, cache_age_sec),
    })
}

#[allow(clippy::too_many_arguments)]
pub fn fetch_recent_matches(
    runtime: &RuntimeLocations,
    source: ProviderSourceSelector,
    freshness: FreshnessMode,
    player_id: Option<i64>,
    hero: Option<&str>,
    limit: usize,
    sort: MatchSort,
    won_only: bool,
    effective_context: &BTreeMap<String, String>,
) -> std::result::Result<MatchRecentOutput, ErrorContext> {
    match source {
        ProviderSourceSelector::Stratz => {
            return Err(ErrorContext::new(
                "provider.unsupported_surface",
                "STRATZ recent match queries are not implemented yet; use OpenDota or auto routing",
                "match_provider",
            )
            .with_detail("provider", "stratz"));
        }
        ProviderSourceSelector::Auto | ProviderSourceSelector::Opendota => {}
    }

    // Resolve player_id from context if not provided explicitly
    let resolved_player_id = player_id.or_else(|| {
        effective_context
            .get("player_id")
            .and_then(|v| v.parse::<i64>().ok())
    });

    let Some(pid) = resolved_player_id else {
        return Err(ErrorContext::new(
            "match.player_not_found",
            "recent matches require --player-id or a player_id Active Context selector",
            "match_validation",
        ));
    };

    if matches!(sort, MatchSort::Winrate)
        && player_id.is_none()
        && !effective_context.contains_key("player_id")
    {
        return Err(ErrorContext::new(
            "match.unsupported_filter",
            "winrate sort requires --player-id or player_id context",
            "match_validation",
        )
        .with_detail("sort", "winrate"));
    }

    let cache_root = ensure_cache_dir(runtime)?;
    let url = opendota_url(&format!("players/{pid}/recentMatches"));
    let (matches_data, cache_state, cache_age_sec) = load_or_cache::<Vec<OpenDotaPlayerMatch>>(
        &cache_root.join(format!("opendota-player-{pid}-recent.json")),
        RECENT_MATCHES_TTL_SEC,
        freshness,
        "opendota",
        &url,
        |_| true,
    )?;

    let mut matches: Vec<RecentMatchEntry> = matches_data
        .into_iter()
        .filter(|m| {
            if let Some(hero_filter) = hero {
                // Exact integer match against hero_id, or string equality
                // against the stringified ID. No substring matching.
                hero_filter
                    .parse::<i32>()
                    .ok()
                    .map(|filter_id| m.hero_id == Some(filter_id))
                    .unwrap_or(false)
            } else {
                true
            }
        })
        .filter(|m| {
            if won_only {
                let is_radiant = m.player_slot.map(|s| s < 128).unwrap_or(true);
                m.radiant_win
                    .map(|rw| if is_radiant { rw } else { !rw })
                    .unwrap_or(false)
            } else {
                true
            }
        })
        .map(|m| {
            let is_radiant = m.player_slot.map(|s| s < 128).unwrap_or(true);
            let won = m.radiant_win.map(|rw| if is_radiant { rw } else { !rw });
            RecentMatchEntry {
                match_id: m.match_id,
                hero_id: m.hero_id,
                won,
                duration_sec: m.duration,
                kills: m.kills,
                deaths: m.deaths,
                assists: m.assists,
            }
        })
        .collect();

    // Sort
    match sort {
        MatchSort::Recent => {} // API already returns in recency order
        MatchSort::Duration => matches.sort_by(|a, b| b.duration_sec.cmp(&a.duration_sec)),
        MatchSort::Kills => matches.sort_by(|a, b| b.kills.cmp(&a.kills)),
        MatchSort::Winrate => {
            // Sort won matches first
            matches.sort_by(|a, b| b.won.cmp(&a.won));
        }
    }

    matches.truncate(limit);
    let match_count = matches.len();

    Ok(MatchRecentOutput {
        player_id: Some(pid),
        match_count,
        source: match_source_metadata(source, freshness, &cache_state, cache_age_sec),
        matches,
    })
}
