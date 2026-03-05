use crate::timecode::parse_sbv_time_range;
use crate::{ProjectState, SubtitleFileType, SubtitleLine};
use std::fs::File;
use std::io::{BufRead, BufReader};

pub fn import_file(path: &str) -> Result<ProjectState, String> {
    if path.ends_with(".txt") {
        let lines = import_txt(path)?;
        Ok(ProjectState {
            lines,
            file_type: Some(SubtitleFileType::Txt),
            file_path: Some(path.to_string()),
        })
    } else if path.ends_with(".sbv") {
        let lines = import_sbv(path)?;
        Ok(ProjectState {
            lines,
            file_type: Some(SubtitleFileType::Sbv),
            file_path: Some(path.to_string()),
        })
    } else if path.ends_with(".seproj") {
        let mut project = import_seproj(path)?;
        if project.file_path.is_none() {
            project.file_path = Some(path.to_string());
        }
        if project.file_type.is_none() {
            project.file_type = Some(SubtitleFileType::Seproj);
        }
        Ok(project)
    } else {
        Err("Неизвестный формат файла".to_string())
    }
}

fn import_txt(path: &str) -> Result<Vec<SubtitleLine>, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let reader = BufReader::new(file);
    let mut result = Vec::new();
    let mut current_block: Vec<String> = Vec::new();

    for line in reader.lines() {
        let line = line.map_err(|e| e.to_string())?;
        let stripped = line.trim();

        if stripped.is_empty() {
            if !current_block.is_empty() {
                result.push(SubtitleLine {
                    text: current_block.join("\n"),
                    timecode: None,
                });
                current_block.clear();
            }
        } else {
            current_block.push(stripped.to_string());
        }
    }

    if !current_block.is_empty() {
        result.push(SubtitleLine {
            text: current_block.join("\n"),
            timecode: None,
        });
    }

    Ok(result)
}

fn import_sbv(path: &str) -> Result<Vec<SubtitleLine>, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let reader = BufReader::new(file);
    let raw_lines: Vec<String> = reader
        .lines()
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;

    let mut lines = Vec::new();
    let mut idx = 0;

    while idx < raw_lines.len() {
        let line = raw_lines[idx].trim();
        if line.is_empty() {
            idx += 1;
            continue;
        }

        let timecode = parse_sbv_time_range(line)
            .ok_or_else(|| format!("Некорректная строка тайм-кода SBV: {}", line))?;
        idx += 1;

        let mut text_parts = Vec::new();
        while idx < raw_lines.len() {
            let text_line = raw_lines[idx].trim();
            if text_line.is_empty() {
                break;
            }
            text_parts.push(text_line.to_string());
            idx += 1;
        }

        lines.push(SubtitleLine {
            text: text_parts.join("\n"),
            timecode: Some(timecode),
        });

        while idx < raw_lines.len() && raw_lines[idx].trim().is_empty() {
            idx += 1;
        }
    }

    Ok(lines)
}

fn import_seproj(path: &str) -> Result<ProjectState, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let reader = BufReader::new(file);
    serde_json::from_reader(reader).map_err(|e| e.to_string())
}
