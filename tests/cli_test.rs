use assert_cmd::Command;
use predicates::prelude::*;
use std::env;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

fn cmd() -> Command {
    Command::cargo_bin(env!("CARGO_PKG_NAME")).expect("binary should exist")
}

fn assert_plain_text_help_sections(stdout: &str) {
    let labels = [
        "NAME\n",
        "SYNOPSIS\n",
        "DESCRIPTION\n",
        "OPTIONS\n",
        "FORMATS\n",
        "EXAMPLES\n",
        "EXIT CODES\n",
    ];
    let mut last = 0;
    for label in labels {
        let idx = stdout
            .find(label)
            .unwrap_or_else(|| panic!("missing help section {label:?}"));
        assert!(
            idx >= last,
            "section {label:?} appeared out of order in:\n{stdout}"
        );
        last = idx;
    }

    assert!(!stdout.contains("ARGUMENTS\n"));
    assert!(!stdout.contains("OUTPUT FORMATS\n"));
    assert!(!stdout.contains("RUNTIME DIRECTORIES\n"));
    assert!(!stdout.contains("ACTIVE CONTEXT\n"));
}

fn sandbox_args(temp_dir: &TempDir) -> Vec<String> {
    vec![
        "--config-dir".to_string(),
        temp_dir.path().join("config").display().to_string(),
        "--data-dir".to_string(),
        temp_dir.path().join("data").display().to_string(),
        "--state-dir".to_string(),
        temp_dir.path().join("state").display().to_string(),
        "--cache-dir".to_string(),
        temp_dir.path().join("cache").display().to_string(),
    ]
}

fn skip_bind_tests() -> bool {
    env::var("DOTA_AGENT_CLI_SKIP_BIND_TESTS")
        .or_else(|_| env::var("DOTA_CLI_SKIP_BIND_TESTS"))
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

struct MockOpenDota {
    base_url: String,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl MockOpenDota {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        listener
            .set_nonblocking(true)
            .expect("configure nonblocking listener");
        let addr = listener.local_addr().expect("read listener address");
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);

        let handle = thread::spawn(move || {
            while !stop_thread.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let mut buffer = [0_u8; 4096];
                        let read = match stream.read(&mut buffer) {
                            Ok(read) => read,
                            Err(_) => continue,
                        };
                        let request = String::from_utf8_lossy(&buffer[..read]);
                        let path = request
                            .lines()
                            .next()
                            .and_then(|line| line.split_whitespace().nth(1))
                            .unwrap_or("/");
                        let body = match path {
                            "/api/heroStats" | "/heroStats" => HERO_STATS,
                            "/api/constants/items" | "/constants/items" => ITEM_CONSTANTS,
                            "/api/live" | "/live" => LIVE_MATCHES,
                            _ if path.starts_with("/api/matches/")
                                || path.starts_with("/matches/") =>
                            {
                                MATCH_DETAIL
                            }
                            _ if path.starts_with("/api/players/")
                                || path.starts_with("/players/") =>
                            {
                                PLAYER_RECENT_MATCHES
                            }
                            _ => "{\"error\":\"not found\"}",
                        };
                        let status = if matches!(
                            path,
                            "/api/heroStats"
                                | "/heroStats"
                                | "/api/constants/items"
                                | "/constants/items"
                                | "/api/live"
                                | "/live"
                        ) || path.starts_with("/api/matches/")
                            || path.starts_with("/matches/")
                            || path.starts_with("/api/players/")
                            || path.starts_with("/players/")
                        {
                            "200 OK"
                        } else {
                            "404 Not Found"
                        };
                        let response = format!(
                            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        let _ = stream.write_all(response.as_bytes());
                        let _ = stream.flush();
                        let _ = stream.shutdown(Shutdown::Both);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            base_url: format!("http://127.0.0.1:{}/api", addr.port()),
            stop,
            handle: Some(handle),
        }
    }
}

impl Drop for MockOpenDota {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = std::net::TcpStream::connect(self.base_url.replacen("/api", "", 1));
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

const HERO_STATS: &str = r#"[
  {
    "id": 2,
    "name": "npc_dota_hero_axe",
    "localized_name": "Axe",
    "primary_attr": "str",
    "attack_type": "Melee",
    "roles": ["Initiator", "Durable", "Disabler"],
    "pro_pick": 120,
    "pro_win": 66,
    "move_speed": 315,
    "base_armor": 1.0,
    "base_health": 670
  },
  {
    "id": 74,
    "name": "npc_dota_hero_invoker",
    "localized_name": "Invoker",
    "primary_attr": "all",
    "attack_type": "Ranged",
    "roles": ["Nuker", "Disabler", "Escape"],
    "pro_pick": 80,
    "pro_win": 38,
    "move_speed": 285,
    "base_armor": 0.0,
    "base_health": 560
  }
]"#;

const ITEM_CONSTANTS: &str = r#"{
  "blink": {
    "id": 1,
    "dname": "Blink Dagger",
    "qual": "common",
    "cost": 2250,
    "notes": "Instant repositioning tool used for initiation.",
    "hint": ["Teleports the wielder a short distance."],
    "components": [],
    "mc": 0,
    "cd": 15
  },
  "ward_observer": {
    "id": 42,
    "dname": "Observer Ward",
    "qual": "consumable",
    "cost": 0,
    "notes": "Provides ground vision for map control.",
    "hint": ["Grants vision in an area."],
    "components": [],
    "mc": 0,
    "cd": 0
  }
}"#;

const LIVE_MATCHES: &str = r#"[
  {
    "match_id": 7890123456,
    "server_steam_id": 123456789,
    "game_time": 1800,
    "league_id": 15431,
    "radiant_lead": 3500,
    "average_mmr": 5500,
    "players": [
      {"account_id": 1001, "hero_id": 2, "name": "Player1"},
      {"account_id": 1002, "hero_id": 74, "name": "Player2"}
    ]
  },
  {
    "match_id": 7890123457,
    "server_steam_id": 123456790,
    "game_time": 600,
    "league_id": 0,
    "radiant_lead": -500,
    "average_mmr": 4200,
    "players": [
      {"account_id": 2001, "hero_id": 5, "name": "Player3"}
    ]
  }
]"#;

