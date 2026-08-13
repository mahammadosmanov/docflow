"""Long-lived JSON-lines bridge for the Tauri desktop shell.

The webview owns presentation only.  Authentication tokens, Supabase calls,
OCR, filesystem mutations, plans, and undo journals stay in this process.
Every request produces exactly one response; long operations may additionally
emit progress messages carrying the originating request id.
"""

from __future__ import annotations

import json
import os
import sys
import traceback
from collections.abc import Callable
from pathlib import Path
from typing import Any


def _configure_bundled_tesseract() -> None:
    """Point OCR at a Tesseract staged beside the packaged sidecar."""

    executable = "tesseract.exe" if sys.platform == "win32" else "tesseract"
    executable_dir = Path(sys.executable).resolve().parent
    candidates = [
        executable_dir / "tesseract" / executable,
        executable_dir.parent / "Resources" / "tesseract" / executable,
        Path("/opt/homebrew/bin/tesseract"),
        Path("/usr/local/bin/tesseract"),
    ]
    for candidate in candidates:
        if candidate.is_file():
            os.environ.setdefault("DOCFLOW_TESSERACT", str(candidate))
            break


_configure_bundled_tesseract()

from docflow.auth import AppUser, AuthService  # noqa: E402
from docflow import __version__  # noqa: E402
from docflow.cloud import CloudHistory  # noqa: E402
from docflow.contracts import Result  # noqa: E402
from docflow.engine import Engine  # noqa: E402
from docflow.extract.ocr import diagnostics as ocr_diagnostics  # noqa: E402
from docflow.progress import Progress  # noqa: E402
from docflow.settings import AppPrefs, SettingsStore  # noqa: E402


JsonObject = dict[str, Any]
ProgressEmitter = Callable[[Progress], None]


def _user_json(user: AppUser | None) -> JsonObject | None:
    if user is None:
        return None
    return {
        "id": user.id,
        "email": user.email,
        "username": user.username,
        "display": user.display,
        "allowed": user.allowed,
        "isAdmin": user.is_admin,
        "createdAt": user.created_at,
    }


