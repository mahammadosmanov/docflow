"""Small JSON-over-HTTPS client used by Supabase services."""

from __future__ import annotations

import json
import urllib.error
import urllib.request
from dataclasses import dataclass
from typing import Any


@dataclass(frozen=True)
class Response:
    status: int
    text: str

    def json(self) -> Any:
        return None if not self.text else json.loads(self.text)


class HttpError(Exception):
    def __init__(self, response: Response) -> None:
        super().__init__(f"HTTP {response.status}")
        self.response = response


def send_json(
    method: str,
    url: str,
    *,
    headers: dict[str, str] | None = None,
    body: Any = None,
    timeout: float = 15.0,
) -> Response:
    payload = None
    if body is not None:
        payload = json.dumps(body, ensure_ascii=False).encode("utf-8")
    request = urllib.request.Request(url, data=payload, method=method)
    request.add_header("Content-Type", "application/json")
    request.add_header("Accept", "application/json")
    for name, value in (headers or {}).items():
        request.add_header(name, value)
    try:
        with urllib.request.urlopen(request, timeout=timeout) as result:
            text = result.read().decode("utf-8")
            return Response(result.status, text)
    except urllib.error.HTTPError as error:
        text = error.read().decode("utf-8", errors="replace")
        return Response(error.code, text)


def require_success(response: Response) -> Response:
    if not 200 <= response.status < 300:
        raise HttpError(response)
    return response