const MATCH_DETAIL: &str = r#"{
  "match_id": 7890123456,
  "radiant_win": true,
  "duration": 2400,
  "game_mode": 2,
  "leagueid": 15431,
  "picks_bans": [
    {"is_pick": true, "hero_id": 2, "side": 0, "order": 1},
    {"is_pick": false, "hero_id": 74, "side": 1, "order": 2}
  ],
  "players": [
    {
      "account_id": 1001,
      "player_slot": 0,
      "hero_id": 2,
      "kills": 12,
      "deaths": 3,
      "assists": 8,
      "gold_per_min": 650,
      "xp_per_min": 720
    },
    {
      "account_id": 1002,
      "player_slot": 128,
      "hero_id": 74,
      "kills": 5,
      "deaths": 8,
      "assists": 4,
      "gold_per_min": 380,
      "xp_per_min": 450
    }
  ]
}"#;

const PLAYER_RECENT_MATCHES: &str = r#"[
  {
    "match_id": 7890123456,
    "player_slot": 0,
    "radiant_win": true,
    "hero_id": 2,
    "duration": 2400,
    "game_mode": 2,
    "kills": 12,
    "deaths": 3,
    "assists": 8,
    "start_time": 1713078000
  },
  {
    "match_id": 7890123400,
    "player_slot": 128,
    "radiant_win": false,
    "hero_id": 74,
    "duration": 1800,
    "game_mode": 22,
    "kills": 5,
    "deaths": 8,
    "assists": 4,
    "start_time": 1712991600
  }
]"#;

#[test]
fn test_version_prints_semver() {
    cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::is_match(r"\d+\.\d+\.\d+").unwrap());
}

#[test]
fn test_top_level_auto_help_exits_zero() {
    let output = cmd().assert().success().get_output().stdout.clone();
    let stdout = String::from_utf8(output).expect("non-utf8 output");
    assert_plain_text_help_sections(&stdout);
    assert!(stdout.contains("source"));
    assert!(stdout.contains("daemon"));
}

