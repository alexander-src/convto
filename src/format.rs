use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Family {
    Image,
    Gif,
    Audio,
    Video,
    Pdf,
    Data,
    Archive,
    Document,
}

impl fmt::Display for Family {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Family::Image => "image",
            Family::Gif => "gif",
            Family::Audio => "audio",
            Family::Video => "video",
            Family::Pdf => "pdf",
            Family::Data => "data",
            Family::Archive => "archive",
            Family::Document => "document",
        };
        f.write_str(label)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    // Images
    Png,
    Jpg,
    Webp,
    Avif,
    Heic,
    Jxl,
    Gif,
    Bmp,
    Tiff,
    Ico,
    Svg,
    Tga,
    Psd,
    Qoi,
    Ppm,
    // Audio
    Mp3,
    Wav,
    Flac,
    Ogg,
    Opus,
    M4a,
    Aac,
    Wma,
    Aiff,
    Mka,
    // Video
    Mp4,
    Mkv,
    Webm,
    Mov,
    Avi,
    Flv,
    Wmv,
    Mpg,
    ThreeGp,
    // PDF & Text
    Pdf,
    Txt,
    // Documents
    Md,
    Html,
    Docx,
    Odt,
    Epub,
    Rtf,
    // Structured data
    Json,
    Yaml,
    Toml,
    Csv,
    Tsv,
    // Archives
    Zip,
    Tar,
    TarGz,
    TarZst,
    TarXz,
    TarBz2,
    SevenZ,
    Rar,
}

impl Format {
    pub fn parse(raw: &str) -> Option<Self> {
        let s = raw.trim().trim_start_matches('.').to_ascii_lowercase();
        match s.as_str() {
            "png" => Some(Self::Png),
            "jpg" | "jpeg" | "jpe" => Some(Self::Jpg),
            "webp" => Some(Self::Webp),
            "avif" => Some(Self::Avif),
            "heic" | "heif" => Some(Self::Heic),
            "jxl" => Some(Self::Jxl),
            "gif" => Some(Self::Gif),
            "bmp" => Some(Self::Bmp),
            "tif" | "tiff" => Some(Self::Tiff),
            "ico" => Some(Self::Ico),
            "svg" => Some(Self::Svg),
            "tga" => Some(Self::Tga),
            "psd" => Some(Self::Psd),
            "qoi" => Some(Self::Qoi),
            "ppm" | "pgm" | "pbm" => Some(Self::Ppm),
            "mp3" => Some(Self::Mp3),
            "wav" | "wave" => Some(Self::Wav),
            "flac" => Some(Self::Flac),
            "ogg" | "oga" => Some(Self::Ogg),
            "opus" => Some(Self::Opus),
            "m4a" => Some(Self::M4a),
            "aac" => Some(Self::Aac),
            "wma" => Some(Self::Wma),
            "aif" | "aiff" => Some(Self::Aiff),
            "mka" => Some(Self::Mka),
            "mp4" | "m4v" => Some(Self::Mp4),
            "mkv" => Some(Self::Mkv),
            "webm" => Some(Self::Webm),
            "mov" => Some(Self::Mov),
            "avi" => Some(Self::Avi),
            "flv" => Some(Self::Flv),
            "wmv" => Some(Self::Wmv),
            "mpg" | "mpeg" => Some(Self::Mpg),
            "3gp" => Some(Self::ThreeGp),
            "pdf" => Some(Self::Pdf),
            "txt" => Some(Self::Txt),
            "md" | "markdown" => Some(Self::Md),
            "html" | "htm" => Some(Self::Html),
            "docx" => Some(Self::Docx),
            "odt" => Some(Self::Odt),
            "epub" => Some(Self::Epub),
            "rtf" => Some(Self::Rtf),
            "json" => Some(Self::Json),
            "yaml" | "yml" => Some(Self::Yaml),
            "toml" => Some(Self::Toml),
            "csv" => Some(Self::Csv),
            "tsv" => Some(Self::Tsv),
            "zip" => Some(Self::Zip),
            "tar" => Some(Self::Tar),
            "tar.gz" | "tgz" => Some(Self::TarGz),
            "tar.zst" | "tzst" => Some(Self::TarZst),
            "tar.xz" | "txz" => Some(Self::TarXz),
            "tar.bz2" | "tbz2" | "tbz" => Some(Self::TarBz2),
            "7z" => Some(Self::SevenZ),
            "rar" => Some(Self::Rar),
            _ => None,
        }
    }

