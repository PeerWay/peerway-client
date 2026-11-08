//! Installation and auto-start helpers.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub fn install(current_exe: &Path) -> Result<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let base = std::env::var("LOCALAPPDATA").context("LOCALAPPDATA no está definido")?;
        let dir = PathBuf::from(base).join("PeerWay");
        std::fs::create_dir_all(&dir)?;
        let target = dir.join("PeerWay.exe");
        if current_exe.canonicalize().ok() != target.canonicalize().ok() {
            std::fs::copy(current_exe, &target)
                .with_context(|| format!("copiando a {}", target.display()))?;
        }
        let start_menu = PathBuf::from(std::env::var("APPDATA")?)
            .join("Microsoft\\Windows\\Start Menu\\Programs");
        std::fs::create_dir_all(&start_menu)?;
        let lnk = start_menu.join("PeerWay.lnk");
        let script = format!(
            "$s=(New-Object -ComObject WScript.Shell).CreateShortcut('{}'); $s.TargetPath='{}'; $s.Arguments='run'; $s.WorkingDirectory='{}'; $s.IconLocation='{},0'; $s.Save()",
            escape_ps(&lnk.to_string_lossy()),
            escape_ps(&target.to_string_lossy()),
            escape_ps(&dir.to_string_lossy()),
            escape_ps(&target.to_string_lossy())
        );
        let mut command = std::process::Command::new("powershell");
        command.args(["-NoProfile", "-Command", &script]);
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let status = command.status().context("creando acceso directo")?;
        if !status.success() {
            anyhow::bail!("no se pudo crear el acceso directo del menú inicio");
        }
        Ok(target)
    }
    #[cfg(target_os = "linux")]
    {
        let home = dirs::home_dir().context("no hay home")?;
        let bin = home.join(".local/bin");
        std::fs::create_dir_all(&bin)?;
        let target = bin.join("peerway");
        if current_exe.canonicalize().ok() != target.canonicalize().ok() {
            std::fs::copy(current_exe, &target)?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&target)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&target, perms)?;
        }
        let apps = home.join(".local/share/applications");
        std::fs::create_dir_all(&apps)?;
        let desktop = format!(
            "[Desktop Entry]\nType=Application\nName=PeerWay\nComment=Transferencia P2P de archivos\nExec=\"{}\" run\nTerminal=false\nCategories=Network;FileTransfer;\n",
            target.display()
        );
        std::fs::write(apps.join("peerway.desktop"), desktop)?;
        Ok(target)
    }
    #[cfg(target_os = "macos")]
    {
        let home = dirs::home_dir().context("no hay home")?;
        let bin = home.join(".local/bin");
        std::fs::create_dir_all(&bin)?;
        let target = bin.join("peerway");
        if current_exe.canonicalize().ok() != target.canonicalize().ok() {
            std::fs::copy(current_exe, &target)?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&target)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&target, perms)?;
        }
        Ok(target)
    }
}

fn escape_ps(value: &str) -> String {
    value.replace('\'', "''")
}

pub fn enable_autostart(exe: &Path) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = hkcu.create_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run")?;
        let command = format!("\"{}\" daemon --no-open", exe.display());
        key.set_value("PeerWay", &command)?;
        let _ = KEY_SET_VALUE;
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        let home = dirs::home_dir().context("no hay home")?;
        let dir = home.join(".config/autostart");
        std::fs::create_dir_all(&dir)?;
        let entry = format!(
            "[Desktop Entry]\nType=Application\nName=PeerWay\nComment=Inicia PeerWay en segundo plano\nExec=\"{}\" daemon --no-open\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
            exe.display()
        );
        std::fs::write(dir.join("peerway.desktop"), entry)?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        let home = dirs::home_dir().context("no hay home")?;
        let dir = home.join("Library/LaunchAgents");
        std::fs::create_dir_all(&dir)?;
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>app.peerway.node</string>
<key>ProgramArguments</key><array><string>{}</string><string>daemon</string><string>--no-open</string></array>
<key>RunAtLoad</key><true/>
<key>KeepAlive</key><false/>
</dict></plist>"#,
            exe.display()
        );
        std::fs::write(dir.join("app.peerway.node.plist"), plist)?;
        Ok(())
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        let _ = exe;
        anyhow::bail!("auto-inicio no soportado en esta plataforma")
    }
}

pub fn disable_autostart() -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let key = hkcu.open_subkey_with_flags(
            r"Software\Microsoft\Windows\CurrentVersion\Run",
            KEY_SET_VALUE,
        )?;
        let _ = key.delete_value("PeerWay");
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        let home = dirs::home_dir().context("no hay home")?;
        let _ = std::fs::remove_file(home.join(".config/autostart/peerway.desktop"));
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        let home = dirs::home_dir().context("no hay home")?;
        let _ = std::fs::remove_file(home.join("Library/LaunchAgents/app.peerway.node.plist"));
        Ok(())
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        anyhow::bail!("auto-inicio no soportado en esta plataforma")
    }
}
