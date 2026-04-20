---
name: "dota-agent-cli"
description: "Live provider-backed Dota 2 knowledge and live-match CLI for agent-friendly hero, item, match state, and source lookups"
---

# dota-agent-cli

## Description

Live provider-backed Dota 2 knowledge and live-match CLI for agent-friendly hero, item, match state, and source lookups.

This skill uses live providers for hero, item, and match discovery. OpenDota is the default public source. STRATZ is modeled as an optional richer provider and status surface, but this revision does not yet resolve encyclopedia hero/item indexes through STRATZ.

The default publish channel is the repository's own GitHub Release. Released archives are expected to ship with `release-evidence.json`, `.release-manifest.json`, and the clone-first install helper `scripts/install-current-release.sh`.

## Prerequisites

- A working Rust toolchain (`rustup`, `cargo`) to compile and test the binary.
- Network access is required for fresh live-provider reads and cache warming.
- No additional system packages are required for the default build.

## Invocation

```text
dota-agent-cli [OPTIONS] <COMMAND>
dota-agent-cli help [COMMAND_PATH ...] [--format yaml|json|toml]
dota-agent-cli run [OPTIONS] <QUERY>
dota-agent-cli show <hero|item> <NAME> [OPTIONS]
dota-agent-cli list <hero|item> [OPTIONS]
dota-agent-cli source <status|warm> [OPTIONS]
dota-agent-cli match live [OPTIONS]
dota-agent-cli match show <MATCH_ID> [OPTIONS]
dota-agent-cli match recent [OPTIONS]
dota-agent-cli daemon <start|stop|restart|status> [OPTIONS]
dota-agent-cli daemon run [OPTIONS]
dota-agent-cli paths [OPTIONS]
dota-agent-cli context <show|use> [OPTIONS]
dota-agent-cli --repl [OPTIONS]
```

The canonical agent-facing contract uses the bare command name shown above. `cargo run -- ...` and `./target/release/dota-agent-cli ...` are development and release-verification forms only.

### Global Options

| Flag              | Type                       | Default                   | Description                                                        |
| ----------------- | -------------------------- | ------------------------- | ------------------------------------------------------------------ |
| `--format`, `-f`  | `yaml` \| `json` \| `toml` | `yaml`                    | Structured output format for one-shot commands and structured help |
| `--help`, `-h`    | —                          | —                         | Plain-text help only                                               |
| `--config-dir`    | `PATH`                     | platform default          | Override the configuration directory                               |
| `--data-dir`      | `PATH`                     | platform default          | Override the durable data directory                                |
| `--state-dir`     | `PATH`                     | derived from data         | Override the runtime state directory                               |
| `--cache-dir`     | `PATH`                     | platform default          | Override the cache directory                                       |
| `--log-dir`       | `PATH`                     | `state/logs` when enabled | Override the optional log directory                                |
| `--repl`          | —                          | `false`                   | Start an interactive REPL session                                  |
| `--version`, `-V` | —                          | —                         | Print version and exit                                             |

### Commands

| Command         | Kind    | Purpose                                                             |
| --------------- | ------- | ------------------------------------------------------------------- |
| `help`          | leaf    | Return structured help for a command path                           |
| `run`           | leaf    | Search live provider-backed hero/item data                          |
| `show`          | leaf    | Return a detailed encyclopedia entry                                |
| `list`          | leaf    | Enumerate entries in one knowledge category                         |
| `source status` | leaf    | Inspect provider auth, cache, and reachability state                |
| `source warm`   | leaf    | Warm provider caches for later recent or cache-only reads           |
| `match live`    | leaf    | List currently in-progress matches from live provider feeds         |
| `match show`    | leaf    | Show detailed data for one match by match ID                        |
| `match recent`  | leaf    | List recent matches filtered by player or hero                      |
| `daemon`        | tree    | Control one managed background daemon through CLI commands          |
| `paths`         | leaf    | Inspect runtime directory defaults and overrides                    |
| `context show`  | leaf    | Display the current Active Context                                  |
| `context use`   | leaf    | Persist selectors or ambient cues as the Active Context             |
| `repl`          | session | Interactive terminal session for exploratory lookups (via `--repl`) |

## Input

