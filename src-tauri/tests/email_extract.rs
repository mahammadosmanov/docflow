use std::collections::BTreeMap;

use docflow_lib::email_extract::{
    EmailContext, MAX_EMAIL_CHARS, offline_extract, validate_gemini_extraction,
};
use serde_json::json;

fn context() -> EmailContext {
    EmailContext {
        discipline: "MEC".to_string(),
        itp_to_items: BTreeMap::from([
            (
                "AZE-SUN-INT-QMS-ITP-CIV-0005".to_string(),
                vec!["CIV-5.2.1".to_string(), "CIV-5.3.1".to_string()],
            ),
            (
                "AZE-SUN-INT-QMS-ITP-CIV-0007".to_string(),
                vec!["CIV-7.1.1".to_string()],
            ),
            (
                "AZE-SUN-INT-QMS-ITP-MEC-0003".to_string(),
                vec!["MEC-3.1".to_string(), "MEC-3.2".to_string()],
            ),
        ]),
        known_tags: vec![
            "PCU-14".to_string(),
            "PCU-15".to_string(),
            "PCU-16".to_string(),
        ],
        known_subcontractors: vec!["AZFEN".to_string()],
    }
}

#[test]
fn offline_email_contract_matches_count_mapping_and_duplicate_rows() {
    let extraction = offline_extract(
        "Please create 1 CIV RFI number.\n\
         ITP No: 5\nItem: CIV-5.2.1\nTag No: N12, S14\n\
         Inspection Location: Foundation A\nSubcontractor: AZFEN",
        &context(),
    );

    assert_eq!(extraction.provider, "offline");
    assert_eq!(extraction.discipline, "CIV");
    assert_eq!(extraction.count, 1);
    assert_eq!(
        extraction
            .rows
            .iter()
            .map(|row| row.rfi_index)
            .collect::<Vec<_>>(),
        vec![1, 1]
    );
    assert_eq!(
        extraction
            .rows
            .iter()
            .map(|row| row.tag_no.as_str())
            .collect::<Vec<_>>(),
        vec!["N12", "S14"]
    );
    assert!(
        extraction
            .rows
            .iter()
            .all(|row| row.itp_no == "AZE-SUN-INT-QMS-ITP-CIV-0005"
                && row.item_no == "CIV-5.2.1"
                && row.location == "Foundation A")
    );
}

#[test]
fn offline_email_repairs_real_azerbaijani_markdown_and_separator_glyphs() {
    let mut context = context();
    context.itp_to_items = BTreeMap::from([
        (
            "AZE˗SUN˗INT˗QMS˗ITP˗CIV˗0005".to_string(),
            vec!["CIV-5.2.1".to_string()],
        ),
        (
            "AZE˗SUN˗INT˗QMS˗ITP˗MEC˗0003".to_string(),
            vec!["MEC-3.3.1".to_string()],
        ),
    ]);

    let civil = offline_extract(
        "Transformator təməllərinin hazırlığı üçün **PCU-14/15/16** ərazilərinə\n\
         (**AZE-SUN-INT-QMS-ITP-CIV-0005**) **3** ədəd **2.1** bəndi üzrə **RFI**",
        &context,
    );
    let mechanical = offline_extract(
        r"PCS sahəsi üçün (AZE-SUN-INT-QMS-ITP-MEC-0003 əsasən) 3.1 bəndi üzrə
          1 ədəd RFI nömrəsi. N3\_PCS-02\_S7/S8",
        &context,
    );

    assert_eq!(civil.discipline, "CIV");
    assert_eq!(civil.count, 3);
    assert_eq!(
        civil
            .rows
            .iter()
            .map(|row| row.tag_no.as_str())
            .collect::<Vec<_>>(),
        vec!["PCU-14", "PCU-15", "PCU-16"]
    );
    assert!(civil.rows.iter().all(|row| row.item_no == "CIV-5.2.1"));
    assert_eq!(mechanical.discipline, "MEC");
    assert_eq!(mechanical.rows[0].item_no, "MEC-3.3.1");
    assert_eq!(mechanical.rows[0].tag_no, "N3_PCS-02_S7/S8");
}

#[test]
fn extraction_is_bounded_and_malformed_gemini_output_is_rejected() {
    let text = format!(
        "Create 100 CIV RFI numbers\nTag No: {}\n{}",
        "N1,".repeat(500),
        "x".repeat(MAX_EMAIL_CHARS + 50)
    );
    let extraction = offline_extract(&text, &context());
    assert!(extraction.rows.len() <= 100);
    assert!(extraction.rows.iter().all(|row| row.rfi_index <= 100));

    let malformed = validate_gemini_extraction(json!({
        "discipline": "CIV",
        "count": 101,
        "rows": [],
        "confidence": 2.0,
        "notes": [],
        "provider": "gemini",
        "model": "fixture-model"
    }))
    .expect_err("untrusted Gemini output must be range-checked");
    assert_eq!(malformed.code, "validation");
    assert!(!malformed.message.contains("secret"));
}
