//! Optional REPL overlay for dota-agent-cli. Provides an interactive terminal
//! session for exploratory hero/item lookups with persistent state.
//! It does not replace the shared daemon contract: daemon control remains in
//! the dedicated `daemon start|stop|restart|status` command family, and
//! attached foreground daemon execution stays out of scope.

use crate::context::{
    ActiveContextState, InvocationContextOverrides, RuntimeLocations, build_context_state,
    inspect_context, load_active_context, parse_selectors, persist_active_context,
    resolve_effective_context,
};
use crate::encyclopedia::{self, SearchRequest, SearchType};
use crate::providers::{FreshnessMode, SourceSelector, load_live_entries};
use crate::{Format, StructuredError, serialize_value, write_structured_error};
use anyhow::Result;
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{Context as RustylineContext, Editor, Helper};
use serde::Serialize;
use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::PathBuf;

/// REPL-specific output struct that wraps encyclopedia search results
/// into the simple status/message/input/effective_context shape
/// expected by the REPL rendering functions.
#[derive(Debug, Clone, Serialize)]
pub struct ReplOutput {
    pub status: String,
    pub message: String,
    pub input: String,
    pub effective_context: BTreeMap<String, String>,
}

impl ReplOutput {
    pub fn not_found(input: &str, effective_context: BTreeMap<String, String>) -> Self {
        Self {
            status: "not_found".to_string(),
            message: format!("no results for query '{}'", input),
            input: input.to_string(),
            effective_context,
        }
    }

