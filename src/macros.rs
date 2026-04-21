/// Times an expression and logs the duration at `TRACE` level.
///
/// Similar to `dbg!` but measures elapsed time instead of printing the value.
///
/// # Forms
///
/// ```rust,ignore
/// // Auto-stringifies the expression as the label:
/// let result = time_it!(some_function());
/// // → TRACE "some_function() took 1.23ms"
///
/// // Explicit label (useful for blocks or when the auto-label would be noisy):
/// let result = time_it!("descriptive label", some_function());
/// // → TRACE "descriptive label took 1.23ms"
/// ```
macro_rules! time_it {
    ($label:literal, $expr:expr) => {{
        let __start = std::time::Instant::now();
        let __result = $expr;
        log::trace!("{} took {:?}", $label, __start.elapsed());
        __result
    }};
    ($expr:expr) => {{
        let __start = std::time::Instant::now();
        let __result = $expr;
        log::trace!("{} took {:?}", stringify!($expr), __start.elapsed());
        __result
    }};
}
