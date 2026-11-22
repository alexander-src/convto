#![forbid(unsafe_code)]

use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use convto::convert::{convert_file, ConvertOptions, Outcome};
use convto::desktop::{
    dolphin_menu_dir, install_menus, uninstall_menus, DesktopTarget, MENU_GROUPS,
};
use convto::detect::Backends;
use convto::format::{detect_format, Format};
use convto::gui::{has_gtk4_gui, launch_shrink_gui};
use convto::shrink::{
    format_bytes, parse_size_bytes, savings_percent, shrink_file, ShrinkGoal, ShrinkOptions,
    ShrinkOutcome, VideoPreset,
};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn print_usage() {
    println!(
        "convto {VERSION} - Linux right-click file converter, shrinker, and CLI\n\
\n\
USAGE:\n\
  convto [OPTIONS] <target> <file>...\n\
  convto [OPTIONS] to <target> <file>...\n\
  convto shrink [SHRINK OPTIONS] <file>...\n\
  convto targets <file>\n\
  convto formats\n\
  convto doctor\n\
  convto install [--desktop auto|dolphin|nautilus|nemo|thunar|all] [--all-menus] [--bin <path>]\n\
  convto uninstall [--desktop auto|dolphin|nautilus|nemo|thunar|all]\n\
\n\
CONVERT OPTIONS:\n\
  -n, --notify           Send a desktop notification via notify-send on completion\n\
  -f, --overwrite        Overwrite existing destination files instead of adding (1), (2)\n\
      --force            Re-encode even when input is already in the target format\n\
  -o, --out-dir <dir>    Write converted files into <dir>\n\
  -q, --quality <1-100>  Quality preset for lossy image formats (jpg, webp, avif, jxl)\n\
  -j, --jobs <n>         Number of parallel worker threads\n\
      --delete-source    Delete source file after a successful conversion\n\
      --dry-run          Show planned operations without modifying files\n\
\n\
SHRINK OPTIONS:\n\
  -s, --target-size <sz> Fit under size at highest quality (e.g. 10MB, 500KB, 2.5MiB)\n\
  -p, --percent <1-99>   Target percentage of original file size at highest quality\n\
  -q, --quality <1-100>  Direct perceptual quality level (default: 82)\n\
      --lossless         Lossless metadata strip + maximum lossless compression only\n\
      --to <fmt>         Optionally change format while shrinking (e.g. webp, avif, mp4, opus)\n\
      --hevc             Use H.265/HEVC (libx265) for MP4/MKV video shrinking\n\
      --max-dim <px>     Cap longest image edge or video resolution (e.g. 1920, 1280)\n\
      --preset <speed>   Video encoder speed: fast, medium, slow (default: medium)\n\
      --no-two-pass      Use single-pass video bitrate encoding instead of two-pass\n\
      --keep-metadata    Keep EXIF/GPS/media metadata (default strips metadata)\n\
      --in-place         Atomically replace the original file instead of saving *-shrunk\n\
      --gui              Open the interactive Shrink & Optimize dialog (GTK4 / kdialog / zenity)\n\
      --json             Print newline-delimited JSON status records\n\
\n\
EXAMPLES:\n\
  convto png photo.webp\n\
  convto mp3 video.mp4\n\
  convto yaml config.json\n\
  convto shrink clip.mp4 --target-size 10MB\n\
  convto shrink photo.png --percent 40 --to webp\n\
  convto shrink --gui photo.png clip.mp4\n\
  convto install"
    );
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        print_usage();
        return ExitCode::from(2);
    }

    let mut notify = false;
    let mut opts = ConvertOptions::default();
    let mut shrink_opts = ShrinkOptions::default();
    let mut shrink_goal_set = false;
    let mut launch_gui = false;
    let mut json_output = false;
    let mut jobs: Option<usize> = None;
    let mut desktop = DesktopTarget::Auto;
    let mut all_menus = false;
    let mut custom_bin: Option<PathBuf> = None;
    let mut positional: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        match a {
            "-h" | "--help" => {
                print_usage();
                return ExitCode::SUCCESS;
            }
            "-V" | "--version" => {
                println!("convto {VERSION}");
                return ExitCode::SUCCESS;
            }
            "-n" | "--notify" => notify = true,
            "-f" | "--overwrite" => {
                opts.overwrite = true;
                shrink_opts.overwrite = true;
            }
            "--force" => {
                opts.force_same = true;
                shrink_opts.force = true;
            }
            "--delete-source" => opts.delete_source = true,
            "--dry-run" => {
                opts.dry_run = true;
                shrink_opts.dry_run = true;
            }
            "--all-menus" => all_menus = true,
            "--lossless" => {
                shrink_opts.goal = ShrinkGoal::Lossless;
                shrink_goal_set = true;
            }
            "--hevc" => shrink_opts.video_hevc = true,
            "--no-two-pass" => shrink_opts.two_pass = false,
            "--keep-metadata" => shrink_opts.strip_metadata = false,
            "--strip" => shrink_opts.strip_metadata = true,
            "--in-place" => shrink_opts.in_place = true,
            "--gui" => launch_gui = true,
            "--json" => json_output = true,
            "-o" | "--out-dir" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    eprintln!("error: --out-dir requires a directory path");
                    return ExitCode::from(2);
                };
                opts.out_dir = Some(PathBuf::from(v));
                shrink_opts.out_dir = Some(PathBuf::from(v));
            }
            "-q" | "--quality" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    eprintln!("error: --quality requires an integer 1..=100");
                    return ExitCode::from(2);
                };
                match v.parse::<u8>() {
                    Ok(q) if (1..=100).contains(&q) => {
                        opts.quality = Some(q);
                        shrink_opts.goal = ShrinkGoal::Quality(q);
                        shrink_goal_set = true;
                    }
                    _ => {
                        eprintln!("error: invalid quality '{v}' (expected 1..=100)");
                        return ExitCode::from(2);
                    }
                }
            }
            "-s" | "--target-size" | "--size" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    eprintln!("error: --target-size requires a size (e.g. 10MB, 500KB)");
                    return ExitCode::from(2);
                };
                match parse_size_bytes(v) {
                    Ok(bytes) => {
                        shrink_opts.goal = ShrinkGoal::TargetBytes(bytes);
                        shrink_goal_set = true;
                    }
                    Err(e) => {
                        eprintln!("error: {e}");
                        return ExitCode::from(2);
                    }
                }
            }
            "-p" | "--percent" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    eprintln!("error: --percent requires a number 1..=99");
                    return ExitCode::from(2);
                };
                let cleaned = v.trim().trim_end_matches('%');
                match cleaned.parse::<f64>() {
                    Ok(p) if (1.0..=99.0).contains(&p) => {
                        shrink_opts.goal = ShrinkGoal::SizePercent(p);
                        shrink_goal_set = true;
                    }
                    _ => {
                        eprintln!("error: invalid percentage '{v}' (expected 1..=99)");
                        return ExitCode::from(2);
                    }
                }
            }
            "--to" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    eprintln!("error: --to requires a target format");
                    return ExitCode::from(2);
                };
                match Format::parse(v) {
                    Some(f) => shrink_opts.target_format = Some(f),
                    None => {
                        eprintln!("error: unknown target format '{v}'");
                        return ExitCode::from(2);
                    }
                }
            }
            "--max-dim" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    eprintln!("error: --max-dim requires a positive pixel value (e.g. 1920)");
                    return ExitCode::from(2);
                };
                match v.parse::<u32>() {
                    Ok(px) if px >= 32 => shrink_opts.max_dim = Some(px),
                    _ => {
                        eprintln!("error: invalid --max-dim '{v}' (expected >= 32)");
                        return ExitCode::from(2);
                    }
                }
            }
            "--preset" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    eprintln!("error: --preset requires fast, medium, or slow");
                    return ExitCode::from(2);
                };
                match VideoPreset::parse(v) {
                    Some(p) => shrink_opts.preset = p,
                    None => {
                        eprintln!("error: invalid --preset '{v}' (expected fast, medium, or slow)");
                        return ExitCode::from(2);
                    }
                }
            }
            "-j" | "--jobs" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    eprintln!("error: --jobs requires a positive integer");
                    return ExitCode::from(2);
                };
                match v.parse::<usize>() {
                    Ok(j) if j >= 1 => jobs = Some(j),
                    _ => {
                        eprintln!("error: invalid jobs '{v}' (expected >= 1)");
                        return ExitCode::from(2);
                    }
                }
            }
            "--desktop" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    eprintln!("error: --desktop requires auto|dolphin|nautilus|nemo|thunar|all");
                    return ExitCode::from(2);
                };
                match DesktopTarget::parse(v) {
                    Some(d) => desktop = d,
                    None => {
                        eprintln!("error: unknown desktop '{v}' (expected auto|dolphin|nautilus|nemo|thunar|all)");
                        return ExitCode::from(2);
                    }
                }
            }
            "--bin" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    eprintln!("error: --bin requires a binary path");
                    return ExitCode::from(2);
                };
                custom_bin = Some(PathBuf::from(v));
            }
            "--" => {
                positional.extend(args[i + 1..].iter().cloned());
                break;
            }
            other if other.starts_with('-') => {
                eprintln!("error: unknown option '{other}' (try --help)");
                return ExitCode::from(2);
            }
            _ => positional.push(args[i].clone()),
        }
        i += 1;
    }

    if positional.is_empty() {
        print_usage();
        return ExitCode::from(2);
    }

    let backends = Backends::probe();

    match positional[0].as_str() {
        "formats" => {
            cmd_formats();
            ExitCode::SUCCESS
        }
        "doctor" => {
            cmd_doctor(&backends);
            ExitCode::SUCCESS
        }
        "targets" => {
            let Some(path) = positional.get(1) else {
                eprintln!("error: 'convto targets' requires a file path");
                return ExitCode::from(2);
            };
            cmd_targets(Path::new(path))
        }
        "install" => cmd_install(desktop, all_menus, custom_bin, &backends),
        "uninstall" => cmd_uninstall(desktop, &backends),
        "shrink" | "optimize" => {
            let files = &positional[1..];
            if files.is_empty() {
                eprintln!("error: 'convto shrink' requires at least one file");
                return ExitCode::from(2);
            }
            if launch_gui {
                let exe = env::current_exe().unwrap_or_else(|_| PathBuf::from("convto"));
                match launch_shrink_gui(&exe, files, notify, &backends) {
                    Ok(()) => ExitCode::SUCCESS,
                    Err(e) => {
                        eprintln!("error: {e}");
                        if notify {
                            send_notification(&backends, "convto error", &e, true);
                        }
                        ExitCode::from(1)
                    }
                }
            } else {
                if !shrink_goal_set {
                    shrink_opts.goal = ShrinkGoal::Quality(82);
                }
                cmd_shrink(files, &backends, &shrink_opts, jobs, notify, json_output)
            }
        }
        "to" => {
            if positional.len() < 3 {
                eprintln!("error: 'convto to <target> <file>...' requires a target format and at least one file");
                return ExitCode::from(2);
            }
            cmd_convert(
                &positional[1],
                &positional[2..],
                &backends,
                &opts,
                jobs,
                notify,
            )
        }
        first => {
            if positional.len() < 2 {
                eprintln!(
                    "error: expected 'convto <target> <file>...' or a subcommand (try --help)"
                );
                return ExitCode::from(2);
            }
            cmd_convert(first, &positional[1..], &backends, &opts, jobs, notify)
        }
    }
}

