//! The prototypes of the imports: how many integer arguments each takes, and which arguments are
//! doubles or 64-bit integers and what comes back where that matters (the C runtime's math, the
//! Win32 functions that take a `LARGE_INTEGER`). The C runtime's functions and the runtime's own
//! shims are written here by hand; the Windows API comes from [`win32`], which reads the table
//! `ra-asm-protos` generates from the SDK headers and which the hand table takes precedence over.
//! An import neither knows is modeled as reading all four argument registers, which is safe and
//! may add a parameter the enclosing function does not have. The Xbox kernel's exports come from
//! the loader's table ([`ra_asm_pe::xbe`]), keyed by the DLL where a name is also Win32's.
//!
//! Beside the kinds, the parameters and results that take one of the SDK's named constants
//! ([`named`], [`constant_name`]): the binding and the generated names are `ra-asm-protos`'s too,
//! under this path for the emitters that read them here.

use ra_asm_ir::{FnType, Type};
pub use ra_asm_protos::{constant_name, named};

use super::abi::{Kind, KindType, Signature};

/// The prototype of a Windows API function from the generated table: the kind of its result
/// (`None` for a `void` function) and of each parameter.
pub fn win32(name: &str) -> Option<(Option<Kind>, Vec<Kind>)> {
    ra_asm_protos::win32_kinds(name).map(kinds_of)
}

/// The prototype of an SDL2 function (its mixer's and network library's too) from the table
/// generated from SDL's headers: `cdecl`, so the caller pops ([`stdcall_pops`]).
pub fn sdl2(name: &str) -> Option<(Option<Kind>, Vec<Kind>)> {
    ra_asm_protos::sdl2_kinds(name).map(kinds_of)
}

/// Whether the generated layouts name a structure some parameter of `import` points to: the
/// port's wrapper of the import converts it between the program's layout and the host's.
pub fn adapts_structures(import: &str) -> bool {
    !ra_asm_protos::pointees(import).is_empty()
}

/// The prototype of an OpenAL function from the hand-written table: `cdecl`, as SDL's are.
pub fn openal(name: &str) -> Option<(Option<Kind>, Vec<Kind>)> {
    ra_asm_protos::openal_kinds(name).map(kinds_of)
}

/// The kinds of an OpenGL function a program has by name from its context, the result's letter
/// and each parameter's: `stdcall` on 32-bit Windows.
pub fn opengl(name: &str) -> Option<&'static str> {
    ra_asm_protos::opengl_kinds(name)
}

/// Whether a program that imports `import` asks its OpenGL context for functions by name.
pub fn asks_opengl(import: &str) -> bool {
    matches!(import, "ra_x86_sdl_gl_get_proc_address" | "ra_x86_wgl_get_proc_address")
}

/// The prototype of a library function: the Windows API's, SDL2's or OpenAL's.
fn library(name: &str) -> Option<(Option<Kind>, Vec<Kind>)> {
    win32(name).or_else(|| sdl2(name)).or_else(|| openal(name))
}

/// A prototype from a table's letters: the result's, then each parameter's.
fn kinds_of(letters: &str) -> (Option<Kind>, Vec<Kind>) {
    let kinds = letters.as_bytes();
    let kind = |c: u8| match c {
        b'w' => Kind::Wide,
        b'd' => Kind::Double,
        b'f' => Kind::Float,
        _ => Kind::Int,
    };
    let returns = (kinds[0] != b'v').then(|| kind(kinds[0]));
    (returns, kinds[1..].iter().map(|&c| kind(c)).collect())
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
    // The x64 C++ library's inserter of a floating-point value: `this`, then the value in `xmm1`;
    // the stream back.
    if let Some(value) = inserted_float(name) {
        return Some(Typed { params: vec![Int, value], returns: Some(Int) });
    }
    let (params, returns) = match name {
        "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "sinh" | "cosh" | "tanh" | "asinh" | "acosh" | "atanh" | "exp" | "exp2"
        | "expm1" | "log" | "log2" | "log10" | "log1p" | "logb" | "sqrt" | "ra_x86_sqrt" | "cbrt" | "floor" | "ceil" | "trunc"
        | "round" | "rint" | "nearbyint" | "fabs" | "erf" | "erfc" | "tgamma" | "lgamma" | "_j0" | "_j1" | "_y0" | "_y1" | "_chgsign"
        | "ra_x86_sqrt_sse2" | "ra_x86_sin_sse2" | "ra_x86_cos_sse2" | "ra_x86_tan_sse2" | "ra_x86_asin_sse2" | "ra_x86_acos_sse2"
        | "ra_x86_atan_sse2" | "ra_x86_exp_sse2" | "ra_x86_log_sse2" | "ra_x86_log10_sse2" => (vec![Double], Some(Double)),
        // Its operands and result are on the x87 stack, in memory the call reads and writes.
        "ra_x86_ci_atan2" | "ra_x86_ci_fmod" => (vec![], None),
        "sinf" | "cosf" | "tanf" | "asinf" | "acosf" | "atanf" | "sinhf" | "coshf" | "tanhf" | "expf" | "exp2f" | "logf" | "log2f"
        | "log10f" | "sqrtf" | "cbrtf" | "floorf" | "ceilf" | "truncf" | "roundf" | "rintf" | "nearbyintf" | "fabsf" | "_chgsignf" => {
            (vec![Float], Some(Float))
        }
        "atan2" | "fmod" | "pow" | "ra_x86_pow_sse2" | "hypot" | "_hypot" | "fmin" | "fmax" | "fdim" | "copysign" | "_copysign"
        | "nextafter" | "_nextafter" | "remainder" => (vec![Double, Double], Some(Double)),
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
        "ra_x86_vfprintf" | "ra_x86_vfscanf" => (vec![Wide, Int, Int, Int, Int], Some(Int)),
        // A Windows API or SDL2 function whose arguments or result are not all integers of the
        // address width: the generated tables know which. One that takes integers only needs no
        // type, only its arity.
        _ => {
            let (returns, params) = library(name)?;
            let wide = |k: &Kind| !matches!(k, Int);
            if !params.iter().any(wide) && !returns.as_ref().is_some_and(wide) {
                return None;
            }
            (params, returns)
        }
    };
    Some(Typed { params, returns })
}

