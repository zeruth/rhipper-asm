//! The prototypes of the imports: how many integer arguments each takes, and which arguments are
//! doubles or 64-bit integers and what comes back where that matters (the C runtime's math, the
//! Win32 functions that take a `LARGE_INTEGER`). The C runtime's functions and the runtime's own
//! shims are written here by hand; the Windows API comes from [`win32`], a table generated from the
//! SDK headers (`tests/win32_protos.rs` regenerates it), which the hand table takes precedence
//! over. An import neither knows is modeled as reading all four argument registers, which is safe
//! and may add a parameter the enclosing function does not have. The Xbox kernel's exports come
//! from the loader's table ([`ra_asm_pe::xbe`]), keyed by the DLL where a name is also Win32's.
//!
//! Beside the kinds, the parameters and results that take one of the SDK's named constants: the
//! enumeration each belongs to is bound here by hand ([`named`]), and the names and values of the
//! enumerations come from [`win32_names`], generated from the headers (`tests/win32_names.rs`), so
//! the C reads `CreateFileA(name, GENERIC_READ | GENERIC_WRITE, ...)` and `GetLastError() ==
//! ERROR_FILE_NOT_FOUND`.

mod win32;
mod win32_names;

use ra_asm_ir::{FnType, Type};

use super::abi::{Kind, Signature};

/// The Windows API functions whose parameters or result take a named constant: the enumeration
/// of the result (empty for none), then of each parameter in order (empty for none; a parameter
/// past the end takes none). An enumeration's names are in the generated table.
const NAMED: &[(&str, &str, &[&str])] = &[
    ("CreateFileA", "HANDLE_VALUE", &["", "ACCESS_MASK", "FILE_SHARE_MODE", "", "CREATION_DISPOSITION", "FILE_FLAGS_AND_ATTRIBUTES"]),
    ("CreateFileW", "HANDLE_VALUE", &["", "ACCESS_MASK", "FILE_SHARE_MODE", "", "CREATION_DISPOSITION", "FILE_FLAGS_AND_ATTRIBUTES"]),
    ("FindFirstFileA", "HANDLE_VALUE", &[]),
    ("FindFirstFileW", "HANDLE_VALUE", &[]),
    ("GetFileAttributesA", "FILE_FLAGS_AND_ATTRIBUTES", &[]),
    ("GetFileAttributesW", "FILE_FLAGS_AND_ATTRIBUTES", &[]),
    ("SetFileAttributesA", "", &["", "FILE_FLAGS_AND_ATTRIBUTES"]),
    ("SetFileAttributesW", "", &["", "FILE_FLAGS_AND_ATTRIBUTES"]),
    ("SetFilePointer", "SET_FILE_POINTER_RESULT", &["", "", "", "FILE_MOVE_METHOD"]),
    ("SetFilePointerEx", "", &["", "", "", "FILE_MOVE_METHOD"]),
    ("GetStdHandle", "HANDLE_VALUE", &["STD_HANDLE"]),
    ("SetStdHandle", "", &["STD_HANDLE"]),
    ("GetLastError", "WIN32_ERROR", &[]),
    ("SetLastError", "", &["WIN32_ERROR"]),
    ("VirtualAlloc", "", &["", "", "MEM_ALLOCATION", "PAGE_PROTECTION"]),
    ("VirtualAllocEx", "", &["", "", "", "MEM_ALLOCATION", "PAGE_PROTECTION"]),
    ("VirtualFree", "", &["", "", "MEM_ALLOCATION"]),
    ("VirtualFreeEx", "", &["", "", "", "MEM_ALLOCATION"]),
    ("VirtualProtect", "", &["", "", "PAGE_PROTECTION"]),
    ("VirtualProtectEx", "", &["", "", "", "PAGE_PROTECTION"]),
    ("HeapCreate", "", &["HEAP_FLAGS"]),
    ("HeapAlloc", "", &["", "HEAP_FLAGS"]),
    ("HeapReAlloc", "", &["", "HEAP_FLAGS"]),
    ("HeapFree", "", &["", "HEAP_FLAGS"]),
    ("HeapSize", "", &["", "HEAP_FLAGS"]),
    ("WaitForSingleObject", "WAIT_RESULT", &["", "TIMEOUT"]),
    ("WaitForSingleObjectEx", "WAIT_RESULT", &["", "TIMEOUT"]),
    ("WaitForMultipleObjects", "WAIT_RESULT", &["", "", "", "TIMEOUT"]),
    ("WaitForMultipleObjectsEx", "WAIT_RESULT", &["", "", "", "TIMEOUT"]),
    ("Sleep", "", &["TIMEOUT"]),
    ("SleepEx", "", &["TIMEOUT"]),
    ("MultiByteToWideChar", "", &["CODE_PAGE"]),
    ("WideCharToMultiByte", "", &["CODE_PAGE"]),
    ("ShowWindow", "", &["", "SHOW_WINDOW_CMD"]),
    ("SendMessageA", "", &["", "WINDOW_MESSAGE"]),
    ("SendMessageW", "", &["", "WINDOW_MESSAGE"]),
    ("PostMessageA", "", &["", "WINDOW_MESSAGE"]),
    ("PostMessageW", "", &["", "WINDOW_MESSAGE"]),
    ("DefWindowProcA", "", &["", "WINDOW_MESSAGE"]),
    ("DefWindowProcW", "", &["", "WINDOW_MESSAGE"]),
    ("RegOpenKeyExA", "WIN32_ERROR", &["HKEY"]),
    ("RegOpenKeyExW", "WIN32_ERROR", &["HKEY"]),
    ("RegCreateKeyExA", "WIN32_ERROR", &["HKEY"]),
    ("RegCreateKeyExW", "WIN32_ERROR", &["HKEY"]),
    ("RegQueryValueExA", "WIN32_ERROR", &["HKEY"]),
    ("RegQueryValueExW", "WIN32_ERROR", &["HKEY"]),
    ("RegSetValueExA", "WIN32_ERROR", &["HKEY"]),
    ("RegSetValueExW", "WIN32_ERROR", &["HKEY"]),
    ("RegCloseKey", "WIN32_ERROR", &["HKEY"]),
];

