//! The XBE container of the original Xbox: a PE the XDK's image builder rewrote, with a header of
//! its own in place of the DOS, COFF and optional headers. The entry point and the address of the
//! kernel thunk table are XOR-encoded by the keys of the retail, the debug or the Chihiro kernel;
//! the sections carry their own flags; the certificate names the title; the library versions name
//! the SDK the program was linked with; and the TLS directory has the PE's own layout. The loader
//! reads all of it into the same [`Image`] a PE makes: x86, 32 bits, mapped at its base, with the
//! kernel thunk table as the import table of `xboxkrnl.exe`, whose exports are by ordinal alone
//! and named here from the kernel's export table.

use anyhow::{Context, Result, bail, ensure};
use ra_asm_format::Reader;
use serde::Serialize;

use crate::headers::{CoffHeader, DataDirectory, Directory, Headers, Machine, OptionalHeader, OptionalMagic};
use crate::image::Image;
use crate::imports::Import;
use crate::section::Section;

/// `XBEH`, little-endian.
pub const MAGIC: u32 = 0x4845_4258;
/// The module the kernel thunk table imports from, as the code and the port name it.
pub const KERNEL_DLL: &str = "xboxkrnl.exe";

const SECTION_HEADER_SIZE: usize = 0x38;
const LIBRARY_VERSION_SIZE: usize = 0x10;
const SUBSYSTEM_XBOX: u16 = 14;

/// The kernel an image was built for, which decides the keys its header is encoded with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kernel {
    Retail,
    Debug,
    /// The arcade board's kernel.
    Chihiro,
}

impl Kernel {
    const ALL: [Self; 3] = [Self::Retail, Self::Debug, Self::Chihiro];

    fn entry_key(self) -> u32 {
        match self {
            Self::Retail => 0xA8FC_57AB,
            Self::Debug => 0x9485_9D4B,
            Self::Chihiro => 0x40B5_C16E,
        }
    }

    fn thunk_key(self) -> u32 {
        match self {
            Self::Retail => 0x5B6D_40B6,
            Self::Debug => 0xEFB1_F152,
            Self::Chihiro => 0x2290_059D,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Retail => "retail",
            Self::Debug => "debug",
            Self::Chihiro => "chihiro",
        }
    }
}

/// One library of the SDK the image was linked with, from the header's version table.
#[derive(Debug, Clone, Serialize)]
pub struct LibraryVersion {
    pub name: String,
    pub major: u16,
    pub minor: u16,
    pub build: u16,
    pub flags: u16,
}

impl LibraryVersion {
    /// `XAPILIB 1.0.5849`, as the SDK numbers itself.
    pub fn display(&self) -> String {
        format!("{} {}.{}.{}", self.name, self.major, self.minor, self.build)
    }
}

/// What the XBE header holds beyond what the PE headers carry.
#[derive(Debug, Clone, Serialize)]
pub struct Xbe {
    /// The kernel whose keys decoded the entry point.
    pub kernel: Kernel,
    pub entry_va: u64,
    /// The first slot of the kernel thunk table, which the import table was read from.
    pub kernel_thunk_va: u64,
    /// The TLS directory, or zero when the header names none.
    pub tls_va: u64,
    pub init_flags: u32,
    pub timestamp: u32,
    pub title_id: u32,
    pub title_name: String,
    pub region: u32,
    pub version: u32,
    pub libraries: Vec<LibraryVersion>,
    /// The path of the PE the image was built from, as the builder recorded it.
    pub debug_path: Option<String>,
}

/// The section flags of the XBE header.
const FLAG_WRITABLE: u32 = 0x1;
const FLAG_EXECUTABLE: u32 = 0x4;

/// Whether `reader` opens on an XBE.
pub(crate) fn is_xbe(reader: Reader<'_>) -> bool {
    reader.u32(0, "magic").is_ok_and(|m| m == MAGIC)
}

