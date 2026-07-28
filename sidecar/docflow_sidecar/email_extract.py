"""Extract RFI request fields from pasted email text.

The offline path is deterministic and private. Gemini is optional; its output
is still treated as untrusted input and validated against workbook lookups by
the RFI operation before it reaches a plan.
"""
from __future__ import annotations

import json
import os
import re
import urllib.error
import urllib.request
from typing import Any

DEFAULT_GEMINI_MODEL = "gemini-3.1-flash-lite"
FALLBACK_GEMINI_MODELS = (
    "gemini-3.5-flash-lite",
    "gemini-3.5-flash",
)
MAX_EMAIL_CHARS = 30_000
MAX_ERROR_CHARS = 300

_DISCIPLINE = re.compile(r"(?<![A-Z])(CIV|MEC)(?![A-Z])", re.IGNORECASE)
_COUNT_PATTERNS = (
    re.compile(
        r"\b(\d{1,3})\s*(?:CIV|MEC)?\s*RFI(?:\s+numbers?)?\b",
        re.IGNORECASE,
    ),
    re.compile(
        r"\b(\d{1,3})\s*(?:adet|ədəd|pcs?\.?|rfi(?:\s+numbers?)?|"
        r"numara(?:s[ıi])?|n[oö]mr[əe])\b",
        re.IGNORECASE,
    ),
    re.compile(
        r"\b(?:give|create|open|ver|olu[sş]tur|ay[ıi]r)\D{0,18}"
        r"(\d{1,3})\s*(?:new\s+)?(?:rfi|number|numara)",
        re.IGNORECASE,
    ),
)
_TAG_LINE = re.compile(
    r"(?im)^\s*(?:tag(?:\s+no\.?)?|etiket)\s*[:=\-]\s*([^\r\n]+)"
)
_AREA_LINE = re.compile(r"(?im)^\s*area\s*[:=\-]\s*([^\r\n]+)")
_LOCATION_LINE = re.compile(
    r"(?im)^\s*(?:inspection\s+)?location\s*[:=\-]\s*([^\r\n]+)"
)
_SUBCONTRACTOR_LINE = re.compile(
    r"(?im)^\s*subcontractor\s*[:=\-]\s*([^\r\n]+)"
)

_ROW_SCHEMA = {
    "type": "OBJECT",
    "properties": {
        "rfiIndex": {"type": "INTEGER", "minimum": 1, "maximum": 100},
        "itpNo": {"type": "STRING"},
        "itemNo": {"type": "STRING"},
        "tagNo": {"type": "STRING"},
        "area": {"type": "STRING"},
        "location": {"type": "STRING"},
        "subcontractor": {"type": "STRING"},
    },
    "required": [
        "rfiIndex",
        "itpNo",
        "itemNo",
        "tagNo",
        "area",
        "location",
        "subcontractor",
    ],
}
_RESPONSE_SCHEMA = {
    "type": "OBJECT",
    "properties": {
        "discipline": {
            "type": "STRING",
            "enum": ["CIV", "MEC"],
        },
        "count": {"type": "INTEGER", "minimum": 0, "maximum": 100},
        "rows": {"type": "ARRAY", "items": _ROW_SCHEMA, "maxItems": 100},
        "confidence": {"type": "NUMBER", "minimum": 0, "maximum": 1},
        "notes": {"type": "ARRAY", "items": {"type": "STRING"}},
    },
    "required": ["discipline", "count", "rows", "confidence", "notes"],
}


def _as_json_schema(value: Any) -> Any:
    """Convert legacy Gemini Schema enum names to JSON Schema type names."""
    if isinstance(value, dict):
        return {
            key: (
                item.lower()
                if key == "type" and isinstance(item, str)
                else _as_json_schema(item)
            )
            for key, item in value.items()
        }
    if isinstance(value, list):
        return [_as_json_schema(item) for item in value]
    return value


_CURRENT_RESPONSE_SCHEMA = _as_json_schema(_RESPONSE_SCHEMA)


def detect_discipline(text: str) -> str | None:
    match = _DISCIPLINE.search(text.upper())
    return match.group(1).upper() if match else None


def _explicit_count(text: str) -> int | None:
    for pattern in _COUNT_PATTERNS:
        match = pattern.search(text)
        if match:
            value = int(match.group(1))
            return value if 1 <= value <= 100 else None
    return None


def _split_values(value: str) -> list[str]:
    return [
        item.strip(" \t.;")
        for item in re.split(r"\s*[,;]\s*|\s+/\s+", value)
        if item.strip(" \t.;")
    ]


def _first_line(pattern: re.Pattern[str], text: str) -> str:
    match = pattern.search(text)
    return match.group(1).strip() if match else ""


def _mentioned_known(text: str, values: list[str]) -> list[str]:
    upper = text.upper()
    return [value for value in values if value and value.upper() in upper]


def _expand_compact_tags(text: str) -> list[str]:
    expanded: list[str] = []
    for match in re.finditer(
        r"\b([A-Z][A-Z0-9]*-)(\d+(?:\s*/\s*\d+)+)\b",
        text.upper(),
    ):
        prefix = match.group(1)
        expanded.extend(
            f"{prefix}{number}"
            for number in re.split(r"\s*/\s*", match.group(2))
        )
    return expanded


