use std::sync::atomic::{AtomicBool, Ordering};

static TUTORIAL_MODE: AtomicBool = AtomicBool::new(false);

pub fn set_tutorial_mode(enabled: bool) {
    TUTORIAL_MODE.store(enabled, Ordering::Relaxed);
}

pub fn tutorial_mode() -> bool {
    TUTORIAL_MODE.load(Ordering::Relaxed)
}
