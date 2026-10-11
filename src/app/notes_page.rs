//! The notes list and editor. Search results are cached per vault revision
//! and only short, bounded previews are formatted, so large vaults and large
//! notes add no per-frame work to the sidebar.

use std::cmp::Reverse;

use eframe::egui::{self, RichText};
use rakshamemo::{
    model::{self, Note},
    settings::NoteSort,
};
use zeroize::{Zeroize, Zeroizing};

use super::{Dialog, Notepad, Shortcut, style};

const LIST_ROW_HEIGHT: f32 = 46.0;
const PREVIEW_CHARS: usize = 80;

/// Indices of the notes matching a search. Rebuilt only when the query, the
/// vault revision, or the number of notes changes, never on mere repaints.
#[derive(Default)]
pub(super) struct SearchCache {
    key: Option<(u64, usize)>,
    needle: Zeroizing<String>,
    hits: Vec<usize>,
}

impl SearchCache {
    fn matches(&mut self, notes: &[Note], needle: &str, revision: u64) -> &[usize] {
        let key = (revision, notes.len());
        if self.key != Some(key) || *self.needle != needle {
            self.hits = notes
                .iter()
                .enumerate()
                .filter(|(_, note)| {
                    needle.is_empty()
                        || note.title.to_lowercase().contains(needle)
                        || note.content.to_lowercase().contains(needle)
                })
                .map(|(index, _)| index)
                .collect();
            self.needle.zeroize();
            self.needle.push_str(needle);
            self.key = Some(key);
        }
        &self.hits
    }
}

/// Pinned notes first, then the chosen order.
fn sort_notes(order: &mut [usize], notes: &[Note], sort: NoteSort) {
    match sort {
        NoteSort::RecentlyEdited => order.sort_by_cached_key(|&index| {
            let note = &notes[index];
            let edited = chrono::DateTime::parse_from_rfc3339(&note.updated_at)
                .map_or(i64::MIN, |date| date.timestamp_millis());
            (!note.pinned, Reverse(edited))
        }),
        NoteSort::Title => order.sort_by_cached_key(|&index| {
            let note = &notes[index];
            (!note.pinned, note.title.to_lowercase())
        }),
        // Notes are stored in creation order; show the newest first.
        NoteSort::Created => order.sort_by_key(|&index| (!notes[index].pinned, Reverse(index))),
    }
}

fn sort_label(sort: NoteSort) -> &'static str {
    match sort {
        NoteSort::RecentlyEdited => "Last edited",
        NoteSort::Title => "Title (A–Z)",
        NoteSort::Created => "Newest first",
    }
}

