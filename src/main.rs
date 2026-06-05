use clap::Parser;
use rand::seq::SliceRandom;
use rand::thread_rng;
use rayon::prelude::*;
use regex::Regex;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use sysinfo::{get_current_pid, System};
use walkdir::WalkDir;

mod parser;
mod overlay;
mod ffmpeg;

const COMPLETION_LEDGER_PATH: &str = r"C:\Users\Administrator\OneDrive\Videos\fbfINAL\ALL.TXT";

#[derive(Debug, Clone, Copy)]
enum WorkerMode {
    Fixed(usize),
    Auto,
}

#[derive(Debug, Clone, Copy, Default)]
struct AutoWorkerState {
    locked_workers: Option<usize>,
}

#[derive(Debug, Clone, Copy, Default)]
struct ResourceSnapshot {
    peak_cpu_percent: f32,
    peak_memory_percent: f32,
    peak_process_memory_mb: u64,
}

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

    #[arg(long, default_value = "2", help = "Number of parallel workers for rendering, or 'auto'")]
    workers: String,

    #[arg(long, default_value = "none", help = "Background blur strength: none, light, middle, heavy")]
    blur: String,
}

fn list_files_with_extensions(folder: &Path, extensions: &[&str]) -> Vec<PathBuf> {
    if !folder.exists() || !folder.is_dir() {
        return Vec::new();
    }
    let mut files: Vec<PathBuf> = Vec::new();
    for entry in WalkDir::new(folder).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                let ext_lower = ext.to_lowercase();
                if extensions.iter().any(|&e| e == ext_lower) {
                    files.push(path.to_path_buf());
                }
            }
        }
    }
    files.sort();
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

fn parse_worker_mode(value: &str) -> WorkerMode {
    let trimmed = value.trim();
    if trimmed.eq_ignore_ascii_case("auto") {
        WorkerMode::Auto
    } else {
        WorkerMode::Fixed(trimmed.parse::<usize>().unwrap_or(4).max(1))
    }
}

fn auto_worker_limit(script_count: usize) -> usize {
    let available = thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(4)
        .max(2);
    available.min(script_count.max(1))
}

fn spawn_resource_monitor(stop_flag: Arc<AtomicBool>) -> thread::JoinHandle<ResourceSnapshot> {
    thread::spawn(move || {
        let mut system = System::new_all();
        let current_pid = get_current_pid().ok();
        let mut snapshot = ResourceSnapshot::default();

        while !stop_flag.load(Ordering::Relaxed) {
            system.refresh_cpu_usage();
            system.refresh_memory();
            if current_pid.is_some() {
                system.refresh_processes_specifics(sysinfo::ProcessRefreshKind::everything());
            }

            snapshot.peak_cpu_percent = snapshot
                .peak_cpu_percent
                .max(system.global_cpu_info().cpu_usage());

            let total_memory = system.total_memory();
            if total_memory > 0 {
                let used_percent = (system.used_memory() as f32 / total_memory as f32) * 100.0;
                snapshot.peak_memory_percent = snapshot.peak_memory_percent.max(used_percent);
            }

            if let Some(pid) = current_pid {
                if let Some(process) = system.process(pid) {
                    snapshot.peak_process_memory_mb = snapshot
                        .peak_process_memory_mb
                        .max(process.memory() / (1024 * 1024));
                }
            }

            thread::sleep(Duration::from_millis(500));
        }

        snapshot
    })
}

fn collect_script_sources(script_path: &Path) -> Result<Vec<PathBuf>, String> {
    if script_path.is_file() {
        return Ok(vec![script_path.to_path_buf()]);
    }

    if !script_path.is_dir() {
        return Err(format!("Script path does not exist: {}", script_path.display()));
    }

    let mut files = list_files_with_extensions(script_path, &["txt"]);
    files.sort_by(|left, right| natural_path_cmp(left, right));

    if files.is_empty() {
        return Err(format!("No .txt script files found in {}", script_path.display()));
    }

    Ok(files)
}

fn natural_path_cmp(left: &Path, right: &Path) -> std::cmp::Ordering {
    let left_parts: Vec<String> = left
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect();
    let right_parts: Vec<String> = right
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect();

    for (left_part, right_part) in left_parts.iter().zip(right_parts.iter()) {
        let part_cmp = natural_str_cmp(left_part, right_part);
        if part_cmp != std::cmp::Ordering::Equal {
            return part_cmp;
        }
    }

    left_parts.len().cmp(&right_parts.len())
}

