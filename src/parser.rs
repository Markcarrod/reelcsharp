use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Script {
    pub title: String,
    pub points: Vec<String>,
    pub point_pause_counts_before: Vec<u32>,
    pub cta_pause_count_before: u32,
    pub cta: String,
    pub code: String,
    pub duration: Option<f32>,
    pub layout: String,
    pub all_at_once: bool,
    pub video: Option<PathBuf>,
    pub audio: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlurStrength {
    None,
    Light,
    Middle,
    Heavy,
}

impl BlurStrength {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Light => "light",
            Self::Middle => "middle",
            Self::Heavy => "heavy",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value.trim().to_lowercase().as_str() {
            "light" => Self::Light,
            "middle" | "medium" => Self::Middle,
            "heavy" => Self::Heavy,
            _ => Self::None,
        }
    }

}

pub fn slugify(value: &str) -> String {
    let re = Regex::new(r"[^a-zA-Z0-9]+").unwrap();
    let slug = re.replace_all(value, "-").to_lowercase();
    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        "reel".to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn normalize_layout(value: &str) -> String {
    let re = Regex::new(r"[^a-z0-9]+").unwrap();
    let val_lower = value.to_lowercase();
    let normalized = re.replace_all(&val_lower, "_");
    let normalized = normalized.trim_matches('_');

    match normalized {
        "question_answer" | "question" | "answer" => "question_answer".to_string(),
        "advice" | "list" | "list_style" => "list_style".to_string(),
        "reels" | "center" | "center_stack" => "center_stack".to_string(),
        "left" | "left_stack" => "left_stack".to_string(),
        "right" | "right_stack" => "right_stack".to_string(),
        "top_bottom" => "top_bottom".to_string(),
        "one_word" | "one_word_hook" => "one_word_hook".to_string(),
        "quote" | "quote_style" => "quote_style".to_string(),
        "story" | "story_block" => "story_block".to_string(),
        "full_text" | "full_list" | "all_at_once" | "static_text" => "story_block".to_string(),
        "progress" | "progress_reveal" => "progress_reveal".to_string(),
        "card" | "center_card" => "center_card".to_string(),
        "two_column_split" => "two_column_split".to_string(),
        "grid_layout" => "grid_layout".to_string(),
        "masonry_layout" => "masonry_layout".to_string(),
        "hero_list" => "hero_list".to_string(),
        "alternating_rows" => "alternating_rows".to_string(),
        "sidebar_layout" => "sidebar_layout".to_string(),
        "collage_layout" => "collage_layout".to_string(),
        "auto_fit_tiles" => "auto_fit_tiles".to_string(),
        "tabbed_layout" => "tabbed_layout".to_string(),
        "magazine_layout" => "magazine_layout".to_string(),
        "template_rotation_layout" => "template_rotation_layout".to_string(),
        "priority_based_layout" => "priority_based_layout".to_string(),
        "adaptive_smart_layout" => "adaptive_smart_layout".to_string(),
        "fallback_universal_layout" => "fallback_universal_layout".to_string(),
        _ => "center_stack".to_string(),
    }
}

fn loose_text_key(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn collapse_duplicate_title_point(script: &Script) -> Script {
    let Some(first_point) = script.points.first() else {
        return script.clone();
    };

    if loose_text_key(&script.title) != loose_text_key(first_point) {
        return script.clone();
    }

    let mut normalized = script.clone();
    normalized.points.remove(0);
    if !normalized.point_pause_counts_before.is_empty() {
        normalized.point_pause_counts_before.remove(0);
    }
    normalized
}

pub fn parse_scripts<P: AsRef<Path>>(path: P) -> Result<Vec<Script>, std::io::Error> {
    let content = fs::read_to_string(path)?;
    let block_re = Regex::new(r"(?m)^\s*---+\s*$").unwrap();
    let blocks: Vec<&str> = block_re.split(&content).map(|b| b.trim()).filter(|b| !b.is_empty()).collect();

    let mut scripts = Vec::new();
    for (i, block) in blocks.into_iter().enumerate() {
        scripts.push(parse_script_block(block, i));
    }
    Ok(scripts)
}

fn parse_script_block(block: &str, index: usize) -> Script {
    let mut title = String::new();
    let mut cta = String::new();
    let mut points = Vec::new();
    let mut point_pause_counts_before = Vec::new();
    let mut pending_pause_count = 0u32;
    let mut code = String::new();
    let mut duration = None;
    let mut layout = "center_stack".to_string();
    let mut all_at_once = false;
    let mut video = None;
    let mut audio = None;

    let line_num_re = Regex::new(r"(?i)^line_?\d+$").unwrap();
    let metadata_key_re = Regex::new(r"(?i)^[a-z][a-z0-9_ -]*$").unwrap();
    let list_prefix_re = Regex::new(r"^\s*(?:[-*]|\d+[.)])\s*").unwrap();
    let duration_num_re = Regex::new(r"\d+(?:\.\d+)?").unwrap();

    let mut push_point = |raw_value: &str| {
        let cleaned = list_prefix_re.replace(raw_value, "").trim().to_string();
        if cleaned.eq_ignore_ascii_case("(pause)") || cleaned.eq_ignore_ascii_case("[pause]") {
            pending_pause_count += 1;
            return;
        }
        if !cleaned.is_empty() {
            points.push(cleaned);
            point_pause_counts_before.push(pending_pause_count);
            pending_pause_count = 0;
        }
    };

    for raw_line in block.lines() {
        let line = raw_line.trim().trim_start_matches('\u{feff}').trim();
        if line.is_empty() {
            continue;
        }

        if let Some(pos) = line.find(':') {
            let key = line[..pos].trim().to_lowercase();
            let value = line[pos + 1..].trim();

            match key.as_str() {
                "title" | "question" => {
                    title = value.to_string();
                }
                "cta" | "caption" | "fb_caption" => {
                    cta = value.to_string();
                }
                "code" => {
                    code = value.to_string();
                }
                "video" | "vid" => {
                    if !value.is_empty() {
                        video = Some(PathBuf::from(value));
                    }
                }
                "audio" => {
                    if !value.is_empty() {
                        audio = Some(PathBuf::from(value));
                    }
                }
                "duration" => {
                    if let Some(mat) = duration_num_re.find(value) {
                        if let Ok(d) = mat.as_str().parse::<f32>() {
                            duration = Some(d);
                        }
                    }
                }
                "format" | "layout" => {
                    layout = normalize_layout(value);
                    let val_lower = value.to_lowercase();
                    if val_lower.contains("full")
                        || val_lower.contains("static")
                        || val_lower.contains("all at once")
                    {
                        all_at_once = true;
                    }
                }
                "text_animation" => {
                    let val_lower = value.to_lowercase();
                    if val_lower.contains("static")
                        || val_lower.contains("all at once")
                        || val_lower.contains("no pop")
                        || val_lower.contains("no popping")
                        || val_lower.contains("none")
                    {
                        all_at_once = true;
                    } else if val_lower.contains("question") && val_lower.contains("answer") {
                        layout = "question_answer".to_string();
                    } else if val_lower.contains("fade") || val_lower.contains("line") {
                        layout = "list_style".to_string();
                    }
                }
                k if line_num_re.is_match(k) => {
                    push_point(value);
                }
                "style" | "niche" | "sub_style" => {
                    // Skip
                }
                _ => {
                    // Ignore unknown metadata-style keys so parser labels never render.
                    if !metadata_key_re.is_match(key.as_str()) {
                        push_point(line);
                    }
                }
            }
        } else {
            push_point(line);
        }
    }

    if title.is_empty() && !points.is_empty() {
        title = points.remove(0);
        point_pause_counts_before.remove(0);
    }
    if title.is_empty() {
        title = format!("Video {}", index + 1);
    }

    Script {
        title,
        points,
        point_pause_counts_before,
        cta_pause_count_before: pending_pause_count,
        cta,
        code,
        duration,
        layout,
        all_at_once,
        video,
        audio,
    }
}