/// The headers of an XBE as the PE headers they stand for, its sections, and what is its own.
pub(crate) fn parse(reader: Reader<'_>) -> Result<(Headers, Vec<Section>, Xbe)> {
    let field = |offset: usize, what: &str| reader.u32(offset, &format!("the XBE header's {what}"));
    let base = field(0x104, "base address")?;
    let size_of_headers = field(0x108, "size of headers")?;
    let size_of_image = field(0x10c, "size of image")?;
    let timestamp = field(0x114, "time stamp")?;
    let certificate = field(0x118, "certificate address")?;
    let section_count = field(0x11c, "section count")?;
    let section_headers = field(0x120, "section headers address")?;
    let init_flags = field(0x124, "initialization flags")?;
    let entry_encoded = field(0x128, "entry point")?;
    let tls = field(0x12c, "TLS address")?;
    let stack_commit = field(0x130, "stack commit")?;
    let heap_reserve = field(0x134, "heap reserve")?;
    let heap_commit = field(0x138, "heap commit")?;
    let checksum = field(0x144, "PE checksum")?;
    let debug_path = field(0x14c, "debug path address")?;
    let thunk_encoded = field(0x158, "kernel thunk address")?;
    let library_count = field(0x160, "library version count")?;
    let libraries = field(0x164, "library versions address")?;
    ensure!(base != 0, "the XBE header's base address at offset 0x104 is zero");
    ensure!(size_of_headers as usize <= reader.len(), "the XBE header's size of headers {size_of_headers:#x} is more than the file holds");

    // Every address in the header is a virtual address; the header itself is mapped at the base.
    let in_header = |va: u32, what: &str| -> Result<usize> {
        match va.checked_sub(base) {
            Some(offset) if offset < size_of_headers => Ok(offset as usize),
            _ => bail!("the XBE header's {what} {va:#x} lies outside the headers (base {base:#x}, {size_of_headers:#x} bytes)"),
        }
    };
    let in_image = |va: u32| va >= base && u64::from(va) < u64::from(base) + u64::from(size_of_image);
    let Some(kernel) = Kernel::ALL.into_iter().find(|k| in_image(entry_encoded ^ k.entry_key())) else {
        bail!("the entry point at offset 0x128 ({entry_encoded:#x}) decodes into the image under no known kernel's key");
    };
    let entry_va = entry_encoded ^ kernel.entry_key();
    let thunk_kernel = std::iter::once(kernel).chain(Kernel::ALL).find(|k| in_image(thunk_encoded ^ k.thunk_key()));
    let Some(thunk_kernel) = thunk_kernel else {
        bail!("the kernel thunk address at offset 0x158 ({thunk_encoded:#x}) decodes into the image under no known kernel's key");
    };
    let kernel_thunk_va = thunk_encoded ^ thunk_kernel.thunk_key();

    let section_table_offset = in_header(section_headers, "section headers address")?;
    ensure!(section_count <= 4096, "the XBE header names {section_count} sections");
    let mut sections = Vec::with_capacity(section_count as usize);
    let mut c = reader.at(section_table_offset);
    for index in 0..section_count as usize {
        let what = |field: &str| format!("XBE section {index} {field}");
        let flags = c.u32(&what("flags"))?;
        let va = c.u32(&what("virtual address"))?;
        let virtual_size = c.u32(&what("virtual size"))?;
        let raw_offset = c.u32(&what("raw address"))?;
        let raw_size = c.u32(&what("raw size"))?;
        let name_va = c.u32(&what("name address"))?;
        c.skip(SECTION_HEADER_SIZE - 6 * 4);
        let Some(virtual_address) = va.checked_sub(base) else {
            bail!("{} {va:#x} lies below the base {base:#x}", what("virtual address"))
        };
        let name = reader.cstr(in_header(name_va, &what("name address"))?, 64, &what("name"))?.to_owned();
        let mut characteristics = Section::MEM_READ;
        if flags & FLAG_WRITABLE != 0 {
            characteristics |= Section::MEM_WRITE;
        }
        if flags & FLAG_EXECUTABLE != 0 {
            characteristics |= Section::MEM_EXECUTE | Section::CNT_CODE;
        } else if raw_size == 0 {
            characteristics |= Section::CNT_UNINITIALIZED_DATA;
        } else {
            characteristics |= Section::CNT_INITIALIZED_DATA;
        }
        sections.push(Section { name, virtual_size, virtual_address, raw_size, raw_offset, characteristics });
    }

    let (title_id, title_name, region, version) = match in_header(certificate, "certificate address") {
        Ok(at) => {
            let title_id = reader.u32(at + 0x08, "the certificate's title id")?;
            let name_units = reader.slice(at + 0x0c, 80, "the certificate's title name")?;
            let units: Vec<u16> = name_units.chunks_exact(2).map(|u| u16::from_le_bytes([u[0], u[1]])).take_while(|&u| u != 0).collect();
            let region = reader.u32(at + 0xa0, "the certificate's region")?;
            let version = reader.u32(at + 0xac, "the certificate's version")?;
            (title_id, String::from_utf16_lossy(&units), region, version)
        }
        Err(_) if certificate == 0 => (0, String::new(), 0, 0),
        Err(e) => return Err(e),
    };
    let mut versions = Vec::new();
    if library_count != 0 && libraries != 0 {
        ensure!(library_count <= 256, "the XBE header names {library_count} library versions");
        let mut c = reader.at(in_header(libraries, "library versions address")?);
        for i in 0..library_count {
            let what = format!("library version {i}");
            let name = c.fixed_str(8, &what)?;
            let (major, minor, build, flags) = (c.u16(&what)?, c.u16(&what)?, c.u16(&what)?, c.u16(&what)?);
            versions.push(LibraryVersion { name, major, minor, build, flags });
        }
        debug_assert_eq!(c.position() - in_header(libraries, "")?, library_count as usize * LIBRARY_VERSION_SIZE);
    }
    let debug_path = (debug_path != 0)
        .then(|| in_header(debug_path, "debug path address").and_then(|at| reader.cstr(at, 260, "the debug path")).ok().map(str::to_owned))
        .flatten();

    // The sizes are the header's words: a sum past 4 GiB stays at the top, as a PE's field would.
    let total = |filter: fn(&&Section) -> bool| sections.iter().filter(filter).map(Section::mapped_size).fold(0u32, u32::saturating_add);
    let size_of_code = total(|s| s.is_code());
    let base_of_code = sections.iter().filter(|s| s.is_code()).map(|s| s.virtual_address).min().unwrap_or(0);
    let size_of_initialized_data = total(|s| !s.is_code() && !s.is_uninitialized());
    let size_of_uninitialized_data = total(|s| s.is_uninitialized());
    let mut directories = vec![DataDirectory::default(); 16];
    if tls != 0 {
        let Some(rva) = tls.checked_sub(base) else { bail!("the XBE header's TLS address {tls:#x} lies below the base {base:#x}") };
        directories[Directory::Tls as usize] = DataDirectory { rva, size: 24 };
    }
    let headers = Headers {
        pe_offset: 0,
        coff: CoffHeader {
            machine: Machine::I386,
            sections: u16::try_from(section_count).unwrap_or(u16::MAX),
            timestamp,
            symbol_table: 0,
            symbols: 0,
            optional_header_size: 0,
            characteristics: CoffHeader::EXECUTABLE_IMAGE | CoffHeader::RELOCS_STRIPPED,
        },
        optional: OptionalHeader {
            magic: OptionalMagic::Pe32,
            linker_version: (0, 0),
            size_of_code,
            size_of_initialized_data,
            size_of_uninitialized_data,
            entry_point: entry_va - base,
            base_of_code,
            image_base: u64::from(base),
            // The kernel maps pages; the builder packs the file by 0x20 and lets two sections
            // share a page, so neither alignment is a PE's.
            section_alignment: 0x1000,
            file_alignment: 0x20,
            os_version: (0, 0),
            image_version: (0, 0),
            subsystem_version: (0, 0),
            size_of_image,
            size_of_headers,
            checksum,
            subsystem: SUBSYSTEM_XBOX,
            dll_characteristics: 0,
            stack_reserve: u64::from(stack_commit),
            stack_commit: u64::from(stack_commit),
            heap_reserve: u64::from(heap_reserve),
            heap_commit: u64::from(heap_commit),
            directories,
        },
        section_table_offset,
    };
    let xbe = Xbe {
        kernel,
        entry_va: u64::from(entry_va),
        kernel_thunk_va: u64::from(kernel_thunk_va),
        tls_va: u64::from(tls),
        init_flags,
        timestamp,
        title_id,
        title_name,
        region,
        version,
        libraries: versions,
        debug_path,
    };
    Ok((headers, sections, xbe))
}

