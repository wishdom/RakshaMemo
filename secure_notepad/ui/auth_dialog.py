from PySide6.QtWidgets import QDialog, QVBoxLayout, QLabel, QLineEdit, QDialogButtonBox
from PySide6.QtCore import Qt


class UnlockMode:
    UNLOCK = "unlock"
    SETUP = "setup"


class AuthDialog(QDialog):
    def __init__(self, mode: str, parent=None) -> None:
        super().__init__(parent)
        self.mode = mode
        self.setWindowTitle("Unlock Vault" if mode == UnlockMode.UNLOCK else "Create Master Password")
        self.password: str | None = None

        layout = QVBoxLayout(self)
        if mode == UnlockMode.SETUP:
            layout.addWidget(QLabel("Create a master password. Do not forget it."))
        else:
            layout.addWidget(QLabel("Enter your master password to unlock."))

        self.edit1 = QLineEdit()
        self.edit1.setEchoMode(QLineEdit.Password)
        self.edit1.setPlaceholderText("Master password")
        layout.addWidget(self.edit1)

        self.edit2 = None
        if mode == UnlockMode.SETUP:
            self.edit2 = QLineEdit()
            self.edit2.setEchoMode(QLineEdit.Password)
            self.edit2.setPlaceholderText("Confirm master password")
            layout.addWidget(self.edit2)

        self.buttons = QDialogButtonBox(QDialogButtonBox.Ok | QDialogButtonBox.Cancel)
        self.buttons.accepted.connect(self._on_accept)
        self.buttons.rejected.connect(self.reject)
        layout.addWidget(self.buttons)

    def _on_accept(self) -> None:
        p1 = self.edit1.text()
        if self.mode == UnlockMode.SETUP:
            p2 = self.edit2.text() if self.edit2 is not None else ""
            if len(p1) < 8:
                self._set_error("Password must be at least 8 characters.")
                return
            if p1 != p2:
                self._set_error("Passwords do not match.")
                return
        if not p1:
            self._set_error("Password cannot be empty.")
            return
        self.password = p1
        self.accept()

    def _set_error(self, msg: str) -> None:
        self.edit1.setStyleSheet("border: 1px solid #cc3333;")
        if self.edit2 is not None:
            self.edit2.setStyleSheet("border: 1px solid #cc3333;")
        self.setWindowTitle(msg)