fn natural_str_cmp(left: &str, right: &str) -> std::cmp::Ordering {
    let splitter = Regex::new(r"\d+|\D+").unwrap();
    let left_chunks: Vec<&str> = splitter.find_iter(left).map(|m| m.as_str()).collect();
    let right_chunks: Vec<&str> = splitter.find_iter(right).map(|m| m.as_str()).collect();

    for (left_chunk, right_chunk) in left_chunks.iter().zip(right_chunks.iter()) {
        let chunk_cmp = match (left_chunk.parse::<u64>(), right_chunk.parse::<u64>()) {
            (Ok(left_num), Ok(right_num)) => left_num.cmp(&right_num),
            _ => left_chunk.to_lowercase().cmp(&right_chunk.to_lowercase()),
        };
        if chunk_cmp != std::cmp::Ordering::Equal {
            return chunk_cmp;
        }
    }

    left_chunks.len().cmp(&right_chunks.len())
}

fn append_completion_ledger(script_file: &Path) -> Result<(), String> {
    let stem = script_file
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("scripts");
    let week_name = script_file
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("root");

    let ledger_path = Path::new(COMPLETION_LEDGER_PATH);
    if let Some(parent) = ledger_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            format!(
                "Failed to create completion ledger folder {}: {}",
                parent.display(),
                e
            )
        })?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(ledger_path)
        .map_err(|e| format!("Failed to open completion ledger {}: {}", ledger_path.display(), e))?;

    writeln!(file, "{}:{}", stem, week_name)
        .map_err(|e| format!("Failed to append completion ledger {}: {}", ledger_path.display(), e))?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn render_script_chunk<F>(
    scripts: &[parser::Script],
    script_label: &str,
    total_scripts: usize,
    global_start_index: usize,
    video_root: &Path,
    music_root: &Path,
    output_root: &Path,
    overlay_root: &Path,
    default_duration: f32,
    workers: usize,
    blur_strength: parser::BlurStrength,
    stop_requested: Option<&AtomicBool>,
    logger: &F,
) -> Result<Vec<Result<PathBuf, String>>, String>
where
    F: Fn(&str) + Sync,
{
    let video_extensions = ["mp4", "mov", "mkv", "webm"];
    let audio_extensions = ["mp3", "wav", "m4a", "aac"];

    let mut videos = list_files_with_extensions(video_root, &video_extensions);
    let mut music_files = list_files_with_extensions(music_root, &audio_extensions);

    if videos.is_empty() {
        logger("No background videos found; rendering on solid black background.");
    } else {
        logger(&format!("Found {} background video(s)", videos.len()));
        let mut rng = thread_rng();
        videos.shuffle(&mut rng);
    }

    if music_files.is_empty() {
        logger("No music tracks found; rendering silent videos.");
    } else {
        logger(&format!("Found {} music track(s)", music_files.len()));
        let mut rng = thread_rng();
        music_files.shuffle(&mut rng);
    }

    logger("Loading system font library...");
    let font = overlay::load_system_font();
    logger(&format!("Spinning up worker pool ({} parallel workers)...", workers));

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers.max(1))
        .build()
        .map_err(|e| format!("Failed to initialize thread pool: {}", e))?;

    Ok(pool.install(|| {
        scripts
            .par_iter()
            .enumerate()
            .map(|(index, script)| {
                if let Some(flag) = stop_requested {
                    if flag.load(Ordering::Relaxed) {
                        return Err("Render stopped by user".to_string());
                    }
                }

                let stamp = get_millisecond_stamp();
                let duration = script.duration.unwrap_or(default_duration).max(1.0);
                let script_number = global_start_index + index + 1;

                logger(&format!(
                    "[Worker] Rendering script {}/{} from {}: \"{}\"",
                    script_number,
                    total_scripts,
                    script_label,
                    script.title
                ));

                let mut overlay_paths = Vec::new();
                match overlay::make_overlay(script, 0, overlay_root, &stamp, &font) {
                    Ok(path) => overlay_paths.push(path),
                    Err(e) => return Err(format!("Failed to make title overlay: {}", e)),
                }

                for point_index in 0..script.points.len() {
                    if let Some(flag) = stop_requested {
                        if flag.load(Ordering::Relaxed) {
                            return Err("Render stopped by user".to_string());
                        }
                    }
                    match overlay::make_overlay(script, point_index + 1, overlay_root, &stamp, &font) {
                        Ok(path) => overlay_paths.push(path),
                        Err(e) => return Err(format!("Failed to make point overlay {}: {}", point_index + 1, e)),
                    }
                }

                if !script.cta.is_empty() {
                    match overlay::make_overlay(script, script.points.len() + 1, overlay_root, &stamp, &font) {
                        Ok(path) => overlay_paths.push(path),
                        Err(e) => return Err(format!("Failed to make CTA overlay: {}", e)),
                    }
                }

                match ffmpeg::render_video(
                    script,
                    global_start_index + index,
                    &videos,
                    &music_files,
                    output_root,
                    &overlay_paths,
                    duration,
                    blur_strength,
                    &stamp,
                    stop_requested,
                ) {
                    Ok(out_path) => Ok(out_path),
                    Err(e) => Err(format!("FFmpeg composition failed: {}", e)),
                }
            })
            .collect()
    }))
}

