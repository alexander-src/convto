import json
import os
import subprocess
import sys
import threading

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Gdk", "4.0")
from gi.repository import Gdk, Gio, GLib, Gtk

HAS_ADW = False
try:
    gi.require_version("Adw", "1")
    from gi.repository import Adw

    HAS_ADW = True
except Exception:
    Adw = None


def fmt_bytes(n):
    if n >= 1_000_000_000:
        return f"{n / 1_000_000_000:.2f} GB"
    if n >= 1_000_000:
        return f"{n / 1_000_000:.2f} MB"
    if n >= 1_000:
        return f"{n / 1_000:.1f} KB"
    return f"{n} B"


def detect_family(paths):
    img = {"png", "jpg", "jpeg", "webp", "avif", "heic", "heif", "jxl", "bmp", "tif", "tiff", "svg", "tga", "psd", "qoi"}
    vid = {"mp4", "mkv", "webm", "mov", "avi", "flv", "wmv", "m4v", "mpg", "mpeg", "3gp", "gif"}
    aud = {"mp3", "wav", "flac", "ogg", "opus", "m4a", "aac", "wma", "aiff", "mka"}
    fams = set()
    for p in paths:
        ext = os.path.splitext(p)[1].lstrip(".").lower()
        if ext in img:
            fams.add("image")
        elif ext in vid:
            fams.add("video")
        elif ext in aud:
            fams.add("audio")
        elif ext == "pdf":
            fams.add("pdf")
        else:
            fams.add("other")
    if len(fams) == 1:
        return next(iter(fams))
    return "mixed"


def format_choices(family):
    if family == "image":
        return [
            ("Keep original format", None, False),
            ("WebP (.webp)", "webp", False),
            ("AVIF (.avif)", "avif", False),
            ("JPEG (.jpg)", "jpg", False),
            ("JPEG XL (.jxl)", "jxl", False),
            ("PNG (.png)", "png", False),
        ]
    if family == "video":
        return [
            ("Keep original format", None, False),
            ("MP4 H.264 (.mp4)", "mp4", False),
            ("MP4 H.265 / HEVC (.mp4)", "mp4", True),
            ("WebM VP9 (.webm)", "webm", False),
            ("MKV (.mkv)", "mkv", False),
        ]
    if family == "audio":
        return [
            ("Keep original format", None, False),
            ("Opus (.opus)", "opus", False),
            ("MP3 (.mp3)", "mp3", False),
            ("M4A / AAC (.m4a)", "m4a", False),
            ("OGG Vorbis (.ogg)", "ogg", False),
            ("FLAC lossless (.flac)", "flac", False),
        ]
    return [("Keep original format", None, False)]


RES_CHOICES = [
    ("Keep original resolution", None),
    ("4K (max 2160px)", 2160),
    ("1080p (max 1920px)", 1920),
    ("720p (max 1280px)", 1280),
    ("480p (max 854px)", 854),
]

PRESET_CHOICES = [
    ("Balanced (medium)", "medium"),
    ("Higher quality (slow)", "slow"),
    ("Faster (fast)", "fast"),
]


