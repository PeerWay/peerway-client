#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

// ¿Con que viendo el código fuente? No hay nada que ocultar aquí.
// La ingeniería inversa es bienvenida para aprender e interoperar; solo
// pedimos no usarla para saltarse licencias, robar claves o romper la
// privacidad de otras personas. El cliente podría abrirse en el futuro.

use anyhow::{Context, Result};
use peerway_core::code;
use peerway_node::config::{load_config, Paths};
use peerway_node::local_api;
use peerway_node::node::Node;
use peerway_node::state::{TransferView, UiEvent};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::window::Color;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_updater::UpdaterExt;
use tokio::runtime::Runtime;

struct Engine {
    _runtime: Mutex<Option<Runtime>>,
}

#[derive(Clone, serde::Serialize)]
struct UpdateInfo {
    version: String,
    security: bool,
}

struct UpdateMenu {
    open: MenuItem<tauri::Wry>,
    check: MenuItem<tauri::Wry>,
    update: MenuItem<tauri::Wry>,
    quit: MenuItem<tauri::Wry>,
    pending: Mutex<Option<UpdateInfo>>,
}

static UPDATE_RUNNING: AtomicBool = AtomicBool::new(false);
static MANUAL_UPDATE_CHECK: OnceLock<Mutex<Option<(Instant, Option<UpdateInfo>)>>> =
    OnceLock::new();

fn manual_update_check() -> &'static Mutex<Option<(Instant, Option<UpdateInfo>)>> {
    MANUAL_UPDATE_CHECK.get_or_init(|| Mutex::new(None))
}

