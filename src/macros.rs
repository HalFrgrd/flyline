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
    ($label:expr, $expr:expr) => {{
        let _timer = $crate::perf::PerfTimer::start_and_log_on_drop($label);
        $expr
    }};
}

/// Print a `flyline …` user-facing error message to stderr and return the
/// `Usage` exit code from the enclosing function. Equivalent to writing
/// `eprintln!(…); return bash_symbols::BuiltinExitCode::Usage as c_int;` but
/// keeps the CLI subcommand handlers in `Flyline::call` readable.
macro_rules! return_usage_error {
    ($($arg:tt)*) => {{
        eprintln!($($arg)*);
        return $crate::bash_symbols::BuiltinExitCode::Usage as ::libc::c_int;
    }};
}
