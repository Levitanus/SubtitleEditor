use eframe::egui;
use rfd::FileDialog;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

mod deepl;
mod deepl_gui;
mod exporters;
mod history_actions;
mod importers;
mod line_widget;
mod timecode;

fn main() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .init();
    let (app, options) = SubtitleEditorApp::bootstrap();
    eframe::run_native(
        "Subtitle Editor",
        options,
        Box::new(move |_cc| Ok(Box::new(app))),
    )
    .unwrap();
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum SubtitleFileType {
    Txt,
    Sbv,
    Srt,
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
    active_focus: Option<line_widget::LineWidgetFocus>,
    config: EditorConfig,
    deepl_state: DeepLUiState,
    last_saved_settings: Option<PersistedSettings>,
    error: Option<String>,
    undo_stack: Vec<EditorSnapshot>,
    redo_stack: Vec<EditorSnapshot>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
struct GlossaryEntry {
    source: String,
    target: String,
}

#[derive(Debug, Clone)]
struct AlternativesState {
    pane: line_widget::PaneSide,
    line_index: usize,
    start: usize,
    end: usize,
    selected_text: String,
    query: String,
    items: Vec<String>,
    open: bool,
}

#[derive(Debug, Clone)]
struct DeepLUiState {
    api_key: String,
    use_free_api: bool,
    translation_mode: bool,
    translation_context_radius: u8,
    translation_batch_size: u16,
    source_lang: String,
    target_lang: String,
    glossary_name: String,
    glossary_id: Option<String>,
    glossary_entries: Vec<GlossaryEntry>,
    glossary_window_open: bool,
    alternatives: Option<AlternativesState>,
}

impl Default for DeepLUiState {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            use_free_api: true,
            translation_mode: false,
            translation_context_radius: 2,
            translation_batch_size: 20,
            source_lang: String::new(),
            target_lang: "RU".to_string(),
            glossary_name: "Subtitle Glossary".to_string(),
            glossary_id: None,
            glossary_entries: Vec::new(),
            glossary_window_open: false,
            alternatives: None,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
struct PersistedSettings {
    window_pos: Option<(f32, f32)>,
    window_size: Option<(f32, f32)>,
    deepl_api_key: String,
    deepl_use_free_api: bool,
    deepl_translation_mode: bool,
    #[serde(default = "default_translation_context_radius")]
    deepl_translation_context_radius: u8,
    #[serde(default = "default_translation_batch_size")]
    deepl_translation_batch_size: u16,
    deepl_source_lang: String,
    deepl_target_lang: String,
    deepl_glossary_name: String,
    deepl_glossary_id: Option<String>,
    deepl_glossary_entries: Vec<GlossaryEntry>,
    diff_view_enabled: bool,
}

fn default_translation_context_radius() -> u8 {
    2
}

fn default_translation_batch_size() -> u16 {
    20
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

        if self.handle_deepl_shortcuts(ctx) {
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
                if ui.button("Load file...").clicked() {
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

                if ui.button("Save").clicked() {
                    if let Err(e) = self.save_current_file() {
                        self.error = Some(e);
                    } else {
                        self.error = None;
                    }
                }

                if ui.button("Export").clicked() {
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

                if self.deepl_state.translation_mode {
                    self.diff_view_enabled = true;
                    ui.add_enabled(
                        false,
                        egui::Checkbox::new(&mut self.diff_view_enabled, "Diff view"),
                    );
                } else {
                    ui.checkbox(&mut self.diff_view_enabled, "Diff view");
                }

                if self.diff_view_enabled {
                    if self.deepl_state.translation_mode {
                        ui.label("Translation mode: Right side would be affected by translator");
                        if ui.button("Reset translation project").clicked() {
                            self.reset_translation_project();
                            skip_history_capture = true;
                        }
                    } else {
                        if ui.button("Load the second file...").clicked() {
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
                    }

                    if self.secondary_project.is_some() {
                        if ui.button("Save right").clicked() {
                            if let Err(e) = self.save_secondary_file() {
                                self.error = Some(e);
                            } else {
                                self.error = None;
                            }
                        }

                        if ui.button("Export right").clicked() {
                            if let Err(e) = self.export_secondary_file() {
                                self.error = Some(e);
                            } else {
                                self.error = None;
                            }
                        }

                        if ui.button("Timcodes L->R").clicked() {
                            self.apply_timecodes_primary_to_secondary();
                        }

                        if ui.button("Timecodes L<-R").clicked() {
                            self.apply_timecodes_secondary_to_primary();
                        }
                    }
                }
            });

            ui.separator();
            self.render_deepl_panel(ui);

            if let Some(path) = &self.project.file_path {
                ui.label(format!("Left file: {}", path));
            }

            if self.diff_view_enabled {
                if let Some(secondary) = &self.secondary_project {
                    if let Some(path) = &secondary.file_path {
                        ui.label(format!("Right file: {}", path));
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
                    if self.deepl_state.translation_mode {
                        ui.label("Diff view is on. Press «reset translation» or «translate file».");
                    } else {
                        ui.label("Diff view is on. Load the right file.");
                    }
                }
            } else {
                ui.label("Lines list:");
                render_single_editor(ui, &mut self.project, &mut focused_line);
            }
        });

        if let Some(focused_line) = focused_line {
            self.active_focus = Some(focused_line);
        }

        self.render_glossary_window(ctx);
        self.render_alternatives_window(ctx);

        if self.handle_shortcuts(ctx, focused_line) {
            skip_history_capture = true;
        }

        if !skip_history_capture {
            self.capture_history_if_project_changed(state_before);
        }

        self.maybe_auto_save_settings();
    }
}

impl SubtitleEditorApp {
    fn bootstrap() -> (Self, eframe::NativeOptions) {
        let mut app = Self::default();

        if let Some(settings) = load_persisted_settings() {
            app.apply_persisted_settings(settings);
        }

        app.last_saved_settings = Some(app.to_persisted_settings());

        let mut options = eframe::NativeOptions::default();
        if let Some((width, height)) = app.config.window_size {
            options.viewport = options.viewport.with_inner_size([width, height]);
        }
        if let Some((x, y)) = app.config.window_pos {
            options.viewport = options.viewport.with_position([x, y]);
        }

        (app, options)
    }

    fn current_snapshot(&self) -> EditorSnapshot {
        EditorSnapshot {
            project: self.project.clone(),
            secondary_project: self.secondary_project.clone(),
            diff_view_enabled: self.diff_view_enabled,
        }
    }

    fn to_persisted_settings(&self) -> PersistedSettings {
        PersistedSettings {
            window_pos: self
                .config
                .window_pos
                .map(|(x, y)| (round_window_value(x), round_window_value(y))),
            window_size: self
                .config
                .window_size
                .map(|(w, h)| (round_window_value(w), round_window_value(h))),
            deepl_api_key: self.deepl_state.api_key.clone(),
            deepl_use_free_api: self.deepl_state.use_free_api,
            deepl_translation_mode: self.deepl_state.translation_mode,
            deepl_translation_context_radius: self.deepl_state.translation_context_radius,
            deepl_translation_batch_size: self.deepl_state.translation_batch_size,
            deepl_source_lang: self.deepl_state.source_lang.clone(),
            deepl_target_lang: self.deepl_state.target_lang.clone(),
            deepl_glossary_name: self.deepl_state.glossary_name.clone(),
            deepl_glossary_id: self.deepl_state.glossary_id.clone(),
            deepl_glossary_entries: self.deepl_state.glossary_entries.clone(),
            diff_view_enabled: self.diff_view_enabled,
        }
    }

    fn apply_persisted_settings(&mut self, settings: PersistedSettings) {
        self.config.window_pos = settings.window_pos;
        self.config.window_size = settings.window_size;
        self.deepl_state.api_key = settings.deepl_api_key;
        self.deepl_state.use_free_api = settings.deepl_use_free_api;
        self.deepl_state.translation_mode = settings.deepl_translation_mode;
        self.deepl_state.translation_context_radius = settings.deepl_translation_context_radius;
        self.deepl_state.translation_batch_size = settings.deepl_translation_batch_size;
        self.deepl_state.source_lang = settings.deepl_source_lang;
        self.deepl_state.target_lang = settings.deepl_target_lang;
        self.deepl_state.glossary_name = settings.deepl_glossary_name;
        self.deepl_state.glossary_id = settings.deepl_glossary_id;
        self.deepl_state.glossary_entries = settings.deepl_glossary_entries;
        self.diff_view_enabled = settings.diff_view_enabled;

        if self.deepl_state.translation_mode {
            self.diff_view_enabled = true;
            self.reset_translation_project();
        }
    }

    fn maybe_auto_save_settings(&mut self) {
        let settings = self.to_persisted_settings();
        if self.last_saved_settings.as_ref() == Some(&settings) {
            return;
        }

        if let Err(error) = save_persisted_settings(&settings) {
            log::warn!("Failed to save settings: {}", error);
            return;
        }

        self.last_saved_settings = Some(settings);
    }

    fn load_primary_file(&mut self) -> Result<bool, String> {
        let Some(path) = FileDialog::new()
            .add_filter("Subtitles", &["txt", "sbv", "srt", "seproj"] as &[&str])
            .pick_file()
        else {
            return Ok(false);
        };

        let project = importers::import_file(&path.to_string_lossy())?;
        self.project = project;

        if self.deepl_state.translation_mode {
            self.reset_translation_project();
        }

        Ok(true)
    }

    fn load_secondary_file(&mut self) -> Result<bool, String> {
        let Some(path) = FileDialog::new()
            .add_filter("Subtitles", &["txt", "sbv", "srt", "seproj"] as &[&str])
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

fn settings_file_path() -> PathBuf {
    if let Ok(xdg_config_home) = env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg_config_home)
            .join("subtitle_editor")
            .join("settings.json");
    }

    if let Ok(home) = env::var("HOME") {
        return PathBuf::from(home)
            .join(".config")
            .join("subtitle_editor")
            .join("settings.json");
    }

    PathBuf::from("subtitle_editor.settings.json")
}

fn load_persisted_settings() -> Option<PersistedSettings> {
    let path = settings_file_path();
    if !path.exists() {
        return None;
    }

    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) => {
            log::warn!("Failed to read settings from {:?}: {}", path, error);
            return None;
        }
    };

    match serde_json::from_str::<PersistedSettings>(&contents) {
        Ok(settings) => Some(settings),
        Err(error) => {
            log::warn!("Failed to parse settings from {:?}: {}", path, error);
            None
        }
    }
}

fn save_persisted_settings(settings: &PersistedSettings) -> Result<(), String> {
    let path = settings_file_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Cannot make settings folder: {}", error))?;
    }

    let json = serde_json::to_string_pretty(settings)
        .map_err(|error| format!("Cannot serialize settings: {}", error))?;
    fs::write(&path, json).map_err(|error| format!("Cannot write settings: {}", error))
}

fn round_window_value(value: f32) -> f32 {
    (value * 10.0).round() / 10.0
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