#[test]
fn test_help_flag_stays_plain_text_even_with_json_format() {
    let output = cmd()
        .args(["run", "--help", "--format", "json"])
        .output()
        .expect("failed to execute");

    assert!(output.status.success(), "expected exit 0");

    let stdout = String::from_utf8(output.stdout).expect("non-utf8 output");
    assert_plain_text_help_sections(&stdout);
    assert!(!stdout.trim_start().starts_with('{'));
}

#[test]
fn test_non_leaf_auto_help_uses_canonical_plain_text_sections() {
    let output = cmd()
        .args(["daemon", "--help"])
        .output()
        .expect("failed to execute");

    assert!(output.status.success(), "expected exit 0");
    let stdout = String::from_utf8(output.stdout).expect("non-utf8 output");
    assert_plain_text_help_sections(&stdout);
    assert!(stdout.contains("Subcommands:"));
}

#[test]
fn test_structured_help_yaml_for_run_mentions_provider_flags() {
    let output = cmd()
        .args(["help", "run", "--format", "yaml"])
        .output()
        .expect("failed to execute");

    assert!(output.status.success(), "expected exit 0");

    let stdout = String::from_utf8(output.stdout).expect("non-utf8 output");
    let value: serde_yaml::Value =
        serde_yaml::from_str(&stdout).expect("stdout should be valid YAML");

    assert_eq!(value["command_path"][0], "run");
    assert!(!stdout.contains("concept"));
    let options = value["options"]
        .as_sequence()
        .expect("options should exist");
    assert!(options.iter().any(|option| option["name"] == "--source"));
    assert!(options.iter().any(|option| option["name"] == "--freshness"));
    assert!(options.iter().any(|option| option["name"] == "--via"));
    assert!(
        options
            .iter()
            .any(|option| option["name"] == "--ensure-daemon")
    );
    let descriptions = value["description"]
        .as_sequence()
        .expect("description should exist");
    assert!(descriptions.iter().any(|line| {
        line.as_str()
            .unwrap_or_default()
            .contains("--via local|daemon")
    }));
    assert!(
        value["runtime_directories"]["state"]
            .as_str()
            .unwrap_or_default()
            .contains("daemon artifacts")
    );
}

#[test]
fn test_missing_run_query_returns_structured_yaml_error() {
    let output = cmd().arg("run").output().expect("failed to execute");

    assert!(!output.status.success(), "expected non-zero exit");
    let stderr = String::from_utf8(output.stderr).expect("non-utf8 stderr");
    let value: serde_yaml::Value =
        serde_yaml::from_str(&stderr).expect("stderr should be valid YAML");

    assert_eq!(value["code"], "run.missing_query");
}

#[test]
fn test_run_returns_provider_backed_item_matches() {
    if skip_bind_tests() {
        return;
    }

    let mock = MockOpenDota::start();
    let temp_dir = TempDir::new().expect("temp dir");
    let sandbox = sandbox_args(&temp_dir);

    let output = cmd()
        .env("DOTA_AGENT_CLI_OPENDOTA_BASE_URL", &mock.base_url)
        .args(&sandbox)
        .args([
            "run",
            "blink",
            "initiation",
            "--type",
            "item",
            "--source",
            "opendota",
            "--freshness",
            "live",
            "--format",
            "json",
        ])
        .output()
        .expect("failed to execute");

    assert!(output.status.success(), "expected exit 0");

    let stdout = String::from_utf8(output.stdout).expect("non-utf8 output");
    let value: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON");

    assert_eq!(value["requested_type"], "item");
    assert_eq!(value["source"]["resolved_sources"][0], "opendota");
    let names = value["results"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|entry| entry["name"].as_str())
        .collect::<Vec<_>>();
    assert!(names.contains(&"Blink Dagger"));
}

#[test]
fn test_local_only_command_rejects_daemon_routing() {
    let temp_dir = TempDir::new().expect("temp dir");
    let sandbox = sandbox_args(&temp_dir);

    let output = cmd()
        .args(&sandbox)
        .args(["--via", "daemon", "paths", "--format", "json"])
        .output()
        .expect("failed to execute");

    assert!(!output.status.success(), "expected non-zero exit");
    let stderr = String::from_utf8(output.stderr).expect("non-utf8 stderr");
    let value: serde_json::Value =
        serde_json::from_str(&stderr).expect("stderr should be valid JSON");
    assert_eq!(value["code"], "runtime.daemon_routing_unsupported");
}

