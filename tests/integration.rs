use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use convto::convert::{convert_file, ConvertOptions, Outcome};
use convto::desktop::{install_menus, uninstall_menus, DesktopTarget};
use convto::detect::Backends;
use convto::format::{detect_format, Format};
use convto::shrink::{shrink_file, ShrinkGoal, ShrinkOptions, ShrinkOutcome};

static TEST_DIR_SEQ: AtomicU64 = AtomicU64::new(0);

struct TempWorkDir {
    path: PathBuf,
}

impl TempWorkDir {
    fn new(prefix: &str) -> Self {
        let id = TEST_DIR_SEQ.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let path = std::env::temp_dir().join(format!("convto-test-{prefix}-{pid}-{id}"));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempWorkDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn data_conversions_roundtrip_end_to_end() {
    let dir = TempWorkDir::new("data");
    let backends = Backends::probe();
    let opts = ConvertOptions::default();

    let json_path = dir.path().join("config.json");
    fs::write(
        &json_path,
        r#"{
  "service": "convto",
  "retries": 3,
  "ratio": 0.75,
  "active": true,
  "tags": ["linux", "kde", "cli"],
  "limits": {
    "workers": 8,
    "label": "a: b # not comment"
  }
}
"#,
    )
    .unwrap();

    let Outcome::Converted {
        outputs: yaml_outs, ..
    } = convert_file(&json_path, Format::Yaml, &backends, &opts).unwrap()
    else {
        panic!("expected YAML conversion");
    };
    let yaml_path = &yaml_outs[0];
    assert!(yaml_path.ends_with("config.yaml"));

    let Outcome::Converted {
        outputs: toml_outs, ..
    } = convert_file(yaml_path, Format::Toml, &backends, &opts).unwrap()
    else {
        panic!("expected TOML conversion");
    };
    let toml_path = &toml_outs[0];
    assert!(toml_path.ends_with("config.toml"));

    // Converting back to JSON should avoid clobbering config.json -> config (1).json.
    let Outcome::Converted {
        outputs: back_outs, ..
    } = convert_file(toml_path, Format::Json, &backends, &opts).unwrap()
    else {
        panic!("expected JSON conversion");
    };
    let back_path = &back_outs[0];
    assert_eq!(back_path.file_name().unwrap(), "config (1).json");

    let orig = convto::data::parse_json(&fs::read_to_string(&json_path).unwrap()).unwrap();
    let round = convto::data::parse_json(&fs::read_to_string(back_path).unwrap()).unwrap();
    assert_eq!(orig, round);

    // CSV <-> TSV <-> JSON round-trip with quoted commas and leading-zero preservation.
    let csv_path = dir.path().join("table.csv");
    fs::write(
        &csv_path,
        "id,name,notes\n001,Alice,\"one, two\"\n002,Bob,\"quoted \"\"text\"\"\"\n",
    )
    .unwrap();
    let Outcome::Converted {
        outputs: tsv_outs, ..
    } = convert_file(&csv_path, Format::Tsv, &backends, &opts).unwrap()
    else {
        panic!("expected TSV conversion");
    };
    let Outcome::Converted {
        outputs: csv_back, ..
    } = convert_file(&tsv_outs[0], Format::Csv, &backends, &opts).unwrap()
    else {
        panic!("expected CSV roundtrip");
    };
    assert_eq!(csv_back[0].file_name().unwrap(), "table (1).csv");
    let orig_csv =
        convto::data::parse_delimited(&fs::read_to_string(&csv_path).unwrap(), ',').unwrap();
    let round_csv =
        convto::data::parse_delimited(&fs::read_to_string(&csv_back[0]).unwrap(), ',').unwrap();
    assert_eq!(orig_csv, round_csv);
}

#[test]
fn live_image_and_misnamed_magic_conversions() {
    let backends = Backends::probe();
    let Some(magick) = backends.image_tool() else {
        return;
    };
    let dir = TempWorkDir::new("img");
    let opts = ConvertOptions::default();

    // Generate a 64x64 RGBA PNG with spaces and brackets in the filename.
    let src_png = dir.path().join("sample shot [1].png");
    let status = Command::new(magick)
        .args(["-size", "64x64", "xc:rgba(30,144,255,0.5)"])
        .arg(&src_png)
        .status()
        .unwrap();
    assert!(status.success());

    // PNG -> WebP -> JPG -> ICO -> BMP -> TIFF -> PDF.
    let Outcome::Converted {
        outputs: webp_outs, ..
    } = convert_file(&src_png, Format::Webp, &backends, &opts).unwrap()
    else {
        panic!("expected WebP");
    };
    assert_eq!(detect_format(&webp_outs[0]), Some(Format::Webp));

    // Copy the WebP bytes under a deceptive `.jpg` extension: magic sniffing should still
    // recognize WebP and convert it cleanly to a real JPEG!
    let misnamed = dir.path().join("tricky.jpg");
    fs::copy(&webp_outs[0], &misnamed).unwrap();
    assert_eq!(detect_format(&misnamed), Some(Format::Webp));
    let Outcome::Converted {
        outputs: fixed_jpg,
        source_format,
        ..
    } = convert_file(&misnamed, Format::Jpg, &backends, &opts).unwrap()
    else {
        panic!("expected misnamed WebP->JPG conversion");
    };
    assert_eq!(source_format, Format::Webp);
    assert_eq!(fixed_jpg[0].file_name().unwrap(), "tricky (1).jpg");
    assert_eq!(detect_format(&fixed_jpg[0]), Some(Format::Jpg));

    // Same-format skip check on the real JPEG.
    let skipped = convert_file(&fixed_jpg[0], Format::Jpg, &backends, &opts).unwrap();
    assert!(matches!(skipped, Outcome::Skipped { .. }));

    // Transparent PNG -> ICO, BMP, PDF, GIF.
    for target in [Format::Ico, Format::Bmp, Format::Gif, Format::Pdf] {
        let Outcome::Converted { outputs, .. } =
            convert_file(&src_png, target, &backends, &opts).unwrap()
        else {
            panic!("expected {target}");
        };
        assert!(outputs[0].exists());
        assert!(fs::metadata(&outputs[0]).unwrap().len() > 0);
    }

    // SVG -> PNG.
    let svg_path = dir.path().join("icon.svg");
    fs::write(
        &svg_path,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"><circle cx="32" cy="32" r="24" fill="#d94e28"/></svg>"##,
    )
    .unwrap();
    let Outcome::Converted {
        outputs: svg_png, ..
    } = convert_file(&svg_path, Format::Png, &backends, &opts).unwrap()
    else {
        panic!("expected SVG->PNG");
    };
    assert_eq!(detect_format(&svg_png[0]), Some(Format::Png));
}

#[test]
fn live_audio_video_and_gif_conversions() {
    let backends = Backends::probe();
    let Some(ffmpeg) = backends.ffmpeg.as_deref() else {
        return;
    };
    let dir = TempWorkDir::new("media");
    let opts = ConvertOptions::default();

    // Synthesize a 0.3s 440Hz WAV tone.
    let wav = dir.path().join("tone.wav");
    let st = Command::new(ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=0.3",
        ])
        .arg(&wav)
        .status()
        .unwrap();
    assert!(st.success());