fn main() {
    #[cfg(feature = "store")]
    std::env::set_var("PEERWAY_STORE", "1");
    #[cfg(not(feature = "store"))]
    apply_saved_autostart();
    let args: Vec<String> = std::env::args().collect();
    let minimized = args.iter().any(|arg| arg == "--minimized");
    let launch_code = deep_link_code(&args);

    let result = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            show_main(app);
            if let Some(code) = deep_link_code(&args) {
                open_code(app, &code);
            }
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            install_update,
            check_updates_now,
            pending_update,
            notify_native,
            refresh_tray,
            window_minimize,
            window_toggle_maximize,
            window_is_maximized,
            window_close
        ])
        .setup(move |app| {
            setup_tray(app)?;

            match start_backend(app.handle().clone()) {
                Ok((port, runtime)) => {
                    app.manage(Engine {
                        _runtime: Mutex::new(runtime),
                    });
                    let base = ui_url(port);
                    let url = match &launch_code {
                        Some(found) => {
                            let separator = if base.contains('?') { '&' } else { '?' };
                            format!("{base}{separator}to={}", code::format(found))
                        }
                        None => base,
                    };
                    let parsed = url
                        .parse()
                        .with_context(|| format!("URL de interfaz inválida: {url}"))?;
                    build_window(app, WebviewUrl::External(parsed), minimized)?;
                }
                Err(error) => {
                    log_error(&format!("{error:#}"));
                    let window =
                        build_window(app, WebviewUrl::App("desktop-error.html".into()), true)?;
                    let message = serde_json::to_string(&format!("{error:#}"))
                        .unwrap_or_else(|_| "\"Error desconocido\"".into());
                    let _ = window.eval(&format!(
                        "document.getElementById('message').textContent = {message};"
                    ));
                    let _ = window.show();
                }
            }
            #[cfg(not(feature = "store"))]
            spawn_updater(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!());

    if let Err(error) = result {
        log_error(&format!("Tauri: {error}"));
    }
}

/// The autostart entry is written by the node when the setting is saved. On
/// startup we reconcile it with the persisted preference, which also handles
/// the first run after installing a new version.
#[cfg(not(feature = "store"))]
fn apply_saved_autostart() {
    let Ok(paths) = Paths::discover() else {
        return;
    };
    let Ok(config) = load_config(&paths.config) else {
        return;
    };
    if let Err(error) = peerway_node::autostart::apply(config.autostart_enabled) {
        log_error(&format!("inicio automático: {error:#}"));
    }
}

/// Native window/webview backdrop. Matching the interface palette avoids a
/// white flash in dark mode and a dark flash in light mode before the first
/// paint, and keeps resizes seamless.
const WINDOW_BACKGROUND_DARK: Color = Color(12, 13, 15, 255);
const WINDOW_BACKGROUND_LIGHT: Color = Color(244, 245, 247, 255);

fn sync_window_background(window: &tauri::WebviewWindow) {
    let color = match window.theme() {
        Ok(tauri::Theme::Light) => WINDOW_BACKGROUND_LIGHT,
        _ => WINDOW_BACKGROUND_DARK,
    };
    let _ = window.set_background_color(Some(color));
}

fn build_window(
    app: &tauri::App,
    url: WebviewUrl,
    minimized: bool,
) -> Result<tauri::WebviewWindow> {
    let window = WebviewWindowBuilder::new(app, "main", url)
        .title("PeerWay")
        .inner_size(1180.0, 800.0)
        .min_inner_size(900.0, 600.0)
        .decorations(false)
        .visible(false)
        .disable_drag_drop_handler()
        // The window is a webview with no address bar: never let a link
        // replace the local UI with an arbitrary site. External links open in
        // the system browser instead.
        .on_navigation(|url| {
            let local = matches!(url.scheme(), "tauri" | "about")
                || (matches!(url.scheme(), "http" | "https")
                    && matches!(
                        url.host_str(),
                        Some("127.0.0.1") | Some("localhost") | Some("tauri.localhost")
                    ));
            if local {
                return true;
            }
            if matches!(url.scheme(), "http" | "https" | "mailto") {
                let _ = open::that(url.as_str());
            }
            false
        })
        .build()
        .context("creando la ventana de PeerWay")?;

    // Set the native backdrop from the OS preference while the window is still
    // hidden, then reveal it, so light and dark users never see the opposite
    // theme flash before the webview paints.
    sync_window_background(&window);
    if !minimized {
        window.show().context("mostrando la ventana de PeerWay")?;
    }

    let handle = window.clone();
    window.on_window_event(move |event| match event {
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            let _ = handle.hide();
        }
        WindowEvent::ThemeChanged(_) => sync_window_background(&handle),
        _ => {}
    });
    Ok(window)
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let english = ui_language() == "en";
    let open = MenuItem::with_id(
        app,
        "open",
        if english {
            "Open PeerWay"
        } else {
            "Abrir PeerWay"
        },
        true,
        None::<&str>,
    )?;
    let check = MenuItem::with_id(
        app,
        "check_updates",
        if english {
            "Check for updates"
        } else {
            "Buscar actualizaciones"
        },
        true,
        None::<&str>,
    )?;
    let update = MenuItem::with_id(
        app,
        "install_update",
        if english {
            "Update…"
        } else {
            "Actualizar…"
        },
        false,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(
        app,
        "quit",
        if english { "Quit" } else { "Salir" },
        true,
        None::<&str>,
    )?;
    #[cfg(feature = "store")]
    let menu = Menu::with_items(app, &[&open, &quit])?;
    #[cfg(not(feature = "store"))]
    let menu = Menu::with_items(app, &[&open, &check, &update, &quit])?;
    app.manage(UpdateMenu {
        open,
        check,
        update,
        quit,
        pending: Mutex::new(None),
    });

    let mut builder = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("PeerWay")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "check_updates" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = check_updates(&app, false, true).await;
                });
            }
            "install_update" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = check_updates(&app, true, true).await;
                });
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }
    builder.build(app)?;
    Ok(())
}

fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Local UI URL with the per-run token. Another OS user cannot read the token
/// file, so knowing the loopback port is not enough to open the interface.
fn ui_url(port: u16) -> String {
    let token = Paths::discover()
        .ok()
        .and_then(|paths| std::fs::read_to_string(&paths.local_token).ok())
        .map(|token| token.trim().to_string())
        .unwrap_or_default();
    if token.is_empty() {
        format!("http://127.0.0.1:{port}/")
    } else {
        format!("http://127.0.0.1:{port}/?token={token}")
    }
}

/// Extract a code from `peerway://CODE` or `peerway://open?code=CODE`.
fn deep_link_code(args: &[String]) -> Option<String> {
    for arg in args {
        let value = arg.trim();
        let Some(rest) = value.strip_prefix("peerway://") else {
            continue;
        };
        let rest = rest.trim_start_matches('/');
        let candidate = rest
            .split_once("code=")
            .map(|(_, tail)| tail)
            .unwrap_or(rest);
        let candidate = candidate.split(['&', '#', '?']).next().unwrap_or_default();
        if let Some(normalized) = code::normalize(candidate) {
            return Some(normalized);
        }
    }
    None
}

