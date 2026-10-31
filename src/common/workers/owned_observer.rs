use crate::common::error::ObservationError;
use std::sync::{Arc, Mutex};
/// 잠금은 소유권 전달에만 쓴다. 판독·외부 호출 중에는 독점 소유하고 unwind에도 반환한다.
pub(crate) struct OwnedObserver<T> {
    owner: Arc<Mutex<Option<T>>>,
    name: &'static str,
}
impl<T> Clone for OwnedObserver<T> {
    fn clone(&self) -> Self {
        Self {
            owner: self.owner.clone(),
            name: self.name,
        }
    }
}
impl<T> OwnedObserver<T> {
    pub(crate) fn new(observer: T, name: &'static str) -> Self {
        Self {
            owner: Arc::new(Mutex::new(Some(observer))),
            name,
        }
    }
    pub(crate) fn with<R>(
        &self,
        call: impl FnOnce(&mut T) -> Result<R, ObservationError>,
    ) -> Result<R, ObservationError> {
        self.with_error(call)
    }
    pub(crate) fn with_error<R, E: From<ObservationError>>(
        &self,
        call: impl FnOnce(&mut T) -> Result<R, E>,
    ) -> Result<R, E> {
        let observer = self
            .owner
            .lock()
            .map_err(|_| ObservationError::invalid(format!("{} owner lock poisoned", self.name)))?
            .take()
            .ok_or_else(|| {
                ObservationError::invalid(format!("{} observation already in flight", self.name))
            })?;
        let mut lease = Lease {
            observer: Some(observer),
            owner: self.owner.clone(),
        };
        call(
            lease.observer.as_mut().ok_or_else(|| {
                ObservationError::invalid(format!("{} lease unavailable", self.name))
            })?,
        )
    }
}
struct Lease<T> {
    observer: Option<T>,
    owner: Arc<Mutex<Option<T>>>,
}
impl<T> Drop for Lease<T> {
    fn drop(&mut self) {
        if let Some(observer) = self.observer.take() {
            *self.owner.lock().unwrap_or_else(|e| e.into_inner()) = Some(observer);
        }
    }
}
