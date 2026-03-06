use crate::{
    deepl, line_widget, AlternativesState, GlossaryEntry, ProjectState, SubtitleEditorApp,
    SubtitleLine,
};
use eframe::egui;

impl SubtitleEditorApp {
    pub(crate) fn handle_deepl_shortcuts(&mut self, ctx: &egui::Context) -> bool {
        let translate_batch_pressed = ctx.input_mut(|i| {
            i.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::T,
            ))
        });
        if translate_batch_pressed {
            let batch_size = self.deepl_state.translation_batch_size as usize;
            if let Err(error) = self.translate_n_lines_from_focus(batch_size) {
                self.error = Some(error);
            } else {
                self.error = None;
            }
            return true;
        }

        let translate_pressed = ctx.input_mut(|i| {
            i.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::COMMAND,
                egui::Key::T,
            ))
        });
        if translate_pressed {
            if let Err(error) = self.translate_focused_line() {
                self.error = Some(error);
            } else {
                self.error = None;
            }
            return true;
        }

        let alternatives_pressed = ctx.input_mut(|i| {
            i.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::COMMAND,
                egui::Key::A,
            ))
        });
        if alternatives_pressed {
            if let Err(error) = self.find_alternatives_for_focus() {
                self.error = Some(error);
            } else {
                self.error = None;
            }
            return true;
        }

        false
    }

    pub(crate) fn reset_translation_project(&mut self) {
        self.secondary_project = Some(self.build_translation_project_from_source());
    }

    fn build_translation_project_from_source(&self) -> ProjectState {
        ProjectState {
            lines: self
                .project
                .lines
                .iter()
                .map(|line| SubtitleLine {
                    text: String::new(),
                    timecode: line.timecode.clone(),
                })
                .collect(),
            file_type: self.project.file_type.clone(),
            file_path: None,
        }
    }

    fn ensure_translation_project_for_source(&mut self) {
        let source_len = self.project.lines.len();

        match self.secondary_project.as_mut() {
            None => {
                self.reset_translation_project();
            }
            Some(secondary) if secondary.lines.len() != source_len => {
                let previous_lines = secondary.lines.clone();
                let mut rebuilt = self.build_translation_project_from_source();
                for (target, previous) in rebuilt.lines.iter_mut().zip(previous_lines.into_iter()) {
                    target.text = previous.text;
                }
                self.secondary_project = Some(rebuilt);
            }
            Some(_) => {}
        }
    }

    fn enable_translation_mode(&mut self) {
        self.deepl_state.translation_mode = true;
        self.diff_view_enabled = true;
        self.ensure_translation_project_for_source();
    }

    pub(crate) fn render_deepl_panel(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("DeepL", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("API key:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.deepl_state.api_key)
                        .password(true)
                        .desired_width(220.0),
                );
                ui.checkbox(&mut self.deepl_state.use_free_api, "Free API");

                ui.label("SRC:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.deepl_state.source_lang)
                        .desired_width(48.0)
                        .hint_text("auto"),
                );

                ui.label("DST:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.deepl_state.target_lang)
                        .desired_width(48.0),
                );

                ui.label("Контекст:");
                ui.add(
                    egui::DragValue::new(&mut self.deepl_state.translation_context_radius)
                        .range(0..=10)
                        .speed(1),
                );

                ui.label("N:");
                ui.add(
                    egui::DragValue::new(&mut self.deepl_state.translation_batch_size)
                        .range(1..=500)
                        .speed(1),
                );

                let mut translation_mode = self.deepl_state.translation_mode;
                if ui
                    .checkbox(&mut translation_mode, "Translation mode (diff)")
                    .changed()
                {
                    if translation_mode {
                        self.enable_translation_mode();
                    } else {
                        self.deepl_state.translation_mode = false;
                    }
                }
            });

            ui.horizontal_wrapped(|ui| {
                if ui.button("Translate line").clicked() {
                    if let Err(e) = self.translate_focused_line() {
                        self.error = Some(e);
                    } else {
                        self.error = None;
                    }
                }

                if ui.button("Translate N lines").clicked() {
                    let batch_size = self.deepl_state.translation_batch_size as usize;
                    if let Err(e) = self.translate_n_lines_from_focus(batch_size) {
                        self.error = Some(e);
                    } else {
                        self.error = None;
                    }
                }

                if ui.button("Translate file").clicked() {
                    if let Err(e) = self.translate_active_file() {
                        self.error = Some(e);
                    } else {
                        self.error = None;
                    }
                }

                if ui.button("Alternatives").clicked() {
                    if let Err(e) = self.find_alternatives_for_focus() {
                        self.error = Some(e);
                    } else {
                        self.error = None;
                    }
                }

                if ui.button("Glossary").clicked() {
                    self.deepl_state.glossary_window_open = true;
                }
            });

            ui.label(
                egui::RichText::new(
                    "Ctrl+T: Translate line. Ctrl+Shift+T: translate N lines from focus. Ctrl+A: alternatives.",
                )
                .weak(),
            );
        });
    }

    fn deepl_client(&self) -> Result<deepl::DeepLClient, String> {
        let api_key = self.deepl_state.api_key.trim();
        if api_key.is_empty() {
            return Err("Set DeepL API key".to_string());
        }

        Ok(deepl::DeepLClient::new(
            api_key.to_string(),
            self.deepl_state.use_free_api,
        ))
    }

    fn normalized_target_lang(&self) -> Result<String, String> {
        let target = self.deepl_state.target_lang.trim().to_uppercase();
        if target.is_empty() {
            return Err("Set target language (for example, RU, EN, DE)".to_string());
        }
        Ok(target)
    }

    fn normalized_source_lang(&self) -> Option<String> {
        let source = self.deepl_state.source_lang.trim().to_uppercase();
        if source.is_empty() {
            None
        } else {
            Some(source)
        }
    }

    fn project_for_pane_mut(&mut self, pane: line_widget::PaneSide) -> Option<&mut ProjectState> {
        match pane {
            line_widget::PaneSide::Primary => Some(&mut self.project),
            line_widget::PaneSide::Secondary => self.secondary_project.as_mut(),
        }
    }

    fn project_for_pane(&self, pane: line_widget::PaneSide) -> Option<&ProjectState> {
        match pane {
            line_widget::PaneSide::Primary => Some(&self.project),
            line_widget::PaneSide::Secondary => self.secondary_project.as_ref(),
        }
    }

    fn line_text_by_focus(
        &self,
        pane: line_widget::PaneSide,
        index: usize,
    ) -> Result<String, String> {
        let project = self
            .project_for_pane(pane)
            .ok_or_else(|| "The chosen pane is unaccesible".to_string())?;
        let line = project
            .lines
            .get(index)
            .ok_or_else(|| "Line is not found".to_string())?;
        Ok(line.text.clone())
    }

    fn translate_focused_line(&mut self) -> Result<(), String> {
        let focus = self
            .active_focus
            .ok_or_else(|| "Choose the line for translation".to_string())?;

        let source_lang = self.normalized_source_lang();
        let target_lang = self.normalized_target_lang()?;
        let glossary_id = self.deepl_state.glossary_id.clone();
        let client = self.deepl_client()?;

        if self.deepl_state.translation_mode {
            let source_line = self
                .project
                .lines
                .get(focus.index)
                .ok_or_else(|| "The source line is not found".to_string())?;
            let context = build_translation_context_from_lines(
                &self.project.lines,
                focus.index,
                self.deepl_state.translation_context_radius as usize,
            );
            let translated = client.translate_texts_with_context(
                &[source_line.text.clone()],
                source_lang.as_deref(),
                &target_lang,
                glossary_id.as_deref(),
                context.as_deref(),
            )?;
            let translated_text = translated
                .into_iter()
                .next()
                .ok_or_else(|| "DeepL вернул пустой ответ".to_string())?;

            self.ensure_translation_project_for_source();
            let secondary = self
                .secondary_project
                .as_mut()
                .ok_or_else(|| "Проект перевода недоступен".to_string())?;
            let line = secondary
                .lines
                .get_mut(focus.index)
                .ok_or_else(|| "Строка перевода не найдена".to_string())?;
            line.text = translated_text;

            return Ok(());
        }

        let source_text = self.line_text_by_focus(focus.pane, focus.index)?;
        let context = self
            .project_for_pane(focus.pane)
            .map(|project| {
                build_translation_context_from_lines(
                    &project.lines,
                    focus.index,
                    self.deepl_state.translation_context_radius as usize,
                )
            })
            .unwrap_or(None);
        let translated = client.translate_texts_with_context(
            &[source_text],
            source_lang.as_deref(),
            &target_lang,
            glossary_id.as_deref(),
            context.as_deref(),
        )?;
        let new_text = translated
            .into_iter()
            .next()
            .ok_or_else(|| "DeepL вернул пустой ответ".to_string())?;

        let project = self
            .project_for_pane_mut(focus.pane)
            .ok_or_else(|| "Выбранная панель недоступна".to_string())?;
        let line = project
            .lines
            .get_mut(focus.index)
            .ok_or_else(|| "Строка не найдена".to_string())?;
        line.text = new_text;

        Ok(())
    }

    fn translate_n_lines_from_focus(&mut self, requested_count: usize) -> Result<(), String> {
        let focus = self
            .active_focus
            .ok_or_else(|| "Choose a line to start batch translation".to_string())?;

        let source_lang = self.normalized_source_lang();
        let target_lang = self.normalized_target_lang()?;
        let glossary_id = self.deepl_state.glossary_id.clone();
        let client = self.deepl_client()?;
        let count = requested_count.max(1);

        if self.deepl_state.translation_mode {
            let total = self.project.lines.len();
            if focus.index >= total {
                return Err("Focused line is out of range".to_string());
            }

            let start = focus.index;
            let end = (start + count).min(total);

            let translated = self.translate_line_range_with_context(
                &self.project.lines,
                start,
                end,
                &client,
                source_lang.as_deref(),
                &target_lang,
                glossary_id.as_deref(),
            )?;

            self.ensure_translation_project_for_source();
            let secondary = self
                .secondary_project
                .as_mut()
                .ok_or_else(|| "Translation project is unavailable".to_string())?;

            for (offset, translated_line) in translated.into_iter().enumerate() {
                let target_index = start + offset;
                let line = secondary
                    .lines
                    .get_mut(target_index)
                    .ok_or_else(|| "Target line is not found".to_string())?;
                line.text = translated_line;
            }

            return Ok(());
        }

        let pane = focus.pane;
        let (start, end, translated) = {
            let project = self
                .project_for_pane(pane)
                .ok_or_else(|| "The chosen pane is unaccesible".to_string())?;

            if focus.index >= project.lines.len() {
                return Err("Focused line is out of range".to_string());
            }

            let start = focus.index;
            let end = (start + count).min(project.lines.len());
            let translated = self.translate_line_range_with_context(
                &project.lines,
                start,
                end,
                &client,
                source_lang.as_deref(),
                &target_lang,
                glossary_id.as_deref(),
            )?;
            (start, end, translated)
        };

        let project = self
            .project_for_pane_mut(pane)
            .ok_or_else(|| "The chosen pane is unaccesible".to_string())?;

        if end > project.lines.len() {
            return Err("Pane lines changed during translation".to_string());
        }

        for (index, translated_line) in (start..end).zip(translated.into_iter()) {
            if let Some(line) = project.lines.get_mut(index) {
                line.text = translated_line;
            }
        }

        Ok(())
    }

    fn translate_active_file(&mut self) -> Result<(), String> {
        let source_lang = self.normalized_source_lang();
        let target_lang = self.normalized_target_lang()?;
        let glossary_id = self.deepl_state.glossary_id.clone();
        let client = self.deepl_client()?;

        if self.deepl_state.translation_mode {
            let source_lines = self
                .project
                .lines
                .iter()
                .map(|line| line.text.clone())
                .collect::<Vec<_>>();

            if source_lines.is_empty() {
                return Ok(());
            }

            let translated_all = self.translate_lines_with_context(
                &self.project.lines,
                &client,
                source_lang.as_deref(),
                &target_lang,
                glossary_id.as_deref(),
            )?;

            if translated_all.len() != source_lines.len() {
                return Err("DeepL вернул неполный результат перевода".to_string());
            }

            self.ensure_translation_project_for_source();
            let secondary = self
                .secondary_project
                .as_mut()
                .ok_or_else(|| "Проект перевода недоступен".to_string())?;
            for (line, translated) in secondary.lines.iter_mut().zip(translated_all.into_iter()) {
                line.text = translated;
            }

            return Ok(());
        }

        let pane = self
            .active_focus
            .map(|focus| focus.pane)
            .unwrap_or(line_widget::PaneSide::Primary);

        let source_lines = {
            let project = self
                .project_for_pane(pane)
                .ok_or_else(|| "Выбранная панель недоступна".to_string())?;
            project
                .lines
                .iter()
                .map(|line| line.text.clone())
                .collect::<Vec<_>>()
        };

        if source_lines.is_empty() {
            return Ok(());
        }

        let project_lines = self
            .project_for_pane(pane)
            .ok_or_else(|| "Выбранная панель недоступна".to_string())?;
        let translated_all = self.translate_lines_with_context(
            &project_lines.lines,
            &client,
            source_lang.as_deref(),
            &target_lang,
            glossary_id.as_deref(),
        )?;

        if translated_all.len() != source_lines.len() {
            return Err("DeepL вернул неполный результат перевода".to_string());
        }

        let project = self
            .project_for_pane_mut(pane)
            .ok_or_else(|| "Выбранная панель недоступна".to_string())?;
        for (line, translated) in project.lines.iter_mut().zip(translated_all.into_iter()) {
            line.text = translated;
        }

        Ok(())
    }

    fn translate_lines_with_context(
        &self,
        lines: &[SubtitleLine],
        client: &deepl::DeepLClient,
        source_lang: Option<&str>,
        target_lang: &str,
        glossary_id: Option<&str>,
    ) -> Result<Vec<String>, String> {
        self.translate_line_range_with_context(
            lines,
            0,
            lines.len(),
            client,
            source_lang,
            target_lang,
            glossary_id,
        )
    }

    fn translate_line_range_with_context(
        &self,
        lines: &[SubtitleLine],
        start: usize,
        end: usize,
        client: &deepl::DeepLClient,
        source_lang: Option<&str>,
        target_lang: &str,
        glossary_id: Option<&str>,
    ) -> Result<Vec<String>, String> {
        if lines.is_empty() || start >= lines.len() || start >= end {
            return Ok(Vec::new());
        }

        let effective_end = end.min(lines.len());
        let mut translated = Vec::with_capacity(effective_end - start);
        let context_radius = self.deepl_state.translation_context_radius as usize;

        for (index, line) in lines.iter().enumerate().skip(start).take(effective_end - start) {
            let context = build_translation_context_from_lines(lines, index, context_radius);
            let response = client.translate_texts_with_context(
                &[line.text.clone()],
                source_lang,
                target_lang,
                glossary_id,
                context.as_deref(),
            )?;

            translated.push(response.into_iter().next().unwrap_or_default());
        }

        Ok(translated)
    }

    fn find_alternatives_for_focus(&mut self) -> Result<(), String> {
        let focus = self
            .active_focus
            .ok_or_else(|| "Выберите строку и выделите слово/фразу".to_string())?;

        let line_text = self.line_text_by_focus(focus.pane, focus.index)?;
        let (start, end) = if let Some((start, end)) = focus.selection_range {
            (start, end)
        } else {
            word_span_at_char(&line_text, focus.char_index)
                .ok_or_else(|| "Не удалось определить слово под курсором".to_string())?
        };

        let selected_text = substring_by_char_range(&line_text, start, end)
            .ok_or_else(|| "Не удалось извлечь выделенный текст".to_string())?;

        let target_lang = self.normalized_target_lang()?;
        let client = self.deepl_client()?;
        let alternatives = client.find_alternatives(&selected_text, &target_lang)?;

        if alternatives.is_empty() {
            return Err("DeepL не вернул альтернативы".to_string());
        }

        self.deepl_state.alternatives = Some(AlternativesState {
            pane: focus.pane,
            line_index: focus.index,
            start,
            end,
            selected_text: selected_text.clone(),
            query: selected_text,
            items: alternatives,
            open: true,
        });

        Ok(())
    }

    fn sync_glossary_to_deepl(&mut self) -> Result<(), String> {
        let source_lang = self
            .normalized_source_lang()
            .ok_or_else(|| "Для глоссария укажите SRC language".to_string())?;
        let target_lang = self.normalized_target_lang()?;

        let entries = glossary_entries_to_tsv(&self.deepl_state.glossary_entries)?;
        let client = self.deepl_client()?;

        if let Some(glossary_id) = self.deepl_state.glossary_id.clone() {
            client.replace_glossary_dictionary(
                &glossary_id,
                &source_lang,
                &target_lang,
                &entries,
            )?;
        } else {
            let glossary_id = client.create_glossary(
                &self.deepl_state.glossary_name,
                &source_lang,
                &target_lang,
                &entries,
            )?;
            self.deepl_state.glossary_id = Some(glossary_id);
        }

        Ok(())
    }

    fn load_glossary_from_deepl(&mut self) -> Result<(), String> {
        let source_lang = self
            .normalized_source_lang()
            .ok_or_else(|| "Для загрузки глоссария укажите SRC language".to_string())?;
        let target_lang = self.normalized_target_lang()?;
        let client = self.deepl_client()?;

        let glossary_id = if let Some(glossary_id) = self.deepl_state.glossary_id.clone() {
            glossary_id
        } else {
            let glossaries = client.list_glossaries()?;
            let by_name_and_pair = glossaries.iter().find(|meta| {
                meta.name
                    .eq_ignore_ascii_case(&self.deepl_state.glossary_name)
                    && glossary_has_pair(meta, &source_lang, &target_lang)
            });

            let by_pair = glossaries
                .iter()
                .find(|meta| glossary_has_pair(meta, &source_lang, &target_lang));

            by_name_and_pair
                .or(by_pair)
                .map(|meta| meta.glossary_id.clone())
                .ok_or_else(|| {
                    format!(
                        "Не найден глоссарий в DeepL для пары {}→{}. Укажите Glossary ID или создайте/выгрузите глоссарий.",
                        source_lang, target_lang
                    )
                })?
        };

        let entries = client.load_glossary_entries(&glossary_id, &source_lang, &target_lang)?;
        self.deepl_state.glossary_entries = entries
            .into_iter()
            .map(|(source, target)| GlossaryEntry { source, target })
            .collect();
        self.deepl_state.glossary_id = Some(glossary_id);

        Ok(())
    }

    fn upload_glossary_to_deepl(&mut self) -> Result<(), String> {
        self.sync_glossary_to_deepl()
    }

    fn refresh_alternatives(&mut self) -> Result<(), String> {
        let query = self
            .deepl_state
            .alternatives
            .as_ref()
            .map(|alt| alt.query.trim().to_string())
            .ok_or_else(|| "Нет активного окна альтернатив".to_string())?;

        if query.is_empty() {
            return Err("Введите слово/фразу для поиска альтернатив".to_string());
        }

        let target_lang = self.normalized_target_lang()?;
        let client = self.deepl_client()?;
        let alternatives = client.find_alternatives(&query, &target_lang)?;

        if alternatives.is_empty() {
            return Err("DeepL не вернул альтернативы".to_string());
        }

        if let Some(state) = self.deepl_state.alternatives.as_mut() {
            state.items = alternatives;
        }

        Ok(())
    }

    fn apply_alternative_text(&mut self, replacement: &str) -> Result<(), String> {
        let alternatives = self
            .deepl_state
            .alternatives
            .as_ref()
            .ok_or_else(|| "Нет активных альтернатив".to_string())?
            .clone();

        let project = self
            .project_for_pane_mut(alternatives.pane)
            .ok_or_else(|| "Выбранная панель недоступна".to_string())?;
        let line = project
            .lines
            .get_mut(alternatives.line_index)
            .ok_or_else(|| "Строка не найдена".to_string())?;

        line.text = replace_char_range(
            &line.text,
            alternatives.start,
            alternatives.end,
            replacement,
        )
        .ok_or_else(|| "Не удалось заменить выделенный фрагмент".to_string())?;

        Ok(())
    }

    pub(crate) fn render_glossary_window(&mut self, ctx: &egui::Context) {
        if !self.deepl_state.glossary_window_open {
            return;
        }

        let mut open = self.deepl_state.glossary_window_open;
        let mut remove_index: Option<usize> = None;
        let mut sync_clicked = false;

        egui::Window::new("DeepL Glossary")
            .open(&mut open)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Имя:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.deepl_state.glossary_name)
                            .desired_width(220.0),
                    );
                });

                if let Some(glossary_id) = &self.deepl_state.glossary_id {
                    ui.label(format!("Glossary ID: {}", glossary_id));
                }

                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("SRC");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.deepl_state.source_lang)
                            .desired_width(48.0)
                            .hint_text("EN"),
                    );
                    ui.label("DST");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.deepl_state.target_lang)
                            .desired_width(48.0)
                            .hint_text("RU"),
                    );
                });

                ui.separator();
                egui::ScrollArea::vertical()
                    .max_height(280.0)
                    .show(ui, |ui| {
                        for (index, entry) in self.deepl_state.glossary_entries.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut entry.source)
                                        .desired_width(200.0)
                                        .hint_text("source phrase"),
                                );
                                ui.add(
                                    egui::TextEdit::singleline(&mut entry.target)
                                        .desired_width(200.0)
                                        .hint_text("target phrase"),
                                );
                                if ui.button("✕").clicked() {
                                    remove_index = Some(index);
                                }
                            });
                        }
                    });

                ui.horizontal(|ui| {
                    if ui.button("+ Добавить запись").clicked() {
                        self.deepl_state.glossary_entries.push(GlossaryEntry {
                            source: String::new(),
                            target: String::new(),
                        });
                    }

                    if ui.button("Загрузить из DeepL (пара)").clicked() {
                        match self.load_glossary_from_deepl() {
                            Ok(()) => self.error = None,
                            Err(e) => self.error = Some(e),
                        }
                    }

                    if ui.button("Выгрузить в DeepL (пара)").clicked() {
                        sync_clicked = true;
                    }
                });

                ui.label(
                    egui::RichText::new(
                        "Загрузка читает записи из DeepL для пары SRC→DST. Выгрузка создаёт/обновляет словарь для этой пары.",
                    )
                    .weak(),
                );
            });

        if let Some(index) = remove_index {
            if index < self.deepl_state.glossary_entries.len() {
                self.deepl_state.glossary_entries.remove(index);
            }
        }

        if sync_clicked {
            match self.upload_glossary_to_deepl() {
                Ok(()) => self.error = None,
                Err(e) => self.error = Some(e),
            }
        }

        self.deepl_state.glossary_window_open = open;
    }

    pub(crate) fn render_alternatives_window(&mut self, ctx: &egui::Context) {
        let Some(alternatives) = self.deepl_state.alternatives.as_mut() else {
            return;
        };

        let mut open = alternatives.open;
        let mut selected_item: Option<String> = None;
        let mut refresh_clicked = false;

        egui::Window::new("Альтернативы DeepL")
            .open(&mut open)
            .resizable(true)
            .show(ctx, |ui| {
                ui.label(format!("Исходный фрагмент: {}", alternatives.selected_text));
                ui.horizontal(|ui| {
                    ui.label("Искать для:");
                    ui.add(
                        egui::TextEdit::singleline(&mut alternatives.query).desired_width(240.0),
                    );
                    if ui.button("Обновить варианты").clicked() {
                        refresh_clicked = true;
                    }
                });
                ui.separator();

                for item in &alternatives.items {
                    if ui.button(item).clicked() {
                        selected_item = Some(item.clone());
                    }
                }
            });

        alternatives.open = open;

        if refresh_clicked && self.deepl_state.alternatives.is_some() {
            if let Err(e) = self.refresh_alternatives() {
                self.error = Some(e);
            } else {
                self.error = None;
            }
        }

        if let Some(selected_item) = selected_item {
            if let Err(e) = self.apply_alternative_text(&selected_item) {
                self.error = Some(e);
            } else {
                self.error = None;
                self.deepl_state.alternatives = None;
            }
        } else if !open {
            self.deepl_state.alternatives = None;
        }
    }
}

