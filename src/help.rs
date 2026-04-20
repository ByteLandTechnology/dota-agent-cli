use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct HelpOption {
    pub name: String,
    pub value_name: String,
    pub default_value: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HelpSubcommand {
    pub name: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HelpExample {
    pub command: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExitCodeSpec {
    pub code: i32,
    pub meaning: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeDirectoryHelp {
    pub config: String,
    pub data: String,
    pub state: String,
    pub cache: String,
    pub logs: String,
    pub scope: String,
    pub overrides: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActiveContextHelp {
    pub persisted_values: Vec<String>,
    pub ambient_cues: Vec<String>,
    pub inspection_command: String,
    pub switch_command: String,
    pub precedence_rule: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FeatureAvailability {
    pub streaming: String,
    pub repl: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HelpDocument {
    pub command_path: Vec<String>,
    pub purpose: String,
    pub usage: String,
    pub arguments: Vec<String>,
    pub options: Vec<HelpOption>,
    pub subcommands: Vec<HelpSubcommand>,
    pub output_formats: Vec<String>,
    pub exit_behavior: Vec<ExitCodeSpec>,
    pub runtime_directories: RuntimeDirectoryHelp,
    pub active_context: ActiveContextHelp,
    pub feature_availability: FeatureAvailability,
    #[serde(skip_serializing)]
    pub description: Vec<String>,
    #[serde(skip_serializing)]
    pub examples: Vec<HelpExample>,
}

fn runtime_directory_help() -> RuntimeDirectoryHelp {
    RuntimeDirectoryHelp {
        config: "User-authored configuration (user-scoped by default)".to_string(),
        data: "Durable skill data and user-managed outputs".to_string(),
        state:
            "Recoverable runtime state, Active Context persistence, and daemon artifacts such as pid, bind, status, and auth metadata"
                .to_string(),
        cache: "Disposable provider caches and derived lookup indexes".to_string(),
        logs:
            "Optional daemon and invocation logs beneath state when logging is enabled".to_string(),
        scope: "user_scoped_default".to_string(),
        overrides: vec![
            "--config-dir".to_string(),
            "--data-dir".to_string(),
            "--state-dir".to_string(),
            "--cache-dir".to_string(),
            "--log-dir".to_string(),
        ],
    }
}

fn active_context_help() -> ActiveContextHelp {
    ActiveContextHelp {
        persisted_values: vec![
            "named profile label".to_string(),
            "selector key/value pairs such as role=support or lane=mid".to_string(),
        ],
        ambient_cues: vec!["current_directory".to_string()],
        inspection_command: "dota-agent-cli context show".to_string(),
        switch_command: "dota-agent-cli context use --selector role=support".to_string(),
        precedence_rule:
            "explicit invocation values override the persisted Active Context for one invocation only"
                .to_string(),
    }
}

fn feature_availability() -> FeatureAvailability {
    FeatureAvailability {
        streaming: "out_of_scope".to_string(),
        repl: "enabled".to_string(),
    }
}

fn daemon_routing_options() -> Vec<HelpOption> {
    vec![
        HelpOption {
            name: "--via".to_string(),
            value_name: "local|daemon".to_string(),
            default_value: "local".to_string(),
            description:
                "Select inline execution or managed daemon routing for daemonizable commands"
                    .to_string(),
        },
        HelpOption {
            name: "--ensure-daemon".to_string(),
            value_name: "-".to_string(),
            default_value: "false".to_string(),
            description: "Start or reuse the managed daemon before daemon-routed execution"
                .to_string(),
        },
    ]
}

fn daemon_routing_description() -> String {
    "Daemonizable commands honor global --via local|daemon routing, and --ensure-daemon can start or reuse the managed daemon before a daemon-routed invocation.".to_string()
}

fn local_only_routing_description() -> String {
    "Local-only control surfaces reject daemon routing flags with a structured routing error even when those flags are parsed globally.".to_string()
}

fn shared_doc(command_path: Vec<String>, purpose: &str, usage: &str) -> HelpDocument {
    HelpDocument {
        command_path,
        purpose: purpose.to_string(),
        usage: usage.to_string(),
        arguments: Vec::new(),
        options: Vec::new(),
        subcommands: Vec::new(),
        output_formats: vec!["yaml".to_string(), "json".to_string(), "toml".to_string()],
        exit_behavior: vec![
            ExitCodeSpec {
                code: 0,
                meaning: "Success or plain-text help".to_string(),
            },
            ExitCodeSpec {
                code: 2,
                meaning: "Structured usage, validation, or provider error".to_string(),
            },
        ],
        runtime_directories: runtime_directory_help(),
        active_context: active_context_help(),
        feature_availability: feature_availability(),
        description: Vec::new(),
        examples: Vec::new(),
    }
}

fn top_level_help() -> HelpDocument {
    let mut doc = shared_doc(
        Vec::new(),
        "Live provider-backed Dota 2 knowledge and live-match CLI for agent-friendly hero, item, match state, and source lookups",
        "dota-agent-cli [OPTIONS] <COMMAND>",
    );
    doc.options = vec![
        HelpOption {
            name: "--via".to_string(),
            value_name: "local|daemon".to_string(),
            default_value: "local".to_string(),
            description:
                "Select inline execution or managed daemon routing for daemonizable commands"
                    .to_string(),
        },
        HelpOption {
            name: "--ensure-daemon".to_string(),
            value_name: "-".to_string(),
            default_value: "false".to_string(),
            description: "Start or reuse the managed daemon before daemon-routed execution"
                .to_string(),
        },
        HelpOption {
            name: "--format".to_string(),
            value_name: "yaml|json|toml".to_string(),
            default_value: "yaml".to_string(),
            description:
                "Select the structured output format for one-shot commands and structured help"
                    .to_string(),
        },
        HelpOption {
            name: "--config-dir".to_string(),
            value_name: "PATH".to_string(),
            default_value: "platform default".to_string(),
            description: "Override the default configuration directory".to_string(),
        },
        HelpOption {
            name: "--data-dir".to_string(),
            value_name: "PATH".to_string(),
            default_value: "platform default".to_string(),
            description: "Override the default durable data directory".to_string(),
        },
        HelpOption {
            name: "--state-dir".to_string(),
            value_name: "PATH".to_string(),
            default_value: "derived from data".to_string(),
            description: "Override the runtime state directory".to_string(),
        },
        HelpOption {
            name: "--cache-dir".to_string(),
            value_name: "PATH".to_string(),
            default_value: "platform default".to_string(),
            description: "Override the cache directory".to_string(),
        },
        HelpOption {
            name: "--log-dir".to_string(),
            value_name: "PATH".to_string(),
            default_value: "state/logs when enabled".to_string(),
            description: "Override the optional log directory".to_string(),
        },
        HelpOption {
            name: "--repl".to_string(),
            value_name: "-".to_string(),
            default_value: "false".to_string(),
            description: "Start an interactive REPL session".to_string(),
        },
        HelpOption {
            name: "--help".to_string(),
            value_name: "-".to_string(),
            default_value: "false".to_string(),
            description: "Render plain-text help for the selected command path".to_string(),
        },
    ];
    doc.subcommands = vec![
        HelpSubcommand {
            name: "run".to_string(),
            summary: "Search across live hero and item data".to_string(),
        },
        HelpSubcommand {
            name: "show".to_string(),
            summary: "Return one detailed encyclopedia entry with live overlays when available"
                .to_string(),
        },
        HelpSubcommand {
            name: "list".to_string(),
            summary: "List encyclopedia entries by type with provider-aware sorting".to_string(),
        },
        HelpSubcommand {
            name: "source".to_string(),
            summary: "Inspect provider availability and warm provider caches".to_string(),
        },
        HelpSubcommand {
            name: "match".to_string(),
            summary: "Query live, recent, and historical match data backed by live providers"
                .to_string(),
        },
        HelpSubcommand {
            name: "daemon".to_string(),
            summary: "Control the managed background cache-warmer and provider-proxy daemon"
                .to_string(),
        },
        HelpSubcommand {
            name: "paths".to_string(),
            summary: "Inspect runtime directory defaults and overrides".to_string(),
        },
        HelpSubcommand {
            name: "context".to_string(),
            summary: "Inspect or persist the Active Context".to_string(),
        },
        HelpSubcommand {
            name: "help".to_string(),
            summary: "Return machine-readable help for a command path".to_string(),
        },
        HelpSubcommand {
            name: "repl".to_string(),
            summary: "Start an interactive terminal session (--repl)".to_string(),
        },
    ];
    doc.description = vec![
        "dota-agent-cli is a live provider-backed Dota 2 knowledge and live-match CLI designed for agent workflows."
            .to_string(),
        "OpenDota is the default public source for hero and item encyclopedia surfaces."
            .to_string(),
        "Provider caches keep repeated lookups fast and explainable without hiding freshness or source-routing decisions."
            .to_string(),
        daemon_routing_description(),
        "Managed daemon state lives under the state root so routed invocations can reuse transport, status, and auth artifacts across calls."
            .to_string(),
    ];
    doc.examples = vec![
        HelpExample {
            command: "dota-agent-cli run axe --type hero --source auto --freshness live"
                .to_string(),
            description: "Search live hero data and bypass cache".to_string(),
        },
        HelpExample {
            command: "dota-agent-cli show hero Axe --overlay stats".to_string(),
            description: "Inspect one hero with provider-backed overlay details".to_string(),
        },
        HelpExample {
            command: "dota-agent-cli source warm --scope indexes".to_string(),
            description: "Refresh provider-backed lookup indexes into cache".to_string(),
        },
    ];
    doc
}

fn run_help() -> HelpDocument {
    let mut doc = shared_doc(
        vec!["run".to_string()],
        "Search the encyclopedia with a natural query",
        "dota-agent-cli run [OPTIONS] <QUERY ...>",
    );
    doc.arguments =
        vec!["QUERY: one or more search terms describing the desired hero or item".to_string()];
    doc.options = vec![
        HelpOption {
            name: "--type".to_string(),
            value_name: "all|hero|item".to_string(),
            default_value: "all".to_string(),
            description: "Restrict matches to one knowledge class".to_string(),
        },
        HelpOption {
            name: "--source".to_string(),
            value_name: "auto|opendota|stratz|cache-only".to_string(),
            default_value: "auto".to_string(),
            description: "Select the provider routing mode for this invocation".to_string(),
        },
        HelpOption {
            name: "--freshness".to_string(),
            value_name: "live|recent|cached-ok".to_string(),
            default_value: "recent".to_string(),
            description: "Control whether the command must fetch live data or may reuse cache"
                .to_string(),
        },
        HelpOption {
            name: "--limit".to_string(),
            value_name: "INT".to_string(),
            default_value: "5".to_string(),
            description: "Maximum number of results to return".to_string(),
        },
        HelpOption {
            name: "--expand".to_string(),
            value_name: "-".to_string(),
            default_value: "false".to_string(),
            description: "Include longer detail bullets and overlay snippets in results"
                .to_string(),
        },
        HelpOption {
            name: "--tag".to_string(),
            value_name: "TAG".to_string(),
            default_value: "none".to_string(),
            description: "Additional thematic filter such as carry, support, initiation, or vision"
                .to_string(),
        },
        HelpOption {
            name: "--selector".to_string(),
            value_name: "KEY=VALUE".to_string(),
            default_value: "none".to_string(),
            description: "Apply an explicit per-invocation Active Context selector".to_string(),
        },
        HelpOption {
            name: "--cwd".to_string(),
            value_name: "PATH".to_string(),
            default_value: "none".to_string(),
            description: "Apply an explicit current-directory ambient cue".to_string(),
        },
        HelpOption {
            name: "--log-enabled".to_string(),
            value_name: "-".to_string(),
            default_value: "false".to_string(),
            description: "Expose the optional log directory for this invocation".to_string(),
        },
    ];
    doc.options.extend(daemon_routing_options());
    doc.description = vec![
        "run performs fuzzy lookup across provider-backed hero and item indexes.".to_string(),
        "Use --freshness live when a current snapshot matters more than cache reuse, and --source cache-only when the invocation must stay offline after caches are warmed."
            .to_string(),
        "Context selectors such as role=support or lane=mid bias ranking toward entries whose tags align with the effective context."
            .to_string(),
        daemon_routing_description(),
    ];
    doc.examples = vec![
        HelpExample {
            command: "dota-agent-cli run blink initiation --type item --freshness recent"
                .to_string(),
            description: "Search provider-backed item data with normal cache reuse".to_string(),
        },
        HelpExample {
            command: "dota-agent-cli run observer vision --type item --expand".to_string(),
            description: "Inspect an item query with extra detail bullets".to_string(),
        },
    ];
    doc
}

fn show_help() -> HelpDocument {
    let mut doc = shared_doc(
        vec!["show".to_string()],
        "Return a detailed encyclopedia entry",
        "dota-agent-cli show <hero|item> <NAME ...> [OPTIONS]",
    );
    doc.arguments = vec![
        "TYPE: encyclopedia category (hero or item)".to_string(),
        "NAME: one or more words that identify the desired entry".to_string(),
    ];
    doc.options = vec![
        HelpOption {
            name: "--source".to_string(),
            value_name: "auto|opendota|stratz|cache-only".to_string(),
            default_value: "auto".to_string(),
            description: "Select the provider routing mode for this invocation".to_string(),
        },
        HelpOption {
            name: "--freshness".to_string(),
            value_name: "live|recent|cached-ok".to_string(),
            default_value: "recent".to_string(),
            description: "Control whether the command must fetch live data or may reuse cache"
                .to_string(),
        },
        HelpOption {
            name: "--overlay".to_string(),
            value_name: "basic|stats|full".to_string(),
            default_value: "stats".to_string(),
            description:
                "Choose whether to return base records only or include supported live overlays"
                    .to_string(),
        },
        HelpOption {
            name: "--related".to_string(),
            value_name: "-".to_string(),
            default_value: "false".to_string(),
            description: "Expand related entries inline when they exist".to_string(),
        },
        HelpOption {
            name: "--selector".to_string(),
            value_name: "KEY=VALUE".to_string(),
            default_value: "none".to_string(),
            description: "Apply an explicit per-invocation Active Context selector".to_string(),
        },
        HelpOption {
            name: "--cwd".to_string(),
            value_name: "PATH".to_string(),
            default_value: "none".to_string(),
            description: "Apply an explicit current-directory ambient cue".to_string(),
        },
        HelpOption {
            name: "--log-enabled".to_string(),
            value_name: "-".to_string(),
            default_value: "false".to_string(),
            description: "Expose the optional log directory for this invocation".to_string(),
        },
    ];
    doc.options.extend(daemon_routing_options());
    doc.description = vec![
        "show is the deterministic lookup path when the caller already knows the entity category."
            .to_string(),
        "Hero and item responses can include provider-backed overlays.".to_string(),
        "The current revision keeps STRATZ as an optional future enhancement path and resolves available encyclopedia hero/item surfaces through OpenDota."
            .to_string(),
        daemon_routing_description(),
    ];
    doc.examples = vec![
        HelpExample {
            command: "dota-agent-cli show hero Axe --overlay stats --related".to_string(),
            description: "Inspect one hero and include live overlay plus related suggestions"
                .to_string(),
        },
        HelpExample {
            command: "dota-agent-cli show item Blink Dagger --overlay full".to_string(),
            description: "Inspect one item with a fuller overlay".to_string(),
        },
    ];
    doc
}

fn list_help() -> HelpDocument {
    let mut doc = shared_doc(
        vec!["list".to_string()],
        "List encyclopedia entries by category",
        "dota-agent-cli list <hero|item> [OPTIONS]",
    );
    doc.arguments = vec!["TYPE: encyclopedia category to enumerate".to_string()];
    doc.options = vec![
        HelpOption {
            name: "--source".to_string(),
            value_name: "auto|opendota|stratz|cache-only".to_string(),
            default_value: "auto".to_string(),
            description: "Select the provider routing mode for this invocation".to_string(),
        },
        HelpOption {
            name: "--freshness".to_string(),
            value_name: "live|recent|cached-ok".to_string(),
            default_value: "recent".to_string(),
            description: "Control whether the command must fetch live data or may reuse cache"
                .to_string(),
        },
        HelpOption {
            name: "--tag".to_string(),
            value_name: "TAG".to_string(),
            default_value: "none".to_string(),
            description: "Filter returned entries by tag".to_string(),
        },
        HelpOption {
            name: "--sort".to_string(),
            value_name: "name|popularity|winrate|updated".to_string(),
            default_value: "name".to_string(),
            description: "Sort order for returned entries".to_string(),
        },
        HelpOption {
            name: "--limit".to_string(),
            value_name: "INT".to_string(),
            default_value: "20".to_string(),
            description: "Maximum number of entries to return".to_string(),
        },
        HelpOption {
            name: "--selector".to_string(),
            value_name: "KEY=VALUE".to_string(),
            default_value: "none".to_string(),
            description: "Apply an explicit per-invocation Active Context selector".to_string(),
        },
        HelpOption {
            name: "--cwd".to_string(),
            value_name: "PATH".to_string(),
            default_value: "none".to_string(),
            description: "Apply an explicit current-directory ambient cue".to_string(),
        },
        HelpOption {
            name: "--log-enabled".to_string(),
            value_name: "-".to_string(),
            default_value: "false".to_string(),
            description: "Expose the optional log directory for this invocation".to_string(),
        },
    ];
    doc.options.extend(daemon_routing_options());
    doc.description = vec![
        "list is useful when an agent wants to discover heroes or items before exact lookup."
            .to_string(),
        "Popularity and winrate sorting are meaningful for provider-backed hero and item surfaces."
            .to_string(),
        daemon_routing_description(),
    ];
    doc.examples = vec![
        HelpExample {
            command: "dota-agent-cli list hero --sort popularity --limit 10".to_string(),
            description: "List popular heroes from the current provider snapshot".to_string(),
        },
        HelpExample {
            command: "dota-agent-cli list item --tag vision".to_string(),
            description: "List items related to vision and information control".to_string(),
        },
    ];
    doc
}

fn source_help() -> HelpDocument {
    let mut doc = shared_doc(
        vec!["source".to_string()],
        "Inspect provider availability and warm live caches",
        "dota-agent-cli source <status|warm> [OPTIONS]",
    );
    doc.subcommands = vec![
        HelpSubcommand {
            name: "status".to_string(),
            summary: "Report configured providers, auth availability, cache age, and reachability"
                .to_string(),
        },
        HelpSubcommand {
            name: "warm".to_string(),
            summary: "Refresh provider indexes and metadata into cache".to_string(),
        },
    ];
    doc.description = vec![
        "source separates provider operations from the public encyclopedia contract."
            .to_string(),
        "Use it when an agent needs to inspect routing state or prefetch caches before offline-friendly lookups."
            .to_string(),
        daemon_routing_description(),
    ];
    doc.examples = vec![
        HelpExample {
            command: "dota-agent-cli source status --freshness recent".to_string(),
            description: "Inspect provider reachability and cache freshness".to_string(),
        },
        HelpExample {
            command: "dota-agent-cli source warm --scope all --force".to_string(),
            description: "Force-refresh all supported cache layers".to_string(),
        },
    ];
    doc
}

fn source_status_help() -> HelpDocument {
    let mut doc = shared_doc(
        vec!["source".to_string(), "status".to_string()],
        "Report configured providers, auth availability, cache age, and reachability",
        "dota-agent-cli source status [OPTIONS]",
    );
    doc.options = vec![
        HelpOption {
            name: "--source".to_string(),
            value_name: "auto|opendota|stratz".to_string(),
            default_value: "auto".to_string(),
            description: "Limit status checks to one provider or use auto-detection".to_string(),
        },
        HelpOption {
            name: "--freshness".to_string(),
            value_name: "live|recent|cached-ok".to_string(),
            default_value: "recent".to_string(),
            description: "Control whether reachability must be checked live or may use cache"
                .to_string(),
        },
    ];
    doc.options.extend(daemon_routing_options());
    doc.description = vec![
        "status is the operator-facing surface for provider routing state.".to_string(),
        "OpenDota checks can use cache-aware probes, while STRATZ status currently focuses on auth/configuration readiness."
            .to_string(),
        daemon_routing_description(),
    ];
    doc.examples = vec![HelpExample {
        command: "dota-agent-cli source status --source opendota --freshness live".to_string(),
        description: "Force a live reachability check for the default public provider".to_string(),
    }];
    doc
}

fn source_warm_help() -> HelpDocument {
    let mut doc = shared_doc(
        vec!["source".to_string(), "warm".to_string()],
        "Refresh hero and item indexes plus provider metadata into cache",
        "dota-agent-cli source warm [OPTIONS]",
    );
    doc.options = vec![
        HelpOption {
            name: "--source".to_string(),
            value_name: "auto|opendota|stratz".to_string(),
            default_value: "auto".to_string(),
            description: "Select which provider or provider set to warm".to_string(),
        },
        HelpOption {
            name: "--scope".to_string(),
            value_name: "indexes|details|all".to_string(),
            default_value: "indexes".to_string(),
            description:
                "Choose whether to warm only indexes, details, or all supported cache layers"
                    .to_string(),
        },
        HelpOption {
            name: "--force".to_string(),
            value_name: "-".to_string(),
            default_value: "false".to_string(),
            description: "Ignore TTLs and refresh cache entries immediately".to_string(),
        },
    ];
    doc.options.extend(daemon_routing_options());
    doc.description = vec![
        "warm prefetches provider-backed data into the cache root so later invocations can reuse it."
            .to_string(),
        "The current revision shares one hero/item index layer across search and detail lookups."
            .to_string(),
        daemon_routing_description(),
    ];
    doc.examples = vec![HelpExample {
        command: "dota-agent-cli source warm --source opendota --scope indexes".to_string(),
        description: "Refresh the OpenDota lookup indexes used by run/show/list".to_string(),
    }];
    doc
}

fn help_subcommand_help() -> HelpDocument {
    let mut doc = shared_doc(
        vec!["help".to_string()],
        "Return machine-readable help for a command path",
        "dota-agent-cli help [COMMAND_PATH ...] [--format yaml|json|toml]",
    );
    doc.arguments = vec![
        "COMMAND_PATH: optional command path such as run, source status, or daemon start"
            .to_string(),
    ];
    doc.description = vec![
        "Use the help subcommand when an agent needs machine-readable command metadata."
            .to_string(),
        "Plain-text help still belongs exclusively to --help.".to_string(),
        local_only_routing_description(),
    ];
    doc.examples = vec![
        HelpExample {
            command: "dota-agent-cli help run --format yaml".to_string(),
            description: "Inspect run as structured YAML".to_string(),
        },
        HelpExample {
            command: "dota-agent-cli help source warm --format json".to_string(),
            description: "Inspect provider warming semantics as structured JSON".to_string(),
        },
    ];
    doc
}

fn paths_help() -> HelpDocument {
    let mut doc = shared_doc(
        vec!["paths".to_string()],
        "Inspect runtime directory defaults and explicit overrides",
        "dota-agent-cli paths [OPTIONS]",
    );
    doc.options = vec![HelpOption {
        name: "--log-enabled".to_string(),
        value_name: "-".to_string(),
        default_value: "false".to_string(),
        description: "Include the optional log directory in the output".to_string(),
    }];
    doc.description = vec![
        "paths documents config, data, state, cache, and optional log locations separately."
            .to_string(),
        "User-scoped defaults remain the baseline unless explicit overrides are provided."
            .to_string(),
        "The state root is where daemon routing persists reusable pid, bind, readiness, and auth artifacts when managed execution is enabled."
            .to_string(),
        local_only_routing_description(),
    ];
    doc.examples = vec![
        HelpExample {
            command: "dota-agent-cli paths".to_string(),
            description: "Inspect default runtime directories".to_string(),
        },
        HelpExample {
            command: "dota-agent-cli paths --log-enabled".to_string(),
            description: "Inspect the optional log directory as well".to_string(),
        },
    ];
    doc
}

fn daemon_help() -> HelpDocument {
    let mut doc = shared_doc(
        vec!["daemon".to_string()],
        "Control the managed background daemon lifecycle",
        "dota-agent-cli daemon <start|stop|restart|status>",
    );
    doc.subcommands = vec![
        HelpSubcommand {
            name: "start".to_string(),
            summary: "Start the managed background daemon and wait for a terminal outcome"
                .to_string(),
        },
        HelpSubcommand {
            name: "stop".to_string(),
            summary: "Stop the managed background daemon and wait for a terminal outcome"
                .to_string(),
        },
        HelpSubcommand {
            name: "restart".to_string(),
            summary: "Restart the managed background daemon and wait for a terminal outcome"
                .to_string(),
        },
        HelpSubcommand {
            name: "status".to_string(),
            summary: "Inspect the current daemon lifecycle state and next action".to_string(),
        },
        HelpSubcommand {
            name: "run".to_string(),
            summary: "Internal daemon process entrypoint for managed launch only".to_string(),
        },
    ];
    doc.description = vec![
        "The shared daemon contract covers only managed background daemon mode.".to_string(),
        "The public control surface remains daemon <start|stop|restart|status> under a single managed instance model."
            .to_string(),
        "Attached foreground execution is out of scope, while daemon run stays reserved for internal managed launch."
            .to_string(),
        local_only_routing_description(),
    ];
    doc.examples = vec![
        HelpExample {
            command: "dota-agent-cli daemon start --transport unix".to_string(),
            description: "Start the managed daemon with the default transport".to_string(),
        },
        HelpExample {
            command: "dota-agent-cli daemon status --format json".to_string(),
            description: "Inspect readiness, state, and the next action".to_string(),
        },
    ];
    doc
}

fn daemon_start_help() -> HelpDocument {
    let mut doc = shared_doc(
        vec!["daemon".to_string(), "start".to_string()],
        "Start the managed background daemon",
        "dota-agent-cli daemon start [OPTIONS]",
    );
    doc.options = daemon_launch_options("30");
    doc.description = vec![
        "start returns only after the daemon reaches running, failed, timed out, or a no-op state."
            .to_string(),
        "The daemon's primary role is cache warming and reusable provider proxy work for agent-driven invocations."
            .to_string(),
        "Startup materializes runtime artifacts under the state root, including transport binding details, readiness state, and authentication metadata."
            .to_string(),
    ];
    doc.examples = vec![HelpExample {
        command: "dota-agent-cli daemon start --transport tcp --bind 127.0.0.1:8787".to_string(),
        description: "Start the managed daemon on a TCP listener".to_string(),
    }];
    doc
}

fn daemon_stop_help() -> HelpDocument {
    let mut doc = shared_doc(
        vec!["daemon".to_string(), "stop".to_string()],
        "Stop the managed background daemon",
        "dota-agent-cli daemon stop [OPTIONS]",
    );
    doc.options = vec![HelpOption {
        name: "--timeout-sec".to_string(),
        value_name: "INT".to_string(),
        default_value: "30".to_string(),
        description: "Maximum seconds to wait for stopped, failed, or timeout state".to_string(),
    }];
    doc.description = vec![
        "stop remains the standardized recovery path when the daemon must be shut down cleanly."
            .to_string(),
        "Stopping leaves behind the latest status artifact so later inspections can explain the last managed transition."
            .to_string(),
    ];
    doc.examples = vec![HelpExample {
        command: "dota-agent-cli daemon stop --timeout-sec 10".to_string(),
        description: "Stop the managed daemon with a shorter wait budget".to_string(),
    }];
    doc
}

fn daemon_restart_help() -> HelpDocument {
    let mut doc = shared_doc(
        vec!["daemon".to_string(), "restart".to_string()],
        "Restart the managed background daemon",
        "dota-agent-cli daemon restart [OPTIONS]",
    );
    doc.options = daemon_launch_options("30");
    doc.description = vec![
        "restart stays inside the shared four-command daemon contract.".to_string(),
        "Use restart when the daemon is already running but needs a controlled transport or auth-mode refresh."
            .to_string(),
        "Restart rewrites the same managed runtime artifacts beneath the state root after the new instance becomes ready."
            .to_string(),
    ];
    doc.examples = vec![HelpExample {
        command: "dota-agent-cli daemon restart --transport unix".to_string(),
        description: "Perform a controlled daemon restart with the default transport".to_string(),
    }];
    doc
}

fn daemon_status_help() -> HelpDocument {
    let mut doc = shared_doc(
        vec!["daemon".to_string(), "status".to_string()],
        "Inspect the current managed daemon state",
        "dota-agent-cli daemon status",
    );
    doc.description = vec![
        "status is the authoritative inspection surface after daemon failures or explicit timeouts."
            .to_string(),
        "It reports the managed runtime state derived from persisted daemon artifacts under the state directory."
            .to_string(),
    ];
    doc.examples = vec![HelpExample {
        command: "dota-agent-cli daemon status --format json".to_string(),
        description: "Inspect the current daemon state as structured JSON".to_string(),
    }];
    doc
}

fn daemon_run_help() -> HelpDocument {
    let mut doc = shared_doc(
        vec!["daemon".to_string(), "run".to_string()],
        "Internal daemon process entrypoint used for managed background launch only",
        "dota-agent-cli daemon run [OPTIONS]",
    );
    doc.options = daemon_run_options();
    doc.description = vec![
        "run is not the recommended human-facing control surface.".to_string(),
        "It exists so managed launchers can activate the cache-warmer/provider-proxy process without bypassing the shared daemon contract."
            .to_string(),
        "Managed launch writes the same transport, readiness, and auth artifacts inspected later by daemon status and daemon-routed commands."
            .to_string(),
    ];
    doc.examples = vec![HelpExample {
        command: "dota-agent-cli daemon run --transport unix".to_string(),
        description: "Internal managed launch using the default transport".to_string(),
    }];
    doc
}

fn context_help() -> HelpDocument {
    let mut doc = shared_doc(
        vec!["context".to_string()],
        "Inspect or persist the Active Context",
        "dota-agent-cli context <COMMAND>",
    );
    doc.subcommands = vec![
        HelpSubcommand {
            name: "show".to_string(),
            summary: "Display the current persisted and effective context".to_string(),
        },
        HelpSubcommand {
            name: "use".to_string(),
            summary: "Persist selectors and ambient cues as the Active Context".to_string(),
        },
    ];
    doc.description = vec![
        "The Active Context is inspectable, persisted under state, and overridden explicitly per invocation."
            .to_string(),
        local_only_routing_description(),
    ];
    doc.examples = vec![
        HelpExample {
            command: "dota-agent-cli context show".to_string(),
            description: "Inspect the current effective context".to_string(),
        },
        HelpExample {
            command: "dota-agent-cli context use --selector role=support".to_string(),
            description: "Persist a selector for future invocations".to_string(),
        },
    ];
    doc
}

fn context_show_help() -> HelpDocument {
    let mut doc = shared_doc(
        vec!["context".to_string(), "show".to_string()],
        "Display the current Active Context",
        "dota-agent-cli context show",
    );
    doc.description = vec![
        "context show reveals both the persisted state and the effective values in use."
            .to_string(),
        local_only_routing_description(),
    ];
    doc.examples = vec![HelpExample {
        command: "dota-agent-cli context show".to_string(),
        description: "Display the current Active Context".to_string(),
    }];
    doc
}

fn context_use_help() -> HelpDocument {
    let mut doc = shared_doc(
        vec!["context".to_string(), "use".to_string()],
        "Persist selectors and ambient cues as the Active Context",
        "dota-agent-cli context use [OPTIONS]",
    );
    doc.options = vec![
        HelpOption {
            name: "--name".to_string(),
            value_name: "NAME".to_string(),
            default_value: "none".to_string(),
            description: "Optional label for the persisted context".to_string(),
        },
        HelpOption {
            name: "--selector".to_string(),
            value_name: "KEY=VALUE".to_string(),
            default_value: "none".to_string(),
            description: "Persist one selector in the Active Context".to_string(),
        },
        HelpOption {
            name: "--cwd".to_string(),
            value_name: "PATH".to_string(),
            default_value: "none".to_string(),
            description: "Persist an ambient current-directory cue".to_string(),
        },
    ];
    doc.description = vec![
        "Persist reusable selectors or ambient cues for future encyclopedia lookups.".to_string(),
        local_only_routing_description(),
    ];
    doc.examples = vec![
        HelpExample {
            command: "dota-agent-cli context use --selector role=support --selector lane=offlane"
                .to_string(),
            description: "Persist multiple selectors together".to_string(),
        },
        HelpExample {
            command: "dota-agent-cli context use --cwd /tmp/replay-analysis".to_string(),
            description: "Persist a current-directory cue".to_string(),
        },
    ];
    doc
}

fn match_help() -> HelpDocument {
    let mut doc = shared_doc(
        vec!["match".to_string()],
        "Query live, recent, and historical match data",
        "dota-agent-cli match <live|show|recent> [OPTIONS]",
    );
    doc.subcommands = vec![
        HelpSubcommand {
            name: "live".to_string(),
            summary: "List currently in-progress matches from live provider feeds".to_string(),
        },
        HelpSubcommand {
            name: "show".to_string(),
            summary: "Show detailed data for one match by match ID".to_string(),
        },
        HelpSubcommand {
            name: "recent".to_string(),
            summary: "List recent matches filtered by player or hero".to_string(),
        },
    ];
    doc.description = vec![
        "The match tree provides live match tracking, match detail lookups, and player session queries backed by live providers."
            .to_string(),
        "OpenDota is the default provider for match surfaces. STRATZ match support is planned but not yet implemented."
            .to_string(),
        daemon_routing_description(),
    ];
    doc.examples = vec![
        HelpExample {
            command: "dota-agent-cli match live --freshness live".to_string(),
            description: "List currently in-progress matches".to_string(),
        },
        HelpExample {
            command: "dota-agent-cli match show 7890123456 --expand".to_string(),
            description: "Show detailed match data with player stats".to_string(),
        },
        HelpExample {
            command: "dota-agent-cli match recent --player-id 12345 --limit 5".to_string(),
            description: "List recent matches for a specific player".to_string(),
        },
    ];
    doc
}

fn match_live_help() -> HelpDocument {
    let mut doc = shared_doc(
        vec!["match".to_string(), "live".to_string()],
        "List currently in-progress matches",
        "dota-agent-cli match live [OPTIONS]",
    );
    doc.options = vec![
        HelpOption {
            name: "--source".to_string(),
            value_name: "auto|opendota|stratz".to_string(),
            default_value: "auto".to_string(),
            description: "Select the provider routing mode for live match feeds".to_string(),
        },
        HelpOption {
            name: "--freshness".to_string(),
            value_name: "live|recent|cached-ok".to_string(),
            default_value: "live".to_string(),
            description: "Control whether live match data must be fetched live or may use cache"
                .to_string(),
        },
        HelpOption {
            name: "--limit".to_string(),
            value_name: "INT".to_string(),
            default_value: "10".to_string(),
            description: "Maximum number of live matches to return".to_string(),
        },
        HelpOption {
            name: "--league-id".to_string(),
            value_name: "INT".to_string(),
            default_value: "none".to_string(),
            description: "Filter to matches from a specific league or tournament".to_string(),
        },
        HelpOption {
            name: "--min-mmr".to_string(),
            value_name: "INT".to_string(),
            default_value: "none".to_string(),
            description: "Minimum average MMR threshold for public live games".to_string(),
        },
        HelpOption {
            name: "--selector".to_string(),
            value_name: "KEY=VALUE".to_string(),
            default_value: "none".to_string(),
            description: "Apply an explicit per-invocation Active Context selector".to_string(),
        },
        HelpOption {
            name: "--cwd".to_string(),
            value_name: "PATH".to_string(),
            default_value: "none".to_string(),
            description: "Apply an explicit current-directory ambient cue".to_string(),
        },
        HelpOption {
            name: "--log-enabled".to_string(),
            value_name: "-".to_string(),
            default_value: "false".to_string(),
            description: "Expose the optional log directory for this invocation".to_string(),
        },
    ];
    doc.options.extend(daemon_routing_options());
    doc.description = vec![
        "live fetches the current list of in-progress matches from the configured provider."
            .to_string(),
        "Default freshness is live to prioritize real-time accuracy for active games.".to_string(),
        daemon_routing_description(),
    ];
    doc.examples = vec![HelpExample {
        command: "dota-agent-cli match live --limit 5 --freshness live".to_string(),
        description: "Show the 5 most recent live matches".to_string(),
    }];
    doc
}

fn match_show_help() -> HelpDocument {
    let mut doc = shared_doc(
        vec!["match".to_string(), "show".to_string()],
        "Show detailed data for one match",
        "dota-agent-cli match show <MATCH_ID> [OPTIONS]",
    );
    doc.arguments = vec!["MATCH_ID: the match ID to look up".to_string()];
    doc.options = vec![
        HelpOption {
            name: "--source".to_string(),
            value_name: "auto|opendota|stratz".to_string(),
            default_value: "auto".to_string(),
            description: "Select the provider routing mode for match detail".to_string(),
        },
        HelpOption {
            name: "--freshness".to_string(),
            value_name: "live|recent|cached-ok".to_string(),
            default_value: "recent".to_string(),
            description: "Control whether match data must be fetched live or may use cache"
                .to_string(),
        },
        HelpOption {
            name: "--expand".to_string(),
            value_name: "-".to_string(),
            default_value: "false".to_string(),
            description: "Include extended analytics: player stats and pick/ban details"
                .to_string(),
        },
        HelpOption {
            name: "--selector".to_string(),
            value_name: "KEY=VALUE".to_string(),
            default_value: "none".to_string(),
            description: "Apply an explicit per-invocation Active Context selector".to_string(),
        },
        HelpOption {
            name: "--cwd".to_string(),
            value_name: "PATH".to_string(),
            default_value: "none".to_string(),
            description: "Apply an explicit current-directory ambient cue".to_string(),
        },
        HelpOption {
            name: "--log-enabled".to_string(),
            value_name: "-".to_string(),
            default_value: "false".to_string(),
            description: "Expose the optional log directory for this invocation".to_string(),
        },
    ];
    doc.options.extend(daemon_routing_options());
    doc.description = vec![
        "show returns detailed match data including duration, game mode, and optionally per-player stats and draft picks/bans."
            .to_string(),
        "Use --expand to include full player performance breakdowns and the complete draft order."
            .to_string(),
        daemon_routing_description(),
    ];
    doc.examples = vec![HelpExample {
        command: "dota-agent-cli match show 7890123456 --expand".to_string(),
        description: "Show full match detail with player stats and draft".to_string(),
    }];
    doc
}

fn match_recent_help() -> HelpDocument {
    let mut doc = shared_doc(
        vec!["match".to_string(), "recent".to_string()],
        "List recent matches for a player",
        "dota-agent-cli match recent [OPTIONS]",
    );
    doc.options = vec![
        HelpOption {
            name: "--player-id".to_string(),
            value_name: "INT".to_string(),
            default_value: "none".to_string(),
            description: "Filter to matches for a specific player by account ID".to_string(),
        },
        HelpOption {
            name: "--hero".to_string(),
            value_name: "NAME".to_string(),
            default_value: "none".to_string(),
            description: "Filter recent matches to a specific hero".to_string(),
        },
        HelpOption {
            name: "--source".to_string(),
            value_name: "auto|opendota|stratz".to_string(),
            default_value: "auto".to_string(),
            description: "Select the provider routing mode for recent match queries".to_string(),
        },
        HelpOption {
            name: "--freshness".to_string(),
            value_name: "live|recent|cached-ok".to_string(),
            default_value: "recent".to_string(),
            description: "Control whether match data must be fetched live or may use cache"
                .to_string(),
        },
        HelpOption {
            name: "--limit".to_string(),
            value_name: "INT".to_string(),
            default_value: "20".to_string(),
            description: "Maximum number of recent matches to return".to_string(),
        },
        HelpOption {
            name: "--sort".to_string(),
            value_name: "recent|winrate|duration|kills".to_string(),
            default_value: "recent".to_string(),
            description: "Sort order for returned matches".to_string(),
        },
        HelpOption {
            name: "--won".to_string(),
            value_name: "-".to_string(),
            default_value: "false".to_string(),
            description: "Filter to matches the player won (requires --player-id)".to_string(),
        },
        HelpOption {
            name: "--selector".to_string(),
            value_name: "KEY=VALUE".to_string(),
            default_value: "none".to_string(),
            description: "Apply an explicit per-invocation Active Context selector".to_string(),
        },
        HelpOption {
            name: "--cwd".to_string(),
            value_name: "PATH".to_string(),
            default_value: "none".to_string(),
            description: "Apply an explicit current-directory ambient cue".to_string(),
        },
        HelpOption {
            name: "--log-enabled".to_string(),
            value_name: "-".to_string(),
            default_value: "false".to_string(),
            description: "Expose the optional log directory for this invocation".to_string(),
        },
    ];
    doc.options.extend(daemon_routing_options());
    doc.description = vec![
        "recent lists recent matches for a specific player, with optional hero and win filters."
            .to_string(),
        "Requires --player-id or a player_id Active Context selector.".to_string(),
        daemon_routing_description(),
    ];
    doc.examples = vec![
        HelpExample {
            command: "dota-agent-cli match recent --player-id 12345 --limit 10".to_string(),
            description: "List 10 recent matches for a player".to_string(),
        },
        HelpExample {
            command: "dota-agent-cli match recent --player-id 12345 --won --sort kills".to_string(),
            description: "List won matches sorted by kills".to_string(),
        },
    ];
    doc
}

fn daemon_launch_options(timeout_default: &str) -> Vec<HelpOption> {
    let mut options = daemon_run_options();
    options.insert(
        2,
        HelpOption {
            name: "--timeout-sec".to_string(),
            value_name: "INT".to_string(),
            default_value: timeout_default.to_string(),
            description: "Maximum seconds to wait for a terminal daemon state".to_string(),
        },
    );
    options
}

fn daemon_run_options() -> Vec<HelpOption> {
    vec![
        HelpOption {
            name: "--transport".to_string(),
            value_name: "stdio|tcp|unix".to_string(),
            default_value: "unix".to_string(),
            description: "Choose the daemon transport mode".to_string(),
        },
        HelpOption {
            name: "--bind".to_string(),
            value_name: "ADDRESS".to_string(),
            default_value: "runtime default".to_string(),
            description: "TCP address or Unix socket path for daemon binding".to_string(),
        },
        HelpOption {
            name: "--cert-file".to_string(),
            value_name: "PATH".to_string(),
            default_value: "none".to_string(),
            description: "TLS certificate path when TCP transport uses TLS".to_string(),
        },
        HelpOption {
            name: "--key-file".to_string(),
            value_name: "PATH".to_string(),
            default_value: "none".to_string(),
            description: "TLS private key path when TCP transport uses TLS".to_string(),
        },
        HelpOption {
            name: "--auth-mode".to_string(),
            value_name: "capability-token|signed-bearer-token".to_string(),
            default_value: "capability-token".to_string(),
            description: "Select the daemon authentication mode".to_string(),
        },
    ]
}

pub fn structured_help(path: &[String]) -> Option<HelpDocument> {
    match path {
        [] => Some(top_level_help()),
        [one] if one == "help" => Some(help_subcommand_help()),
        [one] if one == "run" => Some(run_help()),
        [one] if one == "show" => Some(show_help()),
        [one] if one == "list" => Some(list_help()),
        [one] if one == "source" => Some(source_help()),
        [one] if one == "daemon" => Some(daemon_help()),
        [one] if one == "paths" => Some(paths_help()),
        [one] if one == "context" => Some(context_help()),
        [first, second] if first == "source" && second == "status" => Some(source_status_help()),
        [first, second] if first == "source" && second == "warm" => Some(source_warm_help()),
        [first, second] if first == "daemon" && second == "start" => Some(daemon_start_help()),
        [first, second] if first == "daemon" && second == "stop" => Some(daemon_stop_help()),
        [first, second] if first == "daemon" && second == "restart" => Some(daemon_restart_help()),
        [first, second] if first == "daemon" && second == "status" => Some(daemon_status_help()),
        [first, second] if first == "daemon" && second == "run" => Some(daemon_run_help()),
        [first, second] if first == "context" && second == "show" => Some(context_show_help()),
        [first, second] if first == "context" && second == "use" => Some(context_use_help()),
        [one] if one == "match" => Some(match_help()),
        [first, second] if first == "match" && second == "live" => Some(match_live_help()),
        [first, second] if first == "match" && second == "show" => Some(match_show_help()),
        [first, second] if first == "match" && second == "recent" => Some(match_recent_help()),
        _ => None,
    }
}

pub fn plain_text_help(path: &[String]) -> Option<String> {
    structured_help(path).map(|doc| render_plain_text_help(&doc))
}

pub fn render_plain_text_help(doc: &HelpDocument) -> String {
    let command_name = if doc.command_path.is_empty() {
        "dota-agent-cli".to_string()
    } else {
        format!("dota-agent-cli {}", doc.command_path.join(" "))
    };

    let mut out = String::new();
    out.push_str("NAME\n");
    out.push_str(&format!("  {} - {}\n\n", command_name, doc.purpose));

    out.push_str("SYNOPSIS\n");
    out.push_str(&format!("  {}\n\n", doc.usage));

    out.push_str("DESCRIPTION\n");
    for paragraph in &doc.description {
        out.push_str(&format!("  {}\n", paragraph));
    }
    if !doc.arguments.is_empty() {
        out.push_str("  Arguments:\n");
        for argument in &doc.arguments {
            out.push_str(&format!("    {}\n", argument));
        }
    }
    if !doc.subcommands.is_empty() {
        out.push_str("  Subcommands:\n");
        for subcommand in &doc.subcommands {
            out.push_str(&format!(
                "    {:<12} {}\n",
                subcommand.name, subcommand.summary
            ));
        }
    }
    out.push_str("  Runtime:\n");
    out.push_str(&format!(
        "    config: {}\n    data: {}\n    state: {}\n    cache: {}\n    logs: {}\n",
        doc.runtime_directories.config,
        doc.runtime_directories.data,
        doc.runtime_directories.state,
        doc.runtime_directories.cache,
        doc.runtime_directories.logs
    ));
    out.push_str("  Active Context:\n");
    out.push_str(&format!(
        "    inspect: {}\n    persist: {}\n    precedence: {}\n",
        doc.active_context.inspection_command,
        doc.active_context.switch_command,
        doc.active_context.precedence_rule
    ));
    out.push('\n');

    out.push_str("OPTIONS\n");
    if doc.options.is_empty() {
        out.push_str("  None.\n");
    } else {
        for option in &doc.options {
            out.push_str(&format!(
                "  {:<16} default: {:<18} {}\n",
                option.name, option.default_value, option.description
            ));
        }
    }
    out.push('\n');

    out.push_str("FORMATS\n");
    out.push_str(&format!("  {}\n\n", doc.output_formats.join(", ")));

    out.push_str("EXAMPLES\n");
    if doc.examples.is_empty() {
        out.push_str("  None.\n\n");
    } else {
        for example in &doc.examples {
            out.push_str(&format!(
                "  {}\n    {}\n",
                example.command, example.description
            ));
        }
        out.push('\n');
    }

    out.push_str("EXIT CODES\n");
    for exit in &doc.exit_behavior {
        out.push_str(&format!("  {} -> {}\n", exit.code, exit.meaning));
    }
    out.push('\n');

    out
}
