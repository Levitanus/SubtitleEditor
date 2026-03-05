use eframe::egui;
use rfd::FileDialog;
use serde::{Deserialize, Serialize};
use std::time::Duration;

mod exporters;
mod history_actions;
mod importers;
mod line_widget;
mod timecode;

fn main() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .init();
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Subtitle Editor",
        options,
        Box::new(|_cc| Ok(Box::new(SubtitleEditorApp::default()))),
    )
    .unwrap();
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum SubtitleFileType {
    Txt,
    Sbv,
    Seproj,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SubtitleLine {
    pub text: String,
    pub timecode: Option<TimecodeRange>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct TimecodeRange {
    #[serde(with = "crate::timecode::duration_millis_serde")]
    pub start: Duration,
    #[serde(with = "crate::timecode::duration_millis_serde")]
    pub end: Duration,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
pub struct ProjectState {
    pub lines: Vec<SubtitleLine>,
    pub file_type: Option<SubtitleFileType>,
    pub file_path: Option<String>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct EditorConfig {
    pub window_pos: Option<(f32, f32)>,
    pub window_size: Option<(f32, f32)>,
}

#[derive(Default, Debug, Clone)]
struct SubtitleEditorApp {
    project: ProjectState,
    secondary_project: Option<ProjectState>,
    diff_view_enabled: bool,
    config: EditorConfig,
    error: Option<String>,
    undo_stack: Vec<EditorSnapshot>,
    redo_stack: Vec<EditorSnapshot>,
}

#[derive(Debug, Clone, PartialEq)]
struct EditorSnapshot {
    project: ProjectState,
    secondary_project: Option<ProjectState>,
    diff_view_enabled: bool,
}

impl eframe::App for SubtitleEditorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let state_before = self.current_snapshot();
        let mut skip_history_capture = false;

        if self.handle_history_shortcuts(ctx) {
            skip_history_capture = true;
        }

        let screen_rect = ctx.content_rect();
        self.config.window_pos = Some((screen_rect.min.x, screen_rect.min.y));
        self.config.window_size = Some((screen_rect.width(), screen_rect.height()));

        let mut focused_line: Option<line_widget::LineWidgetFocus> = None;

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Subtitle Editor");
            ui.separator();

            ui.horizontal(|ui| {
                if ui.button("Загрузить файл...").clicked() {
                    match self.load_primary_file() {
                        Ok(changed) => {
                            if changed {
                                self.clear_history();
                                self.error = None;
                                skip_history_capture = true;
                            }
                        }
                        Err(e) => {
                            self.error = Some(e);
                        }
                    }
                }

                if ui.button("Сохранить").clicked() {
                    if let Err(e) = self.save_current_file() {
                        self.error = Some(e);
                    } else {
                        self.error = None;
                    }
                }

                if ui.button("Экспорт").clicked() {
                    if let Err(e) = self.export_current_file() {
                        self.error = Some(e);
                    } else {
                        self.error = None;
                    }
                }

                if ui
                    .add_enabled(!self.undo_stack.is_empty(), egui::Button::new("Undo"))
                    .clicked()
                {
                    self.undo();
                    skip_history_capture = true;
                }

                if ui
                    .add_enabled(!self.redo_stack.is_empty(), egui::Button::new("Redo"))
                    .clicked()
                {
                    self.redo();
                    skip_history_capture = true;
                }

                ui.separator();

                ui.checkbox(&mut self.diff_view_enabled, "Diff view");

                if self.diff_view_enabled {
                    if ui.button("Загрузить второй файл...").clicked() {
                        match self.load_secondary_file() {
                            Ok(changed) => {
                                if changed {
                                    self.clear_history();
                                    self.error = None;
                                    skip_history_capture = true;
                                }
                            }
                            Err(e) => {
                                self.error = Some(e);
                            }
                        }
                    }

                    if self.secondary_project.is_some() {
                        if ui.button("Сохранить второй").clicked() {
                            if let Err(e) = self.save_secondary_file() {
                                self.error = Some(e);
                            } else {
                                self.error = None;
                            }
                        }

                        if ui.button("Экспорт второй").clicked() {
                            if let Err(e) = self.export_secondary_file() {
                                self.error = Some(e);
                            } else {
                                self.error = None;
                            }
                        }

                        if ui.button("Тайм-коды L→R").clicked() {
                            self.apply_timecodes_primary_to_secondary();
                        }

                        if ui.button("Тайм-коды R→L").clicked() {
                            self.apply_timecodes_secondary_to_primary();
                        }
                    }
                }
            });

            if let Some(path) = &self.project.file_path {
                ui.label(format!("Левый файл: {}", path));
            }

            if self.diff_view_enabled {
                if let Some(secondary) = &self.secondary_project {
                    if let Some(path) = &secondary.file_path {
                        ui.label(format!("Правый файл: {}", path));
                    }
                }
            }

            if let Some(ref err) = self.error {
                ui.colored_label(egui::Color32::RED, err);
            }

            if self.diff_view_enabled {
                ui.label("Diff view:");

                if let Some(secondary_project) = self.secondary_project.as_mut() {
                    render_diff_editor(ui, &mut self.project, secondary_project, &mut focused_line);
                } else {
                    ui.label("Diff view включен. Загрузите второй файл.");
                }
            } else {
                ui.label("Список строк:");
                render_single_editor(ui, &mut self.project, &mut focused_line);
            }
        });

        if self.handle_shortcuts(ctx, focused_line) {
            skip_history_capture = true;
        }

        if !skip_history_capture {
            self.capture_history_if_project_changed(state_before);
        }
    }
}

