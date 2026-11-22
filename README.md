# convto

Zero-dependency Rust CLI and Linux file-manager context menu ("Convert to") for converting images, GIFs, audio, video, PDFs, structured data, archives, and documents from a right-click or terminal.

![demo](assets/demo.png)

## What it does

Right-click one or more files in your Linux file manager, hover **Convert to**, and pick a target format. `convto` writes converted outputs atomically beside the source file without overwriting existing files (`photo.png`, then `photo (1).png`), converts multi-file selections in parallel, and sends a desktop notification via `notify-send`.

Key details:
- Single ~600 KB stripped binary (`#![forbid(unsafe_code)]`, zero external crates).
- Magic-byte sniffing catches misnamed files (for example, WebP bytes saved with a `.jpg` extension still convert properly instead of skipping as already JPEG).
- Disjoint MIME menus so Dolphin shows one **Convert to** submenu per file type, only installing groups whose backends exist on your machine.
- Built-in structured data converter (`json`, `yaml`, `toml`, `csv`, `tsv`) with no external dependencies.
- Archive repacking validates member paths against directory traversal (`..` and absolute paths) before extraction.
- Image conversion flattens transparency onto white for JPEG/BMP, scales down cleanly for multi-size `.ico`, and uses `rsvg-convert` for SVG when available. Video to GIF uses two-pass `palettegen`/`paletteuse`.

## Supported formats

| Family | Inputs | Targets | Backend |
| --- | --- | --- | --- |
| Images | `png`, `jpg`, `webp`, `avif`, `heic`, `jxl`, `bmp`, `tiff`, `ico`, `svg`, `tga`, `psd`, `qoi`, `ppm` | `png`, `jpg`, `webp`, `avif`, `jxl`, `gif`, `ico`, `tiff`, `bmp`, `pdf` | `magick` / `convert`, `rsvg-convert` |
| GIF | `gif` | `mp4`, `webm`, `webp`, `png`, `jpg`, `avif` | `ffmpeg`, `magick` |
| Audio | `mp3`, `wav`, `flac`, `ogg`, `opus`, `m4a`, `aac`, `wma`, `aiff`, `mka` | `mp3`, `wav`, `flac`, `ogg`, `opus`, `m4a` | `ffmpeg` |
| Video | `mp4`, `mkv`, `webm`, `mov`, `avi`, `flv`, `wmv`, `mpg`, `3gp` | `mp4`, `mkv`, `webm`, `mov`, `gif`, `mp3`, `wav`, `opus` | `ffmpeg` |
| PDF | `pdf` | `png`, `jpg`, `webp`, `txt` | `pdftoppm`, `pdftotext` |
| Data | `json`, `yaml`, `toml`, `csv`, `tsv` | `json`, `yaml`, `toml`, `csv`, `tsv` | Built-in |
| Archives | `zip`, `tar`, `tar.gz`, `tar.zst`, `tar.xz`, `tar.bz2`, `7z`, `rar` | `zip`, `tar.gz`, `tar.zst`, `tar.xz`, `tar.bz2`, `tar`, `7z` | `tar`, `bsdtar`, `7z`, `zstd` |
| Documents | `md`, `html`, `docx`, `odt`, `epub`, `rtf` | `pdf`, `html`, `md`, `docx`, `odt`, `txt` | `pandoc`, `libreoffice` |

## Install

Build and install the binary plus your desktop right-click menus:

```bash
cargo build --release
install -Dm755 target/release/convto ~/.local/bin/convto
convto install
```

Supported file managers (auto-detected by `convto install`, or pass `--desktop dolphin|nautilus|nemo|thunar|all`):

- KDE Dolphin (Plasma 6 and 5): `~/.local/share/kio/servicemenus/convto-*.desktop` (auto-runs `kbuildsycoca6`).
- GNOME Files (Nautilus): `~/.local/share/nautilus-python/extensions/convto_nautilus.py` (requires `python-nautilus` / `nautilus-python`; restart Nautilus with `nautilus -q`).
- Cinnamon Nemo: `~/.local/share/nemo/actions/convto-*.nemo_action`.
- XFCE Thunar: managed `<!-- convto-managed -->` entries in `~/.config/Thunar/uca.xml`.

Remove all generated menus cleanly at any time without affecting your other custom actions:

```bash
convto uninstall
```

## CLI usage

```bash
# Convert images (single or batch)
convto png photo.webp
convto webp *.png -q 85

# Convert animated GIFs, video, or extract audio
convto mp4 reaction.gif
convto gif clip.mp4
convto mp3 lecture.mkv

# Render PDF pages or extract text
convto png slides.pdf
convto txt paper.pdf

# Convert config and table formats (built-in, no external tools)
convto yaml config.json
convto toml config.yaml
convto tsv export.csv

# Repack archives safely
convto tar.zst backup.zip

# Inspect detected format, valid targets, and installed backends
convto targets photo.webp
convto doctor
```

Useful flags: `--out-dir <dir>`, `--overwrite`, `--force` (re-encode same format), `--delete-source`, `--quality <1-100>`, `--jobs <n>`, `--dry-run`, `--notify`.

## Limitations

- Media, PDF, archive, and document conversions shell out to system tools (`magick`, `ffmpeg`, `pdftoppm`, `tar`/`7z`, `pandoc`/`libreoffice`). Run `convto doctor` to see what is installed on your machine.
- In KDE Dolphin, KIO service menus filter by MIME family rather than hiding the current file's own format inside the submenu (so `PNG (.png)` still appears when right-clicking a `.png`, and `convto` skips it cleanly unless `--force` is passed). Nautilus's Python extension hides the active format dynamically.
- The built-in YAML parser covers standard block/flow mappings, sequences, and scalars; it does not implement anchors/aliases (`&a`/`*a`), multi-document streams, or custom tags.
- Focused on Linux desktops; the CLI works on macOS too, while file-manager menu installation targets Linux file managers.

## License

MIT. See [LICENSE](LICENSE).
