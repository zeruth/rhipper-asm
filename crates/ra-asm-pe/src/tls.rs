//! The TLS directory: the template of each thread's static storage, and the callbacks that run
//! before the entry point. The callbacks are code the analysis must find.

use anyhow::{Context, Result};
use serde::Serialize;

use crate::headers::Directory;
use crate::image::Image;

/// The TLS directory of an image.
#[derive(Debug, Clone, Serialize)]
pub struct Tls {
    /// The template: the initialized bytes each thread starts with.
    pub start_va: u64,
    pub end_va: u64,
    /// The variable the loader writes the TLS index to.
    pub index_va: u64,
    /// The array of callback pointers, NUL-terminated.
    pub callbacks_va: u64,
    pub zero_fill_size: u32,
    pub characteristics: u32,
    /// The callbacks, in order.
    pub callbacks: Vec<u64>,
}

pub(crate) fn parse(image: &Image) -> Result<Option<Tls>> {
    let Some((va, _)) = image.directory_va(Directory::Tls) else { return Ok(None) };
    let width = u64::from(image.pointer_size());
    let field = |i: u64| image.pointer(va + i * width).with_context(|| format!("TLS directory field {i} at {va:#x}"));
    let start_va = field(0)?;
    let end_va = field(1)?;
    let index_va = field(2)?;
    let callbacks_va = field(3)?;
    let zero_fill_size = image.u32(va + 4 * width)?;
    let characteristics = image.u32(va + 4 * width + 4)?;
    let mut callbacks = Vec::new();
    if callbacks_va != 0 && image.is_mapped(callbacks_va) {
        let mut slot = callbacks_va;
        loop {
            let callback = image.pointer(slot).with_context(|| format!("TLS callback slot at {slot:#x}"))?;
            if callback == 0 {
                break;
            }
            callbacks.push(callback);
            slot += width;
        }
    }
    Ok(Some(Tls { start_va, end_va, index_va, callbacks_va, zero_fill_size, characteristics, callbacks }))
}