def _extract_tag_tokens(text: str) -> list[str]:
    """Find tag-shaped tokens without mistaking ITP or Item numbers for tags."""
    tokens = re.findall(
        r"(?<![A-Z0-9])([A-Z0-9]+(?:[_-][A-Z0-9]+)+(?:/[A-Z0-9]+)*)"
        r"(?![A-Z0-9])",
        text.upper(),
    )
    return list(
        dict.fromkeys(
            token
            for token in tokens
            if "QMS-ITP" not in token
            and not token.startswith(("AZE-SUN-", "CIV-", "MEC-"))
        )
    )


def _abbreviated_item(
    text: str,
    itps: list[str],
    itp_to_items: dict[str, list[str]],
) -> str:
    candidates = (
        itp_to_items[itps[0]]
        if len(itps) == 1
        else [item for items in itp_to_items.values() for item in items]
    )
    matches: list[str] = []
    for item in candidates:
        numbers = re.findall(r"\d+", item)
        if len(numbers) < 2:
            continue
        short = ".".join(numbers[-2:])
        if re.search(rf"(?<![\d.]){re.escape(short)}(?![\d.])", text):
            matches.append(item)
    unique = list(dict.fromkeys(matches))
    return unique[0] if len(unique) == 1 else ""


def offline_extract(
    text: str,
    *,
    discipline: str,
    itp_to_items: dict[str, list[str]],
    known_tags: list[str],
    known_subcontractors: list[str],
) -> dict[str, Any]:
    clean = text[:MAX_EMAIL_CHARS].replace(r"\_", "_")
    clean = re.sub(r"[*`]+", "", clean)
    all_items = [item for items in itp_to_items.values() for item in items]
    items = _mentioned_known(clean, all_items)
    itps = _mentioned_known(clean, list(itp_to_items))

    if not itps:
        number_match = re.search(
            rf"\bITP(?:\s+NO\.?)?\s*[:#\-]?\s*0*(\d{{1,4}})\b",
            clean,
            re.IGNORECASE,
        )
        if number_match:
            suffix = int(number_match.group(1))
            itps = [
                label
                for label in itp_to_items
                if re.search(rf"{suffix:04d}$", label)
            ]
    if not items:
        abbreviated = _abbreviated_item(clean, itps, itp_to_items)
        items = [abbreviated] if abbreviated else []

    tag_values: list[str] = []
    for match in _TAG_LINE.finditer(clean):
        tag_values.extend(_split_values(match.group(1)))
    if not tag_values:
        compact_tags = _expand_compact_tags(clean)
        known = _mentioned_known(clean, known_tags)
        tag_values = compact_tags or known or _extract_tag_tokens(clean)
    tag_values = list(dict.fromkeys(tag_values))

    count = _explicit_count(clean)
    if count is None and tag_values:
        count = len(tag_values)

    area = _first_line(_AREA_LINE, clean)
    location = _first_line(_LOCATION_LINE, clean)
    subcontractor = _first_line(_SUBCONTRACTOR_LINE, clean)
    if not subcontractor:
        mentioned = _mentioned_known(clean, known_subcontractors)
        subcontractor = mentioned[0] if mentioned else ""

    shared = {
        "itpNo": itps[0] if len(itps) == 1 else "",
        "itemNo": items[0] if len(items) == 1 else "",
        "area": area,
        "location": location,
        "subcontractor": subcontractor,
    }
    row_count = max(count or 0, len(tag_values), 1)
    rows = [
        {
            **shared,
            "rfiIndex": min(index + 1, count or row_count),
            "tagNo": tag_values[index] if index < len(tag_values) else "",
        }
        for index in range(row_count)
    ]
    notes: list[str] = []
    if len(items) > 1:
        notes.append("Multiple Item No. values were mentioned; review each row.")
    if len(itps) > 1:
        notes.append("Multiple ITP No. values were mentioned; review each row.")
    if count is None:
        notes.append("RFI count was not explicit; the form count was kept.")

    recognized = sum(
        bool(value)
        for value in (
            detect_discipline(clean),
            count,
            items,
            itps,
            tag_values,
            area,
            location,
            subcontractor,
        )
    )
    return {
        "provider": "offline",
        "discipline": detect_discipline(clean) or discipline,
        "count": count or 0,
        "rows": rows,
        "confidence": min(0.95, 0.35 + recognized * 0.08),
        "notes": notes,
    }