fn cmd_formats() {
    println!("Supported format families and targets:\n");
    for group in MENU_GROUPS {
        let targets: Vec<&str> = group.actions.iter().map(|a| a.target.ext()).collect();
        println!("  {:10} -> {}", group.slug, targets.join(", "));
    }
    println!("\nBuilt-in (zero external tools): json, yaml, toml, csv, tsv");
    println!(
        "Shrink / optimize: images, GIF, video (2-pass target size), audio, PDF (Ghostscript)"
    );
}

fn cmd_targets(path: &Path) -> ExitCode {
    let Some(fmt) = detect_format(path) else {
        eprintln!("error: unrecognized format for {}", path.display());
        return ExitCode::from(1);
    };
    let targets: Vec<&str> = fmt
        .valid_targets()
        .iter()
        .copied()
        .filter(|&t| t != fmt)
        .map(Format::ext)
        .collect();
    println!("file:    {}", path.display());
    println!("format:  {fmt} ({})", fmt.family());
    println!("targets: {}", targets.join(", "));
    ExitCode::SUCCESS
}

fn cmd_doctor(backends: &Backends) {
    println!("convto {VERSION} environment check\n");
    println!("Backends:");
    let checks: &[(&str, Option<&Path>, &str)] = &[
        (
            "magick",
            backends.magick.as_deref(),
            "images convert & binary-search shrink",
        ),
        (
            "convert",
            backends.convert.as_deref(),
            "images fallback (ImageMagick 6)",
        ),
        (
            "rsvg-convert",
            backends.rsvg_convert.as_deref(),
            "SVG -> PNG vector rendering",
        ),
        (
            "ffmpeg",
            backends.ffmpeg.as_deref(),
            "audio, video, 2-pass target-size shrink, GIF",
        ),
        (
            "ffprobe",
            backends.ffprobe.as_deref(),
            "media duration & resolution probing for target-size shrink",
        ),
        (
            "pdftoppm",
            backends.pdftoppm.as_deref(),
            "PDF -> PNG/JPG/WebP",
        ),
        ("pdftotext", backends.pdftotext.as_deref(), "PDF -> TXT"),
        ("gs", backends.gs.as_deref(), "PDF shrinking (Ghostscript)"),
        (
            "pngquant",
            backends.pngquant.as_deref(),
            "optional perceptual PNG quantization",
        ),
        (
            "tar",
            backends.tar.as_deref(),
            "tar / tar.gz / tar.zst / tar.xz / tar.bz2",
        ),
        (
            "bsdtar",
            backends.bsdtar.as_deref(),
            "zip / tar archive extraction & packing",
        ),
        ("7z", backends.seven_z.as_deref(), "7z / zip / rar archives"),
        ("zstd", backends.zstd.as_deref(), "tar.zst compression"),
        (
            "pandoc",
            backends.pandoc.as_deref(),
            "documents (md/html/docx/odt/epub/rtf/txt)",
        ),
        (
            "libreoffice",
            backends.libreoffice.as_deref(),
            "documents -> pdf/docx/odt",
        ),
        (
            "notify-send",
            backends.notify_send.as_deref(),
            "desktop notifications",
        ),
    ];
    for (name, path, desc) in checks {
        match path {
            Some(p) => println!("  [ok]      {:14} {} ({desc})", name, p.display()),
            None => println!("  [missing] {:14} ({desc})", name),
        }
    }
    println!("  [builtin] {:14} json, yaml, toml, csv, tsv", "data");
    let gui_status = if has_gtk4_gui(backends) {
        "GTK4 interactive dialog (python3 + gi)"
    } else if backends.kdialog.is_some() {
        "kdialog fallback"
    } else if backends.zenity.is_some() {
        "zenity fallback"
    } else {
        "missing (install python-gobject + gtk4, kdialog, or zenity)"
    };
    println!("  [gui]     {:14} {gui_status}", "shrink-ui");

    if let Some(home) = home_dir() {
        println!("\nDesktop integrations:");
        let dolphin_dir = dolphin_menu_dir(&home, backends);
        let dolphin_count = count_prefixed(&dolphin_dir, "convto-", ".desktop");
        println!(
            "  dolphin:  {} ({dolphin_count} menu file(s))",
            dolphin_dir.display()
        );
        let nautilus_file = home.join(".local/share/nautilus-python/extensions/convto_nautilus.py");
        println!(
            "  nautilus: {} ({})",
            nautilus_file.display(),
            if nautilus_file.exists() {
                "installed"
            } else {
                "not installed"
            }
        );
        let nemo_dir = home.join(".local/share/nemo/actions");
        let nemo_count = count_prefixed(&nemo_dir, "convto-", ".nemo_action");
        println!(
            "  nemo:     {} ({nemo_count} action file(s))",
            nemo_dir.display()
        );
        let thunar_uca = home.join(".config/Thunar/uca.xml");
        let thunar_has = std::fs::read_to_string(&thunar_uca)
            .map(|s| s.contains("convto-managed:begin"))
            .unwrap_or(false);
        println!(
            "  thunar:   {} ({})",
            thunar_uca.display(),
            if thunar_has {
                "installed"
            } else {
                "not installed"
            }
        );
    }
}

