from __future__ import annotations

import json
import time
from pathlib import Path

import pytest

from docflow.auth import AppUser, AuthService, AuthSession, UsernameRule
from docflow.engine import Engine
from docflow.http import Response
from docflow.i18n import SUPPORTED, Strings, catalog_keys
from docflow.settings import AppPrefs, SettingsStore


def test_i18n_catalogs_have_identical_key_sets() -> None:
    english = catalog_keys("en")
    assert english
    for locale in SUPPORTED:
        assert catalog_keys(locale) == english


def test_missing_translation_is_visible() -> None:
    strings = Strings("en")
    assert strings.t("action.apply") == "Apply"
    assert strings.t("missing.key") == "missing.key"
    assert strings.count("plan.summary", 3) == "3 files will be processed"


def test_settings_round_trip_filters_operation_values(tmp_path: Path) -> None:
    path = tmp_path / "settings.json"
    store = SettingsStore(path)
    store.save_params(
        "rfi.process_scans",
        {
            "source": "/in",
            "dest": "/out",
            "keep_original": True,
            "unsafe": [1, 2, 3],
        },
    )
    restored = SettingsStore(path)
    assert restored.params_for("rfi.process_scans") == {
        "source": "/in",
        "dest": "/out",
        "keep_original": True,
    }
    assert restored.params_for("unknown") == {}


def test_settings_write_is_valid_json(tmp_path: Path) -> None:
    path = tmp_path / "settings.json"
    store = SettingsStore(path)
    store.save_section("session", {"accessToken": "token"})
    assert json.loads(path.read_text())["session"]["accessToken"] == "token"
    assert not path.with_suffix(".tmp").exists()


def test_preferences_validate_persist_and_reset(tmp_path: Path) -> None:
    path = tmp_path / "settings.json"
    path.write_text(
        json.dumps(
            {
                "prefs": {
                    "locale": 42,
                    "theme": "neon",
                    "font": "comic",
                    "fontScale": "large",
                }
            }
        )
    )
    store = SettingsStore(path)
    prefs = AppPrefs.load(store)
    assert prefs.locale is None
    assert prefs.theme == "system"
    assert prefs.font == "system"
    assert prefs.font_scale == 1.0

    prefs.update(
        locale="az",
        set_locale=True,
        theme="dark",
        font="mono",
        font_scale=9.0,
        compact=True,
    )
    restored = AppPrefs.load(SettingsStore(path))
    assert restored.locale == "az"
    assert restored.theme == "dark"
    assert restored.font == "mono"
    assert restored.font_scale == AppPrefs.MAX_SCALE
    assert restored.compact is True

    restored.reset()
    defaults = AppPrefs.load(SettingsStore(path))
    assert defaults.locale is None
    assert defaults.theme == "system"
    assert defaults.font == "system"
    assert defaults.font_scale == 1.0
    assert defaults.compact is False


def test_auth_session_legacy_disk_contract_and_expiry() -> None:
    now = int(time.time())
    current = AuthSession("a", "r", "u", "e@example.com", now + 3600)
    restored = AuthSession.from_json(current.to_json())
    assert restored is not None
    assert restored.access_token == "a"
    assert restored.refresh_token == "r"
    assert restored.expired is False
    assert AuthSession("a", "r", "u", "e", now + 30).expired is True
    assert AuthSession.from_json({"accessToken": "missing-refresh"}) is None


def test_app_user_defaults_are_fail_closed() -> None:
    user = AppUser.from_json({"id": "u", "email": "e@example.com"})
    assert user.allowed is False
    assert user.is_admin is False
    assert user.display == "e@example.com"
    named = AppUser.from_json(
        {"id": "u", "email": "e@example.com", "username": "ali_k"}
    )
    assert named.display == "ali_k"


