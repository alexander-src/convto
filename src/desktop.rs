use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::detect::{find_in_path, Backends};
use crate::format::{Family, Format};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopTarget {
    Auto,
    Dolphin,
    Nautilus,
    Nemo,
    Thunar,
    All,
}

impl DesktopTarget {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "dolphin" | "kde" | "plasma" => Some(Self::Dolphin),
            "nautilus" | "gnome" | "files" => Some(Self::Nautilus),
            "nemo" | "cinnamon" => Some(Self::Nemo),
            "thunar" | "xfce" => Some(Self::Thunar),
            "all" => Some(Self::All),
            _ => None,
        }
    }
}

pub struct ActionSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub target: Format,
    pub icon: &'static str,
}

pub struct MenuGroup {
    pub slug: &'static str,
    pub family: Family,
    pub mime_types: &'static [&'static str],
    pub actions: &'static [ActionSpec],
}

pub const MENU_GROUPS: &[MenuGroup] = &[
    MenuGroup {
        slug: "image",
        family: Family::Image,
        mime_types: &[
            "image/png",
            "image/jpeg",
            "image/webp",
            "image/avif",
            "image/heif",
            "image/heic",
            "image/jxl",
            "image/bmp",
            "image/tiff",
            "image/vnd.microsoft.icon",
            "image/x-icon",
            "image/svg+xml",
            "image/x-tga",
            "image/vnd.adobe.photoshop",
            "image/qoi",
            "image/x-qoi",
            "image/x-portable-pixmap",
            "image/x-portable-graymap",
            "image/x-portable-bitmap",
        ],
        actions: &[
            ActionSpec {
                id: "png",
                label: "PNG (.png)",
                target: Format::Png,
                icon: "image-png",
            },
            ActionSpec {
                id: "jpg",
                label: "JPEG (.jpg)",
                target: Format::Jpg,
                icon: "image-jpeg",
            },
            ActionSpec {
                id: "webp",
                label: "WebP (.webp)",
                target: Format::Webp,
                icon: "image-x-generic",
            },
            ActionSpec {
                id: "avif",
                label: "AVIF (.avif)",
                target: Format::Avif,
                icon: "image-x-generic",
            },
            ActionSpec {
                id: "jxl",
                label: "JPEG XL (.jxl)",
                target: Format::Jxl,
                icon: "image-x-generic",
            },
            ActionSpec {
                id: "gif",
                label: "GIF (.gif)",
                target: Format::Gif,
                icon: "image-gif",
            },
            ActionSpec {
                id: "ico",
                label: "ICO (.ico)",
                target: Format::Ico,
                icon: "image-x-generic",
            },
            ActionSpec {
                id: "tiff",
                label: "TIFF (.tiff)",
                target: Format::Tiff,
                icon: "image-tiff",
            },
            ActionSpec {
                id: "bmp",
                label: "BMP (.bmp)",
                target: Format::Bmp,
                icon: "image-bmp",
            },
            ActionSpec {
                id: "pdf",
                label: "PDF (.pdf)",
                target: Format::Pdf,
                icon: "application-pdf",
            },
        ],
    },
    MenuGroup {
        slug: "gif",
        family: Family::Gif,
        mime_types: &["image/gif"],
        actions: &[
            ActionSpec {
                id: "mp4",
                label: "MP4 Video (.mp4)",
                target: Format::Mp4,
                icon: "video-x-generic",
            },
            ActionSpec {
                id: "webm",
                label: "WebM Video (.webm)",
                target: Format::Webm,
                icon: "video-x-generic",
            },
            ActionSpec {
                id: "webp",
                label: "WebP (.webp)",
                target: Format::Webp,
                icon: "image-x-generic",
            },
            ActionSpec {
                id: "png",
                label: "PNG Frame (.png)",
                target: Format::Png,
                icon: "image-png",
            },
            ActionSpec {
                id: "jpg",
                label: "JPEG Frame (.jpg)",
                target: Format::Jpg,
                icon: "image-jpeg",
            },
            ActionSpec {
                id: "avif",
                label: "AVIF (.avif)",
                target: Format::Avif,
                icon: "image-x-generic",
            },
        ],
    },
    MenuGroup {
        slug: "audio",
        family: Family::Audio,
        mime_types: &[
            "audio/mpeg",
            "audio/vnd.wave",
            "audio/wav",
            "audio/x-wav",
            "audio/flac",
            "audio/x-flac",
            "audio/ogg",
            "audio/opus",
            "audio/x-opus+ogg",
            "audio/mp4",
            "audio/x-m4a",
            "audio/aac",
            "audio/x-ms-wma",
            "audio/x-aiff",
            "audio/matroska",
        ],
        actions: &[
            ActionSpec {
                id: "mp3",
                label: "MP3 (.mp3)",
                target: Format::Mp3,
                icon: "audio-x-generic",
            },
            ActionSpec {
                id: "wav",
                label: "WAV (.wav)",
                target: Format::Wav,
                icon: "audio-x-generic",
            },
            ActionSpec {
                id: "flac",
                label: "FLAC (.flac)",
                target: Format::Flac,
                icon: "audio-x-generic",
            },
            ActionSpec {
                id: "ogg",
                label: "OGG Vorbis (.ogg)",
                target: Format::Ogg,
                icon: "audio-x-generic",
            },
            ActionSpec {
                id: "opus",
                label: "Opus (.opus)",
                target: Format::Opus,
                icon: "audio-x-generic",
            },
            ActionSpec {
                id: "m4a",
                label: "M4A / AAC (.m4a)",
                target: Format::M4a,
                icon: "audio-x-generic",
            },
        ],
    },
    MenuGroup {
        slug: "video",
        family: Family::Video,
        mime_types: &[
            "video/mp4",
            "video/x-matroska",
            "video/matroska",
            "video/webm",
            "video/quicktime",
            "video/vnd.avi",
            "video/x-msvideo",
            "video/x-flv",
            "video/x-ms-wmv",
            "video/mpeg",
            "video/3gpp",
            "video/mp2t",
        ],
        actions: &[
            ActionSpec {
                id: "mp4",
                label: "MP4 (.mp4)",
                target: Format::Mp4,
                icon: "video-x-generic",
            },
            ActionSpec {
                id: "mkv",
                label: "MKV (.mkv)",
                target: Format::Mkv,
                icon: "video-x-generic",
            },
            ActionSpec {
                id: "webm",
                label: "WebM (.webm)",
                target: Format::Webm,
                icon: "video-x-generic",
            },
            ActionSpec {
                id: "mov",
                label: "MOV (.mov)",
                target: Format::Mov,
                icon: "video-x-generic",
            },
            ActionSpec {
                id: "gif",
                label: "Animated GIF (.gif)",
                target: Format::Gif,
                icon: "image-gif",
            },
            ActionSpec {
                id: "mp3",
                label: "Extract MP3 Audio (.mp3)",
                target: Format::Mp3,
                icon: "audio-x-generic",
            },
            ActionSpec {
                id: "wav",
                label: "Extract WAV Audio (.wav)",
                target: Format::Wav,
                icon: "audio-x-generic",
            },
            ActionSpec {
                id: "opus",
                label: "Extract Opus Audio (.opus)",
                target: Format::Opus,
                icon: "audio-x-generic",
            },
        ],
    },
    MenuGroup {
        slug: "pdf",
        family: Family::Pdf,
        mime_types: &["application/pdf"],
        actions: &[
            ActionSpec {
                id: "png",
                label: "PNG Images (.png)",
                target: Format::Png,
                icon: "image-png",
            },
            ActionSpec {
                id: "jpg",
                label: "JPEG Images (.jpg)",
                target: Format::Jpg,
                icon: "image-jpeg",
            },
            ActionSpec {
                id: "webp",
                label: "WebP Images (.webp)",
                target: Format::Webp,
                icon: "image-x-generic",
            },
            ActionSpec {
                id: "txt",
                label: "Plain Text (.txt)",
                target: Format::Txt,
                icon: "text-plain",
            },
        ],
    },
    MenuGroup {
        slug: "data",
        family: Family::Data,
        mime_types: &[
            "application/json",
            "application/yaml",
            "application/x-yaml",
            "text/yaml",
            "text/x-yaml",
            "application/toml",
            "text/x-toml",
            "text/csv",
            "text/tab-separated-values",
        ],
        actions: &[
            ActionSpec {
                id: "json",
                label: "JSON (.json)",
                target: Format::Json,
                icon: "application-json",
            },
            ActionSpec {
                id: "yaml",
                label: "YAML (.yaml)",
                target: Format::Yaml,
                icon: "text-plain",
            },
            ActionSpec {
                id: "toml",
                label: "TOML (.toml)",
                target: Format::Toml,
                icon: "text-plain",
            },
            ActionSpec {
                id: "csv",
                label: "CSV (.csv)",
                target: Format::Csv,
                icon: "text-csv",
            },
            ActionSpec {
                id: "tsv",
                label: "TSV (.tsv)",
                target: Format::Tsv,
                icon: "text-plain",
            },
        ],
    },
    MenuGroup {
        slug: "archive",
        family: Family::Archive,
        mime_types: &[
            "application/zip",
            "application/x-tar",
            "application/x-compressed-tar",
            "application/gzip",
            "application/x-zstd-compressed-tar",
            "application/x-xz-compressed-tar",
            "application/x-bzip2-compressed-tar",
            "application/x-7z-compressed",
            "application/vnd.rar",
            "application/x-rar",
        ],
        actions: &[
            ActionSpec {
                id: "zip",
                label: "ZIP (.zip)",
                target: Format::Zip,
                icon: "application-x-archive",
            },
            ActionSpec {
                id: "tar-gz",
                label: "TAR.GZ (.tar.gz)",
                target: Format::TarGz,
                icon: "application-x-archive",
            },
            ActionSpec {
                id: "tar-zst",
                label: "TAR.ZST (.tar.zst)",
                target: Format::TarZst,
                icon: "application-x-archive",
            },
            ActionSpec {
                id: "tar-xz",
                label: "TAR.XZ (.tar.xz)",
                target: Format::TarXz,
                icon: "application-x-archive",
            },
            ActionSpec {
                id: "7z",
                label: "7Z (.7z)",
                target: Format::SevenZ,
                icon: "application-x-archive",
            },
        ],
    },
    MenuGroup {
        slug: "document",
        family: Family::Document,
        mime_types: &[
            "text/markdown",
            "text/html",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            "application/vnd.oasis.opendocument.text",
            "application/epub+zip",
            "application/rtf",
        ],
        actions: &[
            ActionSpec {
                id: "pdf",
                label: "PDF (.pdf)",
                target: Format::Pdf,
                icon: "application-pdf",
            },
            ActionSpec {
                id: "html",
                label: "HTML (.html)",
                target: Format::Html,
                icon: "text-html",
            },
            ActionSpec {
                id: "md",
                label: "Markdown (.md)",
                target: Format::Md,
                icon: "text-plain",
            },
            ActionSpec {
                id: "docx",
                label: "Word (.docx)",
                target: Format::Docx,
                icon: "x-office-document",
            },
            ActionSpec {
                id: "odt",
                label: "OpenDocument (.odt)",
                target: Format::Odt,
                icon: "x-office-document",
            },
            ActionSpec {
                id: "txt",
                label: "Plain Text (.txt)",
                target: Format::Txt,
                icon: "text-plain",
            },
        ],
    },
];

