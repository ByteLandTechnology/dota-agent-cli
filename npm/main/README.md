# dota-agent-cli

Live provider-backed Dota 2 knowledge and live-match CLI for agent-friendly hero, item, match state, and source lookups.

## Install

```bash
npm install -g dota-agent-cli
```

## Usage

```bash
dota-agent-cli --help
dota-agent-cli run <query>
dota-agent-cli show hero <name>
dota-agent-cli match live
```

npm automatically installs the correct platform-specific binary via `optionalDependencies`.