    for target in [Format::Mp3, Format::Flac, Format::Opus] {
        let Outcome::Converted { outputs, .. } =
            convert_file(&wav, target, &backends, &opts).unwrap()
        else {
            panic!("expected WAV -> {target}");
        };
        assert!(fs::metadata(&outputs[0]).unwrap().len() > 100);
    }

    // Synthesize a tiny 0.4s MP4 clip with audio, convert to GIF and extract MP3.
    let mp4 = dir.path().join("clip.mp4");
    let st = Command::new(ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=64x64:rate=10:duration=0.4",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=0.4",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
        ])
        .arg(&mp4)
        .status()
        .unwrap();
    assert!(st.success());

    let Outcome::Converted {
        outputs: gif_outs, ..
    } = convert_file(&mp4, Format::Gif, &backends, &opts).unwrap()
    else {
        panic!("expected MP4 -> GIF");
    };
    assert_eq!(detect_format(&gif_outs[0]), Some(Format::Gif));

    let Outcome::Converted {
        outputs: audio_outs,
        ..
    } = convert_file(&mp4, Format::Mp3, &backends, &opts).unwrap()
    else {
        panic!("expected MP4 -> MP3");
    };
    assert!(fs::metadata(&audio_outs[0]).unwrap().len() > 100);

    // GIF -> MP4.
    let Outcome::Converted {
        outputs: back_mp4, ..
    } = convert_file(&gif_outs[0], Format::Mp4, &backends, &opts).unwrap()
    else {
        panic!("expected GIF -> MP4");
    };
    assert_eq!(back_mp4[0].file_name().unwrap(), "clip (1).mp4");
    assert!(fs::metadata(&back_mp4[0]).unwrap().len() > 100);
}