pub fn group_enabled(group: &MenuGroup, backends: &Backends, all_menus: bool) -> bool {
    if all_menus {
        return true;
    }
    match group.family {
        Family::Image => backends.has_images(),
        Family::Gif => backends.has_images() || backends.has_audio_video(),
        Family::Audio | Family::Video => backends.has_audio_video(),
        Family::Pdf => backends.has_pdf(),
        Family::Data => true,
        Family::Archive => backends.has_archives(),
        Family::Document => backends.has_documents(),
    }
}

pub fn action_enabled(
    group: &MenuGroup,
    action: &ActionSpec,
    backends: &Backends,
    all_menus: bool,
) -> bool {
    if all_menus {
        return true;
    }
    match (group.family, action.target) {
        (Family::Gif, Format::Mp4 | Format::Webm) => backends.has_audio_video(),
        (Family::Gif, _) => backends.has_images(),
        (Family::Pdf, Format::Txt) => backends.pdftotext.is_some(),
        (Family::Pdf, Format::Webp) => backends.pdftoppm.is_some() && backends.has_images(),
        (Family::Pdf, _) => backends.pdftoppm.is_some(),
        (Family::Archive, Format::SevenZ) => backends.seven_z.is_some(),
        (Family::Archive, Format::Zip) => {
            backends.zip.is_some() || backends.bsdtar.is_some() || backends.seven_z.is_some()
        }
        (Family::Archive, Format::TarZst) => {
            (backends.tar.is_some() || backends.bsdtar.is_some()) && backends.zstd.is_some()
        }
        (Family::Archive, _) => backends.tar.is_some() || backends.bsdtar.is_some(),
        _ => group_enabled(group, backends, all_menus),
    }
}

