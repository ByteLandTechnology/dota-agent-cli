//! Match command module for dota-agent-cli.
//! Provides live match tracking, match detail, and recent match queries
//! backed by live providers.

use crate::ErrorContext;
use crate::context::RuntimeLocations;
use crate::encyclopedia::find_hero_by_name;
use crate::providers::{
    FreshnessMode, ProviderSourceSelector, ResponseSourceMetadata, build_client, fetch_json, now,
    now_string, opendota_url,
};
use anyhow::Context;
use clap::ValueEnum;
use reqwest::blocking::Client;
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

// ---------------------------------------------------------------------------
// STRATZ GraphQL API response types (private)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
struct StratzLiveMatchResponse {
    #[serde(rename = "liveMatches")]
    live_matches: Option<StratzLiveMatchList>,
}

#[derive(Debug, Clone, Deserialize)]
struct StratzLiveMatchList {
    #[serde(rename = "top")]
    top: Option<Vec<StratzLiveMatch>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct StratzLiveMatch {
    #[serde(rename = "matchId")]
    match_id: Option<i64>,
    #[serde(rename = "gameTime")]
    game_time: Option<i64>,
    #[serde(rename = "leagueId")]
    league_id: Option<i32>,
    #[serde(rename = "radiantLead")]
    radiant_lead: Option<i32>,
    #[serde(rename = "averageRank")]
    average_rank: Option<i32>,
    #[serde(rename = "players")]
    players: Option<Vec<StratzLivePlayer>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct StratzLivePlayer {
    #[serde(rename = "steamAccountId")]
    steam_account_id: Option<i64>,
    #[serde(rename = "heroId")]
    hero_id: Option<i32>,
    #[serde(rename = "name")]
    name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct StratzMatchResponse {
    #[serde(rename = "match")]
    match_data: Option<StratzMatchDetail>,
}

#[derive(Debug, Clone, Deserialize)]
struct StratzMatchDetail {
    #[serde(rename = "id")]
    id: Option<i64>,
    #[serde(rename = "didRadiantWin")]
    did_radiant_win: Option<bool>,
    #[serde(rename = "durationSeconds")]
    duration_seconds: Option<i64>,
    #[serde(rename = "gameMode")]
    game_mode: Option<i32>,
    #[serde(rename = "leagueId")]
    league_id: Option<i32>,
    #[serde(rename = "picksBans")]
    picks_bans: Option<Vec<StratzPickBan>>,
    #[serde(rename = "players")]
    players: Option<Vec<StratzPlayerDetail>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct StratzPickBan {
    #[serde(rename = "isPick")]
    is_pick: Option<bool>,
    #[serde(rename = "heroId")]
    hero_id: Option<i32>,
    #[serde(rename = "side")]
    side: Option<u8>,
    #[serde(rename = "order")]
    order: Option<i32>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct StratzPlayerDetail {
    #[serde(rename = "steamAccountId")]
    steam_account_id: Option<i64>,
    #[serde(rename = "playerSlot")]
    player_slot: Option<i32>,
    #[serde(rename = "heroId")]
    hero_id: Option<i32>,
    #[serde(rename = "kills")]
    kills: Option<i32>,
    #[serde(rename = "deaths")]
    deaths: Option<i32>,
    #[serde(rename = "assists")]
    assists: Option<i32>,
    #[serde(rename = "goldPerMinute")]
    gold_per_minute: Option<i32>,
    #[serde(rename = "experiencePerMinute")]
    experience_per_minute: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
struct StratzPlayerMatchesResponse {
    #[serde(rename = "player")]
    player: Option<StratzPlayerProfile>,
}

#[derive(Debug, Clone, Deserialize)]
struct StratzPlayerProfile {
    #[serde(rename = "recentMatches")]
    recent_matches: Option<StratzRecentMatchList>,
}

#[derive(Debug, Clone, Deserialize)]
struct StratzRecentMatchList {
    #[serde(rename = "matches")]
    matches: Option<Vec<StratzRecentMatch>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct StratzRecentMatch {
    #[serde(rename = "matchId")]
    match_id: Option<i64>,
    #[serde(rename = "heroId")]
    hero_id: Option<i32>,
    #[serde(rename = "didRadiantWin")]
    did_radiant_win: Option<bool>,
    #[serde(rename = "durationSeconds")]
    duration_seconds: Option<i64>,
    #[serde(rename = "kills")]
    kills: Option<i32>,
    #[serde(rename = "deaths")]
    deaths: Option<i32>,
    #[serde(rename = "assists")]
    assists: Option<i32>,
    #[serde(rename = "startDateTime")]
    start_date_time: Option<i64>,
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
// STRATZ GraphQL API helpers
// ---------------------------------------------------------------------------

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
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| {
            ErrorContext::new(
                "provider.unreachable",
                format!("failed to construct STRATZ client: {}", e),
                "stratz_client",
            )
        })
}

fn stratz_fetch_json<T: serde::de::DeserializeOwned>(
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
            .filter_map(|e: &serde_json::Value| e.get("message").and_then(|m| m.as_str()))
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

fn http_status_error(provider: &str, url: &str, status: reqwest::StatusCode) -> ErrorContext {
    let code = match status.as_u16() {
        401 | 403 => "provider.auth_required",
        404 => "provider.not_found",
        429 => "provider.rate_limited",
        _ => "provider.http_error",
    };
    ErrorContext::new(
        code,
        format!("{} returned HTTP {} ({})", provider, status, url),
        "match_provider",
    )
}

fn stratz_live_matches_query() -> &'static str {
    r#"
        query LiveMatches {
            liveMatches {
                top {
                    matchId
                    gameTime
                    leagueId
                    radiantLead
                    averageRank
                    players {
                        steamAccountId
                        heroId
                        name
                    }
                }
            }
        }
    "#
}

fn stratz_match_detail_query() -> &'static str {
    r#"
        query MatchDetails($matchId: Long!) {
            match(id: $matchId) {
                id
                didRadiantWin
                durationSeconds
                gameMode
                leagueId
                picksBans {
                    isPick
                    heroId
                    side
                    order
                }
                players {
                    steamAccountId
                    playerSlot
                    heroId
                    kills
                    deaths
                    assists
                    goldPerMinute
                    experiencePerMinute
                }
            }
        }
    "#
}

fn stratz_player_recent_query() -> &'static str {
    r#"
        query PlayerRecent($playerId: Long!) {
            player(steamAccountId: $playerId) {
                recentMatches {
                    matches {
                        matchId
                        heroId
                        didRadiantWin
                        durationSeconds
                        kills
                        deaths
                        assists
                        startDateTime
                    }
                }
            }
        }
    "#
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
            let _cache_root = ensure_cache_dir(runtime)?;
            let client = stratz_client()?;
            let mut variables = BTreeMap::new();
            variables.insert("limit".to_string(), serde_json::json!(limit));

            let response: StratzLiveMatchResponse =
                stratz_fetch_json(&client, stratz_live_matches_query(), Some(variables))?;

            let games = response
                .live_matches
                .and_then(|l| l.top)
                .unwrap_or_default();

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
                        game.average_rank.unwrap_or(0) >= min
                    } else {
                        true
                    }
                })
                .map(|game| LiveMatchEntry {
                    match_id: game.match_id,
                    game_time_sec: game.game_time,
                    league_id: game.league_id,
                    radiant_lead: game.radiant_lead,
                    average_mmr: game.average_rank,
                    player_count: game.players.as_ref().map(|p| p.len()).unwrap_or(0),
                })
                .take(limit)
                .collect();

            let match_count = matches.len();
            return Ok(MatchLiveOutput {
                match_count,
                source: ResponseSourceMetadata {
                    requested_source: "stratz".to_string(),
                    resolved_sources: vec!["stratz".to_string()],
                    freshness: freshness.as_str().to_string(),
                    cache_state: "live_fetch".to_string(),
                    cache_age_sec: Some(0),
                    live_data_used: true,
                    fetched_at: Some(now_string()),
                    notes: vec!["Live matches resolved via STRATZ GraphQL API.".to_string()],
                },
                matches,
            });
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
            let client = stratz_client()?;
            let mut variables = BTreeMap::new();
            variables.insert("matchId".to_string(), serde_json::json!(match_id));

            let response: StratzMatchResponse =
                stratz_fetch_json(&client, stratz_match_detail_query(), Some(variables))?;

            let match_data = response.match_data;
            let actual_match_id = match_data.as_ref().and_then(|m| m.id).unwrap_or(match_id);
            let duration_seconds = match_data.as_ref().and_then(|m| m.duration_seconds);
            let radiant_win = match_data.as_ref().and_then(|m| m.did_radiant_win);
            let game_mode = match_data.as_ref().and_then(|m| m.game_mode);
            let league_id = match_data.as_ref().and_then(|m| m.league_id);

            let mut players = Vec::new();
            let mut picks_bans = Vec::new();

            if expand {
                if let Some(players_data) = match_data.as_ref().and_then(|m| m.players.clone()) {
                    players = players_data
                        .iter()
                        .map(|p| MatchPlayerSummary {
                            hero_id: p.hero_id,
                            kills: p.kills,
                            deaths: p.deaths,
                            assists: p.assists,
                            gpm: p.gold_per_minute,
                            xpm: p.experience_per_minute,
                        })
                        .collect();
                }

                if let Some(pb) = match_data.as_ref().and_then(|m| m.picks_bans.clone()) {
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

            let player_count = match_data
                .as_ref()
                .and_then(|m| m.players.as_ref())
                .map(|p| p.len())
                .unwrap_or(0);
            let picks_bans_count = match_data
                .as_ref()
                .and_then(|m| m.picks_bans.as_ref())
                .map(|pb| pb.len())
                .unwrap_or(0);

            return Ok(MatchShowOutput {
                match_id: actual_match_id,
                radiant_win,
                duration_sec: duration_seconds,
                game_mode,
                league_id,
                picks_bans_count,
                player_count,
                players,
                picks_bans,
                source: ResponseSourceMetadata {
                    requested_source: "stratz".to_string(),
                    resolved_sources: vec!["stratz".to_string()],
                    freshness: freshness.as_str().to_string(),
                    cache_state: "live_fetch".to_string(),
                    cache_age_sec: Some(0),
                    live_data_used: true,
                    fetched_at: Some(now_string()),
                    notes: vec!["Match detail resolved via STRATZ GraphQL API.".to_string()],
                },
            });
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
    hero_entries: &[crate::encyclopedia::KnowledgeEntry],
) -> std::result::Result<MatchRecentOutput, ErrorContext> {
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

    match source {
        ProviderSourceSelector::Stratz => {
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

            let client = stratz_client()?;
            let mut variables = BTreeMap::new();
            variables.insert("playerId".to_string(), serde_json::json!(pid));

            let response: StratzPlayerMatchesResponse =
                stratz_fetch_json(&client, stratz_player_recent_query(), Some(variables))?;

            let matches_data = response
                .player
                .and_then(|p| p.recent_matches)
                .and_then(|r| r.matches)
                .unwrap_or_default();

            let mut matches: Vec<RecentMatchEntry> = matches_data
                .iter()
                .filter(|m| {
                    if let Some(hero_filter) = hero {
                        if let Ok(filter_id) = hero_filter.parse::<i32>() {
                            m.hero_id == Some(filter_id)
                        } else {
                            find_hero_by_name(hero_entries, hero_filter)
                                .map(|resolved_id| m.hero_id == Some(resolved_id as i32))
                                .unwrap_or(false)
                        }
                    } else {
                        true
                    }
                })
                .filter(|m| {
                    if won_only {
                        m.did_radiant_win.unwrap_or(false)
                    } else {
                        true
                    }
                })
                .map(|m| RecentMatchEntry {
                    match_id: m.match_id,
                    hero_id: m.hero_id,
                    won: m.did_radiant_win,
                    duration_sec: m.duration_seconds,
                    kills: m.kills,
                    deaths: m.deaths,
                    assists: m.assists,
                })
                .collect();

            // Sort
            match sort {
                MatchSort::Recent => {} // API may already return recency order
                MatchSort::Duration => matches.sort_by(|a, b| b.duration_sec.cmp(&a.duration_sec)),
                MatchSort::Kills => matches.sort_by(|a, b| b.kills.cmp(&a.kills)),
                MatchSort::Winrate => {
                    matches.sort_by(|a, b| b.won.cmp(&a.won));
                }
            }

            matches.truncate(limit);
            let match_count = matches.len();

            return Ok(MatchRecentOutput {
                player_id: Some(pid),
                match_count,
                source: ResponseSourceMetadata {
                    requested_source: "stratz".to_string(),
                    resolved_sources: vec!["stratz".to_string()],
                    freshness: freshness.as_str().to_string(),
                    cache_state: "live_fetch".to_string(),
                    cache_age_sec: Some(0),
                    live_data_used: true,
                    fetched_at: Some(now_string()),
                    notes: vec!["Recent matches resolved via STRATZ GraphQL API.".to_string()],
                },
                matches,
            });
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
                // Try to parse as integer ID first, then resolve name to ID
                if let Ok(filter_id) = hero_filter.parse::<i32>() {
                    m.hero_id == Some(filter_id)
                } else {
                    // Resolve hero name to ID using encyclopedia entries
                    find_hero_by_name(hero_entries, hero_filter)
                        .map(|resolved_id| m.hero_id == Some(resolved_id as i32))
                        .unwrap_or(false)
                }
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