/// Show the window already pointed at a code, so the send form is prefilled.
fn open_code(app: &AppHandle, raw: &str) {
    show_main(app);
    let port = Paths::discover()
        .ok()
        .and_then(|paths| load_config(&paths.config).ok())
        .map(|config| config.ui_port)
        .unwrap_or(8787);
    if let Some(window) = app.get_webview_window("main") {
        let url = format!("http://127.0.0.1:{port}/?to={}", code::format(raw));
        if let Ok(parsed) = url.parse() {
            let _ = window.navigate(parsed);
        }
    }
}

fn ui_language() -> String {
    Paths::discover()
        .ok()
        .and_then(|paths| load_config(&paths.config).ok())
        .map(|config| config.language)
        .unwrap_or_else(|| "es".into())
}

#[tauri::command]
async fn install_update(app: AppHandle) -> Result<(), String> {
    if cfg!(feature = "store") {
        return Err("las actualizaciones se instalan desde Microsoft Store".into());
    }
    if UPDATE_RUNNING.load(Ordering::SeqCst) {
        return Err("ya hay una comprobación de actualizaciones en curso".into());
    }
    check_updates(&app, true, true)
        .await
        .map(|_| ())
        .map_err(|error| format!("{error:#}"))
}

#[tauri::command]
async fn check_updates_now(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    if cfg!(feature = "store") {
        return Ok(None);
    }
    // Manual checks are intentionally quiet for 30 seconds. Repeating the
    // action returns the same result instead of hammering the update server,
    // which keeps the button responsive without showing a countdown.
    if let Some((checked_at, result)) = manual_update_check().lock().unwrap().clone() {
        if checked_at.elapsed() < Duration::from_secs(30) {
            if let Some(info) = result.as_ref() {
                announce_update(&app, info);
            }
            return Ok(result);
        }
    }
    let result = check_updates(&app, false, true)
        .await
        .map_err(|error| format!("{error:#}"))?;
    *manual_update_check().lock().unwrap() = Some((Instant::now(), result.clone()));
    Ok(result)
}

#[tauri::command]
fn pending_update(state: tauri::State<'_, UpdateMenu>) -> Option<UpdateInfo> {
    if cfg!(feature = "store") {
        return None;
    }
    state.pending.lock().unwrap().clone()
}