pub fn render_dolphin_desktop(
    group: &MenuGroup,
    bin_path: &Path,
    backends: &Backends,
    all_menus: bool,
) -> Option<String> {
    let active_actions: Vec<&ActionSpec> = group
        .actions
        .iter()
        .filter(|a| action_enabled(group, a, backends, all_menus))
        .collect();
    if active_actions.is_empty() {
        return None;
    }
    let bin = bin_path.display();
    let mime_list = group.mime_types.join(";") + ";";
    let action_ids: Vec<&str> = active_actions.iter().map(|a| a.id).collect();
    let mut out = String::new();
    out.push_str("[Desktop Entry]\n");
    out.push_str("Type=Service\n");
    out.push_str("Name=Convert to\n");
    out.push_str("X-KDE-ServiceTypes=KonqPopupMenu/Plugin\n");
    out.push_str(&format!("MimeType={mime_list}\n"));
    out.push_str("X-KDE-Protocols=file\n");
    out.push_str("X-KDE-Priority=TopLevel\n");
    out.push_str("X-KDE-Submenu=Convert to\n");
    out.push_str("Icon=document-export\n");
    out.push_str(&format!("Actions={};\n", action_ids.join(";")));
    out.push_str("X-Convto-Generated=true\n");

    for action in active_actions {
        out.push_str(&format!("\n[Desktop Action {}]\n", action.id));
        out.push_str(&format!("Name={}\n", action.label));
        out.push_str(&format!("Icon={}\n", action.icon));
        out.push_str(&format!(
            "Exec=\"{bin}\" --notify to {} %F\n",
            action.target.ext()
        ));
    }
    Some(out)
}

