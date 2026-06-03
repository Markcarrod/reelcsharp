use regex::Regex;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Script {
    pub title: String,
    pub points: Vec<String>,
    pub cta: String,
    pub code: String,
    pub duration: Option<f32>,
    pub layout: String,
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

    pub fn ffmpeg_filter(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Light => Some("gblur=sigma=8:steps=1"),
            Self::Middle => Some("gblur=sigma=16:steps=2"),
            Self::Heavy => Some("gblur=sigma=28:steps=2"),
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
        "progress" | "progress_reveal" => "progress_reveal".to_string(),
        "card" | "center_card" => "center_card".to_string(),
        _ => "center_stack".to_string(),
    }
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
    let mut code = String::new();
    let mut duration = None;
    let mut layout = "center_stack".to_string();

    let line_num_re = Regex::new(r"(?i)^line_?\d+$").unwrap();
    let metadata_key_re = Regex::new(r"(?i)^[a-z][a-z0-9_ -]*$").unwrap();
    let list_prefix_re = Regex::new(r"^\s*(?:[-*]|\d+[.)])\s*").unwrap();

    for raw_line in block.lines() {
        let line = raw_line.trim();
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
                "duration" => {
                    let num_re = Regex::new(r"\d+(?:\.\d+)?").unwrap();
                    if let Some(mat) = num_re.find(value) {
                        if let Ok(d) = mat.as_str().parse::<f32>() {
                            duration = Some(d);
                        }
                    }
                }
                "format" | "layout" => {
                    layout = normalize_layout(value);
                }
                "text_animation" => {
                    let val_lower = value.to_lowercase();
                    if val_lower.contains("question") && val_lower.contains("answer") {
                        layout = "question_answer".to_string();
                    } else if val_lower.contains("fade") || val_lower.contains("line") {
                        layout = "list_style".to_string();
                    }
                }
                k if line_num_re.is_match(k) => {
                    points.push(value.to_string());
                }
                "style" | "niche" | "sub_style" => {
                    // Skip
                }
                _ => {
                    // Ignore unknown metadata-style keys so parser labels never render.
                    if !metadata_key_re.is_match(key.as_str()) {
                        let cleaned = list_prefix_re.replace(line, "").into_owned();
                        points.push(cleaned);
                    }
                }
            }
        } else {
            let cleaned = list_prefix_re.replace(line, "").into_owned();
            points.push(cleaned);
        }
    }

    if title.is_empty() && !points.is_empty() {
        title = points.remove(0);
    }
    if title.is_empty() {
        title = format!("Video {}", index + 1);
    }

    Script {
        title,
        points,
        cta,
        code,
        duration,
        layout,
    }
}
