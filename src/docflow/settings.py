"""Persistent application settings shared across platforms."""

from __future__ import annotations

import json
import os
import tempfile
import threading
from dataclasses import dataclass
from pathlib import Path
from typing import Any, ClassVar


def default_settings_path() -> Path:
    home = os.environ.get("HOME") or os.environ.get("USERPROFILE")
    root = Path(home) if home else Path(tempfile.gettempdir())
    return root / ".docflow" / "settings.json"


class SettingsStore:
    def __init__(self, path: Path | None = None) -> None:
        self.path = path or default_settings_path()
        self._lock = threading.RLock()
        self._data: dict[str, Any] = {}
        self.reload()

    def reload(self) -> None:
        with self._lock:
            try:
                decoded = json.loads(self.path.read_text(encoding="utf-8"))
                self._data = decoded if isinstance(decoded, dict) else {}
            except (FileNotFoundError, OSError, ValueError, TypeError):
                self._data = {}

    def params_for(self, op_id: str) -> dict[str, Any]:
        with self._lock:
            params = self._data.get("params")
            value = params.get(op_id) if isinstance(params, dict) else None
            return dict(value) if isinstance(value, dict) else {}

    def save_params(self, op_id: str, values: dict[str, Any]) -> None:
        with self._lock:
            clean = {
                key: value
                for key, value in values.items()
                if value is None or isinstance(value, (str, bool, int, float))
            }
            params = dict(self._data.get("params", {}))
            params[op_id] = clean
            self._data = {**self._data, "params": params}
            self._write()

    def section(self, name: str) -> dict[str, Any] | None:
        with self._lock:
            value = self._data.get(name)
            return dict(value) if isinstance(value, dict) else None

    def save_section(self, name: str, value: dict[str, Any] | None) -> None:
        with self._lock:
            next_data = dict(self._data)
            if value is None:
                next_data.pop(name, None)
            else:
                next_data[name] = value
            self._data = next_data
            self._write()

    def _write(self) -> None:
        """Write atomically so a power loss cannot truncate all settings."""
        try:
            self.path.parent.mkdir(parents=True, exist_ok=True)
            temporary = self.path.with_suffix(".tmp")
            temporary.write_text(
                json.dumps(self._data, ensure_ascii=False, separators=(",", ":")),
                encoding="utf-8",
            )
            temporary.replace(self.path)
        except OSError:
            # Settings are a convenience; document operations must stay usable.
            return


@dataclass
class AppPrefs:
    store: SettingsStore
    locale: str | None = None
    theme: str = "system"
    font: str = "system"
    font_scale: float = 1.0
    compact: bool = False

    MIN_SCALE: ClassVar[float] = 0.8
    MAX_SCALE: ClassVar[float] = 1.6
    LOCALES: ClassVar[set[str]] = {"tr", "en", "ru", "az"}
    THEMES: ClassVar[set[str]] = {"system", "light", "dark"}
    FONTS: ClassVar[set[str]] = {"system", "sans", "serif", "mono"}

    @classmethod
    def load(cls, store: SettingsStore) -> "AppPrefs":
        raw = store.section("prefs") or {}
        locale = raw.get("locale")
        theme = raw.get("theme")
        font = raw.get("font")
        scale = raw.get("fontScale")
        compact = raw.get("compact")
        return cls(
            store=store,
            locale=locale if isinstance(locale, str) and locale in cls.LOCALES else None,
            theme=theme if isinstance(theme, str) and theme in cls.THEMES else "system",
            font=font if isinstance(font, str) and font in cls.FONTS else "system",
            font_scale=cls.clamp_scale(float(scale)) if isinstance(scale, (int, float)) else 1.0,
            compact=compact if isinstance(compact, bool) else False,
        )

    @classmethod
    def clamp_scale(cls, value: float) -> float:
        return max(cls.MIN_SCALE, min(cls.MAX_SCALE, value))

    def update(
        self,
        *,
        locale: str | None = None,
        theme: str | None = None,
        font: str | None = None,
        font_scale: float | None = None,
        compact: bool | None = None,
        set_locale: bool = False,
    ) -> None:
        if set_locale:
            self.locale = locale if locale in self.LOCALES else None
        if theme is not None:
            self.theme = theme if theme in self.THEMES else "system"
        if font is not None:
            self.font = font if font in self.FONTS else "system"
        if font_scale is not None:
            self.font_scale = self.clamp_scale(font_scale)
        if compact is not None:
            self.compact = compact
        self.persist()

    def reset(self) -> None:
        self.locale = None
        self.theme = "system"
        self.font = "system"
        self.font_scale = 1.0
        self.compact = False
        self.persist()

    def persist(self) -> None:
        self.store.save_section(
            "prefs",
            {
                "locale": self.locale,
                "theme": self.theme,
                "font": self.font,
                "fontScale": self.font_scale,
                "compact": self.compact,
            },
        )