pub fn render_nautilus_extension(bin_path: &Path) -> String {
    let bin = bin_path
        .display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    format!(
        r#"# Generated by convto install
import os
import subprocess
from gi.repository import GObject, Nautilus

CONVTO_BIN = "{bin}"

GROUPS = {{
    "image": (
        {{"png", "jpg", "jpeg", "webp", "avif", "heic", "heif", "jxl", "bmp", "tif", "tiff", "ico", "svg", "tga", "psd", "qoi", "ppm"}},
        [("png", "PNG (.png)"), ("jpg", "JPEG (.jpg)"), ("webp", "WebP (.webp)"), ("avif", "AVIF (.avif)"), ("jxl", "JPEG XL (.jxl)"), ("gif", "GIF (.gif)"), ("ico", "ICO (.ico)"), ("tiff", "TIFF (.tiff)"), ("bmp", "BMP (.bmp)"), ("pdf", "PDF (.pdf)")],
    ),
    "gif": (
        {{"gif"}},
        [("mp4", "MP4 Video (.mp4)"), ("webm", "WebM Video (.webm)"), ("webp", "WebP (.webp)"), ("png", "PNG Frame (.png)"), ("jpg", "JPEG Frame (.jpg)"), ("avif", "AVIF (.avif)")],
    ),
    "audio": (
        {{"mp3", "wav", "flac", "ogg", "opus", "m4a", "aac", "wma", "aiff", "mka"}},
        [("mp3", "MP3 (.mp3)"), ("wav", "WAV (.wav)"), ("flac", "FLAC (.flac)"), ("ogg", "OGG (.ogg)"), ("opus", "Opus (.opus)"), ("m4a", "M4A (.m4a)")],
    ),
    "video": (
        {{"mp4", "mkv", "webm", "mov", "avi", "flv", "wmv", "m4v", "mpg", "mpeg", "3gp"}},
        [("mp4", "MP4 (.mp4)"), ("mkv", "MKV (.mkv)"), ("webm", "WebM (.webm)"), ("mov", "MOV (.mov)"), ("gif", "Animated GIF (.gif)"), ("mp3", "Extract MP3 (.mp3)"), ("wav", "Extract WAV (.wav)"), ("opus", "Extract Opus (.opus)")],
    ),
    "pdf": (
        {{"pdf"}},
        [("png", "PNG Images (.png)"), ("jpg", "JPEG Images (.jpg)"), ("webp", "WebP Images (.webp)"), ("txt", "Plain Text (.txt)")],
    ),
    "data": (
        {{"json", "yaml", "yml", "toml", "csv", "tsv"}},
        [("json", "JSON (.json)"), ("yaml", "YAML (.yaml)"), ("toml", "TOML (.toml)"), ("csv", "CSV (.csv)"), ("tsv", "TSV (.tsv)")],
    ),
    "archive": (
        {{"zip", "tar", "tar.gz", "tgz", "tar.zst", "tar.xz", "tar.bz2", "7z", "rar"}},
        [("zip", "ZIP (.zip)"), ("tar.gz", "TAR.GZ (.tar.gz)"), ("tar.zst", "TAR.ZST (.tar.zst)"), ("tar.xz", "TAR.XZ (.tar.xz)"), ("7z", "7Z (.7z)")],
    ),
}}

def _ext(path):
    low = os.path.basename(path).lower()
    for compound in ("tar.gz", "tar.zst", "tar.xz", "tar.bz2"):
        if low.endswith("." + compound):
            return compound
    _, e = os.path.splitext(low)
    return e.lstrip(".")

class ConvtoMenuProvider(GObject.GObject, Nautilus.MenuProvider):
    def get_file_items(self, *args):
        files = args[-1]
        paths = []
        for f in files:
            if f.is_directory() or f.get_uri_scheme() != "file":
                return []
            p = f.get_location().get_path()
            if not p:
                return []
            paths.append(p)
        if not paths:
            return []
        exts = [_ext(p) for p in paths]
        matched = None
        for name, (src_exts, targets) in GROUPS.items():
            if all(e in src_exts for e in exts):
                matched = (name, targets)
                break
        if matched is None:
            return []
        group_name, targets = matched
        single_ext = exts[0] if len(set(exts)) == 1 else None
        top = Nautilus.MenuItem(name=f"Convto::{{group_name}}", label="Convert to")
        submenu = Nautilus.Menu()
        top.set_submenu(submenu)
        for fmt, label in targets:
            if single_ext == fmt:
                continue
            item = Nautilus.MenuItem(name=f"Convto::{{group_name}}::{{fmt}}", label=label)
            item.connect("activate", self._run, fmt, paths)
            submenu.append_item(item)
        return [top]

    def _run(self, _menu, fmt, paths):
        subprocess.Popen([CONVTO_BIN, "--notify", "to", fmt, *paths])
"#
    )
}

