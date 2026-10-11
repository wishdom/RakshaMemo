//! A compact credential browser with one live inline editor. Only visible rows
//! create widgets; passwords stay masked unless the user reveals a row.

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use rakshamemo::model::PasswordItem;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use zeroize::Zeroizing;

use super::style;

const ROW_HEIGHT: f32 = 32.0;
const HEADER_HEIGHT: f32 = 36.0;
const MIN_TABLE_WIDTH: f32 = 860.0;
const EXTRA_EDIT_HEIGHT: f32 = 128.0;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub(super) enum Field {
    Title,
    Username,
    Password,
    Category,
    Extra,
}

struct InlineEditor {
    id: String,
    field: Field,
    value: Zeroizing<String>,
    request_focus: bool,
    context: Option<egui::Context>,
}

impl InlineEditor {
    fn widget_id(&self) -> egui::Id {
        egui::Id::new(("password_inline", &self.id, self.field))
    }
}

impl Drop for InlineEditor {
    fn drop(&mut self) {
        // Release egui's undo snapshots as soon as this editing session ends.
        if let Some(context) = &self.context {
            context.data_mut(|data| {
                data.remove::<egui::text_edit::TextEditState>(self.widget_id());
            });
            context.memory_mut(|memory| memory.surrender_focus(self.widget_id()));
        }
    }
}

#[derive(Clone, Copy, Default, Hash, PartialEq, Eq)]
enum SortColumn {
    #[default]
    Title,
    Username,
    Category,
    Updated,
}

pub(super) struct TableState {
    sort: SortColumn,
    descending: bool,
    selected: Option<String>,
    fingerprint: Option<u64>,
    rows: Vec<usize>,
    show_category: bool,
    show_updated: bool,
    active: Option<InlineEditor>,
    revealed: HashSet<String>,
    membership: Option<u64>,
    view_fingerprint: Option<u64>,
    reset_scroll: bool,
    next_edit: Option<(String, Field)>,
}

impl Default for TableState {
    fn default() -> Self {
        Self {
            sort: SortColumn::default(),
            descending: false,
            selected: None,
            fingerprint: None,
            rows: Vec::new(),
            show_category: true,
            show_updated: false,
            active: None,
            revealed: HashSet::new(),
            membership: None,
            view_fingerprint: None,
            reset_scroll: true,
            next_edit: None,
        }
    }
}

#[derive(PartialEq, Eq)]
pub(super) enum Action {
    New,
    ManageCategories,
    Edit(String),
    Delete(String),
    CopyUsername(String),
    CopyPassword(String),
    Update {
        id: String,
        field: Field,
        value: Zeroizing<String>,
    },
    FinishCategory(String),
}

impl std::fmt::Debug for Action {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::New => formatter.write_str("New"),
            Self::ManageCategories => formatter.write_str("ManageCategories"),
            Self::Edit(id) => formatter.debug_tuple("Edit").field(id).finish(),
            Self::Delete(id) => formatter.debug_tuple("Delete").field(id).finish(),
            Self::CopyUsername(id) => formatter.debug_tuple("CopyUsername").field(id).finish(),
            Self::CopyPassword(id) => formatter.debug_tuple("CopyPassword").field(id).finish(),
            Self::FinishCategory(id) => formatter.debug_tuple("FinishCategory").field(id).finish(),
            Self::Update { id, field, .. } => formatter
                .debug_struct("Update")
                .field("id", id)
                .field("field", field)
                .field("value", &"[REDACTED]")
                .finish(),
        }
    }
}

impl TableState {
    pub(super) fn finish_edit(&mut self) -> Option<Action> {
        let editor = self.active.take()?;
        self.fingerprint = None;
        (editor.field == Field::Category).then(|| Action::FinishCategory(editor.id.clone()))
    }

    pub(super) fn clear_sensitive(&mut self) {
        self.active = None;
        self.revealed.clear();
        self.fingerprint = None;
        self.next_edit = None;
    }

    fn begin_edit(&mut self, item: &PasswordItem, field: Field, actions: &mut Vec<Action>) {
        if let Some(action) = self.finish_edit() {
            actions.push(action);
        }
        let value = match field {
            Field::Title => &item.title,
            Field::Username => &item.username,
            Field::Password => &item.password,
            Field::Category => &item.kind,
            Field::Extra => &item.extra,
        };
        self.selected = Some(item.id.clone());
        self.active = Some(InlineEditor {
            id: item.id.clone(),
            field,
            value: Zeroizing::new(value.clone()),
            request_focus: true,
            context: None,
        });
    }

