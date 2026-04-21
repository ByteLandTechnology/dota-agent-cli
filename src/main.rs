//! CLI entrypoint for the Dota 2 encyclopedia skill.

use clap::{ArgAction, Args, Parser, Subcommand, ValueEnum};
use dota_agent_cli::context::{
    InvocationContextOverrides, PersistedDaemonState, RuntimeLocations, RuntimeOverrides,
    build_context_state, daemon_simulation_flags, inspect_context, load_active_context,
    load_daemon_state, parse_selectors, persist_active_context, persist_daemon_state,
    resolve_effective_context, resolve_runtime_locations,
};
use dota_agent_cli::encyclopedia::{self, EntryKind, KnowledgeEntry, SearchRequest, SearchType};
use dota_agent_cli::help::{HelpDocument, HelpExample, plain_text_help, structured_help};
use dota_agent_cli::match_commands::{self, MatchSort};
use dota_agent_cli::providers::{
    FreshnessMode, ListSort, OverlayMode, ProviderSourceSelector, ResponseSourceMetadata,
    SourceSelector, WarmScope, load_live_entries, source_status, source_warm,
};
use dota_agent_cli::repl::start_repl;
use dota_agent_cli::{
    DaemonCommandOutput, DaemonLifecycleState, DaemonStatusOutput, ErrorContext, Format,
    StructuredError, serialize_value, write_structured_error,
};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
enum AppExit {
    Usage,
    Failure(anyhow::Error),
}

impl From<anyhow::Error> for AppExit {
    fn from(error: anyhow::Error) -> Self {
        Self::Failure(error)
    }
}

/// Live provider-backed Dota 2 knowledge and live-match CLI for agent-friendly lookups
#[derive(Parser, Debug)]
#[command(
    name = "dota-agent-cli",
    version,
    about = "Live provider-backed Dota 2 knowledge and live-match CLI for agent-friendly hero, item, match state, and source lookups",
    disable_help_flag = true,
    disable_help_subcommand = true
)]
struct Cli {
    /// Output format
    #[arg(long, short, value_enum, global = true, default_value_t = OutputFormat::Yaml)]
    format: OutputFormat,

    /// Render plain-text help for the selected command path
    #[arg(long, short = 'h', global = true, action = ArgAction::SetTrue)]
    help: bool,

    /// Override the default configuration directory
    #[arg(long, global = true)]
    config_dir: Option<PathBuf>,

    /// Override the default durable data directory
    #[arg(long, global = true)]
    data_dir: Option<PathBuf>,

    /// Override the runtime state directory
    #[arg(long, global = true)]
    state_dir: Option<PathBuf>,

    /// Override the cache directory
    #[arg(long, global = true)]
    cache_dir: Option<PathBuf>,

    /// Override the optional log directory
    #[arg(long, global = true)]
    log_dir: Option<PathBuf>,

    /// Start an interactive REPL session
    #[arg(long, global = true)]
    repl: bool,

    /// Choose whether eligible commands execute directly or route through the daemon contract
    #[arg(long, value_enum, global = true, default_value_t = ExecutionVia::Local)]
    via: ExecutionVia,

    /// Auto-start the daemon when a daemon-routed command finds it unavailable
    #[arg(long, global = true)]
    ensure_daemon: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    Yaml,
    Json,
    Toml,
}

impl From<OutputFormat> for Format {
    fn from(value: OutputFormat) -> Self {
        match value {
            OutputFormat::Yaml => Format::Yaml,
            OutputFormat::Json => Format::Json,
            OutputFormat::Toml => Format::Toml,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum ExecutionVia {
    Local,
    Daemon,
}

impl ExecutionVia {
    fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Daemon => "daemon",
        }
    }
}

#[derive(Copy, Clone, Debug)]
struct RoutingOptions {
    via: ExecutionVia,
    ensure_daemon: bool,
}

#[derive(Debug, Subcommand)]
enum Command {
    Help(HelpCommand),
    Run(RunCommand),
    Show(ShowCommand),
    List(ListCommand),
    Source(SourceCommand),
    Daemon(DaemonCommand),
    Paths(PathsCommand),
    Context(ContextCommand),
    Match(MatchCommand),
}

#[derive(Debug, Args)]
struct HelpCommand {
    #[arg(value_name = "COMMAND_PATH")]
    path: Vec<String>,
}

#[derive(Debug, Args)]
struct RunCommand {
    #[arg(value_name = "QUERY")]
    query: Vec<String>,

    #[arg(long = "type", value_enum, default_value_t = SearchType::All)]
    requested_type: SearchType,

    #[arg(long, default_value_t = 5)]
    limit: usize,

    #[arg(long)]
    expand: bool,

    #[arg(long)]
    tag: Option<String>,

    #[arg(long, value_enum, default_value_t = SourceSelector::Auto)]
    source: SourceSelector,

    #[arg(long, value_enum, default_value_t = FreshnessMode::Recent)]
    freshness: FreshnessMode,

    #[arg(long = "selector", value_name = "KEY=VALUE")]
    selectors: Vec<String>,

    #[arg(long = "cwd")]
    current_directory: Option<PathBuf>,

    #[arg(long)]
    log_enabled: bool,
}

#[derive(Debug, Args)]
struct ShowCommand {
    kind: EntryKind,

    #[arg(value_name = "NAME")]
    name: Vec<String>,

    #[arg(long)]
    related: bool,

    #[arg(long, value_enum, default_value_t = SourceSelector::Auto)]
    source: SourceSelector,

    #[arg(long, value_enum, default_value_t = FreshnessMode::Recent)]
    freshness: FreshnessMode,

    #[arg(long, value_enum, default_value_t = OverlayMode::Stats)]
    overlay: OverlayMode,

    #[arg(long = "selector", value_name = "KEY=VALUE")]
    selectors: Vec<String>,

    #[arg(long = "cwd")]
    current_directory: Option<PathBuf>,

    #[arg(long)]
    log_enabled: bool,
}

#[derive(Debug, Args)]
struct ListCommand {
    kind: EntryKind,

    #[arg(long)]
    tag: Option<String>,

    #[arg(long, default_value_t = 20)]
    limit: usize,

    #[arg(long, value_enum, default_value_t = SourceSelector::Auto)]
    source: SourceSelector,

    #[arg(long, value_enum, default_value_t = FreshnessMode::Recent)]
    freshness: FreshnessMode,

    #[arg(long, value_enum, default_value_t = ListSort::Name)]
    sort: ListSort,

    #[arg(long = "selector", value_name = "KEY=VALUE")]
    selectors: Vec<String>,

    #[arg(long = "cwd")]
    current_directory: Option<PathBuf>,

    #[arg(long)]
    log_enabled: bool,
}

#[derive(Debug, Args)]
struct SourceCommand {
    #[command(subcommand)]
    command: Option<SourceSubcommand>,
}

#[derive(Debug, Subcommand)]
enum SourceSubcommand {
    Status(SourceStatusArgs),
    Warm(SourceWarmArgs),
}

#[derive(Debug, Args)]
struct SourceStatusArgs {
    #[arg(long, value_enum, default_value_t = ProviderSourceSelector::Auto)]
    source: ProviderSourceSelector,