#[test]
fn test_daemon_routed_run_requires_ready_daemon_without_ensure() {
    let temp_dir = TempDir::new().expect("temp dir");
    let sandbox = sandbox_args(&temp_dir);

    let output = cmd()
        .args(&sandbox)
        .args([
            "--via", "daemon", "run", "blink", "--type", "item", "--format", "json",
        ])
        .output()
        .expect("failed to execute");

    assert!(!output.status.success(), "expected non-zero exit");
    let stderr = String::from_utf8(output.stderr).expect("non-utf8 stderr");
    let value: serde_json::Value =
        serde_json::from_str(&stderr).expect("stderr should be valid JSON");
    assert_eq!(value["code"], "daemon.route_unavailable");
}

#[test]
fn test_daemon_routed_run_auto_starts_and_writes_artifacts() {
    if skip_bind_tests() {
        return;
    }

    let mock = MockOpenDota::start();
    let temp_dir = TempDir::new().expect("temp dir");
    let sandbox = sandbox_args(&temp_dir);

    let output = cmd()
        .env("DOTA_AGENT_CLI_OPENDOTA_BASE_URL", &mock.base_url)
        .args(&sandbox)
        .args([
            "--via",
            "daemon",
            "--ensure-daemon",
            "run",
            "blink",
            "initiation",
            "--type",
            "item",
            "--source",
            "opendota",
            "--freshness",
            "live",
            "--format",
            "json",
        ])
        .output()
        .expect("failed to execute");

    assert!(output.status.success(), "expected exit 0");
    let stdout = String::from_utf8(output.stdout).expect("non-utf8 output");
    let value: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON");
    let names = value["results"]
        .as_array()
        .expect("results array")
        .iter()
        .filter_map(|entry| entry["name"].as_str())
        .collect::<Vec<_>>();
    assert!(names.contains(&"Blink Dagger"));

    let daemon_dir = temp_dir.path().join("state").join("daemon");
    assert!(daemon_dir.join("state.toml").exists());
    assert!(daemon_dir.join("metadata.toml").exists());
    assert!(daemon_dir.join("daemon.pid").exists());
    assert!(daemon_dir.join("daemon.lock").exists());
    assert!(daemon_dir.join("endpoint").exists());
    assert!(daemon_dir.join("daemon.log").exists());
}

#[test]
fn test_show_hero_overlay_stats_json() {
    if skip_bind_tests() {
        return;
    }

    let mock = MockOpenDota::start();
    let temp_dir = TempDir::new().expect("temp dir");
    let sandbox = sandbox_args(&temp_dir);

    let output = cmd()
        .env("DOTA_AGENT_CLI_OPENDOTA_BASE_URL", &mock.base_url)
        .args(&sandbox)
        .args([
            "show",
            "hero",
            "Axe",
            "--source",
            "opendota",
            "--overlay",
            "stats",
            "--format",
            "json",
        ])
        .output()
        .expect("failed to execute");

    assert!(output.status.success(), "expected exit 0");

    let stdout = String::from_utf8(output.stdout).expect("non-utf8 output");
    let value: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON");

    assert_eq!(value["kind"], "hero");
    assert_eq!(value["name"], "Axe");
    assert_eq!(value["overlay_mode"], "stats");
    assert!(value["live_overlay"].is_object());
    assert_eq!(value["source"]["resolved_sources"][0], "opendota");
}

#[test]
fn test_list_rejects_removed_concept_type() {
    let output = cmd()
        .args(["list", "concept", "--format", "json"])
        .output()
        .expect("failed to execute");

    assert!(!output.status.success(), "expected non-zero exit");

    let stderr = String::from_utf8(output.stderr).expect("non-utf8 stderr");
    let value: serde_json::Value =
        serde_json::from_str(&stderr).expect("stderr should be valid JSON");
    assert_eq!(value["code"], "usage.parse_error");
    assert!(value["message"].as_str().unwrap().contains("concept"));
}

