use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::data;
use crate::detect::Backends;
use crate::format::{detect_format, stem_without_format, Family, Format};

static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Default)]
pub struct ConvertOptions {
    pub overwrite: bool,
    pub force_same: bool,
    pub delete_source: bool,
    pub dry_run: bool,
    pub out_dir: Option<PathBuf>,
    pub quality: Option<u8>,
}

#[derive(Debug, Clone)]
pub enum Outcome {
    Converted {
        source: PathBuf,
        source_format: Format,
        outputs: Vec<PathBuf>,
        backend: String,
    },
    DryRun {
        source: PathBuf,
        source_format: Format,
        destination: PathBuf,
        backend: String,
    },
    Skipped {
        source: PathBuf,
        reason: String,
    },
}

struct TempFileGuard {
    path: PathBuf,
    armed: bool,
}

impl TempFileGuard {
    fn new(dir: &Path, ext: &str) -> Self {
        let pid = std::process::id();
        let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let path = dir.join(format!(".convto-{pid}-{seq}.tmp.{ext}"));
        Self { path, armed: true }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn commit(mut self, dest: &Path) -> Result<(), String> {
        fs::rename(&self.path, dest).map_err(|e| {
            format!(
                "Failed to move temporary output {} to {}: {e}",
                self.path.display(),
                dest.display()
            )
        })?;
        self.armed = false;
        Ok(())
    }
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    fn create(parent: &Path) -> Result<Self, String> {
        let pid = std::process::id();
        let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(".convto-stage-{pid}-{seq}"));
        fs::create_dir_all(&path)
            .map_err(|e| format!("Failed to create staging dir {}: {e}", path.display()))?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub fn pick_destination(
    src: &Path,
    target: Format,
    out_dir: Option<&Path>,
    overwrite: bool,
) -> Result<PathBuf, String> {
    let dir = match out_dir {
        Some(d) => {
            fs::create_dir_all(d)
                .map_err(|e| format!("Failed to create output directory {}: {e}", d.display()))?;
            d.to_path_buf()
        }
        None => src
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf(),
    };
    let stem = stem_without_format(src);
    let ext = target.ext();
    let base = dir.join(format!("{stem}.{ext}"));
    if overwrite && !paths_refer_to_same(src, &base) {
        return Ok(base);
    }
    if !base.exists() && !paths_refer_to_same(src, &base) {
        return Ok(base);
    }
    for n in 1..=9999u32 {
        let candidate = dir.join(format!("{stem} ({n}).{ext}"));
        if !candidate.exists() && !paths_refer_to_same(src, &candidate) {
            return Ok(candidate);
        }
    }
    Err(format!(
        "Could not find a free output filename for {} in {}",
        src.display(),
        dir.display()
    ))
}

fn pick_page_destination(
    dir: &Path,
    stem: &str,
    page: usize,
    ext: &str,
    overwrite: bool,
) -> PathBuf {
    let base = dir.join(format!("{stem}-page-{page}.{ext}"));
    if overwrite || !base.exists() {
        return base;
    }
    for n in 1..=9999u32 {
        let candidate = dir.join(format!("{stem}-page-{page} ({n}).{ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    base
}

fn paths_refer_to_same(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => false,
    }
}

pub fn convert_file(
    src: &Path,
    target: Format,
    backends: &Backends,
    opts: &ConvertOptions,
) -> Result<Outcome, String> {
    if !src.exists() {
        return Err(format!("File not found: {}", src.display()));
    }
    if src.is_dir() {
        return Err(format!("Cannot convert directory: {}", src.display()));
    }
    let src_fmt =
        detect_format(src).ok_or_else(|| format!("Unrecognized file format: {}", src.display()))?;

    if src_fmt == target && !opts.force_same {
        return Ok(Outcome::Skipped {
            source: src.to_path_buf(),
            reason: format!("already {target}"),
        });
    }

    if !src_fmt.valid_targets().contains(&target) {
        return Err(format!(
            "Cannot convert {} ({}) to {}",
            src.display(),
            src_fmt,
            target
        ));
    }

    let dest = pick_destination(src, target, opts.out_dir.as_deref(), opts.overwrite)?;
    let out_dir = dest
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    let backend_label = planned_backend_name(src_fmt, target, backends)?;
    if opts.dry_run {
        return Ok(Outcome::DryRun {
            source: src.to_path_buf(),
            source_format: src_fmt,
            destination: dest,
            backend: backend_label,
        });
    }

    let outputs = match (src_fmt.family(), target.family()) {
        (Family::Data, Family::Data) => {
            convert_data_file(src, src_fmt, target, out_dir, &dest)?;
            vec![dest]
        }
        (Family::Image, Family::Image | Family::Gif | Family::Pdf)
        | (Family::Gif, Family::Image) => {
            convert_image_file(src, src_fmt, target, out_dir, &dest, backends, opts)?;
            vec![dest]
        }
        (Family::Gif, Family::Video)
        | (Family::Audio, Family::Audio)
        | (Family::Video, Family::Video | Family::Audio | Family::Gif) => {
            convert_media_file(src, src_fmt, target, out_dir, &dest, backends)?;
            vec![dest]
        }
        (Family::Pdf, Family::Image | Family::Document) => {
            convert_pdf_file(src, target, out_dir, &dest, backends, opts)?
        }
        (Family::Archive, Family::Archive) => {
            convert_archive_file(src, src_fmt, target, out_dir, &dest, backends)?;
            vec![dest]
        }
        (Family::Document, Family::Document | Family::Pdf) => {
            convert_document_file(src, target, out_dir, &dest, backends)?;
            vec![dest]
        }
        _ => {
            return Err(format!("No conversion path from {src_fmt} to {target}"));
        }
    };

    if opts.delete_source {
        fs::remove_file(src).map_err(|e| {
            format!(
                "Converted successfully, but failed to delete source {}: {e}",
                src.display()
            )
        })?;
    }

    Ok(Outcome::Converted {
        source: src.to_path_buf(),
        source_format: src_fmt,
        outputs,
        backend: backend_label,
    })
}

fn planned_backend_name(
    src_fmt: Format,
    target: Format,
    backends: &Backends,
) -> Result<String, String> {
    match (src_fmt.family(), target.family()) {
        (Family::Data, Family::Data) => Ok("builtin".to_string()),
        (Family::Image, Family::Image | Family::Gif | Family::Pdf)
        | (Family::Gif, Family::Image) => {
            if src_fmt == Format::Svg && target == Format::Png && backends.rsvg_convert.is_some() {
                Ok("rsvg-convert".to_string())
            } else if backends.magick.is_some() {
                Ok("magick".to_string())
            } else if backends.convert.is_some() {
                Ok("convert".to_string())
            } else {
                Err("Image conversion requires ImageMagick ('magick' or 'convert')".to_string())
            }
        }
        (Family::Gif, Family::Video)
        | (Family::Audio, Family::Audio)
        | (Family::Video, Family::Video | Family::Audio | Family::Gif) => {
            if backends.ffmpeg.is_some() {
                Ok("ffmpeg".to_string())
            } else {
                Err("Audio/video conversion requires 'ffmpeg'".to_string())
            }
        }
        (Family::Pdf, Family::Document) if target == Format::Txt => {
            if backends.pdftotext.is_some() {
                Ok("pdftotext".to_string())
            } else {
                Err("PDF to text conversion requires 'pdftotext' (poppler)".to_string())
            }
        }
        (Family::Pdf, Family::Image) => {
            if backends.pdftoppm.is_some() {
                Ok("pdftoppm".to_string())
            } else if backends.image_tool().is_some() {
                Ok("magick".to_string())
            } else {
                Err(
                    "PDF to image conversion requires 'pdftoppm' (poppler) or ImageMagick"
                        .to_string(),
                )
            }
        }
        (Family::Archive, Family::Archive) => {
            if backends.has_archives() {
                Ok("archive".to_string())
            } else {
                Err("Archive conversion requires 'tar', 'bsdtar', or '7z'".to_string())
            }
        }
        (Family::Document, Family::Document | Family::Pdf) => {
            if target == Format::Pdf && backends.libreoffice.is_some() {
                Ok("libreoffice".to_string())
            } else if backends.pandoc.is_some() {
                Ok("pandoc".to_string())
            } else if backends.libreoffice.is_some() {
                Ok("libreoffice".to_string())
            } else {
                Err("Document conversion requires 'pandoc' or 'libreoffice'".to_string())
            }
        }
        _ => Err(format!("Unsupported conversion: {src_fmt} -> {target}")),
    }
}

fn convert_data_file(
    src: &Path,
    src_fmt: Format,
    target: Format,
    out_dir: &Path,
    dest: &Path,
) -> Result<(), String> {
    let text =
        fs::read_to_string(src).map_err(|e| format!("Failed to read {}: {e}", src.display()))?;
    let converted = data::convert_data(&text, src_fmt, target)?;
    let tmp = TempFileGuard::new(out_dir, target.ext());
    fs::write(tmp.path(), converted.as_bytes())
        .map_err(|e| format!("Failed to write {}: {e}", tmp.path().display()))?;
    tmp.commit(dest)
}

fn convert_image_file(
    src: &Path,
    src_fmt: Format,
    target: Format,
    out_dir: &Path,
    dest: &Path,
    backends: &Backends,
    opts: &ConvertOptions,
) -> Result<(), String> {
    let tmp = TempFileGuard::new(out_dir, target.ext());

    // Prefer native librsvg for crisp SVG -> PNG rendering when installed.
    if src_fmt == Format::Svg && target == Format::Png {
        if let Some(rsvg) = &backends.rsvg_convert {
            let mut cmd = Command::new(rsvg);
            cmd.arg("-f").arg("png").arg("-o").arg(tmp.path()).arg(src);
            run_checked(&mut cmd, "rsvg-convert")?;
            return tmp.commit(dest);
        }
    }

    let tool = backends.image_tool().ok_or_else(|| {
        "Image conversion requires ImageMagick ('magick' or 'convert')".to_string()
    })?;
    let mut cmd = Command::new(tool);

    if src_fmt == Format::Svg {
        cmd.arg("-background")
            .arg("none")
            .arg("-density")
            .arg("192");
    }

    cmd.arg(src);

    // Flatten animated GIF to frame 0 when targeting a static format, without using `[0]`
    // in the filename (which would break on literal brackets in filenames).
    if src_fmt == Format::Gif && target != Format::Webp && target != Format::Gif {
        cmd.arg("-coalesce").arg("-delete").arg("1--1");
    } else if src_fmt == Format::Gif && target == Format::Webp {
        cmd.arg("-coalesce");
    }

    if matches!(target, Format::Jpg | Format::Bmp) {
        cmd.arg("-background")
            .arg("white")
            .arg("-alpha")
            .arg("remove")
            .arg("-alpha")
            .arg("off");
    }

    if target == Format::Ico {
        cmd.arg("-resize")
            .arg("256x256>")
            .arg("-define")
            .arg("icon:auto-resize=256,128,64,48,32,16");
    }

    let default_quality = match target {
        Format::Avif => Some(82),
        Format::Jpg | Format::Webp | Format::Jxl => Some(88),
        _ => None,
    };
    if let Some(q) = opts.quality.or(default_quality) {
        let clamped = q.clamp(1, 100);
        cmd.arg("-quality").arg(clamped.to_string());
    }

    cmd.arg(tmp.path());
    run_checked(&mut cmd, "magick")?;
    tmp.commit(dest)
}

fn convert_media_file(
    src: &Path,
    src_fmt: Format,
    target: Format,
    out_dir: &Path,
    dest: &Path,
    backends: &Backends,
) -> Result<(), String> {
    let ffmpeg = backends
        .ffmpeg
        .as_deref()
        .ok_or_else(|| "Audio/video conversion requires 'ffmpeg'".to_string())?;
    let tmp = TempFileGuard::new(out_dir, target.ext());
    let mut cmd = Command::new(ffmpeg);
    cmd.arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-y")
        .arg("-i")
        .arg(src);

    match target {
        Format::Mp3 => {
            cmd.args(["-vn", "-c:a", "libmp3lame", "-q:a", "2"]);
        }
        Format::Wav => {
            cmd.args(["-vn", "-c:a", "pcm_s16le"]);
        }
        Format::Flac => {
            cmd.args(["-vn", "-c:a", "flac"]);
        }
        Format::Ogg => {
            cmd.args(["-vn", "-c:a", "libvorbis", "-q:a", "5"]);
        }
        Format::Opus => {
            cmd.args(["-vn", "-c:a", "libopus", "-b:a", "128k"]);
        }
        Format::M4a => {
            cmd.args(["-vn", "-c:a", "aac", "-b:a", "192k"]);
        }
        Format::Mp4 => {
            if src_fmt == Format::Gif {
                cmd.args([
                    "-movflags",
                    "+faststart",
                    "-pix_fmt",
                    "yuv420p",
                    "-vf",
                    "scale=trunc(iw/2)*2:trunc(ih/2)*2",
                    "-c:v",
                    "libx264",
                    "-crf",
                    "20",
                ]);
            } else {
                cmd.args([
                    "-c:v",
                    "libx264",
                    "-pix_fmt",
                    "yuv420p",
                    "-crf",
                    "22",
                    "-preset",
                    "medium",
                    "-c:a",
                    "aac",
                    "-b:a",
                    "192k",
                    "-movflags",
                    "+faststart",
                ]);
            }
        }
        Format::Mkv => {
            cmd.args([
                "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "22", "-preset", "medium",
                "-c:a", "aac", "-b:a", "192k",
            ]);
        }
        Format::Mov => {
            cmd.args([
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-crf",
                "22",
                "-preset",
                "medium",
                "-c:a",
                "aac",
                "-b:a",
                "192k",
                "-movflags",
                "+faststart",
            ]);
        }
        Format::Webm => {
            if src_fmt == Format::Gif {
                cmd.args([
                    "-c:v",
                    "libvpx-vp9",
                    "-crf",
                    "28",
                    "-b:v",
                    "0",
                    "-pix_fmt",
                    "yuv420p",
                ]);
            } else {
                cmd.args([
                    "-c:v",
                    "libvpx-vp9",
                    "-crf",
                    "30",
                    "-b:v",
                    "0",
                    "-c:a",
                    "libopus",
                    "-b:a",
                    "128k",
                ]);
            }
        }
        Format::Gif => {
            cmd.args([
                "-vf",
                "fps=15,scale='min(640,iw)':-1:flags=lanczos,split[s0][s1];[s0]palettegen=stats_mode=diff[p];[s1][p]paletteuse=dither=bayer:bayer_scale=3",
            ]);
        }
        _ => return Err(format!("Unsupported ffmpeg target format: {target}")),
    }

    cmd.arg(tmp.path());
    run_checked(&mut cmd, "ffmpeg")?;
    tmp.commit(dest)
}

fn convert_pdf_file(
    src: &Path,
    target: Format,
    out_dir: &Path,
    dest: &Path,
    backends: &Backends,
    opts: &ConvertOptions,
) -> Result<Vec<PathBuf>, String> {
    if target == Format::Txt {
        let pdftotext = backends
            .pdftotext
            .as_deref()
            .ok_or_else(|| "PDF to text requires 'pdftotext' (poppler)".to_string())?;
        let tmp = TempFileGuard::new(out_dir, "txt");
        let mut cmd = Command::new(pdftotext);
        cmd.arg(src).arg(tmp.path());
        run_checked(&mut cmd, "pdftotext")?;
        tmp.commit(dest)?;
        return Ok(vec![dest.to_path_buf()]);
    }

    let pdftoppm = backends
        .pdftoppm
        .as_deref()
        .ok_or_else(|| "PDF to image requires 'pdftoppm' (poppler)".to_string())?;
    let stage = TempDirGuard::create(out_dir)?;
    let prefix = stage.path().join("page");
    let mut cmd = Command::new(pdftoppm);
    match target {
        Format::Png | Format::Webp => {
            cmd.arg("-png");
        }
        Format::Jpg => {
            cmd.arg("-jpeg");
        }
        _ => return Err(format!("Unsupported PDF target: {target}")),
    }
    cmd.arg("-r").arg("200").arg(src).arg(&prefix);
    run_checked(&mut cmd, "pdftoppm")?;

    let mut pages: Vec<PathBuf> = fs::read_dir(stage.path())
        .map_err(|e| format!("Failed to read rendered PDF pages: {e}"))?
        .filter_map(|e| e.ok().map(|x| x.path()))
        .filter(|p| p.is_file())
        .collect();
    pages.sort();

    if pages.is_empty() {
        return Err(format!("No pages rendered from {}", src.display()));
    }

    let stem = stem_without_format(src);
    let mut outputs = Vec::with_capacity(pages.len());

    for (idx, page_path) in pages.iter().enumerate() {
        let final_path = if pages.len() == 1 {
            dest.to_path_buf()
        } else {
            pick_page_destination(out_dir, &stem, idx + 1, target.ext(), opts.overwrite)
        };

        if target == Format::Webp {
            convert_image_file(
                page_path,
                Format::Png,
                Format::Webp,
                out_dir,
                &final_path,
                backends,
                opts,
            )?;
        } else {
            fs::rename(page_path, &final_path).map_err(|e| {
                format!(
                    "Failed to move rendered page to {}: {e}",
                    final_path.display()
                )
            })?;
        }
        outputs.push(final_path);
    }

    Ok(outputs)
}

fn convert_archive_file(
    src: &Path,
    src_fmt: Format,
    target: Format,
    out_dir: &Path,
    dest: &Path,
    backends: &Backends,
) -> Result<(), String> {
    let stage = TempDirGuard::create(out_dir)?;
    extract_archive_safely(src, src_fmt, stage.path(), backends)?;

    let entries: Vec<String> = fs::read_dir(stage.path())
        .map_err(|e| format!("Failed to list extracted archive contents: {e}"))?
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    if entries.is_empty() {
        return Err(format!("Archive is empty: {}", src.display()));
    }

    let tmp = TempFileGuard::new(out_dir, target.ext());
    pack_archive(stage.path(), &entries, tmp.path(), target, backends)?;
    tmp.commit(dest)
}

fn extract_archive_safely(
    src: &Path,
    src_fmt: Format,
    stage_dir: &Path,
    backends: &Backends,
) -> Result<(), String> {
    match src_fmt {
        Format::Tar | Format::TarGz | Format::TarZst | Format::TarXz | Format::TarBz2 => {
            let tar = backends
                .tar
                .as_deref()
                .or(backends.bsdtar.as_deref())
                .ok_or_else(|| "Extracting tar archives requires 'tar' or 'bsdtar'".to_string())?;
            let list = run_capture(Command::new(tar).arg("-tf").arg(src), "tar")?;
            validate_member_paths(&list)?;
            let mut cmd = Command::new(tar);
            cmd.arg("-xf").arg(src).arg("-C").arg(stage_dir);
            run_checked(&mut cmd, "tar")
        }
        Format::Zip => {
            if let Some(bsdtar) = &backends.bsdtar {
                let list = run_capture(Command::new(bsdtar).arg("-tf").arg(src), "bsdtar")?;
                validate_member_paths(&list)?;
                let mut cmd = Command::new(bsdtar);
                cmd.arg("-xf").arg(src).arg("-C").arg(stage_dir);
                run_checked(&mut cmd, "bsdtar")
            } else if let Some(unzip) = &backends.unzip {
                let list = run_capture(Command::new(unzip).args(["-Z1"]).arg(src), "unzip")?;
                validate_member_paths(&list)?;
                let mut cmd = Command::new(unzip);
                cmd.args(["-qq"]).arg(src).arg("-d").arg(stage_dir);
                run_checked(&mut cmd, "unzip")
            } else if let Some(seven_z) = &backends.seven_z {
                extract_with_7z(seven_z, src, stage_dir)
            } else {
                Err("Extracting ZIP archives requires 'bsdtar', 'unzip', or '7z'".to_string())
            }
        }
        Format::SevenZ => {
            let seven_z = backends
                .seven_z
                .as_deref()
                .ok_or_else(|| "Extracting 7z archives requires '7z'".to_string())?;
            extract_with_7z(seven_z, src, stage_dir)
        }
        Format::Rar => {
            if let Some(seven_z) = &backends.seven_z {
                extract_with_7z(seven_z, src, stage_dir)
            } else if let Some(unrar) = &backends.unrar {
                let list = run_capture(Command::new(unrar).arg("lb").arg(src), "unrar")?;
                validate_member_paths(&list)?;
                let mut cmd = Command::new(unrar);
                cmd.args(["x", "-inul", "-y"]).arg(src).arg(stage_dir);
                run_checked(&mut cmd, "unrar")
            } else {
                Err("Extracting RAR archives requires '7z' or 'unrar'".to_string())
            }
        }
        _ => Err(format!("Unsupported archive format: {src_fmt}")),
    }
}

fn extract_with_7z(seven_z: &Path, src: &Path, stage_dir: &Path) -> Result<(), String> {
    let listing = run_capture(Command::new(seven_z).args(["l", "-slt"]).arg(src), "7z")?;
    let mut paths = String::new();
    for line in listing.lines() {
        if let Some(p) = line.strip_prefix("Path = ") {
            if p != src.to_string_lossy() {
                paths.push_str(p);
                paths.push('\n');
            }
        }
    }
    validate_member_paths(&paths)?;
    let mut out_arg = std::ffi::OsString::from("-o");
    out_arg.push(stage_dir.as_os_str());
    let mut cmd = Command::new(seven_z);
    cmd.args(["x", "-y", "-bd"]).arg(out_arg).arg(src);
    run_checked(&mut cmd, "7z")
}

pub fn validate_member_paths(listing: &str) -> Result<(), String> {
    for raw in listing.lines() {
        let entry = raw.trim();
        if entry.is_empty() {
            continue;
        }
        let normalized = entry.replace('\\', "/");
        let path = Path::new(&normalized);
        if path.is_absolute() {
            return Err(format!("Refusing unsafe absolute archive path: {entry}"));
        }
        for comp in path.components() {
            if matches!(
                comp,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            ) {
                return Err(format!("Refusing unsafe archive traversal path: {entry}"));
            }
        }
    }
    Ok(())
}

fn pack_archive(
    stage_dir: &Path,
    entries: &[String],
    out_path: &Path,
    target: Format,
    backends: &Backends,
) -> Result<(), String> {
    let abs_out = if out_path.is_absolute() {
        out_path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| format!("Failed to read cwd: {e}"))?
            .join(out_path)
    };
    match target {
        Format::Zip => {
            if let Some(zip) = &backends.zip {
                let mut cmd = Command::new(zip);
                cmd.current_dir(stage_dir)
                    .args(["-rq"])
                    .arg(&abs_out)
                    .args(entries);
                run_checked(&mut cmd, "zip")
            } else if let Some(bsdtar) = &backends.bsdtar {
                let mut cmd = Command::new(bsdtar);
                cmd.current_dir(stage_dir)
                    .args(["--format", "zip", "-cf"])
                    .arg(&abs_out)
                    .args(entries);
                run_checked(&mut cmd, "bsdtar")
            } else if let Some(seven_z) = &backends.seven_z {
                let mut cmd = Command::new(seven_z);
                cmd.current_dir(stage_dir)
                    .args(["a", "-tzip", "-bd", "-y"])
                    .arg(&abs_out)
                    .args(entries);
                run_checked(&mut cmd, "7z")
            } else {
                Err("Creating ZIP archives requires 'zip', 'bsdtar', or '7z'".to_string())
            }
        }
        Format::SevenZ => {
            let seven_z = backends
                .seven_z
                .as_deref()
                .ok_or_else(|| "Creating 7z archives requires '7z'".to_string())?;
            let mut cmd = Command::new(seven_z);
            cmd.current_dir(stage_dir)
                .args(["a", "-t7z", "-bd", "-y"])
                .arg(&abs_out)
                .args(entries);
            run_checked(&mut cmd, "7z")
        }
        Format::Tar | Format::TarGz | Format::TarZst | Format::TarXz | Format::TarBz2 => {
            let tar = backends
                .tar
                .as_deref()
                .or(backends.bsdtar.as_deref())
                .ok_or_else(|| "Creating tar archives requires 'tar' or 'bsdtar'".to_string())?;
            let mut cmd = Command::new(tar);
            cmd.current_dir(stage_dir);
            match target {
                Format::Tar => {
                    cmd.arg("-cf");
                }
                Format::TarGz => {
                    cmd.arg("-czf");
                }
                Format::TarBz2 => {
                    cmd.arg("-cjf");
                }
                Format::TarXz => {
                    cmd.arg("-cJf");
                }
                Format::TarZst => {
                    cmd.arg("--zstd").arg("-cf");
                }
                _ => unreachable!(),
            }
            cmd.arg(&abs_out).args(entries);
            run_checked(&mut cmd, "tar")
        }
        _ => Err(format!("Cannot pack archive target: {target}")),
    }
}

fn convert_document_file(
    src: &Path,
    target: Format,
    out_dir: &Path,
    dest: &Path,
    backends: &Backends,
) -> Result<(), String> {
    if target == Format::Pdf {
        if let Some(lo) = &backends.libreoffice {
            return convert_with_libreoffice(lo, src, target, out_dir, dest);
        }
    }
    if let Some(pandoc) = &backends.pandoc {
        let tmp = TempFileGuard::new(out_dir, target.ext());
        let mut cmd = Command::new(pandoc);
        cmd.arg(src).arg("-o").arg(tmp.path());
        run_checked(&mut cmd, "pandoc")?;
        return tmp.commit(dest);
    }
    if let Some(lo) = &backends.libreoffice {
        return convert_with_libreoffice(lo, src, target, out_dir, dest);
    }
    Err("Document conversion requires 'pandoc' or 'libreoffice'".to_string())
}

fn convert_with_libreoffice(
    lo: &Path,
    src: &Path,
    target: Format,
    out_dir: &Path,
    dest: &Path,
) -> Result<(), String> {
    let stage = TempDirGuard::create(out_dir)?;
    let mut cmd = Command::new(lo);
    cmd.args(["--headless", "--convert-to"])
        .arg(target.ext())
        .arg("--outdir")
        .arg(stage.path())
        .arg(src);
    run_checked(&mut cmd, "libreoffice")?;
    let expected_ext = OsStr::new(target.ext());
    let produced = fs::read_dir(stage.path())
        .map_err(|e| format!("Failed to read LibreOffice output: {e}"))?
        .filter_map(|e| e.ok().map(|x| x.path()))
        .find(|p| p.extension() == Some(expected_ext))
        .ok_or_else(|| format!("LibreOffice produced no .{} file", target.ext()))?;
    fs::rename(&produced, dest).map_err(|e| {
        format!(
            "Failed to move {} to {}: {e}",
            produced.display(),
            dest.display()
        )
    })
}

fn run_checked(cmd: &mut Command, tool: &str) -> Result<(), String> {
    let output = cmd
        .output()
        .map_err(|e| format!("Failed to run '{tool}': {e}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = stderr
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("non-zero exit status");
    Err(format!("{tool} failed: {detail}"))
}

fn run_capture(cmd: &mut Command, tool: &str) -> Result<String, String> {
    let output = cmd
        .output()
        .map_err(|e| format!("Failed to run '{tool}': {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or("non-zero exit status");
        return Err(format!("{tool} failed: {detail}"));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_zip_slip_paths() {
        assert!(validate_member_paths("dir/file.txt\nsub/a.png\n").is_ok());
        assert!(validate_member_paths("../evil.sh\n").is_err());
        assert!(validate_member_paths("a/../../etc/passwd\n").is_err());
        assert!(validate_member_paths("/etc/passwd\n").is_err());
        assert!(validate_member_paths("..\\windows\\evil\n").is_err());
    }
}
