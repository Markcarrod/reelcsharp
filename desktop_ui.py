import concurrent.futures
import ctypes
import ctypes.wintypes as wintypes
import json
import math
import os
import re
import signal
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
        self.script_source = tk.StringVar(value="")
        self.duration = tk.StringVar(value="12.5")
        self.workers = tk.StringVar(value="auto")
        self.manual_workers = tk.BooleanVar(value=False)
        self.last_manual_workers = "4"
        self.blur_strength = tk.StringVar(value="none")
        self.status = tk.StringVar(value="Ready")
        self.batch_progress = tk.IntVar(value=0)
        self.batch_progress_text = tk.StringVar(value="")
        self.preview_image: ImageTk.PhotoImage | None = None
        self.process: subprocess.Popen | None = None
        self.is_paused = False

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
        options.columnconfigure(4, weight=1)
        ttk.Label(options, text="Video Duration").grid(row=0, column=0, sticky="w")
        ttk.Entry(options, textvariable=self.duration, width=8).grid(row=0, column=1, sticky="w", padx=(8, 22))
        ttk.Label(options, text="Parallel Threads / auto").grid(row=0, column=2, sticky="w")
        self.workers_entry = ttk.Entry(options, textvariable=self.workers, width=8)
        self.workers_entry.grid(row=0, column=3, sticky="w", padx=(8, 8))
        ttk.Checkbutton(
            options,
            text="Manual",
            variable=self.manual_workers,
            command=self.toggle_manual_workers,
        ).grid(row=0, column=4, sticky="w")
        ttk.Label(options, text="Blur").grid(row=1, column=0, sticky="w", pady=(8, 0))
        ttk.Combobox(
            options,
            textvariable=self.blur_strength,
            values=("none", "light", "middle", "heavy"),
            state="readonly",
            width=10,
        ).grid(row=1, column=1, sticky="w", padx=(8, 22), pady=(8, 0))

        # Script source + editor
        script_frame = ttk.LabelFrame(left, text="Reel Script Editor", padding=10)
        script_frame.grid(row=2, column=0, sticky="nsew", pady=(12, 0))
        script_frame.columnconfigure(0, weight=1)
        script_frame.rowconfigure(1, weight=1)
        source_row = ttk.Frame(script_frame)
        source_row.grid(row=0, column=0, columnspan=2, sticky="ew", pady=(0, 8))
        source_row.columnconfigure(1, weight=1)
        ttk.Label(source_row, text="Script Source").grid(row=0, column=0, sticky="w")
        ttk.Entry(source_row, textvariable=self.script_source).grid(row=0, column=1, sticky="ew", padx=8)
        self.script_text = tk.Text(script_frame, wrap="word", undo=True, height=18)
        self.script_text.grid(row=1, column=0, sticky="nsew")
        scrollbar = ttk.Scrollbar(script_frame, command=self.script_text.yview)
        scrollbar.grid(row=1, column=1, sticky="ns")
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
        ttk.Button(buttons, text="Load Folder", command=self.load_script_folder).pack(side="left", padx=(8, 0))
        ttk.Button(buttons, text="Use Editor", command=self.use_editor_script).pack(side="left", padx=(8, 0))
        ttk.Button(buttons, text="Save Settings", command=self.save_input).pack(side="left", padx=(8, 0))
        ttk.Button(buttons, text="Start Rust Render", command=self.start_render, style="Accent.TButton").pack(side="left", padx=(8, 0))
        ttk.Button(buttons, text="Pause", command=self.pause_render).pack(side="left", padx=(8, 0))
        ttk.Button(buttons, text="Resume", command=self.resume_render).pack(side="left", padx=(8, 0))
        ttk.Button(buttons, text="Stop", command=self.cancel_render).pack(side="left", padx=(8, 0))
        ttk.Button(buttons, text="Clear Log", command=self.clear_log).pack(side="left", padx=(8, 0))
        ttk.Label(buttons, textvariable=self.status, font=("Helvetica", 10, "bold")).pack(side="right")

        progress_row = ttk.Frame(left)
        progress_row.grid(row=4, column=0, sticky="ew", pady=(8, 0))
        progress_row.columnconfigure(0, weight=1)
        self.progress_bar = ttk.Progressbar(
            progress_row,
            variable=self.batch_progress,
            mode="determinate",
            maximum=1,
        )
        self.progress_bar.grid(row=0, column=0, sticky="ew")
        ttk.Label(progress_row, textvariable=self.batch_progress_text, width=14).grid(row=0, column=1, padx=(8, 0))

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
        self.script_source.set(str(Path(file_path).resolve()))
        self.script_text.delete("1.0", "end")
        self.script_text.insert("1.0", Path(file_path).read_text(encoding="utf-8"))
        self.save_state()

    def load_script_folder(self) -> None:
        init_dir = str(ROOT.parent / "python_reel_forge" / "input" / "scripts")
        folder = filedialog.askdirectory(initialdir=init_dir)
        if not folder:
            return
        folder_path = Path(folder).resolve()
        script_files = sorted(folder_path.rglob("*.txt"))
        self.script_source.set(str(folder_path))
        self.script_text.delete("1.0", "end")
        if script_files:
            preview = "\n".join(str(path) for path in script_files[:20])
            if len(script_files) > 20:
                preview += f"\n... and {len(script_files) - 20} more file(s)"
            self.script_text.insert(
                "1.0",
                f"Folder mode enabled.\nAll .txt files in this folder will be rendered.\n\n{preview}",
            )
        else:
            self.script_text.insert("1.0", "Folder mode enabled, but no .txt files were found yet.")
        self.save_state()

    def use_editor_script(self) -> None:
        self.script_source.set("")
        self.save_state()

    def current_state(self) -> dict[str, str | bool]:
        return {
            "video_folder": self.video_folder.get(),
            "music_folder": self.music_folder.get(),
            "output_folder": self.output_folder.get(),
            "overlay_folder": self.overlay_folder.get(),
            "script_source": self.script_source.get(),
            "duration": self.duration.get(),
            "workers": self.normalized_workers(),
            "manual_workers": self.manual_workers.get(),
            "blur_strength": self.blur_strength.get(),
            "script_text": self.script_text.get("1.0", "end").rstrip(),
        }

    def normalized_workers(self) -> str:
        value = self.workers.get().strip()
        if self.manual_workers.get():
            return value or self.last_manual_workers or "4"
        return "auto"

    def toggle_manual_workers(self) -> None:
        if self.manual_workers.get():
            current = self.workers.get().strip()
            if current and current.isdigit():
                self.last_manual_workers = current
            self.workers.set(self.last_manual_workers or "4")
            self.workers_entry.configure(state="normal")
        else:
            current = self.workers.get().strip()
            if current and current.isdigit():
                self.last_manual_workers = current
            self.workers.set("auto")
            self.workers_entry.configure(state="disabled")
        self.save_state()

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
        self.script_source.set(state.get("script_source", self.script_source.get()))
        self.duration.set(state.get("duration", self.duration.get()))
        saved_workers = (state.get("workers", self.workers.get()) or "auto").strip() or "auto"
        saved_manual = bool(state.get("manual_workers", saved_workers.isdigit()))
        if saved_workers.isdigit():
            self.last_manual_workers = saved_workers
        self.manual_workers.set(saved_manual)
        self.workers.set(saved_workers if saved_manual else "auto")
        self.workers_entry.configure(state="normal" if saved_manual else "disabled")
        self.blur_strength.set(state.get("blur_strength", self.blur_strength.get()))
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

    def clear_log(self) -> None:
        self.log_text.configure(state="normal")
        self.log_text.delete("1.0", "end")
        self.log_text.configure(state="disabled")
        self.batch_progress.set(0)
        self.batch_progress_text.set("")

    def handle_backend_line(self, line: str) -> None:
        self.log(line)
        match = re.search(r"Completed batch file\s+(\d+)/(\d+)", line)
        if match:
            current = int(match.group(1))
            total = max(1, int(match.group(2)))
            self.progress_bar.configure(maximum=total)
            self.batch_progress.set(current)
            self.batch_progress_text.set(f"{current}/{total}")

    def cancel_render(self) -> None:
        if self.process:
            self.log("🛑 Cancelling current Rust compile/render process...")
            self.stop_process_tree(self.process)
            if self.process is proc:
                self.process = None
            self.is_paused = False
            self.is_paused = False
            self.status.set("Cancelled")

    def pause_render(self) -> None:
        if self.process and not self.is_paused:
            self.set_process_tree_suspended(self.process.pid, True)
            self.is_paused = True
            self.status.set("Paused")
            self.log("Render paused.")

    def resume_render(self) -> None:
        if self.process and self.is_paused:
            self.set_process_tree_suspended(self.process.pid, False)
            self.is_paused = False
            self.status.set("Rendering...")
            self.log("Render resumed.")

    def stop_process_tree(self, process: subprocess.Popen) -> None:
        if os.name == "nt":
            subprocess.run(
                ["taskkill", "/PID", str(process.pid), "/T", "/F"],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
            )
        else:
            process.terminate()

    def set_process_tree_suspended(self, root_pid: int, suspend: bool) -> None:
        if os.name != "nt":
            sig = signal.SIGSTOP if suspend else signal.SIGCONT
            try:
                os.kill(root_pid, sig)
            except OSError as e:
                self.log(f"Could not update process state: {e}")
            return

        action = self.suspend_pid if suspend else self.resume_pid
        for pid in self.process_tree_pids(root_pid):
            action(pid)

    def process_tree_pids(self, root_pid: int) -> list[int]:
        if os.name != "nt":
            return [root_pid]

        class PROCESSENTRY32(ctypes.Structure):
            _fields_ = [
                ("dwSize", wintypes.DWORD),
                ("cntUsage", wintypes.DWORD),
                ("th32ProcessID", wintypes.DWORD),
                ("th32DefaultHeapID", ctypes.POINTER(ctypes.c_ulong)),
                ("th32ModuleID", wintypes.DWORD),
                ("cntThreads", wintypes.DWORD),
                ("th32ParentProcessID", wintypes.DWORD),
                ("pcPriClassBase", ctypes.c_long),
                ("dwFlags", wintypes.DWORD),
                ("szExeFile", ctypes.c_char * 260),
            ]

        snapshot = ctypes.windll.kernel32.CreateToolhelp32Snapshot(0x00000002, 0)
        if snapshot == wintypes.HANDLE(-1).value:
            return [root_pid]

        try:
            entries: list[tuple[int, int]] = []
            entry = PROCESSENTRY32()
            entry.dwSize = ctypes.sizeof(PROCESSENTRY32)
            if ctypes.windll.kernel32.Process32First(snapshot, ctypes.byref(entry)):
                while True:
                    entries.append((int(entry.th32ProcessID), int(entry.th32ParentProcessID)))
                    if not ctypes.windll.kernel32.Process32Next(snapshot, ctypes.byref(entry)):
                        break

            pids = [root_pid]
            changed = True
            while changed:
                changed = False
                parents = set(pids)
                for pid, parent_pid in entries:
                    if parent_pid in parents and pid not in pids:
                        pids.append(pid)
                        changed = True
            return list(reversed(pids))
        finally:
            ctypes.windll.kernel32.CloseHandle(snapshot)

    def suspend_pid(self, pid: int) -> None:
        self.call_nt_process_state(pid, "NtSuspendProcess")

    def resume_pid(self, pid: int) -> None:
        self.call_nt_process_state(pid, "NtResumeProcess")

    def call_nt_process_state(self, pid: int, function_name: str) -> None:
        process_suspend_resume = 0x0800
        handle = ctypes.windll.kernel32.OpenProcess(process_suspend_resume, False, pid)
        if not handle:
            return
        try:
            getattr(ctypes.windll.ntdll, function_name)(handle)
        finally:
            ctypes.windll.kernel32.CloseHandle(handle)

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
            "--workers", self.normalized_workers(),
            "--blur", self.blur_strength.get(),
        ])
        return cmd

    def start_render(self) -> None:
        self.cancel_render()
        self.save_state()

        if self.manual_workers.get():
            worker_value = self.workers.get().strip()
            if not worker_value.isdigit() or int(worker_value) <= 0:
                messagebox.showerror("Error", "Manual threads must be a positive whole number.")
                return

        script_source = self.script_source.get().strip()
        if script_source:
            temp_script_path = Path(script_source)
            if not temp_script_path.exists():
                messagebox.showerror("Error", "Selected script file or folder does not exist.")
                return
        else:
            script_text = self.script_text.get("1.0", "end").strip()
            if not script_text:
                messagebox.showerror("Error", "Please enter a script layout first.")
                return

            # Write to a temp script file
            temp_script_path = ROOT / "output" / "temp_script.txt"
            temp_script_path.parent.mkdir(parents=True, exist_ok=True)
            temp_script_path.write_text(script_text, encoding="utf-8")

        self.status.set("Rendering...")
        self.is_paused = False
        self.batch_progress.set(0)
        self.batch_progress_text.set("")
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

        proc = None
        try:
            proc = subprocess.Popen(
                cmd,
                cwd=str(ROOT),
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                encoding="utf-8",
                errors="replace",
                bufsize=1,
                universal_newlines=True,
                creationflags=subprocess.CREATE_NO_WINDOW if os.name == 'nt' else 0
            )
            self.process = proc

            # Stream logs in real-time to UI
            while True:
                line = proc.stdout.readline()
                if not line and proc.poll() is not None:
                    break
                if line:
                    self.after(0, self.handle_backend_line, line.rstrip())

            rc = proc.poll()
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

