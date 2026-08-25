#[cfg(target_os = "windows")]
pub mod print_queue {
    use std::path::Path;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum PrintFailureKind {
        Failed,
        Unsupported,
        UnsupportedOptions,
        Uncertain,
        Cancelled,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct PrintFailure {
        pub kind: PrintFailureKind,
        pub message: String,
        pub uncertain: bool,
    }

    impl PrintFailure {
        fn new(kind: PrintFailureKind, message: impl Into<String>, uncertain: bool) -> Self {
            Self {
                kind,
                message: message.into(),
                uncertain,
            }
        }

        pub fn failed(message: impl Into<String>) -> Self {
            Self::new(PrintFailureKind::Failed, message, false)
        }

        pub fn unsupported(message: impl Into<String>) -> Self {
            Self::new(PrintFailureKind::Unsupported, message, false)
        }

        pub fn unsupported_options(message: impl Into<String>) -> Self {
            Self::new(PrintFailureKind::UnsupportedOptions, message, false)
        }

        pub fn uncertain(message: impl Into<String>) -> Self {
            Self::new(PrintFailureKind::Uncertain, message, true)
        }

        pub fn cancelled(message: impl Into<String>, uncertain: bool) -> Self {
            Self::new(PrintFailureKind::Cancelled, message, uncertain)
        }
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct PrintSettings {
        pub paper: String,
        pub orientation: String,
        pub scaling: String,
        pub color: String,
        pub duplex: String,
        pub timeout_minutes: u32,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct PrintJobSupport {
        pub verb: String,
        pub settings_limited: bool,
    }

    impl PrintJobSupport {
        pub fn controlled() -> Self {
            Self {
                verb: "docflow".to_string(),
                settings_limited: false,
            }
        }

        pub fn native_application() -> Self {
            Self {
                verb: "printto".to_string(),
                settings_limited: true,
            }
        }
    }

    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    pub struct PrinterInventory {
        pub printers: Vec<String>,
        pub default_printer: Option<String>,
    }

    impl PrinterInventory {
        pub fn normalized(mut self) -> Self {
            self.printers.sort();
            self.printers.dedup();
            self
        }

        pub fn contains(&self, printer: &str) -> bool {
            self.printers.iter().any(|candidate| candidate == printer)
        }
    }

    #[derive(Clone, Debug)]
    pub struct PrintJob {
        pub path: String,
        pub printer: String,
        pub verb: String,
        pub paper: String,
        pub orientation: String,
        pub scaling: String,
        pub color: String,
        pub duplex: String,
        pub timeout_minutes: u32,
    }

    impl PrintJob {
        pub fn settings(&self) -> PrintSettings {
            PrintSettings {
                paper: self.paper.clone(),
                orientation: self.orientation.clone(),
                scaling: self.scaling.clone(),
                color: self.color.clone(),
                duplex: self.duplex.clone(),
                timeout_minutes: self.timeout_minutes,
            }
        }
    }

    pub trait PrintAdapter: Send + Sync {
        fn inventory(&self) -> Result<PrinterInventory, PrintFailure>;
        fn inspect(
            &self,
            path: &Path,
            printer: &str,
            settings: &PrintSettings,
        ) -> Result<PrintJobSupport, PrintFailure>;
        fn submit(
            &self,
            job: &PrintJob,
            cancelled: &dyn Fn() -> bool,
        ) -> Result<(), PrintFailure>;
    }
}

#[cfg(target_os = "windows")]
pub mod windows_pdf {
    use std::path::Path;

    pub fn page_count(_input: &Path) -> Result<u32, String> {
        Ok(1)
    }

    pub fn render_page_png(
        _input: &Path,
        _output: &Path,
        _dpi: i32,
        _page_index: u32,
    ) -> Result<(u32, u32), String> {
        Ok((1, 1))
    }
}

#[cfg(target_os = "windows")]
#[path = "../../../src/platform/mod.rs"]
pub mod platform;

#[cfg(target_os = "windows")]
pub use platform::windows::printing;
