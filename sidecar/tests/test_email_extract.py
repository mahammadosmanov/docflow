from __future__ import annotations

import io
import json
import urllib.error

from docflow_sidecar import email_extract


def _mapping() -> dict[str, list[str]]:
    return {
        "AZE-SUN-INT-QMS-ITP-CIV-0005": ["CIV-5.2.1", "CIV-5.3.1"],
        "AZE-SUN-INT-QMS-ITP-CIV-0007": ["CIV-7.1.1"],
        "AZE-SUN-INT-QMS-ITP-MEC-0003": ["MEC-3.1", "MEC-3.2"],
    }


def test_offline_email_extracts_count_mapping_and_multiple_tags():
    result = email_extract.offline_extract(
        """
        Please create 2 CIV RFI numbers.
        ITP No: 5
        Item: CIV-5.2.1
        Tag No: PCU-14, PCU-15
        Inspection Location: Foundation A
        Subcontractor: AZFEN
        """,
        discipline="MEC",
        itp_to_items=_mapping(),
        known_tags=[],
        known_subcontractors=["AZFEN"],
    )

    assert result["discipline"] == "CIV"
    assert result["count"] == 2
    assert [row["rfiIndex"] for row in result["rows"]] == [1, 2]
    assert [row["tagNo"] for row in result["rows"]] == ["PCU-14", "PCU-15"]
    assert all(row["itemNo"] == "CIV-5.2.1" for row in result["rows"])
    assert all(row["location"] == "Foundation A" for row in result["rows"])


def test_offline_keeps_multiple_rows_on_one_rfi():
    result = email_extract.offline_extract(
        "Create 1 CIV RFI number\nTag No: N12, S14",
        discipline="CIV",
        itp_to_items=_mapping(),
        known_tags=[],
        known_subcontractors=[],
    )

    assert result["count"] == 1
    assert [row["rfiIndex"] for row in result["rows"]] == [1, 1]
    assert [row["tagNo"] for row in result["rows"]] == ["N12", "S14"]


def test_gemini_uses_header_key_and_structured_schema(monkeypatch):
    captured = {}
    structured = {
        "discipline": "CIV",
        "count": 1,
        "rows": [
            {
                "rfiIndex": 1,
                "itpNo": "AZE-SUN-INT-QMS-ITP-CIV-0007",
                "itemNo": "CIV-7.1.1",
                "tagNo": "N12",
                "area": "",
                "location": "Foundation",
                "subcontractor": "AZFEN",
            }
        ],
        "confidence": 0.94,
        "notes": [],
    }
    response_body = {
        "candidates": [
            {"content": {"parts": [{"text": json.dumps(structured)}]}}
        ]
    }

    class _Response:
        def __enter__(self):
            return self

        def __exit__(self, *_args):
            return False

        def read(self):
            return json.dumps(response_body).encode()

    def fake_urlopen(request, timeout):
        captured["request"] = request
        captured["timeout"] = timeout
        return _Response()

    monkeypatch.setattr(email_extract.urllib.request, "urlopen", fake_urlopen)
    result = email_extract.gemini_extract(
        "mail body",
        api_key="secret-key",
        discipline="CIV",
        itp_to_items=_mapping(),
    )

    request = captured["request"]
    body = json.loads(request.data)
    assert "secret-key" not in request.full_url
    assert request.get_header("X-goog-api-key") == "secret-key"
    response_format = body["generationConfig"]["responseFormat"]["text"]
    assert response_format["mimeType"] == "APPLICATION_JSON"
    assert "temperature" not in body["generationConfig"]
    assert response_format["schema"]["type"] == "object"
    assert response_format["schema"]["properties"]["rows"]["type"] == "array"
    assert response_format["schema"]["properties"]["rows"]["items"][
        "type"
    ] == "object"
    assert response_format["schema"]["properties"][
        "discipline"
    ]["enum"] == ["CIV", "MEC"]
    assert result["provider"] == "gemini"
    assert result["model"] == "gemini-3.1-flash-lite"
    assert result["rows"][0]["itemNo"] == "CIV-7.1.1"


def test_gemini_retries_a_current_model_when_configured_model_is_gone(
    monkeypatch,
):
    requested_models = []
    structured = {
        "discipline": "CIV",
        "count": 1,
        "rows": [
            {
                "rfiIndex": 1,
                "itpNo": "",
                "itemNo": "",
                "tagNo": "",
                "area": "",
                "location": "",
                "subcontractor": "",
            }
        ],
        "confidence": 0.5,
        "notes": [],
    }
    response_body = {
        "candidates": [
            {"content": {"parts": [{"text": json.dumps(structured)}]}}
        ]
    }

    class _Response:
        def __enter__(self):
            return self

        def __exit__(self, *_args):
            return False

        def read(self):
            return json.dumps(response_body).encode()

    def fake_urlopen(request, timeout):
        del timeout
        model = request.full_url.split("/models/", 1)[1].split(":", 1)[0]
        requested_models.append(model)
        if model == "retired-model":
            raise urllib.error.HTTPError(
                request.full_url,
                404,
                "not found",
                {},
                io.BytesIO(b'{"error":{"message":"retired"}}'),
            )
        return _Response()

    monkeypatch.setenv("DOCFLOW_GEMINI_MODEL", "retired-model")
    monkeypatch.setattr(email_extract.urllib.request, "urlopen", fake_urlopen)

    result = email_extract.gemini_extract(
        "mail body",
        api_key="secret-key",
        discipline="CIV",
        itp_to_items=_mapping(),
    )

    assert requested_models == ["retired-model", "gemini-3.1-flash-lite"]
    assert result["model"] == "gemini-3.1-flash-lite"


