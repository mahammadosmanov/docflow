"""Optional Supabase-backed shared operation history."""

from __future__ import annotations

import logging
import socket
import urllib.parse
from dataclasses import dataclass
from datetime import datetime
from typing import Any

from .auth import SUPABASE_KEY, SUPABASE_URL, AuthService
from .http import send_json

log = logging.getLogger(__name__)


@dataclass(frozen=True)
class CloudEntry:
    id: str
    created_at: str
    op_id: str
    applied: int
    failed: int
    journal_id: str
    summary: str
    steps: list[dict[str, Any]]
    undone: bool
    user_email: str

    @property
    def when(self) -> datetime | None:
        try:
            return datetime.fromisoformat(self.created_at.replace("Z", "+00:00"))
        except ValueError:
            return None

    @classmethod
    def from_json(cls, raw: dict[str, Any]) -> "CloudEntry":
        steps = raw.get("steps")
        return cls(
            id=str(raw.get("id", "")),
            created_at=str(raw.get("created_at", "")),
            op_id=str(raw.get("op_id", "")),
            applied=int(raw.get("applied", 0)),
            failed=int(raw.get("failed", 0)),
            journal_id=str(raw.get("journal_id", "")),
            summary=str(raw.get("summary", "")),
            steps=list(steps) if isinstance(steps, list) else [],
            undone=raw.get("undone") is True,
            user_email=str(raw.get("user_email", "")),
        )


class CloudHistory:
    def __init__(
        self,
        auth: AuthService,
        *,
        url: str = SUPABASE_URL,
        key: str = SUPABASE_KEY,
    ) -> None:
        self.auth = auth
        self.url = url.rstrip("/")
        self.key = key

    @property
    def enabled(self) -> bool:
        return bool(
            self.url
            and self.key
            and self.auth.signed_in
            and self.auth.allowed
        )

    def _headers(self) -> dict[str, str]:
        token = self.auth.valid_token()
        return {
            "apikey": self.key,
            **({"Authorization": f"Bearer {token}"} if token else {}),
        }

    def push(
        self,
        *,
        op_id: str,
        applied: int,
        failed: int,
        journal_id: str,
        summary: str,
        steps: list[dict[str, Any]],
    ) -> bool:
        if not self.enabled:
            return False
        try:
            response = send_json(
                "POST",
                f"{self.url}/rest/v1/history",
                headers={**self._headers(), "Prefer": "return=minimal"},
                body={
                    "device": socket.gethostname(),
                    "op_id": op_id,
                    "applied": applied,
                    "failed": failed,
                    "journal_id": journal_id,
                    "summary": summary,
                    "steps": steps,
                    "user_id": self.auth.session.user_id if self.auth.session else None,
                    "user_email": self.auth.session.email if self.auth.session else None,
                },
            )
            return 200 <= response.status < 300
        except Exception as error:
            log.info("cloud history push failed: %s", error)
            return False

    def list(self, limit: int = 50) -> list[CloudEntry]:
        if not self.enabled:
            return []
        response = send_json(
            "GET",
            f"{self.url}/rest/v1/history?select=*&order=created_at.desc&limit={limit}",
            headers=self._headers(),
        )
        if not 200 <= response.status < 300:
            raise RuntimeError(f"cloud history [{response.status}]: {response.text}")
        raw = response.json()
        return [CloudEntry.from_json(item) for item in raw]

    def mark_undone(self, entry_id: str) -> None:
        if not self.enabled or not entry_id:
            return
        safe_id = urllib.parse.quote(entry_id, safe="")
        try:
            send_json(
                "PATCH",
                f"{self.url}/rest/v1/history?id=eq.{safe_id}",
                headers={**self._headers(), "Prefer": "return=minimal"},
                body={"undone": True},
            )
        except Exception as error:
            log.info("cloud history mark-undone failed: %s", error)
