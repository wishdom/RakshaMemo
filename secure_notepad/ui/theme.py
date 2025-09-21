from enum import Enum

from PySide6.QtGui import QPalette, QColor
from PySide6.QtWidgets import QApplication
from PySide6.QtCore import QSettings


class ThemeMode(str, Enum):
    LIGHT = "light"
    DARK = "dark"


def get_theme_mode() -> ThemeMode:
    settings = QSettings()
    mode = settings.value("appearance/theme", ThemeMode.LIGHT.value)
    try:
        return ThemeMode(mode)
    except Exception:
        return ThemeMode.LIGHT


def set_theme_mode(mode: ThemeMode) -> None:
    settings = QSettings()
    settings.setValue("appearance/theme", mode.value)


def apply_theme(app: QApplication, mode: ThemeMode) -> None:
    """Apply a modern Fusion palette for light or dark mode."""
    app.setStyle("Fusion")
    if mode == ThemeMode.DARK:
        palette = QPalette()
        palette.setColor(QPalette.Window, QColor(53, 53, 53))
        palette.setColor(QPalette.WindowText, QColor(220, 220, 220))
        palette.setColor(QPalette.Base, QColor(35, 35, 35))
        palette.setColor(QPalette.AlternateBase, QColor(53, 53, 53))
        palette.setColor(QPalette.ToolTipBase, QColor(220, 220, 220))
        palette.setColor(QPalette.ToolTipText, QColor(220, 220, 220))
        palette.setColor(QPalette.Text, QColor(220, 220, 220))
        palette.setColor(QPalette.Button, QColor(53, 53, 53))
        palette.setColor(QPalette.ButtonText, QColor(220, 220, 220))
        palette.setColor(QPalette.BrightText, QColor(255, 0, 0))
        palette.setColor(QPalette.Link, QColor(42, 130, 218))
        palette.setColor(QPalette.Highlight, QColor(42, 130, 218))
        palette.setColor(QPalette.HighlightedText, QColor(35, 35, 35))
        app.setPalette(palette)
    else:
        # Reset to default light palette
        app.setPalette(app.style().standardPalette())
