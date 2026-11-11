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
pub mod linkmap;
pub mod pdb;
pub mod relocs;
pub mod section;
pub mod tls;

pub use debug::{CodeView, DebugEntry};
pub use exception::{RuntimeFunction, UnwindInfo};
pub use exports::{Export, ExportTable};
pub use headers::{DataDirectory, Directory, Headers, Machine, OptionalMagic};
pub use image::Image;
pub use imports::Import;
pub use pdb::{LibraryRange, Symbol};
pub use relocs::{RelocKind, Relocation};
pub use section::Section;
pub use tls::Tls;

use std::path::Path;

use anyhow::Context;

/// Reads and parses the PE file at `path`, and the names of its functions from the PDB beside it
/// when there is one, else from the link map beside it (`<exe>.map`) when there is one.
pub fn load(path: &Path) -> anyhow::Result<Image> {
    let bytes = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    let mut image = Image::parse(bytes).with_context(|| format!("{} is not a PE image this loader reads", path.display()))?;
    image.path = Some(path.to_path_buf());
    match debug::load_pdb(&image) {
        Ok(pdb) => {
            image.symbols = pdb.symbols;
            image.libraries = pdb.libraries;
        }
        Err(e) => image.warnings.push(format!("symbols: {e:#}")),
    }
    if image.symbols.is_empty()
        && let Some(map) = linkmap::map_beside(&image)
    {
        match linkmap::read(&image, &map) {
            Ok(symbols) => image.symbols = symbols,
            Err(e) => image.warnings.push(format!("symbols: {e:#}")),
        }
    }
    Ok(image)
}

/// [`load`], with the names of the functions from the link map at `map`, which the person named:
/// a map that disagrees with the image fails the load.
pub fn load_with_map(path: &Path, map: &Path) -> anyhow::Result<Image> {
    let mut image = load(path)?;
    image.symbols = linkmap::read(&image, map)?;
    Ok(image)
}
