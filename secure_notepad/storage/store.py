from __future__ import annotations

import json
import os
import tempfile
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Dict, List, Optional
from datetime import datetime, timezone
import uuid

from platformdirs import user_data_dir

from ..crypto.crypto import encrypt_json, decrypt_json


APP_AUTHOR = "Wishdom"
APP_NAME = "Secure Notepad"
DEFAULT_FILE_NAME = "vault.json"


@dataclass
class PasswordItem:
    id: str
    title: str
    username: str
    password: str
    type: str = ""
    extra: str = ""
    updated_at: str = field(default_factory=lambda: datetime.now(timezone.utc).isoformat())


class Store:
    def __init__(self) -> None:
        self._path = Path(user_data_dir(APP_NAME, APP_AUTHOR)) / DEFAULT_FILE_NAME
        now = datetime.now(timezone.utc).isoformat()
        self._data: Dict[str, Any] = {
            "meta": {"created_at": now, "modified_at": now, "types": []},
            "notes": "",
            "passwords": [],
        }
        self._loaded = False
        self._password: Optional[str] = None

    @property
    def path(self) -> Path:
        return self._path

    def exists(self) -> bool:
        return self._path.exists()

    def setup_new(self, password: str) -> None:
        self._password = password
        now = datetime.now(timezone.utc).isoformat()
        self._data = {
            "meta": {
                "created_at": now,
                "modified_at": now,
                "types": ["Banking", "Online shopping", "Social", "Email", "Work", "Personal"],
            },
            "notes": "",
            "passwords": [],
        }
        self._loaded = True
        self.save()

    def load(self, password: str) -> None:
        blob = self._path.read_bytes()
        data = decrypt_json(password, blob)
        # schema guard
        meta = data.get("meta") or {}
        if not isinstance(meta, dict):
            meta = {}
        meta.setdefault("created_at", datetime.now(timezone.utc).isoformat())
        meta.setdefault("modified_at", datetime.now(timezone.utc).isoformat())
        meta.setdefault("types", [])
        data["meta"] = meta
        data.setdefault("notes", "")
        data.setdefault("passwords", [])
        # Backfill password item fields
        for obj in data.get("passwords", []):
            obj.setdefault("type", "")
            obj.setdefault("extra", "")
            obj.setdefault("updated_at", datetime.now(timezone.utc).isoformat())
        self._data = data
        self._password = password
        self._loaded = True

    def save(self) -> None:
        if not self._loaded or not self._password:
            raise RuntimeError("Store not unlocked")
        # update modified timestamp
        try:
            self._data.setdefault("meta", {})["modified_at"] = datetime.now(timezone.utc).isoformat()
        except Exception:
            pass
        self._path.parent.mkdir(parents=True, exist_ok=True)
        encrypted = encrypt_json(self._password, self._data)
        # Atomic write
        with tempfile.NamedTemporaryFile("wb", delete=False, dir=str(self._path.parent)) as tf:
            tf.write(encrypted)
            temp_name = tf.name
        # Set restrictive permissions
        try:
            os.chmod(temp_name, 0o600)
        except Exception:
            pass
        os.replace(temp_name, self._path)

    def lock(self) -> None:
        """Forget key material and clear in-memory data."""
        self._password = None
        self._loaded = False
        # Clear sensitive in-memory data
        now = datetime.now(timezone.utc).isoformat()
        self._data = {"meta": {"created_at": now, "modified_at": now, "types": []}, "notes": "", "passwords": []}

    def export_vault(self, dest: Path) -> None:
        """Export the encrypted vault file to a user-selected destination."""
        # Ensure latest on disk
        if self._loaded and self._password:
            self.save()
        dest.parent.mkdir(parents=True, exist_ok=True)
        # Atomic copy via read/write and replace
        blob = self._path.read_bytes()
        with tempfile.NamedTemporaryFile("wb", delete=False, dir=str(dest.parent)) as tf:
            tf.write(blob)
            temp_name = tf.name
        try:
            os.chmod(temp_name, 0o600)
        except Exception:
            pass
        os.replace(temp_name, dest)

    def import_vault(self, src: Path) -> None:
        """Import an encrypted vault file from a user-selected source, replacing current vault."""
        blob = Path(src).read_bytes()
        self._path.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.NamedTemporaryFile("wb", delete=False, dir=str(self._path.parent)) as tf:
            tf.write(blob)
            temp_name = tf.name
        try:
            os.chmod(temp_name, 0o600)
        except Exception:
            pass
        os.replace(temp_name, self._path)
        # After import, require unlock again
        self.lock()

    # Notes API
    def get_notes(self) -> str:
        return str(self._data.get("notes", ""))

    def set_notes(self, text: str) -> None:
        self._data["notes"] = text
        self.save()

    # Passwords API
    def list_passwords(self) -> List[PasswordItem]:
        items = []
        for obj in self._data.get("passwords", []):
            items.append(
                PasswordItem(
                    id=str(obj.get("id")),
                    title=str(obj.get("title", "")),
                    username=str(obj.get("username", "")),
                    password=str(obj.get("password", "")),
                    type=str(obj.get("type", "")),
                    extra=str(obj.get("extra", "")),
                    updated_at=str(obj.get("updated_at", datetime.now(timezone.utc).isoformat())),
                )
            )
        return items

    def add_password(self, title: str, username: str, password: str, extra: str = "", type: str = "") -> PasswordItem:
        item = PasswordItem(id=uuid.uuid4().hex, title=title, username=username, password=password, extra=extra, type=type)
        self._data.setdefault("passwords", []).append(item.__dict__)
        # ensure type exists in meta types
        if type:
            types = self._data.setdefault("meta", {}).setdefault("types", [])
            if type not in types:
                types.append(type)
        self.save()
        return item

    def update_password(self, item_id: str, *, title: Optional[str] = None, username: Optional[str] = None, password: Optional[str] = None, extra: Optional[str] = None, type: Optional[str] = None) -> None:
        arr = self._data.setdefault("passwords", [])
        for obj in arr:
            if obj.get("id") == item_id:
                if title is not None:
                    obj["title"] = title
                if username is not None:
                    obj["username"] = username
                if password is not None:
                    obj["password"] = password
                if extra is not None:
                    obj["extra"] = extra
                if type is not None:
                    obj["type"] = type
                    if type:
                        types = self._data.setdefault("meta", {}).setdefault("types", [])
                        if type not in types:
                            types.append(type)
                obj["updated_at"] = datetime.now(timezone.utc).isoformat()
                self.save()
                return
        raise KeyError("Password item not found")

    def delete_password(self, item_id: str) -> None:
        arr = self._data.setdefault("passwords", [])
        new_arr = [obj for obj in arr if obj.get("id") != item_id]
        if len(new_arr) == len(arr):
            raise KeyError("Password item not found")
        self._data["passwords"] = new_arr
        self.save()

    # Types API
    def list_types(self) -> List[str]:
        return list(self._data.get("meta", {}).get("types", []))

    def add_type(self, name: str) -> None:
        name = name.strip()
        if not name:
            return
        types = self._data.setdefault("meta", {}).setdefault("types", [])
        if name not in types:
            types.append(name)
            self.save()

    def remove_type(self, name: str) -> None:
        types = self._data.setdefault("meta", {}).setdefault("types", [])
        # Prevent removal if used
        for obj in self._data.get("passwords", []):
            if obj.get("type") == name:
                raise ValueError("Type is in use by one or more items")
        self._data["meta"]["types"] = [t for t in types if t != name]
        self.save()

    def rename_type(self, old: str, new: str) -> None:
        new = new.strip()
        if not new:
            return
        types = self._data.setdefault("meta", {}).setdefault("types", [])
        self._data["meta"]["types"] = [new if t == old else t for t in types]
        # Update items
        for obj in self._data.get("passwords", []):
            if obj.get("type") == old:
                obj["type"] = new
        self.save()

    # Meta
    def get_meta(self) -> Dict[str, Any]:
        return dict(self._data.get("meta", {}))