- `run` requires one positional `<QUERY>` argument.
- `show` requires a `<hero|item>` type and a `<NAME>`.
- `list` requires a type and accepts optional filters such as `--tag`.
- `run`, `show`, and `list` accept `--source` and `--freshness`.
- `show` also accepts `--overlay basic|stats|full`.
- `list` also accepts `--sort name|popularity|winrate|updated`.
- `source status` accepts `--source` and `--freshness`.
- `source warm` accepts `--source`, `--scope`, and `--force`.
- `match live` accepts `--source`, `--freshness`, `--limit`, `--league-id`, and `--min-mmr`.
- `match show` requires a positional `<MATCH_ID>` and accepts `--source`, `--freshness`, and `--expand`.
- `match recent` accepts `--player-id`, `--hero`, `--source`, `--freshness`, `--limit`, `--sort`, and `--won`.
- `--repl` starts an interactive session; it accepts `--format` as a global option.

## Output

Standard command results are written to `stdout`. Errors and diagnostics are written to `stderr`.

### Help Channels

- `--help` is the plain-text help channel. It always prints text and exits `0`.
- `help` is the structured help channel. It supports `yaml`, `json`, and `toml`, with YAML as the default.
- Top-level invocation and non-leaf invocation such as `context` display plain-text help automatically and exit `0`.

### Structured Results

The default one-shot result format is YAML.

Example `run` result:

```yaml
query: blink initiation
requested_type: item
match_count: 1
source:
  requested_source: opendota
  resolved_sources:
    - opendota
results:
  - kind: item
    name: Blink Dagger
```

Example `source status` result:

```yaml
requested_source: auto
providers:
  - provider: opendota
    reachability: reachable
  - provider: stratz
    auth: missing
```

### GitHub Release Installation

- Repo-native GitHub Release is the primary distribution path for this skill.
- Released checkouts should be installable with `./scripts/install-current-release.sh <version>`.
- Release evidence is expected in `release-evidence.json`, with `.release-manifest.json` mirroring the same released version.

### Runtime Directories and Active Context

- `paths` exposes `config`, `data`, `state`, `cache`, and optional `logs`.
- Defaults are user-scoped unless explicitly overridden.
- `context show` exposes the persisted and effective Active Context.
- Explicit selectors on `run`, `show`, or `list` override the persisted Active Context for that invocation only.

### Managed Daemon Contract

- Public daemon control uses `daemon start`, `daemon stop`, `daemon restart`, and `daemon status`.
- Daemonizable leaf commands (`run`, `show`, `list`, `match live`, `match show`, `match recent`) accept `--via local|daemon`.
- `--ensure-daemon` can auto-start or reuse the managed daemon before a daemon-routed leaf command runs.
- Local-only surfaces such as `help`, `source`, `paths`, `context`, and `daemon` reject daemon routing with structured errors.
- The contract standardizes only managed background daemon mode.
- Attached foreground execution is out of scope.
- `daemon run` exists only as an internal managed-process entrypoint.
- `daemon start`, `daemon restart`, and `daemon run` accept transport/auth/TLS binding options for the managed process surface.
- Managed daemon artifacts persist under the runtime state directory's `daemon/` subtree.

## REPL Mode

- Start interactive mode with `--repl`.
- The prompt is `dota-agent-cli> ` and is written to stderr.
- `help` inside the REPL is plain text only and explains available commands, context inspection controls, and output behavior.
- Each input line is handled as one command and the result is written to stdout. Default session output is human-readable YAML; explicit `--format json` or `--format toml` preserves structured output.
- Command history persists under the runtime state directory (`repl_history.txt`).
- Tab completion is available for command names, option names, and visible Active Context values.
- `exit`, `quit`, or EOF end the session with exit code `0`.
- Per-command errors are written to stderr and do not terminate the REPL.

## Errors

| Exit Code | Meaning                                         |
| --------- | ----------------------------------------------- |
| `0`       | Success or plain-text help                      |
| `1`       | Unexpected runtime failure                      |
| `2`       | Structured usage, validation, or provider error |

Structured errors preserve the selected output format and include stable `code` and `message` fields.

## Examples

```text
$ dota-agent-cli run blink initiation --type item --source opendota --freshness recent
$ dota-agent-cli show hero Axe --overlay stats
$ dota-agent-cli list item --tag vision
$ dota-agent-cli source status --source auto --freshness recent
$ dota-agent-cli source warm --source opendota --scope indexes --force
$ dota-agent-cli match live --freshness live --limit 5
$ dota-agent-cli match show 7890123456 --expand
$ dota-agent-cli match recent --player-id 12345 --limit 10
$ dota-agent-cli match recent --player-id 12345 --won --sort kills
$ dota-agent-cli context use --selector role=support --selector lane=safelane
$ dota-agent-cli --repl
$ dota-agent-cli --repl --format json
```