fn cmd_install(
    desktop: DesktopTarget,
    all_menus: bool,
    custom_bin: Option<PathBuf>,
    backends: &Backends,
) -> ExitCode {
    let Some(home) = home_dir() else {
        eprintln!("error: HOME environment variable is not set");
        return ExitCode::from(2);
    };
    let bin_path = match custom_bin {
        Some(p) => p,
        None => match env::current_exe() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("error: failed to locate current executable: {e}");
                return ExitCode::from(2);
            }
        },
    };
    match install_menus(&home, &bin_path, desktop, backends, all_menus) {
        Ok(report) => {
            println!(
                "Installed {} integration file(s) for [{}] using binary {}",
                report.installed_files.len(),
                report.desktops.join(", "),
                bin_path.display()
            );
            for path in &report.installed_files {
                println!("  {}", path.display());
            }
            if !report.skipped_groups.is_empty() {
                println!(
                    "Skipped menu group(s) with no installed backend: {} (pass --all-menus to force)",
                    report.skipped_groups.join(", ")
                );
            }
            if report.refreshed_kde_cache {
                println!(
                    "Refreshed KDE service cache (kbuildsycoca). Right-click menus are active now."
                );
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}

fn cmd_uninstall(desktop: DesktopTarget, backends: &Backends) -> ExitCode {
    let Some(home) = home_dir() else {
        eprintln!("error: HOME environment variable is not set");
        return ExitCode::from(2);
    };
    match uninstall_menus(&home, desktop, backends) {
        Ok(removed) => {
            if removed.is_empty() {
                println!("No convto desktop integrations found.");
            } else {
                println!("Removed {} integration file(s):", removed.len());
                for p in removed {
                    println!("  {}", p.display());
                }
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}

fn cmd_shrink(
    files: &[String],
    backends: &Backends,
    opts: &ShrinkOptions,
    jobs: Option<usize>,
    notify: bool,
    json_output: bool,
) -> ExitCode {
    let start = Instant::now();
    let paths: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
    let worker_count = jobs
        .unwrap_or_else(|| {
            thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
        })
        .clamp(1, paths.len().max(1));

    let results: Vec<Result<ShrinkOutcome, String>> = if worker_count == 1 || paths.len() == 1 {
        paths
            .iter()
            .map(|p| shrink_file(p, backends, opts))
            .collect()
    } else {
        let queue = Arc::new(Mutex::new((
            0usize,
            Vec::<(usize, Result<ShrinkOutcome, String>)>::new(),
        )));
        let paths_arc = Arc::new(paths);
        let backends_arc = Arc::new(backends.clone());
        let opts_arc = Arc::new(opts.clone());
        let mut handles = Vec::with_capacity(worker_count);

        for _ in 0..worker_count {
            let q = Arc::clone(&queue);
            let ps = Arc::clone(&paths_arc);
            let bs = Arc::clone(&backends_arc);
            let os = Arc::clone(&opts_arc);
            handles.push(thread::spawn(move || loop {
                let next_idx = {
                    let mut guard = q.lock().unwrap();
                    if guard.0 >= ps.len() {
                        break;
                    }
                    let idx = guard.0;
                    guard.0 += 1;
                    idx
                };
                let res = shrink_file(&ps[next_idx], &bs, &os);
                q.lock().unwrap().1.push((next_idx, res));
            }));
        }
        for h in handles {
            let _ = h.join();
        }
        let mut collected = std::mem::take(&mut queue.lock().unwrap().1);
        collected.sort_by_key(|(idx, _)| *idx);
        collected.into_iter().map(|(_, r)| r).collect()
    };

    let mut shrunk_count = 0usize;
    let mut optimal_count = 0usize;
    let mut total_before = 0u64;
    let mut total_after = 0u64;
    let mut errors = Vec::new();
    let mut last_single = None;

    for res in results {
        match res {
            Ok(ShrinkOutcome::Shrunk {
                source,
                output,
                original_bytes,
                final_bytes,
                detail,
            }) => {
                shrunk_count += 1;
                total_before += original_bytes;
                total_after += final_bytes;
                let saved = savings_percent(original_bytes, final_bytes);
                if json_output {
                    println!(
                        "{{\"status\":\"shrunk\",\"source\":{},\"output\":{},\"original_bytes\":{original_bytes},\"final_bytes\":{final_bytes},\"saved_percent\":{saved:.2},\"detail\":{}}}",
                        json_quote(&source.display().to_string()),
                        json_quote(&output.display().to_string()),
                        json_quote(&detail)
                    );
                } else {
                    println!(
                        "{} -> {} ({} -> {}, -{:.1}%, {detail})",
                        source.display(),
                        output.display(),
                        format_bytes(original_bytes),
                        format_bytes(final_bytes),
                        saved
                    );
                }
                let out_name = output
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("output");
                last_single = Some(format!(
                    "{out_name}: {} -> {} (-{saved:.1}%)",
                    format_bytes(original_bytes),
                    format_bytes(final_bytes)
                ));
            }
            Ok(ShrinkOutcome::AlreadyOptimal {
                source,
                original_bytes,
                reason,
            }) => {
                optimal_count += 1;
                if json_output {
                    println!(
                        "{{\"status\":\"optimal\",\"source\":{},\"original_bytes\":{original_bytes},\"reason\":{}}}",
                        json_quote(&source.display().to_string()),
                        json_quote(&reason)
                    );
                } else {
                    println!("skipped {} ({reason})", source.display());
                }
            }
            Ok(ShrinkOutcome::DryRun {
                source,
                destination,
                original_bytes,
                detail,
            }) => {
                println!(
                    "[dry-run] {} ({}) -> {} ({detail})",
                    source.display(),
                    format_bytes(original_bytes),
                    destination.display()
                );
            }
            Err(e) => {
                if json_output {
                    println!("{{\"status\":\"error\",\"error\":{}}}", json_quote(&e));
                } else {
                    eprintln!("error: {e}");
                }
                errors.push(e);
            }
        }
    }

    if notify && !opts.dry_run {
        let elapsed = start.elapsed().as_secs_f32();
        if errors.is_empty() {
            let body = if shrunk_count == 1 && optimal_count == 0 {
                last_single.unwrap_or_else(|| format!("Shrunk 1 file ({elapsed:.1}s)"))
            } else if shrunk_count == 0 && optimal_count > 0 {
                format!("{optimal_count} file(s) already optimal")
            } else {
                let saved = savings_percent(total_before, total_after);
                format!(
                    "Shrunk {shrunk_count} file(s): {} -> {} (-{saved:.1}%, {elapsed:.1}s)",
                    format_bytes(total_before),
                    format_bytes(total_after)
                )
            };
            send_notification(backends, "convto shrink", &body, false);
        } else {
            let body = format!("Shrink failed: {}", errors[0]);
            send_notification(backends, "convto error", &body, true);
        }
    }

    if errors.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn cmd_convert(
    target_str: &str,
    files: &[String],
    backends: &Backends,
    opts: &ConvertOptions,
    jobs: Option<usize>,
    notify: bool,
) -> ExitCode {
    let Some(target) = Format::parse(target_str) else {
        let msg = format!("Unknown target format '{target_str}' (run 'convto formats')");
        eprintln!("error: {msg}");
        if notify {
            send_notification(backends, "convto error", &msg, true);
        }
        return ExitCode::from(2);
    };

    let start = Instant::now();
    let paths: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
    let worker_count = jobs
        .unwrap_or_else(|| {
            thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
        })
        .clamp(1, paths.len().max(1));

    let results: Vec<Result<Outcome, String>> = if worker_count == 1 || paths.len() == 1 {
        paths
            .iter()
            .map(|p| convert_file(p, target, backends, opts))
            .collect()
    } else {
        let queue = Arc::new(Mutex::new((
            0usize,
            Vec::<(usize, Result<Outcome, String>)>::new(),
        )));
        let paths_arc = Arc::new(paths);
        let backends_arc = Arc::new(backends.clone());
        let opts_arc = Arc::new(opts.clone());
        let mut handles = Vec::with_capacity(worker_count);

        for _ in 0..worker_count {
            let q = Arc::clone(&queue);
            let ps = Arc::clone(&paths_arc);
            let bs = Arc::clone(&backends_arc);
            let os = Arc::clone(&opts_arc);
            handles.push(thread::spawn(move || loop {
                let next_idx = {
                    let mut guard = q.lock().unwrap();
                    if guard.0 >= ps.len() {
                        break;
                    }
                    let idx = guard.0;
                    guard.0 += 1;
                    idx
                };
                let res = convert_file(&ps[next_idx], target, &bs, &os);
                q.lock().unwrap().1.push((next_idx, res));
            }));
        }
        for h in handles {
            let _ = h.join();
        }
        let mut collected = std::mem::take(&mut queue.lock().unwrap().1);
        collected.sort_by_key(|(idx, _)| *idx);
        collected.into_iter().map(|(_, r)| r).collect()
    };

    let mut converted_count = 0usize;
    let mut skipped_count = 0usize;
    let mut errors: Vec<String> = Vec::new();
    let mut last_single_summary: Option<String> = None;

    for res in results {
        match res {
            Ok(Outcome::Converted {
                source,
                outputs,
                backend,
                ..
            }) => {
                converted_count += 1;
                if outputs.len() == 1 {
                    let out_name = outputs[0]
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("output");
                    let src_name = source
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("input");
                    println!(
                        "{} -> {} ({backend})",
                        source.display(),
                        outputs[0].display()
                    );
                    last_single_summary = Some(format!("{src_name} -> {out_name}"));
                } else {
                    println!(
                        "{} -> {} files ({backend})",
                        source.display(),
                        outputs.len()
                    );
                    last_single_summary = Some(format!("{} files created", outputs.len()));
                }
            }
            Ok(Outcome::DryRun {
                source,
                source_format,
                destination,
                backend,
            }) => {
                println!(
                    "[dry-run] {} ({source_format}) -> {} ({backend})",
                    source.display(),
                    destination.display()
                );
            }
            Ok(Outcome::Skipped { source, reason }) => {
                skipped_count += 1;
                println!("skipped {} ({reason})", source.display());
            }
            Err(e) => {
                eprintln!("error: {e}");
                errors.push(e);
            }
        }
    }

    if notify && !opts.dry_run {
        let elapsed = start.elapsed().as_secs_f32();
        if errors.is_empty() {
            let body = if converted_count == 1 && skipped_count == 0 {
                if let Some(s) = last_single_summary {
                    format!("{s} ({elapsed:.1}s)")
                } else {
                    format!("Converted 1 file to {target} ({elapsed:.1}s)")
                }
            } else if converted_count == 0 && skipped_count > 0 {
                format!("Skipped {skipped_count} file(s) (already {target})")
            } else if skipped_count > 0 {
                format!(
                    "Converted {converted_count} file(s) to {target}, skipped {skipped_count} ({elapsed:.1}s)"
                )
            } else {
                format!("Converted {converted_count} file(s) to {target} ({elapsed:.1}s)")
            };
            send_notification(backends, "convto", &body, false);
        } else {
            let first_err = &errors[0];
            let body = if converted_count > 0 {
                format!(
                    "Converted {converted_count}, {} failed: {first_err}",
                    errors.len()
                )
            } else {
                first_err.clone()
            };
            send_notification(backends, "convto error", &body, true);
        }
    }

    if errors.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn json_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn send_notification(backends: &Backends, title: &str, body: &str, critical: bool) {
    let Some(notify) = backends.notify_send.as_deref() else {
        return;
    };
    let mut cmd = Command::new(notify);
    cmd.args(["-a", "convto"]);
    if critical {
        cmd.args(["-u", "critical", "-i", "dialog-error"]);
    } else {
        cmd.args(["-i", "document-export"]);
    }
    cmd.arg(title).arg(body);
    let _ = cmd.status();
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

fn count_prefixed(dir: &Path, prefix: &str, suffix: &str) -> usize {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| {
                    e.file_name()
                        .to_str()
                        .is_some_and(|s| s.starts_with(prefix) && s.ends_with(suffix))
                })
                .count()
        })
        .unwrap_or(0)
}