    /// Cache row indices, not credentials or lowercased copies of private data.
    /// A cheap fingerprint allows edits, imports, and category renames to refresh
    /// the view without sorting or allocating for every mouse movement.
    fn refresh(&mut self, items: &[PasswordItem], query: &str, category: &str) -> bool {
        let mut view = std::collections::hash_map::DefaultHasher::new();
        (query, category, self.sort, self.descending).hash(&mut view);
        let view = view.finish();
        if self.view_fingerprint != Some(view) {
            self.reset_scroll = true;
            self.view_fingerprint = Some(view);
        }
        let mut membership = std::collections::hash_map::DefaultHasher::new();
        items.len().hash(&mut membership);
        for item in items {
            item.id.hash(&mut membership);
        }
        let membership = membership.finish();
        // Keep the edited row in place while its sort/filter values change.
        // Structural changes still rebuild indices before any row is accessed.
        if self.active.is_some() && self.membership == Some(membership) {
            return false;
        }
        if self.membership != Some(membership) {
            self.active = None;
            self.revealed
                .retain(|id| items.iter().any(|item| item.id == *id));
        }
        self.membership = Some(membership);
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        query.hash(&mut hash);
        category.hash(&mut hash);
        self.sort.hash(&mut hash);
        self.descending.hash(&mut hash);
        items.len().hash(&mut hash);
        for item in items {
            item.id.hash(&mut hash);
            if !query.is_empty() {
                item.title.hash(&mut hash);
                item.username.hash(&mut hash);
                item.kind.hash(&mut hash);
                item.extra.hash(&mut hash);
            }
            if !category.is_empty() {
                item.kind.hash(&mut hash);
            }
            match self.sort {
                SortColumn::Title => &item.title,
                SortColumn::Username => &item.username,
                SortColumn::Category => &item.kind,
                SortColumn::Updated => &item.updated_at,
            }
            .hash(&mut hash);
        }
        let fingerprint = hash.finish();
        if self.fingerprint == Some(fingerprint) {
            return false;
        }
        self.fingerprint = Some(fingerprint);
        let needle = query.trim().to_lowercase();
        self.rows = items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                (category.is_empty() || item.kind == category)
                    && (needle.is_empty()
                        || [&item.title, &item.username, &item.kind, &item.extra]
                            .iter()
                            .any(|value| value.to_lowercase().contains(&needle)))
            })
            .map(|(index, _)| index)
            .collect();
        if self.sort == SortColumn::Updated {
            self.rows.sort_by_cached_key(|index| {
                chrono::DateTime::parse_from_rfc3339(&items[*index].updated_at)
                    .map(|timestamp| timestamp.timestamp_millis())
                    .unwrap_or(i64::MIN)
            });
        } else {
            self.rows.sort_by_cached_key(|index| {
                let item = &items[*index];
                match self.sort {
                    SortColumn::Title => &item.title,
                    SortColumn::Username => &item.username,
                    SortColumn::Category => &item.kind,
                    SortColumn::Updated => unreachable!(),
                }
                .to_lowercase()
            });
        }
        if self.descending {
            self.rows.reverse();
        }
        if self
            .selected
            .as_ref()
            .is_some_and(|id| !self.rows.iter().any(|index| items[*index].id == *id))
        {
            self.selected = None;
        }
        true
    }

    fn sort_by(&mut self, column: SortColumn) {
        if self.sort == column {
            self.descending = !self.descending;
        } else {
            self.sort = column;
            self.descending = column == SortColumn::Updated;
        }
    }
}

