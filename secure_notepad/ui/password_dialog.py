from PySide6.QtWidgets import (
    QDialog,
    QVBoxLayout,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QTextEdit,
    QDialogButtonBox,
    QPushButton,
    QSpinBox,
    QCheckBox,
    QProgressBar,
    QComboBox,
)
from PySide6.QtCore import Qt
from PySide6.QtGui import QGuiApplication
import secrets
import string


class PasswordDialog(QDialog):
    def __init__(self, parent=None, *, title: str = "", username: str = "", password: str = "", extra: str = "", types: list[str] | None = None, selected_type: str = "") -> None:
        super().__init__(parent)
        self.setWindowTitle("Password Entry")
        self._build_ui()
        self.title_edit.setText(title)
        self.username_edit.setText(username)
        self.password_edit.setText(password)
        self.extra_edit.setPlainText(extra)
        # Types
        types = types or []
        self.type_combo.clear()
        self.type_combo.setEditable(True)
        self.type_combo.addItem("")
        for t in types:
            self.type_combo.addItem(t)
        if selected_type:
            self.type_combo.setCurrentText(selected_type)

    def _build_ui(self) -> None:
        layout = QVBoxLayout(self)

        # Title
        row1 = QHBoxLayout()
        row1.addWidget(QLabel("Title:"))
        self.title_edit = QLineEdit()
        self.title_edit.setPlaceholderText("e.g., GitHub, Email, Bank")
        row1.addWidget(self.title_edit)
        layout.addLayout(row1)

        # Username
        row2 = QHBoxLayout()
        row2.addWidget(QLabel("Username:"))
        self.username_edit = QLineEdit()
        self.username_edit.setPlaceholderText("username or email")
        row2.addWidget(self.username_edit)
        layout.addLayout(row2)

        # Type
        row2b = QHBoxLayout()
        row2b.addWidget(QLabel("Type:"))
        self.type_combo = QComboBox()
        self.type_combo.setEditable(True)
        self.type_combo.setInsertPolicy(QComboBox.NoInsert)
        row2b.addWidget(self.type_combo)
        layout.addLayout(row2b)

        # Password
        row3 = QHBoxLayout()
        row3.addWidget(QLabel("Password:"))
        self.password_edit = QLineEdit()
        self.password_edit.setEchoMode(QLineEdit.Password)
        row3.addWidget(self.password_edit)
        self.show_chk = QCheckBox("Show")
        self.show_chk.toggled.connect(
            lambda checked: self.password_edit.setEchoMode(QLineEdit.Normal if checked else QLineEdit.Password)
        )
        row3.addWidget(self.show_chk)
        copy_btn = QPushButton("Copy")
        copy_btn.clicked.connect(lambda: QGuiApplication.clipboard().setText(self.password_edit.text()))
        row3.addWidget(copy_btn)
        layout.addLayout(row3)

        # Strength meter
        strength_row = QHBoxLayout()
        strength_row.addWidget(QLabel("Strength:"))
        self.strength_bar = QProgressBar()
        self.strength_bar.setRange(0, 100)
        self.strength_bar.setTextVisible(False)
        self.strength_label = QLabel("")
        strength_row.addWidget(self.strength_bar, 1)
        strength_row.addWidget(self.strength_label)
        layout.addLayout(strength_row)
        self.password_edit.textChanged.connect(self._update_strength)
        self._update_strength()

        # Generator options
        gen_row1 = QHBoxLayout()
        gen_row1.addWidget(QLabel("Length:"))
        self.spin_len = QSpinBox()
        self.spin_len.setRange(8, 128)
        self.spin_len.setValue(20)
        gen_row1.addWidget(self.spin_len)
        self.chk_lower = QCheckBox("a-z")
        self.chk_lower.setChecked(True)
        self.chk_upper = QCheckBox("A-Z")
        self.chk_upper.setChecked(True)
        self.chk_digits = QCheckBox("0-9")
        self.chk_digits.setChecked(True)
        self.chk_symbols = QCheckBox("Symbols")
        self.chk_symbols.setChecked(True)
        for w in (self.chk_lower, self.chk_upper, self.chk_digits, self.chk_symbols):
            gen_row1.addWidget(w)
        gen_btn = QPushButton("Generate")
        gen_btn.clicked.connect(self._generate_password)
        gen_row1.addWidget(gen_btn)
        layout.addLayout(gen_row1)

        # Extra
        layout.addWidget(QLabel("Extra info:"))
        self.extra_edit = QTextEdit()
        self.extra_edit.setPlaceholderText("any additional notes...")
        self.extra_edit.setFixedHeight(80)
        layout.addWidget(self.extra_edit)

        buttons = QDialogButtonBox(QDialogButtonBox.Ok | QDialogButtonBox.Cancel)
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)
        layout.addWidget(buttons)

    def get_values(self) -> tuple[str, str, str, str, str]:
        return (
            self.title_edit.text().strip(),
            self.username_edit.text().strip(),
            self.password_edit.text(),
            self.extra_edit.toPlainText().strip(),
            self.type_combo.currentText().strip(),
        )

    def _generate_password(self) -> None:
        length = int(self.spin_len.value())
        pools = []
        if self.chk_lower.isChecked():
            pools.append(string.ascii_lowercase)
        if self.chk_upper.isChecked():
            pools.append(string.ascii_uppercase)
        if self.chk_digits.isChecked():
            pools.append(string.digits)
        if self.chk_symbols.isChecked():
            pools.append("!@#$%^&*()-_=+[]{};:,<.>/?")
        if not pools:
            pools.append(string.ascii_letters + string.digits)
        all_chars = "".join(pools)
        # Ensure at least one char from each selected set
        password_chars = [secrets.choice(pool) for pool in pools]
        password_chars += [secrets.choice(all_chars) for _ in range(max(0, length - len(password_chars)))]
        secrets.SystemRandom().shuffle(password_chars)
        self.password_edit.setText("".join(password_chars[:length]))
        self._update_strength()

    def _update_strength(self) -> None:
        pwd = self.password_edit.text()
        length = len(pwd)
        has_lower = any(c.islower() for c in pwd)
        has_upper = any(c.isupper() for c in pwd)
        has_digit = any(c.isdigit() for c in pwd)
        has_symbol = any(not c.isalnum() for c in pwd)
        kinds = sum([has_lower, has_upper, has_digit, has_symbol])
        # Simple heuristic score 0-100
        length_score = min(length, 40) / 40.0 * 55.0
        variety_score = kinds / 4.0 * 45.0
        score = int(min(100, round(length_score + variety_score)))
        self.strength_bar.setValue(score)
        if score < 30:
            label = "Weak"
            color = "#cc3d3d"
        elif score < 60:
            label = "Fair"
            color = "#e6a23c"
        elif score < 80:
            label = "Strong"
            color = "#67c23a"
        else:
            label = "Very strong"
            color = "#18a058"
        self.strength_label.setText(label)
        # Colorize the progress bar chunk
        self.strength_bar.setStyleSheet(
            f"QProgressBar::chunk {{ background-color: {color}; }}"
        )
