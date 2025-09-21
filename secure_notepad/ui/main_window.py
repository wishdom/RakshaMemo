from PySide6.QtWidgets import (
    QApplication,
    QMainWindow,
    QTabWidget,
    QMessageBox,
    QFileDialog,
    QDialog,
)
from PySide6.QtCore import QSettings
from PySide6.QtGui import QAction, QActionGroup
from pathlib import Path

from .notes_tab import NotesTab
from .passwords_tab import PasswordsTab
from .theme import apply_theme, ThemeMode, set_theme_mode, get_theme_mode
from ..storage.store import Store
from .auth_dialog import AuthDialog, UnlockMode


class MainWindow(QMainWindow):
    def __init__(self, store: Store) -> None:
        super().__init__()
        self.store = store
        self.setWindowTitle("Secure Notepad")
        self.resize(1000, 700)
        self._setup_ui()
        self._restore_geometry()

    def _setup_ui(self) -> None:
        tabs = QTabWidget()
        self.notes_tab = NotesTab(self.store)
        self.passwords_tab = PasswordsTab(self.store)
        tabs.addTab(self.notes_tab, "Notes")
        tabs.addTab(self.passwords_tab, "Passwords")
        self.setCentralWidget(tabs)
        self._create_menus()

    def _create_menus(self) -> None:
        menubar = self.menuBar()

        file_menu = menubar.addMenu("&File")
        lock_action = QAction("&Lock", self)
        lock_action.triggered.connect(self._action_lock)
        file_menu.addAction(lock_action)

        export_action = QAction("&Export Vault…", self)
        export_action.triggered.connect(self._action_export)
        file_menu.addAction(export_action)

        import_action = QAction("&Import Vault…", self)
        import_action.triggered.connect(self._action_import)
        file_menu.addAction(import_action)

        info_action = QAction("Vault &Info…", self)
        info_action.triggered.connect(self._action_info)
        file_menu.addAction(info_action)

        exit_action = QAction("E&xit", self)
        exit_action.triggered.connect(self.close)
        file_menu.addAction(exit_action)

        view_menu = menubar.addMenu("&View")
        self.theme_light = QAction("Light Theme", self, checkable=True)
        self.theme_dark = QAction("Dark Theme", self, checkable=True)
        group = QActionGroup(self)
        group.setExclusive(True)
        self.theme_light.setActionGroup(group)
        self.theme_dark.setActionGroup(group)

        current = get_theme_mode()
        self.theme_light.setChecked(current == ThemeMode.LIGHT)
        self.theme_dark.setChecked(current == ThemeMode.DARK)

        self.theme_light.triggered.connect(lambda: self._switch_theme(ThemeMode.LIGHT))
        self.theme_dark.triggered.connect(lambda: self._switch_theme(ThemeMode.DARK))
        view_menu.addAction(self.theme_light)
        view_menu.addAction(self.theme_dark)

        help_menu = menubar.addMenu("&Help")
        about_action = QAction("&About", self)
        about_action.triggered.connect(
            lambda: QMessageBox.about(
                self,
                "About Secure Notepad",
                "Secure Notepad\nEncrypted notes and passwords.",
            )
        )
        help_menu.addAction(about_action)

    def _switch_theme(self, mode: ThemeMode) -> None:
        apply_theme(QApplication.instance(), mode)
        set_theme_mode(mode)
        # Ensure menu reflects current theme
        if mode == ThemeMode.DARK:
            self.theme_dark.setChecked(True)
        else:
            self.theme_light.setChecked(True)

    def _restore_geometry(self) -> None:
        s = QSettings()
        g = s.value("window/geometry")
        if g is not None:
            self.restoreGeometry(g)

    def closeEvent(self, event) -> None:  # type: ignore[override]
        # Save any pending notes edits before exit
        try:
            if hasattr(self, "notes_tab") and hasattr(self.notes_tab, "save_now"):
                self.notes_tab.save_now()
        except Exception:
            pass
        s = QSettings()
        s.setValue("window/geometry", self.saveGeometry())
        super().closeEvent(event)

    # File actions
    def _action_lock(self) -> None:
        # Save notes and lock the store, then prompt for unlock
        try:
            if hasattr(self, "notes_tab") and hasattr(self.notes_tab, "save_now"):
                self.notes_tab.save_now()
        except Exception:
            pass
        self.store.lock()
        if not self._prompt_unlock():
            self.close()

    def _action_export(self) -> None:
        meta = self.store.get_meta()
        ts = str(meta.get("modified_at", "")).replace(":", "-").replace("T", " ")[:19] if meta else ""
        suggested = f"vault_{ts}.json" if ts else "vault.json"
        path, _ = QFileDialog.getSaveFileName(self, "Export Vault", str(Path.home() / suggested), "Vault (*.json);;All Files (*)")
        if not path:
            return
        try:
            self.store.export_vault(Path(path))
            QMessageBox.information(self, "Export", "Vault exported successfully.")
        except Exception as e:
            QMessageBox.critical(self, "Export Failed", str(e))

    def _action_import(self) -> None:
        path, _ = QFileDialog.getOpenFileName(self, "Import Vault", str(Path.home()), "Vault (*.json);;All Files (*)")
        if not path:
            return
        try:
            self.store.import_vault(Path(path))
        except Exception as e:
            QMessageBox.critical(self, "Import Failed", str(e))
            return
        # Require unlock and reload data
        if not self._prompt_unlock():
            self.close()

    def _prompt_unlock(self) -> bool:
        while True:
            dlg = AuthDialog(UnlockMode.UNLOCK, self)
            if dlg.exec() != QDialog.Accepted or not dlg.password:
                return False
            try:
                self.store.load(dlg.password)
                # Reload UI data
                self.notes_tab.load_text(self.store.get_notes())
                # Reload types list and items
                if hasattr(self.passwords_tab, "_reload_types_into_combo"):
                    self.passwords_tab._reload_types_into_combo()
                self.passwords_tab.refresh()
                return True
            except Exception:
                QMessageBox.warning(self, "Unlock Failed", "Incorrect password. Please try again.")

    def _action_info(self) -> None:
        meta = self.store.get_meta()
        created = meta.get("created_at", "-")
        modified = meta.get("modified_at", "-")
        types = ", ".join(self.store.list_types()) or "(none)"
        msg = (
            f"Path: {self.store.path}\n"
            f"Created: {created}\n"
            f"Modified: {modified}\n"
            f"Types: {types}\n"
        )
        QMessageBox.information(self, "Vault Info", msg)