#[test]
fn test_source_status_reports_opendota_and_stratz() {
    if skip_bind_tests() {
        return;
    }

    let mock = MockOpenDota::start();
    let temp_dir = TempDir::new().expect("temp dir");
    let sandbox = sandbox_args(&temp_dir);

    let output = cmd()
        .env("DOTA_AGENT_CLI_OPENDOTA_BASE_URL", &mock.base_url)
        .args(&sandbox)
        .args([
            "source",
            "status",
            "--source",
            "auto",
            "--freshness",
            "recent",
            "--format",
            "json",
        ])
        .output()
        .expect("failed to execute");

    assert!(output.status.success(), "expected exit 0");
    let stdout = String::from_utf8(output.stdout).expect("non-utf8 output");
    let value: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON");

    let providers = value["providers"].as_array().expect("providers array");
    assert!(
        providers
            .iter()
            .any(|entry| entry["provider"] == "opendota")
    );
    assert!(providers.iter().any(|entry| entry["provider"] == "stratz"));
}

#[test]
fn test_source_warm_then_cache_only_show_succeeds() {
    if skip_bind_tests() {
        return;
    }

    let mock = MockOpenDota::start();
    let temp_dir = TempDir::new().expect("temp dir");
    let sandbox = sandbox_args(&temp_dir);

    cmd()
        .env("DOTA_AGENT_CLI_OPENDOTA_BASE_URL", &mock.base_url)
        .args(&sandbox)
        .args([
            "source", "warm", "--source", "opendota", "--scope", "indexes", "--force", "--format",
            "json",
        ])
        .assert()
        .success();

    let output = cmd()
        .args(&sandbox)
        .args([
            "show",
            "hero",
            "Axe",
            "--source",
            "cache-only",
            "--freshness",
            "cached-ok",
            "--format",
            "json",
        ])
        .output()
        .expect("failed to execute");

    assert!(output.status.success(), "expected exit 0");
    let stdout = String::from_utf8(output.stdout).expect("non-utf8 output");
    let value: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON");
    assert_eq!(value["name"], "Axe");
    assert_eq!(value["source"]["cache_state"], "fresh_cache");
}

#[test]
fn test_cache_only_without_warm_returns_structured_error() {
    let temp_dir = TempDir::new().expect("temp dir");
    let sandbox = sandbox_args(&temp_dir);

    let output = cmd()
        .args(&sandbox)
        .args([
            "show",
            "hero",
            "Axe",
            "--source",
            "cache-only",
            "--freshness",
            "cached-ok",
            "--format",
            "json",
        ])
        .output()
        .expect("failed to execute");

    assert!(!output.status.success(), "expected non-zero exit");
    let stderr = String::from_utf8(output.stderr).expect("non-utf8 stderr");
    let value: serde_json::Value =
        serde_json::from_str(&stderr).expect("stderr should be valid JSON");
    assert_eq!(value["code"], "provider.cache_miss");
}

#[test]
fn test_context_use_and_show_round_trip() {
    let temp_dir = TempDir::new().expect("temp dir");
    let sandbox = sandbox_args(&temp_dir);

    cmd()
        .args(&sandbox)
        .args([
            "context",
            "use",
            "--selector",
            "role=support",
            "--selector",
            "lane=offlane",
            "--format",
            "json",
        ])
        .assert()
        .success();

    let output = cmd()
        .args(&sandbox)
        .args(["context", "show", "--format", "json"])
        .output()
        .expect("failed to execute");

    assert!(output.status.success(), "expected exit 0");
    let stdout = String::from_utf8(output.stdout).expect("non-utf8 output");
    let value: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON");

    assert_eq!(value["persisted_context"]["selectors"]["role"], "support");
    assert_eq!(
        value["effective_context"]["effective_values"]["lane"],
        "offlane"
    );
}

#[test]
fn test_paths_documents_user_scoped_runtime_dirs() {
    let temp_dir = TempDir::new().expect("temp dir");
    let sandbox = sandbox_args(&temp_dir);

    let output = cmd()
        .args(&sandbox)
        .args(["paths", "--log-enabled", "--format", "json"])
        .output()
        .expect("failed to execute");

    assert!(output.status.success(), "expected exit 0");
    let stdout = String::from_utf8(output.stdout).expect("non-utf8 output");
    let value: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON");

    assert!(value["config_dir"].as_str().unwrap().contains("config"));
    assert_eq!(value["scope"], "explicit_override");
    assert!(value["log_dir"].as_str().unwrap().contains("logs"));
}

