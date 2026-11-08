use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use peerway_node::config::Paths;
use peerway_node::node::Node;
use peerway_node::{install, local_api};
use std::path::PathBuf;
use std::time::Duration;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "peerway",
    version,
    about = "PeerWay: transferencia de archivos P2P con códigos"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Abre PeerWay (inicia el nodo si hace falta).
    Run {
        /// No abrir el navegador.
        #[arg(long)]
        no_open: bool,
    },
    /// Ejecuta el nodo en segundo plano sin abrir la interfaz.
    Daemon {
        #[arg(long, hide = true)]
        no_open: bool,
    },
    /// Envía archivos a un código.
    Send {
        code: String,
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },
    /// Abre la interfaz de una instancia en ejecución.
    Open,
    /// Muestra tu código actual.
    Code,
    /// Muestra el estado del nodo.
    Status,
    /// Instala PeerWay en el perfil del usuario y activa el auto-inicio.
    Install,
    /// Gestiona el auto-inicio.
    Autostart {
        #[command(subcommand)]
        action: AutostartAction,
    },
}

#[derive(Subcommand)]
enum AutostartAction {
    Enable,
    Disable,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
    peerway_core::net::install_crypto_provider();

    match cli.command.unwrap_or(Command::Run { no_open: false }) {
        Command::Run { no_open } => run_daemon(!no_open).await,
        Command::Daemon { .. } => run_daemon(false).await,
        Command::Send { code, paths } => send_command(code, paths).await,
        Command::Open => {
            let base = ensure_daemon().await?;
            open_browser_url(&base);
            Ok(())
        }
        Command::Code => {
            let state = fetch_state().await?;
            println!(
                "{}",
                state
                    .get("code")
                    .and_then(|c| c.as_str())
                    .unwrap_or("(sin código)")
            );
            Ok(())
        }
        Command::Status => {
            let state = fetch_state().await?;
            print_status(&state);
            Ok(())
        }
        Command::Install => {
            let exe = std::env::current_exe()?;
            let installed = install::install(&exe)?;
            println!("PeerWay instalado en {}", installed.display());
            println!("Puedes activar el inicio automático en Ajustes.");
            Ok(())
        }
        Command::Autostart { action } => {
            let exe = std::env::current_exe()?;
            match action {
                AutostartAction::Enable => {
                    install::enable_autostart(&exe)?;
                    println!("Auto-inicio activado.");
                }
                AutostartAction::Disable => {
                    install::disable_autostart()?;
                    println!("Auto-inicio desactivado.");
                }
            }
            Ok(())
        }
    }
}

async fn run_daemon(open_browser: bool) -> Result<()> {
    let paths = Paths::discover()?;
    if let Some(port) = current_port(&paths) {
        if ping(port).await {
            let base = format!("http://127.0.0.1:{port}");
            tracing::info!("PeerWay ya está ejecutándose en {base}");
            if open_browser {
                open_browser_url(&base);
            }
            return Ok(());
        }
    }

    let node = Node::new(paths.clone()).await?;
    node.start();
    let config = node.config();
    let ui_port = config.ui_port;
    let node_for_ui = node.clone();
    tokio::spawn(async move {
        if let Err(error) = local_api::serve(node_for_ui, ui_port).await {
            tracing::error!(?error, "la interfaz local se detuvo");
        }
    });

    let base = wait_for_api(&paths, Duration::from_secs(10))
        .await
        .context("la interfaz local no arrancó")?;
    tracing::info!("código: {}", node.formatted_code());
    if open_browser {
        open_browser_url(&base);
    }
    println!("PeerWay en {base}");
    println!("Tu código: {}", node.formatted_code());

    tokio::signal::ctrl_c().await?;
    let _ = std::fs::remove_file(&paths.ui_port_file);
    let _ = std::fs::remove_file(&paths.local_token);
    Ok(())
}

fn open_browser_url(base: &str) {
    if open::that(base).is_err() {
        println!("Abre {base} en tu navegador.");
    }
}

async fn send_command(code: String, paths: Vec<PathBuf>) -> Result<()> {
    let base = ensure_daemon().await?;
    let token = read_local_token()?;
    let body = serde_json::json!({
        "code": code,
        "paths": paths.iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>()
    });
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{base}/api/send"))
        .header("x-peerway-token", token)
        .json(&body)
        .send()
        .await?;
    if !response.status().is_success() {
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!("{text}");
    }
    println!("Transferencia iniciada.");
    Ok(())
}

async fn fetch_state() -> Result<serde_json::Value> {
    let base = ensure_daemon().await?;
    let token = read_local_token()?;
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{base}/api/state"))
        .header("x-peerway-token", token)
        .send()
        .await?;
    if !response.status().is_success() {
        anyhow::bail!("no se pudo leer el estado del nodo");
    }
    Ok(response.json().await?)
}

fn print_status(state: &serde_json::Value) {
    let text = |key: &str| {
        state
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("-")
            .to_string()
    };
    println!("PeerWay {}", text("version"));
    println!("Código:     {}", text("code"));
    println!("Equipo:     {}", text("device_name"));
    println!(
        "Servidor:   {} ({})",
        text("server_url"),
        if state
            .get("server_online")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            "conectado"
        } else {
            "sin conexión"
        }
    );
    println!(
        "Cuenta:     {}",
        if state
            .get("logged_in")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            format!("{} ({})", text("username"), text("plan_label"))
        } else {
            "temporal".into()
        }
    );
    if let Some(peers) = state.get("lan_peers").and_then(|v| v.as_array()) {
        println!("LAN:        {} dispositivo(s)", peers.len());
    }
    if let Some(transfers) = state.get("transfers").and_then(|v| v.as_array()) {
        println!("Historial:  {} transferencia(s)", transfers.len());
    }
}

fn current_port(paths: &Paths) -> Option<u16> {
    std::fs::read_to_string(&paths.ui_port_file)
        .ok()?
        .trim()
        .parse()
        .ok()
}

async fn ping(port: u16) -> bool {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_millis(700))
        .build()
    {
        Ok(client) => client,
        Err(_) => return false,
    };
    client
        .get(format!("http://127.0.0.1:{port}/api/ping"))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

async fn wait_for_api(paths: &Paths, timeout: Duration) -> Result<String> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if let Some(port) = current_port(paths) {
            if ping(port).await {
                return Ok(format!("http://127.0.0.1:{port}"));
            }
        }
        if std::time::Instant::now() >= deadline {
            anyhow::bail!("tiempo de espera agotado");
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn ensure_daemon() -> Result<String> {
    let paths = Paths::discover()?;
    if let Some(port) = current_port(&paths) {
        if ping(port).await {
            return Ok(format!("http://127.0.0.1:{port}"));
        }
    }
    let exe = std::env::current_exe()?;
    let mut command = std::process::Command::new(exe);
    command
        .arg("daemon")
        .arg("--no-open")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    command
        .spawn()
        .context("iniciando PeerWay en segundo plano")?;
    wait_for_api(&paths, Duration::from_secs(15)).await
}

fn read_local_token() -> Result<String> {
    let paths = Paths::discover()?;
    std::fs::read_to_string(&paths.local_token)
        .map(|s| s.trim().to_string())
        .context("no se encontró el token local")
}
