use std::env;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Backends {
    pub magick: Option<PathBuf>,
    pub convert: Option<PathBuf>,
    pub ffmpeg: Option<PathBuf>,
    pub rsvg_convert: Option<PathBuf>,
    pub pdftoppm: Option<PathBuf>,
    pub pdftotext: Option<PathBuf>,
    pub tar: Option<PathBuf>,
    pub bsdtar: Option<PathBuf>,
    pub seven_z: Option<PathBuf>,
    pub zip: Option<PathBuf>,
    pub unzip: Option<PathBuf>,
    pub unrar: Option<PathBuf>,
    pub zstd: Option<PathBuf>,
    pub gzip: Option<PathBuf>,
    pub bzip2: Option<PathBuf>,
    pub xz: Option<PathBuf>,
    pub pandoc: Option<PathBuf>,
    pub libreoffice: Option<PathBuf>,
    pub notify_send: Option<PathBuf>,
    pub kbuildsycoca6: Option<PathBuf>,
    pub kbuildsycoca5: Option<PathBuf>,
}

impl Backends {
    pub fn probe() -> Self {
        Self {
            magick: find_in_path("magick"),
            convert: find_in_path("convert"),
            ffmpeg: find_in_path("ffmpeg"),
            rsvg_convert: find_in_path("rsvg-convert"),
            pdftoppm: find_in_path("pdftoppm"),
            pdftotext: find_in_path("pdftotext"),
            tar: find_in_path("tar"),
            bsdtar: find_in_path("bsdtar"),
            seven_z: find_in_path("7z").or_else(|| find_in_path("7zz")),
            zip: find_in_path("zip"),
            unzip: find_in_path("unzip"),
            unrar: find_in_path("unrar"),
            zstd: find_in_path("zstd"),
            gzip: find_in_path("gzip"),
            bzip2: find_in_path("bzip2"),
            xz: find_in_path("xz"),
            pandoc: find_in_path("pandoc"),
            libreoffice: find_in_path("libreoffice").or_else(|| find_in_path("soffice")),
            notify_send: find_in_path("notify-send"),
            kbuildsycoca6: find_in_path("kbuildsycoca6"),
            kbuildsycoca5: find_in_path("kbuildsycoca5"),
        }
    }

    pub fn image_tool(&self) -> Option<&Path> {
        self.magick.as_deref().or(self.convert.as_deref())
    }

    pub fn has_images(&self) -> bool {
        self.image_tool().is_some()
    }

    pub fn has_audio_video(&self) -> bool {
        self.ffmpeg.is_some()
    }

    pub fn has_pdf(&self) -> bool {
        self.pdftoppm.is_some() || self.pdftotext.is_some() || self.image_tool().is_some()
    }

    pub fn has_archives(&self) -> bool {
        self.tar.is_some() || self.bsdtar.is_some() || self.seven_z.is_some()
    }

    pub fn has_documents(&self) -> bool {
        self.pandoc.is_some() || self.libreoffice.is_some()
    }
}

pub fn find_in_path(name: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    for dir in env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match path.metadata() {
        Ok(meta) => meta.is_file() && (meta.permissions().mode() & 0o111 != 0),
        Err(_) => false,
    }
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}