#[test]
fn live_pdf_and_archive_conversions() {
    let backends = Backends::probe();
    let dir = TempWorkDir::new("pdf_arc");
    let opts = ConvertOptions::default();

    // PDF -> PNG via ImageMagick + pdftoppm.
    if let (Some(magick), Some(_)) = (backends.image_tool(), backends.pdftoppm.as_deref()) {
        let pdf = dir.path().join("doc.pdf");
        let st = Command::new(magick)
            .args(["-size", "100x100", "xc:white"])
            .arg(&pdf)
            .status()
            .unwrap();
        assert!(st.success());
        let Outcome::Converted { outputs, .. } =
            convert_file(&pdf, Format::Png, &backends, &opts).unwrap()
        else {
            panic!("expected PDF -> PNG");
        };
        assert_eq!(detect_format(&outputs[0]), Some(Format::Png));
    }

    // Archive: tar.gz -> zip -> tar.zst -> 7z.
    if let Some(tar) = backends.tar.as_deref() {
        let payload = dir.path().join("payload");
        fs::create_dir_all(payload.join("sub")).unwrap();
        fs::write(payload.join("hello.txt"), "hello convto\n").unwrap();
        fs::write(payload.join("sub/data.txt"), "nested\n").unwrap();

        let tgz = dir.path().join("bundle.tar.gz");
        let st = Command::new(tar)
            .current_dir(&payload)
            .args(["-czf"])
            .arg(&tgz)
            .args(["hello.txt", "sub"])
            .status()
            .unwrap();
        assert!(st.success());

        let Outcome::Converted {
            outputs: zip_outs, ..
        } = convert_file(&tgz, Format::Zip, &backends, &opts).unwrap()
        else {
            panic!("expected tar.gz -> zip");
        };
        assert!(zip_outs[0].ends_with("bundle.zip"));
        assert!(fs::metadata(&zip_outs[0]).unwrap().len() > 20);

        if backends.zstd.is_some() {
            let Outcome::Converted {
                outputs: zst_outs, ..
            } = convert_file(&zip_outs[0], Format::TarZst, &backends, &opts).unwrap()
            else {
                panic!("expected zip -> tar.zst");
            };
            assert!(zst_outs[0].ends_with("bundle.tar.zst"));
            assert_eq!(detect_format(&zst_outs[0]), Some(Format::TarZst));
        }
    }
}