pub fn render_nemo_action(group: &MenuGroup, action: &ActionSpec, bin_path: &Path) -> String {
    let mimes = group.mime_types.join(";") + ";";
    format!(
        "[Nemo Action]\nName=Convert to {}\nComment=Convert selected file(s) to {} using convto\nExec=\"{}\" --notify to {} %F\nIcon-Name={}\nSelection=notnone\nMimetypes={}\nQuote=double\n",
        action.label,
        action.target.ext(),
        bin_path.display(),
        action.target.ext(),
        action.icon,
        mimes
    )
}

pub fn detect_desktops() -> Vec<DesktopTarget> {
    let mut found = Vec::new();
    let xdg = env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_ascii_lowercase();
    if xdg.contains("kde") || find_in_path("dolphin").is_some() {
        found.push(DesktopTarget::Dolphin);
    }
    if xdg.contains("gnome") || find_in_path("nautilus").is_some() {
        found.push(DesktopTarget::Nautilus);
    }
    if xdg.contains("cinnamon") || find_in_path("nemo").is_some() {
        found.push(DesktopTarget::Nemo);
    }
    if xdg.contains("xfce") || find_in_path("thunar").is_some() {
        found.push(DesktopTarget::Thunar);
    }
    if found.is_empty() {
        found.push(DesktopTarget::Dolphin);
    }
    found
}

