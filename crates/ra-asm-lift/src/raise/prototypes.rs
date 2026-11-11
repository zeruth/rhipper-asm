//! The prototypes of the imports: how many integer arguments each takes, and which arguments are
//! doubles or 64-bit integers and what comes back where that matters (the C runtime's math, the
//! Win32 functions that take a `LARGE_INTEGER`). The C runtime's functions and the runtime's own
//! shims are written here by hand; the Windows API comes from [`win32`], a table generated from the
//! SDK headers (`tests/win32_protos.rs` regenerates it), which the hand table takes precedence
//! over. An import neither knows is modeled as reading all four argument registers, which is safe
//! and may add a parameter the enclosing function does not have.

mod win32;

use ra_asm_ir::{FnType, Type};

use super::abi::{Kind, Signature};

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
    /// arrive in registers, on x86 all of them on the stack (`cdecl`).
    pub fn signature(&self, is_64: bool) -> Signature {
        let registers = if is_64 { self.params.len().min(4) } else { 0 };
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
        | "expm1" | "log" | "log2" | "log10" | "log1p" | "logb" | "sqrt" | "cbrt" | "floor" | "ceil" | "trunc" | "round" | "rint"
        | "nearbyint" | "fabs" | "erf" | "erfc" | "tgamma" | "lgamma" | "_j0" | "_j1" | "_y0" | "_y1" | "_chgsign" => {
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
    if let Some(proto) = typed(name) {
        return Some(proto.stack_bytes_x86() as u64);
    }
    arity(name).map(|n| 4 * n as u64)
}

/// The runtime's own version of an import that a 32-bit program cannot call on the 64-bit host
/// as it is: the C runtime functions that walk a table of pointers (`_initterm` and the onexit
/// table) or hand back one (`argv`, the environment), where the width of a pointer is the
/// layout, and `InitializeSListHead`, whose 64-bit version writes 16 bytes into the program's
/// 8-byte list head and refuses one that is not 16-byte aligned, and `_controlfp`, whose 64-bit
/// version has no x87 precision control to report. The shim takes the same arguments and keeps
/// the program's layout.
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
        "_controlfp" => "ra_x86_controlfp",
        "_setjmp3" | "_setjmp" => "ra_x86_setjmp3",
        "longjmp" | "_longjmpex" => "ra_x86_longjmp",
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

/// The variadic functions of the C runtime: a call passes the universal arguments, every register
/// and stack slot the caller set, since the arguments beyond the format are the caller's to count.
pub fn is_variadic(name: &str) -> bool {
    matches!(
        name,
        "printf"
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

/// The number of integer arguments of a well-known import, by name. A variadic function has
/// none: it takes the universal arguments.
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
        | "__report_gsfailure"
        | "_RTC_Initialize"
        | "__telemetry_main_invoke_trigger"
        | "__telemetry_main_return_trigger"
        | "_kbhit"
        | "_getch"
        | "__iob_func" => 0,

        "ExitProcess"
        | "ExitThread"
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
        | "ra_x86_initialize_onexit_table"
        | "ra_x86_execute_onexit_table" => 1,

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
        | "ra_x86_controlfp"
        | "_setjmp3"
        | "ra_x86_setjmp3"
        | "ra_x86_longjmp"
        | "ra_x86_initterm"
        | "ra_x86_initterm_e"
        | "ra_x86_register_onexit_function" => 2,

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
        | "bsearch"
        | "__stdio_common_vfprintf"
        | "__stdio_common_vfwprintf" => 5,

        "CreateThread" | "MultiByteToWideChar" | "LCMapStringW" | "CompareStringW" | "__stdio_common_vsprintf" | "_beginthreadex" => 6,
        "CreateFileA" | "CreateFileW" | "FormatMessageA" | "FormatMessageW" => 7,
        "WideCharToMultiByte" | "RtlVirtualUnwind" => 8,
        "CreateWindowExA" | "CreateWindowExW" => 12,
        _ => return win32(name).map(|(_, params)| params.len()),
    })
}