/// The kind of the value a 64-bit `basic_ostream<C>::operator<<` of a floating-point type inserts,
/// by its decorated name: `M` a `float`, `N` a `double`, `O` a `long double`, which Microsoft's
/// compiler makes a `double`.
fn inserted_float(name: &str) -> Option<Kind> {
    let body = name.strip_prefix("??6?$basic_ostream@")?.strip_suffix("@Z")?;
    let kind = match body.chars().last()? {
        'M' => Kind::Float,
        'N' | 'O' => Kind::Double,
        _ => return None,
    };
    body[..body.len() - 1].ends_with("@std@@QEAAAEAV01@").then_some(kind)
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
    // SDL's functions are `cdecl`, and OpenAL's: those of the tables, and any other of their
    // libraries, whose argument count from the hand table is no count of bytes popped (taken
    // as `stdcall`, `SDL_GetWindowWMInfo` left its caller's stack eight bytes up, and the
    // caller's cookie check failed).
    let library = dll.to_ascii_lowercase();
    if library.starts_with("sdl2") || library.starts_with("openal") || (win32(name).is_none() && name.starts_with("SDL_")) {
        return Some(0);
    }
    if win32(name).is_none() && (sdl2(name).is_some() || openal(name).is_some()) {
        return Some(0);
    }
    if let Some(proto) = typed(name) {
        return Some(proto.stack_bytes_x86() as u64);
    }
    arity(name).map(|n| 4 * n as u64)
}

/// The bytes a function of a runtime library linked into a 32-bit image pops on return, by the
/// name the port calls it under: none for the C runtime's `cdecl`, and the arguments for the
/// `stdcall` array iterators of the C++ runtime.
pub fn linked_pops(name: &str) -> u64 {
    match name {
        "ra_x86_eh_vector_destructor_iterator" => 16,
        "ra_x86_eh_vector_constructor_iterator" => 20,
        _ => 0,
    }
}

