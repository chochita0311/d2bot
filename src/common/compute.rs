//! 실행 중에만 공유하는 제한된 계산 풀. registry는 프레임/작업/강한 소유권을 보유하지 않는다.
use std::{
    cell::Cell,
    fmt, io,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Condvar, Mutex, OnceLock, Weak},
    thread::JoinHandle,
};

#[derive(Clone, Copy)]
enum WorkClass {
    Foreground,
    Background,
}
thread_local! {
    static ACTIVE_CLASS: Cell<Option<WorkClass>> = const { Cell::new(None) };
}
fn budgets() -> (usize, usize) {
    let total = std::thread::available_parallelism().map_or(1, |n| n.get().min(8));
    let background = if total >= 4 {
        2
    } else {
        usize::from(total > 1)
    };
    (total - background, background)
}

struct Pool {
    executor: Option<rayon::ThreadPool>,
    workers: Vec<JoinHandle<()>>,
    #[cfg(test)]
    exited: Arc<std::sync::atomic::AtomicUsize>,
}
pub(crate) fn parallelism() -> usize {
    let (foreground, background) = budgets();
    match ACTIVE_CLASS.get() {
        Some(WorkClass::Background) => background.max(1),
        _ => foreground,
    }
}
struct StartingWorkers(Arc<Mutex<Vec<JoinHandle<()>>>>);
impl Drop for StartingWorkers {
    fn drop(&mut self) {
        let workers = std::mem::take(&mut *self.0.lock().unwrap_or_else(|e| e.into_inner()));
        // 생성 도중 unwind해도 Rayon의 종료 요청 뒤 실제 OS handle을 회수한다.
        for worker in workers {
            crate::common::error::best_effort_cleanup(|| {
                worker.join().map_err(|_| {
                    crate::common::error::ControlError::Runtime(
                        "compute startup worker panicked".into(),
                    )
                })
            });
        }
    }
}
impl Pool {
    #[cfg(test)]
    fn new() -> io::Result<Self> {
        Self::build(
            #[cfg(test)]
            None,
            #[cfg(test)]
            Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        )
    }
    #[cfg(test)]
    fn build(
        #[cfg(test)] fail_before: Option<usize>,
        #[cfg(test)] exited: Arc<std::sync::atomic::AtomicUsize>,
    ) -> io::Result<Self> {
        Self::build_class(
            parallelism(),
            WorkClass::Foreground,
            #[cfg(test)]
            fail_before,
            #[cfg(test)]
            exited,
        )
    }
    fn build_class(
        count: usize,
        class: WorkClass,
        #[cfg(test)] fail_before: Option<usize>,
        #[cfg(test)] exited: Arc<std::sync::atomic::AtomicUsize>,
    ) -> io::Result<Self> {
        let handles = StartingWorkers(Arc::new(Mutex::new(Vec::new())));
        let spawned = handles.0.clone();
        #[cfg(test)]
        let observed = exited.clone();
        let built = rayon::ThreadPoolBuilder::new()
            .num_threads(count)
            .spawn_handler(move |worker| {
                #[cfg(test)]
                if fail_before == Some(worker.index()) {
                    return Err(io::Error::other("injected worker creation failure"));
                }
                #[cfg(test)]
                let observed = observed.clone();
                let prefix = match class {
                    WorkClass::Foreground => "vision-compute",
                    WorkClass::Background => "vision-background",
                };
                let handle = std::thread::Builder::new()
                    .name(format!("{prefix}-{}", worker.index()))
                    .spawn(move || {
                        ACTIVE_CLASS.set(Some(class));
                        worker.run();
                        ACTIVE_CLASS.set(None);
                        #[cfg(test)]
                        observed.fetch_add(1, std::sync::atomic::Ordering::Release);
                    })?;
                spawned
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(handle);
                Ok(())
            })
            .build();
        let workers = std::mem::take(&mut *handles.0.lock().unwrap_or_else(|e| e.into_inner()));
        match built {
            Ok(executor) => Ok(Self {
                executor: Some(executor),
                workers,
                #[cfg(test)]
                exited,
            }),
            Err(error) => {
                // Rayon은 부분 생성 실패 때 시작한 worker의 종료를 요청한다. 소유한
                // OS handle까지 join한 뒤 실패를 반환해 detached thread를 남기지 않는다.
                for worker in workers {
                    let _ = worker.join();
                }
                Err(io::Error::other(error))
            }
        }
    }
}

