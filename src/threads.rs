#![allow(clippy::disallowed_methods)]

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ThreadTag {
    Warming,
    PathWarming,
    Flycomp,
    TabCompletion,
}

impl ThreadTag {
    pub(crate) fn uses_bash_funcs(&self) -> bool {
        match self {
            ThreadTag::Warming => true,
            ThreadTag::PathWarming => false,
            ThreadTag::Flycomp => false,
            ThreadTag::TabCompletion => false,
        }
    }

    pub(crate) fn thread_name(&self) -> &'static str {
        match self {
            ThreadTag::Warming => "flyline-warming",
            ThreadTag::PathWarming => "flyline-path-warming",
            ThreadTag::Flycomp => "flyline-flycomp",
            ThreadTag::TabCompletion => "flyline-completions",
        }
    }
}

pub(crate) trait Joinable: Send + Sync {
    fn join(&self) -> Result<(), std::boxed::Box<dyn std::any::Any + Send>>;
    fn is_finished(&self) -> bool;
}

pub(crate) struct TaskState<T> {
    result: Mutex<Option<Result<T, Box<dyn std::any::Any + Send>>>>,
    finished: AtomicBool,
    condvar: Condvar,
}

impl<T> TaskState<T> {
    pub fn new() -> Self {
        Self {
            result: Mutex::new(None),
            finished: AtomicBool::new(false),
            condvar: Condvar::new(),
        }
    }

    pub fn set_result(&self, res: Result<T, Box<dyn std::any::Any + Send>>) {
        if let Ok(mut guard) = self.result.lock() {
            *guard = Some(res);
        }
        self.finished.store(true, Ordering::Release);
        self.condvar.notify_all();
    }

    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }

    pub fn join_value(&self) -> Option<Result<T, Box<dyn std::any::Any + Send>>> {
        let mut guard = self.result.lock().ok()?;
        while !self.finished.load(Ordering::Acquire) {
            guard = self.condvar.wait(guard).ok()?;
        }
        guard.take()
    }
}

pub(crate) struct SharedJoinHandle<T> {
    inner: Arc<TaskState<T>>,
}

impl<T> Clone for SharedJoinHandle<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T> std::fmt::Debug for SharedJoinHandle<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedJoinHandle").finish()
    }
}

impl<T> SharedJoinHandle<T> {
    pub(crate) fn new(state: Arc<TaskState<T>>) -> Self {
        Self { inner: state }
    }

    pub(crate) fn join_value(
        &self,
    ) -> Option<Result<T, std::boxed::Box<dyn std::any::Any + Send>>> {
        self.inner.join_value()
    }

    pub(crate) fn is_finished(&self) -> bool {
        self.inner.is_finished()
    }
}

impl<T: Send + 'static> Joinable for SharedJoinHandle<T> {
    fn join(&self) -> Result<(), std::boxed::Box<dyn std::any::Any + Send>> {
        if let Some(res) = self.join_value() {
            match res {
                Ok(_) => Ok(()),
                Err(e) => Err(e),
            }
        } else {
            Ok(())
        }
    }

    fn is_finished(&self) -> bool {
        self.is_finished()
    }
}

pub(crate) struct TrackedThread {
    pub(crate) tag: ThreadTag,
    pub(crate) handle: Box<dyn Joinable>,
}

pub(crate) static BACKGROUND_THREADS: Mutex<Vec<TrackedThread>> = Mutex::new(Vec::new());

type TaskJob = Box<dyn FnOnce() + Send + 'static>;

struct WorkerQueue {
    tasks: std::collections::VecDeque<TaskJob>,
    shutdown: bool,
}

struct PersistentThreadPool {
    state: Mutex<WorkerQueue>,
    condvar: Condvar,
    workers: Mutex<Vec<std::thread::JoinHandle<()>>>,
    spawn_lock: Mutex<()>,
    idle_workers: AtomicUsize,
}

impl PersistentThreadPool {
    fn new() -> Self {
        log::info!("[Threads] Initializing persistent worker thread pool");
        let pool = Self {
            state: Mutex::new(WorkerQueue {
                tasks: std::collections::VecDeque::new(),
                shutdown: false,
            }),
            condvar: Condvar::new(),
            workers: Mutex::new(Vec::new()),
            spawn_lock: Mutex::new(()),
            idle_workers: AtomicUsize::new(0),
        };
        pool.ensure_workers(4);
        log::info!("[Threads] Persistent worker thread pool initialized with 4 workers");
        pool
    }

