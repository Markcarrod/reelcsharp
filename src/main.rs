use clap::Parser;
use rand::seq::SliceRandom;
use rand::thread_rng;
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

mod parser;
mod overlay;
mod ffmpeg;

#[derive(Parser, Debug)]
#[command(name = "rust_reel_forge", version = "0.1.0", about = "Stunning 9:16 reels generator written in pure high-performance Rust")]
struct Args {
    #[arg(long, help = "Path to the script file (txt format)")]
    script: Option<PathBuf>,

    #[arg(long, default_value = "input/videos", help = "Folder containing background videos")]
    videos: PathBuf,

    #[arg(long, default_value = "input/music", help = "Folder containing background music")]
    music: PathBuf,

    #[arg(long, default_value = "output/videos", help = "Folder to save output MP4 videos")]
    output: PathBuf,

    #[arg(long, default_value = "output/overlays", help = "Folder to save transparent text PNG overlays")]
    overlays: PathBuf,

    #[arg(long, default_value_t = 12.5, help = "Default video duration in seconds")]
    duration: f32,

    #[arg(long, default_value_t = 2, help = "Number of parallel workers for rendering")]
    workers: usize,

    #[arg(long, default_value = "none", help = "Background blur strength: none, light, middle, heavy")]
    blur: String,
}

fn list_files_with_extensions(folder: &Path, extensions: &[&str]) -> Vec<PathBuf> {
    if !folder.exists() || !folder.is_dir() {
        return Vec::new();
    }
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(folder) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                    let ext_lower = ext.to_lowercase();
                    if extensions.iter().any(|&e| e == ext_lower) {
                        files.push(path);
                    }
                }
            }
        }
    }
    files
}

fn get_millisecond_stamp() -> String {
    let start = SystemTime::now();
    let since_the_epoch = start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");
    let ms = since_the_epoch.as_millis();
    let ms_str = ms.to_string();
    if ms_str.len() > 8 {
        ms_str[ms_str.len() - 8..].to_string()
    } else {
        ms_str
    }
}

// --------------------------------------------------
//               CLI MODE EXECUTION
// --------------------------------------------------
fn run_cli(args: Args) {
    let script_path = args.script.expect("Script path is required in CLI mode");
    let blur_strength = parser::BlurStrength::from_str(&args.blur);
    println!("--------------------------------------------------");
    println!("        🚀 RUST REEL FORGE — INITIALIZING         ");
    println!("--------------------------------------------------");

    let scripts = match parser::parse_scripts(&script_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("❌ Failed to parse script file: {}", e);
            std::process::exit(1);
        }
    };
    println!("📝 Loaded {} script(s) from {:?}", scripts.len(), script_path);

    let video_extensions = ["mp4", "mov", "mkv", "webm"];
    let audio_extensions = ["mp3", "wav", "m4a", "aac"];

    let mut videos = list_files_with_extensions(&args.videos, &video_extensions);
    let mut music_files = list_files_with_extensions(&args.music, &audio_extensions);

    if videos.is_empty() {
        println!("⚠️  No background videos found; rendering on solid black background.");
    } else {
        println!("📹 Found {} background video(s)", videos.len());
        let mut rng = thread_rng();
        videos.shuffle(&mut rng);
    }

    if music_files.is_empty() {
        println!("🎵 No music tracks found; rendering silent videos.");
    } else {
        println!("🎶 Found {} music track(s)", music_files.len());
        let mut rng = thread_rng();
        music_files.shuffle(&mut rng);
    }

    println!("Blur mode: {}", blur_strength.as_str());
    println!("🎨 Loading system font library...");
    let font = overlay::load_system_font();

    let workers = args.workers.max(1).min(scripts.len());
    println!("🧵 Spinning up worker pool ({} parallel workers)...", workers);

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .expect("Failed to initialize thread pool");

    let start_time = std::time::Instant::now();

    let outputs: Vec<Result<PathBuf, String>> = pool.install(|| {
        scripts
            .par_iter()
            .enumerate()
            .map(|(index, script)| {
                let stamp = get_millisecond_stamp();
                let duration = script.duration.unwrap_or(args.duration);
                let duration = 13.0f32.min(12.0f32.max(duration));

                println!("[Worker] Rendering script {}/{}: \"{}\"", index + 1, scripts.len(), script.title);

                let mut overlay_paths = Vec::new();
                match overlay::make_overlay(script, 0, &args.overlays, &stamp, &font) {
                    Ok(path) => overlay_paths.push(path),
                    Err(e) => return Err(format!("Failed to make title overlay: {}", e)),
                }

                for point_index in 0..script.points.len() {
                    match overlay::make_overlay(script, point_index + 1, &args.overlays, &stamp, &font) {
                        Ok(path) => overlay_paths.push(path),
                        Err(e) => return Err(format!("Failed to make point overlay {}: {}", point_index + 1, e)),
                    }
                }

                match ffmpeg::render_video(
                    script,
                    index,
                    &videos,
                    &music_files,
                    &args.output,
                    &overlay_paths,
                    duration,
                    blur_strength,
                    &stamp,
                    None,
                ) {
                    Ok(out_path) => Ok(out_path),
                    Err(e) => Err(format!("FFmpeg composition failed: {}", e)),
                }
            })
            .collect()
    });

    let elapsed = start_time.elapsed();
    println!("--------------------------------------------------");
    println!("               🎉 RENDERING DONE                  ");
    println!("--------------------------------------------------");
    println!("⏱️  Total elapsed time: {:.2?}", elapsed);

    let mut success_count = 0;
    for (i, res) in outputs.iter().enumerate() {
        match res {
            Ok(path) => {
                success_count += 1;
                println!("  ✅ Reel {}: {:?}", i + 1, path.file_name().unwrap());
            }
            Err(e) => {
                eprintln!("  ❌ Reel {}: Error -> {}", i + 1, e);
            }
        }
    }
    println!("📈 Rendered {}/{} videos successfully.", success_count, scripts.len());
}