class SidecarApplication:
    """Stateful application facade shared by all RPC requests."""

    def __init__(self, settings: SettingsStore | None = None) -> None:
        self.settings = settings or SettingsStore()
        self.prefs = AppPrefs.load(self.settings)
        self.auth = AuthService(self.settings)
        self.cloud = CloudHistory(self.auth)
        self.engine = Engine()

    def _auth_state(self) -> JsonObject:
        session = self.auth.session
        return {
            "signedIn": self.auth.signed_in,
            "allowed": self.auth.allowed,
            "isAdmin": self.auth.is_admin,
            "session": (
                {
                    "userId": session.user_id,
                    "email": session.email,
                    "expiresAt": session.expires_at,
                }
                if session is not None
                else None
            ),
            "user": _user_json(self.auth.me),
        }

    def _prefs_state(self) -> JsonObject:
        return {
            "locale": self.prefs.locale,
            "theme": self.prefs.theme,
            "font": self.prefs.font,
            "fontScale": self.prefs.font_scale,
            "compact": self.prefs.compact,
        }

    def _bootstrap(self) -> JsonObject:
        self.auth.restore()
        return {
            "protocol": 1,
            "version": __version__,
            "auth": self._auth_state(),
            "prefs": self._prefs_state(),
            "operations": [
                operation.to_json() for operation in self.engine.list_operations()
            ],
        }

    def _history(self, limit: int) -> JsonObject:
        if self.cloud.enabled:
            try:
                entries = self.cloud.list(limit)
                return {
                    "online": True,
                    "rows": [
                        {
                            "when": entry.created_at,
                            "opId": entry.op_id,
                            "moves": entry.applied,
                            "failed": entry.failed,
                            "journalId": entry.journal_id,
                            "steps": entry.steps,
                            "cloudId": entry.id,
                            "undone": entry.undone,
                            "fromCloud": True,
                            "userEmail": entry.user_email,
                        }
                        for entry in entries
                    ],
                }
            except Exception:
                pass
        return {
            "online": False,
            "rows": [
                {
                    "when": str(entry.get("ts", "")),
                    "opId": str(entry.get("opId", "")),
                    "moves": int(entry.get("moves", 0)),
                    "failed": 0,
                    "journalId": str(entry.get("journalId", "")),
                    "steps": None,
                    "cloudId": None,
                    "undone": False,
                    "fromCloud": False,
                    "userEmail": "",
                }
                for entry in self.engine.history(limit)
            ],
        }

    def _sync_result(self, op_id: str, result: Result) -> bool:
        if not self.cloud.enabled or not result.journal_id:
            return False
        journal = self.engine.journal(result.journal_id)
        steps = journal.get("steps")
        serialized = list(steps) if isinstance(steps, list) else []
        return self.cloud.push(
            op_id=op_id,
            applied=result.applied,
            failed=result.failed,
            journal_id=result.journal_id,
            summary=f"{result.applied}/{len(serialized)}",
            steps=serialized,
        )

    def dispatch(
        self,
        method: str,
        params: JsonObject,
        emit_progress: ProgressEmitter,
    ) -> Any:
        if method == "app/bootstrap":
            return self._bootstrap()
        if method == "system/ping":
            return {"ok": True, "protocol": 1}
        if method == "system/diagnostics":
            return {"ocr": ocr_diagnostics()}

        if method == "settings/get":
            return self._prefs_state()
        if method == "settings/update":
            locale_value = params.get("locale")
            self.prefs.update(
                locale=locale_value if isinstance(locale_value, str) else None,
                theme=(
                    str(params["theme"]) if "theme" in params else None
                ),
                font=str(params["font"]) if "font" in params else None,
                font_scale=(
                    float(params["fontScale"]) if "fontScale" in params else None
                ),
                compact=(
                    bool(params["compact"]) if "compact" in params else None
                ),
                set_locale="locale" in params,
            )
            return self._prefs_state()
        if method == "settings/reset":
            self.prefs.reset()
            return self._prefs_state()
        if method == "settings/params":
            return self.settings.params_for(str(params.get("opId", "")))
        if method == "settings/saveParams":
            values = params.get("values")
            self.settings.save_params(
                str(params.get("opId", "")),
                values if isinstance(values, dict) else {},
            )
            return {"ok": True}

        if method == "auth/state":
            return self._auth_state()
        if method == "auth/signIn":
            self.auth.sign_in(
                str(params.get("identifier", "")),
                str(params.get("password", "")),
            )
            return self._auth_state()
        if method == "auth/signUp":
            self.auth.sign_up(
                str(params.get("email", "")),
                str(params.get("password", "")),
                str(params.get("username", "")),
            )
            return {"ok": True}
        if method == "auth/signOut":
            self.auth.sign_out()
            return self._auth_state()
        if method == "auth/usernameAvailable":
            return self.auth.username_available(str(params.get("username", "")))
        if method == "auth/loadMe":
            self.auth.load_me()
            return self._auth_state()
        if method == "auth/setUsername":
            self.auth.set_username(str(params.get("username", "")))
            return self._auth_state()
        if method == "auth/listUsers":
            return [_user_json(user) for user in self.auth.list_users()]
        if method == "auth/setFlags":
            allowed = params.get("allowed")
            is_admin = params.get("isAdmin")
            self.auth.set_flags(
                str(params.get("userId", "")),
                allowed=allowed if isinstance(allowed, bool) else None,
                is_admin=is_admin if isinstance(is_admin, bool) else None,
            )
            return {"ok": True}

        if method == "engine/listOperations":
            return [
                operation.to_json() for operation in self.engine.list_operations()
            ]
        if method == "engine/preview":
            values = params.get("values")
            plan = self.engine.preview(
                str(params.get("opId", "")),
                values if isinstance(values, dict) else {},
                progress=emit_progress,
            )
            return plan.to_json()
        if method == "engine/apply":
            op_id = str(params.get("opId", ""))
            result = self.engine.apply(
                str(params.get("planId", "")),
                progress=emit_progress,
            )
            payload = result.to_json()
            payload["cloudSynced"] = self._sync_result(op_id, result)
            return payload
        if method == "engine/revise":
            rows = params.get("rows")
            plan = self.engine.revise(
                str(params.get("planId", "")),
                {"rows": list(rows) if isinstance(rows, list) else []},
            )
            return plan.to_json()
        if method == "engine/copyTable":
            sheet = params.get("sheet")
            return {
                "count": self.engine.copy_plan_table(
                    str(params.get("planId", "")),
                    sheet if isinstance(sheet, str) and sheet else None,
                )
            }
        if method == "engine/previewTable":
            sheet = params.get("sheet")
            return self.engine.preview_plan_table(
                str(params.get("planId", "")),
                sheet if isinstance(sheet, str) and sheet else None,
            )
        if method == "engine/reviewScan":
            return self.engine.review_scan(str(params.get("destination", "")))
        if method == "engine/reviewPreview":
            decisions = params.get("decisions")
            plan = self.engine.review_preview(
                str(params.get("destination", "")),
                list(decisions) if isinstance(decisions, list) else [],
            )
            return plan.to_json()
        if method == "engine/undo":
            raw_steps = params.get("steps")
            steps = list(raw_steps) if isinstance(raw_steps, list) else None
            destination = params.get("destination")
            result = self.engine.undo(
                str(params.get("journalId", "")),
                destination=destination if isinstance(destination, str) else None,
                steps=steps,
            )
            cloud_id = params.get("cloudId")
            if result.failed == 0 and isinstance(cloud_id, str):
                self.cloud.mark_undone(cloud_id)
            return result.to_json()
        if method == "history/list":
            return self._history(int(params.get("limit", 50)))

        raise KeyError(f"unknown method: {method}")


