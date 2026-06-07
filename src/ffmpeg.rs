use crate::overlay::{HEIGHT, WIDTH};
use crate::parser::{BlurStrength, Script};
use rand::Rng;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy)]
enum BackgroundVariant {
    Normal,
    TintOnly,
    GradientTint,
    CardOverlay,
}

#[derive(Clone, Copy)]
struct BlurBand {
    sigma_min: u32,
    sigma_max: u32,
    tint_min: u32,
    tint_max: u32,
}

#[derive(Clone, Copy)]
struct BackgroundTreatment {
    variant: BackgroundVariant,
    blur_sigma: Option<u32>,
    tint_opacity: f32,
    secondary_tint_opacity: f32,
    card_opacity: f32,
}

fn light_band() -> BlurBand {
    BlurBand {
        sigma_min: 3,
        sigma_max: 5,
        tint_min: 18,
        tint_max: 25,
    }
}

fn middle_band() -> BlurBand {
    BlurBand {
        sigma_min: 6,
        sigma_max: 8,
        tint_min: 26,
        tint_max: 34,
    }
}

fn heavy_band() -> BlurBand {
    BlurBand {
        sigma_min: 9,
        sigma_max: 12,
        tint_min: 35,
        tint_max: 42,
    }
}

fn random_band<R: Rng>(rng: &mut R) -> BlurBand {
    let roll = rng.gen_range(0..100);
    if roll < 50 {
        light_band()
    } else if roll < 80 {
        middle_band()
    } else {
        heavy_band()
    }
}

fn opacity_from_percent(percent: u32) -> f32 {
    percent as f32 / 100.0
}

fn random_opacity_in_band<R: Rng>(rng: &mut R, band: BlurBand) -> f32 {
    opacity_from_percent(rng.gen_range(band.tint_min..=band.tint_max))
}

fn random_sigma_in_band<R: Rng>(rng: &mut R, band: BlurBand) -> u32 {
    rng.gen_range(band.sigma_min..=band.sigma_max)
}

fn blur_filter_for_sigma(sigma: u32) -> String {
    let steps = if sigma <= 5 {
        1
    } else if sigma <= 8 {
        2
    } else {
        3
    };
    format!("gblur=sigma={}:steps={}", sigma, steps)
}

fn choose_background_treatment(blur_strength: BlurStrength) -> BackgroundTreatment {
    let mut rng = rand::thread_rng();

    match blur_strength {
        BlurStrength::None => {
            let variant_roll = rng.gen_range(0..100);
            if variant_roll < 10 {
                BackgroundTreatment {
                    variant: BackgroundVariant::TintOnly,
                    blur_sigma: None,
                    tint_opacity: opacity_from_percent(rng.gen_range(18..=42)),
                    secondary_tint_opacity: 0.0,
                    card_opacity: 0.0,
                }
            } else {
                let band = random_band(&mut rng);
                let blur_sigma = Some(random_sigma_in_band(&mut rng, band));
                let tint_opacity = random_opacity_in_band(&mut rng, band);

                if variant_roll < 20 {
                    let secondary = (tint_opacity + 0.09).min(0.48);
                    BackgroundTreatment {
                        variant: BackgroundVariant::GradientTint,
                        blur_sigma,
                        tint_opacity: (tint_opacity - 0.06).max(0.12),
                        secondary_tint_opacity: secondary,
                        card_opacity: 0.0,
                    }
                } else if variant_roll < 30 {
                    BackgroundTreatment {
                        variant: BackgroundVariant::CardOverlay,
                        blur_sigma,
                        tint_opacity: (tint_opacity - 0.10).max(0.10),
                        secondary_tint_opacity: 0.0,
                        card_opacity: rng.gen_range(12..=22) as f32 / 100.0,
                    }
                } else {
                    BackgroundTreatment {
                        variant: BackgroundVariant::Normal,
                        blur_sigma,
                        tint_opacity,
                        secondary_tint_opacity: 0.0,
                        card_opacity: 0.0,
                    }
                }
            }
        }
        BlurStrength::Light | BlurStrength::Middle | BlurStrength::Heavy => {
            let band = match blur_strength {
                BlurStrength::Light => light_band(),
                BlurStrength::Middle => middle_band(),
                BlurStrength::Heavy => heavy_band(),
                BlurStrength::None => unreachable!(),
            };
            let variant_roll = rng.gen_range(0..100);
            let blur_sigma = Some(random_sigma_in_band(&mut rng, band));
            let tint_opacity = random_opacity_in_band(&mut rng, band);

            if variant_roll < 12 {
                BackgroundTreatment {
                    variant: BackgroundVariant::GradientTint,
                    blur_sigma,
                    tint_opacity: (tint_opacity - 0.05).max(0.12),
                    secondary_tint_opacity: (tint_opacity + 0.07).min(0.48),
                    card_opacity: 0.0,
                }
            } else if variant_roll < 24 {
                BackgroundTreatment {
                    variant: BackgroundVariant::CardOverlay,
                    blur_sigma,
                    tint_opacity: (tint_opacity - 0.08).max(0.10),
                    secondary_tint_opacity: 0.0,
                    card_opacity: rng.gen_range(12..=22) as f32 / 100.0,
                }
            } else {
                BackgroundTreatment {
                    variant: BackgroundVariant::Normal,
                    blur_sigma,
                    tint_opacity,
                    secondary_tint_opacity: 0.0,
                    card_opacity: 0.0,
                }
            }
        }
    }
}

