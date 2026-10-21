use super::*;

pub fn canonical_rfi(value: &str) -> Option<String> {
    static RFI: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\AAZE-SUN-INT-QMS-RFI-(CIV|MEC|ELE|PRC)-([0-9]{1,4})\z")
            .expect("canonical RFI regex is valid")
    });
    let normalized = normalize_status(value).to_uppercase();
    let captures = RFI.captures(&normalized)?;
    Some(format!(
        "{FULL_RFI_PREFIX}{}-{}",
        &captures[1],
        normalize_candidate_number(&captures[2])?
    ))
}

pub fn canonical_follow_up_rfi(value: &str) -> Option<(String, String)> {
    static SHORT_RFI: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\A(?:RFI-)?(CIV|MEC|ELE|PRC)[-_]?([0-9]{1,4})\z")
            .expect("short Follow Up RFI regex is valid")
    });
    let normalized = normalize_status(value).to_uppercase();
    let short = if let Some(short) = normalized.strip_prefix(FULL_RFI_PREFIX) {
        short.to_string()
    } else {
        normalized
    };
    let captures = SHORT_RFI.captures(&short)?;
    let canonical_short = format!(
        "{}-{}",
        &captures[1],
        normalize_candidate_number(&captures[2])?
    );
    Some((
        format!("{FULL_RFI_PREFIX}{canonical_short}"),
        canonical_short,
    ))
}

pub fn is_placeholder_rfi(code: &str) -> bool {
    let digits = code.rsplit_once('-').map_or(code, |(_, num)| num);
    digits.parse::<u32>().ok().is_some_and(|n| n >= 2000)
}

pub fn mapped_follow_up_status(remark: &str) -> &'static str {
    if remark
        .lines()
        .any(|line| normalize_status(line) == WORLEY_SIGNED_MARKER)
    {
        "Signed"
    } else {
        "Not Signed"
    }
}