impl Notepad {
    pub(super) fn notes_ui(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        let mut delete = None;
        let mut sort = self.settings.note_sort;
        let shortcut = self.transient.shortcut.take();
        // Requested by the previous frame; reading it before the title renders
        // keeps this frame's title keystrokes out of the body.
        let focus_body = std::mem::take(&mut self.transient.focus_note_body);
        let store = self.store.as_mut().unwrap();
        let mut new_note = shortcut == Some(Shortcut::New);
        egui::Panel::left("note_list")
            .default_size(240.0)
            .show(ui, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.heading("Notes");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        new_note |= style::primary_button(ui, "+ New")
                            .on_hover_text("Ctrl/Cmd+N")
                            .clicked();
                    });
                });
                ui.add_space(2.0);
                let search = ui
                    .add(
                        egui::TextEdit::singleline(&mut self.query)
                            .id(egui::Id::new("note_search"))
                            .hint_text("Search notes…")
                            .margin(egui::vec2(8.0, 6.0))
                            .min_size(egui::vec2(0.0, 32.0))
                            .desired_width(f32::INFINITY),
                    )
                    .on_hover_text("Ctrl/Cmd+F");
                if shortcut == Some(Shortcut::Find) {
                    search.request_focus();
                }
                ui.horizontal(|ui| {
                    ui.weak("Sort");
                    egui::ComboBox::from_id_salt("note_sort")
                        .selected_text(sort_label(sort))
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            for option in
                                [NoteSort::RecentlyEdited, NoteSort::Title, NoteSort::Created]
                            {
                                ui.selectable_value(&mut sort, option, sort_label(option));
                            }
                        });
                });
                ui.add_space(2.0);
                ui.separator();
                let needle = Zeroizing::new(self.query.trim().to_lowercase());
                let mut order = self
                    .transient
                    .note_search
                    .matches(&store.data.notes.items, &needle, self.revision)
                    .to_vec();
                sort_notes(&mut order, &store.data.notes.items, sort);
                egui::ScrollArea::vertical()
                    .id_salt("notes_list")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        for &index in &order {
                            let note = &mut store.data.notes.items[index];
                            let selected = note.id == store.data.notes.active_id;
                            let row = note_row(ui, note, selected);
                            if row.clicked() && !selected {
                                store.data.notes.active_id = note.id.clone();
                                changed = true;
                            }
                            row.context_menu(|ui| {
                                let pin = if note.pinned { "Unpin" } else { "Pin to top" };
                                if ui.button(pin).clicked() {
                                    note.pinned = !note.pinned;
                                    changed = true;
                                    ui.close();
                                }
                                if style::danger_button(ui, "Delete…").clicked() {
                                    delete = Some(note.id.clone());
                                    ui.close();
                                }
                            });
                        }
                        if order.is_empty() {
                            ui.add_space(16.0);
                            ui.vertical_centered(|ui| {
                                ui.weak(if store.data.notes.items.is_empty() {
                                    "No notes yet."
                                } else {
                                    "No matching notes."
                                });
                            });
                        }
                    });
            });
        if new_note {
            self.query.zeroize();
            add_note(&mut store.data.notes);
            self.transient.focus_note_title = true;
            changed = true;
        }
        egui::CentralPanel::default().show(ui, |ui| {
            let Some(note) = store
                .data
                .notes
                .items
                .iter_mut()
                .find(|note| note.id == store.data.notes.active_id)
            else {
                ui.vertical_centered(|ui| {
                    ui.add_space((ui.available_height() / 2.0 - 60.0).max(24.0));
                    ui.strong("Nothing to show yet");
                    ui.weak("Notes are encrypted and saved automatically as you type.");
                    ui.add_space(10.0);
                    if style::primary_button(ui, "+ Create a note").clicked() {
                        add_note(&mut store.data.notes);
                        self.transient.focus_note_title = true;
                        changed = true;
                        ui.ctx().request_repaint();
                    }
                });
                return;
            };
            ui.horizontal(|ui| {
                let controls = 90.0 + ui.spacing().interact_size.y + ui.spacing().item_spacing.x;
                let title = ui.add(
                    egui::TextEdit::singleline(&mut note.title)
                        .id(egui::Id::new(("note_title", &note.id)))
                        .font(egui::TextStyle::Heading)
                        .hint_text("Untitled")
                        .desired_width((ui.available_width() - controls).max(100.0)),
                );
                if std::mem::take(&mut self.transient.focus_note_title) {
                    title.request_focus();
                }
                // Enter in the title continues into the note itself.
                // Move to the body on the next frame, so it never receives the
                // keystrokes (including this Enter) that were meant for the title.
                if title.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                    self.transient.focus_note_body = true;
                    ui.ctx().request_repaint();
                }
                if title.changed() {
                    note.updated_at = model::now();
                    changed = true;
                }
                let pin = if note.pinned { "Unpin" } else { "Pin to top" };
                if style::star_button(ui, note.pinned, pin).clicked() {
                    note.pinned = !note.pinned;
                    changed = true;
                }
                if style::danger_button(ui, "Delete…").clicked() {
                    delete = Some(note.id.clone());
                }
            });
            ui.add_space(4.0);
            // Reserve the footer first so the editor fills the space above it.
            let footer_height = 22.0;
            egui::ScrollArea::vertical()
                .id_salt("note_editor_scroll")
                .max_height((ui.available_height() - footer_height).max(120.0))
                .show(ui, |ui| {
                    if focus_body {
                        ui.memory_mut(|memory| memory.request_focus(content_id(&note.id)));
                    }
                    if ui
                        .add_sized(
                            [ui.available_width(), ui.available_height().max(300.0)],
                            egui::TextEdit::multiline(&mut note.content)
                                .id(content_id(&note.id))
                                .font(egui::FontId::proportional(16.0))
                                .desired_width(f32::INFINITY)
                                .margin(egui::Margin::same(10))
                                .hint_text("Write something worth keeping…"),
                        )
                        .changed()
                    {
                        note.updated_at = model::now();
                        changed = true;
                    }
                });
            ui.horizontal(|ui| {
                let words = note.content.split_whitespace().count();
                let characters = note.content.chars().count();
                ui.weak(format!(
                    "{words} {} · {characters} {}",
                    if words == 1 { "word" } else { "words" },
                    if characters == 1 {
                        "character"
                    } else {
                        "characters"
                    },
                ));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.weak(format!("Edited {}", local_time(&note.updated_at)));
                });
            });
        });
        if changed {
            self.changed();
        }
        if let Some(id) = delete {
            self.dialog = Some(Dialog::DeleteNote(id));
        }
        if sort != self.settings.note_sort {
            self.settings.note_sort = sort;
            self.persist_settings();
        }
    }
}