fn glossary_entries_to_tsv(entries: &[GlossaryEntry]) -> Result<String, String> {
    let pairs = entries
        .iter()
        .filter_map(|entry| {
            let source = normalize_glossary_text(&entry.source);
            let target = normalize_glossary_text(&entry.target);
            if source.is_empty() || target.is_empty() {
                None
            } else {
                Some(format!("{}\t{}", source, target))
            }
        })
        .collect::<Vec<_>>();

    if pairs.is_empty() {
        return Err("Добавьте хотя бы одну непустую запись в глоссарий".to_string());
    }

    Ok(pairs.join("\n"))
}

fn normalize_glossary_text(text: &str) -> String {
    text.trim().replace(['\t', '\n', '\r'], " ")
}

fn glossary_has_pair(meta: &deepl::GlossaryMeta, source_lang: &str, target_lang: &str) -> bool {
    meta.dictionaries.iter().any(|dictionary| {
        language_matches(&dictionary.source_lang, source_lang)
            && language_matches(&dictionary.target_lang, target_lang)
    })
}

fn language_matches(a: &str, b: &str) -> bool {
    let a_up = a.trim().to_uppercase();
    let b_up = b.trim().to_uppercase();
    if a_up == b_up {
        return true;
    }

    let a_base = a_up.split('-').next().unwrap_or(&a_up);
    let b_base = b_up.split('-').next().unwrap_or(&b_up);
    a_base == b_base
}

