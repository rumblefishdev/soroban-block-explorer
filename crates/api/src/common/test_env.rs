//! Test-only helper for mutating process env vars without racing other
//! parallel tests in the same crate binary.
//!
//! Cargo runs every `#[test]` in a crate as one binary with a shared
//! thread pool. `std::env::{set_var, remove_var}` mutates a global,
//! so any two tests that touch the same key must serialise. Using
//! `ENV_LOCK` from this module — rather than a per-file mutex — keeps
//! that serialisation crate-wide, which is the only correct scope.

#![cfg(test)]

use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Set `key` to `value` (or unset it when `value` is `None`) for the
/// duration of `f`, then restore the prior value. Panics in `f` still
/// unwind through the restore via the `Guard` drop impl.
pub fn with_env<R>(key: &str, value: Option<&str>, f: impl FnOnce() -> R) -> R {
    // Poisoning is benign here — the lock only guards the env mutation
    // ordering, not invariants on shared state.
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _restore = Guard::new(key, std::env::var(key).ok());
    unsafe {
        match value {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }
    f()
}

struct Guard<'a> {
    key: &'a str,
    prev: Option<String>,
}

impl<'a> Guard<'a> {
    fn new(key: &'a str, prev: Option<String>) -> Self {
        Self { key, prev }
    }
}

impl Drop for Guard<'_> {
    fn drop(&mut self) {
        unsafe {
            match &self.prev {
                Some(v) => std::env::set_var(self.key, v),
                None => std::env::remove_var(self.key),
            }
        }
    }
}
