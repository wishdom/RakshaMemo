//! The plaintext schema remains compatible with the original Python vault.

use chrono::Utc;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use uuid::Uuid;
use zeroize::Zeroize;

pub type UnknownFields = BTreeMap<String, Value>;

pub fn now() -> String {
    Utc::now().to_rfc3339()
}

fn new_id() -> String {
    Uuid::new_v4().simple().to_string()
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub content: String,
    pub updated_at: String,
    /// Pinned notes stay at the top of the list. Omitted from the file when
    /// false, so unpinned notes are stored exactly as before.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub pinned: bool,
    #[serde(flatten)]
    pub unknown_fields: UnknownFields,
}

impl Default for Note {
    fn default() -> Self {
        Self {
            id: new_id(),
            title: "Untitled".into(),
            content: String::new(),
            updated_at: now(),
            pinned: false,
            unknown_fields: UnknownFields::new(),
        }
    }
}

impl Note {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..Self::default()
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PasswordItem {
    pub id: String,
    pub title: String,
    pub username: String,
    pub password: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub extra: String,
    pub updated_at: String,
    #[serde(flatten)]
    pub unknown_fields: UnknownFields,
}

impl Default for PasswordItem {
    fn default() -> Self {
        Self {
            id: new_id(),
            title: String::new(),
            username: String::new(),
            password: String::new(),
            kind: String::new(),
            extra: String::new(),
            updated_at: now(),
            unknown_fields: UnknownFields::new(),
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Notes {
    pub active_id: String,
    pub items: Vec<Note>,
    #[serde(flatten)]
    pub unknown_fields: UnknownFields,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Meta {
    pub created_at: String,
    pub modified_at: String,
    pub types: Vec<String>,
    #[serde(flatten)]
    pub unknown_fields: UnknownFields,
}

impl Default for Meta {
    fn default() -> Self {
        let timestamp = now();
        Self {
            created_at: timestamp.clone(),
            modified_at: timestamp,
            types: Vec::new(),
            unknown_fields: UnknownFields::new(),
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct VaultData {
    #[serde(default)]
    pub meta: Meta,
    #[serde(default, deserialize_with = "deserialize_notes")]
    pub notes: Notes,
    #[serde(default)]
    pub passwords: Vec<PasswordItem>,
    #[serde(flatten)]
    pub unknown_fields: UnknownFields,
}

impl VaultData {
    pub fn new() -> Self {
        let mut data = Self::default();
        data.meta.types = [
            "Banking",
            "Online shopping",
            "Social",
            "Email",
            "Work",
            "Personal",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        data
    }

    /// Repair only references; never discard notes, credentials, or unknown fields.
    pub fn normalize(&mut self) {
        if !self
            .notes
            .items
            .iter()
            .any(|n| n.id == self.notes.active_id)
        {
            self.notes.active_id = self
                .notes
                .items
                .first()
                .map(|n| n.id.clone())
                .unwrap_or_default();
        }
        for item in &self.passwords {
            if !item.kind.is_empty() && !self.meta.types.contains(&item.kind) {
                self.meta.types.push(item.kind.clone());
            }
        }
    }
}

fn deserialize_notes<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Notes, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum LegacyOrNotes {
        Legacy(String),
        Current(Notes),
    }
    match Option::<LegacyOrNotes>::deserialize(deserializer)? {
        Some(LegacyOrNotes::Current(notes)) => Ok(notes),
        Some(LegacyOrNotes::Legacy(content)) => {
            let note = Note {
                content,
                ..Note::new("Imported Note")
            };
            Ok(Notes {
                active_id: note.id.clone(),
                items: vec![note],
                ..Notes::default()
            })
        }
        None => Ok(Notes::default()),
    }
}

fn wipe_json(value: &mut Value) {
    match value {
        Value::String(text) => text.zeroize(),
        Value::Array(values) => values.iter_mut().for_each(wipe_json),
        Value::Object(values) => {
            for (mut key, mut value) in std::mem::take(values) {
                key.zeroize();
                wipe_json(&mut value);
            }
        }
        _ => {}
    }
}

fn wipe_unknown(fields: &mut UnknownFields) {
    for (mut key, mut value) in std::mem::take(fields) {
        key.zeroize();
        wipe_json(&mut value);
    }
}

impl Drop for VaultData {
    fn drop(&mut self) {
        self.meta.created_at.zeroize();
        self.meta.modified_at.zeroize();
        self.meta.types.zeroize();
        wipe_unknown(&mut self.meta.unknown_fields);
        self.notes.active_id.zeroize();
        for note in &mut self.notes.items {
            note.id.zeroize();
            note.title.zeroize();
            note.content.zeroize();
            note.updated_at.zeroize();
            wipe_unknown(&mut note.unknown_fields);
        }
        wipe_unknown(&mut self.notes.unknown_fields);
        for item in &mut self.passwords {
            item.id.zeroize();
            item.title.zeroize();
            item.username.zeroize();
            item.password.zeroize();
            item.kind.zeroize();
            item.extra.zeroize();
            item.updated_at.zeroize();
            wipe_unknown(&mut item.unknown_fields);
        }
        wipe_unknown(&mut self.unknown_fields);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_legacy_notes_and_missing_password_fields() {
        let mut data: VaultData = serde_json::from_str(r#"{"notes":"旧いメモ 🔐","passwords":[{"id":"p","title":"Mail","username":"u","password":"secret"}]}"#).unwrap();
        data.normalize();
        assert_eq!(data.notes.items[0].title, "Imported Note");
        assert_eq!(data.notes.items[0].content, "旧いメモ 🔐");
        assert_eq!(data.notes.active_id, data.notes.items[0].id);
        assert!(data.passwords[0].kind.is_empty());
        assert!(!data.passwords[0].updated_at.is_empty());
    }

    #[test]
    fn retains_unknown_json_at_every_level() {
        let original = serde_json::json!({
            "future": {"secret": ["preserve"]},
            "meta": {"future_meta": 7},
            "notes": {"future_notes": true, "items": [{"future_note": "yes"}]},
            "passwords": [{"future_password": [1, 2]}]
        });
        let data: VaultData = serde_json::from_value(original.clone()).unwrap();
        let saved = serde_json::to_value(&data).unwrap();
        assert_eq!(saved["future"], original["future"]);
        assert_eq!(saved["meta"]["future_meta"], 7);
        assert_eq!(saved["notes"]["future_notes"], true);
        assert_eq!(saved["notes"]["items"][0]["future_note"], "yes");
        assert_eq!(
            saved["passwords"][0]["future_password"],
            serde_json::json!([1, 2])
        );
    }
}