#[test]
fn desktop_install_and_uninstall_preserve_existing_user_files() {
    let dir = TempWorkDir::new("desktop");
    let home = dir.path();
    let backends = Backends::probe();
    let fake_bin = Path::new("/usr/local/bin/convto");

    // Pre-seed an unrelated Dolphin menu and an unrelated Thunar custom action.
    let kio_dir = convto::desktop::dolphin_menu_dir(home, &backends);
    fs::create_dir_all(&kio_dir).unwrap();
    let other_desktop = kio_dir.join("other-app.desktop");
    fs::write(&other_desktop, "[Desktop Entry]\nName=Other\n").unwrap();

    let thunar_dir = home.join(".config/Thunar");
    fs::create_dir_all(&thunar_dir).unwrap();
    let uca = thunar_dir.join("uca.xml");
    fs::write(
        &uca,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<actions>\n<action><name>User Action</name></action>\n</actions>\n",
    )
    .unwrap();

    let report = install_menus(home, fake_bin, DesktopTarget::All, &backends, true).unwrap();
    assert!(!report.installed_files.is_empty());
    assert!(kio_dir.join("convto-image.desktop").exists());
    assert!(home
        .join(".local/share/nautilus-python/extensions/convto_nautilus.py")
        .exists());
    assert!(home
        .join(".local/share/nemo/actions/convto-image-png.nemo_action")
        .exists());
    let uca_after = fs::read_to_string(&uca).unwrap();
    assert!(uca_after.contains("User Action"));
    assert!(uca_after.contains("convto-managed:begin"));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(kio_dir.join("convto-image.desktop"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111, "Dolphin .desktop must be executable");
    }

    let removed = uninstall_menus(home, DesktopTarget::All, &backends).unwrap();
    assert!(!removed.is_empty());
    assert!(!kio_dir.join("convto-image.desktop").exists());
    assert!(other_desktop.exists(), "unrelated Dolphin menu must remain");
    assert!(!home
        .join(".local/share/nautilus-python/extensions/convto_nautilus.py")
        .exists());
    let uca_final = fs::read_to_string(&uca).unwrap();
    assert!(
        uca_final.contains("User Action"),
        "unrelated Thunar action must remain"
    );
    assert!(!uca_final.contains("convto-managed:begin"));
}