fn apply_background_treatment(base_chain: &mut String, treatment: BackgroundTreatment) {
    if let Some(sigma) = treatment.blur_sigma {
        base_chain.push(',');
        base_chain.push_str(&blur_filter_for_sigma(sigma));
    }

    match treatment.variant {
        BackgroundVariant::Normal | BackgroundVariant::TintOnly => {
            base_chain.push_str(&format!(
                ",drawbox=t=fill:color=black@{:.2}",
                treatment.tint_opacity
            ));
        }
        BackgroundVariant::GradientTint => {
            let split_y = HEIGHT * 11 / 20;
            let bottom_height = HEIGHT - split_y;
            base_chain.push_str(&format!(
                ",drawbox=x=0:y=0:w={}:h={}:t=fill:color=black@{:.2}",
                WIDTH, split_y, treatment.tint_opacity
            ));
            base_chain.push_str(&format!(
                ",drawbox=x=0:y={}:w={}:h={}:t=fill:color=black@{:.2}",
                split_y, WIDTH, bottom_height, treatment.secondary_tint_opacity
            ));
        }
        BackgroundVariant::CardOverlay => {
            base_chain.push_str(&format!(
                ",drawbox=t=fill:color=black@{:.2}",
                treatment.tint_opacity
            ));
            base_chain.push_str(&format!(
                ",drawbox=x=70:y=240:w={}:h={}:t=fill:color=black@{:.2}",
                WIDTH - 140,
                HEIGHT - 480,
                treatment.card_opacity
            ));
        }
    }
}

fn build_reveal_starts(script: &Script, overlay_count: usize) -> Vec<f32> {
    let step = 2.5f32;
    let mut reveal_starts = Vec::with_capacity(overlay_count);

    if script.all_at_once {
        reveal_starts.resize(overlay_count, 0.0);
        return reveal_starts;
    }

    let mut current_start = 0.0f32;
    for overlay_index in 0..overlay_count {
        reveal_starts.push(current_start);
        if overlay_index == 0 {
            current_start += step;
            continue;
        }

        let point_index = overlay_index - 1;
        if point_index < script.points.len() {
            let pause_count = script
                .point_pause_counts_before
                .get(point_index)
                .copied()
                .unwrap_or(0);
            current_start += step * (1.0 + pause_count as f32);
        } else {
            current_start += step * (1.0 + script.cta_pause_count_before as f32);
        }
    }

    reveal_starts
}

fn probe_media_duration(path: &Path) -> Option<f32> {
    let output = Command::new("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-show_entries")
        .arg("format=duration")
        .arg("-of")
        .arg("default=noprint_wrappers=1:nokey=1")
        .arg(path)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|value| *value > 0.0)
}