    pub fn ext(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpg => "jpg",
            Self::Webp => "webp",
            Self::Avif => "avif",
            Self::Heic => "heic",
            Self::Jxl => "jxl",
            Self::Gif => "gif",
            Self::Bmp => "bmp",
            Self::Tiff => "tiff",
            Self::Ico => "ico",
            Self::Svg => "svg",
            Self::Tga => "tga",
            Self::Psd => "psd",
            Self::Qoi => "qoi",
            Self::Ppm => "ppm",
            Self::Mp3 => "mp3",
            Self::Wav => "wav",
            Self::Flac => "flac",
            Self::Ogg => "ogg",
            Self::Opus => "opus",
            Self::M4a => "m4a",
            Self::Aac => "aac",
            Self::Wma => "wma",
            Self::Aiff => "aiff",
            Self::Mka => "mka",
            Self::Mp4 => "mp4",
            Self::Mkv => "mkv",
            Self::Webm => "webm",
            Self::Mov => "mov",
            Self::Avi => "avi",
            Self::Flv => "flv",
            Self::Wmv => "wmv",
            Self::Mpg => "mpg",
            Self::ThreeGp => "3gp",
            Self::Pdf => "pdf",
            Self::Txt => "txt",
            Self::Md => "md",
            Self::Html => "html",
            Self::Docx => "docx",
            Self::Odt => "odt",
            Self::Epub => "epub",
            Self::Rtf => "rtf",
            Self::Json => "json",
            Self::Yaml => "yaml",
            Self::Toml => "toml",
            Self::Csv => "csv",
            Self::Tsv => "tsv",
            Self::Zip => "zip",
            Self::Tar => "tar",
            Self::TarGz => "tar.gz",
            Self::TarZst => "tar.zst",
            Self::TarXz => "tar.xz",
            Self::TarBz2 => "tar.bz2",
            Self::SevenZ => "7z",
            Self::Rar => "rar",
        }
    }

    pub fn family(self) -> Family {
        match self {
            Self::Gif => Family::Gif,
            Self::Png
            | Self::Jpg
            | Self::Webp
            | Self::Avif
            | Self::Heic
            | Self::Jxl
            | Self::Bmp
            | Self::Tiff
            | Self::Ico
            | Self::Svg
            | Self::Tga
            | Self::Psd
            | Self::Qoi
            | Self::Ppm => Family::Image,
            Self::Mp3
            | Self::Wav
            | Self::Flac
            | Self::Ogg
            | Self::Opus
            | Self::M4a
            | Self::Aac
            | Self::Wma
            | Self::Aiff
            | Self::Mka => Family::Audio,
            Self::Mp4
            | Self::Mkv
            | Self::Webm
            | Self::Mov
            | Self::Avi
            | Self::Flv
            | Self::Wmv
            | Self::Mpg
            | Self::ThreeGp => Family::Video,
            Self::Pdf => Family::Pdf,
            Self::Json | Self::Yaml | Self::Toml | Self::Csv | Self::Tsv => Family::Data,
            Self::Zip
            | Self::Tar
            | Self::TarGz
            | Self::TarZst
            | Self::TarXz
            | Self::TarBz2
            | Self::SevenZ
            | Self::Rar => Family::Archive,
            Self::Md | Self::Html | Self::Docx | Self::Odt | Self::Epub | Self::Rtf | Self::Txt => {
                Family::Document
            }
        }
    }

    pub fn is_raster_or_vector_image(self) -> bool {
        matches!(self.family(), Family::Image | Family::Gif)
    }

    pub fn valid_targets(self) -> &'static [Format] {
        match self.family() {
            Family::Image => &[
                Self::Png,
                Self::Jpg,
                Self::Webp,
                Self::Avif,
                Self::Jxl,
                Self::Gif,
                Self::Ico,
                Self::Tiff,
                Self::Bmp,
                Self::Pdf,
            ],
            Family::Gif => &[
                Self::Mp4,
                Self::Webm,
                Self::Webp,
                Self::Png,
                Self::Jpg,
                Self::Avif,
            ],
            Family::Audio => &[
                Self::Mp3,
                Self::Wav,
                Self::Flac,
                Self::Ogg,
                Self::Opus,
                Self::M4a,
            ],
            Family::Video => &[
                Self::Mp4,
                Self::Mkv,
                Self::Webm,
                Self::Mov,
                Self::Gif,
                Self::Mp3,
                Self::Wav,
                Self::Opus,
            ],
            Family::Pdf => &[Self::Png, Self::Jpg, Self::Webp, Self::Txt],
            Family::Data => &[Self::Json, Self::Yaml, Self::Toml, Self::Csv, Self::Tsv],
            Family::Archive => &[
                Self::Zip,
                Self::TarGz,
                Self::TarZst,
                Self::TarXz,
                Self::TarBz2,
                Self::Tar,
                Self::SevenZ,
            ],
            Family::Document => &[
                Self::Pdf,
                Self::Html,
                Self::Md,
                Self::Docx,
                Self::Odt,
                Self::Txt,
            ],
        }
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.ext())
    }
}