/// The kernel thunk table as imports: each slot holds `0x8000_0000 | ordinal` until the kernel
/// writes the export's address over it, so a slot is an IAT entry of `xboxkrnl.exe`.
pub(crate) fn kernel_imports(image: &Image, xbe: &Xbe) -> Result<Vec<Import>> {
    let mut out = Vec::new();
    let mut slot = xbe.kernel_thunk_va;
    loop {
        let entry = image.u32(slot).with_context(|| format!("kernel thunk at {slot:#x}"))?;
        if entry == 0 {
            return Ok(out);
        }
        ensure!(entry & 0x8000_0000 != 0, "the kernel thunk at {slot:#x} is {entry:#x}, not an ordinal");
        let ordinal = (entry & 0xffff) as u16;
        let name = kernel_export(ordinal).map(|e| e.name.to_owned());
        out.push(Import { dll: KERNEL_DLL.to_owned(), name, hint: 0, ordinal: Some(ordinal), iat_va: slot, delayed: false });
        slot += 4;
    }
}

/// Whether `dll` is the Xbox kernel, however the name is cased.
pub fn is_kernel_dll(dll: &str) -> bool {
    dll.eq_ignore_ascii_case(KERNEL_DLL)
}

/// How a kernel export is called, with its argument count in dwords where it has one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Convention {
    /// Every argument on the stack, popped by the callee.
    Stdcall(u8),
    /// The first two arguments in `ecx` and `edx`, the rest on the stack, popped by the callee.
    Fastcall(u8),
    /// Every argument on the stack, popped by the caller.
    Cdecl(u8),
    /// A format and whatever follows it, popped by the caller.
    Variadic,
    /// A variable, not a function: the slot holds its address.
    Data,
}

