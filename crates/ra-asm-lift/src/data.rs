//! The data sections of an image as globals, for a lift that stands without the image.
//!
//! Each readable, non-code section becomes one global named after it (`sec_rdata`, `sec_data`;
//! `sec_<image>_data` in a second image ported with the program), a packed structure of byte runs
//! and pointer-sized words. A word that a base relocation names holds an address: of a function
//! (then it is that function's address), of a byte of a data section (then it is that global's
//! byte), or of something else (then the raw number). An import slot holds the import's own
//! address, which binds it at link time, or the address of the lifted function or global when a
//! second image exports it. An uninitialized section is zero. An image that keeps its read-only
//! data in its code section ([`Image::data_in_code`]) has that section as a global too, with the
//! words outside its functions: the code's own operands stay bytes.

use std::collections::BTreeMap;

use ra_asm_analysis::Analysis;
use ra_asm_ir::{Const, Global, Linkage, Type};
use ra_asm_pe::{Image, RelocKind};

use crate::{DataSection, Lifted};

/// Adds the data sections of `image` to `lifted` as globals, and the headers as one more: the C
/// runtime's startup reads the image's own PE headers (`__ImageBase`, the section table) to find
/// its writable range, and a program reads them for its own resources. `tag` goes into the
/// globals' names after `sec_`: empty for the program, the image's own for a second image. The
/// functions of `analysis` tell the data of a code section from its code.
pub fn add_sections(image: &Image, analysis: &Analysis<'_>, lifted: &mut Lifted, tag: &str) {
    let headers = u64::from(image.headers.optional.size_of_headers).min(u64::from(image.size_of_image()));
    if headers > 0 {
        let start = image.image_base();
        let name = format!("sec_{tag}headers");
        lifted.data.push(DataSection { global: name.clone(), start, end: start + headers });
        if let Ok(bytes) = image.read(start, headers as usize) {
            lifted.module.globals.push(Global {
                name,
                ty: Type::array(headers, Type::I8),
                init: Some(Const::Bytes(bytes.to_vec())),
                constant: true,
                linkage: Linkage::External,
                align: 16,
                address: Some(start),
            });
        }
    }
    // The sections first, so a pointer from one into another resolves whatever the order.
    let data_in_code = image.data_in_code();
    for section in &image.sections {
        let code = section.is_code() || section.is_executable();
        if (code && !data_in_code) || !section.is_readable() || section.mapped_size() == 0 {
            continue;
        }
        if matches!(section.name.as_str(), ".pdata" | ".reloc" | ".rsrc" | ".debug" | ".xdata") {
            continue; // the unwinder's, the loader's and the resource compiler's: no code reads them
        }
        let start = image.va(section.virtual_address);
        lifted.data.push(DataSection { global: global_name(&section.name, tag), start, end: start + u64::from(section.mapped_size()) });
    }
    let width = u64::from(image.pointer_size());
    // A word of a code section is data when it lies outside the functions and on a pointer
    // boundary, as the compiler lays data out; the operands of the code are not read as data.
    let functions = analysis.function_spans();
    let is_word = |section: &ra_asm_pe::Section, va: u64| {
        !(section.is_code() || section.is_executable()) || (va.is_multiple_of(width) && !functions.contains(va))
    };

    for section in &image.sections {
        let Some(data) = lifted.data.iter().find(|d| d.start == image.va(section.virtual_address)).cloned() else { continue };
        let size = data.end - data.start;
        let constant = !section.is_writable();
        if section.is_uninitialized() {
            lifted.module.globals.push(Global {
                name: data.global.clone(),
                ty: Type::array(size, Type::I8),
                init: Some(Const::Zero),
                constant,
                linkage: Linkage::External,
                align: 16,
                address: Some(data.start),
            });
            continue;
        }
        let Ok(bytes) = image.read_to_section_end(data.start) else { continue };

        // The words that hold addresses: the relocated ones, and the import slots. In an image
        // whose relocations were stripped, an aligned word that names a function's entry or a
        // byte of a data section is taken for an address (a number that happens to fall in the
        // image's range would be too: such an image gives no better evidence).
        let mut words: BTreeMap<u64, Const> = BTreeMap::new();
        if image.relocations.is_empty() {
            for (offset, chunk) in bytes[..size as usize].chunks_exact(width as usize).enumerate() {
                let value = if width == 8 {
                    u64::from_le_bytes(chunk.try_into().unwrap())
                } else {
                    u64::from(u32::from_le_bytes(chunk.try_into().unwrap()))
                };
                let named = lifted.functions.contains_key(&value) || (image.section_of(value).is_some_and(|s| !s.is_code()));
                if !named || !is_word(section, data.start + offset as u64 * width) {
                    continue;
                }
                if let Some(ra_asm_ir::Operand::Address { global, offset: o }) = lifted.address_of(value) {
                    words.insert(offset as u64 * width, Const::Address { global, offset: o });
                }
            }
        }
        for r in &image.relocations {
            if r.va < data.start || r.va + width > data.end {
                continue;
            }
            let is_pointer = matches!((r.kind, width), (RelocKind::Dir64, 8) | (RelocKind::HighLow, 4));
            if !is_pointer || !is_word(section, r.va) {
                continue;
            }
            let Ok(value) = image.pointer(r.va) else { continue };
            let constant = match lifted.address_of(value) {
                Some(ra_asm_ir::Operand::Address { global, offset }) => Const::Address { global, offset },
                _ => Const::Int(i128::from(value)),
            };
            words.insert(r.va - data.start, constant);
        }
        for import in &image.imports {
            if import.iat_va >= data.start && import.iat_va + width <= data.end {
                // The slot of an import a second image exports holds the lifted function or the
                // global there, as the loader would have filled it.
                if let Some(ra_asm_ir::Operand::Address { global, offset }) =
                    lifted.bound.get(&import.display_name()).and_then(|va| lifted.address_of(*va))
                {
                    words.insert(import.iat_va - data.start, Const::Address { global, offset });
                    continue;
                }
                let mut symbol = crate::x86::import_symbol(&import.display_name());
                // The slot of an import the runtime shims for a 32-bit program names the shim, as
                // a call through it would.
                if lifted.bits == 32
                    && let Some(shim) = crate::raise::prototypes::x86_shim(&symbol)
                {
                    shim.clone_into(&mut symbol);
                }
                lifted.imports.insert(symbol.clone());
                words.insert(import.iat_va - data.start, Const::Address { global: symbol, offset: 0 });
            }
        }

        // The structure: byte runs between the words.
        let mut fields: Vec<Type> = Vec::new();
        let mut items: Vec<Const> = Vec::new();
        let mut at = 0u64;
        let word_ty = Type::Int((width * 8) as u16);
        for (&offset, constant) in &words {
            if offset < at {
                continue; // overlapping words: the first wins
            }
            if offset > at {
                let run = &bytes[at as usize..offset as usize];
                fields.push(Type::array(run.len() as u64, Type::I8));
                items.push(Const::Bytes(run.to_vec()));
            }
            fields.push(word_ty.clone());
            items.push(constant.clone());
            at = offset + width;
        }
        if at < size {
            let run = &bytes[at as usize..size as usize];
            fields.push(Type::array(run.len() as u64, Type::I8));
            items.push(Const::Bytes(run.to_vec()));
        }
        let (ty, init) = if fields.len() == 1 && words.is_empty() {
            (Type::array(size, Type::I8), Const::Bytes(bytes[..size as usize].to_vec()))
        } else {
            (Type::Packed(fields), Const::Struct(items))
        };
        lifted.module.globals.push(Global {
            name: data.global.clone(),
            ty,
            init: Some(init),
            constant,
            linkage: Linkage::External,
            align: 16,
            address: Some(data.start),
        });
    }
}

/// `sec_rdata` for `.rdata`; `sec_<tag>rdata` with a tag.
fn global_name(section: &str, tag: &str) -> String {
    let body: String = section.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    format!("sec_{tag}{}", body.trim_start_matches('_'))
}
