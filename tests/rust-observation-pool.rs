use d2bot::common::workers::{CooperativeStop, ObservationPool};
use std::{
    error::Error,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
};
type TestResult = Result<(), Box<dyn Error>>;
#[test]
fn drains_accepted_tasks_and_preserves_submission_results() -> TestResult {
    let mut pool = ObservationPool::new(3, 3, CooperativeStop::default())?;
    let completed = Arc::new(AtomicUsize::new(0));
    let mut tasks = vec![];
    for value in 0..20 {
        let completed = completed.clone();
        tasks.push(pool.submit(move || {
            completed.fetch_add(1, Ordering::AcqRel);
            value * value
        })?);
    }
    pool.finish(Duration::from_secs(2))?;
    for (value, task) in tasks.into_iter().enumerate() {
        assert_eq!(task.join()?, value * value);
    }
    assert_eq!(completed.load(Ordering::Acquire), 20);
    assert!(pool.is_finished());
    assert!(pool.submit(|| 0).is_err());
    Ok(())
}
#[test]
fn task_panic_is_an_error_and_worker_remains_available() -> TestResult {
    let mut pool = ObservationPool::new(1, 1, CooperativeStop::default())?;
    let task = pool.submit(|| panic!("vision panic fixture"))?;
    assert!(
        task.join()
            .is_err_and(|error| error.to_string().contains("vision panic fixture"))
    );
    assert_eq!(pool.submit(|| 42)?.join()?, 42);
    pool.finish(Duration::from_secs(1))?;
    Ok(())
}
#[test]
fn timed_out_shutdown_keeps_active_and_queued_tasks_for_retry() -> TestResult {
    let mut pool = ObservationPool::new(1, 1, CooperativeStop::default())?;
    let (entered_tx, entered_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    let active = pool.submit(move || {
        let _ = entered_tx.send(());
        resume_rx.recv_timeout(Duration::from_secs(2))
    })?;
    entered_rx.recv_timeout(Duration::from_secs(2))?;
    let queued = pool.submit(|| 7)?;
    assert_eq!(pool.queued()?, 1);
    assert!(pool.finish(Duration::ZERO).is_err());
    assert!(!pool.is_finished());
    assert!(pool.submit(|| 8).is_err());
    resume_tx.send(())?;
    pool.finish(Duration::from_secs(1))?;
    active.join()??;
    assert_eq!(queued.join()?, 7);
    Ok(())
}
#[test]
fn stop_interrupts_full_queue_submission_without_discarding_accepted_work() -> TestResult {
    let stop = CooperativeStop::default();
    let mut pool = ObservationPool::new(1, 1, stop.clone())?;
    let (entered_tx, entered_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    let active = pool.submit(move || {
        let _ = entered_tx.send(());
        resume_rx.recv_timeout(Duration::from_secs(2))
    })?;
    entered_rx.recv_timeout(Duration::from_secs(2))?;
    let accepted = pool.submit(|| 11)?;
    stop.set();
    assert!(pool.submit(|| 12).is_err());
    resume_tx.send(())?;
    pool.finish(Duration::from_secs(1))?;
    active.join()??;
    assert_eq!(accepted.join()?, 11);
    Ok(())
}
#[test]
fn abandoned_result_releases_task_owned_frame_after_shutdown() -> TestResult {
    let mut pool = ObservationPool::new(1, 1, CooperativeStop::default())?;
    let frame = Arc::new(vec![0u8; 4096]);
    let weak = Arc::downgrade(&frame);
    drop(pool.submit(move || frame)?);
    pool.finish(Duration::from_secs(1))?;
    assert!(weak.upgrade().is_none());
    Ok(())
}
