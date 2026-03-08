use crate::timecode::{format_sbv_timecode, format_srt_timecode};
use crate::{ProjectState, SubtitleEditorApp, SubtitleFileType};
use rfd::FileDialog;
use std::fs::File;
use std::io::Write;
use std::path::Path;

pub fn export_file(project: &ProjectState, path: &str) -> Result<(), String> {
    if path.ends_with(".txt") {
        export_txt(project, path)
    } else if path.ends_with(".sbv") {
        export_sbv(project, path)
    } else if path.ends_with(".srt") {
        export_srt(project, path)
    } else if path.ends_with(".seproj") {
        export_seproj(project, path)
    } else {
        Err("Unknown format to save as".to_string())
    }
}

pub fn export_sbv(project: &ProjectState, path: &str) -> Result<(), String> {
    let mut file = File::create(path).map_err(|e| e.to_string())?;

    for (idx, line) in project.lines.iter().enumerate() {
        let timecode = line
            .timecode
            .as_ref()
            .ok_or_else(|| format!("Line {} timecode is missing", idx + 1))?;

        writeln!(
            file,
            "{},{}",
            format_sbv_timecode(timecode.start),
            format_sbv_timecode(timecode.end)
        )
        .map_err(|e| e.to_string())?;
        writeln!(file, "{}", line.text).map_err(|e| e.to_string())?;
        writeln!(file).map_err(|e| e.to_string())?;
    }

    Ok(())
}

pub fn export_srt(project: &ProjectState, path: &str) -> Result<(), String> {
    let mut file = File::create(path).map_err(|e| e.to_string())?;

    for (idx, line) in project.lines.iter().enumerate() {
        let timecode = line
            .timecode
            .as_ref()
            .ok_or_else(|| format!("Line {} timecode is missing", idx + 1))?;

        writeln!(file, "{}", idx + 1).map_err(|e| e.to_string())?;
        writeln!(
            file,
            "{} --> {}",
            format_srt_timecode(timecode.start),
            format_srt_timecode(timecode.end)
        )
        .map_err(|e| e.to_string())?;
        writeln!(file, "{}", line.text).map_err(|e| e.to_string())?;
        writeln!(file).map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn export_txt(project: &ProjectState, path: &str) -> Result<(), String> {
    let mut file = File::create(path).map_err(|e| e.to_string())?;
    for (idx, line) in project.lines.iter().enumerate() {
        file.write_all(line.text.as_bytes())
            .map_err(|e| e.to_string())?;
        if idx + 1 < project.lines.len() {
            file.write_all(b"\n\n").map_err(|e| e.to_string())?;
        }
    }
    if !project.lines.is_empty() {
        file.write_all(b"\n").map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn export_seproj(project: &ProjectState, path: &str) -> Result<(), String> {
    let file = File::create(path).map_err(|e| e.to_string())?;
    serde_json::to_writer_pretty(file, project).map_err(|e| e.to_string())
}

impl SubtitleEditorApp {
    pub(crate) fn save_current_file(&mut self) -> Result<(), String> {
        save_project_as_seproj(&mut self.project)
    }

    pub(crate) fn save_secondary_file(&mut self) -> Result<(), String> {
        let Some(project) = self.secondary_project.as_mut() else {
            return Ok(());
        };

        save_project_as_seproj(project)
    }

    pub(crate) fn export_current_file(&self) -> Result<(), String> {
        export_project_with_dialog(&self.project)
    }

    pub(crate) fn export_secondary_file(&self) -> Result<(), String> {
        let Some(project) = self.secondary_project.as_ref() else {
            return Ok(());
        };

        export_project_with_dialog(project)
    }
}

fn save_project_as_seproj(project: &mut ProjectState) -> Result<(), String> {
    let mut target_path = project
        .file_path
        .clone()
        .filter(|path| has_extension(path, "seproj"));

    if target_path.is_none() {
        let suggested_name = project
            .file_path
            .as_deref()
            .and_then(|path| Path::new(path).file_stem())
            .and_then(|stem| stem.to_str())
            .map(|stem| format!("{}.seproj", stem))
            .unwrap_or_else(|| "subtitle.seproj".to_string());

        let picked = FileDialog::new()
            .add_filter("Subtitle Project", &["seproj"] as &[&str])
            .set_file_name(&suggested_name)
            .save_file();

        if let Some(path) = picked {
            target_path = Some(path.to_string_lossy().to_string());
        } else {
            return Ok(());
        }
    }

    let path = ensure_extension(target_path.expect("target_path is checked above"), "seproj");
    export_seproj(project, &path)?;
    project.file_path = Some(path);
    project.file_type = Some(SubtitleFileType::Seproj);

    Ok(())
}

fn export_project_with_dialog(project: &ProjectState) -> Result<(), String> {
    let default_ext = infer_default_export_extension(project);

    let picked = FileDialog::new()
        .add_filter("Text", &["txt"] as &[&str])
        .add_filter("SBV", &["sbv"] as &[&str])
        .add_filter("SRT", &["srt"] as &[&str])
        .set_file_name(&format!("subtitle.{}", default_ext))
        .save_file();

    let Some(path) = picked else {
        return Ok(());
    };

    let raw_path = path.to_string_lossy().to_string();
    let normalized_path = if has_extension(&raw_path, "txt")
        || has_extension(&raw_path, "sbv")
        || has_extension(&raw_path, "srt")
    {
        raw_path
    } else if has_any_extension(&raw_path) {
        return Err("Export supports only .txt, .sbv and .srt".to_string());
    } else {
        ensure_extension(raw_path, default_ext)
    };

    export_file(project, &normalized_path)
}

fn infer_default_export_extension(project: &ProjectState) -> &'static str {
    match project.file_type {
        Some(SubtitleFileType::Txt) => "txt",
        Some(SubtitleFileType::Sbv) => "sbv",
        Some(SubtitleFileType::Srt) => "srt",
        _ => {
            if project.lines.iter().any(|line| line.timecode.is_some()) {
                "sbv"
            } else {
                "txt"
            }
        }
    }
}

fn has_extension(path: &str, extension: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case(extension))
        .unwrap_or(false)
}

fn has_any_extension(path: &str) -> bool {
    Path::new(path).extension().is_some()
}

fn ensure_extension(path: String, extension: &str) -> String {
    if has_extension(&path, extension) {
        path
    } else {
        format!("{}.{}", path, extension)
    }
}