class JsonLineServer:
    def __init__(self, application: SidecarApplication | None = None) -> None:
        self.application = application or SidecarApplication()

    @staticmethod
    def _write(payload: JsonObject) -> None:
        sys.stdout.write(
            json.dumps(payload, ensure_ascii=False, separators=(",", ":")) + "\n"
        )
        sys.stdout.flush()

    def _handle(self, request: JsonObject) -> None:
        request_id = request.get("id")
        method = request.get("method")
        params = request.get("params")
        if not isinstance(method, str):
            self._write(
                {
                    "type": "response",
                    "id": request_id,
                    "error": {"message": "method must be a string"},
                }
            )
            return

        def progress(event: Progress) -> None:
            self._write(
                {
                    "type": "progress",
                    "requestId": request_id,
                    "event": {
                        "phase": event.phase,
                        "done": event.done,
                        "total": event.total,
                        "subject": event.subject,
                    },
                }
            )

        try:
            result = self.application.dispatch(
                method,
                params if isinstance(params, dict) else {},
                progress,
            )
            self._write(
                {
                    "type": "response",
                    "id": request_id,
                    "result": result,
                }
            )
        except Exception as error:
            traceback.print_exc(file=sys.stderr)
            self._write(
                {
                    "type": "response",
                    "id": request_id,
                    "error": {
                        "kind": type(error).__name__,
                        "message": str(error) or type(error).__name__,
                    },
                }
            )

    def serve(self) -> int:
        self._write({"type": "ready", "protocol": 1, "version": __version__})
        for line in sys.stdin:
            try:
                request = json.loads(line)
            except (json.JSONDecodeError, TypeError) as error:
                self._write(
                    {
                        "type": "response",
                        "id": None,
                        "error": {"message": f"invalid JSON: {error}"},
                    }
                )
                continue
            if not isinstance(request, dict):
                self._write(
                    {
                        "type": "response",
                        "id": None,
                        "error": {"message": "request must be an object"},
                    }
                )
                continue
            self._handle(request)
        return 0


def main() -> int:
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", line_buffering=True)
    if hasattr(sys.stderr, "reconfigure"):
        sys.stderr.reconfigure(encoding="utf-8", line_buffering=True)
    if "--ping" in sys.argv:
        print('{"ok":true,"protocol":1}')
        return 0
    if len(sys.argv) >= 4 and sys.argv[1] == "--ocr-smoke":
        from docflow.extract import extract_first_page

        match, _method = extract_first_page(sys.argv[2])
        return 0 if match.code == sys.argv[3] else 2
    return JsonLineServer().serve()


if __name__ == "__main__":
    raise SystemExit(main())
