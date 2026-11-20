//! Compiled functions in the interpreter's run. A port's functions are this machine's own code,
//! `void f(ptr state)` each, made by a compiler from the same IR the interpreter would run. The
//! run calls one where it would have interpreted it, and the code calls back for what it does
//! not hold itself: the host's functions, the memory that is no plain RAM, the look at the
//! machine between two blocks. So a run is the same run whichever of its functions are compiled.
//!
//! The code is entered with the interpreter and its host kept for the calls back, which find
//! them again by [`with`]. A call back that stops the run (a fault, an exit, the leave of an
//! exception's handler for another frame) cannot return into the code: it unwinds through the
//! code's frames to the call that entered them, which gives the stop to its caller as the
//! interpreter gives its own.

use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use super::{Host, Stop, Vm};

/// A compiled function: `void f(ptr state)`.
pub type Code = unsafe extern "C-unwind" fn(*mut u8);

type Context = (*mut Vm<'static>, *mut (dyn Host + 'static));

thread_local! {
    /// The interpreter and the host of the compiled code this thread is in, innermost.
    static CONTEXT: Cell<Option<Context>> = const { Cell::new(None) };
}

/// Runs `code` on the state at `state`, with `vm` and `host` kept for its calls back.
pub(super) fn call(vm: &mut Vm<'_>, host: &mut dyn Host, code: Code, state: u64) -> Result<(), Stop> {
    // SAFETY: the pointers are read back only by `with`, on this thread, while this call is on
    // the stack below it, where `vm` and `host` are borrowed for the whole call and not used.
    let context: Context = unsafe { (std::ptr::from_mut(vm).cast(), std::mem::transmute::<*mut dyn Host, *mut (dyn Host + 'static)>(host)) };
    let outer = CONTEXT.replace(Some(context));
    // SAFETY: `code` is a compiled function of the module's, which takes its machine's state.
    let result = catch_unwind(AssertUnwindSafe(|| unsafe { code(state as usize as *mut u8) }));
    CONTEXT.set(outer);
    match result {
        Ok(()) => Ok(()),
        Err(payload) => match payload.downcast::<Stop>() {
            Ok(stop) => Err(*stop),
            Err(other) => resume_unwind(other),
        },
    }
}

/// A call back from compiled code: `f` on the interpreter and the host the code was entered
/// with. A stop does not return: it unwinds to the call that entered the code.
///
/// # Panics
///
/// When no compiled code is on this thread's stack, and with the stop itself as the payload
/// when `f` stops the run.
pub fn with<R>(f: impl FnOnce(&mut Vm<'_>, &mut dyn Host) -> Result<R, Stop>) -> R {
    let (vm, host) = CONTEXT.get().expect("a call back from compiled code outside a run");
    // SAFETY: `call` put the pointers there and is on the stack below; the borrows it took are
    // not used until the code returns to it. A nested entry of compiled code replaces the
    // context and restores it.
    let result = unsafe { f(&mut *vm.cast::<Vm<'_>>(), &mut *host) };
    match result {
        Ok(value) => value,
        Err(stop) => resume_unwind(Box::new(stop)),
    }
}
