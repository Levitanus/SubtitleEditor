use crate::{line_widget, EditorSnapshot, SubtitleEditorApp};
use eframe::egui;

#[derive(Debug, Clone, Copy)]
pub enum HistoryAction {
    Undo,
    Redo,
}

pub fn history_action_from_shortcuts(ctx: &egui::Context) -> Option<HistoryAction> {
    let redo_pressed = ctx.input_mut(|i| {
        i.consume_shortcut(&egui::KeyboardShortcut::new(
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
            egui::Key::Z,
        )) || i.consume_shortcut(&egui::KeyboardShortcut::new(
            egui::Modifiers::COMMAND,
            egui::Key::Y,
        ))
    });
    if redo_pressed {
        return Some(HistoryAction::Redo);
    }

    let undo_pressed = ctx.input_mut(|i| {
        i.consume_shortcut(&egui::KeyboardShortcut::new(
            egui::Modifiers::COMMAND,
            egui::Key::Z,
        ))
    });
    if undo_pressed {
        return Some(HistoryAction::Undo);
    }

    None
}

impl SubtitleEditorApp {
    pub(crate) fn handle_history_shortcuts(&mut self, ctx: &egui::Context) -> bool {
        if let Some(history_action) = history_action_from_shortcuts(ctx) {
            match history_action {
                HistoryAction::Undo => self.undo(),
                HistoryAction::Redo => self.redo(),
            }
            return true;
        }

        false
    }

    pub(crate) fn handle_shortcuts(
        &mut self,
        ctx: &egui::Context,
        focused_line: Option<line_widget::LineWidgetFocus>,
    ) -> bool {
        if let Some(line_action) = line_widget::line_action_from_shortcuts(ctx, focused_line) {
            self.apply_line_action(line_action);
        }

        false
    }

    pub(crate) fn capture_history_if_project_changed(&mut self, before: EditorSnapshot) {
        if self.current_snapshot() != before {
            self.undo_stack.push(before);
            if self.undo_stack.len() > 200 {
                self.undo_stack.remove(0);
            }
            self.redo_stack.clear();
        }
    }

    pub(crate) fn undo(&mut self) {
        if let Some(previous) = self.undo_stack.pop() {
            let current = self.current_snapshot();
            self.project = previous.project;
            self.secondary_project = previous.secondary_project;
            self.diff_view_enabled = previous.diff_view_enabled;
            self.redo_stack.push(current);
        }
    }

    pub(crate) fn redo(&mut self) {
        if let Some(next) = self.redo_stack.pop() {
            let current = self.current_snapshot();
            self.project = next.project;
            self.secondary_project = next.secondary_project;
            self.diff_view_enabled = next.diff_view_enabled;
            self.undo_stack.push(current);
        }
    }

    pub(crate) fn clear_history(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }
}