pub struct InstallReport {
    pub installed_files: Vec<PathBuf>,
    pub skipped_groups: Vec<String>,
    pub desktops: Vec<String>,
    pub refreshed_kde_cache: bool,
}

pub fn install_menus(
    home: &Path,
    bin_path: &Path,
    target: DesktopTarget,
    backends: &Backends,
    all_menus: bool,
) -> Result<InstallReport, String> {
    let targets = match target {
        DesktopTarget::Auto => detect_desktops(),
        DesktopTarget::All => vec![
            DesktopTarget::Dolphin,
            DesktopTarget::Nautilus,
            DesktopTarget::Nemo,
            DesktopTarget::Thunar,
        ],
        other => vec![other],
    };

    let mut installed_files = Vec::new();
    let mut skipped_groups = Vec::new();
    let mut desktops = Vec::new();
    let mut refreshed_kde_cache = false;

    for group in MENU_GROUPS {
        if !group_enabled(group, backends, all_menus) {
            skipped_groups.push(group.slug.to_string());
        }
    }

    for dt in targets {
        match dt {
            DesktopTarget::Dolphin => {
                desktops.push("dolphin".to_string());
                let dir = dolphin_menu_dir(home, backends);
                fs::create_dir_all(&dir)
                    .map_err(|e| format!("Failed to create {}: {e}", dir.display()))?;
                // Remove stale convto-* desktop files first so disabled groups disappear cleanly.
                remove_prefixed_files(&dir, "convto-", ".desktop")?;
                for group in MENU_GROUPS {
                    if let Some(content) =
                        render_dolphin_desktop(group, bin_path, backends, all_menus)
                    {
                        let path = dir.join(format!("convto-{}.desktop", group.slug));
                        fs::write(&path, content)
                            .map_err(|e| format!("Failed to write {}: {e}", path.display()))?;
                        set_executable(&path)?;
                        installed_files.push(path);
                    }
                }
                if let Some(cmd_path) = backends
                    .kbuildsycoca6
                    .as_deref()
                    .or(backends.kbuildsycoca5.as_deref())
                {
                    if Command::new(cmd_path)
                        .output()
                        .map(|o| o.status.success())
                        .unwrap_or(false)
                    {
                        refreshed_kde_cache = true;
                    }
                }
            }
            DesktopTarget::Nautilus => {
                desktops.push("nautilus".to_string());
                let dir = home.join(".local/share/nautilus-python/extensions");
                fs::create_dir_all(&dir)
                    .map_err(|e| format!("Failed to create {}: {e}", dir.display()))?;
                let path = dir.join("convto_nautilus.py");
                fs::write(&path, render_nautilus_extension(bin_path))
                    .map_err(|e| format!("Failed to write {}: {e}", path.display()))?;
                installed_files.push(path);
            }
            DesktopTarget::Nemo => {
                desktops.push("nemo".to_string());
                let dir = home.join(".local/share/nemo/actions");
                fs::create_dir_all(&dir)
                    .map_err(|e| format!("Failed to create {}: {e}", dir.display()))?;
                remove_prefixed_files(&dir, "convto-", ".nemo_action")?;
                for group in MENU_GROUPS {
                    for action in group.actions {
                        if action_enabled(group, action, backends, all_menus) {
                            let path = dir
                                .join(format!("convto-{}-{}.nemo_action", group.slug, action.id));
                            fs::write(&path, render_nemo_action(group, action, bin_path))
                                .map_err(|e| format!("Failed to write {}: {e}", path.display()))?;
                            installed_files.push(path);
                        }
                    }
                }
            }
            DesktopTarget::Thunar => {
                desktops.push("thunar".to_string());
                let dir = home.join(".config/Thunar");
                fs::create_dir_all(&dir)
                    .map_err(|e| format!("Failed to create {}: {e}", dir.display()))?;
                let path = dir.join("uca.xml");
                update_thunar_uca(&path, bin_path, backends, all_menus)?;
                installed_files.push(path);
            }
            DesktopTarget::Auto | DesktopTarget::All => {}
        }
    }

    Ok(InstallReport {
        installed_files,
        skipped_groups,
        desktops,
        refreshed_kde_cache,
    })
}