#[test]
fn test_daemon_help_documents_public_contract_and_internal_run() {
    let output = cmd()
        .args(["daemon", "--help"])
        .output()
        .expect("failed to execute");

    assert!(output.status.success(), "expected exit 0");
    let stdout = String::from_utf8(output.stdout).expect("non-utf8 output");
    assert_plain_text_help_sections(&stdout);
    assert!(stdout.contains("daemon <start|stop|restart|status>"));
    assert!(stdout.contains("single managed instance"));
    assert!(stdout.contains("daemon run"));
}

#[test]
fn test_structured_help_yaml_for_local_only_command_mentions_routing_rejection() {
    let output = cmd()
        .args(["help", "paths", "--format", "yaml"])
        .output()
        .expect("failed to execute");

    assert!(output.status.success(), "expected exit 0");
    let stdout = String::from_utf8(output.stdout).expect("non-utf8 output");
    let value: serde_yaml::Value =
        serde_yaml::from_str(&stdout).expect("stdout should be valid YAML");

    let descriptions = value["description"]
        .as_sequence()
        .expect("description should exist");
    assert!(descriptions.iter().any(|line| {
        line.as_str()
            .unwrap_or_default()
            .contains("reject daemon routing flags")
    }));
    assert!(
        value["runtime_directories"]["logs"]
            .as_str()
            .unwrap_or_default()
            .contains("daemon")
    );
}

#[test]
fn test_daemon_start_status_stop_round_trip_uses_single_instance() {
    let temp_dir = TempDir::new().expect("temp dir");
    let sandbox = sandbox_args(&temp_dir);

    let start_output = cmd()
        .args(&sandbox)
        .args(["daemon", "start"])
        .output()
        .expect("failed to execute");
    assert!(start_output.status.success(), "expected exit 0");
    let start_stdout = String::from_utf8(start_output.stdout).expect("non-utf8 output");
    let start_value: serde_yaml::Value =
        serde_yaml::from_str(&start_stdout).expect("stdout should be valid YAML");
    assert_eq!(start_value["action"], "start");
    assert_eq!(start_value["result"], "running");
    assert_eq!(start_value["state"], "running");
    assert_eq!(start_value["instance_model"], "single_instance");

    let status_output = cmd()
        .args(&sandbox)
        .args(["daemon", "status"])
        .output()
        .expect("failed to execute");
    assert!(status_output.status.success(), "expected exit 0");
    let status_stdout = String::from_utf8(status_output.stdout).expect("non-utf8 output");
    let status_value: serde_yaml::Value =
        serde_yaml::from_str(&status_stdout).expect("stdout should be valid YAML");
    assert_eq!(status_value["state"], "running");
    assert_eq!(status_value["recommended_next_action"], "status");

    let stop_output = cmd()
        .args(&sandbox)
        .args(["daemon", "stop"])
        .output()
        .expect("failed to execute");
    assert!(stop_output.status.success(), "expected exit 0");
    let stop_stdout = String::from_utf8(stop_output.stdout).expect("non-utf8 output");
    let stop_value: serde_yaml::Value =
        serde_yaml::from_str(&stop_stdout).expect("stdout should be valid YAML");
    assert_eq!(stop_value["action"], "stop");
    assert_eq!(stop_value["result"], "stopped");
    assert_eq!(stop_value["state"], "stopped");
}

#[test]
fn test_match_help_documents_subcommands() {
    let output = cmd()
        .args(["match", "--help"])
        .output()
        .expect("failed to execute");

    assert!(output.status.success(), "expected exit 0");
    let stdout = String::from_utf8(output.stdout).expect("non-utf8 output");
    assert!(stdout.contains("match <live|show|recent>"));
    assert!(stdout.contains("live"));
    assert!(stdout.contains("show"));
    assert!(stdout.contains("recent"));
}