fn auto_chunk_size(workers: usize, remaining: usize, tuning: bool) -> usize {
    let target = if tuning {
        (workers * 2).max(2)
    } else {
        (workers * 4).max(4)
    };
    target.min(remaining.max(1))
}

fn auto_should_back_off(snapshot: ResourceSnapshot) -> bool {
    snapshot.peak_cpu_percent >= 92.0 || snapshot.peak_memory_percent >= 88.0
}

#[allow(clippy::too_many_arguments)]
fn render_loaded_scripts<F>(
    scripts: &[parser::Script],
    script_label: &str,
    video_root: &Path,
    music_root: &Path,
    output_root: &Path,
    overlay_root: &Path,
    default_duration: f32,
    worker_mode: WorkerMode,
    mut auto_worker_state: Option<&mut AutoWorkerState>,
    blur_strength: parser::BlurStrength,
    stop_requested: Option<&AtomicBool>,
    logger: &F,
) -> Result<(usize, usize), String>
where
    F: Fn(&str) + Sync,
{
    if scripts.is_empty() {
        return Ok((0, 0));
    }

    let mut success_count = 0;
    let start_time = Instant::now();

    match worker_mode {
        WorkerMode::Fixed(requested_workers) => {
            logger(&format!("Blur mode: {}", blur_strength.as_str()));
            let workers = requested_workers.max(1).min(scripts.len());
            let outputs = render_script_chunk(
                scripts,
                script_label,
                scripts.len(),
                0,
                video_root,
                music_root,
                output_root,
                overlay_root,
                default_duration,
                workers,
                blur_strength,
                stop_requested,
                logger,
            )?;
            for (i, res) in outputs.iter().enumerate() {
                match res {
                    Ok(path) => {
                        success_count += 1;
                        logger(&format!("  Reel {}: {:?}", i + 1, path.file_name().unwrap()));
                    }
                    Err(e) => {
                        logger(&format!("  Reel {}: Error -> {}", i + 1, e));
                    }
                }
            }
        }
        WorkerMode::Auto => {
            logger(&format!("Blur mode: {} (auto workers)", blur_strength.as_str()));
            let max_workers = auto_worker_limit(scripts.len());
            let locked_start = auto_worker_state
                .as_ref()
                .and_then(|state| state.locked_workers)
                .map(|workers| workers.min(max_workers).max(1));
            let mut next_worker = locked_start.unwrap_or(2usize.min(max_workers).max(1));
            let mut chosen_workers: Option<usize> = locked_start;
            let mut best_workers = next_worker;
            let mut best_score = if locked_start.is_some() { 1.0 } else { 0.0 };
            let mut next_index = 0usize;

            if let Some(locked) = locked_start {
                logger(&format!(
                    "Auto thread locked from previous file: {} workers will be reused for this script file.",
                    locked
                ));
            }

            while next_index < scripts.len() {
                if let Some(flag) = stop_requested {
                    if flag.load(Ordering::Relaxed) {
                        break;
                    }
                }

                let tuning = chosen_workers.is_none();
                let workers = chosen_workers.unwrap_or(next_worker).min(scripts.len() - next_index).max(1);
                let chunk_size = auto_chunk_size(workers, scripts.len() - next_index, tuning);
                logger(&format!(
                    "Auto thread pass: rendering reels {}-{} with {} workers...",
                    next_index + 1,
                    next_index + chunk_size,
                    workers
                ));

                let monitor_stop = Arc::new(AtomicBool::new(false));
                let monitor_handle = spawn_resource_monitor(monitor_stop.clone());
                let chunk_start = Instant::now();
                let outputs = render_script_chunk(
                    &scripts[next_index..next_index + chunk_size],
                    script_label,
                    scripts.len(),
                    next_index,
                    video_root,
                    music_root,
                    output_root,
                    overlay_root,
                    default_duration,
                    workers,
                    blur_strength,
                    stop_requested,
                    logger,
                )?;
                monitor_stop.store(true, Ordering::Relaxed);
                let snapshot = monitor_handle.join().unwrap_or_default();
                let elapsed = chunk_start.elapsed();
                let score = chunk_size as f64 / elapsed.as_secs_f64().max(0.1);

                for (offset, res) in outputs.iter().enumerate() {
                    match res {
                        Ok(path) => {
                            success_count += 1;
                            logger(&format!(
                                "  Reel {}: {:?}",
                                next_index + offset + 1,
                                path.file_name().unwrap()
                            ));
                        }
                        Err(e) => {
                            logger(&format!("  Reel {}: Error -> {}", next_index + offset + 1, e));
                        }
                    }
                }

                logger(&format!(
                    "Auto thread stats: {:.2} reels/sec, CPU peak {:.0}%, memory peak {:.0}%, app peak {} MB",
                    score,
                    snapshot.peak_cpu_percent,
                    snapshot.peak_memory_percent,
                    snapshot.peak_process_memory_mb
                ));

                if tuning {
                    let overloaded = auto_should_back_off(snapshot);
                    let improved = best_score == 0.0 || score > best_score * 1.05;
                    if improved && !overloaded {
                        best_score = score;
                        best_workers = workers;
                    }

                    if overloaded {
                        chosen_workers = Some(best_workers.max(1));
                        if let Some(state) = auto_worker_state.as_deref_mut() {
                            state.locked_workers = chosen_workers;
                        }
                        logger(&format!(
                            "Auto thread settled on {} workers after hitting system pressure.",
                            chosen_workers.unwrap()
                        ));
                    } else if workers >= max_workers || next_index + chunk_size >= scripts.len() {
                        chosen_workers = Some(best_workers.max(1));
                        if let Some(state) = auto_worker_state.as_deref_mut() {
                            state.locked_workers = chosen_workers;
                        }
                        logger(&format!(
                            "Auto thread settled on {} workers after finishing the ramp-up scan.",
                            chosen_workers.unwrap()
                        ));
                    } else if !improved && workers > 2 {
                        chosen_workers = Some(best_workers.max(1));
                        if let Some(state) = auto_worker_state.as_deref_mut() {
                            state.locked_workers = chosen_workers;
                        }
                        logger(&format!(
                            "Auto thread settled on {} workers after throughput stopped improving.",
                            chosen_workers.unwrap()
                        ));
                    } else {
                        next_worker = (workers + 2).min(max_workers);
                        logger(&format!("Auto thread increasing to {} workers for the next pass.", next_worker));
                    }
                }

                next_index += chunk_size;
            }
        }
    }

    let elapsed = start_time.elapsed();
    logger("--------------------------------------------------");
    logger("RENDERING DONE");
    logger("--------------------------------------------------");
    logger(&format!("Total elapsed time: {:.2?}", elapsed));
    logger(&format!("Rendered {}/{} videos successfully.", success_count, scripts.len()));

    Ok((success_count, scripts.len()))
}

