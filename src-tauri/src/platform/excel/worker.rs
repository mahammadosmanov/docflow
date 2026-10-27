use super::*;

pub(crate) trait ExcelWorkerRuntime: Send + Sync {
    fn launch(
        &self,
        exporter: Arc<dyn WorksheetPdfExporter>,
        request: ExcelExportRequest,
    ) -> Result<Box<dyn ExcelWorkerHandle>, PrintFailure>;
}

pub(crate) trait ExcelWorkerHandle: Send {
    fn poll(&mut self, wait: Duration) -> WorkerPoll;
    fn join_status(&mut self) -> WorkerJoin;
}

pub(crate) enum WorkerPoll {
    Response(Box<WorkerResponse>),
    Pending,
    Disconnected,
}

pub(crate) enum WorkerJoin {
    Pending,
    Finished,
    Failed,
}

pub(super) struct ThreadWorkerRuntime;

impl ExcelWorkerRuntime for ThreadWorkerRuntime {
    fn launch(
        &self,
        exporter: Arc<dyn WorksheetPdfExporter>,
        request: ExcelExportRequest,
    ) -> Result<Box<dyn ExcelWorkerHandle>, PrintFailure> {
        let request_id = request.request_id;
        let generation = request.generation;
        let (sender, receiver) = mpsc::sync_channel(1);
        let join = thread::Builder::new()
            .name("docflow-excel-export".to_string())
            .spawn(move || {
                let result = exporter.export_pdf(request);
                let _ = sender.send(WorkerResponse {
                    request_id,
                    generation,
                    result,
                });
            })
            .map_err(|_| PrintFailure::failed("error.print.excelWorkerStartFailed"))?;
        Ok(Box::new(ThreadWorkerHandle {
            receiver,
            join: Some(join),
            join_result: None,
        }))
    }
}

struct ThreadWorkerHandle {
    receiver: mpsc::Receiver<WorkerResponse>,
    join: Option<thread::JoinHandle<()>>,
    join_result: Option<bool>,
}

impl ExcelWorkerHandle for ThreadWorkerHandle {
    fn poll(&mut self, wait: Duration) -> WorkerPoll {
        match self.receiver.recv_timeout(wait) {
            Ok(response) => WorkerPoll::Response(Box::new(response)),
            Err(mpsc::RecvTimeoutError::Timeout) => WorkerPoll::Pending,
            Err(mpsc::RecvTimeoutError::Disconnected) => WorkerPoll::Disconnected,
        }
    }

    fn join_status(&mut self) -> WorkerJoin {
        if let Some(join) = self.join.as_ref()
            && !join.is_finished()
        {
            return WorkerJoin::Pending;
        }
        if let Some(join) = self.join.take() {
            self.join_result = Some(join.join().is_ok());
        }
        match self.join_result {
            Some(true) => WorkerJoin::Finished,
            Some(false) | None => WorkerJoin::Failed,
        }
    }
}