pub(super) fn show(
    ui: &mut egui::Ui,
    items: &[PasswordItem],
    types: &[String],
    query: &mut String,
    category: &mut String,
    state: &mut TableState,
    focus_search: bool,
) -> Vec<Action> {
    style::page(ui);
    let mut actions = Vec::new();
    let mut filters_changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
        let search_width = (ui.available_width() - 505.0).clamp(200.0, 440.0);
        let search = ui
            .add(
                egui::TextEdit::singleline(query)
                    .hint_text("Search passwords…")
                    .margin(egui::vec2(8.0, 6.0))
                    .min_size(egui::vec2(0.0, 32.0))
                    .desired_width(search_width),
            )
            .on_hover_text(
                "Search titles, usernames, categories, and extra information (Ctrl/Cmd+F)",
            );
        if focus_search {
            search.request_focus();
        }
        filters_changed |= search.changed();
        egui::ComboBox::from_id_salt("password_category")
            .width(155.0)
            .selected_text(if category.is_empty() {
                "All categories"
            } else {
                category.as_str()
            })
            .show_ui(ui, |ui| {
                ui.set_max_width(330.0);
                ui.label("Categories group your passwords, such as Work or Personal.");
                ui.separator();
                filters_changed |= ui
                    .selectable_value(category, String::new(), "All categories")
                    .changed();
                for kind in types {
                    filters_changed |= ui.selectable_value(category, kind.clone(), kind).changed();
                }
                ui.separator();
                if ui.button("Manage categories…").clicked() {
                    actions.push(Action::ManageCategories);
                    ui.close();
                }
            });
        ui.menu_button("Columns", |ui| {
            filters_changed |= ui.checkbox(&mut state.show_category, "Category").changed();
            filters_changed |= ui
                .checkbox(&mut state.show_updated, "Updated date")
                .changed();
        });
        if matches!(state.sort, SortColumn::Category) && !state.show_category
            || matches!(state.sort, SortColumn::Updated) && !state.show_updated
        {
            state.sort = SortColumn::Title;
            state.descending = false;
        }
        if (!query.is_empty() || !category.is_empty()) && ui.button("Clear").clicked() {
            query.clear();
            category.clear();
            filters_changed = true;
        }
        if style::primary_button(ui, "+ New password")
            .on_hover_text("Ctrl/Cmd+N")
            .clicked()
        {
            actions.push(Action::New);
        }
    });
    if filters_changed && let Some(action) = state.finish_edit() {
        actions.push(action);
    }
    state.refresh(items, query, category);
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let count = state.rows.len();
        ui.label(
            egui::RichText::new(if count == items.len() {
                format!(
                    "{count} {}",
                    if count == 1 { "password" } else { "passwords" }
                )
            } else {
                format!("{count} of {} passwords", items.len())
            })
            .weak(),
        );
        if ui.available_width() >= 500.0 {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new("Click a value to edit · Changes save automatically")
                        .weak(),
                );
            });
        }
    });
    ui.add_space(5.0);
    if state.rows.is_empty() {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_min_size(egui::vec2(ui.available_width(), 160.0));
            ui.vertical_centered(|ui| {
                ui.add_space(32.0);
                if items.is_empty() {
                    ui.strong("Your passwords, neatly organized");
                    ui.weak("Add an account to keep its login details here.");
                    ui.add_space(10.0);
                    if style::primary_button(ui, "+ Add your first password").clicked() {
                        actions.push(Action::New);
                    }
                } else {
                    ui.strong("No matching passwords");
                    ui.weak("Try another search or category.");
                    ui.add_space(10.0);
                    if ui.button("Clear filters").clicked() {
                        query.clear();
                        category.clear();
                    }
                }
            });
        });
        return actions;
    }

    let min_width = MIN_TABLE_WIDTH + if state.show_updated { 115.0 } else { 0.0 }
        - if state.show_category { 0.0 } else { 100.0 };
    let table_width = ui.available_width().max(min_width);
    let table_height = (ui.available_height() - 18.0).max(80.0);
    let mut next_sort = None;
    let mut selection = None;
    let mut finish_requested = None;
    egui::ScrollArea::horizontal()
        .id_salt("password_table_horizontal")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_min_width(table_width);
            ui.spacing_mut().item_spacing = egui::vec2(9.0, 0.0);
            ui.style_mut().interaction.selectable_labels = false;
            let mut table = TableBuilder::new(ui)
                .id_salt(("password_table", state.show_category, state.show_updated))
                .striped(true)
                .resizable(true)
                .sense(egui::Sense::click())
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .auto_shrink([false, false])
                .min_scrolled_height(40.0)
                .max_scroll_height(table_height - HEADER_HEIGHT)
                .column(Column::initial(165.0).at_least(130.0).clip(true))
                .column(Column::initial(190.0).at_least(135.0).clip(true))
                .column(Column::initial(158.0).at_least(150.0).clip(true));
            if state.show_category {
                table = table.column(Column::initial(110.0).at_least(90.0).clip(true));
            }
            table = table.column(Column::remainder().at_least(140.0).clip(true));
            if state.show_updated {
                table = table.column(Column::initial(105.0).at_least(100.0).clip(true));
            }
            table = table.column(Column::exact(98.0));
            if std::mem::take(&mut state.reset_scroll) {
                table = table.vertical_scroll_offset(0.0);
            }
            table
                .header(HEADER_HEIGHT, |mut row| {
                    let mut columns = vec![
                        ("Title", Some(SortColumn::Title)),
                        ("Username", Some(SortColumn::Username)),
                        ("Password", None),
                    ];
                    if state.show_category {
                        columns.push(("Category", Some(SortColumn::Category)));
                    }
                    columns.push(("Extra information", None));
                    if state.show_updated {
                        columns.push(("Updated", Some(SortColumn::Updated)));
                    }
                    columns.push(("Actions", None));
                    for (label, column) in columns {
                        row.col(|ui| {
                            if let Some(column) = column {
                                if ui
                                    .add(
                                        egui::Button::new(egui::RichText::new(label).strong())
                                            .frame(false),
                                    )
                                    .on_hover_text(format!("Sort by {}", label.to_lowercase()))
                                    .clicked()
                                {
                                    next_sort = Some(column);
                                }
                                if state.sort == column {
                                    let (rect, _) = ui.allocate_exact_size(
                                        egui::vec2(8.0, 8.0),
                                        egui::Sense::hover(),
                                    );
                                    let direction = if state.descending { 1.0 } else { -1.0 };
                                    let center = rect.center();
                                    let points = [
                                        center + egui::vec2(-3.0, -direction * 2.0),
                                        center + egui::vec2(0.0, direction * 2.0),
                                        center + egui::vec2(3.0, -direction * 2.0),
                                    ];
                                    ui.painter().add(egui::Shape::line(
                                        points.to_vec(),
                                        egui::Stroke::new(1.2, ui.visuals().text_color()),
                                    ));
                                }
                            } else {
                                ui.strong(label);
                            }
                        });
                    }
                })
                .body(|body| {
                    let expanded_row = state
                        .active
                        .as_ref()
                        .filter(|editor| editor.field == Field::Extra)
                        .and_then(|editor| {
                            state
                                .rows
                                .iter()
                                .position(|index| items[*index].id == editor.id)
                        });
                    let heights = (0..state.rows.len()).map(|index| {
                        if Some(index) == expanded_row {
                            EXTRA_EDIT_HEIGHT
                        } else {
                            ROW_HEIGHT
                        }
                    });
                    body.heterogeneous_rows(heights, |mut row| {
                        let item = &items[state.rows[row.index()]];
                        row.set_selected(state.selected.as_deref() == Some(item.id.as_str()));
                        row.col(|ui| {
                            editable_cell(
                                ui,
                                ui.available_width(),
                                item,
                                Field::Title,
                                state,
                                &mut actions,
                                &mut finish_requested,
                            );
                        });
                        row.col(|ui| {
                            let width = (ui.available_width() - 37.0).max(0.0);
                            editable_cell(
                                ui,
                                width,
                                item,
                                Field::Username,
                                state,
                                &mut actions,
                                &mut finish_requested,
                            );
                            if copy_button(ui, "Copy username").clicked() {
                                actions.push(Action::CopyUsername(item.id.clone()));
                            }
                        });
                        row.col(|ui| {
                            let width = (ui.available_width() - 74.0).max(0.0);
                            editable_cell(
                                ui,
                                width,
                                item,
                                Field::Password,
                                state,
                                &mut actions,
                                &mut finish_requested,
                            );
                            let revealed = state.revealed.contains(&item.id);
                            if eye_button(ui, revealed).clicked() {
                                if revealed {
                                    state.revealed.remove(&item.id);
                                } else {
                                    state.revealed.insert(item.id.clone());
                                }
                                ui.ctx().request_repaint();
                            }
                            if copy_button(ui, "Copy password").clicked() {
                                actions.push(Action::CopyPassword(item.id.clone()));
                            }
                        });
                        if state.show_category {
                            row.col(|ui| {
                                editable_cell(
                                    ui,
                                    ui.available_width(),
                                    item,
                                    Field::Category,
                                    state,
                                    &mut actions,
                                    &mut finish_requested,
                                );
                            });
                        }
                        row.col(|ui| {
                            editable_cell(
                                ui,
                                ui.available_width(),
                                item,
                                Field::Extra,
                                state,
                                &mut actions,
                                &mut finish_requested,
                            );
                        });
                        if state.show_updated {
                            row.col(|ui| {
                                let date = item.updated_at.get(..10).unwrap_or(&item.updated_at);
                                ui.add(egui::Label::new(date).truncate())
                                    .on_hover_text(&item.updated_at);
                            });
                        }
                        row.col(|ui| {
                            ui.spacing_mut().item_spacing.x = 5.0;
                            if ui.button("Edit").clicked() {
                                actions.push(Action::Edit(item.id.clone()));
                            }
                            if delete_button(ui).clicked() {
                                actions.push(Action::Delete(item.id.clone()));
                            }
                        });
                        let response = row.response();
                        if response.clicked() {
                            selection = Some(item.id.clone());
                        }
                    });
                });
        });
    if let Some(column) = next_sort {
        if let Some(action) = state.finish_edit() {
            actions.push(action);
        }
        state.sort_by(column);
        ui.ctx().request_repaint();
    }
    if let Some(id) = selection {
        state.selected = Some(id);
        ui.ctx().request_repaint();
    }
    if finish_requested.is_some_and(|(id, field)| {
        state
            .active
            .as_ref()
            .is_some_and(|editor| editor.id == id && editor.field == field)
    }) && let Some(action) = state.finish_edit()
    {
        actions.push(action);
    }
    if let Some((id, field)) = state.next_edit.take()
        && let Some(item) = items.iter().find(|item| item.id == id)
    {
        // Render the old editor first, so its final keystroke is emitted before
        // category registration or replacing its buffer with another cell.
        state.begin_edit(item, field, &mut actions);
    }
    actions
}