/// One export of the kernel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KernelExport {
    pub ordinal: u16,
    pub name: &'static str,
    pub convention: Convention,
}

impl KernelExport {
    /// The bytes a call pops on return, or `None` for a variable.
    pub fn stack_bytes(&self) -> Option<u64> {
        match self.convention {
            Convention::Stdcall(n) => Some(4 * u64::from(n)),
            Convention::Fastcall(n) => Some(4 * u64::from(n.saturating_sub(2))),
            Convention::Cdecl(_) | Convention::Variadic => Some(0),
            Convention::Data => None,
        }
    }

    /// The argument count, or `None` for a variadic function or a variable.
    pub fn arity(&self) -> Option<usize> {
        match self.convention {
            Convention::Stdcall(n) | Convention::Fastcall(n) | Convention::Cdecl(n) => Some(usize::from(n)),
            Convention::Variadic | Convention::Data => None,
        }
    }

    pub fn is_fastcall(&self) -> bool {
        matches!(self.convention, Convention::Fastcall(_))
    }

    pub fn is_variadic(&self) -> bool {
        self.convention == Convention::Variadic
    }

    pub fn is_data(&self) -> bool {
        self.convention == Convention::Data
    }
}

/// The export at `ordinal`, or `None` past the table or at an ordinal the kernel never filled.
pub fn kernel_export(ordinal: u16) -> Option<KernelExport> {
    let &(name, convention) = KERNEL_EXPORTS.get(usize::from(ordinal).checked_sub(1)?)?;
    (!name.is_empty()).then_some(KernelExport { ordinal, name, convention })
}

/// The export called `name`.
pub fn kernel_export_named(name: &str) -> Option<KernelExport> {
    let index = KERNEL_EXPORTS.iter().position(|(n, _)| *n == name)?;
    kernel_export(index as u16 + 1)
}

use Convention::{Cdecl as C, Data as D, Fastcall as F, Stdcall as S, Variadic as V};