struct Pools {
    foreground: Pool,
    background: Option<Pool>,
}
impl Pools {
    fn new() -> io::Result<Self> {
        let (foreground, background) = budgets();
        let build = |count, class| {
            Pool::build_class(
                count,
                class,
                #[cfg(test)]
                None,
                #[cfg(test)]
                Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            )
        };
        let foreground = build(foreground, WorkClass::Foreground)?;
        // 두 번째 풀 생성 실패 시 첫 풀도 실제 worker join 뒤 해제한다.
        let background = (background > 0)
            .then(|| build(background, WorkClass::Background))
            .transpose()?;
        Ok(Self {
            foreground,
            background,
        })
    }
    fn executor(&self, class: WorkClass) -> io::Result<&rayon::ThreadPool> {
        let pool = match class {
            WorkClass::Foreground => &self.foreground,
            WorkClass::Background => self.background.as_ref().unwrap_or(&self.foreground),
        };
        pool.executor
            .as_ref()
            .ok_or_else(|| io::Error::other("compute pool closed"))
    }
}
impl Drop for Pool {
    fn drop(&mut self) {
        drop(self.executor.take());
        for worker in self.workers.drain(..) {
            // 계산 scope의 외부 lease가 모든 borrowed job이 끝날 때까지 살아 있으므로
            // 마지막 소유권은 이 pool의 job 안에서 해제되지 않는다.
            crate::common::error::best_effort_cleanup(|| {
                worker.join().map_err(|_| {
                    crate::common::error::ControlError::Runtime(
                        "compute worker panicked while closing".into(),
                    )
                })
            });
        }
    }
}

/// 활성 세션/관찰기가 보유한다. 마지막 lease 해제는 실제 계산 worker의 join까지 끝낸다.
#[derive(Clone)]
pub struct ComputeLease(Arc<Pools>);
impl fmt::Debug for ComputeLease {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ComputeLease")
            .field("foreground", &self.0.foreground.workers.len())
            .field(
                "background",
                &self.0.background.as_ref().map_or(0, |p| p.workers.len()),
            )
            .finish()
    }
}
impl ComputeLease {
    pub fn acquire() -> io::Result<Self> {
        #[derive(Default)]
        struct State {
            pool: Weak<Pools>,
            building: bool,
        }
        static REGISTRY: OnceLock<(Mutex<State>, Condvar)> = OnceLock::new();
        let (lock, wake) = REGISTRY.get_or_init(|| (Mutex::new(State::default()), Condvar::new()));
        loop {
            let mut registry = lock
                .lock()
                .map_err(|_| io::Error::other("compute registry lock poisoned"))?;
            if let Some(pool) = registry.pool.upgrade() {
                return Ok(Self(pool));
            }
            if registry.building {
                drop(
                    wake.wait(registry)
                        .map_err(|_| io::Error::other("compute registry wait poisoned"))?,
                );
                continue;
            }
            registry.building = true;
            drop(registry);
            // 생성 실패의 worker join도 registry lock 밖에서 끝낸다.
            let built = catch_unwind(Pools::new)
                .unwrap_or_else(|_| Err(io::Error::other("compute pool initialization panicked")))
                .map(Arc::new);
            let mut registry = lock
                .lock()
                .map_err(|_| io::Error::other("compute registry lock poisoned"))?;
            registry.building = false;
            if let Ok(pool) = &built {
                registry.pool = Arc::downgrade(pool);
            }
            wake.notify_all();
            drop(registry);
            return built.map(Self);
        }
    }
}

