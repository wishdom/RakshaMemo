import sys
from PySide6.QtWidgets import QApplication, QDialog
from PySide6.QtCore import QCoreApplication

from secure_notepad.ui.theme import apply_theme, get_theme_mode
from secure_notepad.ui.main_window import MainWindow
from secure_notepad.ui.auth_dialog import AuthDialog, UnlockMode
from secure_notepad.storage.store import Store
from PySide6.QtWidgets import QMessageBox


def main() -> int:
    app = QApplication(sys.argv)
    QCoreApplication.setOrganizationName("Wishdom")
    QCoreApplication.setApplicationName("Secure Notepad")

    # Apply saved theme (default to light)
    apply_theme(app, get_theme_mode())

    # Unlock or setup store
    store = Store()

    if not store.exists():
        dlg = AuthDialog(UnlockMode.SETUP)
        if dlg.exec() != QDialog.Accepted or not dlg.password:
            return 0
        store.setup_new(dlg.password)
    else:
        # Keep asking until correct or canceled
        while True:
            dlg = AuthDialog(UnlockMode.UNLOCK)
            if dlg.exec() != QDialog.Accepted or not dlg.password:
                return 0
            try:
                store.load(dlg.password)
                break
            except Exception:
                QMessageBox.warning(None, "Unlock Failed", "Incorrect password. Please try again.")

    window = MainWindow(store)
    # Initialize notes from store
    window.notes_tab.load_text(store.get_notes())
    window.show()
    return app.exec()


if __name__ == "__main__":
    raise SystemExit(main())