/// Bound formatting to the visible preview, even for very large pasted notes.
/// Whitespace is collapsed without introducing another font or multiline rows.
fn extra_preview(extra: &str) -> String {
    let mut preview = String::new();
    let mut length = 0;
    let mut whitespace = false;
    for character in extra.chars() {
        if character.is_whitespace() {
            whitespace = !preview.is_empty();
            continue;
        }
        if length >= 180 {
            preview.push('…');
            break;
        }
        if whitespace {
            preview.push(' ');
            length += 1;
            whitespace = false;
        }
        preview.push(character);
        length += 1;
    }
    preview
}

fn extra_tooltip(extra: &str) -> String {
    let mut characters = extra.chars();
    let mut tooltip: String = characters.by_ref().take(1_200).collect();
    if characters.next().is_some() {
        tooltip.push_str("…\n\nOpen this password to read all extra information.");
    }
    tooltip
}

fn editable_cell(
    ui: &mut egui::Ui,
    width: f32,
    item: &PasswordItem,
    field: Field,
    state: &mut TableState,
    actions: &mut Vec<Action>,
    finish_requested: &mut Option<(String, Field)>,
) {
    let is_active = state
        .active
        .as_ref()
        .is_some_and(|editor| editor.id == item.id && editor.field == field);
    let height = if is_active && field == Field::Extra {
        EXTRA_EDIT_HEIGHT - 8.0
    } else {
        28.0
    };
    ui.allocate_ui_with_layout(
        egui::vec2(width, height),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_width(width);
            ui.set_max_width(width);
            if is_active {
                let revealed = state.revealed.contains(&item.id);
                let editor = state.active.as_mut().expect("Active inline editor");
                let id = editor.widget_id();
                editor.context = Some(ui.ctx().clone());
                let finish_shortcut = ui.memory(|memory| memory.has_focus(id))
                    && ui.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                            || (field == Field::Extra
                                && input.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter))
                    });
                let response = if field == Field::Extra {
                    egui::ScrollArea::vertical()
                        .id_salt(("inline_extra_scroll", &item.id))
                        .max_height(height)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut *editor.value)
                                    .id(id)
                                    .font(egui::TextStyle::Body)
                                    .desired_width(width)
                                    .desired_rows(5)
                                    .margin(egui::vec2(4.0, 4.0)),
                            )
                        })
                        .inner
                        .on_hover_text(
                            "Changes save automatically. Ctrl+Enter or Escape finishes editing.",
                        )
                } else {
                    ui.add_sized(
                        egui::vec2(width, 28.0),
                        egui::TextEdit::singleline(&mut *editor.value)
                            .id(id)
                            .font(egui::TextStyle::Body)
                            .password(field == Field::Password && !revealed)
                            .margin(egui::vec2(4.0, 4.0)),
                    )
                };
                if editor.request_focus {
                    response.request_focus();
                    editor.request_focus = false;
                }
                if response.changed() {
                    actions.push(Action::Update {
                        id: item.id.clone(),
                        field,
                        value: editor.value.clone(),
                    });
                }
                if response.lost_focus() || finish_shortcut {
                    *finish_requested = Some((item.id.clone(), field));
                }
            } else {
                let revealed = state.revealed.contains(&item.id);
                let text = match field {
                    Field::Title => {
                        if item.title.is_empty() {
                            "Untitled"
                        } else {
                            &item.title
                        }
                    }
                    Field::Username => {
                        if item.username.is_empty() {
                            "—"
                        } else {
                            &item.username
                        }
                    }
                    Field::Password => {
                        if revealed {
                            &item.password
                        } else {
                            "••••••••"
                        }
                    }
                    Field::Category => {
                        if item.kind.is_empty() {
                            "—"
                        } else {
                            &item.kind
                        }
                    }
                    Field::Extra => &item.extra,
                };
                let preview;
                let text = if field == Field::Extra {
                    preview = extra_preview(text);
                    if preview.is_empty() { "—" } else { &preview }
                } else {
                    text
                };
                let text = if field == Field::Password && !revealed {
                    egui::RichText::new(text).weak()
                } else {
                    egui::RichText::new(text)
                };
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(width, 28.0), egui::Sense::click());
                let galley = egui::WidgetText::from(text).into_galley(
                    ui,
                    Some(egui::TextWrapMode::Truncate),
                    width,
                    egui::TextStyle::Body,
                );
                let position = rect.left_center() - egui::vec2(0.0, galley.size().y / 2.0);
                ui.painter()
                    .galley(position, galley, ui.visuals().text_color());
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        ui.is_enabled(),
                        match field {
                            Field::Title => "Edit title",
                            Field::Username => "Edit username",
                            Field::Password => "Edit password",
                            Field::Category => "Edit category",
                            Field::Extra => "Edit extra information",
                        },
                    )
                });
                let response = response.on_hover_cursor(egui::CursorIcon::Text);
                if response.hovered() || response.has_focus() {
                    ui.painter().rect_stroke(
                        response.rect.expand(1.0),
                        3.0,
                        egui::Stroke::new(1.0, ui.visuals().widgets.hovered.bg_stroke.color),
                        egui::StrokeKind::Inside,
                    );
                }
                let response = if field == Field::Password && revealed {
                    response.on_hover_ui(|ui| {
                        ui.set_max_width(480.0);
                        let mut characters = item.password.chars();
                        let preview =
                            Zeroizing::new(characters.by_ref().take(512).collect::<String>());
                        ui.add(egui::Label::new(preview.as_str()).wrap());
                        if characters.next().is_some() {
                            ui.weak("Open Edit to see the full value.");
                        } else {
                            ui.weak("Click to edit · Use the eye to hide");
                        }
                    })
                } else if field == Field::Extra && !item.extra.trim().is_empty() {
                    response.on_hover_ui(|ui| {
                        ui.set_max_width(480.0);
                        ui.label(extra_tooltip(&item.extra));
                        ui.weak("Click to edit here. Use Edit for the full page.");
                    })
                } else {
                    response.on_hover_text("Click to edit · Changes save automatically")
                };
                if response.clicked() {
                    state.next_edit = Some((item.id.clone(), field));
                    ui.ctx().request_repaint();
                }
            }
        },
    );
}

