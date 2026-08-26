use std::collections::BTreeSet;

use crate::errors::BackendError;

const MAX_SELECTED_PAGES: usize = 1_000_000;
const INVALID_RANGE_KEY: &str = "error.print.pageRangeInvalid";
const RANGE_BOUNDS_KEY: &str = "error.print.pageRangeBounds";
const IMAGE_RANGE_KEY: &str = "error.print.pageRangeImageUnsupported";
const NATIVE_RANGE_KEY: &str = "error.print.pageRangeNativeUnsupported";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PageRange {
    All,
    Pages(BTreeSet<u32>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PageRangeCapability {
    Pdf { page_count: Option<u32> },
    SinglePage,
    AllOnly,
}

impl PageRange {
    pub fn parse(raw: &str) -> Result<Self, BackendError> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err(invalid_range());
        }
        if raw.eq_ignore_ascii_case("all") {
            return Ok(Self::All);
        }

        let mut pages = BTreeSet::new();
        for token in raw.split(',') {
            let token = token.trim();
            if token.is_empty() || token.eq_ignore_ascii_case("all") {
                return Err(invalid_range());
            }
            if let Some((start, end)) = token.split_once('-') {
                if end.contains('-') {
                    return Err(invalid_range());
                }
                let start = positive_page(start)?;
                let end = positive_page(end)?;
                if start > end
                    || usize::try_from(end - start)
                        .ok()
                        .is_none_or(|span| span >= MAX_SELECTED_PAGES)
                    || pages.len() + usize::try_from(end - start + 1).unwrap_or(usize::MAX)
                        > MAX_SELECTED_PAGES
                {
                    return Err(invalid_range());
                }
                pages.extend(start..=end);
            } else {
                pages.insert(positive_page(token)?);
                if pages.len() > MAX_SELECTED_PAGES {
                    return Err(invalid_range());
                }
            }
        }
        if pages.is_empty() {
            return Err(invalid_range());
        }
        Ok(Self::Pages(pages))
    }

    pub fn canonical(&self) -> String {
        match self {
            Self::All => "all".to_string(),
            Self::Pages(pages) => {
                let mut ranges = Vec::new();
                let mut iter = pages.iter().copied();
                let Some(mut start) = iter.next() else {
                    return String::new();
                };
                let mut end = start;
                for page in iter {
                    if end.checked_add(1) == Some(page) {
                        end = page;
                    } else {
                        ranges.push(format_range(start, end));
                        start = page;
                        end = page;
                    }
                }
                ranges.push(format_range(start, end));
                ranges.join(",")
            }
        }
    }

    pub fn resolve_zero_based(&self, page_count: u32) -> Result<Vec<u32>, BackendError> {
        match self {
            Self::All => {
                if usize::try_from(page_count)
                    .ok()
                    .is_none_or(|page_count| page_count > MAX_SELECTED_PAGES)
                {
                    return Err(BackendError::validation(RANGE_BOUNDS_KEY));
                }
                Ok((0..page_count).collect())
            }
            Self::Pages(pages) => {
                if pages.last().is_some_and(|page| *page > page_count) {
                    return Err(BackendError::validation(RANGE_BOUNDS_KEY));
                }
                Ok(pages.iter().map(|page| page - 1).collect())
            }
        }
    }
}

pub fn canonical_for(raw: &str, capability: &PageRangeCapability) -> Result<String, BackendError> {
    let range = PageRange::parse(raw)?;
    match capability {
        PageRangeCapability::Pdf { page_count } => {
            if let Some(page_count) = page_count {
                range.resolve_zero_based(*page_count)?;
            }
            Ok(range.canonical())
        }
        PageRangeCapability::SinglePage => match range {
            PageRange::All => Ok("all".to_string()),
            PageRange::Pages(pages) if pages.len() == 1 && pages.contains(&1) => {
                Ok("all".to_string())
            }
            PageRange::Pages(_) => Err(BackendError::validation(IMAGE_RANGE_KEY)),
        },
        PageRangeCapability::AllOnly => match range {
            PageRange::All => Ok("all".to_string()),
            PageRange::Pages(_) => Err(BackendError::validation(NATIVE_RANGE_KEY)),
        },
    }
}

#[cfg(any(target_os = "windows", test))]
pub(crate) fn selected_pdf_pages(raw: &str, page_count: u32) -> Result<Vec<u32>, BackendError> {
    PageRange::parse(raw)?.resolve_zero_based(page_count)
}

fn positive_page(raw: &str) -> Result<u32, BackendError> {
    let raw = raw.trim();
    if raw.is_empty() || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid_range());
    }
    let page = raw.parse::<u32>().map_err(|_| invalid_range())?;
    if page == 0 {
        return Err(invalid_range());
    }
    Ok(page)
}

fn format_range(start: u32, end: u32) -> String {
    if start == end {
        start.to_string()
    } else {
        format!("{start}-{end}")
    }
}