pub fn uninstall_menus(
    home: &Path,
    target: DesktopTarget,
    backends: &Backends,
) -> Result<Vec<PathBuf>, String> {
    let targets = match target {
        DesktopTarget::Auto | DesktopTarget::All => vec![
            DesktopTarget::Dolphin,
            DesktopTarget::Nautilus,
            DesktopTarget::Nemo,
            DesktopTarget::Thunar,
        ],
        other => vec![other],
    };
    let mut removed = Vec::new();
    let mut touched_kde = false;

    for dt in targets {
        match dt {
            DesktopTarget::Dolphin => {
                for dir in [
                    home.join(".local/share/kio/servicemenus"),
                    home.join(".local/share/kservices5/ServiceMenus"),
                ] {
                    if dir.exists() {
                        let r = remove_prefixed_files(&dir, "convto-", ".desktop")?;
                        if !r.is_empty() {
                            touched_kde = true;
                            removed.extend(r);
                        }
                    }
                }
            }
            DesktopTarget::Nautilus => {
                let path = home.join(".local/share/nautilus-python/extensions/convto_nautilus.py");
                if path.exists() {
                    fs::remove_file(&path)
                        .map_err(|e| format!("Failed to remove {}: {e}", path.display()))?;
                    removed.push(path);
                }
            }
            DesktopTarget::Nemo => {
                let dir = home.join(".local/share/nemo/actions");
                if dir.exists() {
                    removed.extend(remove_prefixed_files(&dir, "convto-", ".nemo_action")?);
                }
            }
            DesktopTarget::Thunar => {
                let path = home.join(".config/Thunar/uca.xml");
                if path.exists() && strip_thunar_uca(&path)? {
                    removed.push(path);
                }
            }
            DesktopTarget::Auto | DesktopTarget::All => {}
        }
    }

    if touched_kde {
        if let Some(cmd_path) = backends
            .kbuildsycoca6
            .as_deref()
            .or(backends.kbuildsycoca5.as_deref())
        {
            let _ = Command::new(cmd_path).output();
        }
    }

    Ok(removed)
}

pub fn dolphin_menu_dir(home: &Path, backends: &Backends) -> PathBuf {
    if backends.kbuildsycoca6.is_some() || backends.kbuildsycoca5.is_none() {
        home.join(".local/share/kio/servicemenus")
    } else {
        home.join(".local/share/kservices5/ServiceMenus")
    }
}

fn remove_prefixed_files(dir: &Path, prefix: &str, suffix: &str) -> Result<Vec<PathBuf>, String> {
    let mut removed = Vec::new();
    let entries =
        fs::read_dir(dir).map_err(|e| format!("Failed to read {}: {e}", dir.display()))?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        if name_str.starts_with(prefix) && name_str.ends_with(suffix) {
            let p = entry.path();
            fs::remove_file(&p).map_err(|e| format!("Failed to remove {}: {e}", p.display()))?;
            removed.push(p);
        }
    }
    Ok(removed)
}

const THUNAR_BEGIN: &str = "<!-- convto-managed:begin -->";
const THUNAR_END: &str = "<!-- convto-managed:end -->";

