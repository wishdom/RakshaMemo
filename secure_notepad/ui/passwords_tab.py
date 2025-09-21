from PySide6.QtWidgets import (
    QWidget,
    QVBoxLayout,
    QHBoxLayout,
    QPushButton,
    QTableWidget,
    QTableWidgetItem,
    QHeaderView,
    QMessageBox,
    QLabel,
    QLineEdit,
    QDialog,
    QComboBox,
    QInputDialog,
)
from PySide6.QtCore import Qt
from PySide6.QtGui import QGuiApplication

from ..storage.store import Store
from .password_dialog import PasswordDialog


class PasswordsTab(QWidget):
    COL_TITLE = 0
    COL_USERNAME = 1
    COL_PASSWORD = 2
    COL_TYPE = 3
    COL_UPDATED = 4

    def __init__(self, store: Store) -> None:
        super().__init__()
        self.store = store
        self._build_ui()
        self.refresh()

    def _build_ui(self) -> None:
        layout = QVBoxLayout(self)

        # Type filter row
        type_row = QHBoxLayout()
        type_row.addWidget(QLabel("Type:"))
        self.type_combo = QComboBox()
        self._reload_types_into_combo()
        self.type_combo.currentIndexChanged.connect(self.refresh)
        type_row.addWidget(self.type_combo)
        self.btn_add_type = QPushButton("Add Type")
        self.btn_add_type.clicked.connect(self.on_add_type)
        self.btn_rename_type = QPushButton("Rename Type")
        self.btn_rename_type.clicked.connect(self.on_rename_type)
        self.btn_delete_type = QPushButton("Delete Type")
        self.btn_delete_type.clicked.connect(self.on_delete_type)
        type_row.addWidget(self.btn_add_type)
        type_row.addWidget(self.btn_rename_type)
        type_row.addWidget(self.btn_delete_type)
        layout.addLayout(type_row)

        # Search/filter row
        search_row = QHBoxLayout()
        search_row.addWidget(QLabel("Search:"))
        self.search_edit = QLineEdit()
        self.search_edit.setPlaceholderText("Search title, username, or notes…")
        self.search_edit.textChanged.connect(self.refresh)
        search_row.addWidget(self.search_edit)
        layout.addLayout(search_row)

        self.table = QTableWidget(0, 5)
        self.table.setHorizontalHeaderLabels(["Title", "Username", "Password", "Type", "Updated"])
        self.table.horizontalHeader().setSectionResizeMode(QHeaderView.Stretch)
        self.table.setSelectionBehavior(QTableWidget.SelectRows)
        self.table.setSelectionMode(QTableWidget.SingleSelection)
        self.table.setEditTriggers(QTableWidget.NoEditTriggers)
        layout.addWidget(self.table)

        self.status = QLabel("")
        layout.addWidget(self.status)

        btns = QHBoxLayout()
        self.btn_add = QPushButton("Add")
        self.btn_edit = QPushButton("Edit")
        self.btn_delete = QPushButton("Delete")
        self.btn_copy_user = QPushButton("Copy Username")
        self.btn_copy_pass = QPushButton("Copy Password")

        self.btn_add.clicked.connect(self.on_add)
        self.btn_edit.clicked.connect(self.on_edit)
        self.btn_delete.clicked.connect(self.on_delete)
        self.btn_copy_user.clicked.connect(lambda: self.copy_selected(field="username"))
        self.btn_copy_pass.clicked.connect(lambda: self.copy_selected(field="password"))

        btns.addWidget(self.btn_add)
        btns.addWidget(self.btn_edit)
        btns.addWidget(self.btn_delete)
        btns.addStretch(1)
        btns.addWidget(self.btn_copy_user)
        btns.addWidget(self.btn_copy_pass)
        layout.addLayout(btns)

    def refresh(self) -> None:
        items = self.store.list_passwords()
        query = self.search_edit.text().strip().lower() if hasattr(self, "search_edit") else ""
        # Filter by type first
        selected_type = self.type_combo.currentText() if hasattr(self, "type_combo") else "All"
        if selected_type and selected_type != "All":
            items = [it for it in items if (it.type or "") == selected_type]
        if query:
            def match(it):
                return (
                    query in (it.title or "").lower()
                    or query in (it.username or "").lower()
                    or query in (it.extra or "").lower()
                    or query in (it.type or "").lower()
                )
            items = [it for it in items if match(it)]
        self.table.setRowCount(0)
        for item in items:
            row = self.table.rowCount()
            self.table.insertRow(row)
            it_title = QTableWidgetItem(item.title)
            it_title.setData(Qt.UserRole, item.id)
            it_username = QTableWidgetItem(item.username)
            it_password = QTableWidgetItem("••••••••")
            it_type = QTableWidgetItem(item.type)
            it_updated = QTableWidgetItem(item.updated_at.replace("T", " ").split(".")[0])

            # Center alignment for all cells
            for it in (it_title, it_username, it_password, it_type, it_updated):
                it.setTextAlignment(Qt.AlignHCenter | Qt.AlignVCenter)

            self.table.setItem(row, self.COL_TITLE, it_title)
            self.table.setItem(row, self.COL_USERNAME, it_username)
            self.table.setItem(row, self.COL_PASSWORD, it_password)
            self.table.setItem(row, self.COL_TYPE, it_type)
            self.table.setItem(row, self.COL_UPDATED, it_updated)

        self.status.setText(f"{len(items)} item(s)")

    def selected_id(self) -> str | None:
        rows = self.table.selectionModel().selectedRows()
        if not rows:
            return None
        r = rows[0].row()
        it = self.table.item(r, self.COL_TITLE)
        return it.data(Qt.UserRole) if it else None

    def on_add(self) -> None:
        # Suggest current type if a specific one is selected
        sel_type = self.type_combo.currentText() if self.type_combo.currentText() != "All" else ""
        dlg = PasswordDialog(self, types=self.store.list_types(), selected_type=sel_type)
        if dlg.exec() != QDialog.Accepted:
            return
        title, username, password, extra, type_val = dlg.get_values()
        if not title:
            QMessageBox.warning(self, "Validation", "Title is required.")
            return
        self.store.add_password(title, username, password, extra, type_val)
        self.refresh()

    def on_edit(self) -> None:
        pid = self.selected_id()
        if not pid:
            QMessageBox.information(self, "Edit", "Select a row to edit.")
            return
        # find item
        found = None
        for it in self.store.list_passwords():
            if it.id == pid:
                found = it
                break
        if not found:
            QMessageBox.warning(self, "Edit", "Selected item not found.")
            return
        dlg = PasswordDialog(self, title=found.title, username=found.username, password=found.password, extra=found.extra, types=self.store.list_types(), selected_type=found.type)
        if dlg.exec() != QDialog.Accepted:
            return
        title, username, password, extra, type_val = dlg.get_values()
        if not title:
            QMessageBox.warning(self, "Validation", "Title is required.")
            return
        self.store.update_password(found.id, title=title, username=username, password=password, extra=extra, type=type_val)
        self.refresh()

    def on_delete(self) -> None:
        pid = self.selected_id()
        if not pid:
            QMessageBox.information(self, "Delete", "Select a row to delete.")
            return
        if QMessageBox.question(self, "Confirm", "Delete selected password?") != QMessageBox.Yes:
            return
        try:
            self.store.delete_password(pid)
            self.refresh()
        except KeyError:
            QMessageBox.warning(self, "Delete", "Selected item not found.")

    def copy_selected(self, *, field: str) -> None:
        pid = self.selected_id()
        if not pid:
            QMessageBox.information(self, "Copy", "Select an item first.")
            return
        for it in self.store.list_passwords():
            if it.id == pid:
                val = it.username if field == "username" else it.password
                QGuiApplication.clipboard().setText(val)
                self.status.setText(f"Copied {field} to clipboard.")
                return

    # Types management
    def _reload_types_into_combo(self) -> None:
        if not hasattr(self, "type_combo"):
            return
        current = self.type_combo.currentText() if self.type_combo.count() > 0 else "All"
        self.type_combo.clear()
        self.type_combo.addItem("All")
        for t in self.store.list_types():
            self.type_combo.addItem(t)
        # restore selection if possible
        idx = self.type_combo.findText(current)
        self.type_combo.setCurrentIndex(idx if idx >= 0 else 0)

    def on_add_type(self) -> None:
        name, ok = QInputDialog.getText(self, "Add Type", "Type name:")
        if not ok or not name.strip():
            return
        self.store.add_type(name.strip())
        self._reload_types_into_combo()
        self.refresh()

    def on_rename_type(self) -> None:
        types = self.store.list_types()
        if not types:
            QMessageBox.information(self, "Rename Type", "No types to rename.")
            return
        old, ok = QInputDialog.getItem(self, "Rename Type", "Select type:", types, editable=False)
        if not ok or not old:
            return
        new, ok2 = QInputDialog.getText(self, "Rename Type", f"New name for '{old}':")
        if not ok2 or not new.strip():
            return
        self.store.rename_type(old, new.strip())
        self._reload_types_into_combo()
        self.refresh()

    def on_delete_type(self) -> None:
        types = self.store.list_types()
        if not types:
            QMessageBox.information(self, "Delete Type", "No types to delete.")
            return
        name, ok = QInputDialog.getItem(self, "Delete Type", "Select type:", types, editable=False)
        if not ok or not name:
            return
        if QMessageBox.question(self, "Confirm", f"Delete type '{name}'? This cannot be undone.") != QMessageBox.Yes:
            return
        try:
            self.store.remove_type(name)
        except ValueError as e:
            QMessageBox.warning(self, "Delete Type", str(e))
            return
        self._reload_types_into_combo()
        self.refresh()