fn invalid_range() -> BackendError {
    BackendError::validation(INVALID_RANGE_KEY)
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_SELECTED_PAGES, PageRange, PageRangeCapability, canonical_for, selected_pdf_pages,
    };

    #[test]
    fn parses_all_case_insensitively() {
        assert_eq!(PageRange::parse("all").unwrap().canonical(), "all");
        assert_eq!(PageRange::parse(" ALL ").unwrap().canonical(), "all");
    }

    #[test]
    fn sorts_deduplicates_and_coalesces_numeric_ranges() {
        assert_eq!(
            PageRange::parse(" 3, 1-2,2,7 ").unwrap().canonical(),
            "1-3,7"
        );
    }

    #[test]
    fn resolves_numeric_ranges_to_zero_based_pages() {
        assert_eq!(
            PageRange::parse("1-3,5")
                .unwrap()
                .resolve_zero_based(5)
                .unwrap(),
            vec![0, 1, 2, 4]
        );
    }

    #[test]
    fn selected_pdf_pages_resolve_canonical_ranges_to_exact_indexes() {
        assert_eq!(selected_pdf_pages("2-3", 4).unwrap(), vec![1, 2]);
        assert_eq!(selected_pdf_pages("3,1-2,2", 4).unwrap(), vec![0, 1, 2]);
        assert_eq!(selected_pdf_pages("all", 4).unwrap(), vec![0, 1, 2, 3]);
    }

    #[test]
    fn all_page_ranges_reject_page_counts_above_the_selection_limit() {
        let limit = u32::try_from(MAX_SELECTED_PAGES).unwrap();
        let over_limit = limit + 1;

        assert_eq!(
            PageRange::All.resolve_zero_based(limit).unwrap().len(),
            MAX_SELECTED_PAGES
        );
        assert_eq!(
            PageRange::All
                .resolve_zero_based(over_limit)
                .unwrap_err()
                .message,
            "error.print.pageRangeBounds"
        );
        assert_eq!(
            selected_pdf_pages("all", over_limit).unwrap_err().message,
            "error.print.pageRangeBounds"
        );
        assert_eq!(
            canonical_for(
                "all",
                &PageRangeCapability::Pdf {
                    page_count: Some(over_limit),
                },
            )
            .unwrap_err()
            .message,
            "error.print.pageRangeBounds"
        );
        assert_eq!(
            selected_pdf_pages("1,1000001", over_limit).unwrap(),
            vec![0, 1_000_000]
        );
    }

    #[test]
    fn rejects_empty_input() {
        assert!(PageRange::parse("").is_err());
    }

    #[test]
    fn rejects_zero() {
        assert!(PageRange::parse("0").is_err());
    }

    #[test]
    fn rejects_negative_values() {
        assert!(PageRange::parse("-1").is_err());
    }

    #[test]
    fn rejects_empty_comma_segments() {
        assert!(PageRange::parse("1,,2").is_err());
    }

    #[test]
    fn rejects_more_than_one_hyphen() {
        assert!(PageRange::parse("1-2-3").is_err());
    }

    #[test]
    fn rejects_descending_ranges() {
        assert!(PageRange::parse("5-3").is_err());
    }

    #[test]
    fn rejects_all_mixed_with_numeric_tokens() {
        assert!(PageRange::parse("all,2").is_err());
    }

    #[test]
    fn rejects_non_numeric_tokens() {
        assert!(PageRange::parse("one").is_err());
    }

    #[test]
    fn rejects_numeric_overflow() {
        assert!(PageRange::parse("4294967296").is_err());
    }

    #[test]
    fn rejects_pages_above_the_supplied_page_count() {
        assert!(
            PageRange::parse("1-3")
                .unwrap()
                .resolve_zero_based(2)
                .is_err()
        );
    }

    #[test]
    fn pdf_capability_canonicalizes_and_enforces_a_known_bound() {
        let capability = PageRangeCapability::Pdf {
            page_count: Some(3),
        };
        assert_eq!(canonical_for("3,1-2", &capability).unwrap(), "1-3");
        assert_eq!(
            canonical_for("4", &capability).unwrap_err().message,
            "error.print.pageRangeBounds"
        );
    }

    #[test]
    fn single_page_capability_normalizes_page_one_to_all() {
        assert_eq!(
            canonical_for("all", &PageRangeCapability::SinglePage).unwrap(),
            "all"
        );
        assert_eq!(
            canonical_for("1", &PageRangeCapability::SinglePage).unwrap(),
            "all"
        );
        assert_eq!(
            canonical_for("2", &PageRangeCapability::SinglePage)
                .unwrap_err()
                .message,
            "error.print.pageRangeImageUnsupported"
        );
    }

    #[test]
    fn all_only_capability_rejects_numeric_ranges() {
        assert_eq!(
            canonical_for("all", &PageRangeCapability::AllOnly).unwrap(),
            "all"
        );
        assert_eq!(
            canonical_for("1", &PageRangeCapability::AllOnly)
                .unwrap_err()
                .message,
            "error.print.pageRangeNativeUnsupported"
        );
    }
}
