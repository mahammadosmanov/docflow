"""Supabase email/password authentication and application allow-list."""

from __future__ import annotations

import json
import re
import time
import urllib.parse
from dataclasses import dataclass
from typing import Any

from .http import Response, send_json
from .settings import SettingsStore


SUPABASE_URL = "https://ndpkgbfmshceffwhpqoz.supabase.co"
SUPABASE_KEY = "sb_publishable_sK0uEH4t-Hw7LVBpnEaMog_z_2MQwxq"


@dataclass
class AuthSession:
    access_token: str
    refresh_token: str
    user_id: str
    email: str
    expires_at: int

    @property
    def expired(self) -> bool:
        return int(time.time()) >= self.expires_at - 60

    def to_json(self) -> dict[str, Any]:
        # Keep the legacy field names so existing sessions survive migration.
        return {
            "accessToken": self.access_token,
            "refreshToken": self.refresh_token,
            "userId": self.user_id,
            "email": self.email,
            "expiresAt": self.expires_at,
        }

    @classmethod
    def from_json(cls, raw: dict[str, Any] | None) -> "AuthSession | None":
        if not isinstance(raw, dict):
            return None
        access = raw.get("accessToken")
        refresh = raw.get("refreshToken")
        if not isinstance(access, str) or not isinstance(refresh, str):
            return None
        raw_user_id = raw.get("userId")
        raw_email = raw.get("email")
        return cls(
            access_token=access,
            refresh_token=refresh,
            user_id=raw_user_id if isinstance(raw_user_id, str) else "",
            email=raw_email if isinstance(raw_email, str) else "",
            expires_at=int(raw.get("expiresAt", 0)),
        )

    @classmethod
    def from_token_response(cls, raw: dict[str, Any]) -> "AuthSession":
        raw_user = raw.get("user")
        user: dict[str, Any] = raw_user if isinstance(raw_user, dict) else {}
        expires_in = int(raw.get("expires_in", 3600))
        return cls(
            access_token=str(raw.get("access_token", "")),
            refresh_token=str(raw.get("refresh_token", "")),
            user_id=str(user.get("id", "")),
            email=str(user.get("email", "")),
            expires_at=int(raw.get("expires_at", int(time.time()) + expires_in)),
        )


@dataclass(frozen=True)
class AppUser:
    id: str
    email: str
    allowed: bool = False
    is_admin: bool = False
    created_at: str = ""
    username: str | None = None

    @property
    def display(self) -> str:
        return self.username if self.username else self.email

    @classmethod
    def from_json(cls, raw: dict[str, Any]) -> "AppUser":
        username = raw.get("username")
        return cls(
            id=str(raw.get("id", "")),
            email=str(raw.get("email", "")),
            allowed=raw.get("allowed") is True,
            is_admin=raw.get("is_admin") is True,
            created_at=str(raw.get("created_at", "")),
            username=username if isinstance(username, str) else None,
        )


class UsernameRule:
    MIN_LENGTH = 3
    MAX_LENGTH = 24
    _PATTERN = re.compile(r"^[A-Za-z0-9._-]+$")

    @classmethod
    def ok(cls, value: str) -> bool:
        cleaned = value.strip()
        return (
            cls.MIN_LENGTH <= len(cleaned) <= cls.MAX_LENGTH
            and cls._PATTERN.fullmatch(cleaned) is not None
        )


class AuthException(Exception):
    pass


