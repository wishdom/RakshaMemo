use eframe::egui::{self, RichText};
use rakshamemo::generator;
use zeroize::Zeroize;

use super::{EntryEditor, Notepad, style};

impl Notepad {
    /// Editing uses the page itself so the fields have the same room as the table.
    /// The navigation stays above the one scrolling area, including at small sizes.
    pub(super) fn password_editor_page(
        &mut self,
        ui: &mut egui::Ui,
        editor: &mut EntryEditor,
    ) -> bool {
        style::page(ui);
        let mut back = false;
        ui.horizontal(|ui| {
            back = ui.button("Back to passwords").clicked();
            ui.add_space(8.0);
            ui.heading("Password details");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.save_status(ui);
            });
        });
        ui.add_space(4.0);
        ui.label("Changes save automatically as you type.");
        ui.add_space(12.0);
        ui.separator();
        ui.add_space(12.0);

        let mut edited = false;
        let mut register_category = false;
        let wide = ui.available_width() >= 850.0;
        let page_height = ui.available_height();
        egui::ScrollArea::vertical()
            .id_salt(("password_editor_page", &editor.item.id))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // A little more space between the two groups helps the information
                // field read as a full editor instead of a small modal attachment.
                ui.spacing_mut().item_spacing.x = 20.0;
                if wide {
                    ui.columns(2, |columns| {
                        self.password_account_fields(
                            &mut columns[0],
                            editor,
                            &mut edited,
                            &mut register_category,
                        );
                        edited |= password_extra_field(
                            &mut columns[1],
                            editor,
                            (page_height - 132.0).max(310.0),
                        );
                    });
                } else {
                    self.password_account_fields(ui, editor, &mut edited, &mut register_category);
                    ui.add_space(16.0);
                    edited |= password_extra_field(ui, editor, 280.0);
                }
                ui.add_space(12.0);
                let updated = chrono::DateTime::parse_from_rfc3339(&editor.item.updated_at)
                    .map(|date| {
                        date.with_timezone(&chrono::Local)
                            .format("%Y-%m-%d at %H:%M")
                            .to_string()
                    })
                    .unwrap_or_else(|_| editor.item.updated_at.clone());
                ui.label(format!("Last updated {updated}"));
            });

        if edited {
            self.update_entry(editor);
        }
        if register_category || back {
            self.register_category(&editor.item.kind);
        }
        back
    }

    fn generate_password(&mut self, editor: &mut EntryEditor, edited: &mut bool) {
        match generator::generate(&editor.generator) {
            Ok(password) => {
                editor.item.password.zeroize();
                editor.item.password = password;
                editor.refresh_strength();
                *edited = true;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn password_account_fields(
        &mut self,
        ui: &mut egui::Ui,
        editor: &mut EntryEditor,
        edited: &mut bool,
        register_category: &mut bool,
    ) {
        egui::Frame::group(ui.style())
            .inner_margin(18)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing = egui::vec2(10.0, 8.0);

                field_label(ui, "Title");
                let title = ui.add(
                    single_line(&mut editor.item.title)
                        .id_salt((&editor.item.id, "title"))
                        .hint_text("Untitled — account or service name"),
                );
                if std::mem::take(&mut editor.focus_title) {
                    title.request_focus();
                }
                *edited |= title.changed();

                ui.add_space(8.0);
                field_label(ui, "Username / email");
                *edited |= ui
                    .add(
                        single_line(&mut editor.item.username)
                            .id_salt((&editor.item.id, "username"))
                            .hint_text("Username or email address"),
                    )
                    .changed();

                ui.add_space(8.0);
                field_label(ui, "Password");
                if ui
                    .add(
                        single_line(&mut editor.item.password)
                            .id_salt((&editor.item.id, "password"))
                            .password(!editor.show_password),
                    )
                    .changed()
                {
                    *edited = true;
                    editor.refresh_strength();
                }
                ui.horizontal_wrapped(|ui| {
                    if style::primary_button(ui, "Generate")
                        .on_hover_text("Replace with a random password using the options below")
                        .clicked()
                    {
                        self.generate_password(editor, edited);
                    }
                    ui.checkbox(&mut editor.show_password, "Show password");
                    if ui
                        .add_enabled(
                            !editor.item.password.is_empty(),
                            egui::Button::new("Copy password"),
                        )
                        .clicked()
                    {
                        self.copy(editor.item.password.clone());
                    }
                    if ui
                        .add_enabled(
                            !editor.item.username.is_empty(),
                            egui::Button::new("Copy username"),
                        )
                        .clicked()
                    {
                        self.copy(editor.item.username.clone());
                    }
                });

                if let Some(strength) = &editor.strength {
                    ui.add_space(2.0);
                    style::strength_meter(ui, strength);
                }

                egui::CollapsingHeader::new("Password generator options")
                    .id_salt((&editor.item.id, "generator"))
                    .show(ui, |ui| {
                        ui.add(
                            egui::Slider::new(&mut editor.generator.length, 8..=128).text("Length"),
                        );
                        ui.horizontal_wrapped(|ui| {
                            ui.checkbox(&mut editor.generator.lowercase, "a–z");
                            ui.checkbox(&mut editor.generator.uppercase, "A–Z");
                            ui.checkbox(&mut editor.generator.digits, "0–9");
                            ui.checkbox(&mut editor.generator.symbols, "Symbols");
                        });
                        if style::primary_button(ui, "Generate password").clicked() {
                            self.generate_password(editor, edited);
                        }
                    });

                ui.add_space(12.0);
                field_label(ui, "Category (optional)");
                ui.horizontal(|ui| {
                    let choice_width = 95.0;
                    let field_width =
                        (ui.available_width() - choice_width - ui.spacing().item_spacing.x)
                            .max(80.0);
                    let field = ui.add(
                        single_line(&mut editor.item.kind)
                            .id_salt((&editor.item.id, "category"))
                            .hint_text("Type a category")
                            .desired_width(field_width),
                    );
                    *edited |= field.changed();
                    *register_category |= field.lost_focus();
                    egui::ComboBox::from_id_salt((&editor.item.id, "entry_category"))
                        .selected_text("Choose")
                        .width(choice_width)
                        .show_ui(ui, |ui| {
                            *edited |= ui
                                .selectable_value(
                                    &mut editor.item.kind,
                                    String::new(),
                                    "No category",
                                )
                                .changed();
                            if let Some(store) = &self.store {
                                for kind in &store.data.meta.types {
                                    *edited |= ui
                                        .selectable_value(&mut editor.item.kind, kind.clone(), kind)
                                        .changed();
                                }
                            }
                        });
                });
                ui.label("Categories help group and filter accounts.");
            });
    }
}

fn single_line(text: &mut String) -> egui::TextEdit<'_> {
    egui::TextEdit::singleline(text)
        .font(egui::TextStyle::Body)
        .desired_width(f32::INFINITY)
        .min_size(egui::vec2(0.0, 36.0))
        .margin(egui::Margin::symmetric(10, 8))
}

fn field_label(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).strong());
}

fn password_extra_field(ui: &mut egui::Ui, editor: &mut EntryEditor, height: f32) -> bool {
    egui::Frame::group(ui.style())
        .inner_margin(18)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            field_label(ui, "Extra information");
            ui.label("Website, recovery notes, or anything useful for this account.");
            ui.add_space(8.0);
            ui.add(
                egui::TextEdit::multiline(&mut editor.item.extra)
                    .id_salt((&editor.item.id, "extra"))
                    .font(egui::TextStyle::Body)
                    .desired_rows(12)
                    .desired_width(f32::INFINITY)
                    .min_size(egui::vec2(0.0, height))
                    .margin(egui::Margin::same(10))
                    .hint_text("Add extra information…"),
            )
            .changed()
        })
        .inner
}