    #[arg(long, value_enum, default_value_t = FreshnessMode::Recent)]
    freshness: FreshnessMode,
}

#[derive(Debug, Args)]
struct SourceWarmArgs {
    #[arg(long, value_enum, default_value_t = ProviderSourceSelector::Auto)]
    source: ProviderSourceSelector,

    #[arg(long, value_enum, default_value_t = WarmScope::Indexes)]
    scope: WarmScope,

    #[arg(long)]
    force: bool,
}

#[derive(Debug, Args)]
struct DaemonCommand {
    #[command(subcommand)]
    command: Option<DaemonSubcommand>,
}

#[derive(Debug, Subcommand)]
enum DaemonSubcommand {
    Start(DaemonStartArgs),
    Stop(DaemonStopArgs),
    Restart(DaemonRestartArgs),
    Status,
    Run(DaemonRunArgs),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum DaemonTransport {
    Stdio,
    Tcp,
    Unix,
}

impl DaemonTransport {
    fn as_str(self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::Tcp => "tcp",
            Self::Unix => "unix",
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum DaemonAuthMode {
    CapabilityToken,
    SignedBearerToken,
}

impl DaemonAuthMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::CapabilityToken => "capability-token",
            Self::SignedBearerToken => "signed-bearer-token",
        }
    }
}

#[derive(Debug, Args)]
struct DaemonStartArgs {
    #[arg(long, value_enum, default_value_t = DaemonTransport::Unix)]
    transport: DaemonTransport,

    #[arg(long)]
    bind: Option<String>,

    #[arg(long, default_value_t = 30)]
    timeout_sec: u64,

    #[arg(long)]
    cert_file: Option<PathBuf>,

    #[arg(long)]
    key_file: Option<PathBuf>,

    #[arg(long, value_enum, default_value_t = DaemonAuthMode::CapabilityToken)]
    auth_mode: DaemonAuthMode,
}

#[derive(Debug, Args)]
struct DaemonStopArgs {
    #[arg(long, default_value_t = 30)]
    timeout_sec: u64,
}

#[derive(Debug, Args)]
struct DaemonRestartArgs {
    #[arg(long, value_enum, default_value_t = DaemonTransport::Unix)]
    transport: DaemonTransport,

    #[arg(long)]
    bind: Option<String>,

    #[arg(long, default_value_t = 30)]
    timeout_sec: u64,

    #[arg(long)]
    cert_file: Option<PathBuf>,

    #[arg(long)]
    key_file: Option<PathBuf>,

    #[arg(long, value_enum, default_value_t = DaemonAuthMode::CapabilityToken)]
    auth_mode: DaemonAuthMode,
}

#[derive(Debug, Args)]
struct DaemonRunArgs {
    #[arg(long, value_enum, default_value_t = DaemonTransport::Unix)]
    transport: DaemonTransport,

    #[arg(long)]
    bind: Option<String>,

    #[arg(long)]
    cert_file: Option<PathBuf>,

    #[arg(long)]
    key_file: Option<PathBuf>,

    #[arg(long, value_enum, default_value_t = DaemonAuthMode::CapabilityToken)]
    auth_mode: DaemonAuthMode,
}

#[derive(Debug, Args)]
struct PathsCommand {
    #[arg(long)]
    log_enabled: bool,
}

#[derive(Debug, Args)]
struct ContextCommand {
    #[command(subcommand)]
    command: Option<ContextSubcommand>,
}

#[derive(Debug, Subcommand)]
enum ContextSubcommand {
    Show,
    Use(ContextUseCommand),
}

#[derive(Debug, Args)]
struct ContextUseCommand {
    #[arg(long)]
    name: Option<String>,

    #[arg(long = "selector", value_name = "KEY=VALUE")]
    selectors: Vec<String>,

    #[arg(long = "cwd")]
    current_directory: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct MatchCommand {
    #[command(subcommand)]
    command: Option<MatchSubcommand>,
}

#[derive(Debug, Subcommand)]
enum MatchSubcommand {
    Live(MatchLiveCommand),
    Show(MatchShowCommand),
    Recent(MatchRecentCommand),
}

#[derive(Debug, Args)]
struct MatchLiveCommand {
    #[arg(long, value_enum, default_value_t = ProviderSourceSelector::Auto)]
    source: ProviderSourceSelector,

    #[arg(long, value_enum, default_value_t = FreshnessMode::Live)]
    freshness: FreshnessMode,

    #[arg(long, default_value_t = 10)]
    limit: usize,

    #[arg(long)]
    league_id: Option<i32>,

    #[arg(long)]
    min_mmr: Option<i32>,

    #[arg(long = "selector", value_name = "KEY=VALUE")]
    selectors: Vec<String>,

    #[arg(long = "cwd")]
    current_directory: Option<PathBuf>,

    #[arg(long)]
    log_enabled: bool,
}

#[derive(Debug, Args)]
struct MatchShowCommand {
    match_id: i64,

    #[arg(long, value_enum, default_value_t = ProviderSourceSelector::Auto)]
    source: ProviderSourceSelector,

    #[arg(long, value_enum, default_value_t = FreshnessMode::Recent)]
    freshness: FreshnessMode,

    #[arg(long)]
    expand: bool,

    #[arg(long = "selector", value_name = "KEY=VALUE")]
    selectors: Vec<String>,

    #[arg(long = "cwd")]
    current_directory: Option<PathBuf>,

    #[arg(long)]
    log_enabled: bool,
}

#[derive(Debug, Args)]
struct MatchRecentCommand {
    #[arg(long)]
    player_id: Option<i64>,

    #[arg(long)]
    hero: Option<String>,

    #[arg(long, value_enum, default_value_t = ProviderSourceSelector::Auto)]
    source: ProviderSourceSelector,

    #[arg(long, value_enum, default_value_t = FreshnessMode::Recent)]
    freshness: FreshnessMode,

    #[arg(long, default_value_t = 20)]
    limit: usize,

    #[arg(long, value_enum, default_value_t = MatchSort::Recent)]
    sort: MatchSort,

    #[arg(long)]
    won: bool,

    #[arg(long = "selector", value_name = "KEY=VALUE")]
    selectors: Vec<String>,

    #[arg(long = "cwd")]
    current_directory: Option<PathBuf>,