/// The enumerations an import's result and parameters take, by name: `None` for a function the
/// binding does not know; inside, `None` for a result or parameter that takes a plain number.
pub fn named(name: &str) -> Option<(Option<&'static str>, Vec<Option<&'static str>>)> {
    let &(_, result, params) = NAMED.iter().find(|(n, ..)| *n == name)?;
    let some = |e: &'static str| (!e.is_empty()).then_some(e);
    Some((some(result), params.iter().map(|e| some(e)).collect()))
}

/// The SDK's name of `value`, a constant of `bits` bits in `enumeration`: one member whose value
/// it is, or, in a flags enumeration, the members whose bits make it up exactly, in the table's
/// order, each with its own value; `None` when the names do not cover it. A member's value is
/// compared at the constant's width, and a 64-bit constant also as the zero-extended `DWORD` the
/// API takes (`STD_OUTPUT_HANDLE`, `(DWORD)-11`, passed in a 64-bit argument).
pub fn constant_name(enumeration: &str, value: u64, bits: u32) -> Option<Vec<(&'static str, u64)>> {
    let i = win32_names::NAMES.binary_search_by_key(&enumeration, |(n, ..)| n).ok()?;
    let (_, flags, members) = win32_names::NAMES[i];
    let mask = if bits >= 64 { u64::MAX } else { (1u64 << bits) - 1 };
    let value = value & mask;
    let same = |member: u64| (member & mask) == value || (bits >= 64 && (member & 0xffff_ffff) == value);
    if let Some(&(name, member)) = members.iter().find(|(_, m)| same(*m)) {
        return Some(vec![(name, member)]);
    }
    if !flags || value == 0 {
        return None;
    }
    let mut rest = value;
    let mut parts = Vec::new();
    for &(name, member) in members {
        let bit = member & mask;
        if bit != 0 && rest & bit == bit {
            parts.push((name, member));
            rest &= !bit;
        }
    }
    (rest == 0).then_some(parts)
}

/// The prototype of a Windows API function from the generated table: the kind of its result
/// (`None` for a `void` function) and of each parameter.
pub fn win32(name: &str) -> Option<(Option<Kind>, Vec<Kind>)> {
    let i = win32::TABLE.binary_search_by_key(&name, |(n, _)| n).ok()?;
    let kinds = win32::TABLE[i].1.as_bytes();
    let kind = |c: u8| match c {
        b'w' => Kind::Wide,
        b'd' => Kind::Double,
        b'f' => Kind::Float,
        _ => Kind::Int,
    };
    let returns = (kinds[0] != b'v').then(|| kind(kinds[0]));
    Some((returns, kinds[1..].iter().map(|&c| kind(c)).collect()))
}

/// The prototype of an import whose arguments or result are not all integers: the C runtime's
/// math. In 32 bits a double is two stack slots and the result comes back in `st(0)`; through the
/// integer adapter the host would take the halves for two arguments and the program would find no
/// result on the x87 stack.
#[derive(Debug, Clone)]
pub struct Typed {
    pub params: Vec<Kind>,
    pub returns: Option<Kind>,
}

impl Typed {
    /// The IR type of the import itself.
    pub fn fn_type(&self) -> FnType {
        FnType::new(self.returns.map_or(Type::Void, Kind::ty), self.params.iter().map(|k| k.ty()).collect())
    }