#[test]
fn test_structured_help_for_match_live() {
    let output = cmd()
        .args(["help", "match", "live", "--format", "yaml"])
        .output()
        .expect("failed to execute");

    assert!(output.status.success(), "expected exit 0");
    let stdout = String::from_utf8(output.stdout).expect("non-utf8 output");
    let value: serde_yaml::Value =
        serde_yaml::from_str(&stdout).expect("stdout should be valid YAML");

    assert_eq!(value["command_path"][0], "match");
    assert_eq!(value["command_path"][1], "live");
    let options = value["options"]
        .as_sequence()
        .expect("options should exist");
    assert!(options.iter().any(|option| option["name"] == "--freshness"));
    assert!(options.iter().any(|option| option["name"] == "--league-id"));
}

#[test]
fn test_match_live_returns_live_matches() {
    if skip_bind_tests() {
        return;
    }

    let mock = MockOpenDota::start();
    let temp_dir = TempDir::new().expect("temp dir");
    let sandbox = sandbox_args(&temp_dir);

    let output = cmd()
        .env("DOTA_AGENT_CLI_OPENDOTA_BASE_URL", &mock.base_url)
        .args(&sandbox)
        .args([
            "match",
            "live",
            "--source",
            "opendota",
            "--freshness",
            "live",
            "--format",
            "json",
        ])
        .output()
        .expect("failed to execute");

    assert!(output.status.success(), "expected exit 0");
    let stdout = String::from_utf8(output.stdout).expect("non-utf8 output");
    let value: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON");

    assert!(value["match_count"].as_u64().unwrap() > 0);
    assert_eq!(value["source"]["resolved_sources"][0], "opendota");
    let matches = value["matches"].as_array().expect("matches array");
    assert!(matches[0]["match_id"].as_i64().unwrap_or(0) > 0);
}

#[test]
fn test_match_show_returns_detail() {
    if skip_bind_tests() {
        return;
    }

    let mock = MockOpenDota::start();
    let temp_dir = TempDir::new().expect("temp dir");
    let sandbox = sandbox_args(&temp_dir);

    let output = cmd()
        .env("DOTA_AGENT_CLI_OPENDOTA_BASE_URL", &mock.base_url)
        .args(&sandbox)
        .args([
            "match",
            "show",
            "7890123456",
            "--source",
            "opendota",
            "--expand",
            "--format",
            "json",
        ])
        .output()
        .expect("failed to execute");

    assert!(output.status.success(), "expected exit 0");
    let stdout = String::from_utf8(output.stdout).expect("non-utf8 output");
    let value: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON");

    assert_eq!(value["match_id"], 7890123456_i64);
    assert_eq!(value["radiant_win"], true);
    assert!(value["player_count"].as_u64().unwrap() > 0);
    assert!(value["players"].is_array());
    assert!(value["picks_bans"].is_array());
}

#[test]
fn test_match_recent_returns_player_matches() {
    if skip_bind_tests() {
        return;
    }

    let mock = MockOpenDota::start();
    let temp_dir = TempDir::new().expect("temp dir");
    let sandbox = sandbox_args(&temp_dir);

    let output = cmd()
        .env("DOTA_AGENT_CLI_OPENDOTA_BASE_URL", &mock.base_url)
        .args(&sandbox)
        .args([
            "match",
            "recent",
            "--player-id",
            "1001",
            "--source",
            "opendota",
            "--format",
            "json",
        ])
        .output()
        .expect("failed to execute");

    assert!(output.status.success(), "expected exit 0");
    let stdout = String::from_utf8(output.stdout).expect("non-utf8 output");
    let value: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON");

    assert_eq!(value["player_id"], 1001);
    assert!(value["match_count"].as_u64().unwrap() > 0);
    assert!(value["matches"].is_array());
}

#[test]
fn test_match_recent_won_without_player_id_returns_error() {
    let temp_dir = TempDir::new().expect("temp dir");
    let sandbox = sandbox_args(&temp_dir);

    let output = cmd()
        .args(&sandbox)
        .args(["match", "recent", "--won", "--format", "json"])
        .output()
        .expect("failed to execute");

    assert!(!output.status.success(), "expected non-zero exit");
    let stderr = String::from_utf8(output.stderr).expect("non-utf8 stderr");
    let value: serde_json::Value =
        serde_json::from_str(&stderr).expect("stderr should be valid JSON");
    assert_eq!(value["code"], "match.unsupported_filter");
}
