use clap::Parser;
use rand::seq::SliceRandom;
use rand::thread_rng;
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

mod parser;
mod overlay;
mod ffmpeg;

#[derive(Parser, Debug)]
#[command(name = "rust_reel_forge", version = "0.1.0", about = "Stunning 9:16 reels generator written in pure high-performance Rust")]
struct Args {
    #[arg(long, help = "Path to the script file (txt format)")]
    script: PathBuf,

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

fn main() {
    let args = Args::parse();

    println!("--------------------------------------------------");
    println!("        🚀 RUST REEL FORGE — INITIALIZING         ");
    println!("--------------------------------------------------");

    let scripts = match parser::parse_scripts(&args.script) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("❌ Failed to parse script file: {}", e);
            std::process::exit(1);
        }
    };
    println!("📝 Loaded {} script(s) from {:?}", scripts.len(), args.script);

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

    println!("🎨 Loading system font library...");
    let font = overlay::load_system_font();

    let workers = args.workers.max(1).min(scripts.len());
    println!("🧵 Spinning up worker pool ({} parallel workers)...", workers);

    // Build the Rayon thread pool
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
                let duration = 13.0f32.min(12.0f32.max(duration)); // normalized duration limit

                println!("[Worker] Rendering script {}/{}: \"{}\"", index + 1, scripts.len(), script.title);

                // 1. Render all PNG text overlay frames
                let mut overlay_paths = Vec::new();
                
                // Title overlay (layer 0)
                match overlay::make_overlay(script, 0, &args.overlays, &stamp, &font) {
                    Ok(path) => overlay_paths.push(path),
                    Err(e) => return Err(format!("Failed to make title overlay: {}", e)),
                }

                // Point overlays (layers 1..N)
                for point_index in 0..script.points.len() {
                    match overlay::make_overlay(script, point_index + 1, &args.overlays, &stamp, &font) {
                        Ok(path) => overlay_paths.push(path),
                        Err(e) => return Err(format!("Failed to make point overlay {}: {}", point_index + 1, e)),
                    }
                }

                // 2. Composite with background media using FFmpeg
                match ffmpeg::render_video(
                    script,
                    index,
                    &videos,
                    &music_files,
                    &args.output,
                    &overlay_paths,
                    duration,
                    &stamp,
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
