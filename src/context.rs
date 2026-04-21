//! Runtime-directory and Active Context support for the generated skill.

use crate::DaemonLifecycleState;
use anyhow::{Context, Result, anyhow};
use directories::ProjectDirs;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Default)]
pub struct RuntimeOverrides {
    pub config_dir: Option<PathBuf>,
    pub data_dir: Option<PathBuf>,
    pub state_dir: Option<PathBuf>,
    pub cache_dir: Option<PathBuf>,
    pub log_dir: Option<PathBuf>,
}

impl RuntimeOverrides {
    pub fn has_overrides(&self) -> bool {
        self.config_dir.is_some()
            || self.data_dir.is_some()
            || self.state_dir.is_some()
            || self.cache_dir.is_some()
            || self.log_dir.is_some()
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeLocations {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub state_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub log_dir: Option<PathBuf>,
    pub scope: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeDirectorySummary {
    pub config_dir: String,
    pub data_dir: String,
    pub state_dir: String,
    pub cache_dir: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_dir: Option<String>,
    pub scope: String,
    pub override_mechanisms: Vec<String>,
}

impl RuntimeLocations {
    pub fn summary(&self) -> RuntimeDirectorySummary {
        RuntimeDirectorySummary {
            config_dir: self.config_dir.display().to_string(),
            data_dir: self.data_dir.display().to_string(),
            state_dir: self.state_dir.display().to_string(),
            cache_dir: self.cache_dir.display().to_string(),
            log_dir: self.log_dir.as_ref().map(|path| path.display().to_string()),
            scope: self.scope.clone(),
            override_mechanisms: vec![
                "--config-dir".to_string(),
                "--data-dir".to_string(),
                "--state-dir".to_string(),
                "--cache-dir".to_string(),
                "--log-dir".to_string(),
            ],
        }
    }

    pub fn context_file(&self) -> PathBuf {
        self.state_dir.join("active-context.toml")
    }

    pub fn daemon_state_file(&self) -> PathBuf {
        self.daemon_dir().join("state.toml")
    }

    pub fn legacy_daemon_state_file(&self) -> PathBuf {
        self.state_dir.join("daemon-state.toml")
    }

    pub fn daemon_dir(&self) -> PathBuf {
        self.state_dir.join("daemon")
    }

    pub fn daemon_pid_file(&self) -> PathBuf {
        self.daemon_dir().join("daemon.pid")
    }

    pub fn daemon_lock_file(&self) -> PathBuf {
        self.daemon_dir().join("daemon.lock")
    }

    pub fn daemon_endpoint_file(&self) -> PathBuf {
        self.daemon_dir().join("endpoint")
    }

    pub fn daemon_metadata_file(&self) -> PathBuf {
        self.daemon_dir().join("metadata.toml")
    }

    pub fn daemon_auth_token_file(&self) -> PathBuf {
        self.daemon_dir().join("auth.token")
    }

    pub fn daemon_log_file(&self) -> PathBuf {
        self.log_dir
            .clone()
            .unwrap_or_else(|| self.daemon_dir())
            .join("daemon.log")
    }

    pub fn history_file(&self) -> PathBuf {
        self.state_dir.join("repl_history.txt")
    }

    pub fn ensure_exists(&self) -> Result<()> {
        fs::create_dir_all(&self.config_dir)
            .with_context(|| format!("failed to create {}", self.config_dir.display()))?;
        fs::create_dir_all(&self.data_dir)
            .with_context(|| format!("failed to create {}", self.data_dir.display()))?;
        fs::create_dir_all(&self.state_dir)
            .with_context(|| format!("failed to create {}", self.state_dir.display()))?;
        fs::create_dir_all(self.daemon_dir())
            .with_context(|| format!("failed to create {}", self.daemon_dir().display()))?;
        fs::create_dir_all(&self.cache_dir)
            .with_context(|| format!("failed to create {}", self.cache_dir.display()))?;
        if let Some(log_dir) = &self.log_dir {
            fs::create_dir_all(log_dir)
                .with_context(|| format!("failed to create {}", log_dir.display()))?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActiveContextState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub selectors: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub ambient_cues: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default)]
pub struct InvocationContextOverrides {
    pub selectors: BTreeMap<String, String>,
    pub current_directory: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EffectiveContextView {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub effective_values: BTreeMap<String, String>,
    pub precedence_rule: String,
    pub persisted_context_present: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextInspection {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub persisted_context: Option<ActiveContextState>,
    pub effective_context: EffectiveContextView,
    pub context_file: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextPersistenceResult {
    pub status: String,
    pub message: String,
    pub active_context: ActiveContextState,
    pub context_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedDaemonState {
    pub state: DaemonLifecycleState,
    pub readiness: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub recommended_next_action: String,
    pub instance_model: String,
    pub instance_id: String,
    pub last_action: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_token_path: Option<String>,
}

impl Default for PersistedDaemonState {
    fn default() -> Self {
        let state = DaemonLifecycleState::Stopped;
        Self {
            readiness: "inactive".to_string(),
            reason: None,
            recommended_next_action: state.as_recommended_action().to_string(),
            instance_model: "single_instance".to_string(),
            instance_id: "default".to_string(),
            last_action: "status".to_string(),
            updated_at: timestamp_string(),
            transport: None,
            endpoint: None,
            pid: None,
            log_path: None,
            auth_token_path: None,
            state,
        }
    }
}

#[derive(Debug, Serialize)]
struct DaemonMetadata {
    state: DaemonLifecycleState,
    readiness: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    recommended_next_action: String,
    instance_model: String,
    instance_id: String,
    last_action: String,
    updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    transport: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pid: Option<u32>,
    lock_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    log_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    auth_token_path: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct DaemonSimulationFlags {
    pub fail_start: bool,
    pub fail_stop: bool,
    pub fail_restart: bool,
    pub timeout_start: bool,
    pub timeout_stop: bool,
    pub timeout_restart: bool,
    pub block_control: bool,
    pub unexpected_exit: bool,
}

pub fn resolve_runtime_locations(
    overrides: &RuntimeOverrides,
    log_enabled: bool,
) -> Result<RuntimeLocations> {
    let project_dirs = ProjectDirs::from("com", "cli-forge", "DotaAgentCli")
        .ok_or_else(|| anyhow!("failed to resolve platform project directories"))?;
    let legacy_project_dirs = ProjectDirs::from("com", "cli-forge", "DotaCli");
    let selected_project_dirs = if !overrides.has_overrides()
        && !project_dirs.config_dir().exists()
        && !project_dirs.data_dir().exists()
        && !project_dirs.cache_dir().exists()
    {
        legacy_project_dirs
            .as_ref()
            .filter(|legacy| {
                legacy.config_dir().exists()
                    || legacy.data_dir().exists()
                    || legacy.cache_dir().exists()
            })
            .unwrap_or(&project_dirs)
    } else {
        &project_dirs
    };

    let data_dir = overrides
        .data_dir
        .clone()
        .unwrap_or_else(|| selected_project_dirs.data_dir().to_path_buf());
    let state_dir = overrides
        .state_dir
        .clone()
        .unwrap_or_else(|| data_dir.join("state"));

    let log_dir = if overrides.log_dir.is_some() || log_enabled {
        Some(
            overrides
                .log_dir
                .clone()
                .unwrap_or_else(|| state_dir.join("logs")),
        )
    } else {
        None
    };

    Ok(RuntimeLocations {
        config_dir: overrides
            .config_dir
            .clone()
            .unwrap_or_else(|| selected_project_dirs.config_dir().to_path_buf()),
        data_dir,
        state_dir,
        cache_dir: overrides
            .cache_dir
            .clone()
            .unwrap_or_else(|| selected_project_dirs.cache_dir().to_path_buf()),
        log_dir,
        scope: if overrides.has_overrides() {
            "explicit_override".to_string()
        } else {
            "user_scoped_default".to_string()
        },
    })
}

pub fn daemon_simulation_flags() -> DaemonSimulationFlags {
    DaemonSimulationFlags {
        fail_start: env_flag(
            "DOTA_AGENT_CLI_DAEMON_FAIL_START",
            "DOTA_CLI_DAEMON_FAIL_START",
        ),
        fail_stop: env_flag(
            "DOTA_AGENT_CLI_DAEMON_FAIL_STOP",
            "DOTA_CLI_DAEMON_FAIL_STOP",
        ),
        fail_restart: env_flag(
            "DOTA_AGENT_CLI_DAEMON_FAIL_RESTART",
            "DOTA_CLI_DAEMON_FAIL_RESTART",
        ),
        timeout_start: env_flag(
            "DOTA_AGENT_CLI_DAEMON_TIMEOUT_START",
            "DOTA_CLI_DAEMON_TIMEOUT_START",
        ),
        timeout_stop: env_flag(
            "DOTA_AGENT_CLI_DAEMON_TIMEOUT_STOP",
            "DOTA_CLI_DAEMON_TIMEOUT_STOP",
        ),
        timeout_restart: env_flag(
            "DOTA_AGENT_CLI_DAEMON_TIMEOUT_RESTART",
            "DOTA_CLI_DAEMON_TIMEOUT_RESTART",
        ),
        block_control: env_flag(
            "DOTA_AGENT_CLI_DAEMON_BLOCK_CONTROL",
            "DOTA_CLI_DAEMON_BLOCK_CONTROL",
        ),
        unexpected_exit: env_flag(
            "DOTA_AGENT_CLI_DAEMON_UNEXPECTED_EXIT",
            "DOTA_CLI_DAEMON_UNEXPECTED_EXIT",
        ),
    }
}

pub fn parse_selector(raw: &str) -> Result<(String, String)> {
    let (key, value) = raw
        .split_once('=')
        .ok_or_else(|| anyhow!("selector '{raw}' must use KEY=VALUE"))?;
    if key.trim().is_empty() || value.trim().is_empty() {
        return Err(anyhow!(
            "selector '{raw}' must include a non-empty key and value"
        ));
    }
    Ok((key.trim().to_string(), value.trim().to_string()))
}

pub fn parse_selectors(values: &[String]) -> Result<BTreeMap<String, String>> {
    let mut selectors = BTreeMap::new();
    for value in values {
        let (key, parsed_value) = parse_selector(value)?;
        selectors.insert(key, parsed_value);
    }
    Ok(selectors)
}

pub fn build_context_state(
    name: Option<String>,
    selectors: BTreeMap<String, String>,
    current_directory: Option<PathBuf>,
) -> ActiveContextState {
    let mut ambient_cues = BTreeMap::new();
    if let Some(current_directory) = current_directory {
        ambient_cues.insert(
            "current_directory".to_string(),
            current_directory.display().to_string(),
        );
    }

    ActiveContextState {
        name,
        selectors,
        ambient_cues,
    }
}

pub fn load_active_context(runtime: &RuntimeLocations) -> Result<Option<ActiveContextState>> {
    let context_file = runtime.context_file();
    if !context_file.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(&context_file)
        .with_context(|| format!("failed to read {}", context_file.display()))?;
    let state = toml::from_str(&raw)
        .with_context(|| format!("failed to parse {}", context_file.display()))?;
    Ok(Some(state))
}

pub fn persist_active_context(
    runtime: &RuntimeLocations,
    state: &ActiveContextState,
) -> Result<ContextPersistenceResult> {
    runtime.ensure_exists()?;
    let serialized = toml::to_string_pretty(state).context("failed to serialize Active Context")?;
    let context_file = runtime.context_file();
    fs::write(&context_file, serialized)
        .with_context(|| format!("failed to write {}", context_file.display()))?;

    Ok(ContextPersistenceResult {
        status: "ok".to_string(),
        message: "Active Context updated".to_string(),
        active_context: state.clone(),
        context_file: context_file.display().to_string(),
    })
}

pub fn load_daemon_state(runtime: &RuntimeLocations) -> Result<PersistedDaemonState> {
    let daemon_state_file = runtime.daemon_state_file();
    if daemon_state_file.exists() {
        let raw = fs::read_to_string(&daemon_state_file)
            .with_context(|| format!("failed to read {}", daemon_state_file.display()))?;
        let state = toml::from_str(&raw)
            .with_context(|| format!("failed to parse {}", daemon_state_file.display()))?;
        return Ok(state);
    }

    let legacy_state_file = runtime.legacy_daemon_state_file();
    if legacy_state_file.exists() {
        let raw = fs::read_to_string(&legacy_state_file)
            .with_context(|| format!("failed to read {}", legacy_state_file.display()))?;
        let state = toml::from_str(&raw)
            .with_context(|| format!("failed to parse {}", legacy_state_file.display()))?;
        return Ok(state);
    }

    Ok(PersistedDaemonState::default())
}

pub fn persist_daemon_state(
    runtime: &RuntimeLocations,
    state: &PersistedDaemonState,
) -> Result<()> {
    runtime.ensure_exists()?;
    let serialized = toml::to_string_pretty(state).context("failed to serialize daemon state")?;
    let daemon_state_file = runtime.daemon_state_file();
    fs::write(&daemon_state_file, serialized)
        .with_context(|| format!("failed to write {}", daemon_state_file.display()))?;
    sync_daemon_runtime_artifacts(runtime, state)?;
    Ok(())
}

fn sync_daemon_runtime_artifacts(
    runtime: &RuntimeLocations,
    state: &PersistedDaemonState,
) -> Result<()> {
    let daemon_dir = runtime.daemon_dir();
    let pid_file = runtime.daemon_pid_file();
    let lock_file = runtime.daemon_lock_file();
    let endpoint_file = runtime.daemon_endpoint_file();
    let metadata_file = runtime.daemon_metadata_file();
    let auth_token_file = runtime.daemon_auth_token_file();
    let log_file = runtime.daemon_log_file();

    fs::create_dir_all(&daemon_dir)
        .with_context(|| format!("failed to create {}", daemon_dir.display()))?;

    let metadata = DaemonMetadata {
        state: state.state.clone(),
        readiness: state.readiness.clone(),
        reason: state.reason.clone(),
        recommended_next_action: state.recommended_next_action.clone(),
        instance_model: state.instance_model.clone(),
        instance_id: state.instance_id.clone(),
        last_action: state.last_action.clone(),
        updated_at: state.updated_at.clone(),
        transport: state.transport.clone(),
        endpoint: state.endpoint.clone(),
        pid: state.pid,
        lock_path: lock_file.display().to_string(),
        log_path: state.log_path.clone(),
        auth_token_path: state.auth_token_path.clone(),
    };
    let metadata_raw =
        toml::to_string_pretty(&metadata).context("failed to serialize daemon metadata")?;
    fs::write(&metadata_file, metadata_raw)
        .with_context(|| format!("failed to write {}", metadata_file.display()))?;

    if matches!(state.state, DaemonLifecycleState::Stopped) {
        remove_if_exists(&pid_file)?;
        remove_if_exists(&lock_file)?;
        remove_if_exists(&endpoint_file)?;
        remove_if_exists(&auth_token_file)?;
        return Ok(());
    }

    if let Some(pid) = state.pid {
        fs::write(&pid_file, format!("{pid}\n"))
            .with_context(|| format!("failed to write {}", pid_file.display()))?;
    } else {
        remove_if_exists(&pid_file)?;
    }

    fs::write(&lock_file, format!("instance_id={}\n", state.instance_id))
        .with_context(|| format!("failed to write {}", lock_file.display()))?;

    if let Some(endpoint) = &state.endpoint {
        fs::write(&endpoint_file, format!("{endpoint}\n"))
            .with_context(|| format!("failed to write {}", endpoint_file.display()))?;
    } else {
        remove_if_exists(&endpoint_file)?;
    }

    if let Some(log_path) = &state.log_path
        && std::path::Path::new(log_path) == log_file.as_path()
        && !log_file.exists()
    {
        fs::write(&log_file, b"")
            .with_context(|| format!("failed to write {}", log_file.display()))?;
    }

    if let Some(auth_token_path) = &state.auth_token_path {
        let token_path = PathBuf::from(auth_token_path);
        // Generate a cryptographically secure random token using 32 random bytes,
        // encoded as hex. This replaces the previous insecure instance_id+timestamp approach.
        let mut rng = rand::rngs::OsRng;
        let mut random_bytes = [0u8; 32];
        rng.fill_bytes(&mut random_bytes);
        let token = hex::encode(random_bytes);
        fs::write(&token_path, format!("{token}\n"))
            .with_context(|| format!("failed to write {}", token_path.display()))?;
    } else {
        remove_if_exists(&auth_token_file)?;
    }

    Ok(())
}

fn remove_if_exists(path: &PathBuf) -> Result<()> {
    if path.exists() {
        fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))?;
    }
    Ok(())
}

pub fn resolve_effective_context(
    persisted: Option<&ActiveContextState>,
    overrides: &InvocationContextOverrides,
) -> EffectiveContextView {
    let mut effective_values = BTreeMap::new();

    if let Some(persisted) = persisted {
        effective_values.extend(persisted.selectors.clone());
        effective_values.extend(persisted.ambient_cues.clone());
    }

    effective_values.extend(overrides.selectors.clone());

    if let Some(current_directory) = &overrides.current_directory {
        effective_values.insert(
            "current_directory".to_string(),
            current_directory.display().to_string(),
        );
    }

    EffectiveContextView {
        name: persisted.and_then(|state| state.name.clone()),
        effective_values,
        precedence_rule:
            "explicit invocation values override the persisted Active Context for one invocation only"
                .to_string(),
        persisted_context_present: persisted.is_some(),
    }
}

pub fn inspect_context(
    runtime: &RuntimeLocations,
    overrides: &InvocationContextOverrides,
) -> Result<ContextInspection> {
    let persisted_context = load_active_context(runtime)?;
    let effective_context = resolve_effective_context(persisted_context.as_ref(), overrides);

    Ok(ContextInspection {
        persisted_context,
        effective_context,
        context_file: runtime.context_file().display().to_string(),
    })
}

fn env_flag(primary_name: &str, legacy_name: &str) -> bool {
    std::env::var(primary_name)
        .or_else(|_| std::env::var(legacy_name))
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

fn timestamp_string() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}
