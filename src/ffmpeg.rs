use std::path::{Path, PathBuf};
use std::process::Command;
use crate::parser::Script;
use crate::overlay::{WIDTH, HEIGHT};

pub fn render_video(
    script: &Script,
    index: usize,
    videos: &[PathBuf],
    music_files: &[PathBuf],
    output_folder: &Path,
    overlay_paths: &[PathBuf],
    duration: f32,
    stamp: &str,
) -> Result<PathBuf, std::io::Error> {
    let video_path = if !videos.is_empty() {
        Some(&videos[index % videos.len()])
    } else {
        None
    };

    let music_path = if !music_files.is_empty() {
        Some(&music_files[index % music_files.len()])
    } else {
        None
    };

    // Calculate reveal times matching the Python version
    let final_hold = 2.5f32;
    let hold_seconds = final_hold.min((duration * 0.35).max(0.5));
    let reveal_window = (duration - hold_seconds).max(0.8);
    let step = 0.8f32.max(3.0f32.min(reveal_window / (overlay_paths.len() as f32).max(1.0)));
    let last_reveal_start = 0.0f32.max(reveal_window - 0.1);

    // Build FFmpeg command
    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-y"); // Overwrite output files without asking

    // Input 0: Background Video or black canvas
    if let Some(v_path) = video_path {
        cmd.arg("-stream_loop").arg("-1");
        cmd.arg("-i").arg(v_path);
    } else {
        cmd.arg("-f").arg("lavfi");
        cmd.arg("-i").arg(format!("color=c=black:s={}x{}:d={}", WIDTH, HEIGHT, duration));
    }

    // Input 1..N: Overlay PNGs
    for path in overlay_paths {
        cmd.arg("-loop").arg("1");
        cmd.arg("-t").arg(duration.to_string());
        cmd.arg("-i").arg(path);
    }

    // Optional Audio Input
    if let Some(m_path) = music_path {
        cmd.arg("-stream_loop").arg("-1");
        cmd.arg("-i").arg(m_path);
    }

    // Construct complex filter graph
    let mut filters = Vec::new();

    // 1. Process background video: scale, crop to 1080x1920, and darken by 42%
    if video_path.is_some() {
        filters.push(format!(
            "[0:v]scale=w={}:h={}:force_original_aspect_ratio=increase,crop={}:{},drawbox=t=fill:color=black@0.42[bg]",
            WIDTH, HEIGHT, WIDTH, HEIGHT
        ));
    } else {
        filters.push(format!("[0:v]drawbox=t=fill:color=black@0.42[bg]"));
    }

    // 2. Apply fade and overlay for each PNG layer
    let mut current_input_label = "[bg]".to_string();
    for (i, _path) in overlay_paths.iter().enumerate() {
        let overlay_input_index = i + 1; // 1-indexed because input 0 is background
        let raw_start = if i == 0 {
            0.0
        } else {
            last_reveal_start.min(i as f32 * step)
        };
        // Frame safe rounding to 30fps
        let start_time = (raw_start * 30.0).round() / 30.0;

        let faded_label = format!("ovr_faded_{}", i);
        let next_bg_label = format!("bg_next_{}", i);

        // First layer (title) has no fade-in. Other layers have 0.1s soft fade-in.
        if i == 0 {
            filters.push(format!(
                "[{}:v]null[{}]",
                overlay_input_index, faded_label
            ));
        } else {
            filters.push(format!(
                "[{}:v]fade=t=in:st={:.3}:d=0.1:alpha=1[{}]",
                overlay_input_index, start_time, faded_label
            ));
        }

        filters.push(format!(
            "{}[{}]overlay=0:0{}[{}]",
            current_input_label,
            faded_label,
            if i > 0 { format!(":enable='gte(t,{:.3})'", start_time) } else { "".to_string() },
            next_bg_label
        ));
        current_input_label = format!("[{}]", next_bg_label);
    }

    let last_bg_label = if overlay_paths.is_empty() {
        "[bg]".to_string()
    } else {
        current_input_label.clone()
    };
    
    cmd.arg("-filter_complex").arg(filters.join(";"));
    cmd.arg("-map").arg(last_bg_label);

    // Audio mapping
    if music_path.is_some() {
        let audio_input_index = overlay_paths.len() + 1;
        cmd.arg("-map").arg(format!("{}:a", audio_input_index));
        cmd.arg("-c:a").arg("aac");
        cmd.arg("-shortest"); // end when the video duration is reached
    }

    // Output settings
    cmd.arg("-c:v").arg("libx264");
    cmd.arg("-preset").arg("veryfast");
    cmd.arg("-pix_fmt").arg("yuv420p");
    cmd.arg("-movflags").arg("+faststart");
    cmd.arg("-t").arg(duration.to_string());

    std::fs::create_dir_all(output_folder)?;
    let safe_title = crate::parser::slugify(if !script.code.is_empty() { &script.code } else { &script.title });
    let output_path = output_folder.join(format!("{}-{}.mp4", safe_title, stamp));
    cmd.arg(&output_path);

    let output = cmd.output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("FFmpeg failed: {}", stderr),
        ));
    }

    Ok(output_path)
}
