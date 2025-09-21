from PySide6.QtCore import QSettings


def get_setting(key: str, default=None):
    return QSettings().value(key, default)


def set_setting(key: str, value) -> None:
    QSettings().setValue(key, value)