impl SubtitleEditorApp {
    fn current_snapshot(&self) -> EditorSnapshot {
        EditorSnapshot {
            project: self.project.clone(),
            secondary_project: self.secondary_project.clone(),
            diff_view_enabled: self.diff_view_enabled,
        }
    }

    fn load_primary_file(&mut self) -> Result<bool, String> {
        let Some(path) = FileDialog::new()
            .add_filter("Subtitles", &["txt", "sbv", "seproj"] as &[&str])
            .pick_file()
        else {
            return Ok(false);
        };

        let project = importers::import_file(&path.to_string_lossy())?;
        self.project = project;
        Ok(true)
    }

    fn load_secondary_file(&mut self) -> Result<bool, String> {
        let Some(path) = FileDialog::new()
            .add_filter("Subtitles", &["txt", "sbv", "seproj"] as &[&str])
            .pick_file()
        else {
            return Ok(false);
        };

        let project = importers::import_file(&path.to_string_lossy())?;
        self.secondary_project = Some(project);
        Ok(true)
    }

    fn apply_timecodes_primary_to_secondary(&mut self) {
        if let Some(secondary) = self.secondary_project.as_mut() {
            copy_timecodes_by_index(&self.project, secondary);
        }
    }

    fn apply_timecodes_secondary_to_primary(&mut self) {
        if let Some(secondary) = self.secondary_project.as_ref() {
            copy_timecodes_by_index(secondary, &mut self.project);
        }
    }
}

fn copy_timecodes_by_index(source: &ProjectState, target: &mut ProjectState) {
    let count = source.lines.len().min(target.lines.len());
    for index in 0..count {
        target.lines[index].timecode = source.lines[index].timecode.clone();
    }
}

fn render_single_editor(
    ui: &mut egui::Ui,
    project: &mut ProjectState,
    focused_line: &mut Option<line_widget::LineWidgetFocus>,
) {
    let row_height = 88.0;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show_rows(ui, row_height, project.lines.len(), |ui, row_range| {
            for index in row_range {
                if let Some(line) = project.lines.get_mut(index) {
                    if focused_line.is_none() {
                        *focused_line = line_widget::render_line_widget(
                            ui,
                            index,
                            line,
                            line_widget::PaneSide::Primary,
                            false,
                        );
                    } else {
                        let _ = line_widget::render_line_widget(
                            ui,
                            index,
                            line,
                            line_widget::PaneSide::Primary,
                            false,
                        );
                    }
                }
            }
        });
}

fn render_diff_editor(
    ui: &mut egui::Ui,
    left_project: &mut ProjectState,
    right_project: &mut ProjectState,
    focused_line: &mut Option<line_widget::LineWidgetFocus>,
) {
    let row_count = left_project.lines.len().max(right_project.lines.len());
    let row_height = 96.0;

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show_rows(ui, row_height, row_count, |ui, row_range| {
            for index in row_range {
                let align_timecode_row = left_project
                    .lines
                    .get(index)
                    .and_then(|line| line.timecode.as_ref())
                    .is_some()
                    || right_project
                        .lines
                        .get(index)
                        .and_then(|line| line.timecode.as_ref())
                        .is_some();

                ui.columns(2, |columns| {
                    columns[0].label(format!("{}:", index + 1));
                    if let Some(line) = left_project.lines.get_mut(index) {
                        let focus = line_widget::render_line_widget(
                            &mut columns[0],
                            index,
                            line,
                            line_widget::PaneSide::Primary,
                            align_timecode_row,
                        );
                        if focus.is_some() {
                            *focused_line = focus;
                        }
                    } else {
                        columns[0].label("—");
                    }

                    columns[1].label(format!("{}:", index + 1));
                    if let Some(line) = right_project.lines.get_mut(index) {
                        let focus = line_widget::render_line_widget(
                            &mut columns[1],
                            index,
                            line,
                            line_widget::PaneSide::Secondary,
                            align_timecode_row,
                        );
                        if focus.is_some() {
                            *focused_line = focus;
                        }
                    } else {
                        columns[1].label("—");
                    }
                });
                ui.separator();
            }
        });
}