/// The bytes a function pops that a 32-bit program asks `GetProcAddress` for by `name`, when the
/// table knows it as a `stdcall` function of the Windows API: a call through the pointer it is
/// handed is no import's, so the runtime's trampoline for it carries them. A variadic function's
/// caller pops, and the runtime takes a C runtime DLL's function as `cdecl` by its module.
pub fn proc_pops(name: &str) -> Option<u64> {
    if is_variadic(name) {
        return None;
    }
    stdcall_pops("", name)
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
        "__p__commode" => "ra_x86_p_commode",
        "__p__fmode" => "ra_x86_p_fmode",
        "__p___argc" => "ra_x86_p_argc",
        "_errno" => "ra_x86_errno",
        "_get_initial_narrow_environment" => "ra_x86_initial_narrow_environment",
        "InitializeSListHead" => "ra_x86_initialize_slist_head",
        // The list itself: the host's operations read a 16-byte head and 8-byte links.
        "InterlockedPushEntrySList" => "ra_x86_interlocked_push_entry_slist",
        "InterlockedPopEntrySList" => "ra_x86_interlocked_pop_entry_slist",
        "InterlockedFlushSList" => "ra_x86_interlocked_flush_slist",
        "QueryDepthSList" => "ra_x86_query_depth_slist",
        // The timer queue writes an 8-byte handle into the program's dword, and its
        // INVALID_HANDLE_VALUE is negative.
        "CreateTimerQueueTimer" => "ra_x86_create_timer_queue_timer",
        "DeleteTimerQueueTimer" => "ra_x86_delete_timer_queue_timer",
        // a pointer-sized INIT_ONCE and context, and an array of pointer-sized handles: the
        // program's are dwords
        "InitOnceExecuteOnce" => "ra_x86_init_once_execute_once",
        "WaitForMultipleObjects" => "ra_x86_wait_for_multiple_objects",
        // a CRITICAL_SECTION is 24 bytes to the program, 40 to the host, and its lock one place
        "InitializeCriticalSection" => "ra_x86_initialize_critical_section",
        "InitializeCriticalSectionAndSpinCount" => "ra_x86_initialize_critical_section_and_spin_count",
        "InitializeCriticalSectionEx" => "ra_x86_initialize_critical_section_ex",
        "EnterCriticalSection" => "ra_x86_enter_critical_section",
        "LeaveCriticalSection" => "ra_x86_leave_critical_section",
        "TryEnterCriticalSection" => "ra_x86_try_enter_critical_section",
        "DeleteCriticalSection" => "ra_x86_delete_critical_section",
        // The window procedure is handed the program's copy of a message's structure, and user32
        // reads its own.
        "DefWindowProcA" => "ra_x86_def_window_proc_a",
        // The 32-bit kernel32 exports the interlocked operations on a 32-bit value; the 64-bit one
        // exports none of them, since they are the compiler's intrinsics there.
        "InterlockedIncrement" => "ra_x86_interlocked_increment",
        "InterlockedDecrement" => "ra_x86_interlocked_decrement",
        "InterlockedExchange" => "ra_x86_interlocked_exchange",
        "InterlockedExchangeAdd" => "ra_x86_interlocked_exchange_add",
        "InterlockedCompareExchange" => "ra_x86_interlocked_compare_exchange",
        // The 64-bit kernel32 encodes a pointer into all 64 bits, which the program keeps in a
        // dword: the decode of what is left is no pointer. The runtime's encoding keeps 32.
        "EncodePointer" | "EncodeSystemPointer" => "ra_x86_encode_pointer",
        "DecodePointer" | "DecodeSystemPointer" => "ra_x86_decode_pointer",
        // shell32 hands back a table of 8-byte pointers to the arguments; the program reads dwords
        "CommandLineToArgvW" => "ra_x86_command_line_to_argv_w",
        "_controlfp" | "_control87" => "ra_x86_controlfp",
        "_controlfp_s" => "ra_x86_controlfp_s",
        "_clearfp" => "ra_x86_clearfp",
        "_statusfp" => "ra_x86_statusfp",
        "fesetround" => "ra_x86_fesetround",
        "fetestexcept" => "ra_x86_fetestexcept",
        "feclearexcept" => "ra_x86_feclearexcept",
        // The x86 C runtime's square root is the x87's, rounded to the precision control.
        "sqrt" => "ra_x86_sqrt",
        // The x86 C runtime's SSE2 math, which the 64-bit one does not export: the host's, under a
        // name of its own since `pow` and the rest may be imports of the stack convention too.
        "_libm_sse2_pow_precise" => "ra_x86_pow_sse2",
        "_libm_sse2_sqrt_precise" => "ra_x86_sqrt_sse2",
        "_libm_sse2_sin_precise" => "ra_x86_sin_sse2",
        "_libm_sse2_cos_precise" => "ra_x86_cos_sse2",
        "_libm_sse2_tan_precise" => "ra_x86_tan_sse2",
        "_libm_sse2_asin_precise" => "ra_x86_asin_sse2",
        "_libm_sse2_acos_precise" => "ra_x86_acos_sse2",
        "_libm_sse2_atan_precise" => "ra_x86_atan_sse2",
        // The x87 one, whose operands are on the unit's stack: the runtime works on the lift's.
        "_CIatan2" => "ra_x86_ci_atan2",
        "_CIfmod" => "ra_x86_ci_fmod",
        "_libm_sse2_exp_precise" => "ra_x86_exp_sse2",
        "_libm_sse2_log_precise" => "ra_x86_log_sse2",
        "_libm_sse2_log10_precise" => "ra_x86_log10_sse2",
        "_setjmp3" | "_setjmp" => "ra_x86_setjmp3",
        "longjmp" | "_longjmpex" => "ra_x86_longjmp",
        // The fail-fast reports of the /GS checks take no argument on x86, and the host has its
        // own once in the DLL and once in the static library: the runtime's end the program.
        "__report_gsfailure" | "__raise_securityfailure" => "ra_x86_report_gsfailure",
        "__report_rangecheckfailure" => "ra_x86_report_rangecheckfailure",
        // The printf and scanf family hand the program's dword arguments on as a va_list, which
        // the host reads in 8-byte slots: the adapter widens them by the format.
        "__stdio_common_vsprintf" => "ra_x86_vsprintf",
        "__stdio_common_vsprintf_s" => "ra_x86_vsprintf_s",
        "__stdio_common_vsnprintf_s" => "ra_x86_vsnprintf_s",
        "__stdio_common_vfprintf" => "ra_x86_vfprintf",
        "__stdio_common_vsscanf" => "ra_x86_vsscanf",
        "__stdio_common_vfscanf" => "ra_x86_vfscanf",
        // The strto* family writes a `char *` of eight bytes where the program expects four.
        "strtol" => "ra_x86_strtol",
        "strtoul" => "ra_x86_strtoul",
        "strtod" => "ra_x86_strtod",
        "strtoll" | "_strtoi64" => "ra_x86_strtoll",
        "strtoull" | "_strtoui64" => "ra_x86_strtoull",
        // The C++ runtime's filebuf asks for the addresses of a FILE's buffer fields through three
        // `char **` the host writes eight bytes into, where the program's are dword locals: the
        // third write would take the dword above them (the saved `this`).
        "_get_stream_buffer_pointers" => "ra_x86_get_stream_buffer_pointers",
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
        // The array iterators of the same library call a `thiscall` structor of the program on
        // each element: the host's would call the dword as a 64-bit function.
        "eh_vector_constructor_iterator" => "ra_x86_eh_vector_constructor_iterator",
        "eh_vector_destructor_iterator" => "ra_x86_eh_vector_destructor_iterator",
        // The run-time type information the host's vcruntime reads in the 64-bit layout
        // (image-relative offsets in eight-byte slots); the program's holds absolute dwords.
        "__RTDynamicCast" => "ra_x86_rt_dynamic_cast",
        "__RTCastToVoid" => "ra_x86_rt_cast_to_void",
        "__RTtypeid" => "ra_x86_rt_typeid",
        // `type_info::name()` caches the undecorated name in a dword of the descriptor, which the
        // host reads as eight bytes.
        "__std_type_info_name" => "ra_x86_std_type_info_name",
        // The comparison and the hash read the decorated name after that pointer: at 4 in the
        // program's descriptor, where the host reads it at 8 and compared two names from their
        // fifth character (`typeid(T *) != typeid(T)` came out equal).
        "__std_type_info_compare" => "ra_x86_std_type_info_compare",
        "__std_type_info_hash" => "ra_x86_std_type_info_hash",
        // A module handle and a function's address of the 64-bit host do not fit the program's
        // dword: the runtime hands out a dword that stands for the handle, and for the function a
        // trampoline below 4 GB that carries what a call through it pops.
        "GetModuleHandleA" => "ra_x86_get_module_handle_a",
        "GetModuleHandleW" => "ra_x86_get_module_handle_w",
        "LoadLibraryA" => "ra_x86_load_library_a",
        "LoadLibraryW" => "ra_x86_load_library_w",
        "LoadLibraryExA" => "ra_x86_load_library_ex_a",
        "LoadLibraryExW" => "ra_x86_load_library_ex_w",
        "GetProcAddress" => "ra_x86_get_proc_address",
        // iphlpapi's list of adapters is a chain of records that hold pointers: the host's
        // records are larger than the program's, and the program walks its own layout.
        "GetAdaptersInfo" => "ra_x86_get_adapters_info",
        // The startup and process records of `CreateProcessA` hold pointers and handles, and
        // Winsock's `hostent` two lists of pointers.
        "CreateProcessA" => "ra_x86_create_process_a",
        "gethostbyname" => "ra_x86_gethostbyname",
        "gethostbyaddr" => "ra_x86_gethostbyaddr",
        // An OpenGL function by name from the context is `stdcall` and takes its floats in
        // vector registers on the host: the runtime hands out one that says what it pops and,
        // where its kinds ask, takes the program's stack words apart for the host.
        "SDL_GL_GetProcAddress" => "ra_x86_sdl_gl_get_proc_address",
        "wglGetProcAddress" => "ra_x86_wgl_get_proc_address",
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

/// Whether `name` is an SSE2 math function of the x86 C runtime (`_libm_sse2_pow_precise`, or
/// the runtime's version of it, `ra_x86_pow_sse2`): a `cdecl` function that takes its doubles in
/// `xmm0` and `xmm1` and returns in `xmm0`, with nothing on the stack. The import wrapper reads
/// them there.
pub fn is_x86_sse2_math(name: &str) -> bool {
    (name.starts_with("_libm_sse2_") && name.ends_with("_precise")) || (name.starts_with("ra_x86_") && name.ends_with("_sse2"))
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
        | "ra_x86_p_commode"
        | "ra_x86_p_fmode"
        | "ra_x86_p_argc"
        | "ra_x86_errno"
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
        | "__iob_func"
        | "tmpfile"
        | "_get_narrow_winmain_command_line" => 0,

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
        | "ra_x86_get_module_handle_a"
        | "ra_x86_get_module_handle_w"
        | "ra_x86_load_library_a"
        | "ra_x86_load_library_w"
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
        | "atexit"
        | "strerror"
        | "_mkdir"
        | "_findclose"
        | "ra_x86_gethostbyname"
        | "ra_x86_sdl_gl_get_proc_address"
        | "ra_x86_wgl_get_proc_address"
        | "_Init_thread_header"
        | "_Init_thread_footer"
        | "_Init_thread_abort"
        | "__std_exception_destroy"
        | "__std_type_info_destroy_list"
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
        | "ra_x86_dispatch_message_a"
        | "ra_x86_interlocked_increment"
        | "ra_x86_interlocked_decrement"
        | "ra_x86_encode_pointer"
        | "ra_x86_decode_pointer"
        | "ra_x86_interlocked_pop_entry_slist"
        | "ra_x86_interlocked_flush_slist"
        | "ra_x86_query_depth_slist"
        | "__RTCastToVoid"
        | "__RTtypeid"
        | "ra_x86_rt_cast_to_void"
        | "ra_x86_rt_typeid"
        | "ra_x86_initialize_critical_section"
        | "ra_x86_enter_critical_section"
        | "ra_x86_leave_critical_section"
        | "ra_x86_try_enter_critical_section"
        | "ra_x86_delete_critical_section"
        | "__std_type_info_hash"
        | "ra_x86_std_type_info_hash"
        // shcore's scaling functions (`STDAPI` in ShellScalingApi.h, which the generated table
        // does not read): a program asks `GetProcAddress` for them and calls the pointer.
        | "SetProcessDpiAwareness" => 1,
        "GetProcessDpiAwareness" | "GetScaleFactorForMonitor" => 2,
        "GetDpiForMonitor" => 4,

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
        | "ra_x86_get_proc_address"
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
        | "ra_x86_end_paint"
        | "ra_x86_interlocked_exchange"
        | "ra_x86_interlocked_exchange_add"
        | "ra_x86_interlocked_push_entry_slist"
        | "__std_type_info_name"
        | "ra_x86_std_type_info_name"
        | "__std_type_info_compare"
        | "ra_x86_std_type_info_compare"
        | "ra_x86_get_adapters_info"
        | "ra_x86_initialize_critical_section_and_spin_count"
        | "rename"
        // SDL's functions the generated table leaves out for a structure they take or return:
        // the window's system information, and a joystick's GUID through the hidden pointer.
        | "SDL_GetWindowWMInfo"
        | "SDL_JoystickGetGUID"
        | "setbuf"
        | "strtok"
        | "_getcwd"
        | "_findfirst64i32"
        | "_findnext64i32"
        | "_fstat64i32"
        | "_stat64i32"
        | "_seh_filter_dll"
        | "__std_exception_copy"
        | "ra_x86_command_line_to_argv_w" => 2,

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
        | "ra_x86_load_library_ex_a"
        | "ra_x86_load_library_ex_w"
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
        | "SDL_Vulkan_GetDrawableSize"
        | "ra_x86_gethostbyaddr"
        | "freopen"
        | "_wassert"
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
        // The POSIX names a Linux or Android C library exports them by.
        | "read"
        | "write"
        | "lseek"
        | "_snprintf"
        | "vsprintf"
        | "vfprintf"
        | "_strtoi64"
        | "_strtoui64"
        | "fopen_s"
        | "sprintf_s"
        | "swprintf_s"
        | "_itoa_s"
        | "ra_x86_interlocked_compare_exchange"
        | "ra_x86_delete_timer_queue_timer"
        | "ra_x86_initialize_critical_section_ex" => 3,

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
        | "ra_x86_def_window_proc_a"
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
        | "strftime"
        | "_vsnprintf"
        | "__stdio_common_vsscanf"
        | "_except_handler4_common"
        | "__C_specific_handler"
        | "__CxxFrameHandler3"
        | "__CxxFrameHandler4"
        | "_vsnprintf_s"
        | "_sopen_s"
        | "_get_stream_buffer_pointers"
        | "ra_x86_get_stream_buffer_pointers"
        | "ra_x86_init_once_execute_once"
        | "ra_x86_eh_vector_destructor_iterator"
        | "ra_x86_wait_for_multiple_objects" => 4,

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
        | "__stdio_common_vfwprintf"
        | "__RTDynamicCast"
        | "ra_x86_eh_vector_constructor_iterator"
        | "ra_x86_rt_dynamic_cast" => 5,

        "CreateThread" | "MultiByteToWideChar" | "LCMapStringW" | "CompareStringW" | "__stdio_common_vsprintf" | "_beginthreadex" => 6,
        "CreateFileA" | "CreateFileW" | "FormatMessageA" | "FormatMessageW" | "ra_x86_create_timer_queue_timer" => 7,
        "ra_x86_create_process_a" => 10,
        "WideCharToMultiByte" | "RtlVirtualUnwind" => 8,
        "CreateWindowExA" | "CreateWindowExW" => 12,
        _ => return library(name).map(|(_, params)| params.len()).or_else(|| ra_asm_pe::xbe::kernel_export_named(name)?.arity()),
    })
}

