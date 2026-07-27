from __future__ import annotations

import json

from docflow_sidecar import email_extract


def _mapping() -> dict[str, list[str]]:
    return {
        "AZE-SUN-INT-QMS-ITP-CIV-0005": ["CIV-5.2.1", "CIV-5.3.1"],
        "AZE-SUN-INT-QMS-ITP-CIV-0007": ["CIV-7.1.1"],
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
    assert body["generationConfig"]["responseMimeType"] == "application/json"
    assert body["generationConfig"]["responseSchema"]["type"] == "OBJECT"
    assert body["generationConfig"]["responseSchema"]["properties"][
        "discipline"
    ]["enum"] == ["CIV", "MEC"]
    assert result["provider"] == "gemini"
    assert result["rows"][0]["itemNo"] == "CIV-7.1.1"


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