/// Strip known compound extensions (`.tar.gz`, `.tar.zst`, `.tar.xz`, `.tar.bz2`) or single extension.
pub fn stem_without_format(path: &Path) -> String {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("output");
    let lower = name.to_ascii_lowercase();
    for compound in [".tar.gz", ".tar.zst", ".tar.xz", ".tar.bz2"] {
        if lower.ends_with(compound) && name.len() > compound.len() {
            return name[..name.len() - compound.len()].to_string();
        }
    }
    match path.file_stem().and_then(|s| s.to_str()) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => name.to_string(),
    }
}

pub fn format_from_path(path: &Path) -> Option<Format> {
    let name = path.file_name()?.to_str()?;
    let lower = name.to_ascii_lowercase();
    for (suffix, fmt) in [
        (".tar.gz", Format::TarGz),
        (".tar.zst", Format::TarZst),
        (".tar.xz", Format::TarXz),
        (".tar.bz2", Format::TarBz2),
    ] {
        if lower.ends_with(suffix) {
            return Some(fmt);
        }
    }
    let ext = path.extension()?.to_str()?;
    Format::parse(ext)
}

/// Sniff unambiguous binary magic bytes so misnamed downloads (such as WebP saved as `.jpg`)
/// still detect accurately.
pub fn sniff_magic(bytes: &[u8]) -> Option<Format> {
    if bytes.len() >= 8 && &bytes[0..8] == b"\x89PNG\r\n\x1a\n" {
        return Some(Format::Png);
    }
    if bytes.len() >= 3 && bytes[0] == 0xff && bytes[1] == 0xd8 && bytes[2] == 0xff {
        return Some(Format::Jpg);
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some(Format::Webp);
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
        return Some(Format::Wav);
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"AVI " {
        return Some(Format::Avi);
    }
    if bytes.len() >= 6 && (&bytes[0..6] == b"GIF87a" || &bytes[0..6] == b"GIF89a") {
        return Some(Format::Gif);
    }
    if bytes.len() >= 2 && &bytes[0..2] == b"BM" {
        return Some(Format::Bmp);
    }
    if bytes.len() >= 4 && ((&bytes[0..4] == b"II*\0") || (&bytes[0..4] == b"MM\0*")) {
        return Some(Format::Tiff);
    }
    if bytes.len() >= 4 && &bytes[0..4] == b"qoif" {
        return Some(Format::Qoi);
    }
    if bytes.len() >= 4 && &bytes[0..4] == b"8BPS" {
        return Some(Format::Psd);
    }
    if bytes.len() >= 12 && &bytes[0..12] == b"\x00\x00\x00\x0cJXL \r\n\x87\n"
        || (bytes.len() >= 2 && bytes[0] == 0xff && bytes[1] == 0x0a)
    {
        return Some(Format::Jxl);
    }
    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        let brand = &bytes[8..12];
        if brand == b"avif" || brand == b"avis" {
            return Some(Format::Avif);
        }
        if matches!(brand, b"heic" | b"heix" | b"hevc" | b"mif1" | b"msf1") {
            return Some(Format::Heic);
        }
        if brand == b"M4A " {
            return Some(Format::M4a);
        }
        if brand == b"qt  " {
            return Some(Format::Mov);
        }
    }
    if bytes.len() >= 5 && &bytes[0..5] == b"%PDF-" {
        return Some(Format::Pdf);
    }
    if bytes.len() >= 4 && &bytes[0..4] == b"fLaC" {
        return Some(Format::Flac);
    }
    if bytes.len() >= 6 && &bytes[0..6] == b"7z\xbc\xaf\x27\x1c" {
        return Some(Format::SevenZ);
    }
    if bytes.len() >= 7 && &bytes[0..7] == b"Rar!\x1a\x07\x00"
        || (bytes.len() >= 8 && &bytes[0..8] == b"Rar!\x1a\x07\x01\x00")
    {
        return Some(Format::Rar);
    }
    if bytes.len() >= 4 && &bytes[0..4] == b"\x28\xb5\x2f\xfd" {
        return Some(Format::TarZst);
    }
    if bytes.len() >= 6 && &bytes[0..6] == b"\xfd7zXZ\x00" {
        return Some(Format::TarXz);
    }
    None
}