fn substring_by_char_range(text: &str, start: usize, end: usize) -> Option<String> {
    if start >= end {
        return None;
    }

    let start_byte = char_to_byte_index(text, start);
    let end_byte = char_to_byte_index(text, end);
    text.get(start_byte..end_byte).map(|s| s.to_string())
}

fn replace_char_range(text: &str, start: usize, end: usize, replacement: &str) -> Option<String> {
    let start_byte = char_to_byte_index(text, start);
    let end_byte = char_to_byte_index(text, end);
    if start_byte > end_byte || end_byte > text.len() {
        return None;
    }

    let mut out = String::new();
    out.push_str(text.get(..start_byte)?);
    out.push_str(replacement);
    out.push_str(text.get(end_byte..)?);
    Some(out)
}

fn char_to_byte_index(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map(|(idx, _)| idx)
        .unwrap_or(text.len())
}

fn word_span_at_char(text: &str, char_index: usize) -> Option<(usize, usize)> {
    let chars = text.chars().collect::<Vec<_>>();
    if chars.is_empty() {
        return None;
    }

    let mut index = char_index.min(chars.len().saturating_sub(1));

    if !is_word_char(chars[index]) {
        if index > 0 && is_word_char(chars[index - 1]) {
            index -= 1;
        } else {
            return None;
        }
    }

    let mut start = index;
    while start > 0 && is_word_char(chars[start - 1]) {
        start -= 1;
    }

    let mut end = index + 1;
    while end < chars.len() && is_word_char(chars[end]) {
        end += 1;
    }

    if start < end {
        Some((start, end))
    } else {
        None
    }
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

fn build_translation_context_from_lines(
    lines: &[SubtitleLine],
    line_index: usize,
    radius: usize,
) -> Option<String> {
    if lines.is_empty() || line_index >= lines.len() {
        return None;
    }

    let mut parts = Vec::new();
    let start = line_index.saturating_sub(radius);
    let end = (line_index + radius + 1).min(lines.len());

    for idx in start..end {
        if idx == line_index {
            continue;
        }

        let text = lines[idx]
            .text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if !text.is_empty() {
            parts.push(text);
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}