/// A function of the `PlayStation` BIOS, which a program calls by jumping to the entry point of
/// its table (`0xa0`, `0xb0` or `0xc0`) with its number in `t1`: its name, how many of `a0..a3`
/// it reads, and whether it answers in `v0`. A variadic one (`printf`) reads all four registers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BiosCall {
    pub table: u8,
    pub number: u8,
    pub name: &'static str,
    pub params: u8,
    pub returns: bool,
}

impl BiosCall {
    /// The runtime function the lift calls for it: `ra_bios_<name>`.
    pub fn symbol(&self) -> String {
        format!("ra_bios_{}", self.name)
    }

    /// `i64 (i64 ...)` of its register arguments, or `void` when it answers nothing.
    pub fn fn_type(&self) -> FnType {
        FnType::new(if self.returns { Type::I64 } else { Type::Void }, vec![Type::I64; usize::from(self.params)])
    }
}

/// The BIOS functions by their table and number, as the console's kernel documents them (the
/// `A0`, `B0` and `C0` tables): the ones whose arguments are integers or pointers in `a0..a3`.
/// A number not here is no call the lift names; the jump stays the program's.
const PSX_BIOS: &[(u8, u8, &str, u8, bool)] = &[
    // The `syscall` instruction, which is no table's: `a0` picks the critical section's entry
    // (1) or exit (2), and the entry answers whether interrupts were on.
    (0x00, 0x00, PSX_SYSCALL, 1, true),
    (0xa0, 0x00, "FileOpen", 2, true),
    (0xa0, 0x01, "FileSeek", 3, true),
    (0xa0, 0x02, "FileRead", 3, true),
    (0xa0, 0x03, "FileWrite", 3, true),
    (0xa0, 0x04, "FileClose", 1, true),
    (0xa0, 0x06, "exit", 1, false),
    (0xa0, 0x08, "FileGetc", 1, true),
    (0xa0, 0x09, "FilePutc", 2, true),
    (0xa0, 0x0a, "todigit", 1, true),
    (0xa0, 0x0c, "strtoul", 3, true),
    (0xa0, 0x0d, "strtol", 3, true),
    (0xa0, 0x0e, "abs", 1, true),
    (0xa0, 0x0f, "labs", 1, true),
    (0xa0, 0x10, "atoi", 1, true),
    (0xa0, 0x13, PSX_SETJMP, 2, true),
    (0xa0, 0x11, "atol", 1, true),
    (0xa0, 0x15, "strcat", 2, true),
    (0xa0, 0x16, "strncat", 3, true),
    (0xa0, 0x17, "strcmp", 2, true),
    (0xa0, 0x18, "strncmp", 3, true),
    (0xa0, 0x19, "strcpy", 2, true),
    (0xa0, 0x1a, "strncpy", 3, true),
    (0xa0, 0x1b, "strlen", 1, true),
    (0xa0, 0x1c, "index", 2, true),
    (0xa0, 0x1d, "rindex", 2, true),
    (0xa0, 0x1e, "strchr", 2, true),
    (0xa0, 0x1f, "strrchr", 2, true),
    (0xa0, 0x20, "strpbrk", 2, true),
    (0xa0, 0x21, "strspn", 2, true),
    (0xa0, 0x22, "strcspn", 2, true),
    (0xa0, 0x23, "strtok", 2, true),
    (0xa0, 0x24, "strstr", 2, true),
    (0xa0, 0x25, "toupper", 1, true),
    (0xa0, 0x26, "tolower", 1, true),
    (0xa0, 0x27, "bcopy", 3, false),
    (0xa0, 0x28, "bzero", 2, false),
    (0xa0, 0x29, "bcmp", 3, true),
    (0xa0, 0x2a, "memcpy", 3, true),
    (0xa0, 0x2b, "memset", 3, true),
    (0xa0, 0x2c, "memmove", 3, true),
    (0xa0, 0x2d, "memcmp", 3, true),
    (0xa0, 0x2e, "memchr", 3, true),
    (0xa0, 0x2f, "rand", 0, true),
    (0xa0, 0x30, "srand", 1, false),
    (0xa0, 0x33, "malloc", 1, true),
    (0xa0, 0x34, "free", 1, false),
    (0xa0, 0x37, "calloc", 2, true),
    (0xa0, 0x38, "realloc", 2, true),
    (0xa0, 0x39, "InitHeap", 2, false),
    (0xa0, 0x3a, "SystemErrorExit", 1, false),
    (0xa0, 0x3b, "std_in_getchar", 0, true),
    (0xa0, 0x3c, "std_out_putchar", 1, false),
    (0xa0, 0x3d, "std_in_gets", 1, true),
    (0xa0, 0x3e, "std_out_puts", 1, false),
    (0xa0, 0x3f, "printf", 4, true),
    (0xa0, 0x44, "FlushCache", 0, false),
    (0xa0, 0x49, "GPU_cw", 1, false),
    (0xa0, 0x54, "_96_init", 0, false),
    (0xa0, 0x55, "_bu_init", 0, false),
    (0xa0, 0x56, "_96_remove", 0, false),
    (0xa0, 0x70, "_bu_init", 0, false),
    (0xa0, 0x71, "_96_init", 0, false),
    (0xa0, 0x72, "_96_remove", 0, false),
    (0xa0, 0x9f, "SetMemSize", 1, false),
    (0xa0, 0xa1, "SystemErrorBootOrDiskFailure", 2, false),
    (0xa0, 0xab, "_card_info", 1, true),
    (0xa0, 0xac, "_card_async_load_directory", 1, true),
    (0xa0, 0xad, "set_card_auto_format", 1, false),
    (0xa0, 0xaf, "card_write_test", 1, true),
    (0xa0, 0xb4, "GetSystemInfo", 1, true),
    (0xb0, 0x00, "alloc_kernel_memory", 1, true),
    (0xb0, 0x01, "free_kernel_memory", 1, false),
    (0xb0, 0x02, "SetRCnt", 3, true),
    (0xb0, 0x03, "GetRCnt", 1, true),
    (0xb0, 0x04, "StartRCnt", 1, true),
    (0xb0, 0x05, "StopRCnt", 1, true),
    (0xb0, 0x06, "ResetRCnt", 1, true),
    (0xb0, 0x07, "DeliverEvent", 2, false),
    (0xb0, 0x08, "OpenEvent", 4, true),
    (0xb0, 0x09, "CloseEvent", 1, true),
    (0xb0, 0x0a, "WaitEvent", 1, true),
    (0xb0, 0x0b, "TestEvent", 1, true),
    (0xb0, 0x0c, "EnableEvent", 1, true),
    (0xb0, 0x0d, "DisableEvent", 1, true),
    (0xb0, 0x12, "InitPad", 4, true),
    (0xb0, 0x13, "StartPad", 0, true),
    (0xb0, 0x14, "StopPad", 0, false),
    (0xb0, 0x15, "OutdatedPadInitAndStart", 4, true),
    (0xb0, 0x16, "OutdatedPadGetButtons", 0, true),
    (0xb0, 0x17, "ReturnFromException", 0, false),
    (0xb0, 0x18, "SetDefaultExitFromException", 0, false),
    (0xb0, 0x19, "SetCustomExitFromException", 1, false),
    (0xb0, 0x20, "UnDeliverEvent", 2, false),
    (0xb0, 0x32, "FileOpen", 2, true),
    (0xb0, 0x33, "FileSeek", 3, true),
    (0xb0, 0x34, "FileRead", 3, true),
    (0xb0, 0x35, "FileWrite", 3, true),
    (0xb0, 0x36, "FileClose", 1, true),
    (0xb0, 0x38, "exit", 1, false),
    (0xb0, 0x3a, "FileGetc", 1, true),
    (0xb0, 0x3b, "FilePutc", 2, true),
    (0xb0, 0x3c, "std_in_getchar", 0, true),
    (0xb0, 0x3d, "std_out_putchar", 1, false),
    (0xb0, 0x3e, "std_in_gets", 1, true),
    (0xb0, 0x3f, "std_out_puts", 1, false),
    (0xb0, 0x40, "chdir", 1, true),
    (0xb0, 0x41, "FormatDevice", 1, true),
    (0xb0, 0x42, "firstfile", 2, true),
    (0xb0, 0x43, "nextfile", 1, true),
    (0xb0, 0x44, "FileRename", 2, true),
    (0xb0, 0x45, "FileDelete", 1, true),
    (0xb0, 0x46, "FileUndelete", 1, true),
    (0xb0, 0x47, "AddDevice", 1, true),
    (0xb0, 0x48, "RemoveDevice", 1, true),
    (0xb0, 0x49, "PrintInstalledDevices", 0, false),
    (0xb0, 0x4a, "InitCard", 1, false),
    (0xb0, 0x4b, "StartCard", 0, false),
    (0xb0, 0x4c, "StopCard", 0, false),
    (0xb0, 0x4e, "_card_write", 3, true),
    (0xb0, 0x4f, "_card_read", 3, true),
    (0xb0, 0x50, "_new_card", 0, false),
    (0xb0, 0x54, "get_last_error", 0, true),
    (0xb0, 0x58, "_card_chan", 0, true),
    (0xb0, 0x5c, "_card_status", 1, true),
    (0xb0, 0x5d, "_card_wait", 1, true),
    (0xb0, 0x56, "GetC0Table", 0, true),
    (0xb0, 0x57, "GetB0Table", 0, true),
    (0xb0, 0x5b, "ChangeClearPad", 1, false),
    (0xc0, 0x00, "EnqueueTimerAndVblankIrqs", 1, false),
    (0xc0, 0x01, "EnqueueSyscallHandler", 1, false),
    (0xc0, 0x02, "SysEnqIntRP", 2, true),
    (0xc0, 0x03, "SysDeqIntRP", 2, true),
    (0xc0, 0x07, "InstallExceptionHandlers", 0, false),
    (0xc0, 0x08, "SysInitMemory", 2, false),
    (0xc0, 0x0a, "ChangeClearRCnt", 2, true),
    (0xc0, 0x0c, "InitDefInt", 1, false),
    (0xc0, 0x12, "InstallDevices", 1, false),
    (0xc0, 0x13, "FlushStdInOutPut", 0, false),
    (0xc0, 0x1c, "AdjustA0Table", 0, false),
];

