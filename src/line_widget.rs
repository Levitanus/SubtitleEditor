use crate::timecode::{duration_to_parts, parts_to_duration};
use crate::{SubtitleEditorApp, SubtitleLine, TimecodeRange};
use eframe::egui;
use egui::{DragValue, TextEdit};
use std::time::Duration;

const TIMECODE_ROW_RESERVED_HEIGHT: f32 = 31.0;
const TIMECODE_BOTTOM_GAP: f32 = 3.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneSide {
    Primary,
    Secondary,
}

#[derive(Debug, Clone, Copy)]
pub enum LineWidgetAction {
    SplitAtCursor {
        pane: PaneSide,
        index: usize,
        char_index: usize,
    },
    MergeWithPrevious {
        pane: PaneSide,
        index: usize,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct LineWidgetFocus {
    pub pane: PaneSide,
    pub index: usize,
    pub char_index: usize,
    pub selection_range: Option<(usize, usize)>,
}

pub fn line_action_from_shortcuts(
    ctx: &egui::Context,
    focused: Option<LineWidgetFocus>,
) -> Option<LineWidgetAction> {
    let focused = focused?;

    let split_pressed = shortcut_pressed(ctx, egui::Key::Enter, true, false);
    if split_pressed {
        return Some(LineWidgetAction::SplitAtCursor {
            pane: focused.pane,
            index: focused.index,
            char_index: focused.char_index,
        });
    }

    let merge_pressed = shortcut_pressed(ctx, egui::Key::Backspace, true, false);
    if merge_pressed && focused.char_index == 0 && focused.index > 0 {
        return Some(LineWidgetAction::MergeWithPrevious {
            pane: focused.pane,
            index: focused.index,
        });
    }

    None
}

fn shortcut_pressed(
    ctx: &egui::Context,
    target_key: egui::Key,
    require_ctrl: bool,
    require_shift: bool,
) -> bool {
    ctx.input(|i| {
        if i.modifiers.ctrl != require_ctrl || i.modifiers.shift != require_shift {
            return false;
        }

        i.raw.events.iter().any(|event| match event {
            egui::Event::Key {
                key,
                physical_key,
                pressed: true,
                ..
            } => key_matches_shortcut(*key, *physical_key, target_key),
            _ => false,
        })
    })
}

fn key_matches_shortcut(
    logical_key: egui::Key,
    physical_key: Option<egui::Key>,
    target_key: egui::Key,
) -> bool {
    if logical_key == target_key {
        return true;
    }

    if is_latin_letter_key(logical_key) {
        return false;
    }

    physical_key == Some(target_key)
}

fn is_latin_letter_key(key: egui::Key) -> bool {
    matches!(
        key,
        egui::Key::A
            | egui::Key::B
            | egui::Key::C
            | egui::Key::D
            | egui::Key::E
            | egui::Key::F
            | egui::Key::G
            | egui::Key::H
            | egui::Key::I
            | egui::Key::J
            | egui::Key::K
            | egui::Key::L
            | egui::Key::M
            | egui::Key::N
            | egui::Key::O
            | egui::Key::P
            | egui::Key::Q
            | egui::Key::R
            | egui::Key::S
            | egui::Key::T
            | egui::Key::U
            | egui::Key::V
            | egui::Key::W
            | egui::Key::X
            | egui::Key::Y
            | egui::Key::Z
    )
}

pub fn render_line_widget(
    ui: &mut egui::Ui,
    index: usize,
    line: &mut SubtitleLine,
    pane: PaneSide,
    reserve_timecode_slot: bool,
) -> Option<LineWidgetFocus> {
    let fill = if index % 2 == 0 {
        ui.visuals().faint_bg_color
    } else {
        ui.visuals().extreme_bg_color
    };

    let mut focus = None;

    egui::Frame::new()
        .fill(fill)
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                if let Some(timecode) = line.timecode.as_mut() {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        timecode_group(ui, "START", &mut timecode.start);
                        timecode_group(ui, "END", &mut timecode.end);
                    });
                    ui.add_space(TIMECODE_BOTTOM_GAP);
                } else if reserve_timecode_slot {
                    ui.add_sized(
                        [ui.available_width(), TIMECODE_ROW_RESERVED_HEIGHT],
                        egui::Label::new(egui::RichText::new("no timecode").weak()),
                    );
                    ui.add_space(TIMECODE_BOTTOM_GAP);
                }

                let output = TextEdit::multiline(&mut line.text)
                    .desired_width(f32::INFINITY)
                    .desired_rows(2)
                    .show(ui);

                if output.response.has_focus() {
                    let (char_index, selection_range) = output
                        .cursor_range
                        .map(|range| {
                            let primary = range.primary.index;
                            let secondary = range.secondary.index;
                            let selection = if primary == secondary {
                                None
                            } else {
                                Some((primary.min(secondary), primary.max(secondary)))
                            };
                            (primary, selection)
                        })
                        .unwrap_or_else(|| (line.text.chars().count(), None));
                    focus = Some(LineWidgetFocus {
                        pane,
                        index,
                        char_index,
                        selection_range,
                    });
                }
            });
        });

    focus
}

