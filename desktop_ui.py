import concurrent.futures
import json
import math
import os
import re
import subprocess
import sys
import threading
import time
import tkinter as tk
from pathlib import Path
from tkinter import filedialog, messagebox, ttk

from PIL import Image, ImageTk

ROOT = Path(__file__).resolve().parent
STATE_PATH = ROOT / "config" / "desktop_state.json"
PREVIEW_PATH = ROOT / "output" / "preview.png"

# Default directories matching standard layouts
DEFAULT_VIDEOS = ROOT.parent / "python_reel_forge" / "input" / "videos"
DEFAULT_MUSIC = ROOT.parent / "python_reel_forge" / "input" / "music"
DEFAULT_OUTPUT = ROOT / "output" / "videos"
DEFAULT_OVERLAYS = ROOT / "output" / "overlays"

class RustReelForgeDesktop(tk.Tk):
    def __init__(self) -> None:
        super().__init__()
        self.title("Rust Reel Forge — Blazing Fast Engine")
        self.geometry("1120x780")
        self.minsize(980, 700)

        # Config variables
        self.video_folder = tk.StringVar(value=str(DEFAULT_VIDEOS.resolve()))
        self.music_folder = tk.StringVar(value=str(DEFAULT_MUSIC.resolve()))
        self.output_folder = tk.StringVar(value=str(DEFAULT_OUTPUT.resolve()))
        self.overlay_folder = tk.StringVar(value=str(DEFAULT_OVERLAYS.resolve()))
        self.duration = tk.StringVar(value="12.5")
        self.workers = tk.StringVar(value="4")
        self.status = tk.StringVar(value="Ready")
        self.preview_image: ImageTk.PhotoImage | None = None
        self.process: subprocess.Popen | None = None

        self._build_ui()
        self.load_saved_state()
        self.protocol("WM_DELETE_WINDOW", self.on_close)

    def _build_ui(self) -> None:
        self.columnconfigure(0, weight=3)
        self.columnconfigure(1, weight=2)
        self.rowconfigure(0, weight=1)

        left = ttk.Frame(self, padding=14)
        right = ttk.Frame(self, padding=14)
        left.grid(row=0, column=0, sticky="nsew")
        right.grid(row=0, column=1, sticky="nsew")
        
        left.columnconfigure(0, weight=1)
        left.rowconfigure(2, weight=1)
        right.columnconfigure(0, weight=1)
        right.rowconfigure(1, weight=1)
        right.rowconfigure(3, weight=1)

        # Folders Configuration
        folders = ttk.LabelFrame(left, text="Configuration Folders", padding=10)
        folders.grid(row=0, column=0, sticky="ew")
        folders.columnconfigure(1, weight=1)

        self._folder_row(folders, 0, "Videos", self.video_folder)
        self._folder_row(folders, 1, "Music", self.music_folder)
        self._folder_row(folders, 2, "Output", self.output_folder)
        self._folder_row(folders, 3, "Overlays", self.overlay_folder)

        # Render options
        options = ttk.LabelFrame(left, text="Performance Controls", padding=10)
        options.grid(row=1, column=0, sticky="ew", pady=(12, 0))
        options.columnconfigure(3, weight=1)
        ttk.Label(options, text="Video Duration").grid(row=0, column=0, sticky="w")
        ttk.Entry(options, textvariable=self.duration, width=8).grid(row=0, column=1, sticky="w", padx=(8, 22))
        ttk.Label(options, text="Parallel Threads").grid(row=0, column=2, sticky="w")
        ttk.Entry(options, textvariable=self.workers, width=8).grid(row=0, column=3, sticky="w", padx=(8, 0))

        # Script TextBox
        script_frame = ttk.LabelFrame(left, text="Reel Script Editor", padding=10)
        script_frame.grid(row=2, column=0, sticky="nsew", pady=(12, 0))
        script_frame.columnconfigure(0, weight=1)
        script_frame.rowconfigure(0, weight=1)
        self.script_text = tk.Text(script_frame, wrap="word", undo=True, height=18)
        self.script_text.grid(row=0, column=0, sticky="nsew")
        scrollbar = ttk.Scrollbar(script_frame, command=self.script_text.yview)
        scrollbar.grid(row=0, column=1, sticky="ns")
        self.script_text.configure(yscrollcommand=scrollbar.set)
        
        # Load standard example into text box initially
        example_script = (ROOT.parent / "python_reel_forge" / "input" / "scripts" / "example.txt")
        if example_script.exists():
            self.script_text.insert("1.0", example_script.read_text(encoding="utf-8"))
        else:
            self.script_text.insert("1.0", "TITLE:Blazing Fast Rust\nThis runs at extreme speed.\nC++ and Rust rendering.\nCTA:Create, build, succeed.")

        # Bottom Actions
        buttons = ttk.Frame(left)
        buttons.grid(row=3, column=0, sticky="ew", pady=(12, 0))
        ttk.Button(buttons, text="Load Script", command=self.load_script).pack(side="left")
        ttk.Button(buttons, text="Save Settings", command=self.save_input).pack(side="left", padx=(8, 0))
        ttk.Button(buttons, text="Start Rust Render", command=self.start_render, style="Accent.TButton").pack(side="left", padx=(8, 0))
        ttk.Button(buttons, text="Cancel Render", command=self.cancel_render).pack(side="left", padx=(8, 0))
        ttk.Label(buttons, textvariable=self.status, font=("Helvetica", 10, "bold")).pack(side="right")

        # Preview area
        preview_frame = ttk.LabelFrame(right, text="Visual Layout Preview", padding=10)
        preview_frame.grid(row=0, column=0, rowspan=2, sticky="nsew")
        preview_frame.columnconfigure(0, weight=1)
        preview_frame.rowconfigure(0, weight=1)
        self.preview_label = ttk.Label(preview_frame, anchor="center", text="Click Render to generate clips")
        self.preview_label.grid(row=0, column=0, sticky="nsew")

        # Progress Logs
        log_frame = ttk.LabelFrame(right, text="Rust Backend Logs", padding=10)
        log_frame.grid(row=2, column=0, rowspan=2, sticky="nsew", pady=(12, 0))
        log_frame.columnconfigure(0, weight=1)
        log_frame.rowconfigure(0, weight=1)
        self.log_text = tk.Text(log_frame, height=14, wrap="word", state="disabled", font=("Consolas", 9))
        self.log_text.grid(row=0, column=0, sticky="nsew")
        log_scroll = ttk.Scrollbar(log_frame, command=self.log_text.yview)
        log_scroll.grid(row=0, column=1, sticky="ns")
        self.log_text.configure(yscrollcommand=log_scroll.set)

    def _folder_row(self, parent: ttk.Frame, row: int, label: str, variable: tk.StringVar) -> None:
        ttk.Label(parent, text=label, width=10).grid(row=row, column=0, sticky="w", pady=3)
        ttk.Entry(parent, textvariable=variable).grid(row=row, column=1, sticky="ew", padx=8, pady=3)
        ttk.Button(parent, text="Browse", command=lambda: self.pick_folder(variable)).grid(row=row, column=2, pady=3)

    def pick_folder(self, variable: tk.StringVar) -> None:
        folder = filedialog.askdirectory(initialdir=variable.get() or str(ROOT))
        if folder:
            variable.set(Path(folder).resolve())
            self.save_state()

    def load_script(self) -> None:
        init_dir = str(ROOT.parent / "python_reel_forge" / "input" / "scripts")
        file_path = filedialog.askopenfilename(
            initialdir=init_dir,
            filetypes=[("Text files", "*.txt"), ("All files", "*.*")],
        )
        if not file_path:
            return
        self.script_text.delete("1.0", "end")
        self.script_text.insert("1.0", Path(file_path).read_text(encoding="utf-8"))
        self.save_state()

    def current_state(self) -> dict[str, str]:
        return {
            "video_folder": self.video_folder.get(),
            "music_folder": self.music_folder.get(),
            "output_folder": self.output_folder.get(),
            "overlay_folder": self.overlay_folder.get(),
            "duration": self.duration.get(),
            "workers": self.workers.get(),
            "script_text": self.script_text.get("1.0", "end").rstrip(),
        }

    def load_saved_state(self) -> None:
        if not STATE_PATH.exists():
            return
        try:
            state = json.loads(STATE_PATH.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            return
        self.video_folder.set(state.get("video_folder", self.video_folder.get()))
        self.music_folder.set(state.get("music_folder", self.music_folder.get()))
        self.output_folder.set(state.get("output_folder", self.output_folder.get()))
        self.overlay_folder.set(state.get("overlay_folder", self.overlay_folder.get()))
        self.duration.set(state.get("duration", self.duration.get()))
        self.workers.set(state.get("workers", self.workers.get()))
        saved_script = state.get("script_text")
        if saved_script:
            self.script_text.delete("1.0", "end")
            self.script_text.insert("1.0", saved_script)

    def save_state(self) -> None:
        STATE_PATH.parent.mkdir(parents=True, exist_ok=True)
        STATE_PATH.write_text(json.dumps(self.current_state(), indent=2), encoding="utf-8")

    def save_input(self) -> None:
        self.save_state()
        self.status.set("Settings saved")
        self.log(f"Saved layout configuration to {STATE_PATH}")

    def on_close(self) -> None:
        self.cancel_render()
        self.save_state()
        self.destroy()

    def log(self, message: str) -> None:
        self.log_text.configure(state="normal")
        self.log_text.insert("end", f"{message}\n")
        self.log_text.see("end")
        self.log_text.configure(state="disabled")
        self.update_idletasks()

    def cancel_render(self) -> None:
        if self.process:
            self.log("🛑 Cancelling current Rust compile/render process...")
            self.process.terminate()
            self.process = None
            self.status.set("Cancelled")

    def get_rust_runner_command(self, script_path: Path) -> list[str]:
        # Path to precompiled release binary
        exe_path = ROOT / "target" / "release" / "rust_reel_forge.exe"
        debug_exe_path = ROOT / "target" / "debug" / "rust_reel_forge.exe"

        # Check for precompiled release binaries first for maximum speed
        if exe_path.exists():
            cmd = [str(exe_path)]
        elif debug_exe_path.exists():
            cmd = [str(debug_exe_path)]
        else:
            # Fallback to direct Cargo invocation
            cmd = ["cargo", "run", "--release", "--"]

        # Append CLI arguments
        cmd.extend([
            "--script", str(script_path),
            "--videos", self.video_folder.get(),
            "--music", self.music_folder.get(),
            "--output", self.output_folder.get(),
            "--overlays", self.overlay_folder.get(),
            "--duration", self.duration.get(),
            "--workers", self.workers.get(),
        ])
        return cmd

    def start_render(self) -> None:
        self.cancel_render()
        self.save_state()
        
        script_text = self.script_text.get("1.0", "end").strip()
        if not script_text:
            messagebox.showerror("Error", "Please enter a script layout first.")
            return

        # Write to a temp script file
        temp_script_path = ROOT / "output" / "temp_script.txt"
        temp_script_path.parent.mkdir(parents=True, exist_ok=True)
        temp_script_path.write_text(script_text, encoding="utf-8")

        self.status.set("Rendering...")
        self.log("\n==================================================")
        self.log("🎨 LAUNCHING RUST REEL FORGE BACKEND")
        self.log("==================================================")

        thread = threading.Thread(
            target=self.run_rust_backend,
            args=(temp_script_path,),
            daemon=True
        )
        thread.start()

    def run_rust_backend(self, temp_script: Path) -> None:
        cmd = self.get_rust_runner_command(temp_script)
        self.log(f"Running command: {' '.join(cmd)}\n")

        try:
            self.process = subprocess.Popen(
                cmd,
                cwd=str(ROOT),
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                bufsize=1,
                universal_newlines=True,
                creationflags=subprocess.CREATE_NO_WINDOW if os.name == 'nt' else 0
            )

            # Stream logs in real-time to UI
            while True:
                line = self.process.stdout.readline()
                if not line and self.process.poll() is not None:
                    break
                if line:
                    self.after(0, self.log, line.rstrip())

            rc = self.process.poll()
            if rc == 0:
                self.after(0, self.status.set, "Done ✅")
                self.after(0, self.log, "\n🎉 Rendering successfully completed!")
                self.after(0, self.update_preview)
            else:
                self.after(0, self.status.set, "Failed ❌")
                self.after(0, self.log, f"\n❌ Process finished with exit code {rc}")
                
        except Exception as e:
            self.after(0, self.status.set, "Error ⚠️")
            self.after(0, self.log, f"\n⚠️ Error launching process: {e}")
        finally:
            self.process = None

    def update_preview(self) -> None:
        # Scan overlays output folder to display the latest frame as preview
        overlay_dir = Path(self.overlay_folder.get())
        if not overlay_dir.exists():
            return
        
        pngs = sorted(
            [f for f in overlay_dir.iterdir() if f.is_file() and f.suffix.lower() == ".png"],
            key=os.path.getmtime,
            reverse=True
        )

        if not pngs:
            return

        try:
            # Composite latest text overlay onto first background video frame
            latest_overlay = pngs[0]
            videos_dir = Path(self.video_folder.get())
            video_files = sorted(
                [f for f in videos_dir.iterdir() if f.is_file() and f.suffix.lower() in [".mp4", ".mov", ".webm"]],
                key=os.path.getmtime
            )

            if video_files:
                # Use standard Pillow drawing to make a layout preview
                base = Image.new("RGBA", (1080, 1920), (0, 0, 0, 255))
                dark = Image.new("RGBA", base.size, (0, 0, 0, 112))
                base.alpha_composite(dark)
                
                overlay = Image.open(latest_overlay).convert("RGBA")
                base.alpha_composite(overlay)
                
                view = base.resize((270, 480), Image.Resampling.LANCZOS)
                self.preview_image = ImageTk.PhotoImage(view)
                self.preview_label.configure(image=self.preview_image, text="")
        except Exception as e:
            self.log(f"Could not build preview image: {e}")

def main() -> None:
    # Setup premium ttk style looks if available
    app = RustReelForgeDesktop()
    app.mainloop()

if __name__ == "__main__":
    main()
