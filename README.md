# dota-agent-cli

Live provider-backed Dota 2 knowledge and live-match CLI for agent-friendly hero, item, match state, and source lookups.

`dota-agent-cli` routes hero and item reads through live providers with local cache semantics and exposes live match queries through the same CLI surface. Daemonizable query commands can run inline or through the managed daemon with shared runtime artifacts under the state root. The default public route is OpenDota. STRATZ is modeled as an optional richer provider and status surface, but this revision still resolves encyclopedia hero/item reads through OpenDota.

## Build

```sh
cargo build --release
```

The compiled binary will be at `./target/release/dota-agent-cli`.

## Invocation Layers

- Installed skill contract: `dota-agent-cli ...`
- Local development: `cargo run -- ...`
- Built release binary: `./target/release/dota-agent-cli ...`

`SKILL.md` documents the installed contract; this README also shows local verification forms.

## Runtime Contract

- `--help` is always plain text
- `help` returns structured help in YAML, JSON, or TOML
- runtime directories stay split across `config`, `data`, `state`, `cache`, and optional `logs`
- daemonizable commands honor `--via local|daemon` and `--ensure-daemon`
- local-only command surfaces return a structured routing rejection when daemon routing flags are supplied
- Active Context is inspectable and can be persisted or overridden per invocation
- daemon-capable skills expose managed background control through `daemon start`, `daemon stop`, `daemon restart`, and `daemon status`
- `daemon run` exists only as an internal managed-process entrypoint

When `--via daemon` is used, the managed runtime keeps reusable daemon artifacts under `state`, including readiness/status snapshots, transport bind details, and authentication metadata. Optional daemon and invocation logs stay under `logs` when logging is enabled.

## GitHub Release

The default publish channel for this skill is the repository's own GitHub Release, not npm. Release assets are expected to ship version-matched CLI archives together with `release-evidence.json` and `.release-manifest.json`.

Clone-first install flow from a released checkout:

```sh
git checkout v<version>
./scripts/install-current-release.sh <version>
```

The install helper resolves the archive that matches the checked out release version instead of downloading an arbitrary latest asset.

## Provider Routing

- Default route: `OpenDota`
- Optional richer provider: `STRATZ`
- Public control surface: `source status`, `source warm`
- Query freshness modes: `live`, `recent`, `cached-ok`
- Query source modes: `auto`, `opendota`, `stratz`, `cache-only`

Environment variables:

- `DOTA_AGENT_CLI_OPENDOTA_BASE_URL` (preferred; legacy `DOTA_CLI_OPENDOTA_BASE_URL` is still accepted)
- `OPENDOTA_API_KEY` (optional)
- `STRATZ_API_TOKEN` (optional, required only for future STRATZ-backed enrichment)

## Commands

### Search

```sh
dota-agent-cli run blink initiation --type item --source opendota --freshness live
dota-agent-cli run blink initiation --type item --via daemon --ensure-daemon
dota-agent-cli run observer vision --type item --expand
dota-agent-cli run durable initiator --type hero --freshness recent
```

### Detailed Lookup

```sh
dota-agent-cli show hero Axe --overlay stats --source opendota
dota-agent-cli show item Blink Dagger --overlay full
dota-agent-cli show item Observer Ward --related
```

### Listing

```sh
dota-agent-cli list hero --sort popularity --source opendota
dota-agent-cli list item --tag vision
dota-agent-cli list item --tag vision
```

### Provider Status and Warming

```sh
dota-agent-cli source status --source auto --freshness recent
dota-agent-cli source warm --source opendota --scope indexes --force
```

### Active Context

```sh
dota-agent-cli context show
dota-agent-cli context use --name scrim --selector role=support --selector lane=offlane
```

### Managed Daemon Lifecycle

```sh
dota-agent-cli daemon start
dota-agent-cli daemon status --format json
dota-agent-cli daemon restart
dota-agent-cli daemon stop
```

Only managed background daemon mode is standardized. Attached foreground execution is out of scope.

## Knowledge Scope

- Live provider-backed hero discovery and overlays
- Live provider-backed item discovery and overlays
- Cache warming and cache-only fallback for repeated agent workflows

Current limitation:

- STRATZ is surfaced in provider status and future routing, but live encyclopedia hero/item fetches are not implemented against STRATZ yet

## Development

```sh
cargo test
cargo clippy -- -D warnings
cargo fmt --check
```
