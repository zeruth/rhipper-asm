//! The import directory and the delay-load import directory: every function the image takes from
//! another module, with the IAT slot the code calls through.

use anyhow::{Context, Result};
use serde::Serialize;

use crate::headers::Directory;
use crate::image::Image;

/// One imported function.
#[derive(Debug, Clone, Serialize)]
pub struct Import {
    pub dll: String,
    /// The name, or `None` for an import by ordinal; an ordinal of a DLL whose ordinals are fixed
    /// has its name ([`fixed_ordinal`]), the ordinal kept beside it.
    pub name: Option<String>,
    pub hint: u16,
    /// The ordinal of an import by ordinal.
    pub ordinal: Option<u16>,
    /// The virtual address of the IAT slot: `call [slot]` calls the import.
    pub iat_va: u64,
    /// Whether the import comes from the delay-load directory.
    pub delayed: bool,
}

impl Import {
    /// The name as the code sees it: the name, or `dll!#ordinal`.
    pub fn display_name(&self) -> String {
        match (&self.name, self.ordinal) {
            (Some(name), _) => name.clone(),
            (None, Some(ordinal)) => format!("{}!#{ordinal}", self.dll),
            (None, None) => format!("{}!?", self.dll),
        }
    }
}

pub(crate) fn parse(image: &Image) -> Result<Vec<Import>> {
    let mut imports = Vec::new();
    if let Some((va, _)) = image.directory_va(Directory::Import) {
        parse_import_directory(image, va, &mut imports)?;
    }
    if let Some((va, _)) = image.directory_va(Directory::DelayImport) {
        parse_delay_directory(image, va, &mut imports)?;
    }
    Ok(imports)
}

/// The exports of Winsock whose ordinals are fixed: the Winsock 1.1 functions, which `ws2_32.lib`
/// and `wsock32.lib` import by ordinal alone (`NONAME`), as every Windows since has exported them
/// at the same numbers. These are `ws2_32.dll`'s; [`fixed_ordinal`] has the three `wsock32.dll`
/// numbers otherwise. An import of one has a name the prototype tables know, as a kernel import
/// of an Xbox image has ([`crate::xbe::kernel_export`]); the DLL exports each by name as well, so
/// the host resolves the name.
const WINSOCK_ORDINALS: &[(u16, &str)] = &[
    (1, "accept"),
    (2, "bind"),
    (3, "closesocket"),
    (4, "connect"),
    (5, "getpeername"),
    (6, "getsockname"),
    (7, "getsockopt"),
    (8, "htonl"),
    (9, "htons"),
    (10, "ioctlsocket"),
    (11, "inet_addr"),
    (12, "inet_ntoa"),
    (13, "listen"),
    (14, "ntohl"),
    (15, "ntohs"),
    (16, "recv"),
    (17, "recvfrom"),
    (18, "select"),
    (19, "send"),
    (20, "sendto"),
    (21, "setsockopt"),
    (22, "shutdown"),
    (23, "socket"),
    (51, "gethostbyaddr"),
    (52, "gethostbyname"),
    (53, "getprotobyname"),
    (54, "getprotobynumber"),
    (55, "getservbyname"),
    (56, "getservbyport"),
    (57, "gethostname"),
    (101, "WSAAsyncSelect"),
    (102, "WSAAsyncGetHostByAddr"),
    (103, "WSAAsyncGetHostByName"),
    (104, "WSAAsyncGetProtoByNumber"),
    (105, "WSAAsyncGetProtoByName"),
    (106, "WSAAsyncGetServByPort"),
    (107, "WSAAsyncGetServByName"),
    (108, "WSACancelAsyncRequest"),
    (109, "WSASetBlockingHook"),
    (110, "WSAUnhookBlockingHook"),
    (111, "WSAGetLastError"),
    (112, "WSASetLastError"),
    (113, "WSACancelBlockingCall"),
    (114, "WSAIsBlocking"),
    (115, "WSAStartup"),
    (116, "WSACleanup"),
    (151, "__WSAFDIsSet"),
];

