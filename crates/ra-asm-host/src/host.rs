//! The interpreter's run of a lifted program with the residual engine behind it: where the run
//! reaches code of the image that no lifted function has (an address the program computed, code
//! the analysis never found), the engine decodes the function there out of the run's own memory,
//! lifts it and runs it over the caller's state, and the run goes on. The facts of the run name
//! each entry the engine ran under `residual`.
//!
//! Code on a page the run wrote is not the engine's when the run asked for [`Options::unpack`]:
//! the engine's image holds what the file held there, not what the program wrote, and the run
//! stops at the address instead.
//!
//! What the engine runs here is a function that calls nothing: a call it makes (to an address or
//! to an import) stops the run with the call named, since its callee would run in the engine's
//! copy of the memory and not in the run's.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, anyhow};
use ra_asm_ir::interp::{Memory, Stop};
use ra_asm_lift::Lifted;
use ra_asm_lift::raise::abi::Signature;
use ra_asm_lift::run::{Options, Outcome, Residual, Unlifted};
use ra_asm_pe::Image;
use ra_asm_residual::{Call, Calls, Engine, Machine, Reader, State};

/// Runs `lifted` as [`ra_asm_lift::run::run`] does, with the residual engine answering its calls
/// to code it does not have. The engine's image is `image`'s file loaded again: the engine keeps
/// one of its own, whose sections it rewrites from the run's memory.
pub fn run(image: &Image, lifted: &Lifted, signatures: &HashMap<String, Signature>, options: &Options) -> Result<Outcome> {
    let path = image.path.as_ref().ok_or_else(|| anyhow!("the image has no file for the residual engine to load"))?;
    let own = ra_asm_pe::load(path).with_context(|| format!("cannot load {} for the residual engine", path.display()))?;
    let options = Options { residual: Some(residual(own)), ..options.clone() };
    ra_asm_lift::run::run(image, lifted, signatures, &options)
}

/// The residual engine of the program `image` is the image of, as the option of a run.
pub fn residual(image: Image) -> Residual {
    Residual(Arc::new(Mutex::new(Fallback { engine: Engine::new(image) })))
}

/// The engine behind a run's calls to code its lift does not have.
struct Fallback {
    engine: Engine,
}

impl Unlifted for Fallback {
    fn call(&mut self, memory: &mut Memory, state: u64, target: u64) -> Result<(), Stop> {
        let mut entry = State::new(self.engine.isa());
        let len = entry.bytes().len() as u64;
        entry.bytes_mut().copy_from_slice(&memory.read_vec(state, len)?);
        let outcome = {
            let live = Live(RefCell::new(&mut *memory));
            self.engine.run(&live, &entry, target, &mut Leaf)
        }
        .map_err(|e| Stop::fault(format!("the residual engine cannot run {target:#x}: {e:#}")))?;
        memory.write(state, outcome.state.bytes())?;
        for (address, bytes) in &outcome.written {
            memory.write(*address, bytes)?;
        }
        outcome.stop.map_or(Ok(()), Err)
    }
}

/// The run's memory as the engine reads it. The engine takes a reader by shared reference, and
/// a read of the run's memory may map a page the host's probe finds.
struct Live<'a>(RefCell<&'a mut Memory>);

impl Reader for Live<'_> {
    fn read(&self, address: u64, buf: &mut [u8]) -> bool {
        self.0.borrow_mut().read(address, buf).is_ok()
    }
}

/// The calls of a function the engine runs, which this host does not answer.
struct Leaf;

impl Calls for Leaf {
    fn call(&mut self, call: Call<'_>, _: &mut Machine<'_, '_>) -> Result<u64, Stop> {
        let what = match call {
            Call::Address(address) => format!("a call of {address:#x}"),
            Call::Import { name, .. } => format!("a call of the import {name}"),
        };
        Err(Stop::fault(format!("{what} from code the residual engine runs, which the host does not answer")))
    }
}
