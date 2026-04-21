//! Daemon IPC server using Unix Domain Sockets.
//! Provides a persistent background process that handles commands via socket protocol.

use crate::context::RuntimeLocations;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{Mutex, oneshot};

/// Global daemon handle storage
static DAEMON_HANDLE: std::sync::OnceLock<Arc<Mutex<Option<DaemonHandle>>>> =
    std::sync::OnceLock::new();

/// Handle to control a running daemon server
pub struct DaemonHandle {
    pub socket_path: PathBuf,
    pub shutdown_tx: oneshot::Sender<()>,
}

/// IPC message from CLI to daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcRequest {
    pub command: String,
    pub args: BTreeMap<String, String>,
    pub request_id: String,
}

/// IPC response from daemon to CLI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcResponse {
    pub request_id: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<IpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcError {
    pub code: String,
    pub message: String,
}

/// Daemon server state shared across connections
pub struct DaemonState {
    pub runtime: RuntimeLocations,
    pub shutdown_tx: Option<oneshot::Sender<()>>,
}

impl DaemonState {
    pub fn new(runtime: RuntimeLocations) -> Self {
        Self {
            runtime,
            shutdown_tx: None,
        }
    }
}

fn get_daemon_handle() -> &'static Arc<Mutex<Option<DaemonHandle>>> {
    DAEMON_HANDLE.get_or_init(|| Arc::new(Mutex::new(None)))
}

/// Start the daemon server with Unix socket listener
pub async fn start_daemon_server(
    runtime: RuntimeLocations,
    socket_path: PathBuf,
    shutdown_rx: oneshot::Receiver<()>,
) {
    // Remove existing socket file if present
    if socket_path.exists()
        && let Err(e) = std::fs::remove_file(&socket_path)
    {
        eprintln!("daemon: failed to remove existing socket: {}", e);
    }

    // Create parent directory
    if let Some(parent) = socket_path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        eprintln!("daemon: failed to create socket directory: {}", e);
        return;
    }

    let listener = match UnixListener::bind(&socket_path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("daemon: failed to bind Unix socket: {}", e);
            return;
        }
    };

    let state = Arc::new(Mutex::new(Some(DaemonState::new(runtime))));
    let state_clone = state.clone();

    tokio::spawn(async move {
        let mut shutdown_rx = shutdown_rx;
        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, _)) => {
                            let state = state_clone.clone();
                            tokio::spawn(handle_connection(stream, state));
                        }
                        Err(e) => {
                            eprintln!("daemon: accept error: {}", e);
                        }
                    }
                }
                _ = &mut shutdown_rx => {
                    println!("daemon: shutdown signal received");
                    break;
                }
            }
        }
    });

    // Set socket permissions
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) =
            std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600))
        {
            eprintln!("daemon: warning: failed to set socket permissions: {}", e);
        }
    }

    println!("daemon: listening on {}", socket_path.display());
}

/// Handle a single client connection
async fn handle_connection(stream: UnixStream, state: Arc<Mutex<Option<DaemonState>>>) {
    let (reader, mut writer) = tokio::io::split(stream);
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => break, // EOF
            Ok(_) => {}
            Err(e) => {
                eprintln!("daemon: read error: {}", e);
                break;
            }
        }

        let request: IpcRequest = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                let response = IpcResponse {
                    request_id: String::new(),
                    success: false,
                    output: None,
                    error: Some(IpcError {
                        code: "parse_error".to_string(),
                        message: format!("failed to parse request: {}", e),
                    }),
                };
                let response_bytes =
                    format!("{}\n", serde_json::to_string(&response).unwrap_or_default());
                if let Err(e) = writer.write_all(response_bytes.as_bytes()).await {
                    eprintln!("daemon: write error: {}", e);
                }
                break;
            }
        };

        let response = process_request(&request, &state).await;
        let response_bytes = format!("{}\n", serde_json::to_string(&response).unwrap_or_default());
        if let Err(e) = writer.write_all(response_bytes.as_bytes()).await {
            eprintln!("daemon: write error: {}", e);
            break;
        }
    }
}