    pub fn success(response: &crate::encyclopedia::SearchResponse, input: &str) -> Self {
        Self {
            status: "ok".to_string(),
            message: format!("{} result(s) for '{}'", response.match_count, input),
            input: input.to_string(),
            effective_context: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
struct ReplHelper {
    candidates: Vec<String>,
}

impl ReplHelper {
    fn new(candidates: Vec<String>) -> Self {
        Self { candidates }
    }
}

impl Helper for ReplHelper {}
impl Hinter for ReplHelper {
    type Hint = String;
}
impl Highlighter for ReplHelper {}
impl Validator for ReplHelper {}

impl Completer for ReplHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &RustylineContext<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let start = line[..pos]
            .rfind(char::is_whitespace)
            .map(|index| index + 1)
            .unwrap_or(0);
        let needle = &line[start..pos];
        let matches = self
            .candidates
            .iter()
            .filter(|candidate| candidate.starts_with(needle))
            .map(|candidate| Pair {
                display: candidate.clone(),
                replacement: candidate.clone(),
            })
            .collect();
        Ok((start, matches))
    }
}

pub fn repl_help_text() -> String {
    [
        "REPL COMMANDS",
        "  help                         Show this plain-text REPL help",
        "  run <INPUT> [KEY=VALUE...]  Search the encyclopedia with optional context selectors",
        "  show <TYPE> <NAME>           Show a detailed encyclopedia entry",
        "  list <TYPE>                  List entries by type (hero, item)",
        "  match live                   List currently in-progress matches",
        "  match show <MATCH_ID>        Show detailed match data",
        "  match recent --player-id ID  List recent matches for a player",
        "  paths                        Show runtime directories",
        "  context show                 Show the persisted and effective Active Context",
        "  context use KEY=VALUE ...    Persist selectors as the Active Context",
        "  context use cwd=/path        Persist a current-directory cue",
        "  exit | quit                  End the session",
        "",
        "DAEMON CONTRACT",
        "  Managed background daemon control stays on daemon start|stop|restart|status.",
        "  Attached foreground daemon execution is out of scope.",
        "",
        "OUTPUT",
        "  Default REPL output is human-readable when the startup format is YAML.",
        "  Explicit JSON or TOML startup formats remain structured.",
    ]
    .join("\n")
}

pub fn completion_candidates(active_context: Option<&ActiveContextState>) -> Vec<String> {
    let mut candidates = vec![
        "help".to_string(),
        "run".to_string(),
        "show".to_string(),
        "list".to_string(),
        "match".to_string(),
        "match live".to_string(),
        "match show".to_string(),
        "match recent".to_string(),
        "paths".to_string(),
        "context".to_string(),
        "exit".to_string(),
        "quit".to_string(),
        "cwd=".to_string(),
    ];

    if let Some(active_context) = active_context {
        for (key, value) in &active_context.selectors {
            candidates.push(format!("{key}={value}"));
        }
        for (key, value) in &active_context.ambient_cues {
            candidates.push(format!("{key}={value}"));
        }
    }

    candidates.sort();
    candidates.dedup();
    candidates
}

fn render_run_output_for_repl(output: &ReplOutput) -> String {
    let mut text = format!(
        "status: {}\nmessage: {}\ninput: {}\n",
        output.status, output.message, output.input
    );
    if output.effective_context.is_empty() {
        text.push_str("effective_context: <none>\n");
    } else {
        text.push_str("effective_context:\n");
        for (key, value) in &output.effective_context {
            text.push_str(&format!("  {key}: {value}\n"));
        }
    }
    text
}

fn render_map_for_repl(title: &str, values: &BTreeMap<String, String>) -> String {
    let mut text = format!("{title}:\n");
    if values.is_empty() {
        text.push_str("  <none>\n");
        return text;
    }
    for (key, value) in values {
        text.push_str(&format!("  {key}: {value}\n"));
    }
    text
}

/// Execute a search using the encyclopedia's search function, returning a
/// ReplOutput that wraps the SearchResponse into the simple shape the REPL
/// rendering code expects.
fn run_encyclopedia_search(
    runtime: &RuntimeLocations,
    input: &str,
    selectors: BTreeMap<String, String>,
) -> anyhow::Result<ReplOutput> {
    let effective_context = resolve_effective_context(
        None,
        &InvocationContextOverrides {
            selectors,
            current_directory: None,
        },
    );

    if input.is_empty() {
        return Ok(ReplOutput::not_found(
            input,
            effective_context.effective_values,
        ));
    }

    let dataset = load_live_entries(runtime, SourceSelector::Auto, FreshnessMode::Recent)
        .map_err(|e| anyhow::anyhow!("{}", e.message()))?;

    let all_entries = dataset.entries;

    let search_response = encyclopedia::search(
        &all_entries,
        SearchRequest {
            query: input,
            requested_type: SearchType::All,
            tag: None,
            limit: 5,
            expand: false,
            effective_context: &effective_context.effective_values,
            source: dataset.source,
        },
    );

    if search_response.match_count == 0 {
        return Ok(ReplOutput::not_found(
            input,
            effective_context.effective_values,
        ));
    }

    Ok(ReplOutput::success(&search_response, input))
}

/// Start an interactive REPL session for dota-agent-cli.
pub fn start_repl(format: Format, runtime: RuntimeLocations) -> Result<()> {
    runtime.ensure_exists()?;

    let mut active_context = load_active_context(&runtime)?;
    let helper = ReplHelper::new(completion_candidates(active_context.as_ref()));
    let mut editor = Editor::<ReplHelper, DefaultHistory>::new()?;
    editor.set_helper(Some(helper));
    let history_path = runtime.history_file();
    let _ = editor.load_history(&history_path);

    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    let stderr = io::stderr();
    let mut stderr = stderr.lock();

    loop {
        write!(stderr, "dota-agent-cli> ")?;
        stderr.flush()?;

        let line = match editor.readline("") {
            Ok(line) => line,
            Err(ReadlineError::Interrupted) => continue,
            Err(ReadlineError::Eof) => break,
            Err(error) => return Err(error.into()),
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let _ = editor.add_history_entry(trimmed);

        match trimmed {
            "exit" | "quit" => break,
            "help" => {
                writeln!(stdout, "{}", repl_help_text())?;
                stdout.flush()?;
            }
            "paths" => {
                let summary = runtime.summary();
                if format == Format::Yaml {
                    writeln!(stdout, "config_dir: {}", summary.config_dir)?;
                    writeln!(stdout, "data_dir: {}", summary.data_dir)?;
                    writeln!(stdout, "state_dir: {}", summary.state_dir)?;
                    writeln!(stdout, "cache_dir: {}", summary.cache_dir)?;
                    if let Some(log_dir) = summary.log_dir {
                        writeln!(stdout, "log_dir: {log_dir}")?;
                    }
                } else {
                    serialize_value(&mut stdout, &runtime.summary(), format)?;
                }
                stdout.flush()?;
            }
            "context show" => {
                let inspection = inspect_context(&runtime, &InvocationContextOverrides::default())?;
                if format == Format::Yaml {
                    writeln!(stdout, "context_file: {}", inspection.context_file)?;
                    writeln!(
                        stdout,
                        "{}",
                        render_map_for_repl(
                            "effective_context",
                            &inspection.effective_context.effective_values,
                        )
                    )?;
                } else {
                    serialize_value(&mut stdout, &inspection, format)?;
                }
                stdout.flush()?;
            }
            command if command.starts_with("context use ") => {
                let mut selectors = BTreeMap::new();
                let mut cwd = None;
                for token in command.split_whitespace().skip(2) {
                    if let Some(path) = token.strip_prefix("cwd=") {
                        cwd = Some(PathBuf::from(path));
                    } else if let Ok(parsed) = parse_selectors(&[token.to_string()]) {
                        selectors.extend(parsed);
                    }
                }

                let state = build_context_state(None, selectors.clone(), cwd);
                let persisted = persist_active_context(&runtime, &state)?;
                active_context = Some(state);
                if let Some(helper) = editor.helper_mut() {
                    helper.candidates = completion_candidates(active_context.as_ref());
                }

                if format == Format::Yaml {
                    writeln!(stdout, "{}", persisted.message)?;
                } else {
                    serialize_value(&mut stdout, &persisted, format)?;
                }
                stdout.flush()?;
            }
            command if command.starts_with("run ") => {
                let mut parts = command.split_whitespace();
                let _ = parts.next();
                let input = parts.next().unwrap_or("").to_string();
                let mut selectors = BTreeMap::new();
                for token in parts {
                    if let Ok(parsed) = parse_selectors(&[token.to_string()]) {
                        selectors.extend(parsed);
                    }
                }

                match run_encyclopedia_search(&runtime, &input, selectors) {
                    Ok(output) => {
                        if format == Format::Yaml {
                            writeln!(stdout, "{}", render_run_output_for_repl(&output))?;
                        } else {
                            serialize_value(&mut stdout, &output, format)?;
                        }
                    }
                    Err(error) => {
                        let err = StructuredError::new(
                            "repl.run_error",
                            error.to_string(),
                            "repl",
                            format,
                        );
                        write_structured_error(&mut stderr, &err, format)?;
                    }
                }
                stdout.flush()?;
            }
            command if command.starts_with("match live") => {
                match crate::match_commands::fetch_live_matches(
                    &runtime,
                    crate::providers::ProviderSourceSelector::Auto,
                    crate::providers::FreshnessMode::Live,
                    10,
                    None,
                    None,
                ) {
                    Ok(output) => {
                        serialize_value(&mut stdout, &output, format)?;
                    }
                    Err(error) => {
                        let err = StructuredError::new(
                            "repl.match_error",
                            error.message().to_string(),
                            "repl",
                            format,
                        );
                        write_structured_error(&mut stderr, &err, format)?;
                    }
                }
                stdout.flush()?;
            }
            command if command.starts_with("match show ") => {
                let parts: Vec<&str> = command.split_whitespace().collect();
                let match_id = parts.get(2).and_then(|s| s.parse::<i64>().ok());
                match match_id {
                    Some(id) => {
                        match crate::match_commands::fetch_match_detail(
                            &runtime,
                            crate::providers::ProviderSourceSelector::Auto,
                            crate::providers::FreshnessMode::Recent,
                            id,
                            true,
                        ) {
                            Ok(output) => {
                                serialize_value(&mut stdout, &output, format)?;
                            }
                            Err(error) => {
                                let err = StructuredError::new(
                                    "repl.match_error",
                                    error.message().to_string(),
                                    "repl",
                                    format,
                                );
                                write_structured_error(&mut stderr, &err, format)?;
                            }
                        }
                    }
                    None => {
                        let err = StructuredError::new(
                            "repl.match_error",
                            "match show requires a numeric match ID".to_string(),
                            "repl",
                            format,
                        );
                        write_structured_error(&mut stderr, &err, format)?;
                    }
                }
                stdout.flush()?;
            }
            command if command.starts_with("match recent") => {
                let mut player_id = None;
                let mut expect_player_id_value = false;
                for token in command.split_whitespace().skip(2) {
                    if expect_player_id_value {
                        player_id = token.parse::<i64>().ok();
                        expect_player_id_value = false;
                        continue;
                    }
                    if let Some(pid_str) = token.strip_prefix("--player-id=") {
                        player_id = pid_str.parse::<i64>().ok();
                    } else if token == "--player-id" {
                        expect_player_id_value = true;
                    } else if let Some(pid_str) = token.strip_prefix("player_id=") {
                        player_id = pid_str.parse::<i64>().ok();
                    }
                }
                let effective_context = resolve_effective_context(
                    active_context.as_ref(),
                    &InvocationContextOverrides {
                        selectors: BTreeMap::new(),
                        current_directory: None,
                    },
                );
                let hero_entries: Vec<_> = crate::providers::load_live_entries(
                    &runtime,
                    crate::providers::SourceSelector::Auto,
                    crate::providers::FreshnessMode::Recent,
                )
                .map_err(|e| anyhow::anyhow!("{}", e.message()))?
                .entries
                .into_iter()
                .filter(|e| e.kind == crate::encyclopedia::EntryKind::Hero)
                .collect();
                match crate::match_commands::fetch_recent_matches(
                    &runtime,
                    crate::providers::ProviderSourceSelector::Auto,
                    crate::providers::FreshnessMode::Recent,
                    player_id,
                    None,
                    10,
                    crate::match_commands::MatchSort::Recent,
                    false,
                    &effective_context.effective_values,
                    &hero_entries,
                ) {
                    Ok(output) => {
                        serialize_value(&mut stdout, &output, format)?;
                    }
                    Err(error) => {
                        let err = StructuredError::new(
                            "repl.match_error",
                            error.message().to_string(),
                            "repl",
                            format,
                        );
                        write_structured_error(&mut stderr, &err, format)?;
                    }
                }
                stdout.flush()?;
            }
            _ => {
                let error = StructuredError::new(
                    "repl.unknown_command",
                    format!("unknown REPL command: {trimmed}"),
                    "repl",
                    format,
                );
                write_structured_error(&mut stderr, &error, format)?;
                stderr.flush()?;
            }
        }
    }

    let _ = editor.save_history(&history_path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ReplOutput, completion_candidates, render_run_output_for_repl, repl_help_text};
    use crate::context::build_context_state;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    #[test]
    fn repl_help_is_plain_text() {
        let help = repl_help_text();
        assert!(help.contains("REPL COMMANDS"));
        assert!(help.contains("context show"));
        assert!(!help.trim_start().starts_with('{'));
    }

    #[test]
    fn completion_candidates_include_visible_context_values() {
        let mut selectors = BTreeMap::new();
        selectors.insert("workspace".to_string(), "demo".to_string());
        let context = build_context_state(None, selectors, Some(PathBuf::from("/tmp/demo")));
        let candidates = completion_candidates(Some(&context));

        assert!(candidates.contains(&"run".to_string()));
        assert!(candidates.contains(&"workspace=demo".to_string()));
        assert!(candidates.contains(&"current_directory=/tmp/demo".to_string()));
    }

    #[test]
    fn yaml_repl_output_is_human_readable() {
        let mut effective_context = BTreeMap::new();
        effective_context.insert("workspace".to_string(), "demo".to_string());
        let output = ReplOutput {
            status: "ok".to_string(),
            message: "3 result(s) for 'axe'".to_string(),
            input: "axe".to_string(),
            effective_context,
        };
        let rendered = render_run_output_for_repl(&output);

        assert!(rendered.contains("status: ok"));
        assert!(rendered.contains("workspace: demo"));
    }
}
