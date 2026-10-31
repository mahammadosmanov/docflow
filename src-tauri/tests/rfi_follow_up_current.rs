use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{File, OpenOptions},
    io::Seek,
    path::PathBuf,
};

#[test]
#[ignore = "requires operator-supplied current workbook paths"]
fn current_workbooks_produce_a_bounded_guarded_follow_up_plan() {
    let log = PathBuf::from(
        std::env::var("DOCFLOW_CURRENT_RFI_LOG").expect("DOCFLOW_CURRENT_RFI_LOG is required"),
    );
    let follow_up = PathBuf::from(
        std::env::var("DOCFLOW_CURRENT_FOLLOW_UP").expect("DOCFLOW_CURRENT_FOLLOW_UP is required"),
    );
    let mut log_file = File::open(&log).expect("open RFI Log read-only");
    let mut follow_up_file = File::open(&follow_up).expect("open Follow Up read-only");
    let plan = docflow_lib::rfi_follow_up::preview_from_open_files(
        &log,
        &follow_up,
        &mut log_file,
        &mut follow_up_file,
    )
    .expect("current workbooks must produce a safe preview");

    assert_eq!(plan.op_id, "rfi.follow_up_sync");
    assert!(plan.steps.len() <= 1);
    eprintln!(
        "current-template preview: {} approved row pair(s), warnings={:?}",
        plan.steps
            .first()
            .and_then(|step| step.meta.get("updates"))
            .and_then(|updates| updates.as_array())
            .map_or(0, Vec::len),
        plan.warnings
    );
    let submitted = plan.meta["editor"]["rows"]
        .as_array()
        .expect("editor rows")
        .clone();
    let mut rows_by_sheet = BTreeMap::<String, usize>::new();
    let mut rows_by_reason = BTreeMap::<String, usize>::new();
    for row in &submitted {
        *rows_by_sheet
            .entry(row["_sheet"].as_str().unwrap_or("unknown").to_string())
            .or_default() += 1;
        *rows_by_reason
            .entry(row["reasonKey"].as_str().unwrap_or("unknown").to_string())
            .or_default() += 1;
    }
    eprintln!("rows by sheet={rows_by_sheet:?}; rows by reason={rows_by_reason:?}");
    let mut revise_log = File::open(&log).expect("reopen RFI Log read-only");
    let mut revise_follow_up = File::open(&follow_up).expect("reopen Follow Up read-only");
    let revised = docflow_lib::rfi_follow_up::revise_from_open_files(
        &plan,
        &submitted,
        &mut revise_log,
        &mut revise_follow_up,
    )
    .expect("unchanged workbooks must preserve the approved full-schema revision");
    assert_eq!(revised.plan_id, plan.plan_id);

    if let Some(step) = revised.steps.first() {
        let updates = docflow_lib::rfi_follow_up::guarded_updates(step)
            .expect("approved step metadata must decode");
        assert!(!updates.is_empty());
        assert!(updates.keys().all(|sheet| matches!(
            sheet.as_str(),
            "CIV" | "MEC" | "ELE" | "PCU" | "Road" | "Signed By Worley CIV" | "Signed By Worley MEC"
        )));
        assert!(!updates.contains_key("POT test"));
        assert!(!updates.contains_key("STAT"));
        assert!(!updates.contains_key("Changelog"));

        if std::env::var("DOCFLOW_CURRENT_PCS_RFI_LIST").is_err() {
            let mut original = File::open(&follow_up).expect("reopen Follow Up read-only");
            let mut output = tempfile::tempfile().expect("create private temporary output");
            docflow_lib::ooxml::patch_workbook_sheets_guarded_text(
                &mut original,
                &mut output,
                &updates,
            )
            .expect("patch only approved literal cells");
            original.rewind().expect("rewind original");
            output.rewind().expect("rewind output");
            docflow_lib::ooxml::verify_guarded_workbook_delta_text(
                &mut original,
                &mut output,
                &updates,
            )
            .expect("no unrelated OOXML package part may change");
        }

        if let Ok(ids_path) = std::env::var("DOCFLOW_CURRENT_PCS_RFI_LIST") {
            let pcs_ids: BTreeSet<String> = serde_json::from_slice(
                &std::fs::read(ids_path).expect("read excluded CIV PCS RFI ids"),
            )
            .expect("excluded CIV PCS RFI list must be JSON strings");
            let mut selected_cells = BTreeSet::new();
            for row in &submitted {
                let target = &row["target"];
                let Some(current_rfi) = target["rfiCell"]["value"].as_str() else {
                    continue;
                };
                if !pcs_group_contains(current_rfi, &pcs_ids) {
                    continue;
                }
                eprintln!(
                    "PCS cleanup match: {} row {} RFI={}",
                    row["_sheet"].as_str().unwrap_or("?"),
                    row["_row"],
                    current_rfi
                );
                let (Some(sheet), Some(row_number), Some(rfi_column), Some(status_column)) = (
                    row["_sheet"].as_str(),
                    row["_row"].as_u64(),
                    target["rfiColumn"].as_u64(),
                    target["statusColumn"].as_u64(),
                ) else {
                    continue;
                };
                selected_cells.insert((sheet.to_string(), row_number as u32, rfi_column as u32));
                selected_cells.insert((sheet.to_string(), row_number as u32, status_column as u32));
            }
            let cleanup_updates = updates
                .iter()
                .filter_map(|(sheet, cells)| {
                    let selected = cells
                        .iter()
                        .filter(|((row, column), _)| {
                            selected_cells.contains(&(sheet.clone(), *row, *column))
                        })
                        .map(|(coordinate, update)| (*coordinate, update.clone()))
                        .collect::<BTreeMap<_, _>>();
                    (!selected.is_empty()).then(|| (sheet.clone(), selected))
                })
                .collect::<BTreeMap<_, _>>();
            let selected_count = selected_cells.len() / 2;
            eprintln!(
                "PCS cleanup rows={selected_count}; cells={}",
                cleanup_updates.values().map(BTreeMap::len).sum::<usize>()
            );
            if selected_count > 0 {
                let output_path = PathBuf::from(
                    std::env::var("DOCFLOW_CURRENT_PCS_CLEAN_OUTPUT")
                        .expect("PCS cleanup output path is required"),
                );
                let mut original = File::open(&follow_up).expect("open cleanup source read-only");
                let mut output = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create_new(true)
                    .open(&output_path)
                    .expect("create a new cleanup copy without clobbering");
                docflow_lib::ooxml::patch_workbook_sheets_guarded_text(
                    &mut original,
                    &mut output,
                    &cleanup_updates,
                )
                .expect("patch only approved PCS cleanup cells");
                original.rewind().expect("rewind cleanup source");
                output.rewind().expect("rewind cleanup copy");
                docflow_lib::ooxml::verify_guarded_workbook_delta_text(
                    &mut original,
                    &mut output,
                    &cleanup_updates,
                )
                .expect("cleanup copy contains no unrelated OOXML changes");
            }
        }
    }
}

fn pcs_group_contains(value: &str, pcs_ids: &BTreeSet<String>) -> bool {
    let mut discipline = None;
    for part in value.to_uppercase().replace(',', "/").split('/') {
        let part = part.trim();
        let short =
            if let Some((_, short)) = docflow_lib::rfi_follow_up::canonical_follow_up_rfi(part) {
                short
            } else if part.len() == 4 && part.bytes().all(|byte| byte.is_ascii_digit()) {
                format!("{}-{part}", discipline.as_deref().unwrap_or(""))
            } else {
                return false;
            };
        if let Some((found_discipline, _)) = short.split_once('-') {
            discipline = Some(found_discipline.to_string());
        }
        if pcs_ids.contains(&short) {
            return true;
        }
    }
    false
}