def test_auth_email_sign_in_preserves_supabase_contract(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    calls: list[tuple[str, str, object]] = []

    def fake_send(
        method: str,
        url: str,
        *,
        headers: dict[str, str] | None = None,
        body: object = None,
        timeout: float = 15.0,
    ) -> Response:
        del headers, timeout
        calls.append((method, url, body))
        if "/auth/v1/token" in url:
            return Response(
                200,
                json.dumps(
                    {
                        "access_token": "access",
                        "refresh_token": "refresh",
                        "expires_at": int(time.time()) + 3600,
                        "user": {"id": "user-id", "email": "e@example.com"},
                    }
                ),
            )
        return Response(
            200,
            json.dumps(
                [
                    {
                        "id": "user-id",
                        "email": "e@example.com",
                        "allowed": True,
                        "is_admin": False,
                    }
                ]
            ),
        )

    monkeypatch.setattr("docflow.auth.send_json", fake_send)
    store = SettingsStore(tmp_path / "settings.json")
    auth = AuthService(store, url="https://project.supabase.co", key="publishable")
    auth.sign_in("e@example.com", "password")

    assert auth.allowed is True
    assert auth.session is not None
    assert AuthSession.from_json(SettingsStore(store.path).section("session")) is not None
    assert calls[0][0] == "POST"
    assert "grant_type=password" in calls[0][1]
    assert calls[0][2] == {"email": "e@example.com", "password": "password"}
    assert calls[1][0] == "GET"
    assert "/rest/v1/app_users" in calls[1][1]


def test_cloud_history_uses_user_token_and_publishable_key(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    from docflow.cloud import CloudHistory

    captured: dict[str, object] = {}

    def fake_send(
        method: str,
        url: str,
        *,
        headers: dict[str, str] | None = None,
        body: object = None,
        timeout: float = 15.0,
    ) -> Response:
        del timeout
        captured.update(method=method, url=url, headers=headers, body=body)
        return Response(201, "")

    monkeypatch.setattr("docflow.cloud.send_json", fake_send)
    auth = AuthService(
        SettingsStore(tmp_path / "settings.json"),
        url="https://project.supabase.co",
        key="publishable",
    )
    auth.session = AuthSession(
        "access", "refresh", "user-id", "e@example.com", int(time.time()) + 3600
    )
    auth.me = AppUser("user-id", "e@example.com", allowed=True)
    cloud = CloudHistory(
        auth,
        url="https://project.supabase.co",
        key="publishable",
    )
    assert cloud.push(
        op_id="rfi.process_scans",
        applied=1,
        failed=0,
        journal_id="journal",
        summary="1/1",
        steps=[],
    )
    assert captured["method"] == "POST"
    headers = captured["headers"]
    assert isinstance(headers, dict)
    assert headers["apikey"] == "publishable"
    assert headers["Authorization"] == "Bearer access"
    assert "service_role" not in json.dumps(captured)


@pytest.mark.parametrize(
    ("value", "expected"),
    [
        ("ali", True),
        ("ali_k.2-x", True),
        ("a" * 24, True),
        ("  ali  ", True),
        ("al", False),
        ("a" * 25, False),
        ("ali k", False),
        ("ali@k", False),
        ("alı", False),
    ],
)
def test_username_rule_matches_database_contract(value: str, expected: bool) -> None:
    assert UsernameRule.ok(value) is expected


def test_engine_exposes_schema_without_rpc() -> None:
    engine = Engine()
    operations = engine.list_operations()
    assert [operation.id for operation in operations] == [
        "rfi.process_scans",
        "rfi.log_add",
        "files.batch_print",
        "rfi.stage_signed_first_pages",
    ]
    schema = operations[0].schema.to_json()
    assert [item["name"] for item in schema] == [
        "source",
        "dest",
        "organize_into_rfi_folders",
        "keep_original",
    ]


def test_engine_validation_returns_non_actionable_plan() -> None:
    plan = Engine().preview("rfi.process_scans", {})
    assert plan.steps == []
    assert {warning.key for warning in plan.warnings} == {
        "error.param.required.source",
        "error.param.required.dest",
    }


def test_sidecar_contract_keeps_tokens_out_of_webview(tmp_path: Path) -> None:
    from docflow.sidecar import SidecarApplication

    application = SidecarApplication(SettingsStore(tmp_path / "settings.json"))
    application.auth.session = AuthSession(
        "secret-access",
        "secret-refresh",
        "user-id",
        "e@example.com",
        int(time.time()) + 3600,
    )
    application.auth.me = AppUser(
        "user-id",
        "e@example.com",
        username="operator",
        allowed=True,
        is_admin=False,
    )

    payload = application.dispatch("auth/state", {}, lambda _event: None)
    serialized = json.dumps(payload)
    assert payload["signedIn"] is True
    assert payload["user"]["display"] == "operator"
    assert "secret-access" not in serialized
    assert "secret-refresh" not in serialized
    assert "accessToken" not in serialized
    assert "refreshToken" not in serialized


def test_sidecar_schema_and_preferences_are_json_ready(tmp_path: Path) -> None:
    from docflow.sidecar import SidecarApplication

    application = SidecarApplication(SettingsStore(tmp_path / "settings.json"))
    operations = application.dispatch(
        "engine/listOperations", {}, lambda _event: None
    )
    assert operations[0]["id"] == "rfi.process_scans"
    prefs = application.dispatch(
        "settings/update",
        {"locale": "az", "theme": "dark", "fontScale": 1.25},
        lambda _event: None,
    )
    assert prefs == {
        "locale": "az",
        "theme": "dark",
        "font": "system",
        "fontScale": 1.25,
        "compact": False,
    }