#[test]
fn cli_parallel_batch_and_doctor() {
    let dir = TempWorkDir::new("cli");
    let bin = env!("CARGO_BIN_EXE_convto");

    for name in ["a.json", "b.json", "c.json"] {
        fs::write(dir.path().join(name), r#"{"ok":true,"n":42}"#).unwrap();
    }

    let out = Command::new(bin)
        .args(["yaml"])
        .arg(dir.path().join("a.json"))
        .arg(dir.path().join("b.json"))
        .arg(dir.path().join("c.json"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    for name in ["a.yaml", "b.yaml", "c.yaml"] {
        assert!(dir.path().join(name).exists());
    }

    let doc = Command::new(bin).arg("doctor").output().unwrap();
    assert!(doc.status.success());
    let stdout = String::from_utf8_lossy(&doc.stdout);
    assert!(stdout.contains("[builtin]"));
    assert!(stdout.contains("shrink-ui"));
}

#[test]
fn live_shrink_images_video_audio_and_pdf() {
    let backends = Backends::probe();
    let dir = TempWorkDir::new("shrink");

    // 1. High-entropy noisy JPEG (~200+ KB) -> binary search target <= 35 KB!
    if let Some(magick) = backends.image_tool() {
        let noisy_jpg = dir.path().join("photo.jpg");
        let st = Command::new(magick)
            .args([
                "-size", "640x480", "xc:gray", "+noise", "Random", "-quality", "98",
            ])
            .arg(&noisy_jpg)
            .status()
            .unwrap();
        assert!(st.success());
        let orig_sz = fs::metadata(&noisy_jpg).unwrap().len();
        assert!(
            orig_sz > 80_000,
            "expected noisy JPEG > 80KB, got {orig_sz}"
        );

        let target = 35_000u64;
        let opts = ShrinkOptions {
            goal: ShrinkGoal::TargetBytes(target),
            ..Default::default()
        };
        let ShrinkOutcome::Shrunk {
            output,
            original_bytes,
            final_bytes,
            ..
        } = shrink_file(&noisy_jpg, &backends, &opts).unwrap()
        else {
            panic!("expected noisy JPEG to shrink");
        };
        assert_eq!(original_bytes, orig_sz);
        assert!(
            final_bytes <= target,
            "expected final_bytes ({final_bytes}) <= {target}"
        );
        assert_eq!(output.file_name().unwrap(), "photo-shrunk.jpg");
        assert_eq!(detect_format(&output), Some(Format::Jpg));

        // 2. Percentage mode + convert to WebP via CLI with --json output.
        let bin = env!("CARGO_BIN_EXE_convto");
        let cli = Command::new(bin)
            .args(["shrink", "--percent", "40", "--to", "webp", "--json"])
            .arg(&noisy_jpg)
            .output()
            .unwrap();
        assert!(
            cli.status.success(),
            "stderr: {}",
            String::from_utf8_lossy(&cli.stderr)
        );
        let out_json = String::from_utf8_lossy(&cli.stdout);
        assert!(out_json.contains("\"status\":\"shrunk\""), "{out_json}");
        let webp_out = dir.path().join("photo.webp");
        assert!(webp_out.exists());
        assert!(fs::metadata(&webp_out).unwrap().len() <= (orig_sz * 40 / 100) + 1024);

        // 3. PNG target-size shrink.
        let noisy_png = dir.path().join("shot.png");
        let st = Command::new(magick)
            .args(["-size", "320x240", "plasma:fractal"])
            .arg(&noisy_png)
            .status()
            .unwrap();
        assert!(st.success());
        let png_orig = fs::metadata(&noisy_png).unwrap().len();
        let png_opts = ShrinkOptions {
            goal: ShrinkGoal::SizePercent(45.0),
            ..Default::default()
        };
        let res = shrink_file(&noisy_png, &backends, &png_opts).unwrap();
        match res {
            ShrinkOutcome::Shrunk { final_bytes, .. } => {
                assert!(final_bytes < png_orig, "{final_bytes} vs {png_orig}");
            }
            ShrinkOutcome::AlreadyOptimal { .. } => panic!("expected plasma PNG to shrink"),
            ShrinkOutcome::DryRun { .. } => unreachable!(),
        }
    }

    // 4. Video 2-pass exact target-size shrink + audio target-size shrink.
    if let (Some(ffmpeg), Some(_)) = (backends.ffmpeg.as_deref(), backends.ffprobe.as_deref()) {
        let clip = dir.path().join("sample.mp4");
        let st = Command::new(ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=640x360:rate=24:duration=2.0",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=2.0",
                "-c:v",
                "libx264",
                "-crf",
                "14",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-b:a",
                "192k",
            ])
            .arg(&clip)
            .status()
            .unwrap();
        assert!(st.success());
        let orig_vid = fs::metadata(&clip).unwrap().len();
        let target_vid = (orig_vid / 2).max(40_000);
        let opts = ShrinkOptions {
            goal: ShrinkGoal::TargetBytes(target_vid),
            two_pass: true,
            ..Default::default()
        };
        let ShrinkOutcome::Shrunk {
            output,
            final_bytes,
            detail,
            ..
        } = shrink_file(&clip, &backends, &opts).unwrap()
        else {
            panic!("expected video to shrink");
        };
        assert!(detail.contains("2-pass"), "{detail}");
        assert!(final_bytes < orig_vid, "{final_bytes} vs {orig_vid}");
        assert!(output.exists());

        // Audio shrink (WAV -> MP3 target size).
        let wav = dir.path().join("voice.wav");
        let st = Command::new(ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=2.0",
            ])
            .arg(&wav)
            .status()
            .unwrap();
        assert!(st.success());
        let audio_opts = ShrinkOptions {
            goal: ShrinkGoal::TargetBytes(25_000),
            target_format: Some(Format::Mp3),
            ..Default::default()
        };
        let ShrinkOutcome::Shrunk {
            final_bytes: mp3_bytes,
            ..
        } = shrink_file(&wav, &backends, &audio_opts).unwrap()
        else {
            panic!("expected WAV -> MP3 shrink");
        };
        assert!(mp3_bytes <= 30_000, "mp3_bytes={mp3_bytes}");
    }

    // 5. PDF Ghostscript shrink.
    if let (Some(magick), Some(_)) = (backends.image_tool(), backends.gs.as_deref()) {
        let big_pdf = dir.path().join("brochure.pdf");
        let st = Command::new(magick)
            .args(["-size", "600x600", "plasma:fractal", "-compress", "none"])
            .arg(&big_pdf)
            .status()
            .unwrap();
        assert!(st.success());
        let orig_pdf = fs::metadata(&big_pdf).unwrap().len();
        let pdf_opts = ShrinkOptions {
            goal: ShrinkGoal::SizePercent(35.0),
            ..Default::default()
        };
        let ShrinkOutcome::Shrunk { final_bytes, .. } =
            shrink_file(&big_pdf, &backends, &pdf_opts).unwrap()
        else {
            panic!("expected uncompressed PDF to shrink");
        };
        assert!(final_bytes < orig_pdf, "{final_bytes} vs {orig_pdf}");
    }
}
