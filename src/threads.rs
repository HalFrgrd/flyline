#![allow(clippy::disallowed_methods, dead_code, unused)]

use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

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

pub(crate) struct SharedJoinHandle<T> {
    inner: Arc<Mutex<Option<JoinHandle<T>>>>,
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
    pub(crate) fn new(handle: JoinHandle<T>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Some(handle))),
        }
    }

    pub(crate) fn join_value(
        &self,
    ) -> Option<Result<T, std::boxed::Box<dyn std::any::Any + Send>>> {
        let handle = if let Ok(mut guard) = self.inner.lock() {
            guard.take()
        } else {
            None
        };
        handle.map(|h| h.join())
    }

    pub(crate) fn is_finished(&self) -> bool {
        if let Ok(guard) = self.inner.lock() {
            guard.as_ref().map(|h| h.is_finished()).unwrap_or(true)
        } else {
            true
        }
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

pub(crate) fn register_thread<T: Send + 'static>(
    tag: ThreadTag,
    handle: JoinHandle<T>,
) -> SharedJoinHandle<T> {
    let shared = SharedJoinHandle::new(handle);
    if let Ok(mut guard) = BACKGROUND_THREADS.lock() {
        // Clean up finished threads
        guard.retain(|t| !t.handle.is_finished());
        guard.push(TrackedThread {
            tag,
            handle: Box::new(shared.clone()),
        });
    }
    shared
}

pub(crate) fn spawn_thread<F, T>(tag: ThreadTag, f: F) -> SharedJoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let name = tag.thread_name().to_string();
    let builder = std::thread::Builder::new().name(name);
    let handle = builder.spawn(f).expect("Failed to spawn thread");
    register_thread(tag, handle)
}

pub(crate) fn join_bash_func_threads() {
    let mut to_join = Vec::new();
    if let Ok(mut guard) = BACKGROUND_THREADS.lock() {
        let mut i = 0;
        while i < guard.len() {
            if guard[i].tag.uses_bash_funcs() {
                let thread = guard.remove(i);
                to_join.push(thread.handle);
            } else {
                i += 1;
            }
        }
    }
    for handle in to_join {
        let _ = handle.join();
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
}
