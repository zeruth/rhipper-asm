//! The wrappers of the imports rhtest's sys and win call with a structure that changes with the
//! width: the unions of `OVERLAPPED` and `SYSTEM_INFO` crossed by their members of one layout, the
//! size `VirtualQuery` takes and returns, and the window procedure of `WNDCLASSA` called back
//! through an adapter.

use ra_asm_emit::{ADAPTERS_FILE, COptions, Unit, emit_unit};
use ra_asm_ir::{CallConv, Declaration, FnType, Module, Type};

/// The imports by their name and arity, as the raise declares them from the prototype table.
const IMPORTS: [(&str, usize); 12] = [
    ("ReadFile", 5),
    ("WriteFile", 5),
    ("GetOverlappedResult", 4),
    ("VirtualQuery", 3),
    ("GetSystemInfo", 1),
    ("RegisterClassA", 1),
    ("GetClassInfoA", 3),
    ("WSAStartup", 2),
    ("InitializeCriticalSection", 1),
    ("EnterCriticalSection", 1),
    ("LeaveCriticalSection", 1),
    ("DeleteCriticalSection", 1),
];

/// The unit of a 32-bit program that imports each of `imports`: integers in, an integer out.
fn imports_unit(imports: &[(&str, usize)]) -> Unit {
    let mut module = Module::x86_64_windows("adapters");
    for &(name, arity) in imports {
        let ty = FnType::new(Type::I64, vec![Type::I64; arity]);
        module.declare(Declaration { name: name.to_owned(), ty, conv: CallConv::default(), attrs: Vec::new() });
    }
    emit_unit(&module, &COptions { guest32: true, ..Default::default() })
}

fn assert_has(text: &str, line: &str) {
    assert!(text.contains(line), "no `{line}` in:\n{text}");
}

#[test]
fn union_and_callback_adapters() {
    let unit = imports_unit(&IMPORTS);
    let text = &unit.adapters;
    for (import, _) in IMPORTS {
        assert_has(&unit.externs, &format!("#define {import} ra_adapt_{import}\n"));
        assert_has(text, &format!("ra_adapt_{import}("));
    }
    assert!(!text.contains("does not convert"), "every structure converts:\n{text}");
    assert!(unit.files(ra_asm_emit::FUNCTIONS_PER_FILE).iter().any(|f| f.name == ADAPTERS_FILE && f.text == *text));

    // OVERLAPPED's union crosses by its offset, which keeps its layout; its pointer is read
    // through it.
    assert_has(text, "    ra_widen(host + 0, guest + 0, 4, 8, 0); // Internal\n");
    assert_has(text, "    ra_widen(host + 8, guest + 4, 4, 8, 0); // InternalHigh\n");
    assert_has(text, "    ra_widen(host + 16, guest + 8, 4, 4, 0); // Offset\n");
    assert_has(text, "    ra_widen(host + 20, guest + 12, 4, 4, 0); // OffsetHigh\n");
    assert_has(text, "    ra_widen(host + 24, guest + 16, 4, 8, 0); // hEvent\n");
    assert!(!text.contains("// Pointer\n"), "{text}");
    assert_has(text, "    ra_narrow(guest + 8, host + 16, 4, 4, 0, \"the host's _OVERLAPPED.Offset does not fit in 4 bytes\");\n");
    assert_eq!(text.matches("static void ra_to_host__OVERLAPPED(").count(), 1, "one conversion for its three imports");

    // SYSTEM_INFO's union crosses by its dword.
    assert_has(text, "    ra_widen(host + 0, guest + 0, 4, 4, 0); // dwOemId\n");
    assert_has(text, "    ra_widen(host + 24, guest + 16, 4, 8, 0); // dwActiveProcessorMask\n");

    // MEMORY_BASIC_INFORMATION's PartitionId is the host's alone, and VirtualQuery's length and
    // result are the host's size for the program's.
    assert!(!text.contains("PartitionId"), "{text}");
    assert_has(text, "    ra_widen(host + 24, guest + 12, 4, 8, 0); // RegionSize\n");
    assert_has(text, "    uint64_t result = VirtualQuery(a0, guest1 ? (uint64_t)(uintptr_t)host1 : 0, ra_resize(a2, 28, 48));\n");
    assert_has(text, "    result = ra_resize(result, 48, 28);\n");

    // WSADATA's strings copy as their bytes, and its pointer moves to the front at 64 bits.
    assert_has(text, "    ra_widen(host + 16, guest + 4, 257, 257, 0); // szDescription\n");
    assert_has(text, "    ra_widen(host + 8, guest + 396, 4, 8, 0); // lpVendorInfo\n");

    // CRITICAL_SECTION widens its pointers and its spin count, and keeps its counts.
    assert_has(text, "    ra_widen(host + 8, guest + 4, 4, 4, 0); // LockCount\n");
    assert_has(text, "    ra_widen(host + 32, guest + 20, 4, 8, 0); // SpinCount\n");

    // The class's window procedure goes to the host as an adapter, which narrows the host's
    // arguments, calls the program's procedure and sign-extends its LRESULT; GetClassInfoA hands
    // the program's procedure back for the adapter.
    assert_has(text, "    ra_put(host + 8, 8, ra_callback_to_host_WNDPROC(ra_get(guest + 4, 4))); // lpfnWndProc\n");
    assert_has(
        text,
        "    ra_put(guest + 4, 4, ra_callback_to_guest_WNDPROC(ra_get(host + 8, 8), \"the host's tagWNDCLASSA.lpfnWndProc does not fit in 4 bytes\")); // lpfnWndProc\n",
    );
    assert_eq!(text.matches("static int64_t ra_call_WNDPROC(").count(), 1, "one set of adapters for the callback type");
    assert_has(
        text,
        "    if (a3 != (int32_t)a3) ra_adapt_failed(\"the host's WNDPROC argument 3 does not fit in 4 bytes\", (uint64_t)a3);\n",
    );
    assert_has(text, "    return (int64_t)(int32_t)procedure((uint32_t)a0, (uint32_t)a1, (uint32_t)a2, (uint32_t)a3);\n");
    assert_has(text, "static int64_t ra_callback_WNDPROC_7(uint64_t a0, uint32_t a1, uint64_t a2, int64_t a3) {\n");
    assert_has(text, "// RegisterClassA: tagWNDCLASSA (parameter 0) widened for the host.\n");
    assert_has(text, "// GetClassInfoA: tagWNDCLASSA (parameter 2) widened for the host and narrowed back.\n");

    syntax_check(text);
}

/// The wrappers parse as C and as C++ against the runtime's header, when clang is on the PATH.
fn syntax_check(text: &str) {
    let Some(clang) = std::env::var_os("PATH").and_then(|path| ra_asm_testing::tools::find_in("clang", &path)) else {
        eprintln!("union_and_callback_adapters: clang is not on the PATH, the wrappers' C is not parsed");
        return;
    };
    let dir = ra_asm_testing::Scratch::new("emit-adapters", "sys-and-win");
    let file = dir.join(ADAPTERS_FILE);
    std::fs::write(&file, text).unwrap();
    let runtime = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime");
    for language in ["c", "c++"] {
        let output = std::process::Command::new(&clang)
            .args(["-fsyntax-only", "-x", language, "-I"])
            .arg(&runtime)
            .arg(&file)
            .output()
            .unwrap_or_else(|e| panic!("{}: {e}", clang.display()));
        assert!(output.status.success(), "{language}: {}\n{text}", String::from_utf8_lossy(&output.stderr));
    }
}
