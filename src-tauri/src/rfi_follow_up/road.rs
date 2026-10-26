//! Road tag parsing for Follow Up routing.
//!
//! A Road may be split into parts that are decided while work progresses. A Log
//! tag without a part (`Road 06`) belongs to every part of that Road, including
//! parts added later; a tag with a part (`Road 06 (Part 2)`) belongs only to it.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use regex::Regex;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct RoadRef {
    pub(super) road: u32,
    pub(super) part: Option<u32>,
}

/// Parses every Road reference in a tag or target cell. Accepts `Road 15 (Part 1)`,
/// `Road 15 (Part 1), (Part 2)`, `Road 15 (Part 1),Road 15 (Part 2)`, `Road 15 Part 1 and 2`
/// style variations. Unrecognized text is ignored.
pub(super) fn road_refs(value: &str) -> Vec<RoadRef> {
    static ROAD: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\AROAD\s*0*([0-9]+)\s*(?:\(?\s*PART\s*0*([0-9]+)\s*\)?)?\z").unwrap()
    });
    static PART: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\A\(?\s*(?:PART\s*)?0*([0-9]+)\s*\)?\z").unwrap());
    let text = value
        .to_uppercase()
        .replace(" SITE", "")
        .replace('&', ",")
        .replace(" AND ", ",");
    let mut refs = BTreeSet::new();
    let mut current_road = None;
    for token in text.split([',', ';', '/', '\n']) {
        let token = token.trim();
        if let Some(captures) = ROAD.captures(token) {
            let Ok(road) = captures[1].parse::<u32>() else {
                continue;
            };
            current_road = Some(road);
            refs.insert(RoadRef {
                road,
                part: captures.get(2).and_then(|part| part.as_str().parse().ok()),
            });
        } else if let (Some(road), Some(captures)) = (current_road, PART.captures(token))
            && token.contains(['P', '('])
            && let Ok(part) = captures[1].parse::<u32>()
        {
            refs.insert(RoadRef {
                road,
                part: Some(part),
            });
        }
    }
    refs.into_iter().collect()
}

/// Does a Log tag belong to the Follow Up row whose `ROAD No.` cell is `target`?
/// Falls back to the previous exact comparison when either side is not a Road tag.
pub(super) fn road_tag_matches_row(tag: &str, target: &str) -> bool {
    let tag_refs = road_refs(tag);
    let target_refs = road_refs(target);
    if tag_refs.is_empty() || target_refs.len() != 1 {
        return super::normalized_location(tag) == super::normalized_location(target);
    }
    let target = target_refs[0];
    tag_refs.iter().any(|reference| {
        reference.road == target.road && (reference.part.is_none() || reference.part == target.part)
    })
}

/// `Some(true)` when every Road reference in the tag is represented by a candidate
/// row, `Some(false)` when at least one is not. `None` when the tag names no Road.
pub(super) fn road_tag_coverage(tag: &str, candidate_targets: &[String]) -> Option<bool> {
    let tag_refs = road_refs(tag);
    if tag_refs.is_empty() {
        return None;
    }
    let targets: Vec<RoadRef> = candidate_targets
        .iter()
        .flat_map(|cell| road_refs(cell))
        .collect();
    Some(tag_refs.iter().all(|reference| {
        targets.iter().any(|target| {
            reference.road == target.road
                && (reference.part.is_none() || reference.part == target.part)
        })
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(road: u32, part: Option<u32>) -> RoadRef {
        RoadRef { road, part }
    }

    #[test]
    fn parses_partless_parted_and_zero_padded_tags() {
        assert_eq!(road_refs("Road 06"), [r(6, None)]);
        assert_eq!(road_refs("ROAD 6"), [r(6, None)]);
        assert_eq!(road_refs("Road 16 (Part 1)"), [r(16, Some(1))]);
        assert_eq!(road_refs("road 01 (part 03)"), [r(1, Some(3))]);
        assert_eq!(road_refs("Road 14 Site"), [r(14, None)]);
    }

    #[test]
    fn parses_multi_part_variations() {
        let expected = [r(15, Some(1)), r(15, Some(2))];
        assert_eq!(road_refs("Road 15 (Part 1), (Part 2)"), expected);
        assert_eq!(road_refs("Road 15 (Part 1),Road 15 (Part 2)"), expected);
        assert_eq!(road_refs("Road 15 (Part 1) & (Part 2)"), expected);
        assert_eq!(road_refs("Road 15 (Part 1) and Part 2"), expected);
        assert_eq!(road_refs("Road 15 (Part 1); Road 15 (Part 2)"), expected);
        assert_eq!(
            road_refs("Road 02, Road 06 (Part 4)"),
            [r(2, None), r(6, Some(4))]
        );
    }

    #[test]
    fn ignores_non_road_text_and_bare_numbers() {
        assert!(road_refs("PCS-01").is_empty());
        assert!(road_refs("").is_empty());
        assert_eq!(road_refs("Road 15, 2"), [r(15, None)]);
    }

    #[test]
    fn partless_tag_belongs_to_every_part_and_the_partless_row() {
        for target in [
            "Road 06 (Part 1)",
            "Road 06 (Part 4)",
            "Road 06",
            "ROAD 6 (PART 2)",
        ] {
            assert!(road_tag_matches_row("Road 06", target), "{target}");
        }
        assert!(!road_tag_matches_row("Road 06", "Road 07 (Part 1)"));
    }

    #[test]
    fn parted_tag_belongs_only_to_that_part() {
        assert!(road_tag_matches_row("Road 06 (Part 2)", "Road 06 (Part 2)"));
        assert!(!road_tag_matches_row(
            "Road 06 (Part 2)",
            "Road 06 (Part 1)"
        ));
        assert!(!road_tag_matches_row("Road 06 (Part 2)", "Road 06"));
        assert!(road_tag_matches_row(
            "Road 15 (Part 1), (Part 2)",
            "Road 15 (Part 2)"
        ));
        assert!(!road_tag_matches_row(
            "Road 15 (Part 1), (Part 2)",
            "Road 15 (Part 3)"
        ));
    }

    #[test]
    fn non_road_text_keeps_the_previous_exact_comparison() {
        assert!(road_tag_matches_row("Quarry Access", "QUARRY ACCESS"));
        assert!(!road_tag_matches_row("Quarry Access", "Road 06"));
    }

    #[test]
    fn coverage_reports_unrepresented_references() {
        let rows = vec![
            "Road 06 (Part 1)".to_string(),
            "Road 06 (Part 2)".to_string(),
        ];
        assert_eq!(road_tag_coverage("Road 06", &rows), Some(true));
        assert_eq!(road_tag_coverage("Road 06 (Part 2)", &rows), Some(true));
        assert_eq!(
            road_tag_coverage("Road 06 (Part 2), Road 06 (Part 9)", &rows),
            Some(false)
        );
        assert_eq!(road_tag_coverage("Road 07", &rows), Some(false));
        assert_eq!(road_tag_coverage("PCS-01", &rows), None);
    }
}