// --------------------------------------------------
//               NATIVE RUST GUI MODE
// --------------------------------------------------
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
struct SavedConfig {
    video_folder: String,
    music_folder: String,
    output_folder: String,
    overlay_folder: String,
    duration: String,
    workers: String,
    #[serde(default = "default_blur_strength")]
    blur_strength: String,
    script_text: String,
}

fn default_blur_strength() -> String {
    "none".to_string()
}

fn load_config() -> Option<SavedConfig> {
    let path = Path::new("config/desktop_state.json");
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Ok(config) = serde_json::from_str::<SavedConfig>(&content) {
                return Some(config);
            }
        }
    }
    None
}

fn save_config(config: &SavedConfig) {
    let path = Path::new("config/desktop_state.json");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(serialized) = serde_json::to_string_pretty(config) {
        let _ = std::fs::write(path, serialized);
    }
}

struct AppState {
    video_folder: String,
    music_folder: String,
    output_folder: String,
    overlay_folder: String,
    duration: String,
    workers: String,
    blur_strength: String,
    script_text: String,
    logs: Arc<Mutex<Vec<String>>>,
    is_rendering: bool,
    status_msg: String,
    stop_requested: Arc<AtomicBool>,
}

impl Default for AppState {
    fn default() -> Self {
        // Attempt to load from saved JSON first, otherwise fallback to defaults
        if let Some(saved) = load_config() {
            Self {
                video_folder: saved.video_folder,
                music_folder: saved.music_folder,
                output_folder: saved.output_folder,
                overlay_folder: saved.overlay_folder,
                duration: saved.duration,
                workers: saved.workers,
                blur_strength: saved.blur_strength,
                script_text: saved.script_text,
                logs: Arc::new(Mutex::new(vec![
                    "Welcome to Rust Reel Forge! Restored saved configuration.".to_string(),
                ])),
                is_rendering: false,
                status_msg: "Ready".to_string(),
                stop_requested: Arc::new(AtomicBool::new(false)),
            }
        } else {
            Self {
                video_folder: "input/videos".to_string(),
                music_folder: "input/music".to_string(),
                output_folder: "output/videos".to_string(),
                overlay_folder: "output/overlays".to_string(),
                duration: "12.5".to_string(),
                workers: "4".to_string(),
                blur_strength: "none".to_string(),
                script_text: "TITLE:Fast Rust UI\nPerfect native execution.\nPure C++ and Rust performance.\nCTA:Accelerate your workflow.".to_string(),
                logs: Arc::new(Mutex::new(vec!["Welcome to Rust Reel Forge! GUI is ready.".to_string()])),
                is_rendering: false,
                status_msg: "Ready".to_string(),
                stop_requested: Arc::new(AtomicBool::new(false)),
            }
        }
    }
}