/// The BIOS's `setjmp`, whose runtime function takes the buffer and, in place of a second
/// argument, where the caller's registers are: the machine's state.
pub const PSX_SETJMP: &str = "setjmp";

/// The name of the kernel's handler of the `syscall` instruction among the BIOS functions.
const PSX_SYSCALL: &str = "syscall";

/// The kernel's handler of a `PlayStation` program's `syscall`, as a BIOS function.
pub fn psx_syscall() -> BiosCall {
    BiosCall { table: 0, number: 0, name: PSX_SYSCALL, params: 1, returns: true }
}

/// The BIOS function a jump to `entry` (`0xa0`, `0xb0`, `0xc0`) with `number` in `t1` calls.
pub fn psx_bios(entry: u64, number: u64) -> Option<BiosCall> {
    PSX_BIOS
        .iter()
        .find(|&&(t, n, ..)| u64::from(t) == entry && u64::from(n) == number)
        .map(|&(table, number, name, params, returns)| BiosCall { table, number, name, params, returns })
}

/// Every BIOS function of the tables, once by its runtime symbol: the declarations of a
/// `PlayStation` program's module.
pub fn psx_bios_calls() -> Vec<BiosCall> {
    let mut seen = std::collections::HashSet::new();
    PSX_BIOS
        .iter()
        .map(|&(table, number, name, params, returns)| BiosCall { table, number, name, params, returns })
        .filter(|c| seen.insert(c.name))
        .collect()
}