def test_gemini_retries_legacy_schema_after_current_format_is_rejected(
    monkeypatch,
):
    formats = []
    structured = {
        "discipline": "MEC",
        "count": 1,
        "rows": [
            {
                "rfiIndex": 1,
                "itpNo": "AZE-SUN-INT-QMS-ITP-MEC-0003",
                "itemNo": "MEC-3.1",
                "tagNo": "N3_PCS-02_S7/S8",
                "area": "",
                "location": "",
                "subcontractor": "",
            }
        ],
        "confidence": 0.9,
        "notes": [],
    }
    response_body = {
        "candidates": [
            {"content": {"parts": [{"text": json.dumps(structured)}]}}
        ]
    }

    class _Response:
        def __enter__(self):
            return self

        def __exit__(self, *_args):
            return False

        def read(self):
            return json.dumps(response_body).encode()

    def fake_urlopen(request, timeout):
        del timeout
        body = json.loads(request.data)
        config = body["generationConfig"]
        if "responseFormat" in config:
            format_name = "current"
        elif "responseJsonSchema" in config:
            format_name = "json-schema"
        else:
            format_name = "legacy"
        formats.append(format_name)
        if "responseFormat" in config:
            assert config["responseFormat"]["text"]["schema"]["type"] == "object"
            assert (
                config["responseFormat"]["text"]["mimeType"]
                == "APPLICATION_JSON"
            )
            raise urllib.error.HTTPError(
                request.full_url,
                400,
                "invalid",
                {},
                io.BytesIO(
                    b'{"error":{"message":"Request contains an invalid argument."}}'
                ),
            )
        if "responseJsonSchema" in config:
            assert config["responseMimeType"] == "application/json"
            assert config["responseJsonSchema"]["type"] == "object"
            raise urllib.error.HTTPError(
                request.full_url,
                400,
                "invalid",
                {},
                io.BytesIO(
                    b'{"error":{"message":"responseJsonSchema unavailable"}}'
                ),
            )
        assert config["responseSchema"]["type"] == "OBJECT"
        return _Response()

    monkeypatch.setattr(email_extract.urllib.request, "urlopen", fake_urlopen)
    result = email_extract.gemini_extract(
        "mail body",
        api_key="secret-key",
        discipline="MEC",
        itp_to_items=_mapping(),
    )

    assert formats == ["current", "json-schema", "legacy"]
    assert result["rows"][0]["tagNo"] == "N3_PCS-02_S7/S8"


def test_gemini_failure_falls_back_without_losing_offline_result(monkeypatch):
    def fail(*_args, **_kwargs):
        raise RuntimeError("quota")

    monkeypatch.setattr(email_extract, "gemini_extract", fail)
    result, error = email_extract.extract_email(
        "Create 1 CIV RFI\nTag No: N12",
        provider="gemini",
        api_key="x",
        discipline="CIV",
        itp_to_items=_mapping(),
        known_tags=[],
        known_subcontractors=[],
    )

    assert result["provider"] == "offline"
    assert result["rows"][0]["tagNo"] == "N12"
    assert "quota" in str(error)


def test_azerbaijani_markdown_mail_extracts_compact_tags_and_short_item():
    result = email_extract.offline_extract(
        """
        Salam Məhəmməd bəy
        Transformator təməllərinin hazırlığı üçün **PCU-14/15/16**
        ərazilərinə (**AZE-SUN-INT-QMS-ITP-CIV-0005**)
        **3** ədəd **2.1** (**gro beton**) bəndi üzrə **RFI**
        nömrəsi verməyinizi xahiş edirəm.
        """,
        discipline="MEC",
        itp_to_items=_mapping(),
        known_tags=["PCU-14", "PCU-15", "PCU-16"],
        known_subcontractors=["AZFEN"],
    )

    assert result["discipline"] == "CIV"
    assert result["count"] == 3
    assert [row["rfiIndex"] for row in result["rows"]] == [1, 2, 3]
    assert [row["tagNo"] for row in result["rows"]] == [
        "PCU-14",
        "PCU-15",
        "PCU-16",
    ]
    assert all(
        row["itpNo"] == "AZE-SUN-INT-QMS-ITP-CIV-0005"
        for row in result["rows"]
    )
    assert all(row["itemNo"] == "CIV-5.2.1" for row in result["rows"])


def test_azerbaijani_mec_mail_preserves_escaped_underscore_tag():
    result = email_extract.offline_extract(
        r"""
        Salam Məhəmməd bəy
        Aşağıda qeyd olunan PCS(Tracker) sahəsi üçün
        (**AZE-SUN-INT-QMS-ITP-MEC-0003** əsasən)
        **3.1** bəndi üzrə **1** ədəd **RFI** nömrəsi verməyinizi xaiş edirəm.
        **N3\_PCS-02\_S7/S8**
        """,
        discipline="CIV",
        itp_to_items=_mapping(),
        known_tags=[],
        known_subcontractors=["AZFEN"],
    )

    assert result["discipline"] == "MEC"
    assert result["count"] == 1
    assert result["rows"] == [
        {
            "rfiIndex": 1,
            "itpNo": "AZE-SUN-INT-QMS-ITP-MEC-0003",
            "itemNo": "MEC-3.1",
            "tagNo": "N3_PCS-02_S7/S8",
            "area": "",
            "location": "",
            "subcontractor": "",
        }
    ]
