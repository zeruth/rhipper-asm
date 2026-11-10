//! The PE image: DOS header, COFF header, optional header, sections, imports, exports and
//! relocations. The loader maps a file to an image and hands sections to the decoder.

pub mod headers;