fn icon_button(ui: &mut egui::Ui, label: &str) -> (egui::Rect, egui::Response, egui::Stroke) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    let visuals = ui.style().interact(&response);
    if response.hovered() || response.has_focus() {
        ui.painter().rect_filled(rect, 4.0, visuals.weak_bg_fill);
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            4.0,
            egui::Stroke::new(1.5, ui.visuals().widgets.active.bg_stroke.color),
            egui::StrokeKind::Inside,
        );
    }
    let stroke = egui::Stroke::new(1.2, visuals.fg_stroke.color);
    (rect, response.on_hover_text(label), stroke)
}

fn copy_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let (rect, response, stroke) = icon_button(ui, label);
    let center = rect.center();
    for offset in [egui::vec2(-2.0, -2.0), egui::vec2(1.0, 1.0)] {
        let icon = egui::Rect::from_center_size(center + offset, egui::vec2(8.0, 10.0));
        ui.painter().rect(
            icon,
            1.5,
            ui.visuals().panel_fill,
            stroke,
            egui::StrokeKind::Inside,
        );
    }
    response
}

fn eye_button(ui: &mut egui::Ui, revealed: bool) -> egui::Response {
    let (rect, response, stroke) = icon_button(
        ui,
        if revealed {
            "Hide password"
        } else {
            "Show password"
        },
    );
    let center = rect.center();
    let points = [
        (-8.0, 0.0),
        (-5.0, -3.5),
        (0.0, -5.0),
        (5.0, -3.5),
        (8.0, 0.0),
        (5.0, 3.5),
        (0.0, 5.0),
        (-5.0, 3.5),
        (-8.0, 0.0),
    ]
    .into_iter()
    .map(|(x, y)| center + egui::vec2(x, y))
    .collect();
    ui.painter().add(egui::Shape::line(points, stroke));
    ui.painter().circle_stroke(center, 2.2, stroke);
    if revealed {
        ui.painter().line_segment(
            [
                center + egui::vec2(-7.0, -7.0),
                center + egui::vec2(7.0, 7.0),
            ],
            stroke,
        );
    }
    response
}

