from PySide6.QtWidgets import (
    QWidget,
    QVBoxLayout,
    QHBoxLayout,
    QTextEdit,
    QLabel,
    QListWidget,
    QListWidgetItem,
    QPushButton,
    QSplitter,
    QInputDialog,
)
from PySide6.QtCore import QTimer, QDateTime, Qt

from ..storage.store import Store


class NotesTab(QWidget):
    def __init__(self, store: Store) -> None:
        super().__init__()
        self._dirty = False
        self.store = store
        self._current_note_id: str | None = None

        root = QHBoxLayout(self)
        splitter = QSplitter()
        splitter.setOrientation(Qt.Horizontal)
        root.addWidget(splitter)

        # Left: vertical list of notes with controls
        left_panel = QWidget()
        left_layout = QVBoxLayout(left_panel)

        controls = QHBoxLayout()
        lbl = QLabel("Notepads")
        controls.addWidget(lbl)
        controls.addStretch(1)
        self.btn_add = QPushButton("+")
        self.btn_add.setFixedWidth(28)
        self.btn_add.clicked.connect(self._on_add_note)
        self.btn_rename = QPushButton("Rename")
        self.btn_rename.clicked.connect(self._on_rename_note)
        controls.addWidget(self.btn_add)
        controls.addWidget(self.btn_rename)
        left_layout.addLayout(controls)

        self.list = QListWidget()
        self.list.currentRowChanged.connect(self._on_list_selection_changed)
        left_layout.addWidget(self.list)

        # Right: editor and status
        right_panel = QWidget()
        right_layout = QVBoxLayout(right_panel)

        self.editor = QTextEdit()
        self.editor.setPlaceholderText("Type your notes here…")
        right_layout.addWidget(self.editor)

        self.status = QLabel("")
        right_layout.addWidget(self.status)

        splitter.addWidget(left_panel)
        splitter.addWidget(right_panel)
        splitter.setStretchFactor(0, 0)
        splitter.setStretchFactor(1, 1)

        # Autosave timer
        self.timer = QTimer(self)
        self.timer.setInterval(600)  # 0.6s debounce
        self.timer.timeout.connect(self._autosave)
        self.editor.textChanged.connect(self._on_text_changed)

        # Populate and load active note
        self._populate_notes()
        self._select_active_note()
        self._load_current_note_content()

    # ------- UI wiring -------
    def _populate_notes(self) -> None:
        items = self.store.list_note_items()
        active_id = self.store.get_active_note_id()
        self.list.blockSignals(True)
        self.list.clear()
        for it in items:
            wi = QListWidgetItem(it.title)
            wi.setData(Qt.UserRole, it.id)
            self.list.addItem(wi)
            if it.id == active_id:
                self._current_note_id = it.id
        self.list.blockSignals(False)

    def _select_active_note(self) -> None:
        active_id = self.store.get_active_note_id()
        for i in range(self.list.count()):
            if self.list.item(i).data(Qt.UserRole) == active_id:
                self.list.setCurrentRow(i)
                return
        if self.list.count() > 0:
            self.list.setCurrentRow(0)

    def _on_list_selection_changed(self, row: int) -> None:
        if row < 0:
            return
        # Save current note if pending changes
        self._autosave()
        item = self.list.item(row)
        nid = item.data(Qt.UserRole)
        self.store.set_active_note_id(nid)
        self._current_note_id = nid
        self._load_current_note_content()

    def _load_current_note_content(self) -> None:
        text = self.store.get_note_content(self._current_note_id)
        self.editor.blockSignals(True)
        self.editor.setPlainText(text)
        self.editor.blockSignals(False)
        self._dirty = False
        self.status.setText("Loaded.")

    def _on_add_note(self) -> None:
        title, ok = QInputDialog.getText(self, "New Notepad", "Title:", text="New Note")
        if not ok:
            return
        note = self.store.create_note(title.strip() or "Untitled")
        self._populate_notes()
        # Select new
        for i in range(self.list.count()):
            if self.list.item(i).data(Qt.UserRole) == note.id:
                self.list.setCurrentRow(i)
                break
        self._load_current_note_content()

    def _on_rename_note(self) -> None:
        row = self.list.currentRow()
        if row < 0:
            return
        item = self.list.item(row)
        nid = item.data(Qt.UserRole)
        current_title = item.text()
        title, ok = QInputDialog.getText(self, "Rename Notepad", "New title:", text=current_title)
        if not ok:
            return
        self.store.rename_note(nid, title.strip() or "Untitled")
        # Update UI text
        item.setText(title.strip() or "Untitled")

    # ------- Save / load -------
    def _on_text_changed(self) -> None:
        self._dirty = True
        self.status.setText("Unsaved changes…")
        self.timer.start()

    def _autosave(self) -> None:
        self.timer.stop()
        if not self._dirty:
            return
        self.save_now()

    # Compatibility entry-point: set content directly (e.g., main.py initial load)
    def load_text(self, text: str) -> None:
        self.editor.blockSignals(True)
        self.editor.setPlainText(text)
        self.editor.blockSignals(False)
        self._dirty = False
        self.status.setText("Loaded.")

    def save_now(self) -> None:
        """Persist current notes immediately if there are pending changes."""
        if not self._dirty and self.status.text().startswith("Saved"):
            return
        nid = self._current_note_id or self.store.get_active_note_id()
        self.store.set_note_content(self.editor.toPlainText(), nid)
        self._dirty = False
        ts = QDateTime.currentDateTime().toString("yyyy-MM-dd hh:mm:ss")
        self.status.setText(f"Saved at {ts}.")