    #[arg(long)]
    log_enabled: bool,
}

fn main() {
    let exit_code = match run_cli() {
        Ok(()) => 0,
        Err(AppExit::Usage) => 2,
        Err(AppExit::Failure(error)) => {
            eprintln!("error: {error:#}");
            1
        }
    };
    std::process::exit(exit_code);
}

fn run_cli() -> std::result::Result<(), AppExit> {
    let raw_args: Vec<String> = std::env::args().collect();
    let detected_format = detect_requested_format(&raw_args);

    let cli = match Cli::try_parse_from(&raw_args) {
        Ok(cli) => cli,
        Err(error) => return handle_parse_error(error, detected_format),
    };

    let format: Format = cli.format.into();
    let routing = RoutingOptions {
        via: cli.via,
        ensure_daemon: cli.ensure_daemon,
    };

    if cli.help {
        reject_local_only_routing(&routing, "help", format)?;
        return render_plain_text_help_for_cli(&cli);
    }

    let runtime_overrides = RuntimeOverrides {
        config_dir: cli.config_dir.clone(),
        data_dir: cli.data_dir.clone(),
        state_dir: cli.state_dir.clone(),
        cache_dir: cli.cache_dir.clone(),
        log_dir: cli.log_dir.clone(),
    };

    if cli.repl {
        let runtime =
            resolve_runtime_locations(&runtime_overrides, false).map_err(AppExit::from)?;
        return start_repl(format, runtime).map_err(AppExit::from);
    }

    match cli.command {
        None => {
            reject_local_only_routing(&routing, "help", format)?;
            render_plain_text_help_for_path(&[])
        }
        Some(Command::Help(command)) => {
            reject_local_only_routing(&routing, "help", format)?;
            render_structured_help(&command.path, format)
        }
        Some(Command::Run(command)) => execute_run(runtime_overrides, routing, command, format),
        Some(Command::Show(command)) => execute_show(runtime_overrides, routing, command, format),
        Some(Command::List(command)) => execute_list(runtime_overrides, routing, command, format),
        Some(Command::Source(command)) => {
            reject_local_only_routing(&routing, "source", format)?;
            execute_source(runtime_overrides, command, format)
        }
        Some(Command::Daemon(command)) => {
            reject_local_only_routing(&routing, "daemon", format)?;
            execute_daemon(runtime_overrides, command, format)
        }
        Some(Command::Paths(command)) => {
            reject_local_only_routing(&routing, "paths", format)?;
            execute_paths(runtime_overrides, command, format)
        }
        Some(Command::Context(command)) => {
            reject_local_only_routing(&routing, "context", format)?;
            execute_context(runtime_overrides, command, format)
        }
        Some(Command::Match(command)) => execute_match(runtime_overrides, routing, command, format),
    }
}

fn handle_parse_error(error: clap::Error, format: Format) -> std::result::Result<(), AppExit> {
    if error.kind() == clap::error::ErrorKind::DisplayVersion {
        error.print().map_err(|err| AppExit::Failure(err.into()))?;
        return Ok(());
    }

    let structured_error =
        StructuredError::new("usage.parse_error", error.to_string(), "help_usage", format);
    let mut stderr = std::io::stderr().lock();
    write_structured_error(&mut stderr, &structured_error, format).map_err(AppExit::from)?;
    Err(AppExit::Usage)
}

fn detect_requested_format(args: &[String]) -> Format {
    let mut args = args.iter().peekable();
    while let Some(arg) = args.next() {
        if let Some(value) = arg.strip_prefix("--format=") {
            return parse_format_token(value).unwrap_or(Format::Yaml);
        }
        if (arg == "--format" || arg == "-f")
            && let Some(value) = args.peek()
        {
            return parse_format_token(value).unwrap_or(Format::Yaml);
        }
    }
    Format::Yaml
}

fn parse_format_token(token: &str) -> Option<Format> {
    match token {
        "yaml" => Some(Format::Yaml),
        "json" => Some(Format::Json),
        "toml" => Some(Format::Toml),
        _ => None,
    }
}

fn render_plain_text_help_for_cli(cli: &Cli) -> std::result::Result<(), AppExit> {
    let path = match &cli.command {
        None => Vec::new(),
        Some(Command::Help(_)) => vec!["help".to_string()],
        Some(Command::Run(_)) => vec!["run".to_string()],
        Some(Command::Show(_)) => vec!["show".to_string()],
        Some(Command::List(_)) => vec!["list".to_string()],
        Some(Command::Source(SourceCommand { command: None })) => vec!["source".to_string()],
        Some(Command::Source(SourceCommand {
            command: Some(SourceSubcommand::Status(_)),
        })) => vec!["source".to_string(), "status".to_string()],
        Some(Command::Source(SourceCommand {
            command: Some(SourceSubcommand::Warm(_)),
        })) => vec!["source".to_string(), "warm".to_string()],
        Some(Command::Daemon(DaemonCommand { command: None })) => vec!["daemon".to_string()],
        Some(Command::Daemon(DaemonCommand {
            command: Some(DaemonSubcommand::Start(_)),
        })) => vec!["daemon".to_string(), "start".to_string()],
        Some(Command::Daemon(DaemonCommand {
            command: Some(DaemonSubcommand::Stop(_)),
        })) => vec!["daemon".to_string(), "stop".to_string()],
        Some(Command::Daemon(DaemonCommand {
            command: Some(DaemonSubcommand::Restart(_)),
        })) => vec!["daemon".to_string(), "restart".to_string()],
        Some(Command::Daemon(DaemonCommand {
            command: Some(DaemonSubcommand::Status),
        })) => vec!["daemon".to_string(), "status".to_string()],
        Some(Command::Daemon(DaemonCommand {
            command: Some(DaemonSubcommand::Run(_)),
        })) => vec!["daemon".to_string(), "run".to_string()],
        Some(Command::Paths(_)) => vec!["paths".to_string()],
        Some(Command::Context(ContextCommand { command: None })) => vec!["context".to_string()],
        Some(Command::Context(ContextCommand {
            command: Some(ContextSubcommand::Show),
        })) => vec!["context".to_string(), "show".to_string()],
        Some(Command::Context(ContextCommand {
            command: Some(ContextSubcommand::Use(_)),
        })) => vec!["context".to_string(), "use".to_string()],
        Some(Command::Match(MatchCommand { command: None })) => vec!["match".to_string()],
        Some(Command::Match(MatchCommand {
            command: Some(MatchSubcommand::Live(_)),
        })) => vec!["match".to_string(), "live".to_string()],
        Some(Command::Match(MatchCommand {
            command: Some(MatchSubcommand::Show(_)),
        })) => vec!["match".to_string(), "show".to_string()],
        Some(Command::Match(MatchCommand {
            command: Some(MatchSubcommand::Recent(_)),
        })) => vec!["match".to_string(), "recent".to_string()],
    };
    render_plain_text_help_for_path(&path)
}

fn render_plain_text_help_for_path(path: &[String]) -> std::result::Result<(), AppExit> {
    let Some(help_text) = plain_text_help(path) else {
        let error = StructuredError::new(
            "help.unknown_path",
            format!("unknown help path '{}'", path.join(" ")),
            "help_usage",
            Format::Yaml,
        );
        return render_structured_error(error, Format::Yaml);
    };

    println!("{help_text}");
    Ok(())
}

fn render_structured_help(path: &[String], format: Format) -> std::result::Result<(), AppExit> {
    let Some(help_document) = structured_help(path) else {
        let error = StructuredError::new(
            "help.unknown_path",
            format!("unknown help path '{}'", path.join(" ")),
            "help_usage",
            format,
        );
        return render_structured_error(error, format);
    };
    write_stdout(&StructuredHelpOutput::new(&help_document), format)
}

#[derive(serde::Serialize)]
struct StructuredHelpOutput<'a> {
    #[serde(flatten)]
    document: &'a HelpDocument,
    description: &'a [String],
    examples: &'a [HelpExample],
}

impl<'a> StructuredHelpOutput<'a> {
    fn new(document: &'a HelpDocument) -> Self {
        Self {
            document,
            description: &document.description,
            examples: &document.examples,
        }
    }
}

fn reject_local_only_routing(
    routing: &RoutingOptions,
    command: &str,
    format: Format,
) -> std::result::Result<(), AppExit> {
    if matches!(routing.via, ExecutionVia::Local) && !routing.ensure_daemon {
        return Ok(());
    }

    render_leaf_error(
        ErrorContext::new(
            "runtime.daemon_routing_unsupported",
            format!("'{command}' only supports local execution"),
            "daemon_routing",
        )
        .with_detail("command", command)
        .with_detail("via", routing.via.as_str())
        .with_detail("ensure_daemon", routing.ensure_daemon.to_string()),
        format,
    )
}

fn maybe_handle_daemon_routed_command(
    runtime: &RuntimeLocations,
    routing: RoutingOptions,
    command: &str,
    format: Format,
) -> std::result::Result<(), AppExit> {
    if matches!(routing.via, ExecutionVia::Local) {
        if routing.ensure_daemon {
            return render_leaf_error(
                ErrorContext::new(
                    "daemon.ensure_requires_daemon_route",
                    format!("'{command}' can only use --ensure-daemon together with --via daemon"),
                    "daemon_routing",
                )
                .with_detail("command", command)
                .with_detail("via", routing.via.as_str()),
                format,
            );
        }
        return Ok(());
    }

    let mut state = load_daemon_state(runtime).map_err(AppExit::from)?;
    normalize_daemon_state_for_observed_exit(runtime, &mut state).map_err(AppExit::from)?;

    if !daemon_is_ready(&state) {
        if !routing.ensure_daemon {
            return render_routed_daemon_error(
                ErrorContext::new(
                    "daemon.route_unavailable",
                    format!(
                        "daemon routing for '{command}' requires a ready daemon; use --ensure-daemon to auto-start it"
                    ),
                    "daemon_routing",
                )
                .with_detail("command", command),
                &state,
                routing,
                format,
            );
        }

        let flags = daemon_simulation_flags();
        if flags.block_control {
            return render_routed_daemon_error(
                ErrorContext::new(
                    "daemon.route_start_blocked",
                    format!(
                        "daemon routing for '{command}' could not auto-start the daemon because another control action is in progress"
                    ),
                    "daemon_routing",
                )
                .with_detail("command", command),
                &state,
                routing,
                format,
            );
        }

        let transport = state
            .transport
            .as_deref()
            .and_then(parse_daemon_transport)
            .unwrap_or(DaemonTransport::Unix);
        let bind = state.endpoint.clone();
        let auth_mode = if state.auth_token_path.is_some() {
            DaemonAuthMode::CapabilityToken
        } else {
            DaemonAuthMode::SignedBearerToken
        };
        apply_daemon_launch_metadata(runtime, &mut state, transport, bind.as_deref(), auth_mode);
        let launch_summary =
            daemon_launch_summary(transport, bind.as_deref(), auth_mode, None, None);
        let output = daemon_start_output(
            runtime,
            &mut state,
            flags.fail_start,
            flags.timeout_start,
            30,
            &launch_summary,
        )
        .map_err(AppExit::from)?;

        if output.result != "running" && output.result != "no_op" {
            return render_routed_daemon_error(
                ErrorContext::new(
                    "daemon.route_start_failed",
                    format!("daemon routing for '{command}' failed while preparing the daemon"),
                    "daemon_routing",
                )
                .with_detail("command", command)
                .with_detail("daemon_result", output.result.clone())
                .with_detail("daemon_message", output.message.clone()),
                &state,
                routing,
                format,
            );
        }
    }

    if state.transport.is_none() || state.endpoint.is_none() || state.log_path.is_none() {
        let transport = state
            .transport
            .as_deref()
            .and_then(parse_daemon_transport)
            .unwrap_or(DaemonTransport::Unix);
        let bind = state.endpoint.clone();
        let auth_mode = if state.auth_token_path.is_some() {
            DaemonAuthMode::CapabilityToken
        } else {
            DaemonAuthMode::SignedBearerToken
        };
        apply_daemon_launch_metadata(runtime, &mut state, transport, bind.as_deref(), auth_mode);
    }

    state.last_action = format!("route:{command}");
    refresh_daemon_state_timestamp(&mut state);
    persist_daemon_state(runtime, &state).map_err(AppExit::from)?;
    Ok(())
}

fn render_routed_daemon_error(
    error: ErrorContext,
    state: &PersistedDaemonState,
    routing: RoutingOptions,
    format: Format,
) -> std::result::Result<(), AppExit> {
    let mut error = error
        .with_detail("via", routing.via.as_str())
        .with_detail("ensure_daemon", routing.ensure_daemon.to_string())
        .with_detail("daemon_state", format!("{:?}", state.state).to_lowercase())
        .with_detail("daemon_readiness", state.readiness.clone())
        .with_detail(
            "recommended_next_action",
            state.recommended_next_action.clone(),
        );

    if let Some(endpoint) = &state.endpoint {
        error = error.with_detail("daemon_endpoint", endpoint.clone());
    }
    if let Some(reason) = &state.reason {
        error = error.with_detail("daemon_reason", reason.clone());
    }

    render_leaf_error(error, format)
}

fn daemon_is_ready(state: &PersistedDaemonState) -> bool {
    matches!(state.state, DaemonLifecycleState::Running) && state.readiness == "ready"
}

fn normalize_daemon_state_for_observed_exit(
    runtime: &RuntimeLocations,
    state: &mut PersistedDaemonState,
) -> anyhow::Result<()> {
    if daemon_simulation_flags().unexpected_exit
        && matches!(state.state, DaemonLifecycleState::Running)
    {
        state.state = DaemonLifecycleState::Failed;
        state.readiness = "not_ready".to_string();
        state.reason = Some("the managed daemon exited unexpectedly".to_string());
        state.recommended_next_action = "restart".to_string();
        state.last_action = "status".to_string();
        refresh_daemon_state_timestamp(state);
        persist_daemon_state(runtime, state)?;
    }

    Ok(())
}

fn parse_daemon_transport(raw: &str) -> Option<DaemonTransport> {
    match raw {
        "stdio" => Some(DaemonTransport::Stdio),
        "tcp" => Some(DaemonTransport::Tcp),
        "unix" => Some(DaemonTransport::Unix),
        _ => None,
    }
}

fn execute_paths(
    overrides: RuntimeOverrides,
    command: PathsCommand,
    format: Format,
) -> std::result::Result<(), AppExit> {
    let runtime =
        resolve_runtime_locations(&overrides, command.log_enabled).map_err(AppExit::from)?;
    write_stdout(&runtime.summary(), format)
}

fn execute_context(
    overrides: RuntimeOverrides,
    command: ContextCommand,
    format: Format,
) -> std::result::Result<(), AppExit> {
    match command.command {
        None => render_plain_text_help_for_path(&["context".to_string()]),
        Some(ContextSubcommand::Show) => {
            let runtime = resolve_runtime_locations(&overrides, false).map_err(AppExit::from)?;
            runtime.ensure_exists().map_err(AppExit::from)?;
            let inspection = inspect_context(&runtime, &InvocationContextOverrides::default())
                .map_err(AppExit::from)?;
            write_stdout(&inspection, format)
        }
        Some(ContextSubcommand::Use(command)) => {
            let selectors = parse_selectors(&command.selectors).map_err(AppExit::from)?;
            let current_directory = command.current_directory;
            if selectors.is_empty() && current_directory.is_none() && command.name.is_none() {
                return render_leaf_error(
                    ErrorContext::new(
                        "context.missing_values",
                        "provide at least one --selector, --cwd, or --name when persisting an Active Context",
                        "runtime_state",
                    ),
                    format,
                );
            }

            let runtime = resolve_runtime_locations(&overrides, false).map_err(AppExit::from)?;
            runtime.ensure_exists().map_err(AppExit::from)?;
            let state = build_context_state(command.name, selectors, current_directory);
            let persisted = persist_active_context(&runtime, &state).map_err(AppExit::from)?;
            write_stdout(&persisted, format)
        }
    }
}

fn execute_match(
    overrides: RuntimeOverrides,
    routing: RoutingOptions,
    command: MatchCommand,
    format: Format,
) -> std::result::Result<(), AppExit> {
    match command.command {
        None => render_plain_text_help_for_path(&["match".to_string()]),
        Some(MatchSubcommand::Live(args)) => execute_match_live(overrides, routing, args, format),
        Some(MatchSubcommand::Show(args)) => execute_match_show(overrides, routing, args, format),
        Some(MatchSubcommand::Recent(args)) => {
            execute_match_recent(overrides, routing, args, format)
        }
    }
}

fn execute_match_live(
    overrides: RuntimeOverrides,
    routing: RoutingOptions,
    args: MatchLiveCommand,
    format: Format,
) -> std::result::Result<(), AppExit> {
    let runtime = resolve_runtime_locations(&overrides, args.log_enabled).map_err(AppExit::from)?;
    runtime.ensure_exists().map_err(AppExit::from)?;

    maybe_handle_daemon_routed_command(&runtime, routing, "match live", format)?;

    let output = match_commands::fetch_live_matches(
        &runtime,
        args.source,
        args.freshness,
        args.limit,
        args.league_id,
        args.min_mmr,
    )
    .map_err(|error| leaf_exit(error, format))?;
    write_stdout(&output, format)
}

fn execute_match_show(
    overrides: RuntimeOverrides,
    routing: RoutingOptions,
    args: MatchShowCommand,
    format: Format,
) -> std::result::Result<(), AppExit> {
    let runtime = resolve_runtime_locations(&overrides, args.log_enabled).map_err(AppExit::from)?;
    runtime.ensure_exists().map_err(AppExit::from)?;

    maybe_handle_daemon_routed_command(&runtime, routing, "match show", format)?;

    let output = match_commands::fetch_match_detail(
        &runtime,
        args.source,
        args.freshness,
        args.match_id,
        args.expand,
    )
    .map_err(|error| leaf_exit(error, format))?;
    write_stdout(&output, format)
}

fn execute_match_recent(
    overrides: RuntimeOverrides,
    routing: RoutingOptions,
    args: MatchRecentCommand,
    format: Format,
) -> std::result::Result<(), AppExit> {
    let runtime = resolve_runtime_locations(&overrides, args.log_enabled).map_err(AppExit::from)?;
    runtime.ensure_exists().map_err(AppExit::from)?;
    let effective_context =
        resolve_command_context(&runtime, &args.selectors, args.current_directory.clone())?;

    if args.won
        && args.player_id.is_none()
        && !effective_context.effective_values.contains_key("player_id")
    {
        return render_leaf_error(
            ErrorContext::new(
                "match.unsupported_filter",
                "--won requires --player-id or a player_id Active Context selector",
                "match_validation",
            ),
            format,
        );
    }

    maybe_handle_daemon_routed_command(&runtime, routing, "match recent", format)?;

    // Load hero entries for name resolution
    let (hero_entries, _) = knowledge_entries_for_kind(
        &runtime,
        EntryKind::Hero,
        SourceSelector::Auto,
        args.freshness,
    )
    .map_err(|error| leaf_exit(error, format))?;

    let output = match_commands::fetch_recent_matches(
        &runtime,
        args.source,
        args.freshness,
        args.player_id,
        args.hero.as_deref(),
        args.limit,
        args.sort,
        args.won,
        &effective_context.effective_values,
        &hero_entries,
    )
    .map_err(|error| leaf_exit(error, format))?;
    write_stdout(&output, format)
}

fn execute_source(
    overrides: RuntimeOverrides,
    command: SourceCommand,
    format: Format,
) -> std::result::Result<(), AppExit> {
    match command.command {
        None => render_plain_text_help_for_path(&["source".to_string()]),
        Some(SourceSubcommand::Status(args)) => {
            let runtime = resolve_runtime_locations(&overrides, false).map_err(AppExit::from)?;
            runtime.ensure_exists().map_err(AppExit::from)?;
            let output = source_status(&runtime, args.source, args.freshness)
                .map_err(|error| leaf_exit(error, format))?;
            write_stdout(&output, format)
        }
        Some(SourceSubcommand::Warm(args)) => {
            let runtime = resolve_runtime_locations(&overrides, false).map_err(AppExit::from)?;
            runtime.ensure_exists().map_err(AppExit::from)?;
            let output = source_warm(&runtime, args.source, args.scope, args.force)
                .map_err(|error| leaf_exit(error, format))?;
            write_stdout(&output, format)
        }
    }
}

fn execute_daemon(
    overrides: RuntimeOverrides,
    command: DaemonCommand,
    format: Format,
) -> std::result::Result<(), AppExit> {
    match command.command {
        None => render_plain_text_help_for_path(&["daemon".to_string()]),
        Some(DaemonSubcommand::Status) => {
            let runtime = resolve_runtime_locations(&overrides, false).map_err(AppExit::from)?;
            runtime.ensure_exists().map_err(AppExit::from)?;
            let mut state = load_daemon_state(&runtime).map_err(AppExit::from)?;
            normalize_daemon_state_for_observed_exit(&runtime, &mut state)
                .map_err(AppExit::from)?;

            let status = DaemonStatusOutput {
                state: state.state,
                readiness: state.readiness,
                reason: state.reason,
                recommended_next_action: state.recommended_next_action,
                instance_model: state.instance_model,
                instance_id: state.instance_id,
            };
            write_stdout(&status, format)
        }
        Some(DaemonSubcommand::Start(args)) => execute_daemon_start(overrides, args, format),
        Some(DaemonSubcommand::Stop(args)) => execute_daemon_stop(overrides, args, format),
        Some(DaemonSubcommand::Restart(args)) => execute_daemon_restart(overrides, args, format),
        Some(DaemonSubcommand::Run(args)) => execute_daemon_run(overrides, args, format),
    }
}

fn execute_daemon_start(
    overrides: RuntimeOverrides,
    args: DaemonStartArgs,
    format: Format,
) -> std::result::Result<(), AppExit> {
    let runtime = resolve_runtime_locations(&overrides, false).map_err(AppExit::from)?;
    runtime.ensure_exists().map_err(AppExit::from)?;
    let flags = daemon_simulation_flags();
    let mut state = load_daemon_state(&runtime).map_err(AppExit::from)?;
    let launch_summary = daemon_launch_summary(
        args.transport,
        args.bind.as_deref(),
        args.auth_mode,
        args.cert_file.as_deref(),
        args.key_file.as_deref(),
    );
    apply_daemon_launch_metadata(
        &runtime,
        &mut state,
        args.transport,
        args.bind.as_deref(),
        args.auth_mode,
    );

    if flags.block_control {
        return write_stdout(
            &DaemonCommandOutput {
                action: "start".to_string(),
                result: "blocked".to_string(),
                state: state.state,
                message: format!(
                    "another daemon control action is already in progress; requested {launch_summary}"
                ),
                recommended_next_action: "status".to_string(),
                instance_model: state.instance_model,
                instance_id: state.instance_id,
            },
            format,
        );
    }

    let output = daemon_start_output(
        &runtime,
        &mut state,
        flags.fail_start,
        flags.timeout_start,
        args.timeout_sec,
        &launch_summary,
    )
    .map_err(AppExit::from)?;
    write_stdout(&output, format)
}

fn execute_daemon_stop(
    overrides: RuntimeOverrides,
    args: DaemonStopArgs,
    format: Format,
) -> std::result::Result<(), AppExit> {
    let runtime = resolve_runtime_locations(&overrides, false).map_err(AppExit::from)?;
    runtime.ensure_exists().map_err(AppExit::from)?;
    let flags = daemon_simulation_flags();
    let mut state = load_daemon_state(&runtime).map_err(AppExit::from)?;

    if flags.block_control {
        return write_stdout(
            &DaemonCommandOutput {
                action: "stop".to_string(),
                result: "blocked".to_string(),
                state: state.state,
                message: format!(
                    "another daemon control action is already in progress; stop requested with timeout {}s",
                    args.timeout_sec
                ),
                recommended_next_action: "status".to_string(),
                instance_model: state.instance_model,
                instance_id: state.instance_id,
            },
            format,
        );
    }

    let output = daemon_stop_output(
        &runtime,
        &mut state,
        flags.fail_stop,
        flags.timeout_stop,
        args.timeout_sec,
    )
    .map_err(AppExit::from)?;
    write_stdout(&output, format)
}

fn execute_daemon_restart(
    overrides: RuntimeOverrides,
    args: DaemonRestartArgs,
    format: Format,
) -> std::result::Result<(), AppExit> {
    let runtime = resolve_runtime_locations(&overrides, false).map_err(AppExit::from)?;
    runtime.ensure_exists().map_err(AppExit::from)?;
    let flags = daemon_simulation_flags();
    let mut state = load_daemon_state(&runtime).map_err(AppExit::from)?;
    let launch_summary = daemon_launch_summary(
        args.transport,
        args.bind.as_deref(),
        args.auth_mode,
        args.cert_file.as_deref(),
        args.key_file.as_deref(),
    );
    apply_daemon_launch_metadata(
        &runtime,
        &mut state,
        args.transport,
        args.bind.as_deref(),
        args.auth_mode,
    );

    if flags.block_control {
        return write_stdout(
            &DaemonCommandOutput {
                action: "restart".to_string(),
                result: "blocked".to_string(),
                state: state.state,
                message: format!(
                    "another daemon control action is already in progress; requested {launch_summary}"
                ),
                recommended_next_action: "status".to_string(),
                instance_model: state.instance_model,
                instance_id: state.instance_id,
            },
            format,
        );
    }

    let output = daemon_restart_output(
        &runtime,
        &mut state,
        flags.fail_restart,
        flags.timeout_restart,
        args.timeout_sec,
        &launch_summary,
    )
    .map_err(AppExit::from)?;
    write_stdout(&output, format)
}

fn execute_daemon_run(
    overrides: RuntimeOverrides,
    args: DaemonRunArgs,
    format: Format,
) -> std::result::Result<(), AppExit> {
    let runtime = resolve_runtime_locations(&overrides, false).map_err(AppExit::from)?;
    runtime.ensure_exists().map_err(AppExit::from)?;
    let mut state = load_daemon_state(&runtime).map_err(AppExit::from)?;
    let launch_summary = daemon_launch_summary(
        args.transport,
        args.bind.as_deref(),
        args.auth_mode,
        args.cert_file.as_deref(),
        args.key_file.as_deref(),
    );
    apply_daemon_launch_metadata(
        &runtime,
        &mut state,
        args.transport,
        args.bind.as_deref(),
        args.auth_mode,
    );

    state.state = DaemonLifecycleState::Running;
    state.readiness = "ready".to_string();
    state.reason = Some("internal daemon process entrypoint activated".to_string());
    state.recommended_next_action = "status".to_string();
    state.last_action = "run".to_string();
    refresh_daemon_state_timestamp(&mut state);
    persist_daemon_state(&runtime, &state).map_err(AppExit::from)?;

    write_stdout(
        &DaemonCommandOutput {
            action: "run".to_string(),
            result: "running".to_string(),
            state: state.state,
            message: format!(
                "internal daemon process entrypoint initialized for cache warming and provider proxy work using {launch_summary}"
            ),
            recommended_next_action: "status".to_string(),
            instance_model: state.instance_model,
            instance_id: state.instance_id,
        },
        format,
    )
}

fn daemon_start_output(
    runtime: &RuntimeLocations,
    state: &mut PersistedDaemonState,
    fail: bool,
    timeout: bool,
    timeout_sec: u64,
    launch_summary: &str,
) -> anyhow::Result<DaemonCommandOutput> {
    if matches!(state.state, DaemonLifecycleState::Running) {
        return Ok(daemon_command_output(
            "start",
            "no_op",
            state.clone(),
            &format!("the managed daemon is already running using {launch_summary}"),
            "status",
        ));
    }

    if timeout {
        state.state = DaemonLifecycleState::Starting;
        state.readiness = "pending".to_string();
        state.reason = Some(format!(
            "the daemon did not report a terminal outcome before the {timeout_sec}s timeout expired"
        ));
        state.recommended_next_action = "status".to_string();
        state.last_action = "start".to_string();
        refresh_daemon_state_timestamp(state);
        persist_daemon_state(runtime, state)?;
        return Ok(daemon_command_output(
            "start",
            "timed_out",
            state.clone(),
            &format!(
                "the managed daemon is still transitioning after {timeout_sec}s; inspect status for the current observable state ({launch_summary})"
            ),
            "status",
        ));
    }

    if fail {
        state.state = DaemonLifecycleState::Failed;
        state.readiness = "not_ready".to_string();
        state.reason = Some("the managed daemon failed to start".to_string());
        state.recommended_next_action = "restart".to_string();
        state.last_action = "start".to_string();
        refresh_daemon_state_timestamp(state);
        persist_daemon_state(runtime, state)?;
        return Ok(daemon_command_output(
            "start",
            "failed",
            state.clone(),
            &format!("the managed daemon failed to start using {launch_summary}"),
            "restart",
        ));
    }

    state.state = DaemonLifecycleState::Running;
    state.readiness = "ready".to_string();
    state.reason = None;
    state.recommended_next_action = "status".to_string();
    state.last_action = "start".to_string();
    refresh_daemon_state_timestamp(state);
    persist_daemon_state(runtime, state)?;
    Ok(daemon_command_output(
        "start",
        "running",
        state.clone(),
        &format!("the managed daemon is now running using {launch_summary}"),
        "status",
    ))
}

fn daemon_stop_output(
    runtime: &RuntimeLocations,
    state: &mut PersistedDaemonState,
    fail: bool,
    timeout: bool,
    timeout_sec: u64,
) -> anyhow::Result<DaemonCommandOutput> {
    if matches!(state.state, DaemonLifecycleState::Stopped) {
        return Ok(daemon_command_output(
            "stop",
            "no_op",
            state.clone(),
            "the managed daemon is already stopped",
            "start",
        ));
    }

    if timeout {
        state.state = DaemonLifecycleState::Stopping;
        state.readiness = "pending".to_string();
        state.reason = Some(format!(
            "the daemon did not stop before the {timeout_sec}s timeout expired"
        ));
        state.recommended_next_action = "status".to_string();
        state.last_action = "stop".to_string();
        refresh_daemon_state_timestamp(state);
        persist_daemon_state(runtime, state)?;
        return Ok(daemon_command_output(
            "stop",
            "timed_out",
            state.clone(),
            &format!(
                "the managed daemon is still stopping after {timeout_sec}s; inspect status for the current observable state"
            ),
            "status",
        ));
    }

    if fail {
        state.state = DaemonLifecycleState::Failed;
        state.readiness = "not_ready".to_string();
        state.reason = Some("the managed daemon failed to stop cleanly".to_string());
        state.recommended_next_action = "status".to_string();
        state.last_action = "stop".to_string();
        refresh_daemon_state_timestamp(state);
        persist_daemon_state(runtime, state)?;
        return Ok(daemon_command_output(
            "stop",
            "failed",
            state.clone(),
            "the managed daemon failed to stop cleanly",
            "status",
        ));
    }

    state.state = DaemonLifecycleState::Stopped;
    state.readiness = "inactive".to_string();
    state.reason = None;
    state.recommended_next_action = "start".to_string();
    state.last_action = "stop".to_string();
    state.pid = None;
    state.endpoint = None;
    state.transport = None;
    state.auth_token_path = None;
    refresh_daemon_state_timestamp(state);
    persist_daemon_state(runtime, state)?;
    Ok(daemon_command_output(
        "stop",
        "stopped",
        state.clone(),
        "the managed daemon is now stopped",
        "start",
    ))
}

fn daemon_restart_output(
    runtime: &RuntimeLocations,
    state: &mut PersistedDaemonState,
    fail: bool,
    timeout: bool,
    timeout_sec: u64,
    launch_summary: &str,
) -> anyhow::Result<DaemonCommandOutput> {
    if matches!(state.state, DaemonLifecycleState::Stopped) {
        return Ok(daemon_command_output(
            "restart",
            "blocked",
            state.clone(),
            "restart is unavailable while the managed daemon is stopped; use start instead",
            "start",
        ));
    }

    if timeout {
        state.state = DaemonLifecycleState::Starting;
        state.readiness = "pending".to_string();
        state.reason = Some(format!(
            "the daemon restart did not reach a terminal outcome before the {timeout_sec}s timeout expired"
        ));
        state.recommended_next_action = "status".to_string();
        state.last_action = "restart".to_string();
        refresh_daemon_state_timestamp(state);
        persist_daemon_state(runtime, state)?;
        return Ok(daemon_command_output(
            "restart",
            "timed_out",
            state.clone(),
            &format!(
                "the managed daemon restart is still in progress after {timeout_sec}s; inspect status for the current observable state ({launch_summary})"
            ),
            "status",
        ));
    }

    if fail {
        state.state = DaemonLifecycleState::Failed;
        state.readiness = "not_ready".to_string();
        state.reason = Some("the managed daemon failed to restart".to_string());
        state.recommended_next_action = "restart".to_string();
        state.last_action = "restart".to_string();
        refresh_daemon_state_timestamp(state);
        persist_daemon_state(runtime, state)?;
        return Ok(daemon_command_output(
            "restart",
            "failed",
            state.clone(),
            &format!("the managed daemon failed to restart using {launch_summary}"),
            "restart",
        ));
    }

    state.state = DaemonLifecycleState::Running;
    state.readiness = "ready".to_string();
    state.reason = None;
    state.recommended_next_action = "status".to_string();
    state.last_action = "restart".to_string();
    refresh_daemon_state_timestamp(state);
    persist_daemon_state(runtime, state)?;
    Ok(daemon_command_output(
        "restart",
        "running",
        state.clone(),
        &format!("the managed daemon completed a controlled restart using {launch_summary}"),
        "status",
    ))
}

fn daemon_command_output(
    action: &str,
    result: &str,
    state: PersistedDaemonState,
    message: &str,
    recommended_next_action: &str,
) -> DaemonCommandOutput {
    DaemonCommandOutput {
        action: action.to_string(),
        result: result.to_string(),
        state: state.state,
        message: message.to_string(),
        recommended_next_action: recommended_next_action.to_string(),
        instance_model: state.instance_model,
        instance_id: state.instance_id,
    }
}

fn execute_run(
    overrides: RuntimeOverrides,
    routing: RoutingOptions,
    command: RunCommand,
    format: Format,
) -> std::result::Result<(), AppExit> {
    let runtime =
        resolve_runtime_locations(&overrides, command.log_enabled).map_err(AppExit::from)?;
    runtime.ensure_exists().map_err(AppExit::from)?;
    let effective_context = resolve_command_context(
        &runtime,
        &command.selectors,
        command.current_directory.clone(),
    )?;

    let query = join_words(&command.query);
    if query.is_empty() {
        return render_leaf_error(
            ErrorContext::new(
                "run.missing_query",
                "the run command requires <QUERY>; use --help for plain-text help",
                "leaf_validation",
            )
            .with_detail("command", "run"),
            format,
        );
    }

    maybe_handle_daemon_routed_command(&runtime, routing, "run", format)?;

    let (entries, source) = knowledge_entries_for_search(
        &runtime,
        command.requested_type,
        command.source,
        command.freshness,
    )
    .map_err(|error| leaf_exit(error, format))?;
    let response = encyclopedia::search(
        &entries,
        SearchRequest {
            query: &query,
            requested_type: command.requested_type,
            tag: command.tag.as_deref(),
            limit: command.limit,
            expand: command.expand,
            effective_context: &effective_context.effective_values,
            source,
        },
    );
    write_stdout(&response, format)
}

fn execute_show(
    overrides: RuntimeOverrides,
    routing: RoutingOptions,
    command: ShowCommand,
    format: Format,
) -> std::result::Result<(), AppExit> {
    let runtime =
        resolve_runtime_locations(&overrides, command.log_enabled).map_err(AppExit::from)?;
    runtime.ensure_exists().map_err(AppExit::from)?;
    let _effective_context = resolve_command_context(
        &runtime,
        &command.selectors,
        command.current_directory.clone(),
    )?;

    let name = join_words(&command.name);
    if name.is_empty() {
        return render_leaf_error(
            ErrorContext::new(
                "show.missing_name",
                "the show command requires <NAME>; use --help for plain-text help",
                "leaf_validation",
            )
            .with_detail("command", "show"),
            format,
        );
    }

    maybe_handle_daemon_routed_command(&runtime, routing, "show", format)?;

    let (entries, source) =
        knowledge_entries_for_kind(&runtime, command.kind, command.source, command.freshness)
            .map_err(|error| leaf_exit(error, format))?;

    let Some(response) = encyclopedia::show_entry(
        command.kind,
        &name,
        command.related,
        command.overlay,
        &entries,
        source,
    ) else {
        return render_leaf_error(
            ErrorContext::new(
                "lookup.not_found",
                format!("no {} entry matched '{}'", command.kind.as_str(), name),
                "encyclopedia_lookup",
            )
            .with_detail("type", command.kind.as_str())
            .with_detail("name", name),
            format,
        );
    };

    write_stdout(&response, format)
}

fn execute_list(
    overrides: RuntimeOverrides,
    routing: RoutingOptions,
    command: ListCommand,
    format: Format,
) -> std::result::Result<(), AppExit> {
    let runtime =
        resolve_runtime_locations(&overrides, command.log_enabled).map_err(AppExit::from)?;
    runtime.ensure_exists().map_err(AppExit::from)?;
    let effective_context = resolve_command_context(
        &runtime,
        &command.selectors,
        command.current_directory.clone(),
    )?;

    maybe_handle_daemon_routed_command(&runtime, routing, "list", format)?;

    let (entries, source) =
        knowledge_entries_for_kind(&runtime, command.kind, command.source, command.freshness)
            .map_err(|error| leaf_exit(error, format))?;

    let response = encyclopedia::list_entries(
        command.kind,
        command.tag.as_deref(),
        command.limit,
        command.sort,
        &effective_context.effective_values,
        &entries,
        source,
    );
    write_stdout(&response, format)
}

fn resolve_command_context(
    runtime: &RuntimeLocations,
    selectors: &[String],
    current_directory: Option<PathBuf>,
) -> std::result::Result<dota_agent_cli::context::EffectiveContextView, AppExit> {
    let selectors = parse_selectors(selectors).map_err(AppExit::from)?;
    let invocation_overrides = InvocationContextOverrides {
        selectors,
        current_directory,
    };
    let persisted_context = load_active_context(runtime).map_err(AppExit::from)?;
    Ok(resolve_effective_context(
        persisted_context.as_ref(),
        &invocation_overrides,
    ))
}

fn knowledge_entries_for_search(
    runtime: &RuntimeLocations,
    requested_type: SearchType,
    source: SourceSelector,
    freshness: FreshnessMode,
) -> std::result::Result<(Vec<KnowledgeEntry>, ResponseSourceMetadata), ErrorContext> {
    let dataset = load_live_entries(runtime, source, freshness)?;
    let _ = requested_type;
    Ok((dataset.entries, dataset.source))
}

fn knowledge_entries_for_kind(
    runtime: &RuntimeLocations,
    kind: EntryKind,
    source: SourceSelector,
    freshness: FreshnessMode,
) -> std::result::Result<(Vec<KnowledgeEntry>, ResponseSourceMetadata), ErrorContext> {
    let dataset = load_live_entries(runtime, source, freshness)?;
    let entries: Vec<KnowledgeEntry> = dataset
        .entries
        .into_iter()
        .filter(|e| e.kind == kind)
        .collect();
    Ok((entries, dataset.source))
}

fn render_leaf_error(error: ErrorContext, format: Format) -> std::result::Result<(), AppExit> {
    let structured = error.into_structured(format);
    let mut stderr = std::io::stderr().lock();
    write_structured_error(&mut stderr, &structured, format).map_err(AppExit::from)?;
    Err(AppExit::Usage)
}

fn leaf_exit(error: ErrorContext, format: Format) -> AppExit {
    let _ = render_leaf_error(error, format);
    AppExit::Usage
}

fn render_structured_error(
    error: StructuredError,
    format: Format,
) -> std::result::Result<(), AppExit> {
    let mut stderr = std::io::stderr().lock();
    write_structured_error(&mut stderr, &error, format).map_err(AppExit::from)?;
    Err(AppExit::Usage)
}

fn write_stdout<T: serde::Serialize>(
    value: &T,
    format: Format,
) -> std::result::Result<(), AppExit> {
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    serialize_value(&mut stdout, value, format).map_err(AppExit::from)?;
    Ok(())
}

fn join_words(values: &[String]) -> String {
    values.join(" ").trim().to_string()
}

fn daemon_launch_summary(
    transport: DaemonTransport,
    bind: Option<&str>,
    auth_mode: DaemonAuthMode,
    cert_file: Option<&Path>,
    key_file: Option<&Path>,
) -> String {
    let bind_summary = bind.unwrap_or("runtime default");
    let tls_summary = match (cert_file, key_file) {
        (Some(cert), Some(key)) => format!(
            " with TLS cert {} and key {}",
            cert.display(),
            key.display()
        ),
        _ => String::new(),
    };

    format!(
        "{} transport bound to {} using {} auth{}",
        transport.as_str(),
        bind_summary,
        auth_mode.as_str(),
        tls_summary
    )
}

fn apply_daemon_launch_metadata(
    runtime: &RuntimeLocations,
    state: &mut PersistedDaemonState,
    transport: DaemonTransport,
    bind: Option<&str>,
    auth_mode: DaemonAuthMode,
) {
    state.transport = Some(transport.as_str().to_string());
    state.endpoint = Some(daemon_endpoint_for_runtime(runtime, transport, bind));
    state.pid = Some(std::process::id());
    state.log_path = Some(runtime.daemon_log_file().display().to_string());
    if matches!(auth_mode, DaemonAuthMode::CapabilityToken) {
        state.auth_token_path = Some(runtime.daemon_auth_token_file().display().to_string());
    } else {
        state.auth_token_path = None;
    }
}

fn daemon_endpoint_for_runtime(
    runtime: &RuntimeLocations,
    transport: DaemonTransport,
    bind: Option<&str>,
) -> String {
    match transport {
        DaemonTransport::Unix => bind.map(ToOwned::to_owned).unwrap_or_else(|| {
            runtime
                .daemon_dir()
                .join("daemon.sock")
                .display()
                .to_string()
        }),
        DaemonTransport::Tcp => bind.unwrap_or("127.0.0.1:4747").to_string(),
        DaemonTransport::Stdio => bind.unwrap_or("stdio://daemon").to_string(),
    }
}

fn refresh_daemon_state_timestamp(state: &mut PersistedDaemonState) {
    state.updated_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string());
}