/// A function of the `PlayStation 2`'s EE kernel, which a program calls with `syscall` and its
/// number in `v1`: its name in ps2sdk's `kernel.h`, how many of `a0..a3` and `t0..t3` it reads,
/// and the bits of its answer in `v0` (none, a word the register holds sign-extended, or a
/// doubleword). The handler form of a call (`iSignalSema`) is the negated number of its slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KernelCall {
    pub number: i32,
    pub name: &'static str,
    pub params: u8,
    pub returns: u8,
}

impl KernelCall {
    /// The runtime function the lift calls for it: `ra_ee_<name>`.
    pub fn symbol(&self) -> String {
        format!("{EE_PREFIX}{}", self.name)
    }

    /// `i64 (i64 ...)` of its register arguments, or `void` when it answers nothing.
    pub fn fn_type(&self) -> FnType {
        FnType::new(if self.returns == 0 { Type::Void } else { Type::I64 }, vec![Type::I64; usize::from(self.params)])
    }
}

/// The prefix of an EE kernel function's runtime symbol.
const EE_PREFIX: &str = "ra_ee_";

/// The EE kernel's functions by their number, as ps2sdk's `kernel.h` and `syscallnr.h` declare
/// them: the ones whose arguments are integers or pointers in registers. `_print`, which is
/// variadic, and a number not here are no call the lift names; the `syscall` traps. The second
/// forms of `AddIntcHandler` and `AddDmacHandler` share the slot and pass a fourth argument, which
/// the kernel reads either way.
const EE_KERNEL: &[(i32, &str, u8, u8)] = &[
    (0x01, "ResetEE", 1, 0),
    (0x02, "SetGsCrt", 3, 0),
    (0x04, "KExit", 1, 0),
    (0x06, "_LoadExecPS2", 3, 0),
    (0x07, "_ExecPS2", 4, 32),
    (0x0a, "AddSbusIntcHandler", 2, 32),
    (0x0b, "RemoveSbusIntcHandler", 1, 32),
    (0x0c, "Interrupt2Iop", 1, 32),
    (0x0d, "SetVTLBRefillHandler", 2, 32),
    (0x0e, "SetVCommonHandler", 2, 32),
    (0x0f, "SetVInterruptHandler", 2, 32),
    (0x10, "AddIntcHandler", 4, 32),
    (0x11, "RemoveIntcHandler", 2, 32),
    (0x12, "AddDmacHandler", 4, 32),
    (0x13, "RemoveDmacHandler", 2, 32),
    (0x14, "_EnableIntc", 1, 32),
    (0x15, "_DisableIntc", 1, 32),
    (0x16, "_EnableDmac", 1, 32),
    (0x17, "_DisableDmac", 1, 32),
    (0x18, "_SetAlarm", 3, 32),
    (0x19, "_ReleaseAlarm", 1, 32),
    (-0x1a, "_iEnableIntc", 1, 32),
    (-0x1b, "_iDisableIntc", 1, 32),
    (-0x1c, "_iEnableDmac", 1, 32),
    (-0x1d, "_iDisableDmac", 1, 32),
    (-0x1e, "_iSetAlarm", 3, 32),
    (-0x1f, "_iReleaseAlarm", 1, 32),
    (0x20, "CreateThread", 1, 32),
    (0x21, "DeleteThread", 1, 32),
    (0x22, "StartThread", 2, 32),
    (0x23, "ExitThread", 0, 0),
    (0x24, "ExitDeleteThread", 0, 0),
    (0x25, "TerminateThread", 1, 32),
    (-0x26, "iTerminateThread", 1, 32),
    (0x27, "DisableDispatchThread", 0, 0),
    (0x28, "EnableDispatchThread", 0, 0),
    (0x29, "ChangeThreadPriority", 2, 32),
    (-0x2a, "iChangeThreadPriority", 2, 32),
    (0x2b, "RotateThreadReadyQueue", 1, 32),
    (-0x2c, "_iRotateThreadReadyQueue", 1, 32),
    (0x2d, "ReleaseWaitThread", 1, 32),
    (-0x2e, "iReleaseWaitThread", 1, 32),
    (0x2f, "GetThreadId", 0, 32),
    (-0x2f, "_iGetThreadId", 0, 32),
    (0x30, "ReferThreadStatus", 2, 32),
    (-0x31, "iReferThreadStatus", 2, 32),
    (0x32, "SleepThread", 0, 32),
    (0x33, "WakeupThread", 1, 32),
    (-0x34, "_iWakeupThread", 1, 32),
    (0x35, "CancelWakeupThread", 1, 32),
    (-0x36, "iCancelWakeupThread", 1, 32),
    (0x37, "SuspendThread", 1, 32),
    (-0x38, "_iSuspendThread", 1, 32),
    (0x39, "ResumeThread", 1, 32),
    (-0x3a, "iResumeThread", 1, 32),
    (0x3c, "SetupThread", 5, 32),
    (0x3d, "SetupHeap", 2, 32),
    (0x3e, "EndOfHeap", 0, 32),
    (0x40, "CreateSema", 1, 32),
    (0x41, "DeleteSema", 1, 32),
    (0x42, "SignalSema", 1, 32),
    (-0x43, "iSignalSema", 1, 32),
    (0x44, "WaitSema", 1, 32),
    (0x45, "PollSema", 1, 32),
    (-0x46, "iPollSema", 1, 32),
    (0x47, "ReferSemaStatus", 2, 32),
    (-0x48, "iReferSemaStatus", 2, 32),
    (-0x49, "iDeleteSema", 1, 32),
    (0x4a, "SetOsdConfigParam", 1, 0),
    (0x4b, "GetOsdConfigParam", 1, 0),
    (0x4c, "GetGsHParam", 4, 0),
    (0x4d, "GetGsVParam", 4, 0),
    (0x4e, "SetGsHParam", 4, 0),
    (0x4f, "SetGsVParam", 4, 0),
    (0x56, "_SetTLBEntry", 5, 32),
    (0x5a, "Copy", 3, 32),
    (0x5b, "GetEntryAddress", 1, 32),
    (0x5c, "EnableIntcHandler", 1, 32),
    (0x5d, "DisableIntcHandler", 1, 32),
    (0x5e, "EnableDmacHandler", 1, 32),
    (0x5f, "DisableDmacHandler", 1, 32),
    (0x60, "KSeg0", 1, 0),
    (0x61, "EnableCache", 1, 32),
    (0x62, "DisableCache", 1, 32),
    (0x63, "GetCop0", 1, 32),
    (0x64, "FlushCache", 1, 0),
    (0x66, "CpuConfig", 1, 32),
    (-0x67, "iGetCop0", 1, 32),
    (-0x68, "iFlushCache", 1, 0),
    (-0x6a, "iCpuConfig", 1, 32),
    (0x6b, "SifStopDma", 0, 0),
    (0x6c, "SetCPUTimerHandler", 1, 0),
    (0x6d, "SetCPUTimer", 1, 0),
    (0x6e, "SetOsdConfigParam2", 3, 0),
    (0x6f, "GetOsdConfigParam2", 3, 0),
    (0x70, "GsGetIMR", 0, 64),
    (-0x70, "iGsGetIMR", 0, 64),
    (0x71, "GsPutIMR", 1, 64),
    (-0x71, "iGsPutIMR", 1, 64),
    (0x72, "SetPgifHandler", 1, 0),
    (0x73, "SetVSyncFlag", 2, 0),
    (0x74, "SetSyscall", 2, 0),
    (0x76, "SifDmaStat", 1, 32),
    (-0x76, "iSifDmaStat", 1, 32),
    (0x77, "SifSetDma", 2, 32),
    (-0x77, "iSifSetDma", 2, 32),
    (0x78, "SifSetDChain", 0, 0),
    (-0x78, "iSifSetDChain", 0, 0),
    (0x79, "SifSetReg", 2, 32),
    (0x7a, "SifGetReg", 1, 32),
    (0x7b, "ExecOSD", 2, 0),
    (0x7c, "Deci2Call", 2, 32),
    (0x7d, "PSMode", 0, 0),
    (0x7e, "MachineType", 0, 32),
    (0x7f, "GetMemorySize", 0, 32),
    (0x82, "_InitTLB", 0, 32),
];