    /// The import as a signature of the target's convention: on Win64 the first four parameters
    /// arrive in registers, on x86 all of them on the stack (`cdecl`), on ARM64 the first eight of
    /// each class in registers and the rest on the stack.
    pub fn signature(&self, target: super::Target) -> Signature {
        if target.is_arm64() {
            let (mut ints, mut floats) = (0, 0);
            let (params, stack_params): (Vec<Kind>, Vec<Kind>) = self.params.iter().copied().partition(|k| {
                let count = if matches!(k, Kind::Float | Kind::Double) { &mut floats } else { &mut ints };
                *count += 1;
                *count <= target.int_param_slots().len()
            });
            return Signature { params, stack_params, returns: self.returns, ..Signature::default() };
        }
        let registers = if target.is_64() { self.params.len().min(4) } else { 0 };
        Signature {
            params: self.params[..registers].to_vec(),
            stack_params: self.params[registers..].to_vec(),
            returns: self.returns,
            ..Signature::default()
        }
    }

    /// The bytes the arguments take on the x86 stack.
    pub fn stack_bytes_x86(&self) -> i64 {
        self.params.iter().map(|k| if matches!(k, Kind::Wide | Kind::Double) { 8 } else { 4 }).sum()
    }
}

/// The typed prototype of a well-known import, by name: the C runtime's math functions, with the
/// pointer arguments (`frexp`'s exponent, `modf`'s integer part) as integers of the address width.
pub fn typed(name: &str) -> Option<Typed> {
    use Kind::{Double, Float, Int, Wide};
    let (params, returns) = match name {
        "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "sinh" | "cosh" | "tanh" | "asinh" | "acosh" | "atanh" | "exp" | "exp2"
        | "expm1" | "log" | "log2" | "log10" | "log1p" | "logb" | "sqrt" | "ra_x86_sqrt" | "cbrt" | "floor" | "ceil" | "trunc"
        | "round" | "rint" | "nearbyint" | "fabs" | "erf" | "erfc" | "tgamma" | "lgamma" | "_j0" | "_j1" | "_y0" | "_y1" | "_chgsign" => {
            (vec![Double], Some(Double))
        }
        "sinf" | "cosf" | "tanf" | "asinf" | "acosf" | "atanf" | "sinhf" | "coshf" | "tanhf" | "expf" | "exp2f" | "logf" | "log2f"
        | "log10f" | "sqrtf" | "cbrtf" | "floorf" | "ceilf" | "truncf" | "roundf" | "rintf" | "nearbyintf" | "fabsf" | "_chgsignf" => {
            (vec![Float], Some(Float))
        }
        "atan2" | "fmod" | "pow" | "hypot" | "_hypot" | "fmin" | "fmax" | "fdim" | "copysign" | "_copysign" | "nextafter"
        | "_nextafter" | "remainder" => (vec![Double, Double], Some(Double)),
        "atan2f" | "fmodf" | "powf" | "hypotf" | "_hypotf" | "fminf" | "fmaxf" | "fdimf" | "copysignf" | "_copysignf" | "nextafterf"
        | "remainderf" => (vec![Float, Float], Some(Float)),
        "fma" => (vec![Double, Double, Double], Some(Double)),
        "fmaf" => (vec![Float, Float, Float], Some(Float)),
        "frexp" | "modf" | "ldexp" | "scalbn" => (vec![Double, Int], Some(Double)),
        "frexpf" | "modff" | "ldexpf" | "scalbnf" => (vec![Float, Int], Some(Float)),
        "lrint" | "lround" | "ilogb" | "_dclass" | "_dsign" | "_finite" | "_isnan" | "_fpclass" => (vec![Double], Some(Int)),
        "lrintf" | "lroundf" | "ilogbf" | "_fdclass" | "_fdsign" => (vec![Float], Some(Int)),
        "llrint" | "llround" => (vec![Double], Some(Wide)),
        "llrintf" | "llroundf" => (vec![Float], Some(Wide)),
        "atof" => (vec![Int], Some(Double)),
        "strtod" | "ra_x86_strtod" => (vec![Int, Int], Some(Double)),
        "ra_x86_strtoll" | "ra_x86_strtoull" => (vec![Int, Int, Int], Some(Wide)),
        "difftime" | "_difftime64" => (vec![Wide, Wide], Some(Double)),
        // The host's `vcruntime.h` declares it `void (uintptr_t)`, and a lifted cookie check of a
        // statically linked runtime calls it: the port's declaration must agree.
        "__report_gsfailure" => (vec![Int], None),
        "__report_rangecheckfailure" | "ra_x86_report_gsfailure" | "ra_x86_report_rangecheckfailure" => (vec![], None),
        // The C runtime functions whose 64-bit result an x86 caller reads in edx:eax (`div_t` and
        // `ldiv_t` are two ints in one), and the Win32 functions that take or return one: two x86
        // slots, one host register.
        "div" | "ldiv" => (vec![Int, Int], Some(Wide)),
        "_atoi64" | "atoll" | "_wtoi64" | "_telli64" | "_filelengthi64" | "_ftelli64" => (vec![Int], Some(Wide)),
        "llabs" | "_abs64" | "_byteswap_uint64" => (vec![Wide], Some(Wide)),
        "_rotl64" | "_rotr64" => (vec![Wide, Int], Some(Wide)),
        "_lseeki64" => (vec![Int, Wide, Int], Some(Wide)),
        "_fseeki64" => (vec![Int, Wide, Int], Some(Int)),
        "SetFilePointerEx" => (vec![Int, Wide, Int, Int], Some(Int)),
        "GetTickCount64" => (vec![], Some(Wide)),
        // The stdio adapters: the options word is 64 bits, two x86 slots.
        "ra_x86_vsprintf" | "ra_x86_vsprintf_s" | "ra_x86_vsscanf" => (vec![Wide, Int, Int, Int, Int, Int], Some(Int)),
        "ra_x86_vsnprintf_s" => (vec![Wide, Int, Int, Int, Int, Int, Int], Some(Int)),
        "ra_x86_vfprintf" => (vec![Wide, Int, Int, Int, Int], Some(Int)),
        // A Windows API function whose arguments or result are not all integers of the address
        // width: the generated table knows which. One that takes integers only needs no type,
        // only its arity.
        _ => {
            let (returns, params) = win32(name)?;
            let wide = |k: &Kind| !matches!(k, Int);
            if !params.iter().any(wide) && !returns.as_ref().is_some_and(wide) {
                return None;
            }
            (params, returns)
        }
    };
    Some(Typed { params, returns })
}