struct ReelForgeApp {
    state: AppState,
    log_receiver: Option<Receiver<String>>,
}

impl ReelForgeApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            state: AppState::default(),
            log_receiver: None,
        }
    }

    fn add_log(&self, msg: String) {
        if let Ok(mut logs) = self.state.logs.lock() {
            logs.push(msg);
        }
    }

    fn save_current_settings(&self) {
        let config = SavedConfig {
            video_folder: self.state.video_folder.clone(),
            music_folder: self.state.music_folder.clone(),
            output_folder: self.state.output_folder.clone(),
            overlay_folder: self.state.overlay_folder.clone(),
            duration: self.state.duration.clone(),
            workers: self.state.workers.clone(),
            blur_strength: self.state.blur_strength.clone(),
            script_text: self.state.script_text.clone(),
        };
        save_config(&config);
    }

    fn trigger_render(&mut self) {
        // Automatically save configuration on render launch
        self.save_current_settings();

        let video_dir = PathBuf::from(&self.state.video_folder);
        let music_dir = PathBuf::from(&self.state.music_folder);
        let output_dir = PathBuf::from(&self.state.output_folder);
        let overlay_dir = PathBuf::from(&self.state.overlay_folder);

        let duration: f32 = self.state.duration.parse().unwrap_or(12.5);
        let workers: usize = self.state.workers.parse().unwrap_or(4);
        let blur_strength = parser::BlurStrength::from_str(&self.state.blur_strength);
        let script_content = self.state.script_text.clone();
        let stop_requested = Arc::clone(&self.state.stop_requested);

        self.state.stop_requested.store(false, Ordering::Relaxed);
        self.state.is_rendering = true;
        self.state.status_msg = "Rendering...".to_string();

        let (tx, rx) = channel();
        self.log_receiver = Some(rx);

        // Spawn high-performance render thread
        std::thread::spawn(move || {
            let log = |m: &str| {
                let _ = tx.send(m.to_string());
            };

            log("🚀 Starting pure-Rust background render engine...");
            log(&format!("Blur mode: {}", blur_strength.as_str()));

            // Parse temporary script
            let temp_script = PathBuf::from("temp_gui_script.txt");
            if let Err(e) = std::fs::write(&temp_script, &script_content) {
                log(&format!("❌ Failed to write temp script: {}", e));
                return;
            }

            let scripts = match parser::parse_scripts(&temp_script) {
                Ok(s) => s,
                Err(e) => {
                    log(&format!("❌ Script parse error: {}", e));
                    return;
                }
            };
            log(&format!("📝 Parsed {} scripts successfully.", scripts.len()));

            let video_extensions = ["mp4", "mov", "mkv", "webm"];
            let audio_extensions = ["mp3", "wav", "m4a", "aac"];

            let mut videos = list_files_with_extensions(&video_dir, &video_extensions);
            let mut music_files = list_files_with_extensions(&music_dir, &audio_extensions);

            if videos.is_empty() {
                log("⚠️  No background videos found; using black canvas.");
            } else {
                log(&format!("📹 Loaded {} background video(s).", videos.len()));
                videos.shuffle(&mut thread_rng());
            }

            if music_files.is_empty() {
                log("🎵 No audio files found.");
            } else {
                log(&format!("🎶 Loaded {} audio track(s).", music_files.len()));
                music_files.shuffle(&mut thread_rng());
            }

            log("🎨 Compiling and loading system font libraries...");
            let font = overlay::load_system_font();

            let active_workers = workers.max(1).min(scripts.len());
            log(&format!("🧵 Spawning work pool with {} parallel workers...", active_workers));

            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(active_workers)
                .build()
                .unwrap();

            let start_time = std::time::Instant::now();

            let outputs: Vec<Result<PathBuf, String>> = pool.install(|| {
                scripts
                    .par_iter()
                    .enumerate()
                    .map(|(index, script)| {
                        if stop_requested.load(Ordering::Relaxed) {
                            return Err("Render stopped by user".to_string());
                        }

                        let stamp = get_millisecond_stamp();
                        let d = script.duration.unwrap_or(duration);
                        let d = 13.0f32.min(12.0f32.max(d));

                        let _ = tx.send(format!("[Worker] Building text frames for Reel {}...", index + 1));

                        let mut overlay_paths = Vec::new();
                        match overlay::make_overlay(script, 0, &overlay_dir, &stamp, &font) {
                            Ok(path) => overlay_paths.push(path),
                            Err(e) => return Err(format!("Overlay title failed: {}", e)),
                        }

                        for point_index in 0..script.points.len() {
                            if stop_requested.load(Ordering::Relaxed) {
                                return Err("Render stopped by user".to_string());
                            }
                            match overlay::make_overlay(script, point_index + 1, &overlay_dir, &stamp, &font) {
                                Ok(path) => overlay_paths.push(path),
                                Err(e) => return Err(format!("Overlay point failed: {}", e)),
                            }
                        }

                        let _ = tx.send(format!("[Worker] Multiplexing FFmpeg render for Reel {}...", index + 1));
                        match ffmpeg::render_video(
                            script,
                            index,
                            &videos,
                            &music_files,
                            &output_dir,
                            &overlay_paths,
                            d,
                            blur_strength,
                            &stamp,
                            Some(stop_requested.as_ref()),
                        ) {
                            Ok(out) => Ok(out),
                            Err(e) => Err(format!("FFmpeg failed: {}", e)),
                        }
                    })
                    .collect()
            });

            let elapsed = start_time.elapsed();
            log("==================================================");
            log(&format!("🎉 RENDER POOL DONE! Total time: {:.2?}", elapsed));
            log("==================================================");

            for (i, res) in outputs.iter().enumerate() {
                match res {
                    Ok(path) => log(&format!("  ✅ Reel {} generated: {:?}", i + 1, path.file_name().unwrap())),
                    Err(e) => log(&format!("  ❌ Reel {} failed: {}", i + 1, e)),
                }
            }

            let _ = std::fs::remove_file(&temp_script);
            let _ = tx.send("__FINISHED__".to_string());
        });
    }

    fn request_stop(&mut self) {
        self.state.stop_requested.store(true, Ordering::Relaxed);
        self.state.status_msg = "Stopping...".to_string();
        self.add_log("Stop requested. Finishing active work and preventing new reels from starting.".to_string());
    }
}