/// Detect the file's real format using magic bytes first (for unambiguous binary formats),
/// falling back to the filename extension.
pub fn detect_format(path: &Path) -> Option<Format> {
    let ext_fmt = format_from_path(path);
    if let Ok(mut f) = File::open(path) {
        let mut buf = [0u8; 32];
        if let Ok(n) = f.read(&mut buf) {
            if let Some(sniffed) = sniff_magic(&buf[..n]) {
                // Keep ZIP-based document extensions (docx/odt/epub) and specific tar extensions intact.
                if let Some(ext) = ext_fmt {
                    if ext.family() == Family::Document && sniffed == Format::Zip {
                        return Some(ext);
                    }
                    if ext.family() == Family::Archive && sniffed.family() == Family::Archive {
                        return Some(ext);
                    }
                }
                return Some(sniffed);
            }
        }
    }
    ext_fmt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_compound_and_alias_extensions() {
        assert_eq!(Format::parse("JPEG"), Some(Format::Jpg));
        assert_eq!(Format::parse(".webp"), Some(Format::Webp));
        assert_eq!(Format::parse("tar.zst"), Some(Format::TarZst));
        assert_eq!(Format::parse("tgz"), Some(Format::TarGz));
        assert_eq!(
            format_from_path(Path::new("/tmp/a.tar.gz")),
            Some(Format::TarGz)
        );
        assert_eq!(
            stem_without_format(Path::new("/tmp/backup.tar.zst")),
            "backup"
        );
        assert_eq!(stem_without_format(Path::new("/tmp/photo.webp")), "photo");
    }

    #[test]
    fn sniffs_misnamed_webp_and_png() {
        let mut webp = [0u8; 16];
        webp[0..4].copy_from_slice(b"RIFF");
        webp[8..12].copy_from_slice(b"WEBP");
        assert_eq!(sniff_magic(&webp), Some(Format::Webp));
        assert_eq!(sniff_magic(b"\x89PNG\r\n\x1a\nxxxx"), Some(Format::Png));
        assert_eq!(sniff_magic(b"\x00\x00\x00\x1cftypavif"), Some(Format::Avif));
        assert_eq!(sniff_magic(b"%PDF-1.7\n"), Some(Format::Pdf));
    }
}