def _gemini_prompt(
    text: str,
    discipline: str,
    itp_to_items: dict[str, list[str]],
) -> str:
    mapping = json.dumps(itp_to_items, ensure_ascii=False, separators=(",", ":"))
    return (
        "Extract an RFI inspection request from the email below. "
        "Use only facts explicitly present in the email; never invent values. "
        "Discipline must be CIV or MEC; use the currently selected discipline "
        "when the email does not state one. Other unknown strings must be empty. "
        "Count means unique new RFI numbers, not "
        "Excel rows. When several tags/areas are requested for the same RFI, "
        "return multiple rows with the same rfiIndex, but keep the unique RFI "
        "count stated by the sender. rfiIndex is 1-based and identifies which "
        "new RFI number each Excel row belongs to. "
        "ITP and Item values must exactly match this allowed mapping: "
        f"{mapping}. The currently selected discipline is {discipline}. "
        "Return short review notes for ambiguity.\n\nEMAIL:\n"
        + text[:MAX_EMAIL_CHARS]
    )


def gemini_extract(
    text: str,
    *,
    api_key: str,
    discipline: str,
    itp_to_items: dict[str, list[str]],
    timeout: int = 45,
) -> dict[str, Any]:
    key = api_key.strip() or os.environ.get("GEMINI_API_KEY", "").strip()
    if not key:
        raise ValueError("Gemini API key is missing")
    configured = os.environ.get("DOCFLOW_GEMINI_MODEL", "").strip()
    models = list(
        dict.fromkeys(
            [configured, DEFAULT_GEMINI_MODEL, *FALLBACK_GEMINI_MODELS]
        )
    )
    models = [model for model in models if model]
    if any(not re.fullmatch(r"[A-Za-z0-9._-]+", model) for model in models):
        raise ValueError("Gemini model name is invalid")

    base_body = {
        "contents": [
            {
                "role": "user",
                "parts": [
                    {"text": _gemini_prompt(text, discipline, itp_to_items)}
                ],
            }
        ],
    }
    request_formats = (
        (
            "current",
            {
                "responseFormat": {
                    "text": {
                        "mimeType": "APPLICATION_JSON",
                        "schema": _CURRENT_RESPONSE_SCHEMA,
                    }
                }
            },
        ),
        (
            "json-schema",
            {
                "responseMimeType": "application/json",
                "responseJsonSchema": _CURRENT_RESPONSE_SCHEMA,
            },
        ),
        (
            "legacy",
            {
                "responseMimeType": "application/json",
                "responseSchema": _RESPONSE_SCHEMA,
            },
        ),
    )
    payload: dict[str, Any] | None = None
    selected_model = ""
    unavailable: list[str] = []
    invalid_attempts: list[str] = []
    for model in models:
        for format_name, generation_config in request_formats:
            body = {
                **base_body,
                "generationConfig": generation_config,
            }
            request = urllib.request.Request(
                "https://generativelanguage.googleapis.com/v1beta/"
                f"models/{model}:generateContent",
                data=json.dumps(body, ensure_ascii=False).encode("utf-8"),
                headers={
                    "Content-Type": "application/json",
                    "x-goog-api-key": key,
                },
                method="POST",
            )
            try:
                with urllib.request.urlopen(request, timeout=timeout) as response:
                    payload = json.loads(response.read().decode("utf-8"))
                selected_model = model
                break
            except urllib.error.HTTPError as exc:
                detail = exc.read().decode("utf-8", "replace")[:MAX_ERROR_CHARS]
                if exc.code == 404:
                    unavailable.append(model)
                    break
                if exc.code == 400:
                    invalid_attempts.append(f"{model}/{format_name}: {detail}")
                    continue
                raise RuntimeError(
                    f"Gemini HTTP {exc.code} ({model}/{format_name}): {detail}"
                ) from exc
            except (OSError, urllib.error.URLError) as exc:
                raise RuntimeError(f"Gemini connection failed: {exc}") from exc
        if payload is not None:
            break
    if payload is None:
        if invalid_attempts:
            details = " | ".join(invalid_attempts)
            raise RuntimeError(
                f"Gemini rejected all supported request formats: {details}"
            )
        tried = ", ".join(dict.fromkeys(unavailable))
        raise RuntimeError(f"No supported Gemini model was available: {tried}")

    try:
        raw = payload["candidates"][0]["content"]["parts"][0]["text"]
        result = json.loads(raw)
    except (KeyError, IndexError, TypeError, json.JSONDecodeError) as exc:
        raise RuntimeError("Gemini returned an invalid structured response") from exc
    if not isinstance(result, dict):
        raise RuntimeError("Gemini response is not an object")
    result["provider"] = "gemini"
    result["model"] = selected_model
    return result


def extract_email(
    text: str,
    *,
    provider: str,
    api_key: str,
    discipline: str,
    itp_to_items: dict[str, list[str]],
    known_tags: list[str],
    known_subcontractors: list[str],
) -> tuple[dict[str, Any], str | None]:
    offline = offline_extract(
        text,
        discipline=discipline,
        itp_to_items=itp_to_items,
        known_tags=known_tags,
        known_subcontractors=known_subcontractors,
    )
    if provider != "gemini":
        return offline, None
    try:
        result = gemini_extract(
            text,
            api_key=api_key,
            discipline=discipline,
            itp_to_items=itp_to_items,
        )
        return result, None
    except Exception as exc:  # noqa: BLE001 - offline fallback keeps the workflow usable
        message = f"{type(exc).__name__}: {exc}"
        return offline, message[:MAX_ERROR_CHARS]
