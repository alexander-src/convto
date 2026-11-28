use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::detect::Backends;
use crate::format::{detect_format, stem_without_format, Family, Format};

static SHRINK_SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShrinkGoal {
    /// Fit output under this byte ceiling at the highest possible quality.
    TargetBytes(u64),
    /// Target a percentage of the original file size (1..=99%) at the highest possible quality.
    SizePercent(f64),
    /// Direct perceptual quality level (1..=100).
    Quality(u8),
    /// Lossless metadata strip and maximum lossless recompression only.
    Lossless,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoPreset {
    Fast,
    Medium,
    Slow,
}

impl VideoPreset {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "fast" | "faster" => Some(Self::Fast),
            "medium" | "balanced" => Some(Self::Medium),
            "slow" | "high" | "quality" => Some(Self::Slow),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Medium => "medium",
            Self::Slow => "slow",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ShrinkOptions {
    pub goal: ShrinkGoal,
    pub target_format: Option<Format>,
    pub strip_metadata: bool,
    pub max_dim: Option<u32>,
    pub two_pass: bool,
    pub preset: VideoPreset,
    pub video_hevc: bool,
    pub in_place: bool,
    pub overwrite: bool,
    pub force: bool,
    pub dry_run: bool,
    pub out_dir: Option<PathBuf>,
}

impl Default for ShrinkOptions {
    fn default() -> Self {
        Self {
            goal: ShrinkGoal::Quality(82),
            target_format: None,
            strip_metadata: true,
            max_dim: None,
            two_pass: true,
            preset: VideoPreset::Medium,
            video_hevc: false,
            in_place: false,
            overwrite: false,
            force: false,
            dry_run: false,
            out_dir: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ShrinkOutcome {
    Shrunk {
        source: PathBuf,
        output: PathBuf,
        original_bytes: u64,
        final_bytes: u64,
        detail: String,
    },
    AlreadyOptimal {
        source: PathBuf,
        original_bytes: u64,
        reason: String,
    },
    DryRun {
        source: PathBuf,
        destination: PathBuf,
        original_bytes: u64,
        detail: String,
    },
}

struct TempPath {
    path: PathBuf,
    is_dir: bool,
    armed: bool,
}

impl TempPath {
    fn file(dir: &Path, ext: &str) -> Self {
        let pid = std::process::id();
        let seq = SHRINK_SEQ.fetch_add(1, Ordering::Relaxed);
        let path = dir.join(format!(".convto-shrink-{pid}-{seq}.{ext}"));
        Self {
            path,
            is_dir: false,
            armed: true,
        }
    }

    fn dir(parent: &Path) -> Result<Self, String> {
        let pid = std::process::id();
        let seq = SHRINK_SEQ.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(".convto-shrink-dir-{pid}-{seq}"));
        fs::create_dir_all(&path)
            .map_err(|e| format!("Failed to create temp dir {}: {e}", path.display()))?;
        Ok(Self {
            path,
            is_dir: true,
            armed: true,
        })
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn commit(mut self, dest: &Path) -> Result<(), String> {
        fs::rename(&self.path, dest).map_err(|e| {
            format!(
                "Failed to move {} to {}: {e}",
                self.path.display(),
                dest.display()
            )
        })?;
        self.armed = false;
        Ok(())
    }
}

impl Drop for TempPath {
    fn drop(&mut self) {
        if self.armed {
            if self.is_dir {
                let _ = fs::remove_dir_all(&self.path);
            } else {
                let _ = fs::remove_file(&self.path);
            }
        }
    }
}

/// Parse human size strings like `10MB`, `500 KB`, `2.5MiB`, `800k`, `1.2 GB`, `65536`.
pub fn parse_size_bytes(raw: &str) -> Result<u64, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Target size cannot be empty".to_string());
    }
    let split = trimmed
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(trimmed.len());
    let num_part = trimmed[..split].trim();
    let unit_part = trimmed[split..].trim().to_ascii_lowercase();
    let value: f64 = num_part
        .parse()
        .map_err(|_| format!("Invalid target size '{raw}' (examples: 10MB, 500KB, 2.5MiB)"))?;
    if !value.is_finite() || value <= 0.0 {
        return Err(format!("Target size must be greater than zero: '{raw}'"));
    }
    let mult: f64 = match unit_part.as_str() {
        "" | "b" | "byte" | "bytes" => 1.0,
        "k" | "kb" => 1_000.0,
        "kib" => 1_024.0,
        "m" | "mb" => 1_000_000.0,
        "mib" => 1_048_576.0,
        "g" | "gb" => 1_000_000_000.0,
        "gib" => 1_073_741_824.0,
        _ => {
            return Err(format!(
                "Unknown size unit '{unit_part}' in '{raw}' (use B, KB, MB, GB, KiB, MiB, GiB)"
            ));
        }
    };
    let bytes = value * mult;
    if !bytes.is_finite() || bytes < 1.0 || bytes > (u64::MAX as f64) {
        return Err(format!("Target size out of range: '{raw}'"));
    }
    Ok(bytes.floor() as u64)
}

pub fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_000_000_000 {
        format!("{:.2} GB", bytes as f64 / 1_000_000_000.0)
    } else if bytes >= 1_000_000 {
        format!("{:.2} MB", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1_000 {
        format!("{:.1} KB", bytes as f64 / 1_000.0)
    } else {
        format!("{bytes} B")
    }
}

pub fn savings_percent(before: u64, after: u64) -> f64 {
    if before == 0 {
        return 0.0;
    }
    ((before as f64 - after as f64) / before as f64) * 100.0
}

pub fn pick_shrink_destination(
    src: &Path,
    src_fmt: Format,
    out_fmt: Format,
    opts: &ShrinkOptions,
) -> Result<PathBuf, String> {
    if opts.in_place && src_fmt == out_fmt && opts.out_dir.is_none() {
        return Ok(src.to_path_buf());
    }
    let dir = match &opts.out_dir {
        Some(d) => {
            fs::create_dir_all(d)
                .map_err(|e| format!("Failed to create output directory {}: {e}", d.display()))?;
            d.clone()
        }
        None => src
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf(),
    };
    let stem = stem_without_format(src);
    let ext = out_fmt.ext();
    let base = if src_fmt == out_fmt && opts.out_dir.is_none() {
        dir.join(format!("{stem}-shrunk.{ext}"))
    } else {
        dir.join(format!("{stem}.{ext}"))
    };
    if opts.overwrite && base != src {
        return Ok(base);
    }
    if !base.exists() && base != src {
        return Ok(base);
    }
    let prefix = if src_fmt == out_fmt && opts.out_dir.is_none() {
        format!("{stem}-shrunk")
    } else {
        stem
    };
    for n in 1..=9999u32 {
        let cand = dir.join(format!("{prefix} ({n}).{ext}"));
        if !cand.exists() && cand != src {
            return Ok(cand);
        }
    }
    Err(format!(
        "Could not find a free destination filename for {}",
        src.display()
    ))
}

pub fn shrink_file(
    src: &Path,
    backends: &Backends,
    opts: &ShrinkOptions,
) -> Result<ShrinkOutcome, String> {
    if !src.exists() {
        return Err(format!("File not found: {}", src.display()));
    }
    if src.is_dir() {
        return Err(format!("Cannot shrink directory: {}", src.display()));
    }
    let original_bytes = fs::metadata(src)
        .map_err(|e| format!("Failed to stat {}: {e}", src.display()))?
        .len();
    let src_fmt =
        detect_format(src).ok_or_else(|| format!("Unrecognized file format: {}", src.display()))?;
    let out_fmt = opts.target_format.unwrap_or(src_fmt);
    validate_shrink_pair(src_fmt, out_fmt)?;

    let effective_limit = match opts.goal {
        ShrinkGoal::TargetBytes(b) => Some(b),
        ShrinkGoal::SizePercent(pct) => {
            if !(1.0..=99.0).contains(&pct) {
                return Err(format!(
                    "Size percentage must be between 1 and 99 (got {pct})"
                ));
            }
            Some(
                ((original_bytes as f64) * (pct / 100.0))
                    .round()
                    .max(1024.0) as u64,
            )
        }
        ShrinkGoal::Quality(_) | ShrinkGoal::Lossless => None,
    };

    if let Some(limit) = effective_limit {
        if original_bytes <= limit && src_fmt == out_fmt && opts.max_dim.is_none() && !opts.force {
            return Ok(ShrinkOutcome::AlreadyOptimal {
                source: src.to_path_buf(),
                original_bytes,
                reason: format!(
                    "already {} (target <= {})",
                    format_bytes(original_bytes),
                    format_bytes(limit)
                ),
            });
        }
    }

    let dest = pick_shrink_destination(src, src_fmt, out_fmt, opts)?;
    let out_dir = dest
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    if opts.dry_run {
        let goal_desc = match opts.goal {
            ShrinkGoal::TargetBytes(b) => format!("target <= {}", format_bytes(b)),
            ShrinkGoal::SizePercent(p) => {
                format!(
                    "target <= {p:.0}% ({})",
                    format_bytes(effective_limit.unwrap_or(original_bytes))
                )
            }
            ShrinkGoal::Quality(q) => format!("quality {q}%"),
            ShrinkGoal::Lossless => "lossless optimize".to_string(),
        };
        return Ok(ShrinkOutcome::DryRun {
            source: src.to_path_buf(),
            destination: dest,
            original_bytes,
            detail: format!("{src_fmt} -> {out_fmt}, {goal_desc}"),
        });
    }

    let (tmp_result, detail) = match (src_fmt.family(), out_fmt.family()) {
        (Family::Image | Family::Gif, Family::Image | Family::Gif) => shrink_image(
            src,
            src_fmt,
            out_fmt,
            out_dir,
            effective_limit,
            backends,
            opts,
        )?,
        (Family::Video | Family::Gif, Family::Video) => {
            shrink_video(src, out_fmt, out_dir, effective_limit, backends, opts)?
        }
        (Family::Audio | Family::Video, Family::Audio) => {
            shrink_audio(src, out_fmt, out_dir, effective_limit, backends, opts)?
        }
        (Family::Pdf, Family::Pdf) => shrink_pdf(src, out_dir, effective_limit, backends, opts)?,
        _ => {
            return Err(format!(
                "Shrinking {src_fmt} -> {out_fmt} is not supported (supported: images, GIF, video, audio, PDF)"
            ));
        }
    };

    let final_bytes = fs::metadata(tmp_result.path())
        .map_err(|e| format!("Failed to stat output {}: {e}", tmp_result.path().display()))?
        .len();

    // Never leave the user with a larger file when staying in the same format unless forced.
    if src_fmt == out_fmt && opts.max_dim.is_none() && final_bytes >= original_bytes && !opts.force
    {
        drop(tmp_result);
        return Ok(ShrinkOutcome::AlreadyOptimal {
            source: src.to_path_buf(),
            original_bytes,
            reason: format!(
                "already optimal ({} could not be reduced further)",
                format_bytes(original_bytes)
            ),
        });
    }

    tmp_result.commit(&dest)?;

    Ok(ShrinkOutcome::Shrunk {
        source: src.to_path_buf(),
        output: dest,
        original_bytes,
        final_bytes,
        detail,
    })
}

fn validate_shrink_pair(src: Format, out: Format) -> Result<(), String> {
    let ok = match (src.family(), out.family()) {
        (Family::Image | Family::Gif, Family::Image | Family::Gif) => matches!(
            out,
            Format::Png
                | Format::Jpg
                | Format::Webp
                | Format::Avif
                | Format::Jxl
                | Format::Gif
                | Format::Tiff
                | Format::Bmp
        ),
        (Family::Video | Family::Gif, Family::Video) => {
            matches!(out, Format::Mp4 | Format::Mkv | Format::Webm | Format::Mov)
        }
        (Family::Audio | Family::Video, Family::Audio) => {
            matches!(
                out,
                Format::Mp3 | Format::Opus | Format::Ogg | Format::M4a | Format::Flac | Format::Wav
            )
        }
        (Family::Pdf, Family::Pdf) => true,
        _ => false,
    };
    if ok {
        Ok(())
    } else {
        Err(format!(
            "Cannot shrink {src} ({}) into {out} ({})",
            src.family(),
            out.family()
        ))
    }
}

// ============================================================================
// Images (binary-search quality solver + smart PNG quantization + gentle scale)
// ============================================================================

fn shrink_image(
    src: &Path,
    src_fmt: Format,
    out_fmt: Format,
    out_dir: &Path,
    target_bytes: Option<u64>,
    backends: &Backends,
    opts: &ShrinkOptions,
) -> Result<(TempPath, String), String> {
    let tool = backends.image_tool().ok_or_else(|| {
        "Image shrinking requires ImageMagick ('magick' or 'convert')".to_string()
    })?;

    if matches!(opts.goal, ShrinkGoal::Lossless) {
        let tmp = encode_image_trial(
            tool, src, src_fmt, out_fmt, out_dir, None, None, None, true, opts,
        )?;
        return Ok((tmp, "lossless".to_string()));
    }

    if out_fmt == Format::Png {
        return shrink_png(tool, src, src_fmt, out_dir, target_bytes, backends, opts);
    }

    if let Some(limit) = target_bytes {
        // Binary search quality 14..=96 to find the highest quality fitting <= limit.
        let mut lo: u8 = 14;
        let mut hi: u8 = 96;
        let mut best: Option<(TempPath, u8, u64)> = None;
        let mut smallest: Option<(TempPath, u8, u64)> = None;

        while lo <= hi {
            let mid = lo + (hi - lo) / 2;
            let cand = encode_image_trial(
                tool,
                src,
                src_fmt,
                out_fmt,
                out_dir,
                Some(mid),
                None,
                None,
                false,
                opts,
            )?;
            let sz = fs::metadata(cand.path())
                .map(|m| m.len())
                .unwrap_or(u64::MAX);
            if sz <= limit {
                best = Some((cand, mid, sz));
                if mid == 96 {
                    break;
                }
                lo = mid + 1;
            } else {
                if smallest.as_ref().is_none_or(|(_, _, s)| sz < *s) {
                    smallest = Some((cand, mid, sz));
                }
                if mid == 14 {
                    break;
                }
                hi = mid - 1;
            }
        }

        if let Some((tmp, q, _)) = best {
            return Ok((tmp, format!("binary-search q={q}")));
        }

        // Even q=14 exceeded the target budget; gently scale down dimensions to fit cleanly
        // at a crisp quality instead of blocky artifacts.
        let (_, _, min_sz) = smallest.as_ref().unwrap();
        let mut scale_pct = (((limit as f64 / (*min_sz as f64)).sqrt() * 0.92 * 100.0).round()
            as u32)
            .clamp(15, 85);

        for _ in 0..4 {
            let mut s_lo: u8 = 25;
            let mut s_hi: u8 = 88;
            let mut scaled_best: Option<(TempPath, u8)> = None;
            while s_lo <= s_hi {
                let mid = s_lo + (s_hi - s_lo) / 2;
                let cand = encode_image_trial(
                    tool,
                    src,
                    src_fmt,
                    out_fmt,
                    out_dir,
                    Some(mid),
                    Some(scale_pct),
                    None,
                    false,
                    opts,
                )?;
                let sz = fs::metadata(cand.path())
                    .map(|m| m.len())
                    .unwrap_or(u64::MAX);
                if sz <= limit {
                    scaled_best = Some((cand, mid));
                    if mid == 88 {
                        break;
                    }
                    s_lo = mid + 1;
                } else {
                    if mid == 25 {
                        break;
                    }
                    s_hi = mid - 1;
                }
            }
            if let Some((tmp, q)) = scaled_best {
                return Ok((tmp, format!("scaled {scale_pct}% q={q}")));
            }
            if scale_pct <= 15 {
                break;
            }
            scale_pct = (scale_pct * 7 / 10).max(15);
        }

        // Fallback: return the smallest result achieved.
        let (tmp, q, _) = smallest.unwrap();
        return Ok((tmp, format!("best-effort q={q}")));
    }

    let q = match opts.goal {
        ShrinkGoal::Quality(q) => q.clamp(1, 100),
        _ => 82,
    };
    let tmp = encode_image_trial(
        tool,
        src,
        src_fmt,
        out_fmt,
        out_dir,
        Some(q),
        None,
        None,
        false,
        opts,
    )?;
    Ok((tmp, format!("quality q={q}")))
}

fn shrink_png(
    tool: &Path,
    src: &Path,
    src_fmt: Format,
    out_dir: &Path,
    target_bytes: Option<u64>,
    backends: &Backends,
    opts: &ShrinkOptions,
) -> Result<(TempPath, String), String> {
    // Try max-effort lossless PNG first.
    let lossless = encode_image_trial(
        tool,
        src,
        src_fmt,
        Format::Png,
        out_dir,
        None,
        None,
        None,
        true,
        opts,
    )?;
    let lossless_sz = fs::metadata(lossless.path())
        .map(|m| m.len())
        .unwrap_or(u64::MAX);

    if let Some(limit) = target_bytes {
        if lossless_sz <= limit {
            return Ok((lossless, "lossless png".to_string()));
        }
        // Try pngquant if available before ImageMagick palette reduction.
        if let Some(pq) = backends.pngquant.as_deref() {
            for (min_q, max_q) in [(75, 95), (55, 85), (35, 70), (15, 55)] {
                let tmp = TempPath::file(out_dir, "png");
                let st = Command::new(pq)
                    .arg("--force")
                    .arg("--speed")
                    .arg("3")
                    .arg("--quality")
                    .arg(format!("{min_q}-{max_q}"))
                    .arg("--output")
                    .arg(tmp.path())
                    .arg(lossless.path())
                    .status();
                if st.map(|s| s.success()).unwrap_or(false) {
                    let sz = fs::metadata(tmp.path())
                        .map(|m| m.len())
                        .unwrap_or(u64::MAX);
                    if sz <= limit {
                        return Ok((tmp, format!("pngquant {min_q}-{max_q}")));
                    }
                }
            }
        }
        // Palette quantization levels (256 -> 128 -> 64 -> 32 colors) keeping full resolution.
        for colors in [256u32, 128, 64, 32] {
            let cand = encode_image_trial(
                tool,
                src,
                src_fmt,
                Format::Png,
                out_dir,
                None,
                None,
                Some(colors),
                false,
                opts,
            )?;
            let sz = fs::metadata(cand.path())
                .map(|m| m.len())
                .unwrap_or(u64::MAX);
            if sz <= limit {
                return Ok((cand, format!("palette {colors} colors")));
            }
        }
        // Gentle downscale + 256-color palette to meet tight byte target.
        let mut scale = 80u32;
        for _ in 0..5 {
            let cand = encode_image_trial(
                tool,
                src,
                src_fmt,
                Format::Png,
                out_dir,
                None,
                Some(scale),
                Some(256),
                false,
                opts,
            )?;
            let sz = fs::metadata(cand.path())
                .map(|m| m.len())
                .unwrap_or(u64::MAX);
            if sz <= limit {
                return Ok((cand, format!("scaled {scale}% palette 256")));
            }
            scale = (scale * 7 / 10).max(15);
        }
        // Return smallest quantized version if still slightly above target.
        let fallback = encode_image_trial(
            tool,
            src,
            src_fmt,
            Format::Png,
            out_dir,
            None,
            Some(scale),
            Some(64),
            false,
            opts,
        )?;
        return Ok((fallback, format!("best-effort scaled {scale}%")));
    }

    let q = match opts.goal {
        ShrinkGoal::Quality(q) => q.clamp(1, 100),
        _ => 85,
    };
    if q >= 95 {
        return Ok((lossless, "lossless png".to_string()));
    }
    if let Some(pq) = backends.pngquant.as_deref() {
        let min_q = q.saturating_sub(15).max(10);
        let tmp = TempPath::file(out_dir, "png");
        let st = Command::new(pq)
            .arg("--force")
            .arg("--speed")
            .arg("3")
            .arg("--quality")
            .arg(format!("{min_q}-{q}"))
            .arg("--output")
            .arg(tmp.path())
            .arg(lossless.path())
            .status();
        if st.map(|s| s.success()).unwrap_or(false) {
            return Ok((tmp, format!("pngquant q={q}")));
        }
    }
    let colors = match q {
        80..=94 => 256,
        60..=79 => 128,
        40..=59 => 64,
        _ => 32,
    };
    let quant = encode_image_trial(
        tool,
        src,
        src_fmt,
        Format::Png,
        out_dir,
        None,
        None,
        Some(colors),
        false,
        opts,
    )?;
    let q_sz = fs::metadata(quant.path())
        .map(|m| m.len())
        .unwrap_or(u64::MAX);
    if q_sz < lossless_sz {
        Ok((quant, format!("palette {colors} colors")))
    } else {
        Ok((lossless, "lossless png".to_string()))
    }
}

#[allow(clippy::too_many_arguments)]
fn encode_image_trial(
    tool: &Path,
    src: &Path,
    src_fmt: Format,
    out_fmt: Format,
    out_dir: &Path,
    quality: Option<u8>,
    scale_pct: Option<u32>,
    palette_colors: Option<u32>,
    lossless: bool,
    opts: &ShrinkOptions,
) -> Result<TempPath, String> {
    let tmp = TempPath::file(out_dir, out_fmt.ext());
    let mut cmd = Command::new(tool);

    if src_fmt == Format::Svg {
        cmd.args(["-background", "none", "-density", "192"]);
    }
    cmd.arg(src);

    if src_fmt == Format::Gif && out_fmt != Format::Gif && out_fmt != Format::Webp {
        cmd.args(["-coalesce", "-delete", "1--1"]);
    } else if src_fmt == Format::Gif {
        cmd.arg("-coalesce");
    }

    if opts.strip_metadata {
        cmd.arg("-strip");
    }

    if let Some(dim) = opts.max_dim {
        cmd.arg("-resize").arg(format!("{dim}x{dim}>"));
    }
    if let Some(pct) = scale_pct {
        cmd.arg("-filter")
            .arg("Lanczos")
            .arg("-resize")
            .arg(format!("{pct}%"));
    }

    if matches!(out_fmt, Format::Jpg | Format::Bmp) {
        cmd.args(["-background", "white", "-alpha", "remove", "-alpha", "off"]);
    }

    match out_fmt {
        Format::Jpg => {
            cmd.args(["-interlace", "Plane", "-sampling-factor", "4:2:0"]);
            if let Some(q) = quality {
                cmd.arg("-quality").arg(q.to_string());
            } else if lossless {
                cmd.args(["-quality", "95"]);
            }
        }
        Format::Webp => {
            if lossless {
                cmd.args(["-define", "webp:lossless=true", "-define", "webp:method=6"]);
            } else {
                cmd.args(["-define", "webp:method=5"]);
                if let Some(q) = quality {
                    cmd.arg("-quality").arg(q.to_string());
                }
            }
        }
        Format::Avif => {
            if let Some(q) = quality {
                cmd.arg("-quality").arg(q.to_string());
            } else if lossless {
                cmd.args(["-quality", "100"]);
            }
        }
        Format::Jxl => {
            if lossless {
                cmd.args(["-quality", "100"]);
            } else if let Some(q) = quality {
                cmd.arg("-quality").arg(q.to_string());
            }
        }
        Format::Png => {
            if let Some(colors) = palette_colors {
                cmd.args(["-dither", "FloydSteinberg", "-colors"])
                    .arg(colors.to_string());
            }
            cmd.args([
                "-define",
                "png:compression-level=9",
                "-define",
                "png:compression-filter=5",
                "-define",
                "png:compression-strategy=1",
            ]);
        }
        Format::Gif => {
            cmd.args(["-layers", "Optimize"]);
            if let Some(q) = quality {
                let colors = ((q as u32 * 224) / 100 + 32).clamp(32, 256);
                cmd.arg("-colors").arg(colors.to_string());
            }
        }
        _ => {
            if let Some(q) = quality {
                cmd.arg("-quality").arg(q.to_string());
            }
        }
    }

    cmd.arg(tmp.path());
    run_checked(&mut cmd, "magick")?;
    Ok(tmp)
}

// ============================================================================
// Video & Audio (ffprobe duration + 2-pass exact bitrate targeting)
// ============================================================================

#[derive(Debug, Clone)]
pub struct MediaInfo {
    pub duration_secs: f64,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub has_audio: bool,
}

pub fn probe_media(src: &Path, backends: &Backends) -> Result<MediaInfo, String> {
    let ffprobe = backends
        .ffprobe
        .as_deref()
        .ok_or_else(|| "Exact target-size media shrinking requires 'ffprobe'".to_string())?;
    let out = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration:stream=codec_type,width,height",
            "-of",
            "default=noprint_wrappers=1",
        ])
        .arg(src)
        .output()
        .map_err(|e| format!("Failed to run ffprobe: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "ffprobe failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut duration_secs = 0.0f64;
    let mut width = None;
    let mut height = None;
    let mut has_audio = false;
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("duration=") {
            if let Ok(d) = v.trim().parse::<f64>() {
                if d.is_finite() && d > 0.0 {
                    duration_secs = d;
                }
            }
        } else if line.trim() == "codec_type=audio" {
            has_audio = true;
        } else if let Some(v) = line.strip_prefix("width=") {
            if width.is_none() {
                width = v.trim().parse::<u32>().ok();
            }
        } else if let Some(v) = line.strip_prefix("height=") {
            if height.is_none() {
                height = v.trim().parse::<u32>().ok();
            }
        }
    }
    if duration_secs <= 0.0 {
        return Err(format!(
            "Could not determine media duration for {}",
            src.display()
        ));
    }
    Ok(MediaInfo {
        duration_secs,
        width,
        height,
        has_audio,
    })
}

/// Compute `(video_kbps, audio_kbps, auto_max_height)` for a target byte budget.
pub fn compute_video_bitrates(
    target_bytes: u64,
    duration_secs: f64,
    has_audio: bool,
    src_height: Option<u32>,
) -> (u32, u32, Option<u32>) {
    // 6% mux/container safety headroom so final output stays comfortably under target_bytes.
    let total_bps = (target_bytes as f64 * 8.0 * 0.94) / duration_secs.max(0.1);
    let audio_kbps: u32 = if !has_audio {
        0
    } else if total_bps >= 1_500_000.0 {
        128
    } else if total_bps >= 500_000.0 {
        96
    } else if total_bps >= 200_000.0 {
        64
    } else {
        32
    };
    let video_kbps = (((total_bps / 1000.0) - (audio_kbps as f64)).max(50.0)).floor() as u32;
    let auto_max_h = match src_height {
        Some(h) if h > 480 && video_kbps < 250 => Some(480),
        Some(h) if h > 720 && video_kbps < 500 => Some(720),
        _ => None,
    };
    (video_kbps, audio_kbps, auto_max_h)
}

pub fn quality_to_crf(quality: u8) -> u8 {
    let q = quality.clamp(1, 100) as f64;
    (42.0 - (q / 100.0) * 26.0).round().clamp(16.0, 42.0) as u8
}

fn shrink_video(
    src: &Path,
    out_fmt: Format,
    out_dir: &Path,
    target_bytes: Option<u64>,
    backends: &Backends,
    opts: &ShrinkOptions,
) -> Result<(TempPath, String), String> {
    let ffmpeg = backends
        .ffmpeg
        .as_deref()
        .ok_or_else(|| "Video shrinking requires 'ffmpeg'".to_string())?;
    let tmp = TempPath::file(out_dir, out_fmt.ext());

    let (vcodec, acodec, is_webm) = if out_fmt == Format::Webm {
        ("libvpx-vp9", "libopus", true)
    } else if opts.video_hevc {
        ("libx265", "aac", false)
    } else {
        ("libx264", "aac", false)
    };

    if let Some(limit) = target_bytes {
        let info = probe_media(src, backends)?;
        let (video_kbps, audio_kbps, auto_max_h) =
            compute_video_bitrates(limit, info.duration_secs, info.has_audio, info.height);
        let max_dim = opts.max_dim.or(auto_max_h);
        let vf = build_video_scale_filter(max_dim);

        if opts.two_pass && !opts.video_hevc {
            let log_dir = TempPath::dir(out_dir)?;
            let passlog = log_dir.path().join("ffmpeg2pass");

            // Pass 1
            let mut p1 = Command::new(ffmpeg);
            p1.args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
                .arg(src)
                .args(["-vf", &vf, "-c:v", vcodec, "-b:v"])
                .arg(format!("{video_kbps}k"));
            if !is_webm {
                p1.args(["-preset", opts.preset.as_str(), "-pix_fmt", "yuv420p"]);
            } else {
                p1.args(["-pix_fmt", "yuv420p"]);
            }
            p1.args(["-pass", "1", "-passlogfile"])
                .arg(&passlog)
                .args(["-an", "-f", "null"])
                .arg(null_sink());
            run_checked(&mut p1, "ffmpeg (pass 1)")?;

            // Pass 2
            let mut p2 = Command::new(ffmpeg);
            p2.args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
                .arg(src)
                .args(["-vf", &vf, "-c:v", vcodec, "-b:v"])
                .arg(format!("{video_kbps}k"))
                .args([
                    "-maxrate",
                    &format!("{}k", (video_kbps * 3 / 2).max(video_kbps + 50)),
                ])
                .args(["-bufsize", &format!("{}k", (video_kbps * 2).max(100))]);
            if !is_webm {
                p2.args(["-preset", opts.preset.as_str(), "-pix_fmt", "yuv420p"]);
            } else {
                p2.args(["-pix_fmt", "yuv420p"]);
            }
            p2.args(["-pass", "2", "-passlogfile"]).arg(&passlog);
            if info.has_audio {
                p2.args(["-c:a", acodec, "-b:a", &format!("{audio_kbps}k")]);
            } else {
                p2.arg("-an");
            }
            if opts.strip_metadata {
                p2.args(["-map_metadata", "-1"]);
            }
            if matches!(out_fmt, Format::Mp4 | Format::Mov) {
                p2.args(["-movflags", "+faststart"]);
            }
            p2.arg(tmp.path());
            run_checked(&mut p2, "ffmpeg (pass 2)")?;
            return Ok((
                tmp,
                format!("2-pass {vcodec} {video_kbps}k (audio {audio_kbps}k)"),
            ));
        }

        // Single-pass ABR fallback (used for HEVC or when --no-two-pass is set).
        let mut cmd = Command::new(ffmpeg);
        cmd.args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
            .arg(src)
            .args(["-vf", &vf, "-c:v", vcodec, "-b:v"])
            .arg(format!("{video_kbps}k"))
            .args([
                "-maxrate",
                &format!("{}k", (video_kbps * 3 / 2).max(video_kbps + 50)),
            ])
            .args(["-bufsize", &format!("{}k", (video_kbps * 2).max(100))]);
        if !is_webm {
            cmd.args(["-preset", opts.preset.as_str(), "-pix_fmt", "yuv420p"]);
        } else {
            cmd.args(["-pix_fmt", "yuv420p"]);
        }
        if info.has_audio {
            cmd.args(["-c:a", acodec, "-b:a", &format!("{audio_kbps}k")]);
        } else {
            cmd.arg("-an");
        }
        if opts.strip_metadata {
            cmd.args(["-map_metadata", "-1"]);
        }
        if matches!(out_fmt, Format::Mp4 | Format::Mov) {
            cmd.args(["-movflags", "+faststart"]);
        }
        cmd.arg(tmp.path());
        run_checked(&mut cmd, "ffmpeg")?;
        return Ok((tmp, format!("1-pass {vcodec} {video_kbps}k")));
    }

    // Quality / Lossless mode via perceptual CRF.
    let crf = match opts.goal {
        ShrinkGoal::Lossless => 16,
        ShrinkGoal::Quality(q) => quality_to_crf(q),
        _ => 23,
    };
    let vf = build_video_scale_filter(opts.max_dim);
    let mut cmd = Command::new(ffmpeg);
    cmd.args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(src)
        .args(["-vf", &vf, "-c:v", vcodec, "-crf", &crf.to_string()]);
    if is_webm {
        cmd.args([
            "-b:v", "0", "-pix_fmt", "yuv420p", "-c:a", acodec, "-b:a", "128k",
        ]);
    } else {
        cmd.args([
            "-preset",
            opts.preset.as_str(),
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            acodec,
            "-b:a",
            "160k",
        ]);
    }
    if opts.strip_metadata {
        cmd.args(["-map_metadata", "-1"]);
    }
    if matches!(out_fmt, Format::Mp4 | Format::Mov) {
        cmd.args(["-movflags", "+faststart"]);
    }
    cmd.arg(tmp.path());
    run_checked(&mut cmd, "ffmpeg")?;
    Ok((tmp, format!("{vcodec} crf={crf}")))
}

fn build_video_scale_filter(max_dim: Option<u32>) -> String {
    match max_dim {
        Some(limit) => format!(
            "scale='if(gt(iw,ih),min({limit},iw),-2)':'if(gt(iw,ih),-2,min({limit},ih))',scale=trunc(iw/2)*2:trunc(ih/2)*2"
        ),
        None => "scale=trunc(iw/2)*2:trunc(ih/2)*2".to_string(),
    }
}

fn shrink_audio(
    src: &Path,
    out_fmt: Format,
    out_dir: &Path,
    target_bytes: Option<u64>,
    backends: &Backends,
    opts: &ShrinkOptions,
) -> Result<(TempPath, String), String> {
    let ffmpeg = backends
        .ffmpeg
        .as_deref()
        .ok_or_else(|| "Audio shrinking requires 'ffmpeg'".to_string())?;
    let tmp = TempPath::file(out_dir, out_fmt.ext());
    let mut cmd = Command::new(ffmpeg);
    cmd.args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(src)
        .arg("-vn");

    if opts.strip_metadata {
        cmd.args(["-map_metadata", "-1"]);
    }

    if out_fmt == Format::Flac {
        cmd.args(["-c:a", "flac", "-compression_level", "12"]);
        cmd.arg(tmp.path());
        run_checked(&mut cmd, "ffmpeg")?;
        return Ok((tmp, "flac level 12".to_string()));
    }
    if out_fmt == Format::Wav {
        cmd.args(["-c:a", "pcm_s16le"]);
        cmd.arg(tmp.path());
        run_checked(&mut cmd, "ffmpeg")?;
        return Ok((tmp, "wav pcm_s16le".to_string()));
    }

    let kbps: u32 = if let Some(limit) = target_bytes {
        let info = probe_media(src, backends)?;
        (((limit as f64 * 8.0 * 0.95) / info.duration_secs / 1000.0)
            .floor()
            .clamp(24.0, 320.0)) as u32
    } else {
        match opts.goal {
            ShrinkGoal::Lossless => 320,
            ShrinkGoal::Quality(q) => ((q.clamp(1, 100) as u32 * 288) / 100 + 32).clamp(32, 320),
            _ => 160,
        }
    };

    let codec = match out_fmt {
        Format::Mp3 => "libmp3lame",
        Format::Opus => "libopus",
        Format::Ogg => "libvorbis",
        Format::M4a => "aac",
        _ => return Err(format!("Unsupported audio shrink target: {out_fmt}")),
    };
    cmd.args(["-c:a", codec, "-b:a", &format!("{kbps}k")])
        .arg(tmp.path());
    run_checked(&mut cmd, "ffmpeg")?;
    Ok((tmp, format!("{codec} {kbps}k")))
}

// ============================================================================
// PDF (Ghostscript tiered DPI solver preserving crisp vector text)
// ============================================================================

fn shrink_pdf(
    src: &Path,
    out_dir: &Path,
    target_bytes: Option<u64>,
    backends: &Backends,
    opts: &ShrinkOptions,
) -> Result<(TempPath, String), String> {
    let gs = backends
        .gs
        .as_deref()
        .ok_or_else(|| "PDF shrinking requires Ghostscript ('gs')".to_string())?;

    if matches!(opts.goal, ShrinkGoal::Lossless) {
        let tmp = run_gs_pdf(gs, src, out_dir, "/prepress", None)?;
        return Ok((tmp, "gs /prepress".to_string()));
    }

    if let Some(limit) = target_bytes {
        let tiers: &[(&str, Option<u32>, &str)] = &[
            ("/prepress", Some(300), "gs 300dpi"),
            ("/printer", Some(200), "gs 200dpi"),
            ("/ebook", Some(150), "gs 150dpi"),
            ("/ebook", Some(110), "gs 110dpi"),
            ("/screen", Some(72), "gs 72dpi"),
            ("/screen", Some(50), "gs 50dpi"),
        ];
        let mut smallest: Option<(TempPath, u64, String)> = None;
        for &(preset, dpi, label) in tiers {
            let cand = run_gs_pdf(gs, src, out_dir, preset, dpi)?;
            let sz = fs::metadata(cand.path())
                .map(|m| m.len())
                .unwrap_or(u64::MAX);
            if sz <= limit {
                return Ok((cand, label.to_string()));
            }
            if smallest.as_ref().is_none_or(|(_, s, _)| sz < *s) {
                smallest = Some((cand, sz, label.to_string()));
            }
        }
        let (tmp, _, label) = smallest.unwrap();
        return Ok((tmp, format!("best-effort {label}")));
    }

    let q = match opts.goal {
        ShrinkGoal::Quality(q) => q.clamp(1, 100),
        _ => 75,
    };
    let (preset, dpi) = match q {
        85..=100 => ("/prepress", 300),
        65..=84 => ("/printer", 200),
        40..=64 => ("/ebook", 150),
        20..=39 => ("/screen", 96),
        _ => ("/screen", 72),
    };
    let tmp = run_gs_pdf(gs, src, out_dir, preset, Some(dpi))?;
    Ok((tmp, format!("gs {dpi}dpi ({preset})")))
}

fn run_gs_pdf(
    gs: &Path,
    src: &Path,
    out_dir: &Path,
    pdf_settings: &str,
    dpi: Option<u32>,
) -> Result<TempPath, String> {
    let tmp = TempPath::file(out_dir, "pdf");
    let mut cmd = Command::new(gs);
    cmd.args([
        "-sDEVICE=pdfwrite",
        "-dCompatibilityLevel=1.4",
        &format!("-dPDFSETTINGS={pdf_settings}"),
        "-dNOPAUSE",
        "-dQUIET",
        "-dBATCH",
        "-dDetectDuplicateImages=true",
        "-dCompressFonts=true",
        "-dSubsetFonts=true",
    ]);
    if let Some(d) = dpi {
        cmd.args([
            "-dDownsampleColorImages=true",
            "-dDownsampleGrayImages=true",
            "-dDownsampleMonoImages=true",
            &format!("-dColorImageResolution={d}"),
            &format!("-dGrayImageResolution={d}"),
            &format!("-dMonoImageResolution={d}"),
        ]);
    }
    let mut out_arg = std::ffi::OsString::from("-sOutputFile=");
    out_arg.push(tmp.path().as_os_str());
    cmd.arg(out_arg).arg(src);
    run_checked(&mut cmd, "gs")?;
    Ok(tmp)
}

fn null_sink() -> &'static str {
    if cfg!(windows) {
        "NUL"
    } else {
        "/dev/null"
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_human_byte_sizes() {
        assert_eq!(parse_size_bytes("10MB").unwrap(), 10_000_000);
        assert_eq!(parse_size_bytes("10 MB").unwrap(), 10_000_000);
        assert_eq!(parse_size_bytes("500kb").unwrap(), 500_000);
        assert_eq!(parse_size_bytes("2.5MiB").unwrap(), 2_621_440);
        assert_eq!(parse_size_bytes("1.5 GB").unwrap(), 1_500_000_000);
        assert_eq!(parse_size_bytes("4096").unwrap(), 4096);
        assert!(parse_size_bytes("0MB").is_err());
        assert!(parse_size_bytes("-5MB").is_err());
        assert!(parse_size_bytes("10XB").is_err());
    }

    #[test]
    fn video_bitrate_and_crf_math() {
        let (v, a, auto_h) = compute_video_bitrates(10_000_000, 60.0, true, Some(1080));
        assert_eq!(a, 96);
        assert!((1100..=1200).contains(&v), "v={v}");
        assert_eq!(auto_h, None);

        // Tight 10 MB budget over 5 minutes should auto-cap 1080p down to 480p.
        let (v2, a2, auto_h2) = compute_video_bitrates(10_000_000, 300.0, true, Some(1080));
        assert!(a2 <= 64);
        assert!(v2 < 250);
        assert_eq!(auto_h2, Some(480));

        assert_eq!(quality_to_crf(100), 16);
        assert_eq!(quality_to_crf(1), 42);
    }
}