fn content_id(note_id: &str) -> egui::Id {
    egui::Id::new(("note_content", note_id))
}

/// New notes start with an empty title, so typing replaces the "Untitled"
/// placeholder instead of appending to it.
fn add_note(notes: &mut rakshamemo::model::Notes) {
    let note = Note::new("");
    notes.active_id = note.id.clone();
    notes.items.push(note);
}

/// A full-width, two-line list entry: the title and a short muted preview.
fn note_row(ui: &mut egui::Ui, note: &Note, selected: bool) -> egui::Response {
    let title = if note.title.trim().is_empty() {
        "Untitled"
    } else {
        note.title.as_str()
    };
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), LIST_ROW_HEIGHT),
        egui::Sense::click(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, title)
    });
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let visuals = ui.visuals();
    let fill = if selected {
        Some(visuals.selection.bg_fill)
    } else if response.hovered() || response.has_focus() {
        Some(visuals.widgets.hovered.weak_bg_fill)
    } else {
        None
    };
    if let Some(fill) = fill {
        ui.painter().rect_filled(rect, 6.0, fill);
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            6.0,
            egui::Stroke::new(1.5, visuals.widgets.hovered.bg_stroke.color),
            egui::StrokeKind::Inside,
        );
    }
    let mut inner = rect.shrink2(egui::vec2(10.0, 5.0));
    if note.pinned {
        inner.max.x -= 14.0;
    }
    let text_color = visuals.text_color();
    let weak_color = visuals.weak_text_color();
    let title = egui::WidgetText::from(RichText::new(title).strong()).into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        inner.width(),
        egui::TextStyle::Body,
    );
    let detail = preview(&note.content);
    let detail = format!(
        "{}{}{}",
        short_date(&note.updated_at),
        if detail.is_empty() { "" } else { "  " },
        detail
    );
    let detail = egui::WidgetText::from(RichText::new(detail).small()).into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        inner.width(),
        egui::TextStyle::Small,
    );
    let detail_pos = egui::pos2(inner.left(), inner.bottom() - detail.size().y);
    if note.pinned {
        let star = egui::pos2(inner.right() - 6.0, inner.top() + 8.0);
        style::paint_star(ui.painter(), star, 5.5, visuals.warn_fg_color, true);
    }
    ui.painter().galley(inner.left_top(), title, text_color);
    ui.painter().galley(detail_pos, detail, weak_color);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The first few words of a note, with whitespace collapsed onto one line.
