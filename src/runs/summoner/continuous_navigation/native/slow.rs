use super::*;
type ScanTask<T> = ObservationTask<Result<Option<T>, ObservationError>>;
/// 음성/미완료는 현재 화면의 판정이 아니다. 양성만 현재 화면 재확인을 요청한다.
pub(super) struct SlowScan<T> {
    pending: Option<(f64, ScanTask<T>)>,
    warmed: bool,
    hint: Option<f64>,
    completed_source: Option<f64>,
    confirmation: bool,
}
impl<T: Send + 'static> Default for SlowScan<T> {
    fn default() -> Self {
        Self {
            pending: None,
            warmed: false,
            hint: None,
            completed_source: None,
            confirmation: false,
        }
    }
}
impl<T: Send + 'static> SlowScan<T> {
    pub fn schedule(
        &mut self,
        pool: &ObservationPool,
        current_pool: &ObservationPool,
        acquired: f64,
        force_current: bool,
        work: impl FnOnce() -> Result<Option<T>, ObservationError> + Send + 'static,
    ) -> Result<Option<ScanTask<T>>, RunError> {
        self.confirmation = false;
        if let Some((source, task)) = &self.pending
            && let Some(result) = task.try_join()?
        {
            self.completed_source = Some(*source);
            if result?.is_some() {
                self.hint = Some(*source);
            }
            self.pending = None;
        }
        let current = !self.warmed || force_current || self.hint.is_some();
        if current {
            // 접근 전환 중 이전 작업이 남았으면 먼저 회수한다. 현재 좌표로 재라벨하지 않는다.
            if let Some((_, task)) = self.pending.take() {
                task.join()??;
            }
            self.confirmation = self.hint.is_some();
            self.hint = None;
            self.warmed = true;
            // 현재 확인은 입력 경로가 기다린다. broad host/계산 예산 뒤에 세우지 않는다.
            return Ok(Some(current_pool.submit(work)?));
        }
        if self.pending.is_none() {
            self.pending = Some((
                acquired,
                pool.submit(move || match crate::common::compute::background(work) {
                    Ok(result) => result,
                    Err(crate::common::compute::ComputeError::Start(error)) => {
                        Err(ObservationError::Io(error))
                    }
                    Err(crate::common::compute::ComputeError::Panicked) => {
                        Err(ObservationError::invalid("background observation panicked"))
                    }
                })?,
            ));
        }
        Ok(None)
    }
    pub fn confirmation(&self) -> bool {
        self.confirmation
    }
    pub fn diagnostics(&self) -> Value {
        json!({"pending_source_at":self.pending.as_ref().map(|p|p.0),"completed_source_at":self.completed_source,"current_confirmation":self.confirmation,"negative_scope":"goal_unknown"})
    }
    pub fn drain(&mut self) -> Result<(), RunError> {
        if let Some((_, task)) = self.pending.take() {
            task.join()??;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
    };
    #[test]
    fn pending_is_bounded_and_old_positive_only_requests_current_confirmation()
    -> Result<(), RunError> {
        let mut pool = ObservationPool::new(1, 1, CooperativeStop::default())?;
        let mut scan = SlowScan::<u8>::default();
        let initial = scan
            .schedule(&pool, &pool, 1.0, false, || Ok(None))?
            .ok_or_else(|| missing("initial scan missing"))?;
        assert_eq!(initial.join()??, None);
        let (entered_tx, entered_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let count = Arc::new(AtomicUsize::new(0));
        let calls = count.clone();
        assert!(
            scan.schedule(&pool, &pool, 2.0, false, move || {
                calls.fetch_add(1, Ordering::SeqCst);
                entered_tx
                    .send(())
                    .map_err(|e| ObservationError::invalid(e.to_string()))?;
                resume_rx
                    .recv()
                    .map_err(|e| ObservationError::invalid(e.to_string()))?;
                Ok(Some(7))
            })?
            .is_none()
        );
        entered_rx.recv().map_err(|e| missing(&e.to_string()))?;
        assert!(
            scan.schedule(&pool, &pool, 3.0, false, || panic!(
                "pending work must not be duplicated"
            ))?
            .is_none()
        );
        assert_eq!(count.load(Ordering::SeqCst), 1);
        resume_tx.send(()).map_err(|e| missing(&e.to_string()))?;
        // 실제 작업 종료 뒤 poll만 수행하므로 wall-clock sleep에 의존하지 않는다.
        pool.finish(Duration::from_secs(2))?;
        let mut fresh_pool = ObservationPool::new(1, 1, CooperativeStop::default())?;
        let confirmation = scan
            .schedule(&fresh_pool, &fresh_pool, 4.0, false, || Ok(None))?
            .ok_or_else(|| missing("confirmation missing"))?;
        assert!(scan.confirmation());
        assert_eq!(scan.diagnostics()["completed_source_at"], json!(2.0));
        assert_eq!(confirmation.join()??, None);
        fresh_pool.finish(Duration::from_secs(2))?;
        scan.drain()?;
        Ok(())
    }
    #[test]
    fn current_confirmation_does_not_queue_behind_broad_host() -> Result<(), RunError> {
        let mut broad = ObservationPool::new(1, 1, CooperativeStop::default())?;
        let mut foreground = ObservationPool::new(1, 1, CooperativeStop::default())?;
        let mut scan = SlowScan::<u8>::default();
        scan.schedule(&broad, &foreground, 1.0, false, || Ok(None))?
            .ok_or_else(|| missing("initial current scan missing"))?
            .join()??;
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let blocked = broad.submit(move || {
            let _ = entered_tx.send(());
            let _ = release_rx.recv();
        })?;
        entered_rx.recv().map_err(|e| missing(&e.to_string()))?;
        let (ready_tx, ready_rx) = mpsc::channel();
        let current = scan
            .schedule(&broad, &foreground, 2.0, true, move || {
                let name = crate::common::compute::scope(|_| {
                    std::thread::current().name().unwrap_or("").to_owned()
                })
                .map_err(|_| ObservationError::invalid("current compute failed"))?;
                let _ = ready_tx.send(name);
                Ok(Some(9))
            })?
            .ok_or_else(|| missing("forced current scan missing"))?;
        let ready = ready_rx.recv_timeout(Duration::from_secs(2));
        // 실패한 구현도 blocked 작업을 풀고 실제 join한 다음 assertion을 수행한다.
        release_tx.send(()).map_err(|e| missing(&e.to_string()))?;
        blocked.join()?;
        assert_eq!(current.join()??, Some(9));
        assert!(ready.is_ok_and(|name| name.starts_with("vision-compute-")));
        broad.finish(Duration::from_secs(2))?;
        foreground.finish(Duration::from_secs(2))?;
        scan.drain()?;
        Ok(())
    }
    #[test]
    fn background_failure_is_reported_during_drain() -> Result<(), RunError> {
        let mut pool = ObservationPool::new(1, 1, CooperativeStop::default())?;
        let mut scan = SlowScan::<u8>::default();
        scan.schedule(&pool, &pool, 1.0, false, || Ok(None))?
            .ok_or_else(|| missing("initial scan missing"))?
            .join()??;
        assert!(
            scan.schedule(&pool, &pool, 2.0, false, || Err(ObservationError::invalid(
                "slow fixture failure"
            )))?
            .is_none()
        );
        pool.finish(Duration::from_secs(2))?;
        assert!(
            scan.drain()
                .is_err_and(|e| e.to_string().contains("slow fixture failure"))
        );
        Ok(())
    }
}