#[derive(Debug)]
pub(crate) enum ComputeError {
    Start(io::Error),
    Panicked,
}
/// 모든 job을 마친 뒤에만 반환하며 nested scope도 같은 제한된 pool을 공유한다.
pub(crate) fn scope<'scope, R: Send>(
    operation: impl FnOnce(&rayon::Scope<'scope>) -> R + Send,
) -> Result<R, ComputeError> {
    // 내부 job은 진입한 풀에 남아야 broad FFT가 foreground를 다시 점유하지 않는다.
    if ACTIVE_CLASS.get().is_some() {
        return catch_unwind(AssertUnwindSafe(|| rayon::scope(operation)))
            .map_err(|_| ComputeError::Panicked);
    }
    let lease = ComputeLease::acquire().map_err(ComputeError::Start)?;
    let executor = lease
        .0
        .executor(WorkClass::Foreground)
        .map_err(ComputeError::Start)?;
    catch_unwind(AssertUnwindSafe(|| executor.scope(operation))).map_err(|_| ComputeError::Panicked)
}

/// 외부 host의 lease가 nested 작업과 결과 회수까지 두 풀을 소유한다.
pub(crate) fn background<R: Send>(operation: impl FnOnce() -> R + Send) -> Result<R, ComputeError> {
    let lease = ComputeLease::acquire().map_err(ComputeError::Start)?;
    let executor = lease
        .0
        .executor(WorkClass::Background)
        .map_err(ComputeError::Start)?;
    catch_unwind(AssertUnwindSafe(|| executor.install(operation)))
        .map_err(|_| ComputeError::Panicked)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;
    #[test]
    fn concurrent_acquisition_uses_one_shared_pool() -> Result<(), Box<dyn std::error::Error>> {
        let barrier = std::sync::Barrier::new(8);
        let leases = std::thread::scope(|s| {
            let handles: Vec<_> = (0..8)
                .map(|_| {
                    s.spawn(|| {
                        barrier.wait();
                        ComputeLease::acquire()
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| {
                    h.join()
                        .map_err(|_| io::Error::other("acquisition test worker panicked"))?
                })
                .collect::<io::Result<Vec<_>>>()
        })?;
        for lease in &leases {
            assert!(Arc::ptr_eq(&lease.0, &leases[0].0));
        }
        Ok(())
    }
    #[test]
    fn partial_creation_failure_joins_started_workers() {
        let exited = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        assert!(Pool::build(Some(0), exited.clone()).is_err());
        assert_eq!(exited.load(Ordering::Acquire), 0);
        if parallelism() > 1 {
            assert!(Pool::build(Some(1), exited.clone()).is_err());
            assert_eq!(exited.load(Ordering::Acquire), 1);
        }
    }
    #[test]
    fn last_owner_joins_workers_and_releases_borrowed_data()
    -> Result<(), Box<dyn std::error::Error>> {
        let pool = Arc::new(Pool::new()?);
        let exited = pool.exited.clone();
        let count = pool.workers.len();
        let frame = Arc::new(vec![7u8; 1024]);
        let weak = Arc::downgrade(&frame);
        let mut result = [0; 4];
        if let Some(executor) = &pool.executor {
            executor.scope(|s| {
                for output in &mut result {
                    let frame = &frame;
                    s.spawn(move |_| *output = frame[0]);
                }
            });
        }
        assert_eq!(result, [7; 4]);
        drop(frame);
        assert!(weak.upgrade().is_none());
        let other = pool.clone();
        drop(pool);
        assert_eq!(exited.load(Ordering::Acquire), 0);
        drop(other);
        assert_eq!(exited.load(Ordering::Acquire), count);
        Ok(())
    }
    #[test]
    fn nested_scopes_share_workers_and_panic_does_not_poison_reuse()
    -> Result<(), Box<dyn std::error::Error>> {
        let a = ComputeLease::acquire()?;
        let b = ComputeLease::acquire()?;
        assert!(Arc::ptr_eq(&a.0, &b.0));
        let mut rows = [[0; 4]; 4];
        scope(|outer| {
            for row in &mut rows {
                outer.spawn(move |_| {
                    assert!(
                        scope(|inner| for value in row {
                            inner.spawn(move |_| *value = 19);
                        })
                        .is_ok()
                    );
                });
            }
        })
        .map_err(|e| format!("{e:?}"))?;
        assert_eq!(rows, [[19; 4]; 4]);
        assert!(matches!(
            scope(|s| s.spawn(|_| panic!("intentional scoped job fault"))),
            Err(ComputeError::Panicked)
        ));
        assert_eq!(scope(|_| 23).map_err(|e| format!("{e:?}"))?, 23);
        Ok(())
    }

    #[test]
    fn background_nested_jobs_stay_within_reserved_workers_and_recover_from_panic()
    -> Result<(), Box<dyn std::error::Error>> {
        let lease = ComputeLease::acquire()?;
        let (foreground, background_count) = budgets();
        assert_eq!(lease.0.foreground.workers.len(), foreground);
        assert_eq!(
            lease.0.background.as_ref().map_or(0, |p| p.workers.len()),
            background_count
        );
        assert!((1..=8).contains(&(foreground + background_count)));
        let names = background(|| {
            let mut names = [String::new(), String::new()];
            scope(|s| {
                for name in &mut names {
                    s.spawn(move |_| {
                        *name = std::thread::current().name().unwrap_or("").to_owned();
                        assert_eq!(parallelism(), background_count.max(1));
                    });
                }
            })
            .map(|_| names)
        })
        .map_err(|e| format!("{e:?}"))?
        .map_err(|e| format!("{e:?}"))?;
        let prefix = if background_count > 0 {
            "vision-background-"
        } else {
            "vision-compute-"
        };
        assert!(names.iter().all(|name| name.starts_with(prefix)));
        assert!(matches!(
            background(|| panic!("background panic fixture")),
            Err(ComputeError::Panicked)
        ));
        assert_eq!(background(|| 29).map_err(|e| format!("{e:?}"))?, 29);
        Ok(())
    }

    #[test]
    fn blocked_background_does_not_queue_foreground_and_family_joins_both_pools()
    -> Result<(), Box<dyn std::error::Error>> {
        let (foreground_count, background_count) = budgets();
        let pools = Arc::new(Pools::new()?);
        let foreground_exited = pools.foreground.exited.clone();
        let background_exited = pools.background.as_ref().map(|p| p.exited.clone());
        if background_count > 0 {
            let (entered_tx, entered_rx) = std::sync::mpsc::channel();
            let (done_tx, done_rx) = std::sync::mpsc::channel();
            let mut releases = vec![];
            let mut waits = vec![];
            for _ in 0..background_count {
                let (release, wait) = std::sync::mpsc::channel::<()>();
                releases.push(release);
                waits.push(wait);
            }
            std::thread::scope(|threads| -> Result<(), Box<dyn std::error::Error>> {
                let slow = pools.clone();
                threads.spawn(move || {
                    if let Ok(executor) = slow.executor(WorkClass::Background) {
                        executor.scope(|s| {
                            for wait in waits {
                                let entered = entered_tx.clone();
                                s.spawn(move |_| {
                                    let _ = entered.send(());
                                    let _ = wait.recv_timeout(std::time::Duration::from_secs(3));
                                });
                            }
                        });
                    }
                });
                for _ in 0..background_count {
                    entered_rx.recv_timeout(std::time::Duration::from_secs(2))?;
                }
                let fast = pools.clone();
                threads.spawn(move || {
                    if let Ok(executor) = fast.executor(WorkClass::Foreground) {
                        executor.scope(|s| {
                            s.spawn(move |_| {
                                let _ = done_tx.send(());
                            });
                        });
                    }
                });
                let foreground_completed = done_rx.recv_timeout(std::time::Duration::from_secs(1));
                for release in releases {
                    let _ = release.send(());
                }
                foreground_completed?;
                Ok(())
            })?;
        }
        drop(pools);
        assert_eq!(foreground_exited.load(Ordering::Acquire), foreground_count);
        if let Some(exited) = background_exited {
            assert_eq!(exited.load(Ordering::Acquire), background_count);
        }
        Ok(())
    }
}
