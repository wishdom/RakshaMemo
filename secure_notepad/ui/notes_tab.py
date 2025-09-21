from PySide6.QtWidgets import QWidget, QVBoxLayout, QTextEdit, QLabel
from PySide6.QtCore import QTimer, QDateTime

from ..storage.store import Store


class NotesTab(QWidget):
    def __init__(self, store: Store) -> None:
        super().__init__()
        self._dirty = False
        self.store = store

        layout = QVBoxLayout(self)
        self.editor = QTextEdit()
        self.editor.setPlaceholderText("Type your notes here…")
        layout.addWidget(self.editor)

        self.status = QLabel("")
        layout.addWidget(self.status)

        self.timer = QTimer(self)
        self.timer.setInterval(600)  # 0.6s debounce
        self.timer.timeout.connect(self._autosave)

        self.editor.textChanged.connect(self._on_text_changed)

    def _on_text_changed(self) -> None:
        self._dirty = True
        self.status.setText("Unsaved changes…")
        self.timer.start()

    def _autosave(self) -> None:
        self.timer.stop()
        if not self._dirty:
            return
        self.save_now()

    # Load text without triggering save
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
        self.store.set_notes(self.editor.toPlainText())
        self._dirty = False
        ts = QDateTime.currentDateTime().toString("yyyy-MM-dd hh:mm:ss")
        self.status.setText(f"Saved at {ts}.")