    fn ensure_workers(&self, min_workers: usize) {
        let _spawn_guard = self.spawn_lock.lock().unwrap();
        let mut workers = self.workers.lock().unwrap();
        while workers.len() < min_workers {
            let worker_id = workers.len();
            let pool_state_ptr = &self.state as *const Mutex<WorkerQueue> as usize;
            let pool_condvar_ptr = &self.condvar as *const Condvar as usize;
            let idle_ptr = &self.idle_workers as *const AtomicUsize as usize;

            let ready_pair = Arc::new((Mutex::new(false), Condvar::new()));
            let ready_clone = ready_pair.clone();

            let builder =
                std::thread::Builder::new().name(format!("flyline-worker-{}", worker_id));
            let handle = builder
                .spawn(move || {
                    let state_ref = unsafe { &*(pool_state_ptr as *const Mutex<WorkerQueue>) };
                    let condvar_ref = unsafe { &*(pool_condvar_ptr as *const Condvar) };
                    let idle_ref = unsafe { &*(idle_ptr as *const AtomicUsize) };

                    // Signal parent thread that glibc TLS / thread startup allocation is complete
                    {
                        let (lock, cvar) = &*ready_clone;
                        let mut started = lock.lock().unwrap();
                        *started = true;
                        cvar.notify_all();
                    }

                    loop {
                        idle_ref.fetch_add(1, Ordering::SeqCst);
                        let task = {
                            let mut guard = state_ref.lock().unwrap();
                            while guard.tasks.is_empty() && !guard.shutdown {
                                guard = condvar_ref.wait(guard).unwrap();
                            }
                            if guard.shutdown && guard.tasks.is_empty() {
                                idle_ref.fetch_sub(1, Ordering::SeqCst);
                                break;
                            }
                            guard.tasks.pop_front()
                        };
                        idle_ref.fetch_sub(1, Ordering::SeqCst);

                        if let Some(task) = task {
                            task();
                        }
                    }
                })
                .expect("Failed to spawn persistent worker thread");

            // Wait until the spawned worker thread completes its OS/glibc TLS initialization
            let (lock, cvar) = &*ready_pair;
            let mut started = lock.lock().unwrap();
            while !*started {
                started = cvar.wait(started).unwrap();
            }

            workers.push(handle);
        }
    }

    fn execute<F>(&self, job: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let worker_count = {
            let workers = self.workers.lock().unwrap();
            workers.len()
        };
        if self.idle_workers.load(Ordering::SeqCst) == 0 {
            self.ensure_workers(worker_count + 1);
        }
        {
            let mut guard = self.state.lock().unwrap();
            guard.tasks.push_back(Box::new(job));
        }
        self.condvar.notify_one();
    }

    fn shutdown(&self) {
        let workers = {
            let mut guard = self.workers.lock().unwrap();
            std::mem::take(&mut *guard)
        };
        if workers.is_empty() {
            return;
        }
        {
            let mut guard = self.state.lock().unwrap();
            guard.shutdown = true;
        }
        self.condvar.notify_all();
        for worker in workers {
            let _ = worker.join();
        }
    }
}

static THREAD_POOL: std::sync::OnceLock<PersistentThreadPool> = std::sync::OnceLock::new();

fn get_thread_pool() -> &'static PersistentThreadPool {
    THREAD_POOL.get_or_init(PersistentThreadPool::new)
}

pub(crate) fn init_thread_pool() {
    let _ = get_thread_pool();
}

pub(crate) fn spawn_thread<F, T>(tag: ThreadTag, f: F) -> SharedJoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let task_state = Arc::new(TaskState::<T>::new());
    let handle = SharedJoinHandle::new(task_state.clone());

    if let Ok(mut guard) = BACKGROUND_THREADS.lock() {
        guard.retain(|t| !t.handle.is_finished());
        guard.push(TrackedThread {
            tag,
            handle: Box::new(handle.clone()),
        });
    }

    log::info!(
        "[Threads] Executing task for tag {:?} on persistent thread pool",
        tag
    );
    get_thread_pool().execute(move || {
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        task_state.set_result(res);
    });

    handle
}

pub(crate) fn join_bash_func_threads() {
    let mut to_join = Vec::new();
    if let Ok(mut guard) = BACKGROUND_THREADS.lock() {
        let mut i = 0;
        while i < guard.len() {
            if guard[i].tag.uses_bash_funcs() {
                let thread = guard.remove(i);
                to_join.push((thread.tag, thread.handle));
            } else {
                i += 1;
            }
        }
    }
    if !to_join.is_empty() {
        log::info!("[Threads] Joining {} bash_func threads...", to_join.len());
        let start = std::time::Instant::now();
        for (tag, handle) in to_join {
            log::info!("[Threads] Joining thread tag {:?}", tag);
            let _ = handle.join();
        }
        log::info!(
            "[Threads] Joined all bash_func threads in {:?}",
            start.elapsed()
        );
    }
}

pub(crate) fn join_all_before_unload() {
    let mut to_join = Vec::new();
    if let Ok(mut guard) = BACKGROUND_THREADS.lock() {
        for thread in guard.drain(..) {
            to_join.push(thread.handle);
        }
    }
    for handle in to_join {
        let _ = handle.join();
    }
    if let Some(pool) = THREAD_POOL.get() {
        pool.shutdown();
    }
}