#[allow(clippy::too_many_arguments)]
pub fn render_video(
    script: &Script,
    index: usize,
    videos: &[PathBuf],
    music_files: &[PathBuf],
    output_folder: &Path,
    overlay_paths: &[PathBuf],
    duration: f32,
    ffmpeg_threads: usize,
    blur_strength: BlurStrength,
    _stamp: &str,
    stop_requested: Option<&AtomicBool>,
) -> Result<PathBuf, std::io::Error> {
    if let Some(flag) = stop_requested {
        if flag.load(Ordering::Relaxed) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "Render stopped by user",
            ));
        }
    }

    let video_path = if let Some(script_video) = script.video.as_ref() {
        Some(script_video)
    } else if !videos.is_empty() {
        Some(&videos[index % videos.len()])
    } else {
        None
    };

    let music_path = if let Some(script_audio) = script.audio.as_ref() {
        Some(script_audio)
    } else if !music_files.is_empty() {
        Some(&music_files[index % music_files.len()])
    } else {
        None
    };

    let reveal_starts = build_reveal_starts(script, overlay_paths.len());
    let last_reveal_start = reveal_starts.last().copied().unwrap_or(0.0);
    let effective_duration = duration.max(last_reveal_start + 2.0);
    let background_treatment = choose_background_treatment(blur_strength);

    // Build FFmpeg command
    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-y"); // Overwrite output files without asking

    // Input 0: Background Video or black canvas
    if let Some(v_path) = video_path {
        cmd.arg("-stream_loop").arg("-1");
        cmd.arg("-i").arg(v_path);
    } else {
        cmd.arg("-f").arg("lavfi");
        cmd.arg("-i").arg(format!(
            "color=c=black:s={}x{}:d={}",
            WIDTH, HEIGHT, effective_duration
        ));
    }

    // Input 1..N: Overlay PNGs
    for path in overlay_paths {
        cmd.arg("-loop").arg("1");
        cmd.arg("-t").arg(effective_duration.to_string());
        cmd.arg("-i").arg(path);
    }

    // Optional Audio Input
    if let Some(m_path) = music_path {
        if let Some(track_duration) = probe_media_duration(m_path) {
            let required_audio_tail = effective_duration.max(15.0);
            let max_start = (track_duration - required_audio_tail).max(0.0);
            if max_start > 0.0 {
                let audio_start = rand::thread_rng().gen_range(0.0..=max_start);
                cmd.arg("-ss").arg(format!("{:.3}", audio_start));
            }
        }
        cmd.arg("-stream_loop").arg("-1");
        cmd.arg("-i").arg(m_path);
    }

    // Construct complex filter graph
    let mut filters = Vec::new();

    // 1. Process background video with per-reel blur and tint variation.
    if video_path.is_some() {
        let mut bg_chain = format!(
            "[0:v]scale=w={}:h={}:force_original_aspect_ratio=increase,crop={}:{}",
            WIDTH, HEIGHT, WIDTH, HEIGHT
        );
        apply_background_treatment(&mut bg_chain, background_treatment);
        bg_chain.push_str("[bg]");
        filters.push(bg_chain);
    } else {
        let mut bg_chain = "[0:v]null".to_string();
        apply_background_treatment(&mut bg_chain, background_treatment);
        bg_chain.push_str("[bg]");
        filters.push(bg_chain);
    }

    // 2. Apply fade and overlay for each PNG layer
    let mut current_input_label = "[bg]".to_string();

    for (i, _path) in overlay_paths.iter().enumerate() {
        let overlay_input_index = i + 1; // 1-indexed because input 0 is background
        let raw_start = reveal_starts.get(i).copied().unwrap_or(0.0);
        // Frame safe rounding to 30fps
        let start_time = (raw_start * 30.0).round() / 30.0;

        let faded_label = format!("ovr_faded_{}", i);
        let next_bg_label = format!("bg_next_{}", i);

        // First layer (title) has no fade-in. Other layers have 0.1s soft fade-in.
        if i == 0 || script.all_at_once {
            filters.push(format!("[{}:v]null[{}]", overlay_input_index, faded_label));
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
            if i > 0 && !script.all_at_once {
                format!(":enable='gte(t,{:.3})'", start_time)
            } else {
                "".to_string()
            },
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
    cmd.arg("-threads").arg(ffmpeg_threads.max(1).to_string());
    cmd.arg("-c:v").arg("libx264");
    cmd.arg("-preset").arg("veryfast");
    cmd.arg("-pix_fmt").arg("yuv420p");
    cmd.arg("-movflags").arg("+faststart");
    cmd.arg("-t").arg(effective_duration.to_string());

    std::fs::create_dir_all(output_folder)?;
    let safe_title = crate::parser::slugify(if !script.code.is_empty() {
        &script.code
    } else {
        &script.title
    });
    let output_path = output_folder.join(format!("{}.mp4", safe_title));
    cmd.arg(&output_path);

    let output = cmd.output()?;
    if let Some(flag) = stop_requested {
        if flag.load(Ordering::Relaxed) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "Render stopped by user",
            ));
        }
    }
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(std::io::Error::other(format!("FFmpeg failed: {}", stderr)));
    }

    Ok(output_path)
}