/// The bytes a 32-bit import pops on return: four per argument for the `stdcall` of the Win32
/// API, none for the `cdecl` of the C runtime DLLs, and `None` for an import the table does not
/// know, which the lift then takes as `cdecl`.
pub fn stdcall_pops(dll: &str, name: &str) -> Option<u64> {
    if is_crt_dll(dll) {
        return Some(0);
    }
    if ra_asm_pe::xbe::is_kernel_dll(dll) {
        return ra_asm_pe::xbe::kernel_export_named(name).and_then(|e| e.stack_bytes());
    }
    if let Some(proto) = typed(name) {
        return Some(proto.stack_bytes_x86() as u64);
    }
    arity(name).map(|n| 4 * n as u64)
}

/// The runtime's own version of an import that a 32-bit program cannot call on the 64-bit host
/// as it is: the C runtime functions that walk a table of pointers (`_initterm` and the onexit
/// table) or hand back one (`argv`, the environment), where the width of a pointer is the
/// layout, and `InitializeSListHead`, whose 64-bit version writes 16 bytes into the program's
/// 8-byte list head and refuses one that is not 16-byte aligned, and the floating-point
/// environment (`_controlfp`, `_clearfp`, `fesetround`, ...), which on x86 is the x87 control and
/// status words as much as the SSE register: the runtime keeps the lifted words and the host's in
/// step. The shim takes the same arguments and keeps the program's layout.
pub fn x86_shim(name: &str) -> Option<&'static str> {
    Some(match name {
        "_initterm" => "ra_x86_initterm",
        "_initterm_e" => "ra_x86_initterm_e",
        "_initialize_onexit_table" => "ra_x86_initialize_onexit_table",
        "_register_onexit_function" => "ra_x86_register_onexit_function",
        "_execute_onexit_table" => "ra_x86_execute_onexit_table",
        "__p___argv" => "ra_x86_p_argv",
        "_get_initial_narrow_environment" => "ra_x86_initial_narrow_environment",
        "InitializeSListHead" => "ra_x86_initialize_slist_head",
        "_controlfp" | "_control87" => "ra_x86_controlfp",
        "_controlfp_s" => "ra_x86_controlfp_s",
        "_clearfp" => "ra_x86_clearfp",
        "_statusfp" => "ra_x86_statusfp",
        "fesetround" => "ra_x86_fesetround",
        "fetestexcept" => "ra_x86_fetestexcept",
        "feclearexcept" => "ra_x86_feclearexcept",
        // The x86 C runtime's square root is the x87's, rounded to the precision control.
        "sqrt" => "ra_x86_sqrt",
        "_setjmp3" | "_setjmp" => "ra_x86_setjmp3",
        "longjmp" | "_longjmpex" => "ra_x86_longjmp",
        // The fail-fast reports of the /GS checks take no argument on x86, and the host has its
        // own once in the DLL and once in the static library: the runtime's end the program.
        "__report_gsfailure" => "ra_x86_report_gsfailure",
        "__report_rangecheckfailure" => "ra_x86_report_rangecheckfailure",
        // The printf and scanf family hand the program's dword arguments on as a va_list, which
        // the host reads in 8-byte slots: the adapter widens them by the format.
        "__stdio_common_vsprintf" => "ra_x86_vsprintf",
        "__stdio_common_vsprintf_s" => "ra_x86_vsprintf_s",
        "__stdio_common_vsnprintf_s" => "ra_x86_vsnprintf_s",
        "__stdio_common_vfprintf" => "ra_x86_vfprintf",
        "__stdio_common_vsscanf" => "ra_x86_vsscanf",
        // The strto* family writes a `char *` of eight bytes where the program expects four.
        "strtol" => "ra_x86_strtol",
        "strtoul" => "ra_x86_strtoul",
        "strtod" => "ra_x86_strtod",
        "strtoll" | "_strtoi64" => "ra_x86_strtoll",
        "strtoull" | "_strtoui64" => "ra_x86_strtoull",
        // The window structures of user32 hold pointers: the runtime widens the program's record
        // for the host and narrows what comes back.
        "RegisterClassA" => "ra_x86_register_class_a",
        "RegisterClassExA" => "ra_x86_register_class_ex_a",
        "GetMessageA" => "ra_x86_get_message_a",
        "PeekMessageA" => "ra_x86_peek_message_a",
        "TranslateMessage" => "ra_x86_translate_message",
        "DispatchMessageA" => "ra_x86_dispatch_message_a",
        "BeginPaint" => "ra_x86_begin_paint",
        "EndPaint" => "ra_x86_end_paint",
        // The C++ runtime's `operator new` and `operator delete`, linked into the image from the
        // runtime library under names no host library exports (`??2@YAPAXI@Z`, the sized and the
        // plain `??3@`): the host's heap, whose blocks a non-large-address-aware port keeps under
        // 4 GB, takes them. The sized delete's size is the caller's to pop.
        "operator_new" | "operator_new_array" => "ra_x86_operator_new",
        "operator_delete" | "operator_delete_array" => "ra_x86_operator_delete",
        _ => return None,
    })
}