impl eframe::App for ReelForgeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Poll channel messages
        if let Some(ref rx) = self.log_receiver {
            while let Ok(msg) = rx.try_recv() {
                if msg == "__FINISHED__" {
                    let was_stopping = self.state.stop_requested.load(Ordering::Relaxed);
                    self.state.is_rendering = false;
                    self.state.stop_requested.store(false, Ordering::Relaxed);
                    if was_stopping {
                        self.state.status_msg = "Stopped".to_string();
                        self.add_log("⏹ Render stopped.".to_string());
                    } else {
                        self.state.status_msg = "Done ✅".to_string();
                        self.add_log("🎉 Process finished!".to_string());
                    }
                } else {
                    self.add_log(msg);
                }
            }
        }

        ctx.set_visuals(egui::Visuals::dark());

        // --------------------------------------------------
        // LEFT COLUMN PANEL: Independent scrollable layouts
        // --------------------------------------------------
        egui::SidePanel::left("left_config_panel")
            .resizable(false)
            .default_width(450.0)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(8.0);
                    ui.heading("🦀 Rust Reel Forge");
                    ui.label("Configuration & Layout Settings");
                    ui.add_space(4.0);
                });
                ui.separator();

                egui::ScrollArea::vertical().show(ui, |ui| {
                    // Config Form Grid
                    egui::Grid::new("config_grid")
                        .num_columns(3)
                        .spacing([8.0, 10.0])
                        .show(ui, |ui| {
                            ui.label("Videos Folder:");
                            ui.text_edit_singleline(&mut self.state.video_folder);
                            if ui.button("Browse...").clicked() {
                                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                    self.state.video_folder = path.to_string_lossy().to_string();
                                }
                            }
                            ui.end_row();

                            ui.label("Music Folder:");
                            ui.text_edit_singleline(&mut self.state.music_folder);
                            if ui.button("Browse...").clicked() {
                                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                    self.state.music_folder = path.to_string_lossy().to_string();
                                }
                            }
                            ui.end_row();

                            ui.label("Output Folder:");
                            ui.text_edit_singleline(&mut self.state.output_folder);
                            if ui.button("Browse...").clicked() {
                                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                    self.state.output_folder = path.to_string_lossy().to_string();
                                }
                            }
                            ui.end_row();

                            ui.label("Overlays Folder:");
                            ui.text_edit_singleline(&mut self.state.overlay_folder);
                            if ui.button("Browse...").clicked() {
                                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                    self.state.overlay_folder = path.to_string_lossy().to_string();
                                }
                            }
                            ui.end_row();

                            ui.label("Duration (sec):");
                            ui.text_edit_singleline(&mut self.state.duration);
                            ui.label("(12.0 - 13.0)");
                            ui.end_row();

                            ui.label("Threads:");
                            ui.text_edit_singleline(&mut self.state.workers);
                            ui.label("Parallel workers");
                            ui.end_row();

                            ui.label("Blur:");
                            egui::ComboBox::from_id_source("blur_strength")
                                .selected_text(self.state.blur_strength.clone())
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut self.state.blur_strength, "none".to_string(), "none");
                                    ui.selectable_value(&mut self.state.blur_strength, "light".to_string(), "light");
                                    ui.selectable_value(&mut self.state.blur_strength, "middle".to_string(), "middle");
                                    ui.selectable_value(&mut self.state.blur_strength, "heavy".to_string(), "heavy");
                                });
                            ui.label("Background blur strength");
                            ui.end_row();
                        });

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(4.0);
                    ui.label("Script Editor:");
                    ui.add(
                        egui::TextEdit::multiline(&mut self.state.script_text)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY)
                            .desired_rows(12),
                    );
                    ui.add_space(10.0);
                });
            });

        // --------------------------------------------------
        // RIGHT COLUMN PANEL: Controls & Realtime log console
        // --------------------------------------------------
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.add_space(8.0);
                ui.heading("🚀 Execution Panel");
                ui.label("Compile processes and stream logs");
                ui.separator();

                // Actions Layout
                ui.horizontal(|ui| {
                    if ui.add_enabled(!self.state.is_rendering, egui::Button::new("🚀 Start Native Render")).clicked() {
                        self.trigger_render();
                    }

                    if ui.add_enabled(self.state.is_rendering, egui::Button::new("Stop")).clicked() {
                        self.request_stop();
                    }

                    if ui.button("💾 Save Settings").clicked() {
                        self.save_current_settings();
                        self.state.status_msg = "Settings Saved 💾".to_string();
                        self.add_log("💾 Saved current configuration state to JSON.".to_string());
                    }

                    ui.add_space(20.0);
                    ui.label("Status:");
                    ui.colored_label(egui::Color32::from_rgb(0, 255, 128), &self.state.status_msg);
                });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);
                ui.label("Realtime Console Logs:");

                // Render Logs Scroller (takes up remaining vertical height)
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            if let Ok(logs) = self.state.logs.lock() {
                                for log in logs.iter() {
                                    ui.label(log);
                                }
                            }
                        });
                    });
            });
        });

        // Keep updating UI while rendering so logs stream smoothly
        if self.state.is_rendering {
            ctx.request_repaint();
        }
    }
}