/// Process an IPC request and return response
async fn process_request(
    request: &IpcRequest,
    state: &Arc<Mutex<Option<DaemonState>>>,
) -> IpcResponse {
    let state_guard = state.lock().await;
    let daemon_state = match state_guard.as_ref() {
        Some(s) => s,
        None => {
            return IpcResponse {
                request_id: request.request_id.clone(),
                success: false,
                output: None,
                error: Some(IpcError {
                    code: "daemon_shutdown".to_string(),
                    message: "daemon is shutting down".to_string(),
                }),
            };
        }
    };

    // Process command based on type
    let result = match request.command.as_str() {
        "ping" => Ok(serde_json::json!({"status": "pong", "daemon": "running"})),
        "status" => Ok(serde_json::json!({
            "state": "running",
            "runtime": daemon_state.runtime.summary()
        })),
        "source_warm" => {
            // Import and call the source warm function
            let source = request
                .args
                .get("source")
                .and_then(|s| match s.as_str() {
                    "auto" => Some(crate::providers::ProviderSourceSelector::Auto),
                    "opendota" => Some(crate::providers::ProviderSourceSelector::Opendota),
                    "stratz" => Some(crate::providers::ProviderSourceSelector::Stratz),
                    _ => None,
                })
                .unwrap_or(crate::providers::ProviderSourceSelector::Auto);

            let scope = request
                .args
                .get("scope")
                .and_then(|s| match s.as_str() {
                    "indexes" => Some(crate::providers::WarmScope::Indexes),
                    "details" => Some(crate::providers::WarmScope::Details),
                    "all" => Some(crate::providers::WarmScope::All),
                    _ => None,
                })
                .unwrap_or(crate::providers::WarmScope::All);

            let force = request
                .args
                .get("force")
                .map(|s| s == "true")
                .unwrap_or(false);

            match crate::providers::source_warm(&daemon_state.runtime, source, scope, force) {
                Ok(output) => Ok(serde_json::to_value(output).unwrap_or_default()),
                Err(e) => Err(e),
            }
        }
        "cache_status" => {
            // Return cache status for opendota and stratz
            let mut results = Vec::new();

            // Check OpenDota cache
            let od_cache = daemon_state.runtime.cache_dir.join("live-providers");
            if od_cache.exists() {
                let hero_cache = od_cache.join("opendota-hero-stats.json");
                let item_cache = od_cache.join("opendota-items.json");
                results.push(serde_json::json!({
                    "provider": "opendota",
                    "hero_cache_exists": hero_cache.exists(),
                    "item_cache_exists": item_cache.exists(),
                }));
            }

            // Check STRATZ cache
            if od_cache.exists() {
                let stratz_hero_cache = od_cache.join("stratz-hero-stats.json");
                let stratz_item_cache = od_cache.join("stratz-items.json");
                results.push(serde_json::json!({
                    "provider": "stratz",
                    "hero_cache_exists": stratz_hero_cache.exists(),
                    "item_cache_exists": stratz_item_cache.exists(),
                }));
            }

            Ok(serde_json::json!({"caches": results}))
        }
        cmd => {
            return IpcResponse {
                request_id: request.request_id.clone(),
                success: false,
                output: None,
                error: Some(IpcError {
                    code: "unknown_command".to_string(),
                    message: format!("unknown daemon command: {}", cmd),
                }),
            };
        }
    };

    match result {
        Ok(output) => IpcResponse {
            request_id: request.request_id.clone(),
            success: true,
            output: Some(output),
            error: None,
        },
        Err(e) => IpcResponse {
            request_id: request.request_id.clone(),
            success: false,
            output: None,
            error: Some(IpcError {
                code: e.code().to_string(),
                message: e.message().to_string(),
            }),
        },
    }
}

/// Send a request to the daemon via Unix socket
pub async fn send_daemon_request(
    socket_path: &PathBuf,
    request: IpcRequest,
) -> Result<IpcResponse> {
    use tokio::io::{AsyncWriteExt, BufReader};
    use tokio::net::UnixStream;

    let stream = UnixStream::connect(socket_path).await?;
    let (reader, mut writer) = tokio::io::split(stream);

    // Send request
    let request_json = serde_json::to_string(&request)?;
    writer
        .write_all(format!("{}\n", request_json).as_bytes())
        .await?;

    // Read response
    let mut reader = BufReader::new(reader);
    let mut response_line = String::new();
    reader.read_line(&mut response_line).await?;

    let response: IpcResponse =
        serde_json::from_str(&response_line).context("failed to parse daemon response")?;

    Ok(response)
}

/// Start the daemon server in a background thread and store the handle
pub fn start_daemon(runtime: RuntimeLocations, socket_path: PathBuf) -> Result<()> {
    let handle = get_daemon_handle();
    let mut guard = handle.blocking_lock();
    if guard.is_some() {
        return Err(anyhow::anyhow!("daemon already running"));
    }

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let socket_path_clone = socket_path.clone();
    let runtime_clone = runtime.clone();

    thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to create Tokio runtime");

        rt.block_on(async {
            start_daemon_server(runtime_clone, socket_path_clone, shutdown_rx).await
        });
    });

    // Wait a bit for the server to start
    thread::sleep(std::time::Duration::from_millis(100));

    *guard = Some(DaemonHandle {
        socket_path,
        shutdown_tx,
    });

    Ok(())
}

/// Stop the daemon server
pub fn stop_daemon() -> Result<()> {
    let handle = get_daemon_handle();
    let mut guard = handle.blocking_lock();
    if let Some(dh) = guard.take() {
        let _ = dh.shutdown_tx.send(());
        // Remove socket file
        if dh.socket_path.exists() {
            std::fs::remove_file(&dh.socket_path).ok();
        }
    }
    Ok(())
}

/// Check if daemon is running
pub fn is_daemon_running() -> bool {
    let handle = get_daemon_handle();
    let guard = handle.blocking_lock();
    guard.is_some()
}

/// Get the daemon socket path if running
pub fn get_daemon_socket_path() -> Option<PathBuf> {
    let handle = get_daemon_handle();
    let guard = handle.blocking_lock();
    guard.as_ref().map(|dh| dh.socket_path.clone())
}