/// Whether the 64-bit host has no such import: the x86 structured exception handling of the C
/// runtime, which the x86 startup code registers and only an exception would call. The port
/// defines a stub that stops the program if it is ever reached.
pub fn host_lacks(name: &str) -> bool {
    matches!(
        name,
        "_except_handler4_common"
            | "_except_handler3"
            | "_except_handler2"
            | "__except_handler4"
            | "__except_handler3"
            | "_local_unwind2"
            | "_local_unwind4"
            | "_global_unwind2"
            | "_NLG_Notify"
            | "_EH_prolog"
            | "_EH_prolog3"
            | "_EH_epilog3"
            | "_EH_prolog3_catch"
            | "_EH_epilog3_catch"
            | "_EH_prolog3_GS"
            | "_EH_epilog3_GS"
            | "_EH_prolog3_catch_GS"
            | "_EH_epilog3_catch_GS"
            | "_SEH_prolog4"
            | "_SEH_epilog4"
            | "_SEH_prolog4_GS"
            | "_SEH_epilog4_GS"
            | "__CxxFrameHandler"
            | "__CxxFrameHandler2"
            | "__CxxLongjmpUnwind"
            | "_chkstk"
            | "_alloca_probe"
            | "_alloca_probe_8"
            | "_alloca_probe_16"
            | "_ftol"
            | "_ftol2"
            | "_ftol2_sse"
            | "__ftol2"
            | "_allmul"
            | "_alldiv"
            | "_allrem"
            | "_allshl"
            | "_allshr"
            | "_aulldiv"
            | "_aullrem"
            | "_aullshr"
    )
}

/// Whether `name` of `dll` is a `fastcall` export of the Xbox kernel, whose first two arguments
/// arrive in `ecx` and `edx`: the import wrapper reads them there.
pub fn is_xbox_fastcall(dll: &str, name: &str) -> bool {
    ra_asm_pe::xbe::is_kernel_dll(dll) && ra_asm_pe::xbe::kernel_export_named(name).is_some_and(|e| e.is_fastcall())
}

/// Whether `dll` is a C runtime, whose functions are `cdecl`.
pub fn is_crt_dll(dll: &str) -> bool {
    let d = dll.to_ascii_lowercase();
    d.starts_with("msvcr")
        || d.starts_with("msvcp")
        || d.starts_with("ucrtbase")
        || d.starts_with("api-ms-win-crt")
        || d.starts_with("vcruntime")
        || d == "crtdll.dll"
}