// --------------------------------------------------
//               APPLICATION MAIN ENTRY
// --------------------------------------------------
fn main() {
    // Setup robust panic logger to write crashes to a file on RDP
    std::panic::set_hook(Box::new(|info| {
        let msg = if let Some(s) = info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown error".to_string()
        };
        let location = info.location().map(|loc| format!("at {}:{}", loc.file(), loc.line())).unwrap_or_default();
        let log_msg = format!("==================================================\n💥 RUST REEL FORGE CRASHED!\n==================================================\nError: {}\nLocation: {}\n\nIf you are on an RDP, it might be due to missing OpenGL hardware acceleration. Try configuring RDP hardware graphics or install standard Windows system fonts.\n", msg, location);
        let _ = std::fs::write("crash_log.txt", log_msg);
    }));

    let args = Args::parse();

    if args.script.is_some() {
        // Run as CLI tool
        run_cli(args);
    } else {
        // Run as pure-Rust native GUI!
        println!("🚀 Launching native Rust GUI...");
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size(egui::vec2(920.0, 700.0)), // Expanded size to fit beautiful two-column layout
            hardware_acceleration: eframe::HardwareAcceleration::Off,
            ..Default::default()
        };
        if let Err(e) = eframe::run_native(
            "Rust Reel Forge",
            options,
            Box::new(|cc| Box::new(ReelForgeApp::new(cc))),
        ) {
            let log_msg = format!("==================================================\n💥 RUST REEL FORGE FAILED TO LAUNCH!\n==================================================\nError: {}\n", e);
            let _ = std::fs::write("crash_log.txt", log_msg);
            println!("{}", e);
        }
    }
}
