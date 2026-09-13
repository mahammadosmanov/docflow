use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ReadinessStatus {
    Ready,
    Attention,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadinessCheck {
    pub id: &'static str,
    pub status: ReadinessStatus,
    pub count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadinessReport {
    pub overall: ReadinessStatus,
    pub checks: Vec<ReadinessCheck>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadinessFacts {
    pub ocr_available: bool,
    pub ocr_language_data_available: bool,
    pub printer_inventory_available: bool,
    pub printer_count: u64,
    pub recovery_count: u64,
}

pub fn build_readiness(facts: ReadinessFacts) -> ReadinessReport {
    let ocr_ready = facts.ocr_available && facts.ocr_language_data_available;
    let printers_ready = facts.printer_inventory_available && facts.printer_count > 0;
    let recovery_ready = facts.recovery_count == 0;
    let checks = vec![
        ReadinessCheck {
            id: "ocr",
            status: status(ocr_ready),
            count: u64::from(ocr_ready),
        },
        ReadinessCheck {
            id: "printers",
            status: status(printers_ready),
            count: facts.printer_count,
        },
        ReadinessCheck {
            id: "recovery",
            status: status(recovery_ready),
            count: facts.recovery_count,
        },
    ];
    ReadinessReport {
        overall: status(
            checks
                .iter()
                .all(|check| check.status == ReadinessStatus::Ready),
        ),
        checks,
    }
}

const fn status(ready: bool) -> ReadinessStatus {
    if ready {
        ReadinessStatus::Ready
    } else {
        ReadinessStatus::Attention
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readiness_is_attention_when_any_bounded_check_needs_action() {
        let report = build_readiness(ReadinessFacts {
            ocr_available: true,
            ocr_language_data_available: true,
            printer_inventory_available: true,
            printer_count: 2,
            recovery_count: 1,
        });
        assert_eq!(report.overall, ReadinessStatus::Attention);
        assert_eq!(report.checks[0].status, ReadinessStatus::Ready);
        assert_eq!(report.checks[1].count, 2);
        assert_eq!(report.checks[2].status, ReadinessStatus::Attention);
    }

    #[test]
    fn missing_language_data_and_failed_printer_inventory_fail_closed() {
        let report = build_readiness(ReadinessFacts {
            ocr_available: true,
            ocr_language_data_available: false,
            printer_inventory_available: false,
            printer_count: 4,
            recovery_count: 0,
        });
        assert_eq!(report.overall, ReadinessStatus::Attention);
        assert_eq!(report.checks[0].status, ReadinessStatus::Attention);
        assert_eq!(report.checks[1].status, ReadinessStatus::Attention);
        assert_eq!(report.checks[2].status, ReadinessStatus::Ready);
    }
}