class ShrinkWindow(Gtk.ApplicationWindow):
    def __init__(self, app, convto_bin, files, notify):
        super().__init__(application=app, title="convto - Shrink & Optimize")
        self.convto_bin = convto_bin
        self.files = files
        self.notify = notify
        self.family = detect_family(files)
        self.total_bytes = sum(os.path.getsize(p) for p in files if os.path.isfile(p))
        self.first_bytes = os.path.getsize(files[0]) if files and os.path.isfile(files[0]) else 0
        self.mode = "size"  # "size" | "percent" | "quality"
        self.running = False
        self.finished = False
        self.fmt_options = format_choices(self.family)

        self.set_default_size(490, -1)
        self.set_resizable(False)

        css = Gtk.CssProvider()
        css.load_from_data(
            b"""
            .title-header { font-size: 16px; font-weight: 700; }
            .subtitle-meta { opacity: 0.72; font-size: 12.5px; }
            .summary-pill { padding: 8px 12px; border-radius: 8px; background: alpha(currentColor, 0.06); font-size: 12.5px; }
            .result-ok { color: #26a269; font-weight: 600; }
            .result-err { color: #e01b24; font-weight: 600; }
            .preset-btn { padding: 4px 10px; font-size: 12px; }
            """
        )
        Gtk.StyleContext.add_provider_for_display(
            Gdk.Display.get_default(), css, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION
        )

        # Escape closes window when not running.
        key_ctrl = Gtk.EventControllerKey()
        key_ctrl.connect("key-pressed", self._on_key)
        self.add_controller(key_ctrl)

        root = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=14)
        root.set_margin_top(18)
        root.set_margin_bottom(18)
        root.set_margin_start(20)
        root.set_margin_end(20)
        self.set_child(root)

        # Header
        head = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=3)
        if len(files) == 1:
            name = os.path.basename(files[0])
            meta = f"{self.family.upper()}  |  Original size: {fmt_bytes(self.total_bytes)}"
        else:
            name = f"{len(files)} files selected"
            meta = f"{self.family.upper()}  |  Total size: {fmt_bytes(self.total_bytes)}"
        title_lbl = Gtk.Label(label=name, xalign=0)
        title_lbl.add_css_class("title-header")
        title_lbl.set_ellipsize(3)
        meta_lbl = Gtk.Label(label=meta, xalign=0)
        meta_lbl.add_css_class("subtitle-meta")
        head.append(title_lbl)
        head.append(meta_lbl)
        root.append(head)

        # Mode switcher
        mode_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=0)
        mode_box.add_css_class("linked")
        mode_box.set_homogeneous(True)
        self.btn_mode_size = Gtk.ToggleButton(label="Target Size")
        self.btn_mode_pct = Gtk.ToggleButton(label="Size % Wheel", group=self.btn_mode_size)
        self.btn_mode_qual = Gtk.ToggleButton(label="Quality Wheel", group=self.btn_mode_size)
        self.btn_mode_size.set_active(True)
        self.btn_mode_size.connect("toggled", self._on_mode_changed)
        self.btn_mode_pct.connect("toggled", self._on_mode_changed)
        self.btn_mode_qual.connect("toggled", self._on_mode_changed)
        mode_box.append(self.btn_mode_size)
        mode_box.append(self.btn_mode_pct)
        mode_box.append(self.btn_mode_qual)
        root.append(mode_box)

        # Stack for the 3 clean input modes
        self.stack = Gtk.Stack()
        self.stack.set_transition_type(Gtk.StackTransitionType.CROSSFADE)
        self.stack.set_transition_duration(120)
        root.append(self.stack)

        # Page 1: Target Size
        p_size = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10)
        size_row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        self.size_entry = Gtk.Entry()
        default_size = "10 MB" if self.family == "video" else ("2 MB" if self.family == "pdf" else "500 KB")
        if self.total_bytes > 0 and self.total_bytes < 500_000 and self.family == "image":
            default_size = f"{max(20, int(self.total_bytes * 0.5 / 1000))} KB"
        self.size_entry.set_text(default_size)
        self.size_entry.set_placeholder_text("e.g. 10 MB, 500 KB, 2.5 MiB")
        self.size_entry.set_hexpand(True)
        self.size_entry.connect("changed", lambda *_: self._update_summary())
        self.size_entry.connect("activate", lambda *_: self._start_shrink())
        size_row.append(Gtk.Label(label="Max size:"))
        size_row.append(self.size_entry)
        p_size.append(size_row)

        presets = Gtk.FlowBox()
        presets.set_selection_mode(Gtk.SelectionMode.NONE)
        presets.set_row_spacing(6)
        presets.set_column_spacing(6)
        presets.set_max_children_per_line(4)
        preset_list = (
            [("5 MB", "5 MB"), ("10 MB Discord", "10 MB"), ("25 MB Email", "25 MB"), ("50 MB", "50 MB")]
            if self.family == "video"
            else [("100 KB", "100 KB"), ("500 KB Web", "500 KB"), ("2 MB", "2 MB"), ("10 MB Discord", "10 MB")]
        )
        for label, val in preset_list:
            b = Gtk.Button(label=label)
            b.add_css_class("preset-btn")
            b.connect("clicked", self._set_size_preset, val)
            presets.append(b)
        p_size.append(presets)
        self.stack.add_named(p_size, "size")

        # Page 2: Size % Wheel (slider + scroll-wheel SpinButton)
        p_pct = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        pct_row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=10)
        self.pct_adj = Gtk.Adjustment(value=50, lower=5, upper=95, step_increment=5, page_increment=10)
        self.pct_scale = Gtk.Scale(orientation=Gtk.Orientation.HORIZONTAL, adjustment=self.pct_adj)
        self.pct_scale.set_draw_value(False)
        self.pct_scale.set_hexpand(True)
        for mark in (25, 50, 75):
            self.pct_scale.add_mark(mark, Gtk.PositionType.BOTTOM, f"{mark}%")
        self.pct_spin = Gtk.SpinButton(adjustment=self.pct_adj, climb_rate=1, digits=0)
        self.pct_spin.set_width_chars(4)
        self.pct_adj.connect("value-changed", lambda *_: self._update_summary())
        self._attach_wheel(self.pct_scale, self.pct_adj, 5)
        pct_row.append(self.pct_scale)
        pct_row.append(self.pct_spin)
        pct_row.append(Gtk.Label(label="% of original"))
        p_pct.append(pct_row)
        pct_hint = Gtk.Label(
            label="Finds the highest quality that fits inside the chosen percentage of original bytes.",
            xalign=0,
            wrap=True,
        )
        pct_hint.add_css_class("subtitle-meta")
        p_pct.append(pct_hint)
        self.stack.add_named(p_pct, "percent")

        # Page 3: Quality Wheel (slider + scroll-wheel SpinButton + lossless toggle)
        p_qual = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        qual_row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=10)
        self.qual_adj = Gtk.Adjustment(value=82, lower=10, upper=100, step_increment=2, page_increment=10)
        self.qual_scale = Gtk.Scale(orientation=Gtk.Orientation.HORIZONTAL, adjustment=self.qual_adj)
        self.qual_scale.set_draw_value(False)
        self.qual_scale.set_hexpand(True)
        for mark, lbl in ((25, "Small"), (50, "50"), (80, "Sharp"), (100, "Max")):
            self.qual_scale.add_mark(mark, Gtk.PositionType.BOTTOM, lbl)
        self.qual_spin = Gtk.SpinButton(adjustment=self.qual_adj, climb_rate=1, digits=0)
        self.qual_spin.set_width_chars(4)
        self.qual_adj.connect("value-changed", lambda *_: self._update_summary())
        self._attach_wheel(self.qual_scale, self.qual_adj, 2)
        qual_row.append(self.qual_scale)
        qual_row.append(self.qual_spin)
        qual_row.append(Gtk.Label(label="% quality"))
        p_qual.append(qual_row)
        self.chk_lossless = Gtk.CheckButton(label="Lossless optimization only (strip metadata + max lossless recompress)")
        self.chk_lossless.connect("toggled", self._on_lossless_toggled)
        p_qual.append(self.chk_lossless)
        self.stack.add_named(p_qual, "quality")

        # Live summary banner
        self.summary_lbl = Gtk.Label(xalign=0, wrap=True)
        self.summary_lbl.add_css_class("summary-pill")
        root.append(self.summary_lbl)

        # Collapsible Advanced Options
        adv_exp = Gtk.Expander(label="Advanced options")
        adv_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10)
        adv_box.set_margin_top(10)

        grid = Gtk.Grid(column_spacing=12, row_spacing=8)
        row_i = 0

        grid.attach(Gtk.Label(label="Output format:", xalign=0), 0, row_i, 1, 1)
        self.fmt_dd = Gtk.DropDown.new_from_strings([x[0] for x in self.fmt_options])
        self.fmt_dd.set_hexpand(True)
        self.fmt_dd.connect("notify::selected", lambda *_: self._update_summary())
        grid.attach(self.fmt_dd, 1, row_i, 1, 1)
        row_i += 1

        self.res_dd = Gtk.DropDown.new_from_strings([x[0] for x in RES_CHOICES])
        self.res_dd.set_hexpand(True)
        self.res_dd.connect("notify::selected", lambda *_: self._update_summary())
        if self.family in ("image", "video", "mixed"):
            grid.attach(Gtk.Label(label="Max resolution:", xalign=0), 0, row_i, 1, 1)
            grid.attach(self.res_dd, 1, row_i, 1, 1)
            row_i += 1

        self.preset_dd = Gtk.DropDown.new_from_strings([x[0] for x in PRESET_CHOICES])
        self.preset_dd.set_hexpand(True)
        if self.family in ("video", "mixed"):
            grid.attach(Gtk.Label(label="Encoder speed:", xalign=0), 0, row_i, 1, 1)
            grid.attach(self.preset_dd, 1, row_i, 1, 1)
            row_i += 1

        adv_box.append(grid)

        self.chk_strip = Gtk.CheckButton(label="Strip metadata (EXIF, GPS, camera tags)")
        self.chk_strip.set_active(True)
        self.chk_strip.connect("toggled", lambda *_: self._update_summary())
        adv_box.append(self.chk_strip)

        self.chk_twopass = Gtk.CheckButton(label="Two-pass exact bitrate targeting (video)")
        self.chk_twopass.set_active(True)
        if self.family in ("video", "mixed"):
            adv_box.append(self.chk_twopass)

        self.chk_inplace = Gtk.CheckButton(label="Replace original file in-place (otherwise saves *-shrunk)")
        self.chk_inplace.connect("toggled", lambda *_: self._update_summary())
        adv_box.append(self.chk_inplace)

        adv_exp.set_child(adv_box)
        root.append(adv_exp)
        self.adv_exp = adv_exp

        # Progress + Result area (hidden until started)
        self.progress_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
        self.progress_bar = Gtk.ProgressBar()
        self.progress_bar.set_show_text(True)
        self.result_lbl = Gtk.Label(xalign=0, wrap=True)
        self.result_lbl.set_selectable(True)
        self.progress_box.append(self.progress_bar)
        self.progress_box.append(self.result_lbl)
        self.progress_box.set_visible(False)
        root.append(self.progress_box)

        # Action bar
        actions = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        actions.set_halign(Gtk.Align.END)
        self.btn_open = Gtk.Button(label="Open folder")
        self.btn_open.connect("clicked", self._open_folder)
        self.btn_open.set_visible(False)
        self.btn_cancel = Gtk.Button(label="Cancel")
        self.btn_cancel.connect("clicked", lambda *_: self.close())
        self.btn_go = Gtk.Button(label="Shrink")
        self.btn_go.add_css_class("suggested-action")
        self.btn_go.connect("clicked", lambda *_: self._start_shrink())
        actions.append(self.btn_open)
        actions.append(self.btn_cancel)
        actions.append(self.btn_go)
        root.append(actions)

        self._update_summary()

    def _attach_wheel(self, widget, adj, step):
        ctrl = Gtk.EventControllerScroll.new(Gtk.EventControllerScrollFlags.VERTICAL)
        def on_scroll(_c, _dx, dy):
            delta = -step if dy > 0 else step
            lo, hi = adj.get_lower(), adj.get_upper()
            adj.set_value(max(lo, min(hi, adj.get_value() + delta)))
            return True
        ctrl.connect("scroll", on_scroll)
        widget.add_controller(ctrl)

    def _on_key(self, _ctrl, keyval, _keycode, _state):
        if keyval == Gdk.KEY_Escape and not self.running:
            self.close()
            return True
        return False

    def _on_mode_changed(self, *_):
        if self.btn_mode_size.get_active():
            self.mode = "size"
        elif self.btn_mode_pct.get_active():
            self.mode = "percent"
        else:
            self.mode = "quality"
        self.stack.set_visible_child_name(self.mode)
        self._update_summary()

    def _on_lossless_toggled(self, *_):
        lossless = self.chk_lossless.get_active()
        self.qual_scale.set_sensitive(not lossless)
        self.qual_spin.set_sensitive(not lossless)
        self._update_summary()

    def _set_size_preset(self, _btn, val):
        self.size_entry.set_text(val)
        self._update_summary()

    def _update_summary(self):
        parts = []
        if self.mode == "size":
            txt = self.size_entry.get_text().strip() or "?"
            parts.append(f"Target <= {txt} (auto max quality)")
        elif self.mode == "percent":
            pct = int(self.pct_adj.get_value())
            if len(self.files) == 1 and self.first_bytes > 0:
                est = fmt_bytes(int(self.first_bytes * pct / 100.0))
                parts.append(f"Target <= {pct}% ({est})")
            else:
                parts.append(f"Target <= {pct}% of each file")
        else:
            if self.chk_lossless.get_active():
                parts.append("Lossless recompression")
            else:
                parts.append(f"Quality {int(self.qual_adj.get_value())}%")

        fmt_label, fmt_val, hevc = self.fmt_options[self.fmt_dd.get_selected()]
        if fmt_val:
            parts.append("MP4 HEVC" if hevc else fmt_val.upper())
        res_val = RES_CHOICES[self.res_dd.get_selected()][1]
        if res_val:
            parts.append(f"max {res_val}px")
        if self.chk_strip.get_active():
            parts.append("strip metadata")
        parts.append("replaces original" if self.chk_inplace.get_active() else "keeps original")
        self.summary_lbl.set_text("  |  ".join(parts))

    def _build_command(self):
        cmd = [self.convto_bin, "shrink", "--json"]
        if self.notify:
            cmd.append("--notify")
        if self.mode == "size":
            cmd.extend(["--target-size", self.size_entry.get_text().strip()])
        elif self.mode == "percent":
            cmd.extend(["--percent", str(int(self.pct_adj.get_value()))])
        else:
            if self.chk_lossless.get_active():
                cmd.append("--lossless")
            else:
                cmd.extend(["--quality", str(int(self.qual_adj.get_value()))])

        _, fmt_val, hevc = self.fmt_options[self.fmt_dd.get_selected()]
        if fmt_val:
            cmd.extend(["--to", fmt_val])
        if hevc:
            cmd.append("--hevc")
        res_val = RES_CHOICES[self.res_dd.get_selected()][1]
        if res_val:
            cmd.extend(["--max-dim", str(res_val)])
        if self.family in ("video", "mixed"):
            preset = PRESET_CHOICES[self.preset_dd.get_selected()][1]
            cmd.extend(["--preset", preset])
            if not self.chk_twopass.get_active():
                cmd.append("--no-two-pass")
        if not self.chk_strip.get_active():
            cmd.append("--keep-metadata")
        if self.chk_inplace.get_active():
            cmd.append("--in-place")
        cmd.append("--")
        cmd.extend(self.files)
        return cmd

    def _start_shrink(self):
        if self.running:
            return
        if self.finished:
            self.close()
            return
        if self.mode == "size" and not self.size_entry.get_text().strip():
            self.result_lbl.set_text("Enter a target size such as 10 MB or 500 KB.")
            self.progress_box.set_visible(True)
            return

        self.running = True
        self.btn_go.set_sensitive(False)
        self.btn_cancel.set_sensitive(False)
        self.adv_exp.set_sensitive(False)
        self.stack.set_sensitive(False)
        self.btn_mode_size.set_sensitive(False)
        self.btn_mode_pct.set_sensitive(False)
        self.btn_mode_qual.set_sensitive(False)
        self.progress_box.set_visible(True)
        self.progress_bar.set_fraction(0.0)
        self.progress_bar.set_text(f"Shrinking {len(self.files)} file(s)...")
        self.result_lbl.set_text("")

        cmd = self._build_command()
        threading.Thread(target=self._run_worker, args=(cmd,), daemon=True).start()
        GLib.timeout_add(120, self._pulse_while_running)

    def _pulse_while_running(self):
        if self.running:
            self.progress_bar.pulse()
            return True
        return False

    def _run_worker(self, cmd):
        proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        events = []
        for line in proc.stdout:
            line = line.strip()
            if not line:
                continue
            try:
                ev = json.loads(line)
                events.append(ev)
                GLib.idle_add(self._on_progress_event, ev, len(events))
            except Exception:
                pass
        stderr = proc.stderr.read().strip()
        code = proc.wait()
        GLib.idle_add(self._on_done, code, events, stderr)

    def _on_progress_event(self, ev, done_count):
        total = max(1, len(self.files))
        self.progress_bar.set_fraction(min(1.0, done_count / total))
        self.progress_bar.set_text(f"Processed {done_count}/{total}")

    def _on_done(self, code, events, stderr):
        self.running = False
        self.finished = True
        self.progress_bar.set_fraction(1.0)
        lines = []
        ok_count = 0
        err_count = 0
        for ev in events:
            status = ev.get("status")
            if status == "shrunk":
                ok_count += 1
                name = os.path.basename(ev.get("output", ""))
                before = fmt_bytes(ev.get("original_bytes", 0))
                after = fmt_bytes(ev.get("final_bytes", 0))
                saved = ev.get("saved_percent", 0.0)
                detail = ev.get("detail", "")
                lines.append(f"{name}: {before} -> {after} (-{saved:.1f}%, {detail})")
            elif status == "optimal":
                name = os.path.basename(ev.get("source", ""))
                lines.append(f"{name}: {ev.get('reason', 'already optimal')}")
            elif status == "error":
                err_count += 1
                lines.append(f"Error: {ev.get('error', 'failed')}")
        if not lines and stderr:
            err_count += 1
            lines.append(stderr)
        if err_count == 0 and code == 0:
            self.progress_bar.set_text(f"Done ({ok_count} shrunk)" if ok_count else "Done (already optimal)")
            self.result_lbl.add_css_class("result-ok")
        else:
            self.progress_bar.set_text("Finished with errors")
            self.result_lbl.add_css_class("result-err")
        self.result_lbl.set_text("\n".join(lines[:6]))
        self.btn_open.set_visible(True)
        self.btn_cancel.set_visible(False)
        self.btn_go.set_label("Done")
        self.btn_go.set_sensitive(True)

    def _open_folder(self, *_):
        if self.files:
            folder = os.path.dirname(os.path.abspath(self.files[0]))
            subprocess.Popen(["xdg-open", folder])


def main():
    if len(sys.argv) < 4:
        sys.exit(2)
    convto_bin = sys.argv[1]
    notify = sys.argv[2] == "1"
    files = sys.argv[3:]

    if HAS_ADW:
        app = Adw.Application(application_id="io.github.alexandersrc.convto", flags=Gio.ApplicationFlags.NON_UNIQUE)
    else:
        app = Gtk.Application(application_id="io.github.alexandersrc.convto", flags=Gio.ApplicationFlags.NON_UNIQUE)

    def on_activate(a):
        win = ShrinkWindow(a, convto_bin, files, notify)
        win.present()

    app.connect("activate", on_activate)
    app.run(None)


if __name__ == "__main__":
    main()