/// Native Windows/macOS/Linux notification for events that happen while the
/// window is hidden in the tray. The web UI decides when it is useful.
#[tauri::command]
fn notify_native(app: AppHandle, title: String, body: String) -> Result<(), String> {
    use tauri_plugin_notification::NotificationExt;
    app.notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn window_minimize(window: tauri::WebviewWindow) -> Result<(), String> {
    window.minimize().map_err(|error| error.to_string())
}

#[tauri::command]
fn window_toggle_maximize(window: tauri::WebviewWindow) -> Result<bool, String> {
    let maximized = window.is_maximized().map_err(|error| error.to_string())?;
    if maximized {
        window.unmaximize().map_err(|error| error.to_string())?;
    } else {
        window.maximize().map_err(|error| error.to_string())?;
    }
    Ok(!maximized)
}

#[tauri::command]
fn window_is_maximized(window: tauri::WebviewWindow) -> Result<bool, String> {
    window.is_maximized().map_err(|error| error.to_string())
}

#[tauri::command]
fn window_close(window: tauri::WebviewWindow) -> Result<(), String> {
    window.close().map_err(|error| error.to_string())
}

#[tauri::command]
fn refresh_tray(app: AppHandle) {
    let english = ui_language() == "en";
    if let Some(state) = app.try_state::<UpdateMenu>() {
        let _ = state.open.set_text(if english {
            "Open PeerWay"
        } else {
            "Abrir PeerWay"
        });
        let _ = state.check.set_text(if english {
            "Check for updates"
        } else {
            "Buscar actualizaciones"
        });
        let _ = state.quit.set_text(if english { "Quit" } else { "Salir" });
        let pending = state.pending.lock().unwrap().clone();
        let has_pending = pending.is_some();
        let label = match pending {
            Some(info) if english && info.security => {
                format!("Security update to {}", info.version)
            }
            Some(info) if english => format!("Update to {}", info.version),
            Some(info) if info.security => {
                format!("Actualización de seguridad a {}", info.version)
            }
            Some(info) => format!("Actualizar a {}", info.version),
            None if english => "Update…".into(),
            None => "Actualizar…".into(),
        };
        let _ = state.update.set_text(label);
        let _ = state.update.set_enabled(has_pending);
    }
}

fn start_backend(app: AppHandle) -> Result<(u16, Option<Runtime>)> {
    let paths = Paths::discover()?;
    if let Some((port, _token)) = running_backend(&paths) {
        return Ok((port, None));
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("creando runtime de Tokio")?;
    let node = runtime.block_on(async {
        let node = Node::new(paths.clone()).await?;
        node.start();
        Ok::<_, anyhow::Error>(node)
    })?;
    let preferred_port = node.config().ui_port;
    // Native notifications come from the engine, not the webview: the window
    // may be hidden in the tray and its JavaScript can be paused.
    let mut events = node.subscribe();
    let notify_app = app.clone();
    runtime.spawn(async move {
        loop {
            match events.recv().await {
                Ok(UiEvent::Incoming { transfer }) => notify_incoming(&notify_app, &transfer),
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
    });
    let ui_node = node.clone();
    runtime.spawn(async move {
        if let Err(error) = local_api::serve(ui_node, preferred_port).await {
            log_error(&format!("UI local: {error:#}"));
        }
    });

    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some((port, _token)) = running_backend(&paths) {
            return Ok((port, Some(runtime)));
        }
        if Instant::now() >= deadline {
            anyhow::bail!("el motor local no respondió en 15 segundos");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn running_backend(paths: &Paths) -> Option<(u16, String)> {
    let token = std::fs::read_to_string(&paths.local_token)
        .ok()?
        .trim()
        .to_string();
    let port: u16 = std::fs::read_to_string(&paths.ui_port_file)
        .ok()?
        .trim()
        .parse()
        .ok()?;
    if token.is_empty() || !api_ping(port) {
        return None;
    }
    Some((port, token))
}

fn api_ping(port: u16) -> bool {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_millis(500)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(700)));
    let request =
        b"GET /api/ping HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nAccept: application/json\r\n\r\n";
    if stream.write_all(request).is_err() {
        return false;
    }
    let mut response = String::new();
    let _ = stream.read_to_string(&mut response);
    response.starts_with("HTTP/1.1 200") && response.contains("peerway")
}

fn log_error(message: &str) {
    eprintln!("{message}");
    if let Ok(paths) = Paths::discover() {
        let path = paths.home.join("desktop.log");
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(file, "{} {message}", chrono::Utc::now().to_rfc3339());
        }
    }
}

#[cfg(not(feature = "store"))]
fn spawn_updater(app: AppHandle) {
    if cfg!(feature = "store") {
        return;
    }
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(45)).await;
        let mut failures = 0u32;
        loop {
            let auto = Paths::discover()
                .ok()
                .and_then(|paths| load_config(&paths.config).ok())
                .map(|config| config.auto_update)
                .unwrap_or(true);
            let result = if auto {
                check_updates(&app, false, false).await
            } else {
                Ok(None)
            };
            let wait = if result.is_err() {
                // Retry soon after transient network/manifest failures and back
                // off to the normal six-hour interval once it works again.
                failures = failures.saturating_add(1).min(4);
                match failures {
                    1 => 5 * 60,
                    2 => 15 * 60,
                    3 => 60 * 60,
                    _ => 6 * 60 * 60,
                }
            } else {
                failures = 0;
                6 * 60 * 60
            };
            tokio::time::sleep(Duration::from_secs(wait)).await;
        }
    });
}