/// The kernel's export table, by ordinal from 1, with the convention and argument count of each
/// from the kernel's prototypes; the ordinals the kernel never filled are empty.
const KERNEL_EXPORTS: &[(&str, Convention)] = &[
    ("AvGetSavedDataAddress", S(0)),
    ("AvSendTVEncoderOption", S(4)),
    ("AvSetDisplayMode", S(6)),
    ("AvSetSavedDataAddress", S(1)),
    ("DbgBreakPoint", S(0)),
    ("DbgBreakPointWithStatus", S(1)),
    ("DbgLoadImageSymbols", S(3)),
    ("DbgPrint", V),
    ("HalReadSMCTrayState", S(2)),
    ("DbgPrompt", S(3)),
    ("DbgUnLoadImageSymbols", S(3)),
    ("ExAcquireReadWriteLockExclusive", S(1)),
    ("ExAcquireReadWriteLockShared", S(1)),
    ("ExAllocatePool", S(1)),
    ("ExAllocatePoolWithTag", S(2)),
    ("ExEventObjectType", D),
    ("ExFreePool", S(1)),
    ("ExInitializeReadWriteLock", S(1)),
    ("ExInterlockedAddLargeInteger", S(4)),
    ("ExInterlockedAddLargeStatistic", F(2)),
    ("ExInterlockedCompareExchange64", F(3)),
    ("ExMutantObjectType", D),
    ("ExQueryPoolBlockSize", S(1)),
    ("ExQueryNonVolatileSetting", S(5)),
    ("ExReadWriteRefurbInfo", S(3)),
    ("ExRaiseException", S(1)),
    ("ExRaiseStatus", S(1)),
    ("ExReleaseReadWriteLock", S(1)),
    ("ExSaveNonVolatileSetting", S(4)),
    ("ExSemaphoreObjectType", D),
    ("ExTimerObjectType", D),
    ("ExfInterlockedInsertHeadList", F(2)),
    ("ExfInterlockedInsertTailList", F(2)),
    ("ExfInterlockedRemoveHeadList", F(1)),
    ("FscGetCacheSize", S(0)),
    ("FscInvalidateIdleBlocks", S(0)),
    ("FscSetCacheSize", S(1)),
    ("HalClearSoftwareInterrupt", F(1)),
    ("HalDisableSystemInterrupt", S(1)),
    ("HalDiskCachePartitionCount", D),
    ("HalDiskModelNumber", D),
    ("HalDiskSerialNumber", D),
    ("HalEnableSystemInterrupt", S(2)),
    ("HalGetInterruptVector", S(2)),
    ("HalReadSMBusValue", S(4)),
    ("HalReadWritePCISpace", S(6)),
    ("HalRegisterShutdownNotification", S(2)),
    ("HalRequestSoftwareInterrupt", F(1)),
    ("HalReturnToFirmware", S(1)),
    ("HalWriteSMBusValue", S(4)),
    ("InterlockedCompareExchange", F(3)),
    ("InterlockedDecrement", F(1)),
    ("InterlockedIncrement", F(1)),
    ("InterlockedExchange", F(2)),
    ("InterlockedExchangeAdd", F(2)),
    ("InterlockedFlushSList", F(1)),
    ("InterlockedPopEntrySList", F(1)),
    ("InterlockedPushEntrySList", F(2)),
    ("IoAllocateIrp", S(1)),
    ("IoBuildAsynchronousFsdRequest", S(6)),
    ("IoBuildDeviceIoControlRequest", S(9)),
    ("IoBuildSynchronousFsdRequest", S(7)),
    ("IoCheckShareAccess", S(5)),
    ("IoCompletionObjectType", D),
    ("IoCreateDevice", S(6)),
    ("IoCreateFile", S(10)),
    ("IoCreateSymbolicLink", S(2)),
    ("IoDeleteDevice", S(1)),
    ("IoDeleteSymbolicLink", S(1)),
    ("IoDeviceObjectType", D),
    ("IoFileObjectType", D),
    ("IoFreeIrp", S(1)),
    ("IoInitializeIrp", S(3)),
    ("IoInvalidDeviceRequest", S(2)),
    ("IoQueryFileInformation", S(5)),
    ("IoQueryVolumeInformation", S(5)),
    ("IoQueueThreadIrp", S(1)),
    ("IoRemoveShareAccess", S(2)),
    ("IoSetIoCompletion", S(5)),
    ("IoSetShareAccess", S(4)),
    ("IoStartNextPacket", S(1)),
    ("IoStartNextPacketByKey", S(2)),
    ("IoStartPacket", S(3)),
    ("IoSynchronousDeviceIoControlRequest", S(8)),
    ("IoSynchronousFsdRequest", S(5)),
    ("IofCallDriver", F(2)),
    ("IofCompleteRequest", F(2)),
    ("KdDebuggerEnabled", D),
    ("KdDebuggerNotPresent", D),
    ("IoDismountVolume", S(1)),
    ("IoDismountVolumeByName", S(1)),
    ("KeAlertResumeThread", S(1)),
    ("KeAlertThread", S(2)),
    ("KeBoostPriorityThread", S(2)),
    ("KeBugCheck", S(1)),
    ("KeBugCheckEx", S(5)),
    ("KeCancelTimer", S(1)),
    ("KeConnectInterrupt", S(1)),
    ("KeDelayExecutionThread", S(3)),
    ("KeDisconnectInterrupt", S(1)),
    ("KeEnterCriticalRegion", S(0)),
    ("MmGlobalData", D),
    ("KeGetCurrentIrql", S(0)),
    ("KeGetCurrentThread", S(0)),
    ("KeInitializeApc", S(7)),
    ("KeInitializeDeviceQueue", S(1)),
    ("KeInitializeDpc", S(3)),
    ("KeInitializeEvent", S(3)),
    ("KeInitializeInterrupt", S(7)),
    ("KeInitializeMutant", S(2)),
    ("KeInitializeQueue", S(2)),
    ("KeInitializeSemaphore", S(3)),
    ("KeInitializeTimerEx", S(2)),
    ("KeInsertByKeyDeviceQueue", S(3)),
    ("KeInsertDeviceQueue", S(2)),
    ("KeInsertHeadQueue", S(2)),
    ("KeInsertQueue", S(2)),
    ("KeInsertQueueApc", S(4)),
    ("KeInsertQueueDpc", S(3)),
    ("KeInterruptTime", D),
    ("KeIsExecutingDpc", S(0)),
    ("KeLeaveCriticalRegion", S(0)),
    ("KePulseEvent", S(3)),
    ("KeQueryBasePriorityThread", S(1)),
    ("KeQueryInterruptTime", S(0)),
    ("KeQueryPerformanceCounter", S(0)),
    ("KeQueryPerformanceFrequency", S(0)),
    ("KeQuerySystemTime", S(1)),
    ("KeRaiseIrqlToDpcLevel", S(0)),
    ("KeRaiseIrqlToSynchLevel", S(0)),
    ("KeReleaseMutant", S(4)),
    ("KeReleaseSemaphore", S(4)),
    ("KeRemoveByKeyDeviceQueue", S(2)),
    ("KeRemoveDeviceQueue", S(1)),
    ("KeRemoveEntryDeviceQueue", S(2)),
    ("KeRemoveQueue", S(3)),
    ("KeRemoveQueueDpc", S(1)),
    ("KeResetEvent", S(1)),
    ("KeRestoreFloatingPointState", S(1)),
    ("KeResumeThread", S(1)),
    ("KeRundownQueue", S(1)),
    ("KeSaveFloatingPointState", S(1)),
    ("KeSetBasePriorityThread", S(2)),
    ("KeSetDisableBoostThread", S(2)),
    ("KeSetEvent", S(3)),
    ("KeSetEventBoostPriority", S(2)),
    ("KeSetPriorityProcess", S(2)),
    ("KeSetPriorityThread", S(2)),
    ("KeSetTimer", S(4)),
    ("KeSetTimerEx", S(5)),
    ("KeStallExecutionProcessor", S(1)),
    ("KeSuspendThread", S(1)),
    ("KeSynchronizeExecution", S(3)),
    ("KeSystemTime", D),
    ("KeTestAlertThread", S(1)),
    ("KeTickCount", D),
    ("KeTimeIncrement", D),
    ("KeWaitForMultipleObjects", S(8)),
    ("KeWaitForSingleObject", S(5)),
    ("KfRaiseIrql", F(1)),
    ("KfLowerIrql", F(1)),
    ("KiBugCheckData", D),
    ("KiUnlockDispatcherDatabase", F(1)),
    ("LaunchDataPage", D),
    ("MmAllocateContiguousMemory", S(1)),
    ("MmAllocateContiguousMemoryEx", S(5)),
    ("MmAllocateSystemMemory", S(2)),
    ("MmClaimGpuInstanceMemory", S(2)),
    ("MmCreateKernelStack", S(2)),
    ("MmDeleteKernelStack", S(2)),
    ("MmFreeContiguousMemory", S(1)),
    ("MmFreeSystemMemory", S(2)),
    ("MmGetPhysicalAddress", S(1)),
    ("MmIsAddressValid", S(1)),
    ("MmLockUnlockBufferPages", S(3)),
    ("MmLockUnlockPhysicalPage", S(2)),
    ("MmMapIoSpace", S(3)),
    ("MmPersistContiguousMemory", S(3)),
    ("MmQueryAddressProtect", S(1)),
    ("MmQueryAllocationSize", S(1)),
    ("MmQueryStatistics", S(1)),
    ("MmSetAddressProtect", S(3)),
    ("MmUnmapIoSpace", S(2)),
    ("NtAllocateVirtualMemory", S(5)),
    ("NtCancelTimer", S(2)),
    ("NtClearEvent", S(1)),
    ("NtClose", S(1)),
    ("NtCreateDirectoryObject", S(2)),
    ("NtCreateEvent", S(4)),
    ("NtCreateFile", S(9)),
    ("NtCreateIoCompletion", S(4)),
    ("NtCreateMutant", S(3)),
    ("NtCreateSemaphore", S(4)),
    ("NtCreateTimer", S(3)),
    ("NtDeleteFile", S(1)),
    ("NtDeviceIoControlFile", S(10)),
    ("NtDuplicateObject", S(3)),
    ("NtFlushBuffersFile", S(2)),
    ("NtFreeVirtualMemory", S(3)),
    ("NtFsControlFile", S(10)),
    ("NtOpenDirectoryObject", S(2)),
    ("NtOpenFile", S(6)),
    ("NtOpenSymbolicLinkObject", S(2)),
    ("NtProtectVirtualMemory", S(4)),
    ("NtPulseEvent", S(2)),
    ("NtQueueApcThread", S(5)),
    ("NtQueryDirectoryFile", S(10)),
    ("NtQueryDirectoryObject", S(6)),
    ("NtQueryEvent", S(2)),
    ("NtQueryFullAttributesFile", S(2)),
    ("NtQueryInformationFile", S(5)),
    ("NtQueryIoCompletion", S(2)),
    ("NtQueryMutant", S(2)),
    ("NtQuerySemaphore", S(2)),
    ("NtQuerySymbolicLinkObject", S(3)),
    ("NtQueryTimer", S(2)),
    ("NtQueryVirtualMemory", S(2)),
    ("NtQueryVolumeInformationFile", S(5)),
    ("NtReadFile", S(8)),
    ("NtReadFileScatter", S(8)),
    ("NtReleaseMutant", S(2)),
    ("NtReleaseSemaphore", S(3)),
    ("NtRemoveIoCompletion", S(5)),
    ("NtResumeThread", S(2)),
    ("NtSetEvent", S(2)),
    ("NtSetInformationFile", S(5)),
    ("NtSetIoCompletion", S(5)),
    ("NtSetSystemTime", S(2)),
    ("NtSetTimerEx", S(8)),
    ("NtSignalAndWaitForSingleObjectEx", S(5)),
    ("NtSuspendThread", S(2)),
    ("NtUserIoApcDispatcher", S(3)),
    ("NtWaitForSingleObject", S(3)),
    ("NtWaitForSingleObjectEx", S(4)),
    ("NtWaitForMultipleObjectsEx", S(6)),
    ("NtWriteFile", S(8)),
    ("NtWriteFileGather", S(8)),
    ("NtYieldExecution", S(0)),
    ("ObCreateObject", S(4)),
    ("ObDirectoryObjectType", D),
    ("ObInsertObject", S(4)),
    ("ObMakeTemporaryObject", S(1)),
    ("ObOpenObjectByName", S(4)),
    ("ObOpenObjectByPointer", S(3)),
    ("ObpObjectHandleTable", D),
    ("ObReferenceObjectByHandle", S(3)),
    ("ObReferenceObjectByName", S(5)),
    ("ObReferenceObjectByPointer", S(2)),
    ("ObSymbolicLinkObjectType", D),
    ("ObfDereferenceObject", F(1)),
    ("ObfReferenceObject", F(1)),
    ("PhyGetLinkState", S(1)),
    ("PhyInitialize", S(2)),
    ("PsCreateSystemThread", S(5)),
    ("PsCreateSystemThreadEx", S(10)),
    ("PsQueryStatistics", S(1)),
    ("PsSetCreateThreadNotifyRoutine", S(1)),
    ("PsTerminateSystemThread", S(1)),
    ("PsThreadObjectType", D),
    ("RtlAnsiStringToUnicodeString", S(3)),
    ("RtlAppendStringToString", S(2)),
    ("RtlAppendUnicodeStringToString", S(2)),
    ("RtlAppendUnicodeToString", S(2)),
    ("RtlAssert", S(4)),
    ("RtlCaptureContext", S(1)),
    ("RtlCaptureStackBackTrace", S(4)),
    ("RtlCharToInteger", S(3)),
    ("RtlCompareMemory", S(3)),
    ("RtlCompareMemoryUlong", S(3)),
    ("RtlCompareString", S(3)),
    ("RtlCompareUnicodeString", S(3)),
    ("RtlCopyString", S(2)),
    ("RtlCopyUnicodeString", S(2)),
    ("RtlCreateUnicodeString", S(2)),
    ("RtlDowncaseUnicodeChar", S(1)),
    ("RtlDowncaseUnicodeString", S(3)),
    ("RtlEnterCriticalSection", S(1)),
    ("RtlEnterCriticalSectionAndRegion", S(1)),
    ("RtlEqualString", S(3)),
    ("RtlEqualUnicodeString", S(3)),
    ("RtlExtendedIntegerMultiply", S(3)),
    ("RtlExtendedLargeIntegerDivide", S(4)),
    ("RtlExtendedMagicDivide", S(5)),
    ("RtlFillMemory", S(3)),
    ("RtlFillMemoryUlong", S(3)),
    ("RtlFreeAnsiString", S(1)),
    ("RtlFreeUnicodeString", S(1)),
    ("RtlGetCallersAddress", S(2)),
    ("RtlInitAnsiString", S(2)),
    ("RtlInitUnicodeString", S(2)),
    ("RtlInitializeCriticalSection", S(1)),
    ("RtlIntegerToChar", S(4)),
    ("RtlIntegerToUnicodeString", S(3)),
    ("RtlLeaveCriticalSection", S(1)),
    ("RtlLeaveCriticalSectionAndRegion", S(1)),
    ("RtlLowerChar", S(1)),
    ("RtlMapGenericMask", S(2)),
    ("RtlMoveMemory", S(3)),
    ("RtlMultiByteToUnicodeN", S(5)),
    ("RtlMultiByteToUnicodeSize", S(3)),
    ("RtlNtStatusToDosError", S(1)),
    ("RtlRaiseException", S(1)),
    ("RtlRaiseStatus", S(1)),
    ("RtlTimeFieldsToTime", S(2)),
    ("RtlTimeToTimeFields", S(2)),
    ("RtlTryEnterCriticalSection", S(1)),
    ("RtlUlongByteSwap", F(1)),
    ("RtlUnicodeStringToAnsiString", S(3)),
    ("RtlUnicodeStringToInteger", S(3)),
    ("RtlUnicodeToMultiByteN", S(5)),
    ("RtlUnicodeToMultiByteSize", S(3)),
    ("RtlUnwind", S(4)),
    ("RtlUpcaseUnicodeChar", S(1)),
    ("RtlUpcaseUnicodeString", S(3)),
    ("RtlUpcaseUnicodeToMultiByteN", S(5)),
    ("RtlUpperChar", S(1)),
    ("RtlUpperString", S(2)),
    ("RtlUshortByteSwap", F(1)),
    ("RtlWalkFrameChain", S(3)),
    ("RtlZeroMemory", S(2)),
    ("XboxEEPROMKey", D),
    ("XboxHardwareInfo", D),
    ("XboxHDKey", D),
    ("XboxKrnlVersion", D),
    ("XboxSignatureKey", D),
    ("XeImageFileName", D),
    ("XeLoadSection", S(1)),
    ("XeUnloadSection", S(1)),
    ("READ_PORT_BUFFER_UCHAR", S(3)),
    ("READ_PORT_BUFFER_USHORT", S(3)),
    ("READ_PORT_BUFFER_ULONG", S(3)),
    ("WRITE_PORT_BUFFER_UCHAR", S(3)),
    ("WRITE_PORT_BUFFER_USHORT", S(3)),
    ("WRITE_PORT_BUFFER_ULONG", S(3)),
    ("XcSHAInit", S(1)),
    ("XcSHAUpdate", S(3)),
    ("XcSHAFinal", S(2)),
    ("XcRC4Key", S(3)),
    ("XcRC4Crypt", S(3)),
    ("XcHMAC", S(7)),
    ("XcPKEncPublic", S(3)),
    ("XcPKDecPrivate", S(3)),
    ("XcPKGetKeyLen", S(1)),
    ("XcVerifyPKCS1Signature", S(3)),
    ("XcModExp", S(5)),
    ("XcDESKeyParity", S(2)),
    ("XcKeyTable", S(3)),
    ("XcBlockCrypt", S(5)),
    ("XcBlockCryptCBC", S(7)),
    ("XcCryptService", S(2)),
    ("XcUpdateCrypto", S(2)),
    ("RtlRip", S(3)),
    ("XboxLANKey", D),
    ("XboxAlternateSignatureKeys", D),
    ("XePublicKeyData", D),
    ("HalBootSMCVideoMode", D),
    ("IdexChannelObject", D),
    ("HalIsResetOrShutdownPending", S(0)),
    ("IoMarkIrpMustComplete", S(1)),
    ("HalInitiateShutdown", S(0)),
    ("RtlSnprintf", V),
    ("RtlSprintf", V),
    ("RtlVsnprintf", C(4)),
    ("RtlVsprintf", C(3)),
    ("HalEnableSecureTrayEject", S(0)),
    ("HalWriteSMCScratchRegister", S(1)),
    ("", D),
    ("", D),
    ("", D),
    ("", D),
    ("", D),
    ("", D),
    ("", D),
    ("MmDbgAllocateMemory", S(2)),
    ("MmDbgFreeMemory", S(2)),
    ("MmDbgQueryAvailablePages", S(0)),
    ("MmDbgReleaseAddress", S(2)),
    ("MmDbgWriteCheck", S(2)),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_export_table_names_the_known_ordinals() {
        assert_eq!(kernel_export(8).unwrap().name, "DbgPrint");
        assert_eq!(kernel_export(49).unwrap().name, "HalReturnToFirmware");
        assert_eq!(kernel_export(187).unwrap().name, "NtClose");
        assert_eq!(kernel_export(320).unwrap().name, "RtlZeroMemory");
        assert_eq!(kernel_export(378).unwrap().name, "MmDbgWriteCheck");
        assert!(kernel_export(0).is_none() && kernel_export(367).is_none() && kernel_export(379).is_none());
        assert_eq!(kernel_export_named("InterlockedIncrement").unwrap().ordinal, 53);
        assert_eq!(kernel_export_named("NtCreateFile").unwrap().stack_bytes(), Some(36));
        assert_eq!(kernel_export_named("InterlockedCompareExchange").unwrap().stack_bytes(), Some(4));
        assert_eq!(kernel_export_named("DbgPrint").unwrap().stack_bytes(), Some(0));
        assert_eq!(kernel_export_named("KeTickCount").unwrap().stack_bytes(), None);
        assert_eq!(KERNEL_EXPORTS.len(), 378);
    }
}