/// The name of an import by ordinal of a DLL whose ordinals are fixed ([`WINSOCK_ORDINALS`]);
/// `None` for any other DLL or ordinal, whose name the file does not tell.
pub fn fixed_ordinal(dll: &str, ordinal: u16) -> Option<&'static str> {
    // `wsock32.dll` numbers three of them its own way: `inet_addr`, `inet_ntoa` and `ioctlsocket`
    // are 10, 11 and 12 there, where `ws2_32.dll` has `ioctlsocket` first. Read by the one table,
    // a program's `inet_addr` was called as `ioctlsocket`, and a zero netmask came back as the
    // error that is not zero.
    if dll.eq_ignore_ascii_case("wsock32.dll") {
        match ordinal {
            10 => return Some("inet_addr"),
            11 => return Some("inet_ntoa"),
            12 => return Some("ioctlsocket"),
            _ => {}
        }
    } else if !dll.eq_ignore_ascii_case("ws2_32.dll") {
        return None;
    }
    WINSOCK_ORDINALS.iter().find(|(o, _)| *o == ordinal).map(|(_, name)| *name)
}

/// The ordinal flag of a thunk: the top bit of its width.
fn ordinal_flag(image: &Image) -> u64 {
    if image.is_64() { 1 << 63 } else { 1 << 31 }
}

fn parse_import_directory(image: &Image, directory_va: u64, out: &mut Vec<Import>) -> Result<()> {
    const DESCRIPTOR: u64 = 20;
    let mut va = directory_va;
    loop {
        let original_first_thunk = image.u32(va).with_context(|| format!("import descriptor at {va:#x}"))?;
        let name_rva = image.u32(va + 12)?;
        let first_thunk = image.u32(va + 16)?;
        if original_first_thunk == 0 && name_rva == 0 && first_thunk == 0 {
            return Ok(());
        }
        let dll = image.cstr(image.va(name_rva), 256).with_context(|| format!("import descriptor at {va:#x}: the DLL name"))?.to_owned();
        // The lookup table has the names; a bound image may have overwritten the IAT with addresses.
        let lookup = if original_first_thunk != 0 { original_first_thunk } else { first_thunk };
        read_thunks(image, &dll, image.va(lookup), image.va(first_thunk), false, out)?;
        va += DESCRIPTOR;
    }
}

fn parse_delay_directory(image: &Image, directory_va: u64, out: &mut Vec<Import>) -> Result<()> {
    const DESCRIPTOR: u64 = 32;
    let mut va = directory_va;
    loop {
        let attributes = image.u32(va).with_context(|| format!("delay import descriptor at {va:#x}"))?;
        let name = image.u32(va + 4)?;
        let iat = image.u32(va + 12)?;
        let names = image.u32(va + 16)?;
        if name == 0 && iat == 0 && names == 0 {
            return Ok(());
        }
        // Attribute bit 0 says the fields are RVAs; an older descriptor holds virtual addresses.
        let rva_based = attributes & 1 != 0;
        let to_va = |field: u32| -> u64 { if rva_based { image.va(field) } else { u64::from(field) } };
        let dll = image.cstr(to_va(name), 256).with_context(|| format!("delay import descriptor at {va:#x}: the DLL name"))?.to_owned();
        read_thunks(image, &dll, to_va(names), to_va(iat), true, out)?;
        va += DESCRIPTOR;
    }
}

fn read_thunks(image: &Image, dll: &str, lookup_va: u64, iat_va: u64, delayed: bool, out: &mut Vec<Import>) -> Result<()> {
    let width = u64::from(image.pointer_size());
    let flag = ordinal_flag(image);
    let mut index = 0u64;
    loop {
        let entry_va = lookup_va + index * width;
        let entry = image.pointer(entry_va).with_context(|| format!("import lookup entry of {dll} at {entry_va:#x}"))?;
        if entry == 0 {
            return Ok(());
        }
        let slot = iat_va + index * width;
        if entry & flag != 0 {
            let ordinal = (entry & 0xffff) as u16;
            let name = fixed_ordinal(dll, ordinal).map(str::to_owned);
            out.push(Import { dll: dll.to_owned(), name, hint: 0, ordinal: Some(ordinal), iat_va: slot, delayed });
        } else {
            let hint_name_va = image.va((entry & 0x7fff_ffff) as u32);
            let hint = image.u16(hint_name_va).with_context(|| format!("hint of an import of {dll} at {hint_name_va:#x}"))?;
            let name = image.cstr(hint_name_va + 2, 4096).with_context(|| format!("name of an import of {dll} at {hint_name_va:#x}"))?;
            out.push(Import { dll: dll.to_owned(), name: Some(name.to_owned()), hint, ordinal: None, iat_va: slot, delayed });
        }
        index += 1;
    }
}