fn update_thunar_uca(
    uca_path: &Path,
    bin_path: &Path,
    backends: &Backends,
    all_menus: bool,
) -> Result<(), String> {
    let existing = if uca_path.exists() {
        fs::read_to_string(uca_path)
            .map_err(|e| format!("Failed to read {}: {e}", uca_path.display()))?
    } else {
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<actions>\n</actions>\n".to_string()
    };
    let cleaned = remove_managed_block(&existing);
    let mut block = String::new();
    block.push_str(THUNAR_BEGIN);
    block.push('\n');
    let bin = xml_escape(&bin_path.display().to_string());
    for group in MENU_GROUPS {
        let patterns = thunar_patterns_for(group.family);
        for action in group.actions {
            if !action_enabled(group, action, backends, all_menus) {
                continue;
            }
            block.push_str(&format!(
                "<action>\n  <icon>{}</icon>\n  <name>Convert to {}</name>\n  <submenu>Convert to</submenu>\n  <unique-id>convto-{}-{}</unique-id>\n  <command>&quot;{bin}&quot; --notify to {} %F</command>\n  <description>Convert selected file(s) to {}</description>\n  <patterns>{patterns}</patterns>\n  <other-files/>\n  <image-files/>\n  <audio-files/>\n  <video-files/>\n  <text-files/>\n</action>\n",
                xml_escape(action.icon),
                xml_escape(action.label),
                group.slug,
                action.id,
                action.target.ext(),
                action.target.ext(),
            ));
        }
    }
    block.push_str(THUNAR_END);
    block.push('\n');

    let updated = if let Some(idx) = cleaned.rfind("</actions>") {
        format!("{}{block}{}", &cleaned[..idx], &cleaned[idx..])
    } else {
        format!("{cleaned}\n<actions>\n{block}</actions>\n")
    };
    fs::write(uca_path, updated).map_err(|e| format!("Failed to write {}: {e}", uca_path.display()))
}

fn strip_thunar_uca(uca_path: &Path) -> Result<bool, String> {
    let existing = fs::read_to_string(uca_path)
        .map_err(|e| format!("Failed to read {}: {e}", uca_path.display()))?;
    if !existing.contains(THUNAR_BEGIN) {
        return Ok(false);
    }
    let cleaned = remove_managed_block(&existing);
    fs::write(uca_path, cleaned)
        .map_err(|e| format!("Failed to write {}: {e}", uca_path.display()))?;
    Ok(true)
}

fn remove_managed_block(xml: &str) -> String {
    if let (Some(start), Some(end)) = (xml.find(THUNAR_BEGIN), xml.find(THUNAR_END)) {
        let after = end + THUNAR_END.len();
        let after = xml[after..]
            .strip_prefix('\n')
            .map(|_| after + 1)
            .unwrap_or(after);
        format!("{}{}", &xml[..start], &xml[after..])
    } else {
        xml.to_string()
    }
}

fn thunar_patterns_for(family: Family) -> &'static str {
    match family {
        Family::Image => "*.png;*.jpg;*.jpeg;*.webp;*.avif;*.heic;*.heif;*.jxl;*.bmp;*.tif;*.tiff;*.ico;*.svg;*.tga;*.psd;*.qoi;*.ppm",
        Family::Gif => "*.gif",
        Family::Audio => "*.mp3;*.wav;*.flac;*.ogg;*.opus;*.m4a;*.aac;*.wma;*.aiff;*.mka",
        Family::Video => "*.mp4;*.mkv;*.webm;*.mov;*.avi;*.flv;*.wmv;*.m4v;*.mpg;*.mpeg;*.3gp",
        Family::Pdf => "*.pdf",
        Family::Data => "*.json;*.yaml;*.yml;*.toml;*.csv;*.tsv",
        Family::Archive => "*.zip;*.tar;*.tar.gz;*.tgz;*.tar.zst;*.tar.xz;*.tar.bz2;*.7z;*.rar",
        Family::Document => "*.md;*.html;*.docx;*.odt;*.epub;*.rtf",
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .map_err(|e| format!("Failed to chmod +x {}: {e}", path.display()))
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

pub fn verify_disjoint_mime_types() -> Result<(), String> {
    let mut seen: HashSet<&str> = HashSet::new();
    for group in MENU_GROUPS {
        for &mime in group.mime_types {
            if !seen.insert(mime) {
                return Err(format!("Duplicate MIME type across menu groups: {mime}"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_groups_have_disjoint_mime_types() {
        verify_disjoint_mime_types().unwrap();
        for group in MENU_GROUPS {
            for action in group.actions {
                assert!(
                    action
                        .id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-'),
                    "invalid desktop action id: {}",
                    action.id
                );
            }
        }
    }
}
