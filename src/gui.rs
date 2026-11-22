use std::path::Path;
use std::process::Command;

use crate::detect::Backends;

const GTK4_UI_SCRIPT: &str = include_str!("gui.py");

pub fn has_gtk4_gui(backends: &Backends) -> bool {
    let Some(py) = backends.python3.as_deref() else {
        return false;
    };
    Command::new(py)
        .args([
            "-c",
            "import gi; gi.require_version('Gtk', '4.0'); from gi.repository import Gtk",
        ])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Launch the interactive shrink dialog. Prefers native GTK4 (+ Libadwaita when installed)
/// and falls back cleanly to kdialog or zenity on minimal setups.
pub fn launch_shrink_gui(
    convto_bin: &Path,
    files: &[String],
    notify: bool,
    backends: &Backends,
) -> Result<(), String> {
    if files.is_empty() {
        return Err("No files provided to shrink dialog".to_string());
    }
    if has_gtk4_gui(backends) {
        let py = backends.python3.as_deref().unwrap();
        let status = Command::new(py)
            .arg("-c")
            .arg(GTK4_UI_SCRIPT)
            .arg(convto_bin)
            .arg(if notify { "1" } else { "0" })
            .args(files)
            .status()
            .map_err(|e| format!("Failed to launch GTK4 shrink dialog: {e}"))?;
        if status.success() {
            return Ok(());
        }
        return Err("Shrink dialog exited with an error".to_string());
    }

    // Fallback: kdialog (KDE) or zenity (GNOME/XFCE) simple prompt.
    let choice = if let Some(kdialog) = backends.kdialog.as_deref() {
        let out = Command::new(kdialog)
            .args([
                "--title",
                "convto - Shrink & Optimize",
                "--inputbox",
                "Enter target size (e.g. 10MB, 500KB) or percentage (e.g. 50%):",
                "50%",
            ])
            .output()
            .map_err(|e| format!("Failed to run kdialog: {e}"))?;
        if !out.status.success() {
            return Ok(()); // cancelled by user
        }
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    } else if let Some(zenity) = backends.zenity.as_deref() {
        let out = Command::new(zenity)
            .args([
                "--entry",
                "--title=convto - Shrink & Optimize",
                "--text=Enter target size (e.g. 10MB, 500KB) or percentage (e.g. 50%):",
                "--entry-text=50%",
            ])
            .output()
            .map_err(|e| format!("Failed to run zenity: {e}"))?;
        if !out.status.success() {
            return Ok(());
        }
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    } else {
        return Err(
            "Interactive shrink UI requires python3 + GTK4 (python-gobject), kdialog, or zenity"
                .to_string(),
        );
    };

    if choice.is_empty() {
        return Ok(());
    }

    let mut cmd = Command::new(convto_bin);
    cmd.arg("shrink");
    if notify {
        cmd.arg("--notify");
    }
    if let Some(pct) = choice.strip_suffix('%') {
        cmd.args(["--percent", pct.trim()]);
    } else {
        cmd.args(["--target-size", &choice]);
    }
    cmd.arg("--").args(files);
    let st = cmd
        .status()
        .map_err(|e| format!("Failed to run convto shrink: {e}"))?;
    if st.success() {
        Ok(())
    } else {
        Err("Shrink failed".to_string())
    }
}
