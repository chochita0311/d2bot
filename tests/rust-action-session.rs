use d2bot::actions::{
    ActionError,
    session::{ActionSession, SessionOutcome, SessionStatus},
};
use std::{error::Error, sync::mpsc, time::Duration};

#[test]
fn stop_timeout_retains_live_worker_and_rejects_overlapping_restart() -> Result<(), Box<dyn Error>>
{
    let mut session = ActionSession::new("Loot pickup");
    let (entered_tx, entered_rx) = mpsc::channel();
    let (exit_tx, exit_rx) = mpsc::channel();
    session.start(move |token, _| {
        entered_tx
            .send(())
            .map_err(|e| ActionError::runtime(e.to_string()))?;
        exit_rx
            .recv()
            .map_err(|e| ActionError::runtime(e.to_string()))?;
        assert!(token.is_cancelled());
        Ok(SessionOutcome::Interrupted)
    })?;
    entered_rx.recv_timeout(Duration::from_secs(2))?;
    assert_eq!(session.status()?, SessionStatus::Running);
    assert!(session.start(|_, _| Ok(SessionOutcome::Completed)).is_err());
    assert!(session.stop_with_event(Duration::from_millis(1)).is_err());
    assert_eq!(session.status()?, SessionStatus::Stopping);
    assert!(session.start(|_, _| Ok(SessionOutcome::Completed)).is_err());
    assert!(session.events().drain()[0].message.ends_with("stopping."));
    exit_tx.send(())?;
    session.stop(Duration::from_secs(2))?;
    assert_eq!(
        session.status()?,
        SessionStatus::Finished(Ok(SessionOutcome::Interrupted))
    );
    session.start(|_, _| Ok(SessionOutcome::Completed))?;
    session.stop(Duration::from_secs(2))?;
    assert_eq!(
        session.status()?,
        SessionStatus::Finished(Ok(SessionOutcome::Completed))
    );
    Ok(())
}
#[test]
fn ordinary_failure_and_panic_remain_observable_after_join() -> Result<(), Box<dyn Error>> {
    let mut session = ActionSession::new("test action");
    session.start(|_, _| Err(ActionError::runtime("capture failed")))?;
    session.stop(Duration::from_secs(2))?;
    assert_eq!(
        session.status()?,
        SessionStatus::Finished(Err("capture failed".into()))
    );
    session.start(|_, _| panic!("fixture panic"))?;
    session.stop(Duration::from_secs(2))?;
    assert!(
        matches!(session.status()?,SessionStatus::Finished(Err(error))if error.contains("fixture panic"))
    );
    Ok(())
}