fn preview(content: &str) -> String {
    // Scan a bounded prefix, so huge notes cost the same as short ones.
    let end = content
        .char_indices()
        .nth(PREVIEW_CHARS * 4)
        .map_or(content.len(), |(index, _)| index);
    let mut preview = String::new();
    let mut length = 0;
    for word in content[..end].split_whitespace() {
        if length > 0 {
            preview.push(' ');
            length += 1;
        }
        let word_length = word.chars().count();
        if length + word_length > PREVIEW_CHARS {
            preview.extend(word.chars().take(PREVIEW_CHARS.saturating_sub(length)));
            preview.push('…');
            break;
        }
        preview.push_str(word);
        length += word_length;
    }
    preview
}

fn parse_local(timestamp: &str) -> Option<chrono::DateTime<chrono::Local>> {
    chrono::DateTime::parse_from_rfc3339(timestamp)
        .ok()
        .map(|date| date.with_timezone(&chrono::Local))
}

/// Today's notes show a time; older notes show a date.
fn short_date(timestamp: &str) -> String {
    match parse_local(timestamp) {
        Some(date) if date.date_naive() == chrono::Local::now().date_naive() => {
            date.format("%H:%M").to_string()
        }
        Some(date) => date.format("%Y-%m-%d").to_string(),
        None => String::new(),
    }
}

fn local_time(timestamp: &str) -> String {
    parse_local(timestamp)
        .map(|date| date.format("%Y-%m-%d at %H:%M").to_string())
        .unwrap_or_else(|| timestamp.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews_are_single_line_and_bounded() {
        assert_eq!(
            preview("  First line\n\n second\tline  "),
            "First line second line"
        );
        assert_eq!(preview(""), "");
        let long = "語".repeat(10_000);
        let bounded = preview(&long);
        assert!(bounded.chars().count() <= PREVIEW_CHARS + 1);
        assert!(bounded.ends_with('…'));
        let words = "word ".repeat(1_000);
        assert!(preview(&words).chars().count() <= PREVIEW_CHARS + 1);
    }

    fn note(title: &str, updated_at: &str, pinned: bool) -> Note {
        Note {
            updated_at: updated_at.into(),
            pinned,
            ..Note::new(title)
        }
    }

    #[test]
    fn pinned_notes_lead_every_sort_order() {
        let notes = vec![
            note("banana", "2026-10-01T00:00:00Z", false),
            note("apple", "2026-10-03T00:00:00Z", false),
            note("cherry", "2026-10-02T00:00:00Z", true),
        ];
        let sorted = |sort| {
            let mut order = vec![0, 1, 2];
            sort_notes(&mut order, &notes, sort);
            order
        };
        assert_eq!(sorted(NoteSort::RecentlyEdited), [2, 1, 0]);
        assert_eq!(sorted(NoteSort::Title), [2, 1, 0]);
        assert_eq!(sorted(NoteSort::Created), [2, 1, 0]);
        let notes = vec![
            note("Beta", "2026-10-03T00:00:00Z", false),
            note("alpha", "2026-10-01T00:00:00+09:00", false),
        ];
        let mut order = vec![0, 1];
        sort_notes(&mut order, &notes, NoteSort::Title);
        assert_eq!(order, [1, 0], "Titles sort case-insensitively");
    }

    #[test]
    fn search_cache_rebuilds_only_when_inputs_change() {
        let mut notes = vec![note("Groceries", "", false), note("Plans", "", false)];
        notes[1].content = "Buy 日本語 books".into();
        let mut cache = SearchCache::default();
        assert_eq!(cache.matches(&notes, "日本語", 1), [1]);
        notes[0].content = "日本語 too".into();
        assert_eq!(
            cache.matches(&notes, "日本語", 1),
            [1],
            "Same revision reuses the result"
        );
        assert_eq!(cache.matches(&notes, "日本語", 2), [0, 1]);
        assert_eq!(cache.matches(&notes, "", 2), [0, 1]);
        notes.pop();
        assert_eq!(
            cache.matches(&notes, "", 2),
            [0],
            "Removing a note invalidates"
        );
    }

    #[test]
    fn dates_fall_back_for_unparseable_timestamps() {
        assert_eq!(short_date("not a date"), "");
        assert_eq!(local_time("not a date"), "not a date");
        assert_eq!(short_date("2020-01-02T03:04:05Z").len(), 10);
    }
}
