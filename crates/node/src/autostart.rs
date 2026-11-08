//! Small cross-platform helper for the "start with the computer" setting.
//!
//! It never touches a development binary under `target/`, so running the app
//! from Cargo cannot leave a stale autostart entry behind.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub fn apply(enabled: bool) -> Result<()> {
    let exe = installed_executable();
    if enabled && exe.is_none() {
        return Ok(());
    }
    // When disabling, the path is unused; passing an empty path lets a dev
    // build remove an entry left by a previous installed build.
    let exe = exe.unwrap_or_default();
    #[cfg(windows)]
    {
        apply_windows(enabled, &exe)
    }
    #[cfg(target_os = "linux")]
    {
        apply_linux(enabled, &exe)
    }
    #[cfg(target_os = "macos")]
    {
        apply_macos(enabled, &exe)
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        let _ = (enabled, exe);
        Ok(())
    }
}

fn installed_executable() -> Option<PathBuf> {
    let exe = std::env::var("APPIMAGE")
        .ok()
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok())?;
    let path = exe.to_string_lossy().to_lowercase();
    if path.contains("peerway") && !path.contains("/target/") && !path.contains("\\target\\") {
        Some(exe)
    } else {
        None
    }
}

#[cfg(windows)]
fn apply_windows(enabled: bool, exe: &Path) -> Result<()> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu
        .open_subkey_with_flags(
            r"Software\Microsoft\Windows\CurrentVersion\Run",
            KEY_READ | KEY_WRITE,
        )
        .context("abriendo el registro de inicio automático")?;
    if enabled {
        let expected = format!("\"{}\" --minimized", exe.display());
        key.set_value("PeerWay", &expected)
            .context("guardando el inicio automático")?;
    } else {
        match key.delete_value("PeerWay") {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("quitando el inicio automático"),
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn apply_linux(enabled: bool, exe: &Path) -> Result<()> {
    let home = dirs::home_dir().context("no se pudo encontrar el directorio personal")?;
    let path = home.join(".config/autostart/peerway.desktop");
    if !enabled {
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("quitando el inicio automático"),
        }
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("creando la carpeta de inicio automático")?;
    }
    let entry = format!(
        "[Desktop Entry]\nType=Application\nName=PeerWay\nComment=Inicia PeerWay en la bandeja\nExec=\"{}\" --minimized\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
        exe.display()
    );
    std::fs::write(&path, entry).context("guardando el inicio automático")?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn apply_macos(enabled: bool, exe: &Path) -> Result<()> {
    let home = dirs::home_dir().context("no se pudo encontrar el directorio personal")?;
    let path = home.join("Library/LaunchAgents/io.peerway.desktop.plist");
    if !enabled {
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("quitando el inicio automático"),
        }
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("creando la carpeta de inicio automático")?;
    }
    let executable = xml_escape(&exe.to_string_lossy());
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>io.peerway.desktop</string>
<key>ProgramArguments</key><array><string>{executable}</string><string>--minimized</string></array>
<key>RunAtLoad</key><true/>
<key>KeepAlive</key><false/>
</dict></plist>"#
    );
    std::fs::write(&path, plist).context("guardando el inicio automático")?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
