#![allow(dead_code)]

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

/// Register a SIGINT (Ctrl+C) handler and return the interrupted flag.
///
/// When the user presses Ctrl+C for the first time, the flag is set to `true`.
/// dcx then completes any in-progress cleanup (rollback / unmount) before exiting.
///
/// A second Ctrl+C terminates the process immediately with exit code 130.
///
/// The returned `Arc` must be kept alive for the duration of the program — dropping
/// it does not unregister the handler, but keeping it alive lets callers read the flag.
pub fn interrupted_flag() -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(false));

    // Registered first: on signal, if the flag is already true (second Ctrl+C) → exit.
    let _ = signal_hook::flag::register_conditional_shutdown(
        signal_hook::consts::SIGINT,
        130,
        Arc::clone(&flag),
    );

    // Registered second: on signal, set the flag to true (first Ctrl+C).
    let _ = signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&flag));

    flag
}
