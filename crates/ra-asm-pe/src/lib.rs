//! The PE image. [`Image::parse`] reads the headers and the section table, lays the file out by
//! RVA as the loader would, and reads the directories the pipeline needs: imports, exports, base
//! relocations, the exception table, TLS and debug. A malformed directory does not fail the load:
//! the image keeps a warning for it, because a packed or odd binary is still worth opening.

pub mod debug;
pub mod exception;
pub mod exports;
pub mod headers;
pub mod image;
pub mod imports;
pub mod relocs;
pub mod section;
pub mod tls;

pub use debug::{CodeView, DebugEntry};
pub use exception::{RuntimeFunction, UnwindInfo};
pub use exports::{Export, ExportTable};
pub use headers::{DataDirectory, Directory, Headers, Machine, OptionalMagic};
pub use image::Image;
pub use imports::Import;
pub use relocs::{RelocKind, Relocation};
pub use section::Section;
pub use tls::Tls;

use std::path::Path;

use anyhow::Context;

/// Reads and parses the PE file at `path`.
pub fn load(path: &Path) -> anyhow::Result<Image> {
    let bytes = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    let mut image = Image::parse(bytes).with_context(|| format!("{} is not a PE image this loader reads", path.display()))?;
    image.path = Some(path.to_path_buf());
    Ok(image)
}