#[allow(clippy::too_many_arguments)]
fn render_scripts_from_file(
    script_path: &Path,
    video_root: &Path,
    music_root: &Path,
    output_root: &Path,
    overlay_root: &Path,
    default_duration: f32,
    worker_mode: WorkerMode,
    auto_worker_state: Option<&mut AutoWorkerState>,
    blur_strength: parser::BlurStrength,
) -> Result<(usize, usize), String> {
    let scripts = parser::parse_scripts(script_path)
        .map_err(|e| format!("Failed to parse script file {}: {}", script_path.display(), e))?;
    println!("Loaded {} script(s) from {}", scripts.len(), script_path.display());

    render_loaded_scripts(
        &scripts,
        &script_path.display().to_string(),
        video_root,
        music_root,
        output_root,
        overlay_root,
        default_duration,
        worker_mode,
        auto_worker_state,
        blur_strength,
        None,
        &|message| println!("{}", message),
    )
}

// --------------------------------------------------
//               CLI MODE EXECUTION
// --------------------------------------------------
#[allow(dead_code)]
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

    let workers = match parse_worker_mode(&args.workers) {
        WorkerMode::Fixed(count) => count,
        WorkerMode::Auto => auto_worker_limit(scripts.len()),
    }
    .max(1)
    .min(scripts.len());
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
                let duration = script.duration.unwrap_or(args.duration).max(1.0);

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

                if !script.cta.is_empty() {
                    match overlay::make_overlay(script, script.points.len() + 1, &args.overlays, &stamp, &font) {
                        Ok(path) => overlay_paths.push(path),
                        Err(e) => return Err(format!("Failed to make CTA overlay: {}", e)),
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
    #[serde(default)]
    script_source: String,
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
    script_source: String,
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
                script_source: saved.script_source,
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
                script_source: "".to_string(),
                duration: "12.5".to_string(),
                workers: "auto".to_string(),
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
            script_source: self.state.script_source.clone(),
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
        let worker_mode = parse_worker_mode(&self.state.workers);
        let blur_strength = parser::BlurStrength::from_str(&self.state.blur_strength);
        let script_source = self.state.script_source.trim().to_string();
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

            if !script_source.is_empty() {
                let script_path = PathBuf::from(&script_source);
                let script_sources = match collect_script_sources(&script_path) {
                    Ok(files) => files,
                    Err(e) => {
                        log(&format!("❌ Script source error: {}", e));
                        let _ = tx.send("__FINISHED__".to_string());
                        return;
                    }
                };

                let batch_mode = script_path.is_dir();
                let script_file_count = script_sources.len();
                let mut grand_total_success = 0usize;
                let mut grand_total_scripts = 0usize;
                let mut auto_worker_state = AutoWorkerState::default();

                for (file_index, script_file) in script_sources.iter().enumerate() {
                    if stop_requested.load(Ordering::Relaxed) {
                        log("Render stopped by user.");
                        break;
                    }

                    let stem = script_file
                        .file_stem()
                        .and_then(|name| name.to_str())
                        .unwrap_or("scripts");
                    let output_root = if batch_mode {
                        output_dir.join(parser::slugify(stem))
                    } else {
                        output_dir.clone()
                    };
                    let overlay_root = if batch_mode {
                        overlay_dir.join(parser::slugify(stem))
                    } else {
                        overlay_dir.clone()
                    };

                    if batch_mode {
                        log(&format!("Batch file {}/{}", file_index + 1, script_file_count));
                    }
                    log(&format!("Script source: {}", script_file.display()));
                    log(&format!("Video output: {}", output_root.display()));

                    match render_scripts_from_file(
                        script_file,
                        &video_dir,
                        &music_dir,
                        &output_root,
                        &overlay_root,
                        duration,
                        worker_mode,
                        Some(&mut auto_worker_state),
                        blur_strength,
                    ) {
                        Ok((success_count, total_count)) => {
                            grand_total_success += success_count;
                            grand_total_scripts += total_count;
                            if success_count == total_count && total_count > 0 {
                                match append_completion_ledger(script_file) {
                                    Ok(()) => log(&format!(
                                        "Completion ledger updated: {}:{}",
                                        script_file.file_stem().and_then(|name| name.to_str()).unwrap_or("scripts"),
                                        script_file.parent().and_then(|parent| parent.file_name()).and_then(|name| name.to_str()).unwrap_or("root")
                                    )),
                                    Err(e) => log(&format!("⚠️ {}", e)),
                                }
                            }
                            if batch_mode {
                                log(&format!(
                                    "Completed batch file {}/{} -> {} ({} / {} reels succeeded in this file)",
                                    file_index + 1,
                                    script_file_count,
                                    script_file.display(),
                                    success_count,
                                    total_count
                                ));
                            }
                        }
                        Err(e) => {
                            log(&format!("❌ {}", e));
                        }
                    }
                }

                if batch_mode {
                    log("==================================================");
                    log(&format!(
                        "Batch finished: rendered {}/{} reels across {} script file(s).",
                        grand_total_success,
                        grand_total_scripts,
                        script_file_count
                    ));
                }

                let _ = tx.send("__FINISHED__".to_string());
                return;
            }

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
                        let d = script.duration.unwrap_or(duration).max(1.0);

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

                        if !script.cta.is_empty() {
                            match overlay::make_overlay(script, script.points.len() + 1, &overlay_dir, &stamp, &font) {
                                Ok(path) => overlay_paths.push(path),
                                Err(e) => return Err(format!("Overlay CTA failed: {}", e)),
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
                            ui.label("Parallel workers / auto");
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
                    ui.label("Script Source:");
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut self.state.script_source);
                        if ui.button("File...").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("Text scripts", &["txt"])
                                .pick_file()
                            {
                                self.state.script_source = path.to_string_lossy().to_string();
                            }
                        }
                        if ui.button("Folder...").clicked() {
                            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                self.state.script_source = path.to_string_lossy().to_string();
                            }
                        }
                        if ui.button("Editor").clicked() {
                            self.state.script_source.clear();
                        }
                    });
                    ui.add_space(8.0);
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

    if let Some(script_path) = args.script.as_ref() {
        // Run as CLI tool
        let blur_strength = parser::BlurStrength::from_str(&args.blur);
        let worker_mode = parse_worker_mode(&args.workers);
        println!("--------------------------------------------------");
        println!("        RUST REEL FORGE - INITIALIZING         ");
        println!("--------------------------------------------------");

        let script_sources = match collect_script_sources(script_path) {
            Ok(files) => files,
            Err(e) => {
                eprintln!("Failed to load script source: {}", e);
                std::process::exit(1);
            }
        };

        let batch_mode = script_path.is_dir();
        let script_file_count = script_sources.len();
        let mut grand_total_success = 0usize;
        let mut grand_total_scripts = 0usize;
        let mut auto_worker_state = AutoWorkerState::default();

        for (file_index, script_file) in script_sources.iter().enumerate() {
            let stem = script_file
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("scripts");
            let output_root = if batch_mode {
                args.output.join(parser::slugify(stem))
            } else {
                args.output.clone()
            };
            let overlay_root = if batch_mode {
                args.overlays.join(parser::slugify(stem))
            } else {
                args.overlays.clone()
            };

            if batch_mode {
                println!(
                    "=================================================="
                );
                println!(
                    "Batch file {}/{}",
                    file_index + 1,
                    script_file_count
                );
            }
            println!("Script source: {}", script_file.display());
            println!("Video output: {}", output_root.display());

            match render_scripts_from_file(
                script_file,
                &args.videos,
                &args.music,
                &output_root,
                &overlay_root,
                args.duration,
                worker_mode,
                Some(&mut auto_worker_state),
                blur_strength,
            ) {
                Ok((success_count, total_count)) => {
                    grand_total_success += success_count;
                    grand_total_scripts += total_count;
                    if success_count == total_count && total_count > 0 {
                        if let Err(e) = append_completion_ledger(script_file) {
                            eprintln!("{}", e);
                        } else {
                            println!(
                                "Completion ledger updated: {}:{}",
                                script_file.file_stem().and_then(|name| name.to_str()).unwrap_or("scripts"),
                                script_file.parent().and_then(|parent| parent.file_name()).and_then(|name| name.to_str()).unwrap_or("root")
                            );
                        }
                    }
                    if batch_mode {
                        println!(
                            "Completed batch file {}/{} -> {} ({} / {} reels succeeded in this file)",
                            file_index + 1,
                            script_file_count,
                            script_file.display(),
                            success_count,
                            total_count
                        );
                    }
                }
                Err(e) => {
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
            }
        }

        if batch_mode {
            println!("==================================================");
            println!(
                "Batch finished: rendered {}/{} reels across {} script file(s).",
                grand_total_success,
                grand_total_scripts,
                script_file_count
            );
        }
    } else {
        // Run as pure-Rust native GUI!
        println!("🚀 Launching native Rust GUI...");
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size(egui::vec2(920.0, 700.0)), // Expanded size to fit beautiful two-column layout
            renderer: eframe::Renderer::Glow,
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
