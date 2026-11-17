//! The PE image. [`Image::parse`] reads the headers and the section table, lays the file out by
//! RVA as the loader would, and reads the directories the pipeline needs: imports, exports, base
//! relocations, the exception table, TLS, debug and resources. A malformed directory does not fail the load:
//! the image keeps a warning for it, because a packed or odd binary is still worth opening.
//! [`object`] reads the relocatable objects (COFF, ELF) and the archives the signatures of a
//! library are made from. [`xbe`] reads the original Xbox's container into the same image, and
//! [`elf`] the executables and shared objects of Linux, the PSP's PRX modules and the `PlayStation 2`
//! IOP's IRX modules, and [`macho`] the 64-bit executables and
//! libraries of macOS, thin or fat; [`Image`] maps the PSX's `PS-X EXE` too. [`toolset`] fingerprints the compiler,
//! linker and C runtime that built an image.

pub mod debug;
pub mod elf;
pub mod exception;
pub mod exports;
pub mod headers;
pub mod image;
pub mod imports;
mod iop_names;
pub mod linkmap;
pub mod macho;
pub mod object;
pub mod pdb;
mod psp_nids;
pub mod relocs;
pub mod resources;
pub mod section;
pub mod tls;
pub mod toolset;
pub mod xbe;

pub use debug::{CodeView, DebugEntry};
pub use exception::{
    Arm64Epilog, Arm64Unwind, Arm64UnwindOp, CatchHandler, CxxFrame, FuncInfo, RuntimeFunction, ThrowInfo, TryBlock, UnwindInfo, UnwindOp,
};
pub use exports::{Export, ExportTable};
pub use headers::{DataDirectory, Directory, Headers, Machine, OptionalMagic, RichEntry, RichHeader, RichTool};
pub use image::Image;
pub use imports::Import;
pub use object::{Library, Object, ObjectFunction, ObjectReloc, ObjectSection};
pub use pdb::{DataSymbol, LibraryRange, Symbol};
pub use relocs::{RelocKind, Relocation};
pub use resources::{Resource, ResourceId};
pub use section::Section;
pub use tls::Tls;
pub use toolset::{CxxAbi, Toolset};
pub use xbe::{KernelExport, Xbe};

use std::path::Path;

use anyhow::Context;

/// Reads and parses the PE file at `path`, and the names of its functions from the PDB beside it
/// when there is one, else from the link map beside it (`<exe>.map`) when there is one. A file
/// named `1ST_READ.BIN`, which no header marks, is a Dreamcast boot program in its plain form.
pub fn load(path: &Path) -> anyhow::Result<Image> {
    let bytes = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    let boot = path.file_name().is_some_and(|name| name.eq_ignore_ascii_case(ra_asm_format::dreamcast::FILE_NAME));
    let mut image = if boot {
        Image::dreamcast(bytes, false).with_context(|| format!("{} is not a Dreamcast boot program", path.display()))?
    } else {
        Image::parse(bytes).with_context(|| format!("{} is not a PE image this loader reads", path.display()))?
    };
    image.path = Some(path.to_path_buf());
    // An ELF or a Mach-O file names its functions in its own symbol table; a PS-X EXE has no PDB,
    // and the map beside one is `nm`'s over its ELF, not a link map; a boot program and a cartridge
    // ROM have neither.
    if image.elf.is_some() || image.macho.is_some() || image.psx.is_some() || image.dreamcast.is_some() || image.n64.is_some() {
        return Ok(image);
    }
    match debug::load_pdb(&image) {
        Ok(pdb) => {
            image.symbols = pdb.symbols;
            image.libraries = pdb.libraries;
            image.data_symbols = pdb.data;
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
