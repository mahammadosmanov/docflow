"""Runtime localization backed by the existing four ARB catalogs."""

from __future__ import annotations

import json
import locale as system_locale
from importlib.resources import files
from typing import Any


SUPPORTED = ("tr", "en", "ru", "az")


def detect_locale() -> str:
    language, _encoding = system_locale.getlocale()
    code = (language or "en").split("_", 1)[0].split("-", 1)[0]
    return code if code in SUPPORTED else "en"


class Strings:
    def __init__(self, locale: str | None = None) -> None:
        self.locale = locale if locale in SUPPORTED else detect_locale()
        resource = files("docflow.resources.l10n").joinpath(f"app_{self.locale}.arb")
        decoded = json.loads(resource.read_text(encoding="utf-8"))
        self._values: dict[str, Any] = decoded if isinstance(decoded, dict) else {}

    def t(self, key: str) -> str:
        value = self._values.get(key)
        return value if isinstance(value, str) else key

    def count(self, key: str, count: int) -> str:
        return self.t(key).replace("{count}", str(count))


def catalog_keys(locale: str) -> set[str]:
    resource = files("docflow.resources.l10n").joinpath(f"app_{locale}.arb")
    decoded = json.loads(resource.read_text(encoding="utf-8"))
    return {str(key) for key in decoded if not str(key).startswith("@")}