class AuthService:
    def __init__(
        self,
        settings: SettingsStore,
        *,
        url: str = SUPABASE_URL,
        key: str = SUPABASE_KEY,
    ) -> None:
        self.settings = settings
        self.url = url.rstrip("/")
        self.key = key
        self.session: AuthSession | None = None
        self.me: AppUser | None = None

    @property
    def signed_in(self) -> bool:
        return self.session is not None

    @property
    def allowed(self) -> bool:
        return bool(self.me and self.me.allowed)

    @property
    def is_admin(self) -> bool:
        return bool(self.me and self.me.is_admin)

    @property
    def anonymous_headers(self) -> dict[str, str]:
        return {"apikey": self.key}

    @property
    def auth_headers(self) -> dict[str, str]:
        headers = self.anonymous_headers
        if self.session is not None:
            headers = {
                **headers,
                "Authorization": f"Bearer {self.session.access_token}",
            }
        return headers

    def restore(self) -> None:
        self.session = AuthSession.from_json(self.settings.section("session"))
        if self.session is None:
            return
        if self.session.expired and not self._refresh():
            self.sign_out()
            return
        try:
            self.load_me()
        except Exception:
            # Offline startup is allowed; the approval gate can retry.
            return

    @staticmethod
    def _error_from(response: Response) -> str:
        try:
            raw = json.loads(response.text)
            if isinstance(raw, dict):
                for key in ("error_description", "msg", "message", "error"):
                    value = raw.get(key)
                    if isinstance(value, str) and value:
                        return value
        except (ValueError, TypeError):
            pass
        return f"HTTP {response.status}"

    def _require(self, response: Response) -> Any:
        if not 200 <= response.status < 300:
            raise AuthException(self._error_from(response))
        return response.json()

    def _rpc(self, function: str, args: dict[str, Any], *, authed: bool = True) -> Any:
        response = send_json(
            "POST",
            f"{self.url}/rest/v1/rpc/{function}",
            headers=self.auth_headers if authed else self.anonymous_headers,
            body=args,
        )
        return self._require(response)

    def username_available(self, username: str) -> bool:
        if not UsernameRule.ok(username):
            return False
        try:
            return self._rpc(
                "username_available",
                {"p_username": username.strip()},
                authed=False,
            ) is True
        except Exception:
            # Do not block signup on a transient availability-check failure.
            return True

    def sign_up(self, email: str, password: str, username: str) -> None:
        response = send_json(
            "POST",
            f"{self.url}/auth/v1/signup",
            headers=self.anonymous_headers,
            body={
                "email": email,
                "password": password,
                "data": {"username": username.strip()},
            },
        )
        self._require(response)

    def sign_in(self, identifier: str, password: str) -> None:
        login = identifier.strip()
        email = login
        if "@" not in login:
            try:
                resolved = self._rpc(
                    "email_for_login",
                    {"p_username": login, "p_password": password},
                    authed=False,
                )
            except AuthException as error:
                raise AuthException("invalid_credentials") from error
            if not isinstance(resolved, str) or not resolved:
                raise AuthException("invalid_credentials")
            email = resolved

        response = send_json(
            "POST",
            f"{self.url}/auth/v1/token?grant_type=password",
            headers=self.anonymous_headers,
            body={"email": email, "password": password},
        )
        raw = self._require(response)
        self.session = AuthSession.from_token_response(raw)
        self.settings.save_section("session", self.session.to_json())
        self.load_me()

    def _refresh(self) -> bool:
        if self.session is None:
            return False
        try:
            response = send_json(
                "POST",
                f"{self.url}/auth/v1/token?grant_type=refresh_token",
                headers=self.anonymous_headers,
                body={"refresh_token": self.session.refresh_token},
            )
            if not 200 <= response.status < 300:
                return False
            self.session = AuthSession.from_token_response(response.json())
            self.settings.save_section("session", self.session.to_json())
            return True
        except Exception:
            return False

    def valid_token(self) -> str | None:
        if self.session is None:
            return None
        if self.session.expired and not self._refresh():
            return None
        return self.session.access_token

    def sign_out(self) -> None:
        try:
            if self.session is not None:
                send_json(
                    "POST",
                    f"{self.url}/auth/v1/logout",
                    headers=self.auth_headers,
                )
        except Exception:
            pass
        self.session = None
        self.me = None
        self.settings.save_section("session", None)

    def load_me(self) -> AppUser | None:
        if self.valid_token() is None or self.session is None:
            return None
        user_id = urllib.parse.quote(self.session.user_id, safe="")
        response = send_json(
            "GET",
            f"{self.url}/rest/v1/app_users?select=*&id=eq.{user_id}",
            headers=self.auth_headers,
        )
        raw = self._require(response)
        self.me = AppUser.from_json(raw[0]) if isinstance(raw, list) and raw else None
        return self.me

    def set_username(self, username: str) -> None:
        if self.valid_token() is None:
            raise AuthException("not_authenticated")
        self._rpc("set_username", {"p_username": username.strip()})
        self.load_me()

    def list_users(self) -> list[AppUser]:
        if self.valid_token() is None:
            raise AuthException("not_authenticated")
        response = send_json(
            "GET",
            f"{self.url}/rest/v1/app_users?select=*&order=created_at.desc",
            headers=self.auth_headers,
        )
        raw = self._require(response)
        return [AppUser.from_json(item) for item in raw]

    def set_flags(
        self,
        user_id: str,
        *,
        allowed: bool | None = None,
        is_admin: bool | None = None,
    ) -> None:
        if self.valid_token() is None:
            raise AuthException("not_authenticated")
        patch: dict[str, bool] = {}
        if allowed is not None:
            patch["allowed"] = allowed
        if is_admin is not None:
            patch["is_admin"] = is_admin
        if not patch:
            return
        safe_id = urllib.parse.quote(user_id, safe="")
        response = send_json(
            "PATCH",
            f"{self.url}/rest/v1/app_users?id=eq.{safe_id}",
            headers={**self.auth_headers, "Prefer": "return=minimal"},
            body=patch,
        )
        self._require(response)