fn delete_button(ui: &mut egui::Ui) -> egui::Response {
    style::danger(ui, |ui| {
        let (rect, response, stroke) = icon_button(ui, "Delete password…");
        let center = rect.center();
        ui.painter().rect_stroke(
            egui::Rect::from_center_size(center + egui::vec2(0.0, 1.5), egui::vec2(8.0, 9.0)),
            1.5,
            stroke,
            egui::StrokeKind::Inside,
        );
        for (y, half_width) in [(-4.5, 5.0), (-6.5, 2.0)] {
            ui.painter().line_segment(
                [
                    center + egui::vec2(-half_width, y),
                    center + egui::vec2(half_width, y),
                ],
                stroke,
            );
        }
        response
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, title: &str, username: &str, kind: &str) -> PasswordItem {
        PasswordItem {
            id: id.into(),
            title: title.into(),
            username: username.into(),
            kind: kind.into(),
            password: "never render or search this secret".into(),
            ..Default::default()
        }
    }

    #[test]
    fn filters_unicode_metadata_but_never_searches_passwords() {
        let mut items = vec![
            entry("b", "銀行", "Älice", "Banking"),
            entry("a", "Email", "bob", "Work"),
        ];
        items[1].extra = "Recovery account".into();
        let mut state = TableState::default();
        state.refresh(&items, "äLICE", "Banking");
        assert_eq!(state.rows, [0]);
        state.refresh(&items, "銀行", "");
        assert_eq!(state.rows, [0]);
        state.refresh(&items, " recovery ", "Work");
        assert_eq!(state.rows, [1]);
        items[1].extra = "Personal account".into();
        assert!(state.refresh(&items, " recovery ", "Work"));
        assert!(state.rows.is_empty());
        state.refresh(&items, "secret", "");
        assert!(state.rows.is_empty());
    }

    #[test]
    fn sorts_case_insensitively_and_invalidates_after_edits() {
        let mut items = vec![
            entry("z", "zebra", "alpha", "Work"),
            entry("a", "ALPHA", "zed", "Banking"),
        ];
        let mut state = TableState::default();
        assert!(state.refresh(&items, "", ""));
        assert_eq!(state.rows, [1, 0]);
        assert!(!state.refresh(&items, "", ""));
        state.sort_by(SortColumn::Username);
        state.refresh(&items, "", "");
        assert_eq!(state.rows, [0, 1]);
        state.sort_by(SortColumn::Username);
        state.refresh(&items, "", "");
        assert_eq!(state.rows, [1, 0]);
        items[0].username = "zzzz".into();
        assert!(state.refresh(&items, "", ""));
        assert_eq!(state.rows, [0, 1]);
        state.sort_by(SortColumn::Category);
        state.refresh(&items, "", "");
        assert_eq!(state.rows, [1, 0]);
        items.swap(0, 1);
        state.refresh(&items, "", "");
        assert_eq!(state.rows, [0, 1]);
    }

    #[test]
    fn updated_sort_compares_instants_across_time_zones() {
        let mut items = vec![
            entry("early", "Early", "", ""),
            entry("late", "Late", "", ""),
        ];
        items[0].updated_at = "2026-10-11T10:00:00+09:00".into();
        items[1].updated_at = "2026-10-11T02:00:00Z".into();
        let mut state = TableState::default();
        state.sort_by(SortColumn::Updated);
        state.refresh(&items, "", "");
        assert_eq!(state.rows, [1, 0]);
        state.selected = Some("late".into());
        state.refresh(&items, "Early", "");
        assert!(state.selected.is_none());
    }

    fn frame(
        context: &egui::Context,
        items: &[PasswordItem],
        state: &mut TableState,
        events: Vec<egui::Event>,
        width: f32,
    ) -> (egui::FullOutput, Vec<Action>) {
        let mut actions = Vec::new();
        let mut query = String::new();
        let mut category = String::new();
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 720.0),
                )),
                events,
                focused: true,
                ..Default::default()
            },
            |ui| {
                actions.extend(show(
                    ui,
                    items,
                    &[],
                    &mut query,
                    &mut category,
                    state,
                    false,
                ))
            },
        );
        output.textures_delta.clear();
        (output, actions)
    }

    #[test]
    fn thousands_of_passwords_render_only_visible_rows_and_keep_secrets_masked() {
        let items = (0..10_000)
            .map(|index| {
                entry(
                    &index.to_string(),
                    &format!("Account {index:05}"),
                    "user@example.test",
                    "Work",
                )
            })
            .collect::<Vec<_>>();
        let context = egui::Context::default();
        let mut state = TableState::default();
        for width in [1_100.0, 540.0] {
            let (output, actions) = frame(&context, &items, &mut state, Vec::new(), width);
            assert!(actions.is_empty());
            let texts = output
                .shapes
                .iter()
                .filter_map(|shape| {
                    if let egui::Shape::Text(text) = &shape.shape {
                        Some(text.galley.text())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            let rows = texts
                .iter()
                .filter(|text| text.starts_with("Account "))
                .count();
            assert!(
                (10..=30).contains(&rows),
                "Expected visible rows only, got {rows}"
            );
            if width >= MIN_TABLE_WIDTH {
                assert!(
                    texts.contains(&"••••••••"),
                    "Password masks must be visible"
                );
            }
            assert!(
                !texts
                    .iter()
                    .any(|text| text.contains("never render") || text.contains("this secret"))
            );
            assert!(
                output.shapes.len() < 1_000,
                "Rendering work must not grow with the vault size"
            );
        }
    }

    #[test]
    fn extra_information_is_visible_with_consistent_text_and_optional_dates() {
        let mut items = vec![entry("account", "Account", "person", "Work")];
        items[0].extra = "Recovery account\nContact support".into();
        items[0].updated_at = "2026-10-11T02:00:00Z".into();
        let context = egui::Context::default();
        let mut state = TableState::default();
        let (output, _) = frame(&context, &items, &mut state, Vec::new(), 1_100.0);
        let text_shapes = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text),
                _ => None,
            })
            .collect::<Vec<_>>();
        for expected in [
            "Title",
            "Account",
            "person",
            "Work",
            "Extra information",
            "Recovery account Contact support",
            "Edit",
        ] {
            let shape = text_shapes
                .iter()
                .find(|shape| shape.galley.text() == expected)
                .unwrap_or_else(|| panic!("Missing table text: {expected}"));
            assert!(
                shape
                    .galley
                    .job
                    .sections
                    .iter()
                    .all(|section| { section.format.font_id == egui::FontId::proportional(15.0) })
            );
        }
        assert!(!text_shapes.iter().any(|shape| {
            ["Updated", "2026-10-11", "Categories"].contains(&shape.galley.text())
        }));

        state.sort_by(SortColumn::Category);
        state.sort_by(SortColumn::Category);
        state.show_category = false;
        state.show_updated = true;
        let (output, _) = frame(&context, &items, &mut state, Vec::new(), 1_100.0);
        assert!(state.sort == SortColumn::Title && !state.descending);
        let texts = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.text()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(texts.contains(&"Updated"));
        assert!(texts.contains(&"2026-10-11"));
        assert!(!texts.contains(&"Category"));
        assert!(!texts.contains(&"Work"));
        assert!(texts.contains(&"Extra information"));
        state.sort_by(SortColumn::Updated);
        state.show_updated = false;
        frame(&context, &items, &mut state, Vec::new(), 1_100.0);
        assert!(state.sort == SortColumn::Title && !state.descending);
    }

    #[test]
    fn extra_information_preview_handles_multiline_unicode_and_bounds_large_notes() {
        assert_eq!(
            extra_preview("  日本語\n\n  backup\t account  "),
            "日本語 backup account"
        );
        let long_note = "語".repeat(10_000);
        let preview = extra_preview(&long_note);
        assert_eq!(preview.chars().count(), 181);
        assert!(preview.ends_with('…'));
        let tooltip = extra_tooltip(&long_note);
        assert!(tooltip.chars().count() < 1_300);
        assert!(tooltip.ends_with("read all extra information."));
        assert_eq!(extra_tooltip("One\nTwo"), "One\nTwo");
    }

    #[test]
    fn edit_button_returns_the_id_of_the_sorted_row() {
        let items = vec![
            entry("second", "Zebra", "", ""),
            entry("first", "Alpha", "", ""),
        ];
        let context = egui::Context::default();
        let mut state = TableState::default();
        let (output, _) = frame(&context, &items, &mut state, Vec::new(), 1_100.0);
        let position = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Edit" => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .expect("Edit button must be visible");
        let mut actions = Vec::new();
        for pressed in [true, false] {
            let (_, result) = frame(
                &context,
                &items,
                &mut state,
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                1_100.0,
            );
            actions.extend(result);
        }
        assert_eq!(actions, vec![Action::Edit("first".into())]);
    }

    fn click(
        context: &egui::Context,
        items: &[PasswordItem],
        state: &mut TableState,
        position: egui::Pos2,
    ) -> Vec<Action> {
        let mut actions = Vec::new();
        for pressed in [true, false] {
            let (_, result) = frame(
                context,
                items,
                state,
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                1_100.0,
            );
            actions.extend(result);
        }
        actions
    }

    #[test]
    fn category_filter_explains_grouping_and_opens_management() {
        let items = vec![entry("account", "Account", "person", "Work")];
        let context = egui::Context::default();
        let mut state = TableState::default();
        let text_position = |output: &egui::FullOutput, expected: &str| {
            output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == expected => {
                        Some(text.pos + text.galley.size() * 0.5)
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("Missing text: {expected}"))
        };
        let (output, _) = frame(&context, &items, &mut state, Vec::new(), 1_100.0);
        let category = text_position(&output, "All categories");
        assert!(click(&context, &items, &mut state, category).is_empty());
        let (output, _) = frame(&context, &items, &mut state, Vec::new(), 1_100.0);
        text_position(
            &output,
            "Categories group your passwords, such as Work or Personal.",
        );
        let manage = text_position(&output, "Manage categories…");
        assert_eq!(
            click(&context, &items, &mut state, manage),
            [Action::ManageCategories]
        );
    }

    #[test]
    fn clicking_a_value_edits_inline_and_typing_emits_live_changes() {
        let items = vec![entry("account", "Account", "person", "Work")];
        let context = egui::Context::default();
        let mut state = TableState::default();
        let (output, _) = frame(&context, &items, &mut state, Vec::new(), 1_100.0);
        let position = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Account" => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap();
        assert!(click(&context, &items, &mut state, position).is_empty());
        assert_eq!(state.selected.as_deref(), Some("account"));
        assert_eq!(state.active.as_ref().unwrap().field, Field::Title);
        frame(&context, &items, &mut state, Vec::new(), 1_100.0);
        let (_, actions) = frame(
            &context,
            &items,
            &mut state,
            vec![
                key_event(egui::Key::A, egui::Modifiers::COMMAND),
                egui::Event::Text("Updated account".into()),
            ],
            1_100.0,
        );
        assert_eq!(
            actions,
            [Action::Update {
                id: "account".into(),
                field: Field::Title,
                value: Zeroizing::new("Updated account".into())
            }]
        );
        let (_, actions) = frame(
            &context,
            &items,
            &mut state,
            vec![key_event(egui::Key::Escape, egui::Modifiers::NONE)],
            1_100.0,
        );
        assert!(actions.is_empty());
        assert!(state.active.is_none());
    }

    fn key_event(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    #[test]
    fn password_eye_reveals_and_hides_only_its_row_without_editing() {
        let mut items = vec![
            entry("one", "Account 1", "person", "Work"),
            entry("two", "Account 2", "person", "Work"),
        ];
        items[0].password = "VisibleOnlyOnRequest".into();
        items[1].password = "SecondRowStaysSecret".into();
        let context = egui::Context::default();
        context.all_styles_mut(|style| {
            style.interaction.tooltip_delay = 0.0;
            style.interaction.show_tooltips_only_when_still = false;
        });
        let mut state = TableState::default();
        let (output, _) = frame(&context, &items, &mut state, Vec::new(), 1_100.0);
        let eye = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Circle(circle) if (circle.radius - 2.2).abs() < 0.01 => {
                    Some(circle.center)
                }
                _ => None,
            })
            .expect("Eye button must be visible");
        assert!(click(&context, &items, &mut state, eye).is_empty());
        assert!(state.active.is_none());
        let (output, _) = frame(&context, &items, &mut state, Vec::new(), 1_100.0);
        let text = visible_text(&output);
        assert!(
            text.iter()
                .any(|text| text.contains("VisibleOnlyOnRequest"))
        );
        assert!(
            !text
                .iter()
                .any(|text| text.contains("SecondRowStaysSecret"))
        );
        let password_position = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "VisibleOnlyOnRequest" => {
                    Some(text.pos + egui::vec2(4.0, 4.0))
                }
                _ => None,
            })
            .unwrap();
        // egui suppresses tooltips when a pointer movement follows a click by
        // less than 100 ms. Let the eye click settle before hovering the value.
        for _ in 0..12 {
            frame(&context, &items, &mut state, Vec::new(), 1_100.0);
        }
        let mut tooltip = None;
        for _ in 0..3 {
            tooltip = Some(
                frame(
                    &context,
                    &items,
                    &mut state,
                    vec![egui::Event::PointerMoved(password_position)],
                    1_100.0,
                )
                .0,
            );
        }
        assert!(visible_text(&tooltip.unwrap()).contains(&"Click to edit · Use the eye to hide"));
        assert!(click(&context, &items, &mut state, eye).is_empty());
        let (output, _) = frame(
            &context,
            &items,
            &mut state,
            vec![egui::Event::PointerMoved(password_position)],
            1_100.0,
        );
        assert!(
            !visible_text(&output)
                .iter()
                .any(|text| text.contains("VisibleOnlyOnRequest"))
        );
        assert!(state.revealed.is_empty());
    }

    fn visible_text(output: &egui::FullOutput) -> Vec<&str> {
        output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.text()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn multiline_extra_stays_intact_and_category_finishes_once() {
        let mut items = vec![entry("account", "Account", "person", "Work")];
        items[0].extra = "First line\n第二行\nThird line".into();
        let context = egui::Context::default();
        let mut state = TableState::default();
        state.refresh(&items, "", "");
        state.begin_edit(&items[0], Field::Extra, &mut Vec::new());
        let (output, actions) = frame(&context, &items, &mut state, Vec::new(), 1_100.0);
        assert!(actions.is_empty());
        assert_eq!(*state.active.as_ref().unwrap().value, items[0].extra);
        assert!(visible_text(&output).contains(&items[0].extra.as_str()));
        let (_, actions) = frame(
            &context,
            &items,
            &mut state,
            vec![
                key_event(egui::Key::A, egui::Modifiers::COMMAND),
                egui::Event::Text("New line\nAnother line".into()),
            ],
            1_100.0,
        );
        assert_eq!(
            actions,
            [Action::Update {
                id: "account".into(),
                field: Field::Extra,
                value: Zeroizing::new("New line\nAnother line".into())
            }]
        );
        frame(
            &context,
            &items,
            &mut state,
            vec![key_event(egui::Key::Enter, egui::Modifiers::COMMAND)],
            1_100.0,
        );
        assert!(state.active.is_none());

        state.begin_edit(&items[0], Field::Category, &mut Vec::new());
        frame(&context, &items, &mut state, Vec::new(), 1_100.0);
        let (_, actions) = frame(
            &context,
            &items,
            &mut state,
            vec![key_event(egui::Key::Enter, egui::Modifiers::NONE)],
            1_100.0,
        );
        assert_eq!(actions, [Action::FinishCategory("account".into())]);
        assert!(state.finish_edit().is_none());
    }

    #[test]
    fn editing_freezes_order_and_filters_without_resetting_scroll_on_completion() {
        let mut items = vec![
            entry("a", "Alpha", "one", "Work"),
            entry("z", "Zebra", "two", "Work"),
        ];
        let mut state = TableState::default();
        state.refresh(&items, "", "Work");
        state.reset_scroll = false;
        state.begin_edit(&items[0], Field::Title, &mut Vec::new());
        items[0].title = "ZZZZ".into();
        items[0].kind = "Personal".into();
        assert!(!state.refresh(&items, "", "Work"));
        assert_eq!(state.rows, [0, 1]);
        state.finish_edit();
        state.refresh(&items, "", "Work");
        assert_eq!(state.rows, [1]);
        assert!(!state.reset_scroll);

        state.begin_edit(&items[1], Field::Username, &mut Vec::new());
        items.remove(0);
        state.refresh(&items, "", "Work");
        assert_eq!(state.rows, [0]);
        assert!(state.active.is_none());
    }

    #[test]
    fn clear_sensitive_discards_inline_buffers_reveals_and_undo_history() {
        let items = vec![entry("account", "Account", "person", "Work")];
        let context = egui::Context::default();
        let mut state = TableState::default();
        state.refresh(&items, "", "");
        state.begin_edit(&items[0], Field::Password, &mut Vec::new());
        frame(&context, &items, &mut state, Vec::new(), 1_100.0);
        let id = state.active.as_ref().unwrap().widget_id();
        assert!(egui::text_edit::TextEditState::load(&context, id).is_some());
        state.revealed.insert("account".into());
        state.clear_sensitive();
        assert!(state.active.is_none());
        assert!(state.revealed.is_empty());
        assert!(egui::text_edit::TextEditState::load(&context, id).is_none());
        assert!(!context.memory(|memory| memory.has_focus(id)));
        let action = Action::Update {
            id: "account".into(),
            field: Field::Password,
            value: Zeroizing::new("DontPrintThisSecret".into()),
        };
        assert!(!format!("{action:?}").contains("DontPrintThisSecret"));
    }

    #[test]
    fn copy_and_delete_icons_emit_ids_without_opening_the_editor() {
        for (icon_size, occurrence, expected) in [
            (
                egui::vec2(8.0, 10.0),
                0,
                Action::CopyUsername("account".into()),
            ),
            (
                egui::vec2(8.0, 10.0),
                2,
                Action::CopyPassword("account".into()),
            ),
            (egui::vec2(8.0, 9.0), 0, Action::Delete("account".into())),
        ] {
            let items = vec![entry("account", "Account", "person", "Work")];
            let context = egui::Context::default();
            let mut state = TableState::default();
            let (output, _) = frame(&context, &items, &mut state, Vec::new(), 1_100.0);
            let position = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect) if rect.rect.size() == icon_size => {
                        Some(rect.rect.center())
                    }
                    _ => None,
                })
                .nth(occurrence)
                .expect("Action icon must be visible");
            assert_eq!(click(&context, &items, &mut state, position), [expected]);
        }
    }
}