async fn check_updates(
    app: &AppHandle,
    install: bool,
    announce: bool,
) -> Result<Option<UpdateInfo>> {
    if cfg!(feature = "store") {
        return Ok(None);
    }
    if UPDATE_RUNNING.swap(true, Ordering::SeqCst) {
        return Ok(None);
    }
    let result = async {
        let updater = app.updater().context("inicializando updater")?;
        let Some(update) = updater.check().await.context("consultando actualización")? else {
            if let Some(state) = app.try_state::<UpdateMenu>() {
                *state.pending.lock().unwrap() = None;
                let _ = state.update.set_enabled(false);
            }
            refresh_tray(app.clone());
            if announce {
                let version = app.package_info().version.to_string();
                let message = if ui_language() == "en" {
                    format!("PeerWay is up to date (v{version}). No newer version has been published yet.")
                } else {
                    format!("PeerWay ya está actualizado (v{version}). Aún no hay una versión más nueva publicada.")
                };
                notify_user(app, &message, "info");
            }
            return Ok(None);
        };
        let info = UpdateInfo {
            version: update.version.clone(),
            security: update
                .raw_json
                .get("security")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
        };
        if let Some(state) = app.try_state::<UpdateMenu>() {
            *state.pending.lock().unwrap() = Some(info.clone());
            let label = if ui_language() == "en" {
                if info.security {
                    format!("Security update to {}", info.version)
                } else {
                    format!("Update to {}", info.version)
                }
            } else if info.security {
                format!("Actualización de seguridad a {}", info.version)
            } else {
                format!("Actualizar a {}", info.version)
            };
            let _ = state.update.set_text(label);
            let _ = state.update.set_enabled(true);
        }
        if install {
            notify_user(app, "Instalando actualización…", "info");
            update
                .download_and_install(|_, _| {}, || {})
                .await
                .context("instalando actualización")?;
            app.exit(0);
        } else {
            announce_update(app, &info);
        }
        Ok(Some(info))
    }
    .await;
    UPDATE_RUNNING.store(false, Ordering::SeqCst);
    if let Err(error) = &result {
        let raw = format!("{error:#}");
        log_error(&format!("updater: {raw}"));
        if announce {
            let signature = raw.to_ascii_lowercase().contains("signature")
                || raw.to_ascii_lowercase().contains("firma");
            let message = if ui_language() == "en" {
                if !install {
                    "Could not check for updates. Check your connection and try again."
                } else if signature {
                    "The update signature does not match this installation. Download the latest version manually."
                } else {
                    "The update could not be installed. Download the latest version manually."
                }
            } else if !install {
                "No se pudo buscar la actualización. Revisa tu conexión e inténtalo de nuevo."
            } else if signature {
                "La firma de la actualización no coincide con esta instalación. Descarga la última versión manualmente."
            } else {
                "No se pudo instalar la actualización. Descarga la última versión manualmente."
            };
            notify_user(app, message, "error");
            announce_update_error(app, message);
        }
    }
    result
}

fn notify_user(app: &AppHandle, message: &str, level: &str) {
    if let Some(window) = app.get_webview_window("main") {
        let message = serde_json::to_string(message).unwrap_or_else(|_| "\"\"".into());
        let level = serde_json::to_string(level).unwrap_or_else(|_| "\"info\"".into());
        let _ = window.eval(&format!(
            "window.dispatchEvent(new CustomEvent('peerway-toast', {{ detail: {{ message: {message}, level: {level} }} }}));"
        ));
    }
}

fn notify_incoming(app: &AppHandle, transfer: &TransferView) {
    // The in-app banner is enough while the window is visible.
    let visible = app
        .get_webview_window("main")
        .map(|window| window.is_visible().unwrap_or(false))
        .unwrap_or(false);
    if visible {
        return;
    }
    let config = Paths::discover()
        .ok()
        .and_then(|paths| load_config(&paths.config).ok());
    if let Some(config) = config {
        if !config.notifications_enabled || !config.notify_transfers {
            return;
        }
    }
    let count = transfer.files.len();
    let body = if transfer.status == "transferring" {
        format!(
            "Recibiendo {} archivo(s) de {} (contacto de confianza)",
            count, transfer.peer_name
        )
    } else {
        format!(
            "{} quiere enviarte {} archivo(s)",
            transfer.peer_name, count
        )
    };
    use tauri_plugin_notification::NotificationExt;
    let _ = app
        .notification()
        .builder()
        .title("PeerWay")
        .body(body)
        .show();
}

fn announce_update(app: &AppHandle, info: &UpdateInfo) {
    if let Some(window) = app.get_webview_window("main") {
        let version = serde_json::to_string(&info.version).unwrap_or_else(|_| "\"\"".into());
        let security = info.security;
        let _ = window.eval(&format!(
            "window.dispatchEvent(new CustomEvent('peerway-update', {{ detail: {{ version: {version}, security: {security} }} }}));"
        ));
    }
}

fn announce_update_error(app: &AppHandle, message: &str) {
    if let Some(window) = app.get_webview_window("main") {
        let message = serde_json::to_string(message).unwrap_or_else(|_| "\"\"".into());
        let _ = window.eval(&format!(
            "window.dispatchEvent(new CustomEvent('peerway-update-error', {{ detail: {{ message: {message} }} }}));"
        ));
    }
}