fn kernel_call(&(number, name, params, returns): &(i32, &'static str, u8, u8)) -> KernelCall {
    KernelCall { number, name, params, returns }
}

/// The EE kernel function a `syscall` with `number` in `v1` calls.
pub fn ee_syscall(number: i32) -> Option<KernelCall> {
    EE_KERNEL.iter().find(|e| e.0 == number).map(kernel_call)
}

/// The EE kernel function whose runtime symbol is `symbol`, as [`KernelCall::symbol`] makes it.
pub fn ee_syscall_named(symbol: &str) -> Option<KernelCall> {
    let name = symbol.strip_prefix(EE_PREFIX)?;
    EE_KERNEL.iter().find(|e| e.1 == name).map(kernel_call)
}

/// Every EE kernel function of the table: the declarations of a `PlayStation 2` program's module.
pub fn ee_syscalls() -> impl Iterator<Item = KernelCall> {
    EE_KERNEL.iter().map(kernel_call)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The generated table of `ra-asm-protos` read as kinds, and right about the functions the
    /// hand table knows; the table itself, and the test that regenerates it, are that crate's.
    #[test]
    fn the_generated_table_reads_as_kinds() {
        let (returns, params) = win32("WriteFile").expect("WriteFile is in the table");
        assert_eq!((returns, params.len()), (Some(Kind::Int), 5));
        assert_eq!(win32("GetTickCount64").map(|(r, p)| (r, p.len())), Some((Some(Kind::Wide), 0)));
        assert_eq!(win32("SetFilePointerEx").map(|(_, p)| p), Some(vec![Kind::Int, Kind::Wide, Kind::Int, Kind::Int]));
        assert_eq!(win32("ExitProcess").map(|(r, p)| (r, p.len())), Some((None, 1)));
        // The table reaches past the hand one, and agrees with it where both know a function.
        assert_eq!(arity("GetDiskFreeSpaceExA"), Some(4));
        assert_eq!(arity("CreateWindowExW"), Some(12));
        assert_eq!(arity("MultiByteToWideChar"), Some(6));
        assert!(typed("ChildWindowFromPoint").is_some_and(|t| t.params == [Kind::Int, Kind::Wide]));
        assert!(win32("wsprintfA").is_none(), "a variadic function is in the table");
        assert!(win32("printf").is_none());
    }

    /// A 32-bit program's `EncodePointer` and `DecodePointer` go to the runtime's, which keep the
    /// encoding to the dword the program stores it in, and take their one argument.
    #[test]
    fn a_32_bit_programs_pointer_encoding_is_the_runtimes() {
        for (import, shim) in [
            ("EncodePointer", "ra_x86_encode_pointer"),
            ("DecodePointer", "ra_x86_decode_pointer"),
            ("EncodeSystemPointer", "ra_x86_encode_pointer"),
            ("DecodeSystemPointer", "ra_x86_decode_pointer"),
        ] {
            assert_eq!(x86_shim(import), Some(shim), "{import}");
            assert_eq!(arity(shim), Some(1), "{shim}");
        }
    }

    /// SDL's functions take what its headers say, and pop nothing: with the universal eight
    /// arguments for `SDL_GetTicks`, a function that only called it took its caller's stack words
    /// as parameters, and every function above it did.
    #[test]
    fn sdl_functions_take_what_their_headers_say() {
        assert_eq!(arity("SDL_GetTicks"), Some(0));
        assert_eq!(arity("SDL_Delay"), Some(1));
        assert_eq!(arity("SDL_CreateRGBSurface"), Some(8));
        assert_eq!(stdcall_pops("SDL2.dll", "SDL_CreateRGBSurface"), Some(0));
        assert_eq!(stdcall_pops("SDL2_mixer.dll", "Mix_PlayChannelTimed"), Some(0));
        assert!(typed("SDL_SetWindowOpacity").is_some_and(|t| t.params == [Kind::Int, Kind::Float]));
        assert_eq!(arity("SDL_Log"), None, "a variadic function is in the table");
    }

    /// A 32-bit program's `CommandLineToArgvW` is the runtime's, whose table holds dword
    /// pointers: SDL's `main_getcmdline` read the high half of shell32's first pointer as its
    /// second argument, and `SDL_wcslen` read address 0.
    #[test]
    fn a_32_bit_programs_argument_table_is_the_runtimes() {
        assert_eq!(x86_shim("CommandLineToArgvW"), Some("ra_x86_command_line_to_argv_w"));
        assert_eq!(arity("ra_x86_command_line_to_argv_w"), Some(2));
    }

    /// The EE kernel's numbers name the functions of ps2sdk's stubs, a handler form by the negated
    /// number of its slot, and each name is one runtime symbol that reads back as its call.
    #[test]
    fn the_ee_kernel_numbers_name_its_functions() {
        let name = |n: i32| ee_syscall(n).map(|c| c.name);
        assert_eq!(name(0x23), Some("ExitThread"));
        assert_eq!(name(0x64), Some("FlushCache"));
        assert_eq!(name(0x20), Some("CreateThread"));
        assert_eq!(name(0x32), Some("SleepThread"));
        assert_eq!(name(0x02), Some("SetGsCrt"));
        assert_eq!(name(0x77), Some("SifSetDma"));
        assert_eq!(name(-0x43), Some("iSignalSema"));
        assert_eq!(name(-0x2f), Some("_iGetThreadId"));
        assert_eq!(name(0x75), None, "the variadic _print is named");
        let exit = ee_syscall(0x23).unwrap();
        assert_eq!(exit.fn_type(), FnType::new(Type::Void, vec![]));
        assert_eq!(ee_syscall(0x70).unwrap().fn_type(), FnType::new(Type::I64, vec![]));
        let mut names = std::collections::HashSet::new();
        let mut numbers = std::collections::HashSet::new();
        for call in ee_syscalls() {
            assert!(names.insert(call.name) && numbers.insert(call.number), "{call:?} twice");
            assert_eq!(ee_syscall_named(&call.symbol()), Some(call));
        }
        assert_eq!(ee_syscall_named("ExitThread"), None, "a name without the runtime's prefix");
    }
}