/// The variadic functions of the C runtime and of the Xbox kernel: a call passes the universal
/// arguments, every register and stack slot the caller set, since the arguments beyond the format
/// are the caller's to count.
pub fn is_variadic(name: &str) -> bool {
    matches!(
        name,
        "DbgPrint"
            | "RtlSprintf"
            | "RtlSnprintf"
            | "printf"
            | "wprintf"
            | "fprintf"
            | "fwprintf"
            | "sprintf"
            | "swprintf"
            | "snprintf"
            | "_snprintf"
            | "_snwprintf"
            | "sprintf_s"
            | "snprintf_s"
            | "printf_s"
            | "fprintf_s"
            | "scanf"
            | "wscanf"
            | "sscanf"
            | "swscanf"
            | "fscanf"
            | "scanf_s"
            | "sscanf_s"
            | "_scprintf"
            | "_cprintf"
    )
}

/// The number of integer arguments of a well-known import, by name: the hand table, the Windows
/// API's, then the Xbox kernel's (whose names shared with Win32 agree in count). A variadic
/// function has none: it takes the universal arguments.
pub fn arity(name: &str) -> Option<usize> {
    if is_variadic(name) {
        return None;
    }
    Some(match name {
        "ra_x86_p_argv"
        | "ra_x86_initial_narrow_environment"
        | "__current_exception"
        | "__current_exception_context"
        | "GetCurrentThreadId"
        | "GetCurrentProcessId"
        | "GetLastError"
        | "GetTickCount"
        | "GetCurrentProcess"
        | "GetCurrentThread"
        | "GetCommandLineA"
        | "GetCommandLineW"
        | "GetProcessHeap"
        | "IsDebuggerPresent"
        | "GetConsoleOutputCP"
        | "GetConsoleCP"
        | "GetEnvironmentStringsW"
        | "GetACP"
        | "GetOEMCP"
        | "TlsAlloc"
        | "abort"
        | "getchar"
        | "rand"
        | "clock"
        | "_errno"
        | "__p__commode"
        | "__p__fmode"
        | "_get_initial_narrow_environment"
        | "_get_initial_wide_environment"
        | "__p___argc"
        | "__p___argv"
        | "__p___wargv"
        | "_initialize_narrow_environment"
        | "_initialize_wide_environment"
        | "_cexit"
        | "_c_exit"
        | "terminate"
        | "_invalid_parameter_noinfo"
        | "_invalid_parameter_noinfo_noreturn"
        | "__std_terminate"
        | "_purecall"
        | "__getmainargs"
        | "__wgetmainargs"
        | "__crt_debugger_hook"
        | "_crt_debugger_hook"
        | "__security_init_cookie"
        | "__report_rangecheckfailure"
        | "ra_x86_report_gsfailure"
        | "ra_x86_report_rangecheckfailure"
        | "_RTC_Initialize"
        | "__telemetry_main_invoke_trigger"
        | "__telemetry_main_return_trigger"
        | "_kbhit"
        | "_getch"
        | "_clearfp"
        | "_statusfp"
        | "ra_x86_clearfp"
        | "ra_x86_statusfp"
        | "__iob_func" => 0,

        "ExitProcess"
        | "ExitThread"
        | "__report_gsfailure"
        | "Sleep"
        | "SetLastError"
        | "CloseHandle"
        | "SetEvent"
        | "ResetEvent"
        | "QueryPerformanceCounter"
        | "QueryPerformanceFrequency"
        | "GetSystemTimeAsFileTime"
        | "GetSystemTime"
        | "GetLocalTime"
        | "GetStartupInfoA"
        | "GetStartupInfoW"
        | "GetStdHandle"
        | "FlushFileBuffers"
        | "DeleteFileA"
        | "DeleteFileW"
        | "GetFileType"
        | "GetFileAttributesA"
        | "GetFileAttributesW"
        | "SetConsoleOutputCP"
        | "HeapDestroy"
        | "GlobalFree"
        | "LocalFree"
        | "GlobalLock"
        | "GlobalUnlock"
        | "GetModuleHandleA"
        | "GetModuleHandleW"
        | "LoadLibraryA"
        | "LoadLibraryW"
        | "FreeLibrary"
        | "FreeEnvironmentStringsW"
        | "IsValidCodePage"
        | "InitializeCriticalSection"
        | "EnterCriticalSection"
        | "LeaveCriticalSection"
        | "DeleteCriticalSection"
        | "TlsFree"
        | "TlsGetValue"
        | "InitializeSListHead"
        | "ra_x86_initialize_slist_head"
        | "RtlCaptureContext"
        | "SetUnhandledExceptionFilter"
        | "UnhandledExceptionFilter"
        | "IsProcessorFeaturePresent"
        | "GetSystemInfo"
        | "GetNativeSystemInfo"
        | "OutputDebugStringA"
        | "OutputDebugStringW"
        | "GetTimeZoneInformation"
        | "FindClose"
        | "SetCurrentDirectoryA"
        | "SetCurrentDirectoryW"
        | "TranslateMessage"
        | "DispatchMessageA"
        | "DispatchMessageW"
        | "PostQuitMessage"
        | "RegisterClassA"
        | "RegisterClassW"
        | "RegisterClassExA"
        | "RegisterClassExW"
        | "UpdateWindow"
        | "DestroyWindow"
        | "GetDC"
        | "GetAsyncKeyState"
        | "GetKeyState"
        | "SetCursor"
        | "ShowCursor"
        | "GetSystemMetrics"
        | "malloc"
        | "free"
        | "ra_x86_operator_new"
        | "ra_x86_operator_delete"
        | "strlen"
        | "wcslen"
        | "abs"
        | "labs"
        | "exit"
        | "_exit"
        | "_crt_atexit"
        | "_crt_at_quick_exit"
        | "puts"
        | "_putws"
        | "putchar"
        | "srand"
        | "atoi"
        | "atol"
        | "atof"
        | "toupper"
        | "tolower"
        | "isdigit"
        | "isalpha"
        | "isspace"
        | "isalnum"
        | "isupper"
        | "islower"
        | "fclose"
        | "fflush"
        | "rewind"
        | "feof"
        | "ferror"
        | "clearerr"
        | "ftell"
        | "fgetc"
        | "_fileno"
        | "_isatty"
        | "time"
        | "localtime"
        | "gmtime"
        | "ctime"
        | "asctime"
        | "perror"
        | "_strdup"
        | "_wcsdup"
        | "system"
        | "getenv"
        | "remove"
        | "_unlink"
        | "rmdir"
        | "_rmdir"
        | "_chdir"
        | "_set_app_type"
        | "_configure_narrow_argv"
        | "_configure_wide_argv"
        | "_initialize_onexit_table"
        | "_set_fmode"
        | "__setusermatherr"
        | "_set_new_mode"
        | "__acrt_iob_func"
        | "_callnewh"
        | "_endthreadex"
        | "_time64"
        | "_localtime64"
        | "_gmtime64"
        | "_mkgmtime64"
        | "_aligned_free"
        | "_msize"
        | "_amsg_exit"
        | "_onexit"
        | "__set_app_type"
        | "__dllonexit"
        | "_lock"
        | "_unlock"
        | "_get_osfhandle"
        | "_close"
        | "_setjmp"
        | "_wtoi"
        | "_free_base"
        | "_get_errno"
        | "_configthreadlocale"
        | "_register_thread_local_exe_atexit_callback"
        | "fesetround"
        | "fetestexcept"
        | "feclearexcept"
        | "ra_x86_fesetround"
        | "ra_x86_fetestexcept"
        | "ra_x86_feclearexcept"
        | "ra_x86_initialize_onexit_table"
        | "ra_x86_execute_onexit_table"
        | "ra_x86_register_class_a"
        | "ra_x86_register_class_ex_a"
        | "ra_x86_translate_message"
        | "ra_x86_dispatch_message_a" => 1,

        "TerminateProcess"
        | "GetExitCodeProcess"
        | "GetExitCodeThread"
        | "WaitForSingleObject"
        | "SetStdHandle"
        | "GetFileSize"
        | "GetConsoleMode"
        | "SetConsoleMode"
        | "GlobalAlloc"
        | "LocalAlloc"
        | "GetProcAddress"
        | "SetEnvironmentVariableA"
        | "SetEnvironmentVariableW"
        | "GetCPInfo"
        | "TlsSetValue"
        | "InitializeCriticalSectionAndSpinCount"
        | "FileTimeToSystemTime"
        | "SystemTimeToFileTime"
        | "FindFirstFileA"
        | "FindFirstFileW"
        | "FindNextFileA"
        | "FindNextFileW"
        | "GetCurrentDirectoryA"
        | "GetCurrentDirectoryW"
        | "CreateDirectoryA"
        | "CreateDirectoryW"
        | "ShowWindow"
        | "ReleaseDC"
        | "BeginPaint"
        | "EndPaint"
        | "GetClientRect"
        | "GetWindowRect"
        | "SetWindowTextA"
        | "SetWindowTextW"
        | "LoadCursorA"
        | "LoadCursorW"
        | "LoadIconA"
        | "LoadIconW"
        | "KillTimer"
        | "ungetc"
        | "strcpy"
        | "strcat"
        | "strcmp"
        | "_stricmp"
        | "strchr"
        | "strrchr"
        | "strstr"
        | "wcscpy"
        | "wcscmp"
        | "wcscat"
        | "wcschr"
        | "fopen"
        | "fputs"
        | "fputc"
        | "_wfopen"
        | "calloc"
        | "realloc"
        | "_aligned_malloc"
        | "_initterm"
        | "_initterm_e"
        | "_register_onexit_function"
        | "_seh_filter_exe"
        | "_CxxThrowException"
        | "_open_osfhandle"
        | "_setmode"
        | "_itoa"
        | "_ltoa"
        | "_ultoa"
        | "_i64toa"
        | "_ui64toa"
        | "longjmp"
        | "_longjmpex"
        | "sprintf"
        | "fprintf"
        | "sscanf"
        | "scanf"
        | "_except_handler3"
        | "_XcptFilter"
        | "_controlfp"
        | "_control87"
        | "ra_x86_controlfp"
        | "_setjmp3"
        | "ra_x86_setjmp3"
        | "ra_x86_longjmp"
        | "ra_x86_initterm"
        | "ra_x86_initterm_e"
        | "ra_x86_register_onexit_function"
        | "ra_x86_begin_paint"
        | "ra_x86_end_paint" => 2,

        "VirtualFree"
        | "VirtualQuery"
        | "HeapAlloc"
        | "HeapFree"
        | "HeapSize"
        | "HeapCreate"
        | "GetModuleFileNameA"
        | "GetModuleFileNameW"
        | "LoadLibraryExA"
        | "LoadLibraryExW"
        | "GetEnvironmentVariableA"
        | "GetEnvironmentVariableW"
        | "InitializeCriticalSectionEx"
        | "RtlLookupFunctionEntry"
        | "SystemTimeToTzSpecificLocalTime"
        | "InvalidateRect"
        | "memcpy"
        | "memmove"
        | "memset"
        | "memcmp"
        | "memchr"
        | "strncpy"
        | "strncat"
        | "strncmp"
        | "_strnicmp"
        | "strtol"
        | "strtoul"
        | "strtoll"
        | "strtoull"
        | "ra_x86_strtol"
        | "ra_x86_strtoul"
        | "wcsncmp"
        | "fgets"
        | "fseek"
        | "strcpy_s"
        | "strcat_s"
        | "_recalloc"
        | "_controlfp_s"
        | "ra_x86_controlfp_s"
        | "_open"
        | "_read"
        | "_write"
        | "_lseek"
        | "_snprintf"
        | "vsprintf"
        | "vfprintf"
        | "_strtoi64"
        | "_strtoui64"
        | "fopen_s"
        | "sprintf_s"
        | "swprintf_s"
        | "_itoa_s" => 3,

        "VirtualAlloc"
        | "VirtualProtect"
        | "HeapReAlloc"
        | "SetFilePointer"
        | "GetStringTypeW"
        | "RaiseException"
        | "MessageBoxA"
        | "MessageBoxW"
        | "GetMessageA"
        | "GetMessageW"
        | "ra_x86_get_message_a"
        | "DefWindowProcA"
        | "DefWindowProcW"
        | "SendMessageA"
        | "SendMessageW"
        | "PostMessageA"
        | "PostMessageW"
        | "SetTimer"
        | "fread"
        | "fwrite"
        | "setvbuf"
        | "strncpy_s"
        | "memcpy_s"
        | "memmove_s"
        | "qsort"
        | "_vsnprintf"
        | "__stdio_common_vsscanf"
        | "_except_handler4_common"
        | "__C_specific_handler"
        | "__CxxFrameHandler3"
        | "__CxxFrameHandler4"
        | "_vsnprintf_s"
        | "_sopen_s" => 4,

        "WriteFile"
        | "ReadFile"
        | "WriteConsoleA"
        | "WriteConsoleW"
        | "ReadConsoleA"
        | "ReadConsoleW"
        | "PeekMessageA"
        | "PeekMessageW"
        | "ra_x86_peek_message_a"
        | "bsearch"
        | "__stdio_common_vfprintf"
        | "__stdio_common_vfwprintf" => 5,

        "CreateThread" | "MultiByteToWideChar" | "LCMapStringW" | "CompareStringW" | "__stdio_common_vsprintf" | "_beginthreadex" => 6,
        "CreateFileA" | "CreateFileW" | "FormatMessageA" | "FormatMessageW" => 7,
        "WideCharToMultiByte" | "RtlVirtualUnwind" => 8,
        "CreateWindowExA" | "CreateWindowExW" => 12,
        _ => return win32(name).map(|(_, params)| params.len()).or_else(|| ra_asm_pe::xbe::kernel_export_named(name)?.arity()),
    })
}
