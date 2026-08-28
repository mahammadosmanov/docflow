use crate::print_queue::PrintFailure;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SubmitSnapshot {
    pub(crate) cancelled: bool,
    pub(crate) deadline_expired: bool,
}

pub(crate) trait SubmitSignals {
    fn sample(&self) -> SubmitSnapshot;
}

pub(crate) trait ControlledSubmitEffects {
    type JobId: Copy;

    fn start_doc(&mut self) -> Result<Self::JobId, PrintFailure>;
    fn abort_doc(&mut self) -> bool;
    fn render_pages(&mut self) -> Result<(), PrintFailure>;
    fn end_doc(&mut self) -> Result<(), PrintFailure>;
    fn observe_job(&mut self, job_id: Self::JobId) -> Result<(), PrintFailure>;
}

fn post_effect_failure(failure: PrintFailure) -> PrintFailure {
    if failure.kind == crate::print_queue::PrintFailureKind::Cancelled {
        PrintFailure::cancelled("error.print.excelPrintCancelledUnconfirmed", true)
    } else {
        PrintFailure::uncertain("error.print.excelPrintCompletionUnconfirmed")
    }
}

pub(crate) fn execute_controlled_submit<S, E>(
    signals: &S,
    effects: &mut E,
) -> Result<(), PrintFailure>
where
    S: SubmitSignals,
    E: ControlledSubmitEffects,
{
    let before_start = signals.sample();
    if before_start.cancelled {
        return Err(PrintFailure::cancelled(
            "error.print.excelExportCancelled",
            false,
        ));
    }
    if before_start.deadline_expired {
        return Err(PrintFailure::failed("error.print.excelExportTimedOut"));
    }
    let job_id = effects.start_doc()?;
    let after_start = signals.sample();
    if after_start.cancelled {
        let _ = effects.abort_doc();
        return Err(PrintFailure::cancelled(
            "error.print.excelPrintCancelledUnconfirmed",
            true,
        ));
    }
    if after_start.deadline_expired {
        let _ = effects.abort_doc();
        return Err(PrintFailure::uncertain(
            "error.print.excelPrintCompletionUnconfirmed",
        ));
    }
    if let Err(failure) = effects.render_pages() {
        let _ = effects.abort_doc();
        return Err(post_effect_failure(failure));
    }
    if let Err(failure) = effects.end_doc() {
        return Err(post_effect_failure(failure));
    }
    match effects.observe_job(job_id) {
        Ok(()) => Ok(()),
        Err(failure) => Err(post_effect_failure(failure)),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        cell::{Cell, RefCell},
        collections::VecDeque,
    };

    use super::{
        ControlledSubmitEffects, SubmitSignals, SubmitSnapshot, execute_controlled_submit,
    };
    use crate::print_queue::PrintFailure;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Event {
        StartDoc,
        AbortDoc,
        RenderPages,
        EndDoc,
        Observe(u32),
    }

    struct ScriptedSignals {
        snapshots: RefCell<VecDeque<SubmitSnapshot>>,
        samples: Cell<usize>,
    }

    impl ScriptedSignals {
        fn new(snapshots: impl IntoIterator<Item = SubmitSnapshot>) -> Self {
            Self {
                snapshots: RefCell::new(snapshots.into_iter().collect()),
                samples: Cell::new(0),
            }
        }
    }

    impl SubmitSignals for ScriptedSignals {
        fn sample(&self) -> SubmitSnapshot {
            self.samples.set(self.samples.get() + 1);
            self.snapshots
                .borrow_mut()
                .pop_front()
                .expect("one scripted signal snapshot per sample")
        }
    }

    struct RecordingEffects {
        events: Vec<Event>,
        job_id: u32,
        start_failure: Option<PrintFailure>,
        render_failure: Option<PrintFailure>,
        end_failure: Option<PrintFailure>,
        observe_failure: Option<PrintFailure>,
        abort_succeeds: bool,
    }

    impl RecordingEffects {
        fn successful(job_id: u32) -> Self {
            Self {
                events: Vec::new(),
                job_id,
                start_failure: None,
                render_failure: None,
                end_failure: None,
                observe_failure: None,
                abort_succeeds: true,
            }
        }

        fn with_start_failure(mut self, failure: PrintFailure) -> Self {
            self.start_failure = Some(failure);
            self
        }

        fn with_render_failure(mut self, failure: PrintFailure) -> Self {
            self.render_failure = Some(failure);
            self
        }

        fn with_end_failure(mut self, failure: PrintFailure) -> Self {
            self.end_failure = Some(failure);
            self
        }

        fn with_observe_failure(mut self, failure: PrintFailure) -> Self {
            self.observe_failure = Some(failure);
            self
        }

        fn with_abort_failure(mut self) -> Self {
            self.abort_succeeds = false;
            self
        }
    }

    impl ControlledSubmitEffects for RecordingEffects {
        type JobId = u32;

        fn start_doc(&mut self) -> Result<Self::JobId, PrintFailure> {
            self.events.push(Event::StartDoc);
            self.start_failure.clone().map_or(Ok(self.job_id), Err)
        }

        fn abort_doc(&mut self) -> bool {
            self.events.push(Event::AbortDoc);
            self.abort_succeeds
        }

        fn render_pages(&mut self) -> Result<(), PrintFailure> {
            self.events.push(Event::RenderPages);
            self.render_failure.clone().map_or(Ok(()), Err)
        }

        fn end_doc(&mut self) -> Result<(), PrintFailure> {
            self.events.push(Event::EndDoc);
            self.end_failure.clone().map_or(Ok(()), Err)
        }

        fn observe_job(&mut self, job_id: Self::JobId) -> Result<(), PrintFailure> {
            self.events.push(Event::Observe(job_id));
            self.observe_failure.clone().map_or(Ok(()), Err)
        }
    }

    fn clear() -> SubmitSnapshot {
        SubmitSnapshot {
            cancelled: false,
            deadline_expired: false,
        }
    }

    fn cancelled() -> SubmitSnapshot {
        SubmitSnapshot {
            cancelled: true,
            deadline_expired: false,
        }
    }

    fn expired() -> SubmitSnapshot {
        SubmitSnapshot {
            cancelled: false,
            deadline_expired: true,
        }
    }

    #[test]
    fn pre_start_doc_expiry_has_zero_effects() {
        let signals = ScriptedSignals::new([expired()]);
        let mut effects = RecordingEffects::successful(42);

        let failure = execute_controlled_submit(&signals, &mut effects).unwrap_err();

        assert_eq!(signals.samples.get(), 1);
        assert!(effects.events.is_empty());
        assert_eq!(failure.kind, crate::print_queue::PrintFailureKind::Failed);
        assert!(!failure.uncertain);
        assert_eq!(failure.message, "error.print.excelExportTimedOut");
    }

    #[test]
    fn pre_start_doc_cancellation_wins_over_simultaneous_expiry() {
        let signals = ScriptedSignals::new([SubmitSnapshot {
            cancelled: true,
            deadline_expired: true,
        }]);
        let mut effects = RecordingEffects::successful(42);

        let failure = execute_controlled_submit(&signals, &mut effects).unwrap_err();

        assert_eq!(signals.samples.get(), 1);
        assert!(effects.events.is_empty());
        assert_eq!(
            failure.kind,
            crate::print_queue::PrintFailureKind::Cancelled
        );
        assert!(!failure.uncertain);
        assert_eq!(failure.message, "error.print.excelExportCancelled");
    }

    #[test]
    fn pre_start_doc_cancellation_has_zero_effects() {
        let signals = ScriptedSignals::new([cancelled()]);
        let mut effects = RecordingEffects::successful(42);

        let failure = execute_controlled_submit(&signals, &mut effects).unwrap_err();

        assert_eq!(signals.samples.get(), 1);
        assert!(effects.events.is_empty());
        assert_eq!(
            failure.kind,
            crate::print_queue::PrintFailureKind::Cancelled
        );
        assert!(!failure.uncertain);
        assert_eq!(failure.message, "error.print.excelExportCancelled");
    }

    #[test]
    fn rejected_start_doc_is_certain_and_stops_submission() {
        let signals = ScriptedSignals::new([clear()]);
        let mut effects = RecordingEffects::successful(42)
            .with_start_failure(PrintFailure::failed("start doc rejected"));

        let failure = execute_controlled_submit(&signals, &mut effects).unwrap_err();

        assert_eq!(signals.samples.get(), 1);
        assert_eq!(effects.events, [Event::StartDoc]);
        assert_eq!(failure.kind, crate::print_queue::PrintFailureKind::Failed);
        assert!(!failure.uncertain);
        assert_eq!(failure.message, "start doc rejected");
    }

    #[test]
    fn post_start_doc_expiry_aborts_once_and_is_uncertain() {
        let signals = ScriptedSignals::new([clear(), expired()]);
        let mut effects = RecordingEffects::successful(42);

        let failure = execute_controlled_submit(&signals, &mut effects).unwrap_err();

        assert_eq!(signals.samples.get(), 2);
        assert_eq!(effects.events, [Event::StartDoc, Event::AbortDoc]);
        assert_eq!(
            failure.kind,
            crate::print_queue::PrintFailureKind::Uncertain
        );
        assert!(failure.uncertain);
        assert_eq!(
            failure.message,
            "error.print.excelPrintCompletionUnconfirmed"
        );
    }

    #[test]
    fn post_start_doc_cancellation_wins_over_simultaneous_expiry() {
        let signals = ScriptedSignals::new([
            clear(),
            SubmitSnapshot {
                cancelled: true,
                deadline_expired: true,
            },
        ]);
        let mut effects = RecordingEffects::successful(42);

        let failure = execute_controlled_submit(&signals, &mut effects).unwrap_err();

        assert_eq!(signals.samples.get(), 2);
        assert_eq!(effects.events, [Event::StartDoc, Event::AbortDoc]);
        assert_eq!(
            failure.kind,
            crate::print_queue::PrintFailureKind::Cancelled
        );
        assert!(failure.uncertain);
        assert_eq!(
            failure.message,
            "error.print.excelPrintCancelledUnconfirmed"
        );
    }

    #[test]
    fn post_start_doc_cancellation_aborts_once_and_is_uncertain() {
        let signals = ScriptedSignals::new([clear(), cancelled()]);
        let mut effects = RecordingEffects::successful(42);

        let failure = execute_controlled_submit(&signals, &mut effects).unwrap_err();

        assert_eq!(signals.samples.get(), 2);
        assert_eq!(effects.events, [Event::StartDoc, Event::AbortDoc]);
        assert_eq!(
            failure.kind,
            crate::print_queue::PrintFailureKind::Cancelled
        );
        assert!(failure.uncertain);
        assert_eq!(
            failure.message,
            "error.print.excelPrintCancelledUnconfirmed"
        );
    }

    #[test]
    fn abort_failure_does_not_replace_post_start_cancellation() {
        let signals = ScriptedSignals::new([clear(), cancelled()]);
        let mut effects = RecordingEffects::successful(42).with_abort_failure();

        let failure = execute_controlled_submit(&signals, &mut effects).unwrap_err();

        assert_eq!(signals.samples.get(), 2);
        assert_eq!(effects.events, [Event::StartDoc, Event::AbortDoc]);
        assert_eq!(
            failure.kind,
            crate::print_queue::PrintFailureKind::Cancelled
        );
        assert!(failure.uncertain);
        assert_eq!(
            failure.message,
            "error.print.excelPrintCancelledUnconfirmed"
        );
    }

    #[test]
    fn render_cancellation_aborts_once_and_stays_cancelled_uncertain() {
        let signals = ScriptedSignals::new([clear(), clear()]);
        let mut effects = RecordingEffects::successful(42)
            .with_render_failure(PrintFailure::cancelled("render cancelled", false));

        let failure = execute_controlled_submit(&signals, &mut effects).unwrap_err();

        assert_eq!(signals.samples.get(), 2);
        assert_eq!(
            effects.events,
            [Event::StartDoc, Event::RenderPages, Event::AbortDoc]
        );
        assert_eq!(
            failure.kind,
            crate::print_queue::PrintFailureKind::Cancelled
        );
        assert!(failure.uncertain);
        assert_eq!(
            failure.message,
            "error.print.excelPrintCancelledUnconfirmed"
        );
    }

    #[test]
    fn render_failures_abort_once_and_become_uncertain() {
        for render_failure in [
            PrintFailure::failed("render failed"),
            PrintFailure::unsupported("renderer unavailable"),
            PrintFailure::unsupported_options("settings changed"),
        ] {
            let signals = ScriptedSignals::new([clear(), clear()]);
            let mut effects = RecordingEffects::successful(42).with_render_failure(render_failure);

            let failure = execute_controlled_submit(&signals, &mut effects).unwrap_err();

            assert_eq!(signals.samples.get(), 2);
            assert_eq!(
                effects.events,
                [Event::StartDoc, Event::RenderPages, Event::AbortDoc]
            );
            assert_eq!(
                failure.kind,
                crate::print_queue::PrintFailureKind::Uncertain
            );
            assert!(failure.uncertain);
            assert_eq!(
                failure.message,
                "error.print.excelPrintCompletionUnconfirmed"
            );
        }
    }

    #[test]
    fn end_doc_failure_is_uncertain_without_abort_or_observation() {
        let signals = ScriptedSignals::new([clear(), clear()]);
        let mut effects = RecordingEffects::successful(42)
            .with_end_failure(PrintFailure::failed("end doc failed"));

        let failure = execute_controlled_submit(&signals, &mut effects).unwrap_err();

        assert_eq!(signals.samples.get(), 2);
        assert_eq!(
            effects.events,
            [Event::StartDoc, Event::RenderPages, Event::EndDoc]
        );
        assert_eq!(
            failure.kind,
            crate::print_queue::PrintFailureKind::Uncertain
        );
        assert!(failure.uncertain);
        assert_eq!(
            failure.message,
            "error.print.excelPrintCompletionUnconfirmed"
        );
    }

    #[test]
    fn observation_cancellation_stays_cancelled_uncertain() {
        let signals = ScriptedSignals::new([clear(), clear()]);
        let mut effects = RecordingEffects::successful(42)
            .with_observe_failure(PrintFailure::cancelled("observation cancelled", false));

        let failure = execute_controlled_submit(&signals, &mut effects).unwrap_err();

        assert_eq!(signals.samples.get(), 2);
        assert_eq!(
            effects.events,
            [
                Event::StartDoc,
                Event::RenderPages,
                Event::EndDoc,
                Event::Observe(42)
            ]
        );
        assert_eq!(
            failure.kind,
            crate::print_queue::PrintFailureKind::Cancelled
        );
        assert!(failure.uncertain);
        assert_eq!(
            failure.message,
            "error.print.excelPrintCancelledUnconfirmed"
        );
    }

    #[test]
    fn observation_failure_is_uncertain_without_abort() {
        let signals = ScriptedSignals::new([clear(), clear()]);
        let mut effects = RecordingEffects::successful(42)
            .with_observe_failure(PrintFailure::failed("queue read failed"));

        let failure = execute_controlled_submit(&signals, &mut effects).unwrap_err();

        assert_eq!(signals.samples.get(), 2);
        assert_eq!(
            effects.events,
            [
                Event::StartDoc,
                Event::RenderPages,
                Event::EndDoc,
                Event::Observe(42)
            ]
        );
        assert_eq!(
            failure.kind,
            crate::print_queue::PrintFailureKind::Uncertain
        );
        assert!(failure.uncertain);
        assert_eq!(
            failure.message,
            "error.print.excelPrintCompletionUnconfirmed"
        );
    }

    #[test]
    fn success_executes_exact_order_and_propagates_job_id() {
        let signals = ScriptedSignals::new([clear(), clear()]);
        let mut effects = RecordingEffects::successful(42);

        execute_controlled_submit(&signals, &mut effects).unwrap();

        assert_eq!(signals.samples.get(), 2);
        assert_eq!(
            effects.events,
            [
                Event::StartDoc,
                Event::RenderPages,
                Event::EndDoc,
                Event::Observe(42)
            ]
        );
    }
}