impl SubtitleEditorApp {
    pub(crate) fn apply_line_action(&mut self, action: LineWidgetAction) {
        match action {
            LineWidgetAction::SplitAtCursor {
                pane,
                index,
                char_index,
            } => {
                self.split_line_at(pane, index, char_index);
            }
            LineWidgetAction::MergeWithPrevious { pane, index } => {
                self.merge_with_previous(pane, index);
            }
        }
    }

    fn lines_mut(&mut self, pane: PaneSide) -> Option<&mut Vec<SubtitleLine>> {
        match pane {
            PaneSide::Primary => Some(&mut self.project.lines),
            PaneSide::Secondary => self.secondary_project.as_mut().map(|p| &mut p.lines),
        }
    }

    fn merge_with_previous(&mut self, pane: PaneSide, index: usize) {
        let Some(lines) = self.lines_mut(pane) else {
            return;
        };

        merge_with_previous_in(lines, index);
    }

    fn split_line_at(&mut self, pane: PaneSide, index: usize, char_index: usize) {
        let Some(lines) = self.lines_mut(pane) else {
            return;
        };

        split_line_at_in(lines, index, char_index);
    }
}

fn merge_with_previous_in(lines: &mut Vec<SubtitleLine>, index: usize) {
    if index == 0 || index >= lines.len() {
        return;
    }

    let current = lines.remove(index);
    let previous = &mut lines[index - 1];

    if !previous.text.is_empty() && !current.text.is_empty() {
        previous.text.push('\n');
    }
    previous.text.push_str(&current.text);

    previous.timecode = merge_timecodes(previous.timecode.take(), current.timecode);
}

fn split_line_at_in(lines: &mut Vec<SubtitleLine>, index: usize, char_index: usize) {
    if index >= lines.len() {
        return;
    }

    let original = lines[index].clone();
    let split_byte = char_to_byte_index(&original.text, char_index);
    let left_text = original.text[..split_byte].to_string();
    let right_text = original.text[split_byte..].to_string();

    if left_text.is_empty() || right_text.is_empty() {
        return;
    }

    let (left_timecode, right_timecode) = match original.timecode {
        Some(tc) => {
            let left_len = display_len(&left_text);
            let right_len = display_len(&right_text);
            let (left_tc, right_tc) =
                crate::timecode::split_timecode_range(&tc, left_len, right_len);
            (Some(left_tc), Some(right_tc))
        }
        None => (None, None),
    };

    lines[index] = SubtitleLine {
        text: left_text,
        timecode: left_timecode,
    };

    lines.insert(
        index + 1,
        SubtitleLine {
            text: right_text,
            timecode: right_timecode,
        },
    );
}

fn merge_timecodes(
    left: Option<TimecodeRange>,
    right: Option<TimecodeRange>,
) -> Option<TimecodeRange> {
    match (left, right) {
        (Some(a), Some(b)) => {
            let start = if a.start <= b.start { a.start } else { b.start };
            let end = if a.end >= b.end { a.end } else { b.end };
            Some(TimecodeRange { start, end })
        }
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

fn char_to_byte_index(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map(|(idx, _)| idx)
        .unwrap_or(text.len())
}

fn display_len(text: &str) -> usize {
    text.chars().count().max(1)
}

fn timecode_group(ui: &mut egui::Ui, title: &str, value: &mut Duration) {
    egui::Frame::new()
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .inner_margin(egui::Margin::symmetric(6, 3))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                ui.strong(title);
                ui.add_space(3.0);
                timecode_value_edit(ui, value);
            });
        });
}

fn timecode_value_edit(ui: &mut egui::Ui, value: &mut Duration) {
    let mut parts = duration_to_parts(*value);

    let w_h = width_for_digits(ui, 1);
    let w_mm = width_for_digits(ui, 2);
    let w_ss = width_for_digits(ui, 2);
    let w_ms = width_for_digits(ui, 3);

    let mut changed = false;

    changed |= add_compact_drag_value(ui, w_h, &mut parts.hours, 0..=99);
    ui.monospace(":");
    changed |= add_compact_drag_value(ui, w_mm, &mut parts.minutes, 0..=59);
    ui.monospace(":");
    changed |= add_compact_drag_value(ui, w_ss, &mut parts.seconds, 0..=59);
    ui.monospace(".");
    changed |= add_compact_drag_value(ui, w_ms, &mut parts.millis, 0..=999);

    if changed {
        *value = parts_to_duration(parts);
    }
}

fn width_for_digits(ui: &egui::Ui, digits: usize) -> f32 {
    let style = egui::TextStyle::Monospace;
    let text = "0".repeat(digits);
    let text_width = ui.fonts_mut(|fonts| {
        let font_id = style.resolve(ui.style());
        fonts
            .layout_no_wrap(text, font_id, ui.visuals().text_color())
            .size()
            .x
    });
    text_width + (ui.spacing().button_padding.x * 2.0) + 6.0
}

fn add_compact_drag_value(
    ui: &mut egui::Ui,
    width: f32,
    value: &mut i64,
    range: std::ops::RangeInclusive<i64>,
) -> bool {
    let mut changed = false;

    ui.scope(|ui| {
        ui.spacing_mut().interact_size = egui::vec2(width, 18.0);
        ui.spacing_mut().button_padding = egui::vec2(2.0, 0.0);

        changed = ui
            .add(
                DragValue::new(value)
                    .range(range)
                    .max_decimals(0)
                    .min_decimals(0)
                    .speed(1),
            )
            .changed();
    });

    changed
}
