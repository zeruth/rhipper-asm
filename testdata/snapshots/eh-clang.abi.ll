; ModuleID = 'eh-clang.exe'
source_filename = "eh-clang.exe"
target datalayout = "e-m:w-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-pc-windows-msvc"

%State = type { [16 x i64], i64, [8 x i8], [32 x i128] }
%S0 = type { i32, i32, i32 }
%S1 = type { i32, i32, i32, [20 x i8], i32 }
%S2 = type { i32, i32 }
%S3 = type { i32, i32, i32, i32, i32, i32, i32, i32 }
%S4 = type { [6 x i8], i16, [12 x i8], i16 }
%S5 = type { [8 x i8], i32, i32 }
%S6 = type { i32, [20 x i8], i16, [90 x i8], i32, [112 x i8], i32 }
%S7 = type { i32, [12 x i8], i32, i32 }

@ra_x87_stack = global [8 x double] zeroinitializer, align 16
@ra_x87_top = global i64 0, align 8
@ra_x87_cw = global i16 639, align 2
@ra_x87_sw = global i16 0, align 2
@ra_pops = global i64 0, align 8
@ra_mxcsr = global i32 8064, align 4
@sec_headers = constant [1024 x i8] c"MZx\00\01\00\00\00\04\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00@\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00x\00\00\00\0E\1F\BA\0E\00\B4\09\CD!\B8\01L\CD!This program cannot be run in DOS mode.$\00\00PE\00\00L\01\04\00\A4X\09\CF\00\00\00\00\00\00\00\00\E0\00\02\01\0B\01\0E\00\00$\00\00\00\18\00\00\00\00\00\00p(\00\00\00\10\00\00\00\00\00\00\00\00@\00\00\10\00\00\00\02\00\00\06\00\00\00\00\00\00\00\06\00\00\00\00\00\00\00\00p\00\00\00\04\00\00\00\00\00\00\03\00@\81\00\00\10\00\00\10\00\00\00\00\10\00\00\10\00\00\00\00\00\00\10\00\00\00\00\00\00\00\00\00\00\00xC\00\00\A0\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00`\00\00\5C\04\00\00\08C\00\00\1C\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\80A\00\00\C0\00\00\00\00\00\00\00\00\00\00\00\E4D\00\00\CC\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00.text\00\00\00'#\00\00\00\10\00\00\00$\00\00\00\04\00\00\00\00\00\00\00\00\00\00\00\00\00\00 \00\00`.rdata\00\00T\0E\00\00\00@\00\00\00\10\00\00\00(\00\00\00\00\00\00\00\00\00\00\00\00\00\00@\00\00@.data\00\00\00\AC\03\00\00\00P\00\00\00\02\00\00\008\00\00\00\00\00\00\00\00\00\00\00\00\00\00@\00\00\C0.reloc\00\00\5C\04\00\00\00`\00\00\00\06\00\00\00:\00\00\00\00\00\00\00\00\00\00\00\00\00\00@\00\00B\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00", align 16  ; 0x400000
@sec_rdata = constant <{ i32, i32, i32, [16 x i8], i32, i32, [12 x i8], i32, i32, i32, [4 x i8], i32, [20 x i8], i32, [4 x i8], i32, [20 x i8], i32, [12 x i8], i32, i32, [4 x i8], i32, i32, i32, [12 x i8], i32, i32, i32, i32, i32, [12 x i8], i32, i32, [12 x i8], i32, i32, i32, [4 x i8], i32, [20 x i8], i32, [4 x i8], i32, [20 x i8], i32, [12 x i8], i32, i32, [4 x i8], i32, i32, i32, [12 x i8], i32, i32, i32, i32, i32, i32, i32, i32, [64 x i8], i32, i32, [4 x i8], i32, [108 x i8], i32, [16 x i8], i32, i32, [12 x i8], i32, i32, [4 x i8], i32, [20 x i8], i32, [188 x i8], i32, [8 x i8], i32, [8 x i8], i32, i32, [384 x i8], i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, [4 x i8], i32, i32, i32, i32, i32, i32, i32, i32, [4 x i8], i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, [4 x i8], i32, i32, [4 x i8], i32, [4 x i8], i32, [4 x i8], i32, i32, [1112 x i8], i32, [24 x i8], i32, [12 x i8], i32, [16 x i8], i32, [16 x i8], i32, [8 x i8], i32, [16 x i8], i32, [4 x i8], i32, i32, [4 x i8], i32, [4 x i8], i32, [12 x i8], i32, [16 x i8], i32, [8 x i8], i32, [16 x i8], i32, [4 x i8], i32, i32, [4 x i8], i32, [4 x i8], i32, [12 x i8], i32, [24 x i8], i32, [12 x i8], i32, [16 x i8], i32, [24 x i8], i32, [12 x i8], i32, [4 x i8], i32, [4 x i8], i32, [4 x i8], i32, [8 x i8], i32, [28 x i8], i32, [8 x i8], i32, [4 x i8], i32, [48 x i8], i32, [4 x i8], i32, [4 x i8], i32, [4 x i8], i32, [4 x i8], i32, [4 x i8], i32, [4 x i8], i32, [12 x i8], i32, [8 x i8], i32, [28 x i8], i32, [8 x i8], i32, [4 x i8], i32, [64 x i8], i32, [16 x i8], i32, [4 x i8], i32, [4 x i8], i32, [4 x i8], i32, [4 x i8], i32, [8 x i8], i32, [4 x i8], i32, [28 x i8], i32, [12 x i8], i32, [12 x i8], i32, [24 x i8], i32, [16 x i8], i32, [4 x i8], i32, [4 x i8], i32, [4 x i8], i32, [4 x i8], i32, [8 x i8], i32, [4 x i8], i32, [28 x i8], i32, [24 x i8], i32, [4 x i8], i32, [4 x i8], i32, [4 x i8], i32, i32, [8 x i8], i32, [24 x i8], i32, i32, [24 x i8], i32, i32 }> <{ i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 16) to i32), i32 ptrtoint (ptr @U_00401b30 to i32), i32 ptrtoint (ptr @U_00401b10 to i32), [16 x i8] c"\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 16) to i32), i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 36) to i32), [12 x i8] c"\00\00\00\00\00\00\00\00\02\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 52) to i32), i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 64) to i32), i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 96) to i32), [4 x i8] c"\00\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 16) to i32), [20 x i8] c"\01\00\00\00\00\00\00\00\FF\FF\FF\FF\00\00\00\00@\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 36) to i32), [4 x i8] c"\00\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 48) to i32), [20 x i8] c"\00\00\00\00\00\00\00\00\FF\FF\FF\FF\00\00\00\00@\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 124) to i32), [12 x i8] c"\00\00\00\00\00\00\00\00\01\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 140) to i32), i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 96) to i32), [4 x i8] c"\00\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 160) to i32), i32 ptrtoint (ptr @U_00401b30 to i32), i32 ptrtoint (ptr @U_00401b20 to i32), [12 x i8] c"\00\00\00\00\00\00\00\00\00\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 48) to i32), i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 124) to i32), i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 192) to i32), i32 ptrtoint (ptr @U_00401ba0 to i32), i32 ptrtoint (ptr @U_00401b50 to i32), [12 x i8] c"\00\00\00\00\00\00\00\00\00\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 80) to i32), i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 212) to i32), [12 x i8] c"\00\00\00\00\00\00\00\00\02\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 228) to i32), i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 240) to i32), i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 272) to i32), [4 x i8] c"\00\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 80) to i32), [20 x i8] c"\01\00\00\00\00\00\00\00\FF\FF\FF\FF\00\00\00\00@\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 212) to i32), [4 x i8] c"\00\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 112) to i32), [20 x i8] c"\00\00\00\00\00\00\00\00\FF\FF\FF\FF\00\00\00\00@\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 300) to i32), [12 x i8] c"\00\00\00\00\00\00\00\00\01\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 316) to i32), i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 272) to i32), [4 x i8] c"\00\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 336) to i32), i32 ptrtoint (ptr @U_00401b70 to i32), i32 ptrtoint (ptr @U_00401b60 to i32), [12 x i8] c"\00\00\00\00\00\00\00\00\00\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 112) to i32), i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 300) to i32), i32 4198556, i32 4198686, i32 4198592, i32 4198646, i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 576) to i32), i32 ptrtoint (ptr @U_00402540 to i32), [64 x i8] c"\00\00\00\00\C0\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 192) to i32), i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 804) to i32), [4 x i8] c"\08\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 836) to i32), [108 x i8] c"\00\00\00\00\00\00\00\00\00\00\00\00\00\01\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 840) to i32), [16 x i8] c"\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 292) to i32), i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 596) to i32), [12 x i8] c"\00\00\00\00\00\00\00\00\01\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 612) to i32), i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 620) to i32), [4 x i8] c"\00\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 292) to i32), [20 x i8] c"\00\00\00\00\00\00\00\00\FF\FF\FF\FF\00\00\00\00@\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 596) to i32), [188 x i8] c"by_any\00by_int\00rethrows\00events\00ctor\00Unknown exception\00problem\00finally_param\00seh\000123456789abcdef\00by_base\00by_none\00by_std\00nested\00\00\00\00\00\00\00\A4X\09\CF\00\00\00\00\10\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\D0\1B\00\00\E0\1B\00\00\F0\1B\00\00\00\1C\00\00\10\1C\00\00 \1C\00\00p.\00\00\852\00\00", i32 ptrtoint (ptr @U_00402cb0 to i32), [8 x i8] c"\00\00\00\00\00\00\00\00", i32 ptrtoint (ptr @U_00402660 to i32), [8 x i8] c"\00\00\00\00\00\00\00\00", i32 ptrtoint (ptr @U_004025a0 to i32), i32 ptrtoint (ptr @U_00402650 to i32), [384 x i8] c"\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\18D\00\00\00\00\00\00\00\00\00\00,I\00\00\E4D\00\00HD\00\00\00\00\00\00\00\00\00\009I\00\00\14E\00\00lD\00\00\00\00\00\00\00\00\00\00JI\00\008E\00\00\BCD\00\00\00\00\00\00\00\00\00\00lI\00\00\88E\00\00\C8D\00\00\00\00\00\00\00\00\00\00\8CI\00\00\94E\00\00\D0D\00\00\00\00\00\00\00\00\00\00\ABI\00\00\9CE\00\00\D8D\00\00\00\00\00\00\00\00\00\00\CCI\00\00\A4E\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\B0E\00\00\C6E\00\00\DCE\00\00\F0E\00\00\00F\00\00\1AF\00\000F\00\00LF\00\00fF\00\00xF\00\00\96F\00\00\00\00\00\00\A2F\00\00\B8F\00\00\CEF\00\00\E4F\00\00\02G\00\00\1AG\00\004G\00\00HG\00\00\00\00\00\00bG\00\00pG\00\00~G\00\00\88G\00\00\92G\00\00\ACG\00\00\BCG\00\00\CAG\00\00\D2G\00\00\F4G\00\00\16H\00\002H\00\00>H\00\00LH\00\00hH\00\00\96H\00\00\A8H\00\00\B8H\00\00\C0H\00\00\00\00\00\00\CCH\00\00\DCH\00\00\00\00\00\00\EAH\00\00\00\00\00\00\FEH\00\00\00\00\00\00\14I\00\00$I\00\00\00\00\00\00", i32 ptrtoint (ptr @GetCurrentProcessId to i32), i32 ptrtoint (ptr @GetCurrentThreadId to i32), i32 ptrtoint (ptr @ra_x86_get_module_handle_w to i32), i32 ptrtoint (ptr @GetStdHandle to i32), i32 ptrtoint (ptr @GetSystemTimeAsFileTime to i32), i32 ptrtoint (ptr @ra_x86_initialize_slist_head to i32), i32 ptrtoint (ptr @IsProcessorFeaturePresent to i32), i32 ptrtoint (ptr @QueryPerformanceCounter to i32), i32 ptrtoint (ptr @RaiseException to i32), i32 ptrtoint (ptr @SetUnhandledExceptionFilter to i32), i32 ptrtoint (ptr @WriteFile to i32), [4 x i8] c"\00\00\00\00", i32 ptrtoint (ptr @_CxxThrowException to i32), i32 ptrtoint (ptr @__CxxFrameHandler3 to i32), i32 ptrtoint (ptr @__current_exception to i32), i32 ptrtoint (ptr @__current_exception_context to i32), i32 ptrtoint (ptr @__std_exception_copy to i32), i32 ptrtoint (ptr @__std_exception_destroy to i32), i32 ptrtoint (ptr @_except_handler3 to i32), i32 ptrtoint (ptr @_except_handler4_common to i32), [4 x i8] c"\00\00\00\00", i32 ptrtoint (ptr @ra_x86_p_argc to i32), i32 ptrtoint (ptr @ra_x86_p_argv to i32), i32 ptrtoint (ptr @_c_exit to i32), i32 ptrtoint (ptr @_cexit to i32), i32 ptrtoint (ptr @_configure_narrow_argv to i32), i32 ptrtoint (ptr @ra_x86_controlfp_s to i32), i32 ptrtoint (ptr @_crt_atexit to i32), i32 ptrtoint (ptr @_exit to i32), i32 ptrtoint (ptr @ra_x86_initial_narrow_environment to i32), i32 ptrtoint (ptr @_initialize_narrow_environment to i32), i32 ptrtoint (ptr @ra_x86_initialize_onexit_table to i32), i32 ptrtoint (ptr @ra_x86_initterm to i32), i32 ptrtoint (ptr @ra_x86_initterm_e to i32), i32 ptrtoint (ptr @ra_x86_register_onexit_function to i32), i32 ptrtoint (ptr @_register_thread_local_exe_atexit_callback to i32), i32 ptrtoint (ptr @_seh_filter_exe to i32), i32 ptrtoint (ptr @_set_app_type to i32), i32 ptrtoint (ptr @exit to i32), i32 ptrtoint (ptr @terminate to i32), [4 x i8] c"\00\00\00\00", i32 ptrtoint (ptr @ra_x86_p_commode to i32), i32 ptrtoint (ptr @_set_fmode to i32), [4 x i8] c"\00\00\00\00", i32 ptrtoint (ptr @__setusermatherr to i32), [4 x i8] c"\00\00\00\00", i32 ptrtoint (ptr @_configthreadlocale to i32), [4 x i8] c"\00\00\00\00", i32 ptrtoint (ptr @_set_new_mode to i32), i32 ptrtoint (ptr @free to i32), [1112 x i8] c"\00\00\00\00<\02GetCurrentProcessId\00@\02GetCurrentThreadId\00\00\9F\02GetModuleHandleW\00\00\FC\02GetStdHandle\00\00\13\03GetSystemTimeAsFileTime\00\91\03InitializeSListHead\00\B5\03IsProcessorFeaturePresent\00~\04QueryPerformanceCounter\00\94\04RaiseException\00\00\A8\05SetUnhandledExceptionFilter\00O\06WriteFile\00\01\00_CxxThrowException\00\00\10\00__CxxFrameHandler3\00\00\1C\00__current_exception\00\1D\00__current_exception_context\00!\00__std_exception_copy\00\00\22\00__std_exception_destroy\004\00_except_handler3\00\005\00_except_handler4_common\00\05\00__p___argc\00\00\06\00__p___argv\00\00\16\00_c_exit\00\17\00_cexit\00\00\19\00_configure_narrow_argv\00\00\1D\00_controlfp_s\00\00\1F\00_crt_atexit\00%\00_exit\00*\00_get_initial_narrow_environment\005\00_initialize_narrow_environment\00\006\00_initialize_onexit_table\00\008\00_initterm\009\00_initterm_e\00>\00_register_onexit_function\00?\00_register_thread_local_exe_atexit_callback\00\00B\00_seh_filter_exe\00D\00_set_app_type\00X\00exit\00\00j\00terminate\00\01\00__p__commode\00\00T\00_set_fmode\00\00.\00__setusermatherr\00\00\08\00_configthreadlocale\00\16\00_set_new_mode\00\18\00free\00\00KERNEL32.dll\00VCRUNTIME140.dll\00api-ms-win-crt-runtime-l1-1-0.dll\00api-ms-win-crt-stdio-l1-1-0.dll\00api-ms-win-crt-math-l1-1-0.dll\00api-ms-win-crt-locale-l1-1-0.dll\00api-ms-win-crt-heap-l1-1-0.dll\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\01\00\00\00", i32 ptrtoint (ptr @sec_data to i32), [24 x i8] c"\00\00\00\00\FF\FF\FF\FF\00\00\00\00\04\00\00\00\00\00\00\00\01\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 2560) to i32), [12 x i8] c"\00\00\00\00\00\00\00\00\00\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 2588) to i32), [16 x i8] c"\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 16) to i32), [16 x i8] c"\00\00\00\00\FF\FF\FF\FF\00\00\00\00\08\00\00\00", i32 ptrtoint (ptr @U_00401170 to i32), [8 x i8] c"\00\00\00\00\00\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 48) to i32), [16 x i8] c"\00\00\00\00\FF\FF\FF\FF\00\00\00\00\08\00\00\00", i32 ptrtoint (ptr @U_00401190 to i32), [4 x i8] c"\02\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 2624) to i32), i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 2656) to i32), [4 x i8] c"\00\00\00\00", i32 ptrtoint (ptr @U_004011b0 to i32), [4 x i8] c"\00\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 2684) to i32), [12 x i8] c"\00\00\00\00\00\00\00\00\00\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 80) to i32), [16 x i8] c"\00\00\00\00\FF\FF\FF\FF\00\00\00\00\10\00\00\00", i32 ptrtoint (ptr @U_004011c0 to i32), [8 x i8] c"\00\00\00\00\00\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 112) to i32), [16 x i8] c"\00\00\00\00\FF\FF\FF\FF\00\00\00\00\0C\00\00\00", i32 ptrtoint (ptr @U_00401200 to i32), [4 x i8] c"\02\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 2720) to i32), i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 2752) to i32), [4 x i8] c"\00\00\00\00", i32 ptrtoint (ptr @U_00401240 to i32), [4 x i8] c"\00\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 2780) to i32), [12 x i8] c"\00\00\00\00\00\00\00\00\01\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 140) to i32), [24 x i8] c"\00\00\00\00\FF\FF\FF\FF\00\00\00\00\08\00\00\00\00\00\00\00\01\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 2816) to i32), [12 x i8] c"\00\00\00\00\00\00\00\00\00\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 2844) to i32), [16 x i8] c"\00\00\00\00\00\00\00\00\00\00\00\00\01\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 152) to i32), [24 x i8] c"\00\00\00\00\FF\FF\FF\FF\00\00\00\00\04\00\00\00\00\00\00\00\01\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 2880) to i32), [12 x i8] c"\00\00\00\00\00\00\00\00\00\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 2908) to i32), [4 x i8] c"\01\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 2656) to i32), [4 x i8] c"\00\00\00\00", i32 ptrtoint (ptr @U_004011b0 to i32), [4 x i8] c"\00\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 2932) to i32), [8 x i8] c"\22\05\93\19\01\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 2992) to i32), [28 x i8] c"\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\01\00\00\00\FF\FF\FF\FF", i32 4198736, [8 x i8] c"\22\05\93\19\02\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 3036) to i32), [4 x i8] c"\01\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 3052) to i32), [48 x i8] c"\00\00\00\00\00\00\00\00\00\00\00\00\01\00\00\00\FF\FF\FF\FF\00\00\00\00\FF\FF\FF\FF\00\00\00\00\00\00\00\00\00\00\00\00\01\00\00\00\04\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 3072) to i32), [4 x i8] c"\00\00\00\00", i32 ptrtoint (ptr @sec_data to i32), [4 x i8] c"\E8\FF\FF\FF", i32 4199152, [4 x i8] c"\08\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 48) to i32), [4 x i8] c"\E4\FF\FF\FF", i32 4199184, [4 x i8] c"\08\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 112) to i32), [4 x i8] c"\E0\FF\FF\FF", i32 4199248, [12 x i8] c"@\00\00\00\00\00\00\00\00\00\00\00", i32 4199296, [8 x i8] c"\22\05\93\19\01\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 3172) to i32), [28 x i8] c"\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\01\00\00\00\FF\FF\FF\FF", i32 4199504, [8 x i8] c"\22\05\93\19\04\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 3216) to i32), [4 x i8] c"\02\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 3248) to i32), [64 x i8] c"\00\00\00\00\00\00\00\00\00\00\00\00\01\00\00\00\FF\FF\FF\FF\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\FF\FF\FF\FF\00\00\00\00\01\00\00\00\01\00\00\00\02\00\00\00\01\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 3288) to i32), [16 x i8] c"\00\00\00\00\02\00\00\00\03\00\00\00\01\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 3304) to i32), [4 x i8] c"\08\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 80) to i32), [4 x i8] c"\E0\FF\FF\FF", i32 4199696, [4 x i8] c"\08\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 80) to i32), [4 x i8] c"\E4\FF\FF\FF", i32 4199648, [8 x i8] c"\22\05\93\19\07\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 3356) to i32), [4 x i8] c"\02\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 3412) to i32), [28 x i8] c"\00\00\00\00\00\00\00\00\00\00\00\00\01\00\00\00\FF\FF\FF\FF\00\00\00\00\00\00\00\00", i32 4199968, [12 x i8] c"\01\00\00\00\00\00\00\00\02\00\00\00", i32 4199904, [12 x i8] c"\01\00\00\00\00\00\00\00\04\00\00\00", i32 4199936, [24 x i8] c"\FF\FF\FF\FF\00\00\00\00\02\00\00\00\03\00\00\00\05\00\00\00\01\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 3452) to i32), [16 x i8] c"\00\00\00\00\05\00\00\00\06\00\00\00\01\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 3468) to i32), [4 x i8] c"\00\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 152) to i32), [4 x i8] c"\E0\FF\FF\FF", i32 4200048, [4 x i8] c"\00\00\00\00", i32 ptrtoint (ptr @sec_data to i32), [4 x i8] c"\D8\FF\FF\FF", i32 4200000, [8 x i8] c"\22\05\93\19\03\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 3520) to i32), [4 x i8] c"\01\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 3544) to i32), [28 x i8] c"\00\00\00\00\00\00\00\00\00\00\00\00\01\00\00\00\FF\FF\FF\FF\00\00\00\00\00\00\00\00", i32 4200336, [24 x i8] c"\FF\FF\FF\FF\00\00\00\00\00\00\00\00\01\00\00\00\02\00\00\00\01\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 3564) to i32), [4 x i8] c"\08\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 48) to i32), [4 x i8] c"\E0\FF\FF\FF", i32 4200304, [4 x i8] c"\FF\FF\FF\FF", i32 ptrtoint (ptr @U_004018c0 to i32), i32 4200566, [8 x i8] c"\00\00\00\00\00\00\00\00", i32 4200608, [24 x i8] c"\00\00\00\00\FE\FF\FF\FF\00\00\00\00\CC\FF\FF\FF\00\00\00\00\FE\FF\FF\FF", i32 4204547, i32 4204568, [24 x i8] c"\00\00\00\00\FE\FF\FF\FF\00\00\00\00\D8\FF\FF\FF\00\00\00\00\FE\FF\FF\FF", i32 4204914, i32 4204933 }>, align 16  ; 0x404000
@sec_data = global <{ i32, [12 x i8], i32, [28 x i8], i32, [28 x i8], i32, [28 x i8], i32, [24 x i8], i32, [8 x i8], i32, [136 x i8], i32, [644 x i8] }> <{ i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 376) to i32), [12 x i8] c"\00\00\00\00.H\00\00\00\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 376) to i32), [28 x i8] c"\00\00\00\00.?AUDerived@@\00\00\00\00\00\00\00\00\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 376) to i32), [28 x i8] c"\00\00\00\00.?AUBase@@\00\00\00\00\00\00\00\00\00\00\00\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 376) to i32), [28 x i8] c"\00\00\00\00.?AUProblem@@\00\00\00\00\00\00\00\00\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 376) to i32), [24 x i8] c"\00\00\00\00.?AVexception@std@@\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 376) to i32), [8 x i8] c"\00\00\00\00.N\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 376) to i32), [136 x i8] c"\00\00\00\00.I\00\00\07\00\00\00\C5\9D\1C\81\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00N\E6@\BB\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\B1\19\BFDu\98\00\00\FF\FF\FF\FF\01\00\00\00\01\00\00\00\00\00\00\00\FF\FF\FF\FF\FF\FF\FF\FF\01\00\00\00", i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 376) to i32), [644 x i8] c"\00\00\00\00.?AVtype_info@@\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00" }>, align 16  ; 0x405000
@ra_tls = constant <{ i64, i64, i64, i64, i64 }> <{ i64 0, i64 0, i64 0, i64 0, i64 0 }>, align 16
@ra_functions = constant [112 x { i64, ptr }] [{ i64, ptr } { i64 4198400, ptr @U_00401000 }, { i64, ptr } { i64 4198432, ptr @U_00401020 }, { i64, ptr } { i64 4198768, ptr @U_00401170 }, { i64, ptr } { i64 4198800, ptr @U_00401190 }, { i64, ptr } { i64 4198832, ptr @U_004011b0 }, { i64, ptr } { i64 4198848, ptr @U_004011c0 }, { i64, ptr } { i64 4198912, ptr @U_00401200 }, { i64, ptr } { i64 4198976, ptr @U_00401240 }, { i64, ptr } { i64 4199008, ptr @U_00401260 }, { i64, ptr } { i64 4199328, ptr @U_004013a0 }, { i64, ptr } { i64 4199536, ptr @U_00401470 }, { i64, ptr } { i64 4199744, ptr @U_00401540 }, { i64, ptr } { i64 4200112, ptr @U_004016b0 }, { i64, ptr } { i64 4200384, ptr @U_004017c0 }, { i64, ptr } { i64 4200416, ptr @U_004017e0 }, { i64, ptr } { i64 4200640, ptr @U_004018c0 }, { i64, ptr } { i64 4200688, ptr @U_004018f0 }, { i64, ptr } { i64 4200704, ptr @U_00401900 }, { i64, ptr } { i64 4200816, ptr @U_00401970 }, { i64, ptr } { i64 4201232, ptr @U_00401b10 }, { i64, ptr } { i64 4201248, ptr @U_00401b20 }, { i64, ptr } { i64 4201264, ptr @U_00401b30 }, { i64, ptr } { i64 4201296, ptr @U_00401b50 }, { i64, ptr } { i64 4201312, ptr @U_00401b60 }, { i64, ptr } { i64 4201328, ptr @U_00401b70 }, { i64, ptr } { i64 4201376, ptr @U_00401ba0 }, { i64, ptr } { i64 4201424, ptr @U_00401bd0 }, { i64, ptr } { i64 4201440, ptr @U_00401be0 }, { i64, ptr } { i64 4201456, ptr @U_00401bf0 }, { i64, ptr } { i64 4201472, ptr @U_00401c00 }, { i64, ptr } { i64 4201488, ptr @U_00401c10 }, { i64, ptr } { i64 4201504, ptr @U_00401c20 }, { i64, ptr } { i64 4201520, ptr @U_00401c30 }, { i64, ptr } { i64 4201536, ptr @U_00401c40 }, { i64, ptr } { i64 4202608, ptr @U_00402070 }, { i64, ptr } { i64 4202912, ptr @U_004021a0 }, { i64, ptr } { i64 4203824, ptr @U_00402530 }, { i64, ptr } { i64 4203840, ptr @U_00402540 }, { i64, ptr } { i64 4203888, ptr @U_00402570 }, { i64, ptr } { i64 4203904, ptr @U_00402580 }, { i64, ptr } { i64 4203920, ptr @U_00402590 }, { i64, ptr } { i64 4203936, ptr @U_004025a0 }, { i64, ptr } { i64 4204112, ptr @U_00402650 }, { i64, ptr } { i64 4204128, ptr @U_00402660 }, { i64, ptr } { i64 4204160, ptr @U_00402680 }, { i64, ptr } { i64 4204656, ptr @U_00402870 }, { i64, ptr } { i64 4204672, ptr @U_00402880 }, { i64, ptr } { i64 4204752, ptr @U_004028d0 }, { i64, ptr } { i64 4204976, ptr @U_004029b0 }, { i64, ptr } { i64 4205040, ptr @U_004029f0 }, { i64, ptr } { i64 4205072, ptr @U_00402a10 }, { i64, ptr } { i64 4205136, ptr @U_00402a50 }, { i64, ptr } { i64 4205184, ptr @U_00402a80 }, { i64, ptr } { i64 4205360, ptr @U_00402b30 }, { i64, ptr } { i64 4205440, ptr @U_00402b80 }, { i64, ptr } { i64 4205616, ptr @U_00402c30 }, { i64, ptr } { i64 4205632, ptr @U_00402c40 }, { i64, ptr } { i64 4205648, ptr @U_00402c50 }, { i64, ptr } { i64 4205664, ptr @U_00402c60 }, { i64, ptr } { i64 4205680, ptr @U_00402c70 }, { i64, ptr } { i64 4205696, ptr @U_00402c80 }, { i64, ptr } { i64 4205744, ptr @U_00402cb0 }, { i64, ptr } { i64 4205760, ptr @U_00402cc0 }, { i64, ptr } { i64 4205792, ptr @U_00402ce0 }, { i64, ptr } { i64 4205808, ptr @U_00402cf0 }, { i64, ptr } { i64 4205824, ptr @U_00402d00 }, { i64, ptr } { i64 4205840, ptr @U_00402d10 }, { i64, ptr } { i64 4205856, ptr @U_00402d20 }, { i64, ptr } { i64 4205872, ptr @U_00402d30 }, { i64, ptr } { i64 4205952, ptr @U_00402d80 }, { i64, ptr } { i64 4205968, ptr @U_00402d90 }, { i64, ptr } { i64 4205984, ptr @U_00402da0 }, { i64, ptr } { i64 4206000, ptr @U_00402db0 }, { i64, ptr } { i64 4206096, ptr @U_00402e10 }, { i64, ptr } { i64 4206144, ptr @U_00402e40 }, { i64, ptr } { i64 4206192, ptr @U_00402e70 }, { i64, ptr } { i64 4206240, ptr @U_00402ea0 }, { i64, ptr } { i64 4207056, ptr @U_004031d0 }, { i64, ptr } { i64 4207072, ptr @U_004031e0 }, { i64, ptr } { i64 4207201, ptr @U_00403261 }, { i64, ptr } { i64 4207207, ptr @U_00403267 }, { i64, ptr } { i64 4207213, ptr @U_0040326d }, { i64, ptr } { i64 4207219, ptr @U_00403273 }, { i64, ptr } { i64 4207225, ptr @U_00403279 }, { i64, ptr } { i64 4207231, ptr @U_0040327f }, { i64, ptr } { i64 4207237, ptr @U_00403285 }, { i64, ptr } { i64 4207243, ptr @U_0040328b }, { i64, ptr } { i64 4207249, ptr @U_00403291 }, { i64, ptr } { i64 4207255, ptr @U_00403297 }, { i64, ptr } { i64 4207261, ptr @U_0040329d }, { i64, ptr } { i64 4207267, ptr @U_004032a3 }, { i64, ptr } { i64 4207273, ptr @U_004032a9 }, { i64, ptr } { i64 4207279, ptr @U_004032af }, { i64, ptr } { i64 4207285, ptr @U_004032b5 }, { i64, ptr } { i64 4207291, ptr @U_004032bb }, { i64, ptr } { i64 4207297, ptr @U_004032c1 }, { i64, ptr } { i64 4207303, ptr @U_004032c7 }, { i64, ptr } { i64 4207309, ptr @U_004032cd }, { i64, ptr } { i64 4207315, ptr @U_004032d3 }, { i64, ptr } { i64 4207321, ptr @U_004032d9 }, { i64, ptr } { i64 4207327, ptr @U_004032df }, { i64, ptr } { i64 4207333, ptr @U_004032e5 }, { i64, ptr } { i64 4207339, ptr @U_004032eb }, { i64, ptr } { i64 4207345, ptr @U_004032f1 }, { i64, ptr } { i64 4207351, ptr @U_004032f7 }, { i64, ptr } { i64 4207357, ptr @U_004032fd }, { i64, ptr } { i64 4207363, ptr @U_00403303 }, { i64, ptr } { i64 4207369, ptr @U_00403309 }, { i64, ptr } { i64 4207375, ptr @U_0040330f }, { i64, ptr } { i64 4207381, ptr @U_00403315 }, { i64, ptr } { i64 4207387, ptr @U_0040331b }, { i64, ptr } { i64 4207393, ptr @U_00403321 }], align 16
@ra_function_count = constant i64 112, align 8
@ra_import_pops = constant <{ i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64 }> <{ i64 ptrtoint (ptr @GetCurrentProcessId to i64), i64 ptrtoint (ptr @ra_import_names to i64), i64 0, i64 ptrtoint (ptr @GetCurrentThreadId to i64), i64 ptrtoint (ptr getelementptr (i8, ptr @ra_import_names, i64 33) to i64), i64 0, i64 ptrtoint (ptr @GetStdHandle to i64), i64 ptrtoint (ptr getelementptr (i8, ptr @ra_import_names, i64 65) to i64), i64 4, i64 ptrtoint (ptr @GetSystemTimeAsFileTime to i64), i64 ptrtoint (ptr getelementptr (i8, ptr @ra_import_names, i64 91) to i64), i64 4, i64 ptrtoint (ptr @IsProcessorFeaturePresent to i64), i64 ptrtoint (ptr getelementptr (i8, ptr @ra_import_names, i64 128) to i64), i64 4, i64 ptrtoint (ptr @QueryPerformanceCounter to i64), i64 ptrtoint (ptr getelementptr (i8, ptr @ra_import_names, i64 167) to i64), i64 4, i64 ptrtoint (ptr @RaiseException to i64), i64 ptrtoint (ptr getelementptr (i8, ptr @ra_import_names, i64 204) to i64), i64 16, i64 ptrtoint (ptr @SetUnhandledExceptionFilter to i64), i64 ptrtoint (ptr getelementptr (i8, ptr @ra_import_names, i64 232) to i64), i64 4, i64 ptrtoint (ptr @WriteFile to i64), i64 ptrtoint (ptr getelementptr (i8, ptr @ra_import_names, i64 273) to i64), i64 20, i64 ptrtoint (ptr @_CxxThrowException to i64), i64 ptrtoint (ptr getelementptr (i8, ptr @ra_import_names, i64 296) to i64), i64 0, i64 ptrtoint (ptr @__CxxFrameHandler3 to i64), i64 ptrtoint (ptr getelementptr (i8, ptr @ra_import_names, i64 332) to i64), i64 0, i64 ptrtoint (ptr @__current_exception to i64), i64 ptrtoint (ptr getelementptr (i8, ptr @ra_import_names, i64 368) to i64), i64 0, i64 ptrtoint (ptr @__current_exception_context to i64), i64 ptrtoint (ptr getelementptr (i8, ptr @ra_import_names, i64 405) to i64), i64 0, i64 ptrtoint (ptr @__setusermatherr to i64), i64 ptrtoint (ptr getelementptr (i8, ptr @ra_import_names, i64 450) to i64), i64 0, i64 ptrtoint (ptr @__std_exception_copy to i64), i64 ptrtoint (ptr getelementptr (i8, ptr @ra_import_names, i64 498) to i64), i64 0, i64 ptrtoint (ptr @__std_exception_destroy to i64), i64 ptrtoint (ptr getelementptr (i8, ptr @ra_import_names, i64 536) to i64), i64 0, i64 ptrtoint (ptr @_c_exit to i64), i64 ptrtoint (ptr getelementptr (i8, ptr @ra_import_names, i64 577) to i64), i64 0, i64 ptrtoint (ptr @_cexit to i64), i64 ptrtoint (ptr getelementptr (i8, ptr @ra_import_names, i64 619) to i64), i64 0, i64 ptrtoint (ptr @_configthreadlocale to i64), i64 ptrtoint (ptr getelementptr (i8, ptr @ra_import_names, i64 660) to i64), i64 0, i64 ptrtoint (ptr @_configure_narrow_argv to i64), i64 ptrtoint (ptr getelementptr (i8, ptr @ra_import_names, i64 713) to i64), i64 0, i64 ptrtoint (ptr @_crt_atexit to i64), i64 ptrtoint (ptr getelementptr (i8, ptr @ra_import_names, i64 770) to i64), i64 0, i64 ptrtoint (ptr @_except_handler3 to i64), i64 ptrtoint (ptr getelementptr (i8, ptr @ra_import_names, i64 816) to i64), i64 0, i64 ptrtoint (ptr @_except_handler4_common to i64), i64 ptrtoint (ptr getelementptr (i8, ptr @ra_import_names, i64 850) to i64), i64 0, i64 ptrtoint (ptr @_exit to i64), i64 ptrtoint (ptr getelementptr (i8, ptr @ra_import_names, i64 891) to i64), i64 0, i64 ptrtoint (ptr @_initialize_narrow_environment to i64), i64 ptrtoint (ptr getelementptr (i8, ptr @ra_import_names, i64 931) to i64), i64 0, i64 ptrtoint (ptr @_register_thread_local_exe_atexit_callback to i64), i64 ptrtoint (ptr getelementptr (i8, ptr @ra_import_names, i64 996) to i64), i64 0, i64 ptrtoint (ptr @_seh_filter_exe to i64), i64 ptrtoint (ptr getelementptr (i8, ptr @ra_import_names, i64 1073) to i64), i64 0, i64 ptrtoint (ptr @_set_app_type to i64), i64 ptrtoint (ptr getelementptr (i8, ptr @ra_import_names, i64 1123) to i64), i64 0, i64 ptrtoint (ptr @_set_fmode to i64), i64 ptrtoint (ptr getelementptr (i8, ptr @ra_import_names, i64 1171) to i64), i64 0, i64 ptrtoint (ptr @_set_new_mode to i64), i64 ptrtoint (ptr getelementptr (i8, ptr @ra_import_names, i64 1214) to i64), i64 0, i64 ptrtoint (ptr @exit to i64), i64 ptrtoint (ptr getelementptr (i8, ptr @ra_import_names, i64 1259) to i64), i64 0, i64 ptrtoint (ptr @free to i64), i64 ptrtoint (ptr getelementptr (i8, ptr @ra_import_names, i64 1298) to i64), i64 0, i64 ptrtoint (ptr @ra_x86_controlfp_s to i64), i64 0, i64 0, i64 ptrtoint (ptr @ra_x86_get_module_handle_w to i64), i64 0, i64 0, i64 ptrtoint (ptr @ra_x86_initial_narrow_environment to i64), i64 0, i64 0, i64 ptrtoint (ptr @ra_x86_initialize_onexit_table to i64), i64 0, i64 0, i64 ptrtoint (ptr @ra_x86_initialize_slist_head to i64), i64 0, i64 0, i64 ptrtoint (ptr @ra_x86_initterm to i64), i64 0, i64 0, i64 ptrtoint (ptr @ra_x86_initterm_e to i64), i64 0, i64 0, i64 ptrtoint (ptr @ra_x86_p_argc to i64), i64 0, i64 0, i64 ptrtoint (ptr @ra_x86_p_argv to i64), i64 0, i64 0, i64 ptrtoint (ptr @ra_x86_p_commode to i64), i64 0, i64 0, i64 ptrtoint (ptr @ra_x86_register_onexit_function to i64), i64 0, i64 0, i64 ptrtoint (ptr @terminate to i64), i64 ptrtoint (ptr getelementptr (i8, ptr @ra_import_names, i64 1334) to i64), i64 0, i64 0, i64 0, i64 0 }>, align 16
@ra_import_names = constant [1378 x i8] c"KERNEL32.dll\00GetCurrentProcessId\00KERNEL32.dll\00GetCurrentThreadId\00KERNEL32.dll\00GetStdHandle\00KERNEL32.dll\00GetSystemTimeAsFileTime\00KERNEL32.dll\00IsProcessorFeaturePresent\00KERNEL32.dll\00QueryPerformanceCounter\00KERNEL32.dll\00RaiseException\00KERNEL32.dll\00SetUnhandledExceptionFilter\00KERNEL32.dll\00WriteFile\00VCRUNTIME140.dll\00_CxxThrowException\00VCRUNTIME140.dll\00__CxxFrameHandler3\00VCRUNTIME140.dll\00__current_exception\00VCRUNTIME140.dll\00__current_exception_context\00api-ms-win-crt-math-l1-1-0.dll\00__setusermatherr\00VCRUNTIME140.dll\00__std_exception_copy\00VCRUNTIME140.dll\00__std_exception_destroy\00api-ms-win-crt-runtime-l1-1-0.dll\00_c_exit\00api-ms-win-crt-runtime-l1-1-0.dll\00_cexit\00api-ms-win-crt-locale-l1-1-0.dll\00_configthreadlocale\00api-ms-win-crt-runtime-l1-1-0.dll\00_configure_narrow_argv\00api-ms-win-crt-runtime-l1-1-0.dll\00_crt_atexit\00VCRUNTIME140.dll\00_except_handler3\00VCRUNTIME140.dll\00_except_handler4_common\00api-ms-win-crt-runtime-l1-1-0.dll\00_exit\00api-ms-win-crt-runtime-l1-1-0.dll\00_initialize_narrow_environment\00api-ms-win-crt-runtime-l1-1-0.dll\00_register_thread_local_exe_atexit_callback\00api-ms-win-crt-runtime-l1-1-0.dll\00_seh_filter_exe\00api-ms-win-crt-runtime-l1-1-0.dll\00_set_app_type\00api-ms-win-crt-stdio-l1-1-0.dll\00_set_fmode\00api-ms-win-crt-heap-l1-1-0.dll\00_set_new_mode\00api-ms-win-crt-runtime-l1-1-0.dll\00exit\00api-ms-win-crt-heap-l1-1-0.dll\00free\00api-ms-win-crt-runtime-l1-1-0.dll\00terminate\00", align 16
@ra_proc_pops = constant [1 x i8] c"\00", align 16
@ra_args = global [16 x i8] zeroinitializer, align 16

declare void @ra_trap(i64, i64) noreturn
declare void @ra_call(ptr, i64)
declare i64 @ra_pops_of(i64)
declare void @ra_cpuid(i64, i64, ptr)
declare i64 @ra_xgetbv(i64)
declare i64 @ra_rdtsc()
declare i64 @ra_teb()
declare void @ra_f80_load(ptr, ptr)
declare void @ra_f80_store(ptr, ptr)
declare void @ra_f80_pack(ptr, i64, i64)
declare void @ra_f80_from_f64(ptr, double)
declare void @ra_f80_from_i64(ptr, i64)
declare double @ra_f80_to_f64(ptr, i64)
declare float @ra_f80_to_f32(ptr, i64)
declare i64 @ra_f80_to_int(ptr, i64, i64)
declare void @ra_f80_add(ptr, ptr, ptr, i64)
declare void @ra_f80_sub(ptr, ptr, ptr, i64)
declare void @ra_f80_mul(ptr, ptr, ptr, i64)
declare void @ra_f80_div(ptr, ptr, ptr, i64)
declare void @ra_f80_sqrt(ptr, ptr, i64)
declare void @ra_f80_rndint(ptr, ptr, i64)
declare void @ra_f80_chs(ptr, ptr)
declare void @ra_f80_abs(ptr, ptr)
declare i64 @ra_f80_cmp(ptr, ptr)
declare void @ra_unsupported(i64, i64) noreturn
declare i8 @llvm.ctpop.i8(i8)
declare i8 @llvm.fshl.i8(i8, i8, i8)
declare i8 @llvm.fshr.i8(i8, i8, i8)
declare i8 @llvm.bswap.i8(i8)
declare i8 @llvm.cttz.i8(i8, i1)
declare i8 @llvm.ctlz.i8(i8, i1)
declare i16 @llvm.fshl.i16(i16, i16, i16)
declare i16 @llvm.fshr.i16(i16, i16, i16)
declare i16 @llvm.bswap.i16(i16)
declare i16 @llvm.ctpop.i16(i16)
declare i16 @llvm.cttz.i16(i16, i1)
declare i16 @llvm.ctlz.i16(i16, i1)
declare i32 @llvm.fshl.i32(i32, i32, i32)
declare i32 @llvm.fshr.i32(i32, i32, i32)
declare i32 @llvm.bswap.i32(i32)
declare i32 @llvm.ctpop.i32(i32)
declare i32 @llvm.cttz.i32(i32, i1)
declare i32 @llvm.ctlz.i32(i32, i1)
declare i64 @llvm.fshl.i64(i64, i64, i64)
declare i64 @llvm.fshr.i64(i64, i64, i64)
declare i64 @llvm.bswap.i64(i64)
declare i64 @llvm.ctpop.i64(i64)
declare i64 @llvm.cttz.i64(i64, i1)
declare i64 @llvm.ctlz.i64(i64, i1)
declare float @llvm.sqrt.f32(float)
declare float @llvm.fabs.f32(float)
declare float @llvm.minnum.f32(float, float)
declare float @llvm.maxnum.f32(float, float)
declare float @llvm.trunc.f32(float)
declare float @llvm.floor.f32(float)
declare float @llvm.ceil.f32(float)
declare float @llvm.rint.f32(float)
declare double @llvm.sqrt.f64(double)
declare double @llvm.fabs.f64(double)
declare double @llvm.minnum.f64(double, double)
declare double @llvm.maxnum.f64(double, double)
declare double @llvm.trunc.f64(double)
declare double @llvm.floor.f64(double)
declare double @llvm.ceil.f64(double)
declare double @llvm.rint.f64(double)
declare void @ra_eh_throw(i64, i64)
declare void @ra_eh_throw_type(i64, i64, i64)
declare void @ra_eh_rethrow()
declare i64 @ra_eh_pending()
declare i64 @ra_eh_catches(i64, i64, i64)
declare void @ra_eh_enter_catch()
declare void @ra_eh_leave_catch()
declare void @ra_eh_suspend()
declare void @ra_eh_resume()
declare void @ra_eh_throw_destructor(i64)
declare void @ra_eh_throw_copy(i64)
declare i64 @ra_eh_ending_object()
declare i64 @ra_eh_ending_destructor()
declare void @ra_eh_catch_sp(i64)
declare i64 @ra_eh_ending_sp()
declare i64 @ra_eh_probe(i64, i64, i64, i64)
declare void @ra_eh_raise(i64, i64, i64, i64, i64)
declare i64 @ra_eh_pointers()
declare void @ra_eh_handled()
declare void @ra_eh_unhandled(i64)
declare i64 @GetCurrentProcessId()
declare i64 @GetCurrentThreadId()
declare i64 @GetStdHandle(i64)
declare i64 @GetSystemTimeAsFileTime(i64)
declare i64 @IsProcessorFeaturePresent(i64)
declare i64 @QueryPerformanceCounter(i64)
declare i64 @RaiseException(i64, i64, i64, i64)
declare i64 @SetUnhandledExceptionFilter(i64)
declare i64 @WriteFile(i64, i64, i64, i64, i64)
declare i64 @_CxxThrowException(i64, i64)
declare i64 @__CxxFrameHandler3(i64, i64, i64, i64)
declare i64 @__current_exception()
declare i64 @__current_exception_context()
declare i64 @__setusermatherr(i64)
declare i64 @__std_exception_copy(i64, i64, i64, i64, i64, i64, i64, i64)
declare i64 @__std_exception_destroy(i64, i64, i64, i64, i64, i64, i64, i64)
declare i64 @_c_exit()
declare i64 @_cexit()
declare i64 @_configthreadlocale(i64)
declare i64 @_configure_narrow_argv(i64)
declare i64 @_crt_atexit(i64)
declare i64 @_exit(i64)
declare i64 @_initialize_narrow_environment()
declare i64 @_register_thread_local_exe_atexit_callback(i64)
declare i64 @_seh_filter_exe(i64, i64)
declare i64 @_set_app_type(i64)
declare i64 @_set_fmode(i64)
declare i64 @_set_new_mode(i64)
declare i64 @exit(i64)
declare i64 @free(i64)
declare i64 @ra_x86_controlfp_s(i64, i64, i64)
declare i64 @ra_x86_get_module_handle_w(i64)
declare i64 @ra_x86_initial_narrow_environment()
declare i64 @ra_x86_initialize_onexit_table(i64)
declare i64 @ra_x86_initialize_slist_head(i64)
declare i64 @ra_x86_initterm(i64, i64)
declare i64 @ra_x86_initterm_e(i64, i64)
declare i64 @ra_x86_p_argc()
declare i64 @ra_x86_p_argv()
declare i64 @ra_x86_p_commode()
declare i64 @ra_x86_register_onexit_function(i64, i64)
declare i64 @terminate()

define i64 @F_00401000(i32 %arg0) {  ; 0x401000
b_401000:  ; 0x401000
  %local_p4 = alloca i32, align 4
  store i32 %arg0, ptr %local_p4, align 4
  %addr.z = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 316) to i32) to i64  ; 0x401000
  %m.p = inttoptr i64 %addr.z to ptr  ; 0x401000
  %m = load i32, ptr %m.p, align 1  ; 0x401000
  %eax.z = zext i32 %m to i64  ; 0x401000
  %shifted.w = shl i32 %m, 5  ; 0x401007
  %diff = sub i32 %shifted.w, %m  ; 0x40100a
  %sum = add i32 %diff, %arg0  ; 0x40100c
  store i32 %sum, ptr %m.p, align 1  ; 0x401010
  ret i64 %eax.z
}

define i64 @F_00401020(i32 %arg0) {  ; 0x401020
b_401020:  ; 0x401020
  %frame = alloca [4220 x i8], align 16
  %frame.top = getelementptr inbounds i8, ptr %frame, i64 4140
  %rsp0 = ptrtoint ptr %frame.top to i64
  %stack.arg0.p = getelementptr inbounds i8, ptr %frame.top, i64 4
  store i32 %arg0, ptr %stack.arg0.p, align 4
  %ebp = trunc i64 undef to i32  ; 0x401020
  %esp = trunc i64 %rsp0 to i32  ; 0x401020
  %sp = sub i32 %esp, 4  ; 0x401020
  %st.p = getelementptr inbounds i8, ptr %frame.top, i64 -4  ; 0x401020
  store i32 %ebp, ptr %st.p, align 1  ; 0x401020
  %sp.1 = sub i32 %sp, 4  ; 0x401023
  %st.p.1 = getelementptr inbounds i8, ptr %frame.top, i64 -8  ; 0x401023
  store i32 %ebp, ptr %st.p.1, align 1  ; 0x401023
  %sp.2 = sub i32 %sp.1, 4  ; 0x401024
  %st.p.2 = getelementptr inbounds i8, ptr %frame.top, i64 -12  ; 0x401024
  store i32 %ebp, ptr %st.p.2, align 1  ; 0x401024
  %sp.3 = sub i32 %sp.2, 4  ; 0x401025
  %st.p.3 = getelementptr inbounds i8, ptr %frame.top, i64 -16  ; 0x401025
  store i32 %ebp, ptr %st.p.3, align 1  ; 0x401025
  %logic = and i32 %sp.3, -8  ; 0x401026
  %diff = sub i32 %logic, 64  ; 0x401029
  %addr = add i32 %diff, 40  ; 0x40102e
  %addr.z.4 = zext i32 %addr to i64  ; 0x40102e
  %st.p.4 = inttoptr i64 %addr.z.4 to ptr  ; 0x40102e
  store i32 %sp, ptr %st.p.4, align 1  ; 0x40102e
  %m = load i32, ptr %stack.arg0.p, align 1  ; 0x401031
  %addr.2 = add i32 %diff, 44  ; 0x401034
  %addr.z.6 = zext i32 %addr.2 to i64  ; 0x401034
  %st.p.5 = inttoptr i64 %addr.z.6 to ptr  ; 0x401034
  store i32 %diff, ptr %st.p.5, align 1  ; 0x401034
  %addr.3 = add i32 %diff, 56  ; 0x401037
  %addr.z.7 = zext i32 %addr.3 to i64  ; 0x401037
  %st.p.6 = inttoptr i64 %addr.z.7 to ptr  ; 0x401037
  store i32 -1, ptr %st.p.6, align 1  ; 0x401037
  %addr.4 = add i32 %diff, 48  ; 0x40103e
  %addr.5 = add i32 %diff, 52  ; 0x401041
  %addr.z.8 = zext i32 %addr.5 to i64  ; 0x401041
  %st.p.7 = inttoptr i64 %addr.z.8 to ptr  ; 0x401041
  store i32 ptrtoint (ptr @U_00401bd0 to i32), ptr %st.p.7, align 1  ; 0x401041
  %teb = call i64 @ra_teb()  ; 0x401048
  %teb.1 = trunc i64 %teb to i32  ; 0x401048
  %addr.z.9 = zext i32 %teb.1 to i64  ; 0x401048
  %m.p.1 = inttoptr i64 %addr.z.9 to ptr  ; 0x401048
  %m.1 = load i32, ptr %m.p.1, align 1  ; 0x401048
  %addr.z.10 = zext i32 %addr.4 to i64  ; 0x40104f
  %st.p.8 = inttoptr i64 %addr.z.10 to ptr  ; 0x40104f
  store i32 %m.1, ptr %st.p.8, align 1  ; 0x40104f
  %teb.2 = call i64 @ra_teb()  ; 0x401052
  %teb.3 = trunc i64 %teb.2 to i32  ; 0x401052
  %addr.z.11 = zext i32 %teb.3 to i64  ; 0x401052
  %st.p.9 = inttoptr i64 %addr.z.11 to ptr  ; 0x401052
  store i32 %addr.4, ptr %st.p.9, align 1  ; 0x401052
  %addr.7 = add i32 %m, 100  ; 0x401058
  %sp.4 = sub i32 %diff, 4  ; 0x40105b
  %addr.z.12 = zext i32 %sp.4 to i64  ; 0x40105b
  %st.p.10 = inttoptr i64 %addr.z.12 to ptr  ; 0x40105b
  store i32 %addr.7, ptr %st.p.10, align 1  ; 0x40105b
  %sp.5 = sub i32 %sp.4, 4  ; 0x40105c
  %sarg0.a = add i32 %sp.5, 4
  %ret = call i64 @F_00401000(i32 %addr.7)  ; 0x40105c
  %eh.pending = call i64 @ra_eh_pending()  ; 0x40105c
  %eh.thrown = icmp ne i64 %eh.pending, 0  ; 0x40105c
  br i1 %eh.thrown, label %eh.landing, label %eh.ok.40105c
b_40106c:  ; 0x40106c
  %sp.18 = sub i32 %sum.4, 4  ; 0x40106c
  %addr.z.50 = zext i32 %sp.18 to i64  ; 0x40106c
  %st.p.30 = inttoptr i64 %addr.z.50 to ptr  ; 0x40106c
  store i32 %m, ptr %st.p.30, align 1  ; 0x40106c
  %sp.19 = sub i32 %sp.18, 4  ; 0x40106d
  %sarg0.a.1 = add i32 %sp.19, 4
  %ret.1 = call i64 @F_00401000(i32 %m)  ; 0x40106d
  %eh.pending.3 = call i64 @ra_eh_pending()  ; 0x40106d
  %eh.thrown.3 = icmp ne i64 %eh.pending.3, 0  ; 0x40106d
  br i1 %eh.thrown.3, label %eh.landing, label %eh.ok.40106d
b_401095:  ; 0x401095
  switch i32 %addr.14, label %switch.bad [ i32 0, label %b_40109c i32 1, label %b_40111e i32 2, label %b_4010c0 i32 3, label %b_4010f6 ]
b_40109c:  ; 0x40109c
  %addr.18 = add i32 %diff, 4  ; 0x40109c
  %addr.z.62 = zext i32 %addr.18 to i64  ; 0x40109c
  %st.p.35 = inttoptr i64 %addr.z.62 to ptr  ; 0x40109c
  store i32 10, ptr %st.p.35, align 1  ; 0x40109c
  store i32 0, ptr %st.p.6, align 1  ; 0x4010a3
  %sp.27 = sub i32 %sum.4, 4  ; 0x4010ad
  %addr.z.64 = zext i32 %sp.27 to i64  ; 0x4010ad
  %st.p.37 = inttoptr i64 %addr.z.64 to ptr  ; 0x4010ad
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 2596) to i32), ptr %st.p.37, align 1  ; 0x4010ad
  %sp.28 = sub i32 %sp.27, 4  ; 0x4010b2
  %addr.z.65 = zext i32 %sp.28 to i64  ; 0x4010b2
  %st.p.38 = inttoptr i64 %addr.z.65 to ptr  ; 0x4010b2
  store i32 %addr.18, ptr %st.p.38, align 1  ; 0x4010b2
  call void @ra_eh_throw(i64 %addr.z.62, i64 4)  ; 0x4010b3
  call void @ra_eh_throw_type(i64 565892279632451611, i64 0, i64 4)  ; 0x4010b3
  br label %eh.landing
b_4010c0:  ; 0x4010c0
  %addr.21 = add i32 %diff, 32  ; 0x4010c0
  %addr.z.67 = zext i32 %addr.21 to i64  ; 0x4010c0
  %st.p.39 = inttoptr i64 %addr.z.67 to ptr  ; 0x4010c0
  store i32 0, ptr %st.p.39, align 1  ; 0x4010c0
  %addr.22 = add i32 %diff, 28  ; 0x4010c7
  %addr.z.68 = zext i32 %addr.22 to i64  ; 0x4010c7
  %st.p.40 = inttoptr i64 %addr.z.68 to ptr  ; 0x4010c7
  store i32 0, ptr %st.p.40, align 1  ; 0x4010c7
  %addr.23 = add i32 %diff, 24  ; 0x4010ce
  %addr.z.69 = zext i32 %addr.23 to i64  ; 0x4010ce
  %st.p.41 = inttoptr i64 %addr.z.69 to ptr  ; 0x4010ce
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 184) to i32), ptr %st.p.41, align 1  ; 0x4010ce
  %addr.24 = add i32 %diff, 36  ; 0x4010d5
  %addr.z.70 = zext i32 %addr.24 to i64  ; 0x4010d5
  %st.p.42 = inttoptr i64 %addr.z.70 to ptr  ; 0x4010d5
  store i32 3, ptr %st.p.42, align 1  ; 0x4010d5
  store i32 0, ptr %st.p.6, align 1  ; 0x4010dc
  %sp.29 = sub i32 %sum.4, 4  ; 0x4010e6
  %addr.z.72 = zext i32 %sp.29 to i64  ; 0x4010e6
  %st.p.44 = inttoptr i64 %addr.z.72 to ptr  ; 0x4010e6
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 2792) to i32), ptr %st.p.44, align 1  ; 0x4010e6
  %sp.30 = sub i32 %sp.29, 4  ; 0x4010eb
  %addr.z.73 = zext i32 %sp.30 to i64  ; 0x4010eb
  %st.p.45 = inttoptr i64 %addr.z.73 to ptr  ; 0x4010eb
  store i32 %addr.23, ptr %st.p.45, align 1  ; 0x4010eb
  call void @ra_eh_throw(i64 %addr.z.69, i64 16)  ; 0x4010ec
  call void @ra_eh_throw_destructor(i64 4198976)  ; 0x4010ec
  call void @ra_eh_throw_type(i64 7474415530116504195, i64 0, i64 16)  ; 0x4010ec
  call void @ra_eh_throw_copy(i64 4198848)  ; 0x4010ec
  call void @ra_eh_throw_type(i64 -5932227318556162533, i64 0, i64 12)  ; 0x4010ec
  call void @ra_eh_throw_copy(i64 4198912)  ; 0x4010ec
  br label %eh.landing
b_4010f6:  ; 0x4010f6
  %addr.27 = add i32 %diff, 12  ; 0x4010f6
  %addr.z.75 = zext i32 %addr.27 to i64  ; 0x4010f6
  %st.p.46 = inttoptr i64 %addr.z.75 to ptr  ; 0x4010f6
  store i32 1074003968, ptr %st.p.46, align 1  ; 0x4010f6
  %addr.28 = add i32 %diff, 8  ; 0x4010fd
  %addr.z.76 = zext i32 %addr.28 to i64  ; 0x4010fd
  %st.p.47 = inttoptr i64 %addr.z.76 to ptr  ; 0x4010fd
  store i32 0, ptr %st.p.47, align 1  ; 0x4010fd
  store i32 0, ptr %st.p.6, align 1  ; 0x401104
  %sp.31 = sub i32 %sum.4, 4  ; 0x40110e
  %addr.z.78 = zext i32 %sp.31 to i64  ; 0x40110e
  %st.p.49 = inttoptr i64 %addr.z.78 to ptr  ; 0x40110e
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 2852) to i32), ptr %st.p.49, align 1  ; 0x40110e
  %sp.32 = sub i32 %sp.31, 4  ; 0x401113
  %addr.z.79 = zext i32 %sp.32 to i64  ; 0x401113
  %st.p.50 = inttoptr i64 %addr.z.79 to ptr  ; 0x401113
  store i32 %addr.28, ptr %st.p.50, align 1  ; 0x401113
  call void @ra_eh_throw(i64 %addr.z.76, i64 8)  ; 0x401114
  call void @ra_eh_throw_type(i64 565898876702220877, i64 0, i64 8)  ; 0x401114
  br label %eh.landing
b_40111e:  ; 0x40111e
  %addr.31 = add i32 %diff, 20  ; 0x40111e
  %addr.z.81 = zext i32 %addr.31 to i64  ; 0x40111e
  %st.p.51 = inttoptr i64 %addr.z.81 to ptr  ; 0x40111e
  store i32 14, ptr %st.p.51, align 1  ; 0x40111e
  %addr.32 = add i32 %diff, 16  ; 0x401125
  %addr.z.82 = zext i32 %addr.32 to i64  ; 0x401125
  %st.p.52 = inttoptr i64 %addr.z.82 to ptr  ; 0x401125
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 4) to i32), ptr %st.p.52, align 1  ; 0x401125
  store i32 0, ptr %st.p.6, align 1  ; 0x40112c
  %sp.33 = sub i32 %sum.4, 4  ; 0x401136
  %addr.z.84 = zext i32 %sp.33 to i64  ; 0x401136
  %st.p.54 = inttoptr i64 %addr.z.84 to ptr  ; 0x401136
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 2696) to i32), ptr %st.p.54, align 1  ; 0x401136
  %sp.34 = sub i32 %sp.33, 4  ; 0x40113b
  %addr.z.85 = zext i32 %sp.34 to i64  ; 0x40113b
  %st.p.55 = inttoptr i64 %addr.z.85 to ptr  ; 0x40113b
  store i32 %addr.32, ptr %st.p.55, align 1  ; 0x40113b
  call void @ra_eh_throw(i64 %addr.z.82, i64 8)  ; 0x40113c
  call void @ra_eh_throw_destructor(i64 4198832)  ; 0x40113c
  call void @ra_eh_throw_type(i64 7498712998977201317, i64 0, i64 8)  ; 0x40113c
  call void @ra_eh_throw_copy(i64 4198768)  ; 0x40113c
  call void @ra_eh_throw_type(i64 -1082765518775566335, i64 0, i64 8)  ; 0x40113c
  call void @ra_eh_throw_copy(i64 4198800)  ; 0x40113c
  br label %eh.landing
eh.landing:  ; 0x40105c
  %rax.phi = phi i64 [ %ret, %b_401020 ], [ %ret.1, %b_40106c ], [ %addr.z.62, %b_40109c ], [ %addr.z.69, %b_4010c0 ], [ %addr.z.76, %b_4010f6 ], [ %addr.z.82, %b_40111e ], [ %ret.2, %eh.ok.40106d ]  ; 0x40105c
  %rsp.phi = phi i32 [ %sarg0.a, %b_401020 ], [ %sarg0.a.1, %b_40106c ], [ %sp.28, %b_40109c ], [ %sp.30, %b_4010c0 ], [ %sp.32, %b_4010f6 ], [ %sp.34, %b_40111e ], [ %sarg0.a.2, %eh.ok.40106d ]  ; 0x40105c
  %teb.4 = call i64 @ra_teb()  ; 0x40105c
  %teb.5 = trunc i64 %teb.4 to i32  ; 0x40105c
  %addr.z.14 = zext i32 %teb.5 to i64  ; 0x40105c
  %eh.head.p = inttoptr i64 %addr.z.14 to ptr  ; 0x40105c
  %eh.head = load i32, ptr %eh.head.p, align 1  ; 0x40105c
  %eh.head.s.w = zext i32 %eh.head to i64  ; 0x40105c
  %eh.head.s = inttoptr i64 %eh.head.s.w to ptr  ; 0x40105c
  %eh.frame = sub i32 %eh.head, 48  ; 0x40105c
  %eh.reg = add i32 %eh.frame, 48  ; 0x40105c
  %eh.head.f4 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 1  ; 0x40105c
  %eh.handler = load i32, ptr %eh.head.f4, align 1  ; 0x40105c
  %eh.ours = icmp eq i32 %eh.handler, 4201424  ; 0x40105c
  %eh.ours.1 = icmp eq i32 %eh.handler, ptrtoint (ptr @U_00401bd0 to i32)  ; 0x40105c
  %eh.ours.2 = or i1 %eh.ours, %eh.ours.1  ; 0x40105c
  br i1 %eh.ours.2, label %eh.dispatch, label %eh.foreign
eh.dispatch:  ; 0x40105c
  %eh.head.f8 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x40105c
  %eh.state = load i32, ptr %eh.head.f8, align 1  ; 0x40105c
  switch i32 %eh.state, label %eh.state.bad [ i32 -1, label %eh.state_none i32 0, label %eh.state0 ]
eh.foreign:  ; 0x40105c
  call void @ra_trap(i64 4198432, i64 8)  ; 0x40105c
  unreachable
eh.state.bad:  ; 0x40105c
  call void @ra_trap(i64 4198432, i64 7)  ; 0x40105c
  unreachable
eh.state_none:  ; 0x40105c
  call void @ra_eh_suspend()  ; 0x40105c
  %eh.head.f8.1 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x40105c
  store i32 -1, ptr %eh.head.f8.1, align 1  ; 0x40105c
  call void @ra_eh_resume()  ; 0x40105c
  %eh.head.f0 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 0  ; 0x40105c
  %eh.next = load i32, ptr %eh.head.f0, align 1  ; 0x40105c
  %teb.6 = call i64 @ra_teb()  ; 0x40105c
  %teb.7 = trunc i64 %teb.6 to i32  ; 0x40105c
  %addr.z.19 = zext i32 %teb.7 to i64  ; 0x40105c
  %st.p.13 = inttoptr i64 %addr.z.19 to ptr  ; 0x40105c
  store i32 %eh.next, ptr %st.p.13, align 1  ; 0x40105c
  ret i64 %rax.phi
eh.state0:  ; 0x40105c
  call void @ra_eh_suspend()  ; 0x40105c
  %eh.head.f8.2 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x40105c
  store i32 -1, ptr %eh.head.f8.2, align 1  ; 0x40105c
  %eh.frame.1 = add i32 %eh.reg, 12  ; 0x40105c
  %sp.6 = sub i32 %rsp.phi, 4  ; 0x40105c
  %addr.z.21 = zext i32 %sp.6 to i64  ; 0x40105c
  %st.p.15 = inttoptr i64 %addr.z.21 to ptr  ; 0x40105c
  store i32 0, ptr %st.p.15, align 1  ; 0x40105c
  br label %f1_401150
eh.cleaned.401150:  ; 0x40105c
  %eh.head.f8.3 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x40116c
  store i32 -1, ptr %eh.head.f8.3, align 1  ; 0x40116c
  call void @ra_eh_resume()  ; 0x40116c
  %eh.head.f0.1 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 0  ; 0x40116c
  %eh.next.3 = load i32, ptr %eh.head.f0.1, align 1  ; 0x40116c
  %teb.14 = call i64 @ra_teb()  ; 0x40116c
  %teb.15 = trunc i64 %teb.14 to i32  ; 0x40116c
  %addr.z.49 = zext i32 %teb.15 to i64  ; 0x40116c
  %st.p.29 = inttoptr i64 %addr.z.49 to ptr  ; 0x40116c
  store i32 %eh.next.3, ptr %st.p.29, align 1  ; 0x40116c
  ret i64 %ret.3
f1_401150:  ; 0x401150
  %sp.7 = sub i32 %sp.6, 4  ; 0x401150
  %addr.z.22 = zext i32 %sp.7 to i64  ; 0x401150
  %st.p.16 = inttoptr i64 %addr.z.22 to ptr  ; 0x401150
  store i32 %eh.frame.1, ptr %st.p.16, align 1  ; 0x401150
  %diff.1 = sub i32 %sp.7, 8  ; 0x401151
  %addr.8 = add i32 %eh.frame.1, -60  ; 0x401154
  %addr.9 = add i32 %addr.8, 40  ; 0x401157
  %addr.z.23 = zext i32 %addr.9 to i64  ; 0x401157
  %m.p.2 = inttoptr i64 %addr.z.23 to ptr  ; 0x401157
  %m.2 = load i32, ptr %m.p.2, align 1  ; 0x401157
  %addr.10 = add i32 %m.2, 8  ; 0x40115a
  %addr.z.24 = zext i32 %addr.10 to i64  ; 0x40115a
  %m.p.3 = inttoptr i64 %addr.z.24 to ptr  ; 0x40115a
  %m.3 = load i32, ptr %m.p.3, align 1  ; 0x40115a
  %sum = add i32 %m.3, 200  ; 0x40115d
  %sp.8 = sub i32 %diff.1, 4  ; 0x401162
  %addr.z.25 = zext i32 %sp.8 to i64  ; 0x401162
  %st.p.17 = inttoptr i64 %addr.z.25 to ptr  ; 0x401162
  store i32 %sum, ptr %st.p.17, align 1  ; 0x401162
  %sp.9 = sub i32 %sp.8, 4  ; 0x401163
  %sarg0.a.3 = add i32 %sp.9, 4
  %ret.3 = call i64 @F_00401000(i32 %sum)  ; 0x401163
  %eh.pending.1 = call i64 @ra_eh_pending()  ; 0x401163
  %eh.thrown.1 = icmp ne i64 %eh.pending.1, 0  ; 0x401163
  br i1 %eh.thrown.1, label %eh.landing.1, label %eh.ok.401163.1
eh.landing.1:  ; 0x401163
  %rax.phi.1 = phi i64 [ %ret.3, %f1_401150 ], [ %ret.4, %f2_401150 ]  ; 0x401163
  %rsp.phi.1 = phi i32 [ %sarg0.a.3, %f1_401150 ], [ %sarg0.a.4, %f2_401150 ]  ; 0x401163
  %teb.8 = call i64 @ra_teb()  ; 0x401163
  %teb.9 = trunc i64 %teb.8 to i32  ; 0x401163
  %addr.z.27 = zext i32 %teb.9 to i64  ; 0x401163
  %eh.head.p.1 = inttoptr i64 %addr.z.27 to ptr  ; 0x401163
  %eh.head.1 = load i32, ptr %eh.head.p.1, align 1  ; 0x401163
  %eh.head.1.s.w = zext i32 %eh.head.1 to i64  ; 0x401163
  %eh.head.1.s = inttoptr i64 %eh.head.1.s.w to ptr  ; 0x401163
  %eh.frame.2 = sub i32 %eh.head.1, 48  ; 0x401163
  %eh.reg.1 = add i32 %eh.frame.2, 48  ; 0x401163
  %eh.head.1.f4 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 1  ; 0x401163
  %eh.handler.1 = load i32, ptr %eh.head.1.f4, align 1  ; 0x401163
  %eh.ours.3 = icmp eq i32 %eh.handler.1, 4201424  ; 0x401163
  %eh.ours.4 = icmp eq i32 %eh.handler.1, ptrtoint (ptr @U_00401bd0 to i32)  ; 0x401163
  %eh.ours.5 = or i1 %eh.ours.3, %eh.ours.4  ; 0x401163
  br i1 %eh.ours.5, label %eh.dispatch.1, label %eh.foreign.1
eh.dispatch.1:  ; 0x401163
  %eh.head.1.f8 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x401163
  %eh.state.1 = load i32, ptr %eh.head.1.f8, align 1  ; 0x401163
  switch i32 %eh.state.1, label %eh.state.bad.1 [ i32 -1, label %eh.state_none.1 i32 0, label %eh.state0.1 ]
eh.foreign.1:  ; 0x401163
  call void @ra_trap(i64 4198432, i64 8)  ; 0x401163
  unreachable
eh.state.bad.1:  ; 0x401163
  call void @ra_trap(i64 4198432, i64 7)  ; 0x401163
  unreachable
eh.state_none.1:  ; 0x401163
  call void @ra_eh_suspend()  ; 0x401163
  %eh.head.1.f8.1 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x401163
  store i32 -1, ptr %eh.head.1.f8.1, align 1  ; 0x401163
  call void @ra_eh_resume()  ; 0x401163
  %eh.head.1.f0 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 0  ; 0x401163
  %eh.next.1 = load i32, ptr %eh.head.1.f0, align 1  ; 0x401163
  %teb.10 = call i64 @ra_teb()  ; 0x401163
  %teb.11 = trunc i64 %teb.10 to i32  ; 0x401163
  %addr.z.32 = zext i32 %teb.11 to i64  ; 0x401163
  %st.p.20 = inttoptr i64 %addr.z.32 to ptr  ; 0x401163
  store i32 %eh.next.1, ptr %st.p.20, align 1  ; 0x401163
  ret i64 %rax.phi.1
eh.state0.1:  ; 0x401163
  call void @ra_eh_suspend()  ; 0x401163
  %eh.head.1.f8.2 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x401163
  store i32 -1, ptr %eh.head.1.f8.2, align 1  ; 0x401163
  %eh.frame.3 = add i32 %eh.reg.1, 12  ; 0x401163
  %sp.10 = sub i32 %rsp.phi.1, 4  ; 0x401163
  %addr.z.34 = zext i32 %sp.10 to i64  ; 0x401163
  %st.p.22 = inttoptr i64 %addr.z.34 to ptr  ; 0x401163
  store i32 0, ptr %st.p.22, align 1  ; 0x401163
  br label %f2_401150
eh.cleaned.401150.1:  ; 0x401163
  %eh.head.1.f8.3 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x40116c
  store i32 -1, ptr %eh.head.1.f8.3, align 1  ; 0x40116c
  call void @ra_eh_resume()  ; 0x40116c
  %eh.head.1.f0.1 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 0  ; 0x40116c
  %eh.next.2 = load i32, ptr %eh.head.1.f0.1, align 1  ; 0x40116c
  %teb.12 = call i64 @ra_teb()  ; 0x40116c
  %teb.13 = trunc i64 %teb.12 to i32  ; 0x40116c
  %addr.z.44 = zext i32 %teb.13 to i64  ; 0x40116c
  %st.p.27 = inttoptr i64 %addr.z.44 to ptr  ; 0x40116c
  store i32 %eh.next.2, ptr %st.p.27, align 1  ; 0x40116c
  ret i64 %ret.4
f2_401150:  ; 0x401150
  %sp.11 = sub i32 %sp.10, 4  ; 0x401150
  %addr.z.35 = zext i32 %sp.11 to i64  ; 0x401150
  %st.p.23 = inttoptr i64 %addr.z.35 to ptr  ; 0x401150
  store i32 %eh.frame.3, ptr %st.p.23, align 1  ; 0x401150
  %diff.2 = sub i32 %sp.11, 8  ; 0x401151
  %addr.11 = add i32 %eh.frame.3, -60  ; 0x401154
  %addr.12 = add i32 %addr.11, 40  ; 0x401157
  %addr.z.36 = zext i32 %addr.12 to i64  ; 0x401157
  %m.p.4 = inttoptr i64 %addr.z.36 to ptr  ; 0x401157
  %m.4 = load i32, ptr %m.p.4, align 1  ; 0x401157
  %addr.13 = add i32 %m.4, 8  ; 0x40115a
  %addr.z.37 = zext i32 %addr.13 to i64  ; 0x40115a
  %m.p.5 = inttoptr i64 %addr.z.37 to ptr  ; 0x40115a
  %m.5 = load i32, ptr %m.p.5, align 1  ; 0x40115a
  %sum.1 = add i32 %m.5, 200  ; 0x40115d
  %sp.12 = sub i32 %diff.2, 4  ; 0x401162
  %addr.z.38 = zext i32 %sp.12 to i64  ; 0x401162
  %st.p.24 = inttoptr i64 %addr.z.38 to ptr  ; 0x401162
  store i32 %sum.1, ptr %st.p.24, align 1  ; 0x401162
  %sp.13 = sub i32 %sp.12, 4  ; 0x401163
  %sarg0.a.4 = add i32 %sp.13, 4
  %ret.4 = call i64 @F_00401000(i32 %sum.1)  ; 0x401163
  %eh.pending.2 = call i64 @ra_eh_pending()  ; 0x401163
  %eh.thrown.2 = icmp ne i64 %eh.pending.2, 0  ; 0x401163
  br i1 %eh.thrown.2, label %eh.landing.1, label %eh.ok.401163
eh.ok.401163:  ; 0x401163
  br label %eh.cleaned.401150.1
eh.ok.401163.1:  ; 0x40116c
  br label %eh.cleaned.401150
eh.ok.40105c:  ; 0x40116c
  %sum.4 = add i32 %sarg0.a, 4  ; 0x401061
  %addr.14 = add i32 %m, -1  ; 0x401064
  %be = icmp ule i32 %addr.14, 3  ; 0x40106a
  br i1 %be, label %b_401095, label %b_40106c
eh.ok.40106d:  ; 0x40106d
  %sum.5 = add i32 %sarg0.a.1, 4  ; 0x401072
  %sum.6 = add i32 %m, 200  ; 0x401075
  %sp.20 = sub i32 %sum.5, 4  ; 0x40107b
  %addr.z.52 = zext i32 %sp.20 to i64  ; 0x40107b
  %st.p.32 = inttoptr i64 %addr.z.52 to ptr  ; 0x40107b
  store i32 %sum.6, ptr %st.p.32, align 1  ; 0x40107b
  %sp.21 = sub i32 %sp.20, 4  ; 0x40107c
  %sarg0.a.2 = add i32 %sp.21, 4
  %ret.2 = call i64 @F_00401000(i32 %sum.6)  ; 0x40107c
  %eh.pending.4 = call i64 @ra_eh_pending()  ; 0x40107c
  %eh.thrown.4 = icmp ne i64 %eh.pending.4, 0  ; 0x40107c
  br i1 %eh.thrown.4, label %eh.landing, label %eh.ok.40107c
eh.ok.40107c:  ; 0x40107c
  %m.6 = load i32, ptr %st.p.8, align 1  ; 0x401084
  %eax.z.7 = zext i32 %m.6 to i64  ; 0x401084
  %teb.16 = call i64 @ra_teb()  ; 0x401087
  %teb.17 = trunc i64 %teb.16 to i32  ; 0x401087
  %addr.z.55 = zext i32 %teb.17 to i64  ; 0x401087
  %st.p.34 = inttoptr i64 %addr.z.55 to ptr  ; 0x401087
  store i32 %m.6, ptr %st.p.34, align 1  ; 0x401087
  ret i64 %eax.z.7
switch.bad:  ; 0x401095
  call void @ra_trap(i64 4198549, i64 7)  ; 0x401095
  unreachable
}

define i64 @F_00401170(i64 %arg0, i64 %arg1) {  ; 0x401170
b_401170:  ; 0x401170
  %local_p4 = alloca i32, align 4
  %stack.arg0.lo = trunc i64 %arg1 to i32
  store i32 %stack.arg0.lo, ptr %local_p4, align 4
  %ecx = trunc i64 %arg0 to i32  ; 0x401170
  %eax.z = zext i32 %ecx to i64  ; 0x401170
  %st.p = inttoptr i64 %eax.z to ptr  ; 0x401176
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 152) to i32), ptr %st.p, align 1  ; 0x401176
  %addr.1 = add i32 %stack.arg0.lo, 4  ; 0x40117c
  %addr.z.2 = zext i32 %addr.1 to i64  ; 0x40117c
  %m.p.1 = inttoptr i64 %addr.z.2 to ptr  ; 0x40117c
  %m.1 = load i32, ptr %m.p.1, align 1  ; 0x40117c
  %addr.2 = add i32 %ecx, 4  ; 0x40117f
  %addr.z.3 = zext i32 %addr.2 to i64  ; 0x40117f
  %st.p.1 = inttoptr i64 %addr.z.3 to ptr  ; 0x40117f
  store i32 %m.1, ptr %st.p.1, align 1  ; 0x40117f
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 4) to i32), ptr %st.p, align 1  ; 0x401182
  ret i64 %eax.z
}

define i64 @F_00401190(i64 %arg0, i64 %arg1) {  ; 0x401190
b_401190:  ; 0x401190
  %local_p4 = alloca i32, align 4
  %stack.arg0.lo = trunc i64 %arg1 to i32
  store i32 %stack.arg0.lo, ptr %local_p4, align 4
  %ecx = trunc i64 %arg0 to i32  ; 0x401190
  %eax.z = zext i32 %ecx to i64  ; 0x401190
  %st.p = inttoptr i64 %eax.z to ptr  ; 0x401196
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 152) to i32), ptr %st.p, align 1  ; 0x401196
  %addr.1 = add i32 %stack.arg0.lo, 4  ; 0x40119c
  %addr.z.2 = zext i32 %addr.1 to i64  ; 0x40119c
  %m.p.1 = inttoptr i64 %addr.z.2 to ptr  ; 0x40119c
  %m.1 = load i32, ptr %m.p.1, align 1  ; 0x40119c
  %addr.2 = add i32 %ecx, 4  ; 0x40119f
  %addr.z.3 = zext i32 %addr.2 to i64  ; 0x40119f
  %st.p.1 = inttoptr i64 %addr.z.3 to ptr  ; 0x40119f
  store i32 %m.1, ptr %st.p.1, align 1  ; 0x40119f
  ret i64 %eax.z
}

define void @F_004011b0() {  ; 0x4011b0
b_4011b0:  ; 0x4011b0
  ret void
}

define i64 @F_004011c0(i64 %arg0, i64 %arg1, i64 %arg2, i64 %arg3) {  ; 0x4011c0
b_4011c0:  ; 0x4011c0
  %local_m10 = alloca i32, align 4
  %local_mc = alloca i32, align 4
  %local_m8 = alloca i32, align 4
  %local_m4 = alloca i32, align 4
  %local_p0 = alloca i32, align 4
  %local_p4 = alloca i32, align 4
  %local_p8 = alloca i32, align 4
  %local_pc = alloca i32, align 4
  %stack.arg0.lo = trunc i64 %arg1 to i32
  store i32 %stack.arg0.lo, ptr %local_p4, align 4
  %stack.arg1.lo = trunc i64 %arg2 to i32
  store i32 %stack.arg1.lo, ptr %local_p8, align 4
  %stack.arg2.lo = trunc i64 %arg3 to i32
  store i32 %stack.arg2.lo, ptr %local_pc, align 4
  %edi = trunc i64 undef to i32  ; 0x4011c0
  store i32 %edi, ptr %local_m4, align 1  ; 0x4011c0
  store i32 %edi, ptr %local_m8, align 1  ; 0x4011c1
  %ecx = trunc i64 %arg0 to i32  ; 0x4011c2
  %addr.z.3 = zext i32 %ecx to i64  ; 0x4011c8
  %st.p.2 = inttoptr i64 %addr.z.3 to ptr  ; 0x4011c8
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 328) to i32), ptr %st.p.2, align 1  ; 0x4011c8
  %addr.1 = add i32 %ecx, 4  ; 0x4011ce
  %addr.2 = add i32 %ecx, 8  ; 0x4011d1
  %addr.z.4 = zext i32 %addr.2 to i64  ; 0x4011d1
  %st.p.3 = inttoptr i64 %addr.z.4 to ptr  ; 0x4011d1
  store i32 0, ptr %st.p.3, align 1  ; 0x4011d1
  %addr.z.5 = zext i32 %addr.1 to i64  ; 0x4011d8
  %st.p.4 = inttoptr i64 %addr.z.5 to ptr  ; 0x4011d8
  store i32 0, ptr %st.p.4, align 1  ; 0x4011d8
  %addr.4 = add i32 %stack.arg0.lo, 4  ; 0x4011df
  %ecx.z = zext i32 %addr.4 to i64  ; 0x4011df
  store i32 %addr.1, ptr %local_mc, align 1  ; 0x4011e2
  store i32 %addr.4, ptr %local_m10, align 1  ; 0x4011e3
  %sarg2.d = load i32, ptr %local_m8, align 4
  %sarg2 = zext i32 %sarg2.d to i64
  %sarg3.d = load i32, ptr %local_m4, align 4
  %sarg3 = zext i32 %sarg3.d to i64
  %sarg4.d = load i32, ptr %local_p0, align 4
  %sarg4 = zext i32 %sarg4.d to i64
  %sarg5.d = load i32, ptr %local_p4, align 4
  %sarg5 = zext i32 %sarg5.d to i64
  %sarg6.d = load i32, ptr %local_p8, align 4
  %sarg6 = zext i32 %sarg6.d to i64
  %sarg7.d = load i32, ptr %local_pc, align 4
  %sarg7 = zext i32 %sarg7.d to i64
  %ret = call i64 @thunk___std_exception_copy(i64 %ecx.z, i64 %ecx.z, i64 %addr.z.5, i64 %sarg2, i64 %sarg3, i64 %sarg4, i64 %sarg5, i64 %sarg6, i64 %sarg7)  ; 0x4011e4
  %eh.pending = call i64 @ra_eh_pending()  ; 0x4011e4
  %eh.thrown = icmp ne i64 %eh.pending, 0  ; 0x4011e4
  br i1 %eh.thrown, label %eh.landing, label %eh.ok.4011e4
eh.landing:  ; 0x4011e4
  ret i64 %ret
eh.ok.4011e4:  ; 0x4011e4
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 184) to i32), ptr %st.p.2, align 1  ; 0x4011ec
  %addr.5 = add i32 %stack.arg0.lo, 12  ; 0x4011f2
  %addr.z.10 = zext i32 %addr.5 to i64  ; 0x4011f2
  %m.p.1 = inttoptr i64 %addr.z.10 to ptr  ; 0x4011f2
  %m.1 = load i32, ptr %m.p.1, align 1  ; 0x4011f2
  %addr.6 = add i32 %ecx, 12  ; 0x4011f5
  %addr.z.11 = zext i32 %addr.6 to i64  ; 0x4011f5
  %st.p.9 = inttoptr i64 %addr.z.11 to ptr  ; 0x4011f5
  store i32 %m.1, ptr %st.p.9, align 1  ; 0x4011f5
  ret i64 %addr.z.3
}

define i64 @F_00401200(i64 %arg0, i64 %arg1, i64 %arg2, i64 %arg3, i64 %arg4) {  ; 0x401200
b_401200:  ; 0x401200
  %local_mc = alloca i32, align 4
  %local_m8 = alloca i32, align 4
  %local_m4 = alloca i32, align 4
  %local_p0 = alloca i32, align 4
  %local_p4 = alloca i32, align 4
  %local_p8 = alloca i32, align 4
  %local_pc = alloca i32, align 4
  %local_p10 = alloca i32, align 4
  %stack.arg0.lo = trunc i64 %arg1 to i32
  store i32 %stack.arg0.lo, ptr %local_p4, align 4
  %stack.arg1.lo = trunc i64 %arg2 to i32
  store i32 %stack.arg1.lo, ptr %local_p8, align 4
  %stack.arg2.lo = trunc i64 %arg3 to i32
  store i32 %stack.arg2.lo, ptr %local_pc, align 4
  %stack.arg3.lo = trunc i64 %arg4 to i32
  store i32 %stack.arg3.lo, ptr %local_p10, align 4
  %esi = trunc i64 undef to i32  ; 0x401200
  store i32 %esi, ptr %local_m4, align 1  ; 0x401200
  %ecx = trunc i64 %arg0 to i32  ; 0x401201
  %addr.z.2 = zext i32 %ecx to i64  ; 0x401207
  %st.p.1 = inttoptr i64 %addr.z.2 to ptr  ; 0x401207
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 328) to i32), ptr %st.p.1, align 1  ; 0x401207
  %sum = add i32 %ecx, 4  ; 0x40120d
  %ecx.z = zext i32 %sum to i64  ; 0x40120d
  %addr.1 = add i32 %ecx, 8  ; 0x401210
  %addr.z.3 = zext i32 %addr.1 to i64  ; 0x401210
  %st.p.2 = inttoptr i64 %addr.z.3 to ptr  ; 0x401210
  store i32 0, ptr %st.p.2, align 1  ; 0x401210
  %st.p.3 = inttoptr i64 %ecx.z to ptr  ; 0x401217
  store i32 0, ptr %st.p.3, align 1  ; 0x401217
  %sum.1 = add i32 %stack.arg0.lo, 4  ; 0x40121e
  store i32 %sum, ptr %local_m8, align 1  ; 0x401221
  store i32 %sum.1, ptr %local_mc, align 1  ; 0x401222
  %sarg0 = zext i32 %sum.1 to i64
  %sarg2.d = load i32, ptr %local_m4, align 4
  %sarg2 = zext i32 %sarg2.d to i64
  %sarg3.d = load i32, ptr %local_p0, align 4
  %sarg3 = zext i32 %sarg3.d to i64
  %sarg4.d = load i32, ptr %local_p4, align 4
  %sarg4 = zext i32 %sarg4.d to i64
  %sarg5.d = load i32, ptr %local_p8, align 4
  %sarg5 = zext i32 %sarg5.d to i64
  %sarg6.d = load i32, ptr %local_pc, align 4
  %sarg6 = zext i32 %sarg6.d to i64
  %sarg7.d = load i32, ptr %local_p10, align 4
  %sarg7 = zext i32 %sarg7.d to i64
  %ret = call i64 @thunk___std_exception_copy(i64 %ecx.z, i64 %sarg0, i64 %ecx.z, i64 %sarg2, i64 %sarg3, i64 %sarg4, i64 %sarg5, i64 %sarg6, i64 %sarg7)  ; 0x401223
  %eh.pending = call i64 @ra_eh_pending()  ; 0x401223
  %eh.thrown = icmp ne i64 %eh.pending, 0  ; 0x401223
  br i1 %eh.thrown, label %eh.landing, label %eh.ok.401223
eh.landing:  ; 0x401223
  ret i64 %ret
eh.ok.401223:  ; 0x401223
  ret i64 %addr.z.2
}

define void @F_00401240(i64 %arg0, i64 %arg1, i64 %arg2, i64 %arg3, i64 %arg4, i64 %arg5, i64 %arg6) {  ; 0x401240
b_401240:  ; 0x401240
  %local_m4 = alloca i32, align 4
  %local_p0 = alloca i32, align 4
  %local_p4 = alloca i32, align 4
  %local_p8 = alloca i32, align 4
  %local_pc = alloca i32, align 4
  %local_p10 = alloca i32, align 4
  %local_p14 = alloca i32, align 4
  %local_p18 = alloca i32, align 4
  %stack.arg0.lo = trunc i64 %arg1 to i32
  store i32 %stack.arg0.lo, ptr %local_p4, align 4
  %stack.arg1.lo = trunc i64 %arg2 to i32
  store i32 %stack.arg1.lo, ptr %local_p8, align 4
  %stack.arg2.lo = trunc i64 %arg3 to i32
  store i32 %stack.arg2.lo, ptr %local_pc, align 4
  %stack.arg3.lo = trunc i64 %arg4 to i32
  store i32 %stack.arg3.lo, ptr %local_p10, align 4
  %stack.arg4.lo = trunc i64 %arg5 to i32
  store i32 %stack.arg4.lo, ptr %local_p14, align 4
  %stack.arg5.lo = trunc i64 %arg6 to i32
  store i32 %stack.arg5.lo, ptr %local_p18, align 4
  %ecx = trunc i64 %arg0 to i32  ; 0x401240
  %addr.z = zext i32 %ecx to i64  ; 0x401240
  %st.p = inttoptr i64 %addr.z to ptr  ; 0x401240
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 328) to i32), ptr %st.p, align 1  ; 0x401240
  %sum = add i32 %ecx, 4  ; 0x401246
  %ecx.z = zext i32 %sum to i64  ; 0x401246
  store i32 %sum, ptr %local_m4, align 1  ; 0x401249
  %sarg1.d = load i32, ptr %local_p0, align 4
  %sarg1 = zext i32 %sarg1.d to i64
  %sarg2.d = load i32, ptr %local_p4, align 4
  %sarg2 = zext i32 %sarg2.d to i64
  %sarg3.d = load i32, ptr %local_p8, align 4
  %sarg3 = zext i32 %sarg3.d to i64
  %sarg4.d = load i32, ptr %local_pc, align 4
  %sarg4 = zext i32 %sarg4.d to i64
  %sarg5.d = load i32, ptr %local_p10, align 4
  %sarg5 = zext i32 %sarg5.d to i64
  %sarg6.d = load i32, ptr %local_p14, align 4
  %sarg6 = zext i32 %sarg6.d to i64
  %sarg7.d = load i32, ptr %local_p18, align 4
  %sarg7 = zext i32 %sarg7.d to i64
  %ret = call i64 @thunk___std_exception_destroy(i64 %ecx.z, i64 %ecx.z, i64 %sarg1, i64 %sarg2, i64 %sarg3, i64 %sarg4, i64 %sarg5, i64 %sarg6, i64 %sarg7)  ; 0x40124a
  %eh.pending = call i64 @ra_eh_pending()  ; 0x40124a
  %eh.thrown = icmp ne i64 %eh.pending, 0  ; 0x40124a
  br i1 %eh.thrown, label %eh.landing, label %eh.ok.40124a
eh.landing:  ; 0x40124a
  ret void
eh.ok.40124a:  ; 0x40124a
  ret void
}

define i64 @F_00401260(i32 %arg0) {  ; 0x401260
b_401260:  ; 0x401260
  %frame = alloca [4284 x i8], align 16
  %frame.top = getelementptr inbounds i8, ptr %frame, i64 4204
  %rsp0 = ptrtoint ptr %frame.top to i64
  %stack.arg0.p = getelementptr inbounds i8, ptr %frame.top, i64 4
  store i32 %arg0, ptr %stack.arg0.p, align 4
  %ebp = trunc i64 undef to i32  ; 0x401260
  %esp = trunc i64 %rsp0 to i32  ; 0x401260
  %sp = sub i32 %esp, 4  ; 0x401260
  %st.p = getelementptr inbounds i8, ptr %frame.top, i64 -4  ; 0x401260
  store i32 %ebp, ptr %st.p, align 1  ; 0x401260
  %sp.1 = sub i32 %sp, 4  ; 0x401263
  %st.p.1 = getelementptr inbounds i8, ptr %frame.top, i64 -8  ; 0x401263
  store i32 %ebp, ptr %st.p.1, align 1  ; 0x401263
  %sp.2 = sub i32 %sp.1, 4  ; 0x401264
  %st.p.2 = getelementptr inbounds i8, ptr %frame.top, i64 -12  ; 0x401264
  store i32 %ebp, ptr %st.p.2, align 1  ; 0x401264
  %sp.3 = sub i32 %sp.2, 4  ; 0x401265
  %st.p.3 = getelementptr inbounds i8, ptr %frame.top, i64 -16  ; 0x401265
  store i32 %ebp, ptr %st.p.3, align 1  ; 0x401265
  %diff = sub i32 %sp.3, 32  ; 0x401266
  %st.p.4 = getelementptr inbounds i8, ptr %frame.top, i64 -32  ; 0x40126c
  store i32 %diff, ptr %st.p.4, align 1  ; 0x40126c
  %st.p.5 = getelementptr inbounds i8, ptr %frame.top, i64 -20  ; 0x40126f
  store i32 -1, ptr %st.p.5, align 1  ; 0x40126f
  %addr.3 = add i32 %sp, -24  ; 0x401276
  %st.p.6 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x401279
  store i32 ptrtoint (ptr @U_00401be0 to i32), ptr %st.p.6, align 1  ; 0x401279
  %teb = call i64 @ra_teb()  ; 0x401280
  %teb.1 = trunc i64 %teb to i32  ; 0x401280
  %addr.z.8 = zext i32 %teb.1 to i64  ; 0x401280
  %m.p.1 = inttoptr i64 %addr.z.8 to ptr  ; 0x401280
  %m.1 = load i32, ptr %m.p.1, align 1  ; 0x401280
  %edx.z = zext i32 %m.1 to i64  ; 0x401280
  %st.p.7 = getelementptr inbounds i8, ptr %frame.top, i64 -28  ; 0x401287
  store i32 %m.1, ptr %st.p.7, align 1  ; 0x401287
  %teb.2 = call i64 @ra_teb()  ; 0x40128a
  %teb.3 = trunc i64 %teb.2 to i32  ; 0x40128a
  %addr.z.10 = zext i32 %teb.3 to i64  ; 0x40128a
  %st.p.8 = inttoptr i64 %addr.z.10 to ptr  ; 0x40128a
  store i32 %addr.3, ptr %st.p.8, align 1  ; 0x40128a
  store i32 0, ptr %st.p.5, align 1  ; 0x401291
  %st.p.10 = getelementptr inbounds i8, ptr %frame.top, i64 -36  ; 0x401298
  store i32 1, ptr %st.p.10, align 1  ; 0x401298
  %sp.4 = sub i32 %diff, 4  ; 0x40129f
  %st.p.11 = getelementptr inbounds i8, ptr %frame.top, i64 -52  ; 0x40129f
  store i32 %arg0, ptr %st.p.11, align 1  ; 0x40129f
  %sp.5 = sub i32 %sp.4, 4  ; 0x4012a0
  %sarg0.a = add i32 %sp.5, 4
  %ret = call i64 @F_00401020(i32 %arg0)  ; 0x4012a0
  %eh.pending = call i64 @ra_eh_pending()  ; 0x4012a0
  %eh.thrown = icmp ne i64 %eh.pending, 0  ; 0x4012a0
  br i1 %eh.thrown, label %eh.landing, label %eh.ok.4012a0
b_4012a8:  ; 0x4012a8
  %rbp.phi.4 = phi i32 [ %sum.24, %b_4012bc ], [ %sum.25, %b_4012c4 ], [ %sum.26, %b_4012cc ], [ %sum.27, %b_4012d4 ], [ %sp, %eh.ok.4012a0 ]  ; 0x4012a8
  %addr.42 = add i32 %rbp.phi.4, -24  ; 0x4012a8
  %addr.z.163 = zext i32 %addr.42 to i64  ; 0x4012a8
  %m.p.20 = inttoptr i64 %addr.z.163 to ptr  ; 0x4012a8
  %m.20 = load i32, ptr %m.p.20, align 1  ; 0x4012a8
  %teb.20 = call i64 @ra_teb()  ; 0x4012ab
  %teb.21 = trunc i64 %teb.20 to i32  ; 0x4012ab
  %addr.z.164 = zext i32 %teb.21 to i64  ; 0x4012ab
  %st.p.107 = inttoptr i64 %addr.z.164 to ptr  ; 0x4012ab
  store i32 %m.20, ptr %st.p.107, align 1  ; 0x4012ab
  %addr.43 = add i32 %rbp.phi.4, -32  ; 0x4012b1
  %addr.z.165 = zext i32 %addr.43 to i64  ; 0x4012b1
  %m.p.21 = inttoptr i64 %addr.z.165 to ptr  ; 0x4012b1
  %m.21 = load i32, ptr %m.p.21, align 1  ; 0x4012b1
  %eax.z.30 = zext i32 %m.21 to i64  ; 0x4012b1
  ret i64 %eax.z.30
b_4012bc:  ; 0x4012bc
  %rbp.phi.3 = phi i32 [ %eh.frame.1, %eh.destroyed.40130c ], [ %eh.frame.6, %eh.destroyed.40130c.1 ]  ; 0x4012bc
  %sum.24 = add i32 %rbp.phi.3, 12  ; 0x4012bc
  br label %b_4012a8
b_4012c4:  ; 0x4012c4
  %rbp.phi.2 = phi i32 [ %eh.frame.6, %eh.destroyed.401343 ], [ %eh.frame.1, %eh.destroyed.401343.1 ]  ; 0x4012c4
  %sum.25 = add i32 %rbp.phi.2, 12  ; 0x4012c4
  br label %b_4012a8
b_4012cc:  ; 0x4012cc
  %rbp.phi.1 = phi i32 [ %eh.frame.6, %eh.destroyed.40137d ], [ %eh.frame.1, %eh.destroyed.40137d.1 ]  ; 0x4012cc
  %sum.26 = add i32 %rbp.phi.1, 12  ; 0x4012cc
  br label %b_4012a8
b_4012d4:  ; 0x4012d4
  %rbp.phi = phi i32 [ %eh.frame.6, %eh.destroyed.401391 ], [ %eh.frame.1, %eh.destroyed.401391.1 ]  ; 0x4012d4
  %sum.27 = add i32 %rbp.phi, 12  ; 0x4012d4
  %addr.44 = add i32 %sum.27, -32  ; 0x4012d7
  %addr.z.171 = zext i32 %addr.44 to i64  ; 0x4012d7
  %st.p.108 = inttoptr i64 %addr.z.171 to ptr  ; 0x4012d7
  store i32 4000, ptr %st.p.108, align 1  ; 0x4012d7
  br label %b_4012a8
eh.landing:  ; 0x4012a0
  %teb.4 = call i64 @ra_teb()  ; 0x4012a0
  %teb.5 = trunc i64 %teb.4 to i32  ; 0x4012a0
  %addr.z.15 = zext i32 %teb.5 to i64  ; 0x4012a0
  %eh.head.p = inttoptr i64 %addr.z.15 to ptr  ; 0x4012a0
  %eh.head = load i32, ptr %eh.head.p, align 1  ; 0x4012a0
  %eh.head.s.w = zext i32 %eh.head to i64  ; 0x4012a0
  %eh.head.s = inttoptr i64 %eh.head.s.w to ptr  ; 0x4012a0
  %eh.frame = sub i32 %eh.head, -24  ; 0x4012a0
  %eh.reg = add i32 %eh.frame, -24  ; 0x4012a0
  %eh.head.f4 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 1  ; 0x4012a0
  %eh.handler = load i32, ptr %eh.head.f4, align 1  ; 0x4012a0
  %eh.ours = icmp eq i32 %eh.handler, 4201440  ; 0x4012a0
  %eh.ours.1 = icmp eq i32 %eh.handler, ptrtoint (ptr @U_00401be0 to i32)  ; 0x4012a0
  %eh.ours.2 = or i1 %eh.ours, %eh.ours.1  ; 0x4012a0
  br i1 %eh.ours.2, label %eh.dispatch, label %eh.foreign
eh.dispatch:  ; 0x4012a0
  %eh.head.f8 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x4012a0
  %eh.state = load i32, ptr %eh.head.f8, align 1  ; 0x4012a0
  switch i32 %eh.state, label %eh.state.bad [ i32 -1, label %eh.state_none i32 0, label %eh.state0 i32 1, label %eh.state1.1 ]
eh.foreign:  ; 0x4012a0
  call void @ra_trap(i64 4199008, i64 8)  ; 0x4012a0
  unreachable
eh.state.bad:  ; 0x4012a0
  call void @ra_trap(i64 4199008, i64 7)  ; 0x4012a0
  unreachable
eh.state_none:  ; 0x4012a0
  call void @ra_eh_suspend()  ; 0x4012a0
  %eh.head.f8.1 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x4012a0
  store i32 -1, ptr %eh.head.f8.1, align 1  ; 0x4012a0
  call void @ra_eh_resume()  ; 0x4012a0
  %eh.head.f0 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 0  ; 0x4012a0
  %eh.next = load i32, ptr %eh.head.f0, align 1  ; 0x4012a0
  %teb.6 = call i64 @ra_teb()  ; 0x4012a0
  %teb.7 = trunc i64 %teb.6 to i32  ; 0x4012a0
  %addr.z.20 = zext i32 %teb.7 to i64  ; 0x4012a0
  %st.p.14 = inttoptr i64 %addr.z.20 to ptr  ; 0x4012a0
  store i32 %eh.next, ptr %st.p.14, align 1  ; 0x4012a0
  ret i64 %ret
eh.state0:  ; 0x4012a0
  %eh.frame.1 = add i32 %eh.reg, 12  ; 0x4012a0
  %eh.catch.obj = add i32 %eh.frame.1, -24  ; 0x4012a0
  %eh.catch.obj.1 = zext i32 %eh.catch.obj to i64  ; 0x4012a0
  %eh.caught = call i64 @ra_eh_catches(i64 565892279632451611, i64 %eh.catch.obj.1, i64 8192)  ; 0x4012a0
  %eh.hit = icmp ne i64 %eh.caught, 0  ; 0x4012a0
  br i1 %eh.hit, label %eh.catch0_0.s0, label %eh.next0_0.s0
eh.catch0_0.s0:  ; 0x4012a0
  %eh.head.f8.2 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x4012a0
  store i32 0, ptr %eh.head.f8.2, align 1  ; 0x4012a0
  br label %eh.enter0_0
eh.next0_0.s0:  ; 0x4012a0
  %eh.catch.obj.2 = add i32 %eh.frame.1, -28  ; 0x40130c
  %eh.catch.obj.3 = zext i32 %eh.catch.obj.2 to i64  ; 0x40130c
  %eh.caught.1 = call i64 @ra_eh_catches(i64 -1082765518775566335, i64 %eh.catch.obj.3, i64 8200)  ; 0x40130c
  %eh.hit.1 = icmp ne i64 %eh.caught.1, 0  ; 0x40130c
  br i1 %eh.hit.1, label %eh.catch0_1.s0, label %eh.next0_1.s0
eh.enter0_0:  ; 0x4012a0
  call void @ra_eh_enter_catch()  ; 0x4012a0
  %eh.head.f8.3 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x4012a0
  store i32 1, ptr %eh.head.f8.3, align 1  ; 0x4012a0
  %eh.esp.p = sub i32 %eh.reg, 4  ; 0x4012a0
  %addr.z.23 = zext i32 %eh.esp.p to i64  ; 0x4012a0
  %eh.esp.p.1 = inttoptr i64 %addr.z.23 to ptr  ; 0x4012a0
  %eh.esp = load i32, ptr %eh.esp.p.1, align 1  ; 0x4012a0
  %eh.esp.1 = zext i32 %eh.esp to i64  ; 0x4012a0
  call void @ra_eh_catch_sp(i64 %eh.esp.1)  ; 0x4012a0
  %sp.6 = sub i32 %sarg0.a, 4  ; 0x4012a0
  %st.p.17 = getelementptr inbounds i8, ptr %frame.top, i64 -56  ; 0x4012a0
  store i32 0, ptr %st.p.17, align 1  ; 0x4012a0
  br label %f1_4012f0
f1_4012f0:  ; 0x4012f0
  %sp.7 = sub i32 %sp.6, 4  ; 0x4012f0
  %st.p.18 = getelementptr inbounds i8, ptr %frame.top, i64 -60  ; 0x4012f0
  store i32 %eh.frame.1, ptr %st.p.18, align 1  ; 0x4012f0
  %eax.1 = trunc i64 %ret to i32  ; 0x4012f1
  %sp.8 = sub i32 %sp.7, 4  ; 0x4012f1
  %st.p.19 = getelementptr inbounds i8, ptr %frame.top, i64 -64  ; 0x4012f1
  store i32 %eax.1, ptr %st.p.19, align 1  ; 0x4012f1
  %sum = add i32 %eh.frame.1, 12  ; 0x4012f2
  %addr.8 = add i32 %sum, -28  ; 0x4012f5
  %addr.z.27 = zext i32 %addr.8 to i64  ; 0x4012f5
  %st.p.20 = inttoptr i64 %addr.z.27 to ptr  ; 0x4012f5
  store i32 %sp.8, ptr %st.p.20, align 1  ; 0x4012f5
  %addr.9 = add i32 %sum, -36  ; 0x4012fd
  %addr.z.28 = zext i32 %addr.9 to i64  ; 0x4012fd
  %m.p.2 = inttoptr i64 %addr.z.28 to ptr  ; 0x4012fd
  %m.2 = load i32, ptr %m.p.2, align 1  ; 0x4012fd
  %sum.1 = add i32 1000, %m.2  ; 0x4012fd
  %addr.10 = add i32 %sum, -32  ; 0x401300
  %addr.z.29 = zext i32 %addr.10 to i64  ; 0x401300
  %st.p.21 = inttoptr i64 %addr.z.29 to ptr  ; 0x401300
  store i32 %sum.1, ptr %st.p.21, align 1  ; 0x401300
  %eh.ending = call i64 @ra_eh_ending_object()  ; 0x40130c
  %eh.dtor = call i64 @ra_eh_ending_destructor()  ; 0x40130c
  call void @ra_eh_leave_catch()  ; 0x40130c
  %eh.dies = icmp ne i64 %eh.dtor, 0  ; 0x40130c
  br i1 %eh.dies, label %eh.destroy.40130c, label %eh.destroyed.40130c
eh.destroy.40130c:  ; 0x40130c
  %eh.this = trunc i64 %eh.ending to i32  ; 0x40130c
  %ecx.z.1 = zext i32 %eh.this to i64  ; 0x40130c
  store i32 0, ptr %st.p.17, align 1  ; 0x40130c
  %fp.9 = inttoptr i64 %eh.dtor to ptr  ; 0x40130c
  %sarg0.d.10 = load i32, ptr %st.p.11, align 4
  %sarg0.10 = zext i32 %sarg0.d.10 to i64
  %sarg1.p.9 = getelementptr inbounds i8, ptr %frame.top, i64 -48
  %sarg1.d.9 = load i32, ptr %sarg1.p.9, align 4
  %sarg1.9 = zext i32 %sarg1.d.9 to i64
  %sarg2.p.9 = getelementptr inbounds i8, ptr %frame.top, i64 -44
  %sarg2.d.9 = load i32, ptr %sarg2.p.9, align 4
  %sarg2.9 = zext i32 %sarg2.d.9 to i64
  %sarg3.p.9 = getelementptr inbounds i8, ptr %frame.top, i64 -40
  %sarg3.d.9 = load i32, ptr %sarg3.p.9, align 4
  %sarg3.9 = zext i32 %sarg3.d.9 to i64
  %sarg4.d.9 = load i32, ptr %st.p.10, align 4
  %sarg4.9 = zext i32 %sarg4.d.9 to i64
  %sarg5.d.9 = load i32, ptr %st.p.4, align 4
  %sarg5.9 = zext i32 %sarg5.d.9 to i64
  %sarg6.d.9 = load i32, ptr %st.p.7, align 4
  %sarg6.9 = zext i32 %sarg6.d.9 to i64
  %args.p.18 = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr  ; 0x40130c
  store i64 %sarg6.9, ptr %args.p.18, align 8  ; 0x40130c
  %sarg7.d.9 = load i32, ptr %st.p.6, align 4
  %sarg7.9 = zext i32 %sarg7.d.9 to i64
  %args.p.19 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr  ; 0x40130c
  store i64 %sarg7.9, ptr %args.p.19, align 8  ; 0x40130c
  %ret.26 = call i64 %fp.9(i64 %sarg0.10, i64 %sarg1.9, i64 %sarg2.9, i64 %sarg3.9, i64 %sarg4.9, i64 %sarg5.9, i64 %ecx.z.1, i64 %edx.z)  ; 0x40130c
  %pops.9 = call i64 @ra_pops_of(i64 %eh.dtor)  ; 0x40130c
  br label %eh.destroyed.40130c
eh.destroyed.40130c:  ; 0x40130c
  store i32 %eh.esp, ptr %eh.esp.p.1, align 1  ; 0x40130c
  br label %b_4012bc
eh.catch0_1.s0:  ; 0x40130c
  %eh.head.f8.4 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x40130c
  store i32 0, ptr %eh.head.f8.4, align 1  ; 0x40130c
  br label %eh.enter0_1
eh.next0_1.s0:  ; 0x40130c
  %eh.catch.obj.10 = add i32 %eh.frame.1, -32  ; 0x401343
  %eh.catch.obj.11 = zext i32 %eh.catch.obj.10 to i64  ; 0x401343
  %eh.caught.6 = call i64 @ra_eh_catches(i64 -5932227318556162533, i64 %eh.catch.obj.11, i64 8200)  ; 0x401343
  %eh.hit.6 = icmp ne i64 %eh.caught.6, 0  ; 0x401343
  br i1 %eh.hit.6, label %eh.catch0_2.s0.1, label %eh.next0_2.s0.1
eh.enter0_1:  ; 0x40130c
  call void @ra_eh_enter_catch()  ; 0x40130c
  %eh.head.f8.5 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x40130c
  store i32 1, ptr %eh.head.f8.5, align 1  ; 0x40130c
  %eh.esp.p.3 = sub i32 %eh.reg, 4  ; 0x40130c
  %addr.z.36 = zext i32 %eh.esp.p.3 to i64  ; 0x40130c
  %eh.esp.p.4 = inttoptr i64 %addr.z.36 to ptr  ; 0x40130c
  %eh.esp.2 = load i32, ptr %eh.esp.p.4, align 1  ; 0x40130c
  %eh.esp.3 = zext i32 %eh.esp.2 to i64  ; 0x40130c
  call void @ra_eh_catch_sp(i64 %eh.esp.3)  ; 0x40130c
  %sp.12 = sub i32 %sarg0.a, 4  ; 0x40130c
  %st.p.26 = getelementptr inbounds i8, ptr %frame.top, i64 -56  ; 0x40130c
  store i32 0, ptr %st.p.26, align 1  ; 0x40130c
  br label %f2_401310
f2_401310:  ; 0x401310
  %sp.13 = sub i32 %sp.12, 4  ; 0x401310
  %st.p.27 = getelementptr inbounds i8, ptr %frame.top, i64 -60  ; 0x401310
  store i32 %eh.frame.1, ptr %st.p.27, align 1  ; 0x401310
  %eax.5 = trunc i64 %ret to i32  ; 0x401311
  %sp.14 = sub i32 %sp.13, 4  ; 0x401311
  %st.p.28 = getelementptr inbounds i8, ptr %frame.top, i64 -64  ; 0x401311
  store i32 %eax.5, ptr %st.p.28, align 1  ; 0x401311
  %sum.3 = add i32 %eh.frame.1, 12  ; 0x401312
  %addr.11 = add i32 %sum.3, -28  ; 0x401315
  %addr.z.40 = zext i32 %addr.11 to i64  ; 0x401315
  %st.p.29 = inttoptr i64 %addr.z.40 to ptr  ; 0x401315
  store i32 %sp.14, ptr %st.p.29, align 1  ; 0x401315
  %addr.12 = add i32 %sum.3, -40  ; 0x401318
  %addr.z.41 = zext i32 %addr.12 to i64  ; 0x401318
  %m.p.3 = inttoptr i64 %addr.z.41 to ptr  ; 0x401318
  %m.3 = load i32, ptr %m.p.3, align 1  ; 0x401318
  %m.3.s.w = zext i32 %m.3 to i64  ; 0x401318
  %m.3.s = inttoptr i64 %m.3.s.w to ptr  ; 0x401318
  %m.3.f0 = getelementptr inbounds %S2, ptr %m.3.s, i64 0, i32 0  ; 0x40131b
  %m.4 = load i32, ptr %m.3.f0, align 1  ; 0x40131b
  %m.3.f4 = getelementptr inbounds %S2, ptr %m.3.s, i64 0, i32 1  ; 0x40131d
  %m.5 = load i32, ptr %m.3.f4, align 1  ; 0x40131d
  %edx.z.1 = zext i32 %m.5 to i64  ; 0x40131d
  %addr.15 = add i32 %m.4, 4  ; 0x401323
  %addr.z.44 = zext i32 %addr.15 to i64  ; 0x401323
  %m.p.6 = inttoptr i64 %addr.z.44 to ptr  ; 0x401323
  %m.6 = load i32, ptr %m.p.6, align 1  ; 0x401323
  %eax.z.5 = zext i32 %m.6 to i64  ; 0x401323
  %eh.head.f8.6 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x401326
  store i32 1, ptr %eh.head.f8.6, align 1  ; 0x401326
  %sp.15 = sub i32 %sp.14, 4  ; 0x40132d
  %is.fn.4 = icmp eq i64 %eax.z.5, ptrtoint (ptr @U_00401b10 to i64)  ; 0x40132d
  %is.at.4 = icmp eq i32 %m.6, 4201232  ; 0x40132d
  %is.4 = or i1 %is.fn.4, %is.at.4  ; 0x40132d
  br i1 %is.4, label %vcall_96, label %vcall.test_100
eh.landing.1:  ; 0x40132d
  %rax.phi.3 = phi i64 [ %rax.phi.2, %f2_401310.94 ], [ %rax.phi.6, %f4_401310.112 ], [ %rax.phi.5, %f5_401350.103 ], [ %rax.phi.1, %f7_401350.85 ]  ; 0x40132d
  %rsp.phi = phi i32 [ %rsp.91, %f2_401310.94 ], [ %rsp.97, %f4_401310.112 ], [ %rsp.95, %f5_401350.103 ], [ %rsp.89, %f7_401350.85 ]  ; 0x40132d
  %teb.8 = call i64 @ra_teb()  ; 0x40132d
  %teb.9 = trunc i64 %teb.8 to i32  ; 0x40132d
  %addr.z.47 = zext i32 %teb.9 to i64  ; 0x40132d
  %eh.head.p.1 = inttoptr i64 %addr.z.47 to ptr  ; 0x40132d
  %eh.head.1 = load i32, ptr %eh.head.p.1, align 1  ; 0x40132d
  %eh.head.1.s.w = zext i32 %eh.head.1 to i64  ; 0x40132d
  %eh.head.1.s = inttoptr i64 %eh.head.1.s.w to ptr  ; 0x40132d
  %eh.frame.5 = sub i32 %eh.head.1, -24  ; 0x40132d
  %eh.reg.1 = add i32 %eh.frame.5, -24  ; 0x40132d
  %eh.head.1.f4 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 1  ; 0x40132d
  %eh.handler.1 = load i32, ptr %eh.head.1.f4, align 1  ; 0x40132d
  %eh.ours.3 = icmp eq i32 %eh.handler.1, 4201440  ; 0x40132d
  %eh.ours.4 = icmp eq i32 %eh.handler.1, ptrtoint (ptr @U_00401be0 to i32)  ; 0x40132d
  %eh.ours.5 = or i1 %eh.ours.3, %eh.ours.4  ; 0x40132d
  br i1 %eh.ours.5, label %eh.dispatch.1, label %eh.foreign.1
eh.dispatch.1:  ; 0x40132d
  %eh.head.1.f8 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x40132d
  %eh.state.1 = load i32, ptr %eh.head.1.f8, align 1  ; 0x40132d
  switch i32 %eh.state.1, label %eh.state.bad.1 [ i32 -1, label %eh.state_none.1 i32 0, label %eh.state0.1 i32 1, label %eh.state1 ]
eh.foreign.1:  ; 0x40132d
  call void @ra_trap(i64 4199008, i64 8)  ; 0x40132d
  unreachable
eh.state.bad.1:  ; 0x40132d
  call void @ra_trap(i64 4199008, i64 7)  ; 0x40132d
  unreachable
eh.state_none.1:  ; 0x40132d
  call void @ra_eh_suspend()  ; 0x40132d
  %eh.head.1.f8.1 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x40132d
  store i32 -1, ptr %eh.head.1.f8.1, align 1  ; 0x40132d
  call void @ra_eh_resume()  ; 0x40132d
  %eh.head.1.f0 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 0  ; 0x40132d
  %eh.next.1 = load i32, ptr %eh.head.1.f0, align 1  ; 0x40132d
  %teb.10 = call i64 @ra_teb()  ; 0x40132d
  %teb.11 = trunc i64 %teb.10 to i32  ; 0x40132d
  %addr.z.52 = zext i32 %teb.11 to i64  ; 0x40132d
  %st.p.33 = inttoptr i64 %addr.z.52 to ptr  ; 0x40132d
  store i32 %eh.next.1, ptr %st.p.33, align 1  ; 0x40132d
  ret i64 %rax.phi.3
eh.state0.1:  ; 0x40132d
  %eh.frame.6 = add i32 %eh.reg.1, 12  ; 0x40132d
  %eh.catch.obj.4 = add i32 %eh.frame.6, -24  ; 0x40132d
  %eh.catch.obj.5 = zext i32 %eh.catch.obj.4 to i64  ; 0x40132d
  %eh.caught.2 = call i64 @ra_eh_catches(i64 565892279632451611, i64 %eh.catch.obj.5, i64 8192)  ; 0x40132d
  %eh.hit.2 = icmp ne i64 %eh.caught.2, 0  ; 0x40132d
  br i1 %eh.hit.2, label %eh.catch0_0.s0.1, label %eh.next0_0.s0.1
eh.catch0_0.s0.1:  ; 0x40132d
  %eh.head.1.f8.2 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x40132d
  store i32 0, ptr %eh.head.1.f8.2, align 1  ; 0x40132d
  br label %eh.enter0_0.1
eh.next0_0.s0.1:  ; 0x40132d
  %eh.catch.obj.6 = add i32 %eh.frame.6, -28  ; 0x40130c
  %eh.catch.obj.7 = zext i32 %eh.catch.obj.6 to i64  ; 0x40130c
  %eh.caught.3 = call i64 @ra_eh_catches(i64 -1082765518775566335, i64 %eh.catch.obj.7, i64 8200)  ; 0x40130c
  %eh.hit.3 = icmp ne i64 %eh.caught.3, 0  ; 0x40130c
  br i1 %eh.hit.3, label %eh.catch0_1.s0.1, label %eh.next0_1.s0.1
eh.enter0_0.1:  ; 0x40132d
  call void @ra_eh_enter_catch()  ; 0x40132d
  %eh.head.1.f8.3 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x40132d
  store i32 1, ptr %eh.head.1.f8.3, align 1  ; 0x40132d
  %eh.esp.p.5 = sub i32 %eh.reg.1, 4  ; 0x40132d
  %addr.z.55 = zext i32 %eh.esp.p.5 to i64  ; 0x40132d
  %eh.esp.p.6 = inttoptr i64 %addr.z.55 to ptr  ; 0x40132d
  %eh.esp.4 = load i32, ptr %eh.esp.p.6, align 1  ; 0x40132d
  %eh.esp.5 = zext i32 %eh.esp.4 to i64  ; 0x40132d
  call void @ra_eh_catch_sp(i64 %eh.esp.5)  ; 0x40132d
  %sp.16 = sub i32 %rsp.phi, 4  ; 0x40132d
  %addr.z.56 = zext i32 %sp.16 to i64  ; 0x40132d
  %st.p.36 = inttoptr i64 %addr.z.56 to ptr  ; 0x40132d
  store i32 0, ptr %st.p.36, align 1  ; 0x40132d
  br label %f3_4012f0
f3_4012f0:  ; 0x4012f0
  %sp.17 = sub i32 %sp.16, 4  ; 0x4012f0
  %addr.z.57 = zext i32 %sp.17 to i64  ; 0x4012f0
  %st.p.37 = inttoptr i64 %addr.z.57 to ptr  ; 0x4012f0
  store i32 %eh.frame.6, ptr %st.p.37, align 1  ; 0x4012f0
  %eax.8 = trunc i64 %rax.phi.3 to i32  ; 0x4012f1
  %sp.18 = sub i32 %sp.17, 4  ; 0x4012f1
  %addr.z.58 = zext i32 %sp.18 to i64  ; 0x4012f1
  %st.p.38 = inttoptr i64 %addr.z.58 to ptr  ; 0x4012f1
  store i32 %eax.8, ptr %st.p.38, align 1  ; 0x4012f1
  %sum.4 = add i32 %eh.frame.6, 12  ; 0x4012f2
  %addr.17 = add i32 %sum.4, -28  ; 0x4012f5
  %addr.z.59 = zext i32 %addr.17 to i64  ; 0x4012f5
  %st.p.39 = inttoptr i64 %addr.z.59 to ptr  ; 0x4012f5
  store i32 %sp.18, ptr %st.p.39, align 1  ; 0x4012f5
  %addr.18 = add i32 %sum.4, -36  ; 0x4012fd
  %addr.z.60 = zext i32 %addr.18 to i64  ; 0x4012fd
  %m.p.7 = inttoptr i64 %addr.z.60 to ptr  ; 0x4012fd
  %m.7 = load i32, ptr %m.p.7, align 1  ; 0x4012fd
  %sum.5 = add i32 1000, %m.7  ; 0x4012fd
  %addr.19 = add i32 %sum.4, -32  ; 0x401300
  %addr.z.61 = zext i32 %addr.19 to i64  ; 0x401300
  %st.p.40 = inttoptr i64 %addr.z.61 to ptr  ; 0x401300
  store i32 %sum.5, ptr %st.p.40, align 1  ; 0x401300
  %sum.6 = add i32 %sp.18, 4  ; 0x401308
  %sp.19 = add i32 %sum.6, 4  ; 0x40130b
  %sp.20 = add i32 %sp.19, 4  ; 0x40130c
  %eh.ending.1 = call i64 @ra_eh_ending_object()  ; 0x40130c
  %eh.dtor.1 = call i64 @ra_eh_ending_destructor()  ; 0x40130c
  call void @ra_eh_leave_catch()  ; 0x40130c
  %eh.dies.1 = icmp ne i64 %eh.dtor.1, 0  ; 0x40130c
  br i1 %eh.dies.1, label %eh.destroy.40130c.1, label %eh.destroyed.40130c.1
eh.destroy.40130c.1:  ; 0x40130c
  %eh.this.1 = trunc i64 %eh.ending.1 to i32  ; 0x40130c
  %ecx.z.3 = zext i32 %eh.this.1 to i64  ; 0x40130c
  %sp.21 = sub i32 %sp.20, 4  ; 0x40130c
  %addr.z.64 = zext i32 %sp.21 to i64  ; 0x40130c
  %st.p.41 = inttoptr i64 %addr.z.64 to ptr  ; 0x40130c
  store i32 0, ptr %st.p.41, align 1  ; 0x40130c
  %fp.8 = inttoptr i64 %eh.dtor.1 to ptr  ; 0x40130c
  %sarg0.a.9 = add i64 %addr.z.64, 4
  %sarg0.p.9 = inttoptr i64 %sarg0.a.9 to ptr
  %sarg0.d.9 = load i32, ptr %sarg0.p.9, align 4
  %sarg0.9 = zext i32 %sarg0.d.9 to i64
  %sarg1.a.8 = add i64 %addr.z.64, 8
  %sarg1.p.8 = inttoptr i64 %sarg1.a.8 to ptr
  %sarg1.d.8 = load i32, ptr %sarg1.p.8, align 4
  %sarg1.8 = zext i32 %sarg1.d.8 to i64
  %sarg2.a.8 = add i64 %addr.z.64, 12
  %sarg2.p.8 = inttoptr i64 %sarg2.a.8 to ptr
  %sarg2.d.8 = load i32, ptr %sarg2.p.8, align 4
  %sarg2.8 = zext i32 %sarg2.d.8 to i64
  %sarg3.a.8 = add i64 %addr.z.64, 16
  %sarg3.p.8 = inttoptr i64 %sarg3.a.8 to ptr
  %sarg3.d.8 = load i32, ptr %sarg3.p.8, align 4
  %sarg3.8 = zext i32 %sarg3.d.8 to i64
  %sarg4.a.8 = add i64 %addr.z.64, 20
  %sarg4.p.8 = inttoptr i64 %sarg4.a.8 to ptr
  %sarg4.d.8 = load i32, ptr %sarg4.p.8, align 4
  %sarg4.8 = zext i32 %sarg4.d.8 to i64
  %sarg5.a.8 = add i64 %addr.z.64, 24
  %sarg5.p.8 = inttoptr i64 %sarg5.a.8 to ptr
  %sarg5.d.8 = load i32, ptr %sarg5.p.8, align 4
  %sarg5.8 = zext i32 %sarg5.d.8 to i64
  %sarg6.a.8 = add i64 %addr.z.64, 28
  %sarg6.p.8 = inttoptr i64 %sarg6.a.8 to ptr
  %sarg6.d.8 = load i32, ptr %sarg6.p.8, align 4
  %sarg6.8 = zext i32 %sarg6.d.8 to i64
  %args.p.16 = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr  ; 0x40130c
  store i64 %sarg6.8, ptr %args.p.16, align 8  ; 0x40130c
  %sarg7.a.8 = add i64 %addr.z.64, 32
  %sarg7.p.8 = inttoptr i64 %sarg7.a.8 to ptr
  %sarg7.d.8 = load i32, ptr %sarg7.p.8, align 4
  %sarg7.8 = zext i32 %sarg7.d.8 to i64
  %args.p.17 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr  ; 0x40130c
  store i64 %sarg7.8, ptr %args.p.17, align 8  ; 0x40130c
  %ret.25 = call i64 %fp.8(i64 %sarg0.9, i64 %sarg1.8, i64 %sarg2.8, i64 %sarg3.8, i64 %sarg4.8, i64 %sarg5.8, i64 %ecx.z.3, i64 undef)  ; 0x40130c
  %pops.8 = call i64 @ra_pops_of(i64 %eh.dtor.1)  ; 0x40130c
  br label %eh.destroyed.40130c.1
eh.destroyed.40130c.1:  ; 0x40130c
  store i32 %eh.esp.4, ptr %eh.esp.p.6, align 1  ; 0x40130c
  br label %b_4012bc
eh.catch0_1.s0.1:  ; 0x40130c
  %eh.head.1.f8.4 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x40130c
  store i32 0, ptr %eh.head.1.f8.4, align 1  ; 0x40130c
  br label %eh.enter0_1.1
eh.next0_1.s0.1:  ; 0x40130c
  %eh.catch.obj.8 = add i32 %eh.frame.6, -32  ; 0x401343
  %eh.catch.obj.9 = zext i32 %eh.catch.obj.8 to i64  ; 0x401343
  %eh.caught.4 = call i64 @ra_eh_catches(i64 -5932227318556162533, i64 %eh.catch.obj.9, i64 8200)  ; 0x401343
  %eh.hit.4 = icmp ne i64 %eh.caught.4, 0  ; 0x401343
  br i1 %eh.hit.4, label %eh.catch0_2.s0, label %eh.next0_2.s0
eh.enter0_1.1:  ; 0x40130c
  call void @ra_eh_enter_catch()  ; 0x40130c
  %eh.head.1.f8.5 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x40130c
  store i32 1, ptr %eh.head.1.f8.5, align 1  ; 0x40130c
  %eh.esp.p.8 = sub i32 %eh.reg.1, 4  ; 0x40130c
  %addr.z.68 = zext i32 %eh.esp.p.8 to i64  ; 0x40130c
  %eh.esp.p.9 = inttoptr i64 %addr.z.68 to ptr  ; 0x40130c
  %eh.esp.6 = load i32, ptr %eh.esp.p.9, align 1  ; 0x40130c
  %eh.esp.7 = zext i32 %eh.esp.6 to i64  ; 0x40130c
  call void @ra_eh_catch_sp(i64 %eh.esp.7)  ; 0x40130c
  %sp.22 = sub i32 %rsp.phi, 4  ; 0x40130c
  %addr.z.69 = zext i32 %sp.22 to i64  ; 0x40130c
  %st.p.45 = inttoptr i64 %addr.z.69 to ptr  ; 0x40130c
  store i32 0, ptr %st.p.45, align 1  ; 0x40130c
  br label %f4_401310
f4_401310:  ; 0x401310
  %sp.23 = sub i32 %sp.22, 4  ; 0x401310
  %addr.z.70 = zext i32 %sp.23 to i64  ; 0x401310
  %st.p.46 = inttoptr i64 %addr.z.70 to ptr  ; 0x401310
  store i32 %eh.frame.6, ptr %st.p.46, align 1  ; 0x401310
  %eax.12 = trunc i64 %rax.phi.3 to i32  ; 0x401311
  %sp.24 = sub i32 %sp.23, 4  ; 0x401311
  %addr.z.71 = zext i32 %sp.24 to i64  ; 0x401311
  %st.p.47 = inttoptr i64 %addr.z.71 to ptr  ; 0x401311
  store i32 %eax.12, ptr %st.p.47, align 1  ; 0x401311
  %sum.7 = add i32 %eh.frame.6, 12  ; 0x401312
  %addr.20 = add i32 %sum.7, -28  ; 0x401315
  %addr.z.72 = zext i32 %addr.20 to i64  ; 0x401315
  %st.p.48 = inttoptr i64 %addr.z.72 to ptr  ; 0x401315
  store i32 %sp.24, ptr %st.p.48, align 1  ; 0x401315
  %addr.21 = add i32 %sum.7, -40  ; 0x401318
  %addr.z.73 = zext i32 %addr.21 to i64  ; 0x401318
  %m.p.8 = inttoptr i64 %addr.z.73 to ptr  ; 0x401318
  %m.8 = load i32, ptr %m.p.8, align 1  ; 0x401318
  %m.8.s.w = zext i32 %m.8 to i64  ; 0x401318
  %m.8.s = inttoptr i64 %m.8.s.w to ptr  ; 0x401318
  %m.8.f0 = getelementptr inbounds %S2, ptr %m.8.s, i64 0, i32 0  ; 0x40131b
  %m.9 = load i32, ptr %m.8.f0, align 1  ; 0x40131b
  %m.8.f4 = getelementptr inbounds %S2, ptr %m.8.s, i64 0, i32 1  ; 0x40131d
  %m.10 = load i32, ptr %m.8.f4, align 1  ; 0x40131d
  %edx.z.2 = zext i32 %m.10 to i64  ; 0x40131d
  %addr.24 = add i32 %m.9, 4  ; 0x401323
  %addr.z.76 = zext i32 %addr.24 to i64  ; 0x401323
  %m.p.11 = inttoptr i64 %addr.z.76 to ptr  ; 0x401323
  %m.11 = load i32, ptr %m.p.11, align 1  ; 0x401323
  %eax.z.10 = zext i32 %m.11 to i64  ; 0x401323
  %eh.head.1.f8.6 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x401326
  store i32 1, ptr %eh.head.1.f8.6, align 1  ; 0x401326
  %sp.25 = sub i32 %sp.24, 4  ; 0x40132d
  %is.fn.12 = icmp eq i64 %eax.z.10, ptrtoint (ptr @U_00401b10 to i64)  ; 0x40132d
  %is.at.12 = icmp eq i32 %m.11, 4201232  ; 0x40132d
  %is.12 = or i1 %is.fn.12, %is.at.12  ; 0x40132d
  br i1 %is.12, label %vcall_114, label %vcall.test_118
eh.ok.40132d:  ; 0x40132d
  %eax.15 = trunc i64 %rax.phi.6 to i32  ; 0x40132f
  %scaled.2 = mul i32 %m.10, 10  ; 0x40132f
  %addr.26 = add i32 %eax.15, %scaled.2  ; 0x40132f
  %sum.8 = add i32 %addr.26, 2000  ; 0x401332
  %addr.27 = add i32 %sum.7, -32  ; 0x401337
  %addr.z.79 = zext i32 %addr.27 to i64  ; 0x401337
  %st.p.51 = inttoptr i64 %addr.z.79 to ptr  ; 0x401337
  store i32 %sum.8, ptr %st.p.51, align 1  ; 0x401337
  %sum.9 = add i32 %rsp.97, 4  ; 0x40133f
  %sp.26 = add i32 %sum.9, 4  ; 0x401342
  %sp.27 = add i32 %sp.26, 4  ; 0x401343
  %eh.ending.2 = call i64 @ra_eh_ending_object()  ; 0x401343
  %eh.dtor.2 = call i64 @ra_eh_ending_destructor()  ; 0x401343
  call void @ra_eh_leave_catch()  ; 0x401343
  %eh.dies.2 = icmp ne i64 %eh.dtor.2, 0  ; 0x401343
  br i1 %eh.dies.2, label %eh.destroy.401343, label %eh.destroyed.401343
eh.destroy.401343:  ; 0x401343
  %eh.this.2 = trunc i64 %eh.ending.2 to i32  ; 0x401343
  %ecx.z.5 = zext i32 %eh.this.2 to i64  ; 0x401343
  %sp.28 = sub i32 %sp.27, 4  ; 0x401343
  %addr.z.82 = zext i32 %sp.28 to i64  ; 0x401343
  %st.p.52 = inttoptr i64 %addr.z.82 to ptr  ; 0x401343
  store i32 0, ptr %st.p.52, align 1  ; 0x401343
  %fp.7 = inttoptr i64 %eh.dtor.2 to ptr  ; 0x401343
  %sarg0.a.8 = add i64 %addr.z.82, 4
  %sarg0.p.8 = inttoptr i64 %sarg0.a.8 to ptr
  %sarg0.d.8 = load i32, ptr %sarg0.p.8, align 4
  %sarg0.8 = zext i32 %sarg0.d.8 to i64
  %sarg1.a.7 = add i64 %addr.z.82, 8
  %sarg1.p.7 = inttoptr i64 %sarg1.a.7 to ptr
  %sarg1.d.7 = load i32, ptr %sarg1.p.7, align 4
  %sarg1.7 = zext i32 %sarg1.d.7 to i64
  %sarg2.a.7 = add i64 %addr.z.82, 12
  %sarg2.p.7 = inttoptr i64 %sarg2.a.7 to ptr
  %sarg2.d.7 = load i32, ptr %sarg2.p.7, align 4
  %sarg2.7 = zext i32 %sarg2.d.7 to i64
  %sarg3.a.7 = add i64 %addr.z.82, 16
  %sarg3.p.7 = inttoptr i64 %sarg3.a.7 to ptr
  %sarg3.d.7 = load i32, ptr %sarg3.p.7, align 4
  %sarg3.7 = zext i32 %sarg3.d.7 to i64
  %sarg4.a.7 = add i64 %addr.z.82, 20
  %sarg4.p.7 = inttoptr i64 %sarg4.a.7 to ptr
  %sarg4.d.7 = load i32, ptr %sarg4.p.7, align 4
  %sarg4.7 = zext i32 %sarg4.d.7 to i64
  %sarg5.a.7 = add i64 %addr.z.82, 24
  %sarg5.p.7 = inttoptr i64 %sarg5.a.7 to ptr
  %sarg5.d.7 = load i32, ptr %sarg5.p.7, align 4
  %sarg5.7 = zext i32 %sarg5.d.7 to i64
  %sarg6.a.7 = add i64 %addr.z.82, 28
  %sarg6.p.7 = inttoptr i64 %sarg6.a.7 to ptr
  %sarg6.d.7 = load i32, ptr %sarg6.p.7, align 4
  %sarg6.7 = zext i32 %sarg6.d.7 to i64
  %args.p.14 = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr  ; 0x401343
  store i64 %sarg6.7, ptr %args.p.14, align 8  ; 0x401343
  %sarg7.a.7 = add i64 %addr.z.82, 32
  %sarg7.p.7 = inttoptr i64 %sarg7.a.7 to ptr
  %sarg7.d.7 = load i32, ptr %sarg7.p.7, align 4
  %sarg7.7 = zext i32 %sarg7.d.7 to i64
  %args.p.15 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr  ; 0x401343
  store i64 %sarg7.7, ptr %args.p.15, align 8  ; 0x401343
  %ret.24 = call i64 %fp.7(i64 %sarg0.8, i64 %sarg1.7, i64 %sarg2.7, i64 %sarg3.7, i64 %sarg4.7, i64 %sarg5.7, i64 %ecx.z.5, i64 %edx.z.2)  ; 0x401343
  %pops.7 = call i64 @ra_pops_of(i64 %eh.dtor.2)  ; 0x401343
  br label %eh.destroyed.401343
eh.destroyed.401343:  ; 0x401343
  store i32 %eh.esp.6, ptr %eh.esp.p.9, align 1  ; 0x401343
  br label %b_4012c4
eh.catch0_2.s0:  ; 0x401343
  %eh.head.1.f8.7 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x401343
  store i32 0, ptr %eh.head.1.f8.7, align 1  ; 0x401343
  br label %eh.enter0_2
eh.next0_2.s0:  ; 0x401343
  %eh.caught.5 = call i64 @ra_eh_catches(i64 0, i64 0, i64 8256)  ; 0x40137d
  %eh.hit.5 = icmp ne i64 %eh.caught.5, 0  ; 0x40137d
  br i1 %eh.hit.5, label %eh.catch0_3.s0, label %eh.next0_3.s0
eh.enter0_2:  ; 0x401343
  call void @ra_eh_enter_catch()  ; 0x401343
  %eh.head.1.f8.8 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x401343
  store i32 1, ptr %eh.head.1.f8.8, align 1  ; 0x401343
  %eh.esp.p.11 = sub i32 %eh.reg.1, 4  ; 0x401343
  %addr.z.86 = zext i32 %eh.esp.p.11 to i64  ; 0x401343
  %eh.esp.p.12 = inttoptr i64 %addr.z.86 to ptr  ; 0x401343
  %eh.esp.8 = load i32, ptr %eh.esp.p.12, align 1  ; 0x401343
  %eh.esp.9 = zext i32 %eh.esp.8 to i64  ; 0x401343
  call void @ra_eh_catch_sp(i64 %eh.esp.9)  ; 0x401343
  %sp.29 = sub i32 %rsp.phi, 4  ; 0x401343
  %addr.z.87 = zext i32 %sp.29 to i64  ; 0x401343
  %st.p.56 = inttoptr i64 %addr.z.87 to ptr  ; 0x401343
  store i32 0, ptr %st.p.56, align 1  ; 0x401343
  br label %f5_401350
f5_401350:  ; 0x401350
  %sp.30 = sub i32 %sp.29, 4  ; 0x401350
  %addr.z.88 = zext i32 %sp.30 to i64  ; 0x401350
  %st.p.57 = inttoptr i64 %addr.z.88 to ptr  ; 0x401350
  store i32 %eh.frame.6, ptr %st.p.57, align 1  ; 0x401350
  %eax.19 = trunc i64 %rax.phi.3 to i32  ; 0x401351
  %sp.31 = sub i32 %sp.30, 4  ; 0x401351
  %addr.z.89 = zext i32 %sp.31 to i64  ; 0x401351
  %st.p.58 = inttoptr i64 %addr.z.89 to ptr  ; 0x401351
  store i32 %eax.19, ptr %st.p.58, align 1  ; 0x401351
  %sum.10 = add i32 %eh.frame.6, 12  ; 0x401352
  %addr.28 = add i32 %sum.10, -28  ; 0x401355
  %addr.z.90 = zext i32 %addr.28 to i64  ; 0x401355
  %st.p.59 = inttoptr i64 %addr.z.90 to ptr  ; 0x401355
  store i32 %sp.31, ptr %st.p.59, align 1  ; 0x401355
  %addr.29 = add i32 %sum.10, -44  ; 0x401358
  %addr.z.91 = zext i32 %addr.29 to i64  ; 0x401358
  %m.p.12 = inttoptr i64 %addr.z.91 to ptr  ; 0x401358
  %m.12 = load i32, ptr %m.p.12, align 1  ; 0x401358
  %m.12.s.w = zext i32 %m.12 to i64  ; 0x401358
  %m.12.s = inttoptr i64 %m.12.s.w to ptr  ; 0x401358
  %m.12.f0 = getelementptr inbounds %S2, ptr %m.12.s, i64 0, i32 0  ; 0x40135b
  %m.13 = load i32, ptr %m.12.f0, align 1  ; 0x40135b
  %addr.30 = add i32 %m.13, 4  ; 0x40135d
  %addr.z.93 = zext i32 %addr.30 to i64  ; 0x40135d
  %m.p.14 = inttoptr i64 %addr.z.93 to ptr  ; 0x40135d
  %m.14 = load i32, ptr %m.p.14, align 1  ; 0x40135d
  %eax.z.15 = zext i32 %m.14 to i64  ; 0x40135d
  %eh.head.1.f8.9 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x401360
  store i32 1, ptr %eh.head.1.f8.9, align 1  ; 0x401360
  %sp.32 = sub i32 %sp.31, 4  ; 0x401367
  %is.fn.8 = icmp eq i64 %eax.z.15, ptrtoint (ptr @U_00401b10 to i64)  ; 0x401367
  %is.at.8 = icmp eq i32 %m.14, 4201232  ; 0x401367
  %is.8 = or i1 %is.fn.8, %is.at.8  ; 0x401367
  br i1 %is.8, label %vcall_105, label %vcall.test_109
eh.ok.401367:  ; 0x401367
  %eax.22 = trunc i64 %rax.phi.5 to i32  ; 0x401369
  %addr.z.96 = zext i32 %eax.22 to i64  ; 0x401369
  %m.p.15 = inttoptr i64 %addr.z.96 to ptr  ; 0x401369
  %m.15 = load i8, ptr %m.p.15, align 1  ; 0x401369
  %sx = sext i8 %m.15 to i32  ; 0x401369
  %sum.11 = add i32 %sx, 3000  ; 0x40136c
  %addr.32 = add i32 %sum.10, -32  ; 0x401371
  %addr.z.97 = zext i32 %addr.32 to i64  ; 0x401371
  %st.p.62 = inttoptr i64 %addr.z.97 to ptr  ; 0x401371
  store i32 %sum.11, ptr %st.p.62, align 1  ; 0x401371
  %sum.12 = add i32 %rsp.95, 4  ; 0x401379
  %sp.33 = add i32 %sum.12, 4  ; 0x40137c
  %sp.34 = add i32 %sp.33, 4  ; 0x40137d
  %eh.ending.3 = call i64 @ra_eh_ending_object()  ; 0x40137d
  %eh.dtor.3 = call i64 @ra_eh_ending_destructor()  ; 0x40137d
  call void @ra_eh_leave_catch()  ; 0x40137d
  %eh.dies.3 = icmp ne i64 %eh.dtor.3, 0  ; 0x40137d
  br i1 %eh.dies.3, label %eh.destroy.40137d, label %eh.destroyed.40137d
eh.destroy.40137d:  ; 0x40137d
  %eh.this.3 = trunc i64 %eh.ending.3 to i32  ; 0x40137d
  %ecx.z.7 = zext i32 %eh.this.3 to i64  ; 0x40137d
  %sp.35 = sub i32 %sp.34, 4  ; 0x40137d
  %addr.z.100 = zext i32 %sp.35 to i64  ; 0x40137d
  %st.p.63 = inttoptr i64 %addr.z.100 to ptr  ; 0x40137d
  store i32 0, ptr %st.p.63, align 1  ; 0x40137d
  %fp.6 = inttoptr i64 %eh.dtor.3 to ptr  ; 0x40137d
  %sarg0.a.7 = add i64 %addr.z.100, 4
  %sarg0.p.7 = inttoptr i64 %sarg0.a.7 to ptr
  %sarg0.d.7 = load i32, ptr %sarg0.p.7, align 4
  %sarg0.7 = zext i32 %sarg0.d.7 to i64
  %sarg1.a.6 = add i64 %addr.z.100, 8
  %sarg1.p.6 = inttoptr i64 %sarg1.a.6 to ptr
  %sarg1.d.6 = load i32, ptr %sarg1.p.6, align 4
  %sarg1.6 = zext i32 %sarg1.d.6 to i64
  %sarg2.a.6 = add i64 %addr.z.100, 12
  %sarg2.p.6 = inttoptr i64 %sarg2.a.6 to ptr
  %sarg2.d.6 = load i32, ptr %sarg2.p.6, align 4
  %sarg2.6 = zext i32 %sarg2.d.6 to i64
  %sarg3.a.6 = add i64 %addr.z.100, 16
  %sarg3.p.6 = inttoptr i64 %sarg3.a.6 to ptr
  %sarg3.d.6 = load i32, ptr %sarg3.p.6, align 4
  %sarg3.6 = zext i32 %sarg3.d.6 to i64
  %sarg4.a.6 = add i64 %addr.z.100, 20
  %sarg4.p.6 = inttoptr i64 %sarg4.a.6 to ptr
  %sarg4.d.6 = load i32, ptr %sarg4.p.6, align 4
  %sarg4.6 = zext i32 %sarg4.d.6 to i64
  %sarg5.a.6 = add i64 %addr.z.100, 24
  %sarg5.p.6 = inttoptr i64 %sarg5.a.6 to ptr
  %sarg5.d.6 = load i32, ptr %sarg5.p.6, align 4
  %sarg5.6 = zext i32 %sarg5.d.6 to i64
  %sarg6.a.6 = add i64 %addr.z.100, 28
  %sarg6.p.6 = inttoptr i64 %sarg6.a.6 to ptr
  %sarg6.d.6 = load i32, ptr %sarg6.p.6, align 4
  %sarg6.6 = zext i32 %sarg6.d.6 to i64
  %args.p.12 = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr  ; 0x40137d
  store i64 %sarg6.6, ptr %args.p.12, align 8  ; 0x40137d
  %sarg7.a.6 = add i64 %addr.z.100, 32
  %sarg7.p.6 = inttoptr i64 %sarg7.a.6 to ptr
  %sarg7.d.6 = load i32, ptr %sarg7.p.6, align 4
  %sarg7.6 = zext i32 %sarg7.d.6 to i64
  %args.p.13 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr  ; 0x40137d
  store i64 %sarg7.6, ptr %args.p.13, align 8  ; 0x40137d
  %ret.19 = call i64 %fp.6(i64 %sarg0.7, i64 %sarg1.6, i64 %sarg2.6, i64 %sarg3.6, i64 %sarg4.6, i64 %sarg5.6, i64 %ecx.z.7, i64 undef)  ; 0x40137d
  %pops.6 = call i64 @ra_pops_of(i64 %eh.dtor.3)  ; 0x40137d
  br label %eh.destroyed.40137d
eh.destroyed.40137d:  ; 0x40137d
  store i32 %eh.esp.8, ptr %eh.esp.p.12, align 1  ; 0x40137d
  br label %b_4012cc
eh.catch0_3.s0:  ; 0x40137d
  %eh.head.1.f8.10 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x40137d
  store i32 0, ptr %eh.head.1.f8.10, align 1  ; 0x40137d
  br label %eh.enter0_3
eh.next0_3.s0:  ; 0x40137d
  call void @ra_eh_suspend()  ; 0x401391
  %eh.head.1.f8.11 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x401391
  store i32 -1, ptr %eh.head.1.f8.11, align 1  ; 0x401391
  call void @ra_eh_resume()  ; 0x401391
  %eh.head.1.f0.1 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 0  ; 0x401391
  %eh.next.2 = load i32, ptr %eh.head.1.f0.1, align 1  ; 0x401391
  %teb.12 = call i64 @ra_teb()  ; 0x401391
  %teb.13 = trunc i64 %teb.12 to i32  ; 0x401391
  %addr.z.115 = zext i32 %teb.13 to i64  ; 0x401391
  %st.p.74 = inttoptr i64 %addr.z.115 to ptr  ; 0x401391
  store i32 %eh.next.2, ptr %st.p.74, align 1  ; 0x401391
  ret i64 %rax.phi.3
eh.enter0_3:  ; 0x40137d
  call void @ra_eh_enter_catch()  ; 0x40137d
  %eh.head.1.f8.12 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x40137d
  store i32 1, ptr %eh.head.1.f8.12, align 1  ; 0x40137d
  %eh.esp.p.14 = sub i32 %eh.reg.1, 4  ; 0x40137d
  %addr.z.104 = zext i32 %eh.esp.p.14 to i64  ; 0x40137d
  %eh.esp.p.15 = inttoptr i64 %addr.z.104 to ptr  ; 0x40137d
  %eh.esp.10 = load i32, ptr %eh.esp.p.15, align 1  ; 0x40137d
  %eh.esp.11 = zext i32 %eh.esp.10 to i64  ; 0x40137d
  call void @ra_eh_catch_sp(i64 %eh.esp.11)  ; 0x40137d
  %sp.36 = sub i32 %rsp.phi, 4  ; 0x40137d
  %addr.z.105 = zext i32 %sp.36 to i64  ; 0x40137d
  %st.p.67 = inttoptr i64 %addr.z.105 to ptr  ; 0x40137d
  store i32 0, ptr %st.p.67, align 1  ; 0x40137d
  br label %f6_401380
f6_401380:  ; 0x401380
  %sp.37 = sub i32 %sp.36, 4  ; 0x401380
  %addr.z.106 = zext i32 %sp.37 to i64  ; 0x401380
  %st.p.68 = inttoptr i64 %addr.z.106 to ptr  ; 0x401380
  store i32 %eh.frame.6, ptr %st.p.68, align 1  ; 0x401380
  %eax.26 = trunc i64 %rax.phi.3 to i32  ; 0x401381
  %sp.38 = sub i32 %sp.37, 4  ; 0x401381
  %addr.z.107 = zext i32 %sp.38 to i64  ; 0x401381
  %st.p.69 = inttoptr i64 %addr.z.107 to ptr  ; 0x401381
  store i32 %eax.26, ptr %st.p.69, align 1  ; 0x401381
  %sum.13 = add i32 %eh.frame.6, 12  ; 0x401382
  %addr.33 = add i32 %sum.13, -28  ; 0x401385
  %addr.z.108 = zext i32 %addr.33 to i64  ; 0x401385
  %st.p.70 = inttoptr i64 %addr.z.108 to ptr  ; 0x401385
  store i32 %sp.38, ptr %st.p.70, align 1  ; 0x401385
  %sum.14 = add i32 %sp.38, 4  ; 0x40138d
  %sp.39 = add i32 %sum.14, 4  ; 0x401390
  %sp.40 = add i32 %sp.39, 4  ; 0x401391
  %eh.ending.4 = call i64 @ra_eh_ending_object()  ; 0x401391
  %eh.dtor.4 = call i64 @ra_eh_ending_destructor()  ; 0x401391
  call void @ra_eh_leave_catch()  ; 0x401391
  %eh.dies.4 = icmp ne i64 %eh.dtor.4, 0  ; 0x401391
  br i1 %eh.dies.4, label %eh.destroy.401391, label %eh.destroyed.401391
eh.destroy.401391:  ; 0x401391
  %eh.this.4 = trunc i64 %eh.ending.4 to i32  ; 0x401391
  %ecx.z.8 = zext i32 %eh.this.4 to i64  ; 0x401391
  %sp.41 = sub i32 %sp.40, 4  ; 0x401391
  %addr.z.111 = zext i32 %sp.41 to i64  ; 0x401391
  %st.p.71 = inttoptr i64 %addr.z.111 to ptr  ; 0x401391
  store i32 0, ptr %st.p.71, align 1  ; 0x401391
  %fp.5 = inttoptr i64 %eh.dtor.4 to ptr  ; 0x401391
  %sarg0.a.6 = add i64 %addr.z.111, 4
  %sarg0.p.6 = inttoptr i64 %sarg0.a.6 to ptr
  %sarg0.d.6 = load i32, ptr %sarg0.p.6, align 4
  %sarg0.6 = zext i32 %sarg0.d.6 to i64
  %sarg1.a.5 = add i64 %addr.z.111, 8
  %sarg1.p.5 = inttoptr i64 %sarg1.a.5 to ptr
  %sarg1.d.5 = load i32, ptr %sarg1.p.5, align 4
  %sarg1.5 = zext i32 %sarg1.d.5 to i64
  %sarg2.a.5 = add i64 %addr.z.111, 12
  %sarg2.p.5 = inttoptr i64 %sarg2.a.5 to ptr
  %sarg2.d.5 = load i32, ptr %sarg2.p.5, align 4
  %sarg2.5 = zext i32 %sarg2.d.5 to i64
  %sarg3.a.5 = add i64 %addr.z.111, 16
  %sarg3.p.5 = inttoptr i64 %sarg3.a.5 to ptr
  %sarg3.d.5 = load i32, ptr %sarg3.p.5, align 4
  %sarg3.5 = zext i32 %sarg3.d.5 to i64
  %sarg4.a.5 = add i64 %addr.z.111, 20
  %sarg4.p.5 = inttoptr i64 %sarg4.a.5 to ptr
  %sarg4.d.5 = load i32, ptr %sarg4.p.5, align 4
  %sarg4.5 = zext i32 %sarg4.d.5 to i64
  %sarg5.a.5 = add i64 %addr.z.111, 24
  %sarg5.p.5 = inttoptr i64 %sarg5.a.5 to ptr
  %sarg5.d.5 = load i32, ptr %sarg5.p.5, align 4
  %sarg5.5 = zext i32 %sarg5.d.5 to i64
  %sarg6.a.5 = add i64 %addr.z.111, 28
  %sarg6.p.5 = inttoptr i64 %sarg6.a.5 to ptr
  %sarg6.d.5 = load i32, ptr %sarg6.p.5, align 4
  %sarg6.5 = zext i32 %sarg6.d.5 to i64
  %args.p.10 = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr  ; 0x401391
  store i64 %sarg6.5, ptr %args.p.10, align 8  ; 0x401391
  %sarg7.a.5 = add i64 %addr.z.111, 32
  %sarg7.p.5 = inttoptr i64 %sarg7.a.5 to ptr
  %sarg7.d.5 = load i32, ptr %sarg7.p.5, align 4
  %sarg7.5 = zext i32 %sarg7.d.5 to i64
  %args.p.11 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr  ; 0x401391
  store i64 %sarg7.5, ptr %args.p.11, align 8  ; 0x401391
  %ret.14 = call i64 %fp.5(i64 %sarg0.6, i64 %sarg1.5, i64 %sarg2.5, i64 %sarg3.5, i64 %sarg4.5, i64 %sarg5.5, i64 %ecx.z.8, i64 undef)  ; 0x401391
  %pops.5 = call i64 @ra_pops_of(i64 %eh.dtor.4)  ; 0x401391
  br label %eh.destroyed.401391
eh.destroyed.401391:  ; 0x401391
  store i32 %eh.esp.10, ptr %eh.esp.p.15, align 1  ; 0x401391
  br label %b_4012d4
eh.state1:  ; 0x401391
  call void @ra_eh_suspend()  ; 0x401391
  %eh.esp.saved = call i64 @ra_eh_ending_sp()  ; 0x401391
  %eh.esp.saved.1 = trunc i64 %eh.esp.saved to i32  ; 0x401391
  %eh.esp.p.17 = sub i32 %eh.reg.1, 4  ; 0x401391
  %addr.z.116 = zext i32 %eh.esp.p.17 to i64  ; 0x401391
  %st.p.75 = inttoptr i64 %addr.z.116 to ptr  ; 0x401391
  store i32 %eh.esp.saved.1, ptr %st.p.75, align 1  ; 0x401391
  %eh.ending.5 = call i64 @ra_eh_ending_object()  ; 0x401391
  %eh.dtor.5 = call i64 @ra_eh_ending_destructor()  ; 0x401391
  call void @ra_eh_leave_catch()  ; 0x401391
  %eh.dies.5 = icmp ne i64 %eh.dtor.5, 0  ; 0x401391
  br i1 %eh.dies.5, label %eh.destroy.401260, label %eh.destroyed.401260
eh.destroy.401260:  ; 0x401391
  %eh.this.5 = trunc i64 %eh.ending.5 to i32  ; 0x401391
  %ecx.z.9 = zext i32 %eh.this.5 to i64  ; 0x401391
  %sp.42 = sub i32 %rsp.phi, 4  ; 0x401391
  %addr.z.117 = zext i32 %sp.42 to i64  ; 0x401391
  %st.p.76 = inttoptr i64 %addr.z.117 to ptr  ; 0x401391
  store i32 0, ptr %st.p.76, align 1  ; 0x401391
  %fp.4 = inttoptr i64 %eh.dtor.5 to ptr  ; 0x401391
  %sarg0.a.5 = add i64 %addr.z.117, 4
  %sarg0.p.5 = inttoptr i64 %sarg0.a.5 to ptr
  %sarg0.d.5 = load i32, ptr %sarg0.p.5, align 4
  %sarg0.5 = zext i32 %sarg0.d.5 to i64
  %sarg1.a.4 = add i64 %addr.z.117, 8
  %sarg1.p.4 = inttoptr i64 %sarg1.a.4 to ptr
  %sarg1.d.4 = load i32, ptr %sarg1.p.4, align 4
  %sarg1.4 = zext i32 %sarg1.d.4 to i64
  %sarg2.a.4 = add i64 %addr.z.117, 12
  %sarg2.p.4 = inttoptr i64 %sarg2.a.4 to ptr
  %sarg2.d.4 = load i32, ptr %sarg2.p.4, align 4
  %sarg2.4 = zext i32 %sarg2.d.4 to i64
  %sarg3.a.4 = add i64 %addr.z.117, 16
  %sarg3.p.4 = inttoptr i64 %sarg3.a.4 to ptr
  %sarg3.d.4 = load i32, ptr %sarg3.p.4, align 4
  %sarg3.4 = zext i32 %sarg3.d.4 to i64
  %sarg4.a.4 = add i64 %addr.z.117, 20
  %sarg4.p.4 = inttoptr i64 %sarg4.a.4 to ptr
  %sarg4.d.4 = load i32, ptr %sarg4.p.4, align 4
  %sarg4.4 = zext i32 %sarg4.d.4 to i64
  %sarg5.a.4 = add i64 %addr.z.117, 24
  %sarg5.p.4 = inttoptr i64 %sarg5.a.4 to ptr
  %sarg5.d.4 = load i32, ptr %sarg5.p.4, align 4
  %sarg5.4 = zext i32 %sarg5.d.4 to i64
  %sarg6.a.4 = add i64 %addr.z.117, 28
  %sarg6.p.4 = inttoptr i64 %sarg6.a.4 to ptr
  %sarg6.d.4 = load i32, ptr %sarg6.p.4, align 4
  %sarg6.4 = zext i32 %sarg6.d.4 to i64
  %args.p.8 = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr  ; 0x401391
  store i64 %sarg6.4, ptr %args.p.8, align 8  ; 0x401391
  %sarg7.a.4 = add i64 %addr.z.117, 32
  %sarg7.p.4 = inttoptr i64 %sarg7.a.4 to ptr
  %sarg7.d.4 = load i32, ptr %sarg7.p.4, align 4
  %sarg7.4 = zext i32 %sarg7.d.4 to i64
  %args.p.9 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr  ; 0x401391
  store i64 %sarg7.4, ptr %args.p.9, align 8  ; 0x401391
  %ret.13 = call i64 %fp.4(i64 %sarg0.5, i64 %sarg1.4, i64 %sarg2.4, i64 %sarg3.4, i64 %sarg4.4, i64 %sarg5.4, i64 %ecx.z.9, i64 undef)  ; 0x401391
  %pops.4 = call i64 @ra_pops_of(i64 %eh.dtor.5)  ; 0x401391
  br label %eh.destroyed.401260
eh.destroyed.401260:  ; 0x401391
  %rax.phi.4 = phi i64 [ %rax.phi.3, %eh.state1 ], [ %ret.13, %eh.destroy.401260 ]  ; 0x401391
  %eh.head.1.f8.13 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x401391
  store i32 -1, ptr %eh.head.1.f8.13, align 1  ; 0x401391
  call void @ra_eh_resume()  ; 0x401391
  %eh.head.1.f0.2 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 0  ; 0x401391
  %eh.next.3 = load i32, ptr %eh.head.1.f0.2, align 1  ; 0x401391
  %teb.14 = call i64 @ra_teb()  ; 0x401391
  %teb.15 = trunc i64 %teb.14 to i32  ; 0x401391
  %addr.z.120 = zext i32 %teb.15 to i64  ; 0x401391
  %st.p.78 = inttoptr i64 %addr.z.120 to ptr  ; 0x401391
  store i32 %eh.next.3, ptr %st.p.78, align 1  ; 0x401391
  ret i64 %rax.phi.4
eh.ok.40132d.1:  ; 0x401391
  %eax.28 = trunc i64 %rax.phi.2 to i32  ; 0x40132f
  %scaled.3 = mul i32 %m.5, 10  ; 0x40132f
  %addr.34 = add i32 %eax.28, %scaled.3  ; 0x40132f
  %sum.15 = add i32 %addr.34, 2000  ; 0x401332
  %addr.35 = add i32 %sum.3, -32  ; 0x401337
  %addr.z.121 = zext i32 %addr.35 to i64  ; 0x401337
  %st.p.79 = inttoptr i64 %addr.z.121 to ptr  ; 0x401337
  store i32 %sum.15, ptr %st.p.79, align 1  ; 0x401337
  %eh.ending.6 = call i64 @ra_eh_ending_object()  ; 0x401343
  %eh.dtor.6 = call i64 @ra_eh_ending_destructor()  ; 0x401343
  call void @ra_eh_leave_catch()  ; 0x401343
  %eh.dies.6 = icmp ne i64 %eh.dtor.6, 0  ; 0x401343
  br i1 %eh.dies.6, label %eh.destroy.401343.1, label %eh.destroyed.401343.1
eh.destroy.401343.1:  ; 0x401343
  %eh.this.6 = trunc i64 %eh.ending.6 to i32  ; 0x401343
  %ecx.z.10 = zext i32 %eh.this.6 to i64  ; 0x401343
  store i32 0, ptr %st.p.26, align 1  ; 0x401343
  %fp.3 = inttoptr i64 %eh.dtor.6 to ptr  ; 0x401343
  %sarg0.d.4 = load i32, ptr %st.p.11, align 4
  %sarg0.4 = zext i32 %sarg0.d.4 to i64
  %sarg1.p.3 = getelementptr inbounds i8, ptr %frame.top, i64 -48
  %sarg1.d.3 = load i32, ptr %sarg1.p.3, align 4
  %sarg1.3 = zext i32 %sarg1.d.3 to i64
  %sarg2.p.3 = getelementptr inbounds i8, ptr %frame.top, i64 -44
  %sarg2.d.3 = load i32, ptr %sarg2.p.3, align 4
  %sarg2.3 = zext i32 %sarg2.d.3 to i64
  %sarg3.p.3 = getelementptr inbounds i8, ptr %frame.top, i64 -40
  %sarg3.d.3 = load i32, ptr %sarg3.p.3, align 4
  %sarg3.3 = zext i32 %sarg3.d.3 to i64
  %sarg4.d.3 = load i32, ptr %st.p.10, align 4
  %sarg4.3 = zext i32 %sarg4.d.3 to i64
  %sarg5.d.3 = load i32, ptr %st.p.4, align 4
  %sarg5.3 = zext i32 %sarg5.d.3 to i64
  %sarg6.d.3 = load i32, ptr %st.p.7, align 4
  %sarg6.3 = zext i32 %sarg6.d.3 to i64
  %args.p.6 = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr  ; 0x401343
  store i64 %sarg6.3, ptr %args.p.6, align 8  ; 0x401343
  %sarg7.d.3 = load i32, ptr %st.p.6, align 4
  %sarg7.3 = zext i32 %sarg7.d.3 to i64
  %args.p.7 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr  ; 0x401343
  store i64 %sarg7.3, ptr %args.p.7, align 8  ; 0x401343
  %ret.12 = call i64 %fp.3(i64 %sarg0.4, i64 %sarg1.3, i64 %sarg2.3, i64 %sarg3.3, i64 %sarg4.3, i64 %sarg5.3, i64 %ecx.z.10, i64 %edx.z.1)  ; 0x401343
  %pops.3 = call i64 @ra_pops_of(i64 %eh.dtor.6)  ; 0x401343
  br label %eh.destroyed.401343.1
eh.destroyed.401343.1:  ; 0x401343
  store i32 %eh.esp.2, ptr %eh.esp.p.4, align 1  ; 0x401343
  br label %b_4012c4
eh.catch0_2.s0.1:  ; 0x401343
  %eh.head.f8.7 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x401343
  store i32 0, ptr %eh.head.f8.7, align 1  ; 0x401343
  br label %eh.enter0_2.1
eh.next0_2.s0.1:  ; 0x401343
  %eh.caught.7 = call i64 @ra_eh_catches(i64 0, i64 0, i64 8256)  ; 0x40137d
  %eh.hit.7 = icmp ne i64 %eh.caught.7, 0  ; 0x40137d
  br i1 %eh.hit.7, label %eh.catch0_3.s0.1, label %eh.next0_3.s0.1
eh.enter0_2.1:  ; 0x401343
  call void @ra_eh_enter_catch()  ; 0x401343
  %eh.head.f8.8 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x401343
  store i32 1, ptr %eh.head.f8.8, align 1  ; 0x401343
  %eh.esp.p.19 = sub i32 %eh.reg, 4  ; 0x401343
  %addr.z.128 = zext i32 %eh.esp.p.19 to i64  ; 0x401343
  %eh.esp.p.20 = inttoptr i64 %addr.z.128 to ptr  ; 0x401343
  %eh.esp.12 = load i32, ptr %eh.esp.p.20, align 1  ; 0x401343
  %eh.esp.13 = zext i32 %eh.esp.12 to i64  ; 0x401343
  call void @ra_eh_catch_sp(i64 %eh.esp.13)  ; 0x401343
  %sp.46 = sub i32 %sarg0.a, 4  ; 0x401343
  %st.p.84 = getelementptr inbounds i8, ptr %frame.top, i64 -56  ; 0x401343
  store i32 0, ptr %st.p.84, align 1  ; 0x401343
  br label %f7_401350
f7_401350:  ; 0x401350
  %sp.47 = sub i32 %sp.46, 4  ; 0x401350
  %st.p.85 = getelementptr inbounds i8, ptr %frame.top, i64 -60  ; 0x401350
  store i32 %eh.frame.1, ptr %st.p.85, align 1  ; 0x401350
  %eax.32 = trunc i64 %ret to i32  ; 0x401351
  %sp.48 = sub i32 %sp.47, 4  ; 0x401351
  %st.p.86 = getelementptr inbounds i8, ptr %frame.top, i64 -64  ; 0x401351
  store i32 %eax.32, ptr %st.p.86, align 1  ; 0x401351
  %sum.17 = add i32 %eh.frame.1, 12  ; 0x401352
  %addr.36 = add i32 %sum.17, -28  ; 0x401355
  %addr.z.132 = zext i32 %addr.36 to i64  ; 0x401355
  %st.p.87 = inttoptr i64 %addr.z.132 to ptr  ; 0x401355
  store i32 %sp.48, ptr %st.p.87, align 1  ; 0x401355
  %addr.37 = add i32 %sum.17, -44  ; 0x401358
  %addr.z.133 = zext i32 %addr.37 to i64  ; 0x401358
  %m.p.16 = inttoptr i64 %addr.z.133 to ptr  ; 0x401358
  %m.16 = load i32, ptr %m.p.16, align 1  ; 0x401358
  %m.16.s.w = zext i32 %m.16 to i64  ; 0x401358
  %m.16.s = inttoptr i64 %m.16.s.w to ptr  ; 0x401358
  %m.16.f0 = getelementptr inbounds %S2, ptr %m.16.s, i64 0, i32 0  ; 0x40135b
  %m.17 = load i32, ptr %m.16.f0, align 1  ; 0x40135b
  %addr.38 = add i32 %m.17, 4  ; 0x40135d
  %addr.z.135 = zext i32 %addr.38 to i64  ; 0x40135d
  %m.p.18 = inttoptr i64 %addr.z.135 to ptr  ; 0x40135d
  %m.18 = load i32, ptr %m.p.18, align 1  ; 0x40135d
  %eax.z.24 = zext i32 %m.18 to i64  ; 0x40135d
  %eh.head.f8.9 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x401360
  store i32 1, ptr %eh.head.f8.9, align 1  ; 0x401360
  %sp.49 = sub i32 %sp.48, 4  ; 0x401367
  %is.fn = icmp eq i64 %eax.z.24, ptrtoint (ptr @U_00401b10 to i64)  ; 0x401367
  %is.at = icmp eq i32 %m.18, 4201232  ; 0x401367
  %is = or i1 %is.fn, %is.at  ; 0x401367
  br i1 %is, label %vcall_87, label %vcall.test_91
eh.ok.401367.1:  ; 0x401367
  %eax.35 = trunc i64 %rax.phi.1 to i32  ; 0x401369
  %addr.z.138 = zext i32 %eax.35 to i64  ; 0x401369
  %m.p.19 = inttoptr i64 %addr.z.138 to ptr  ; 0x401369
  %m.19 = load i8, ptr %m.p.19, align 1  ; 0x401369
  %sx.1 = sext i8 %m.19 to i32  ; 0x401369
  %sum.18 = add i32 %sx.1, 3000  ; 0x40136c
  %addr.40 = add i32 %sum.17, -32  ; 0x401371
  %addr.z.139 = zext i32 %addr.40 to i64  ; 0x401371
  %st.p.90 = inttoptr i64 %addr.z.139 to ptr  ; 0x401371
  store i32 %sum.18, ptr %st.p.90, align 1  ; 0x401371
  %eh.ending.7 = call i64 @ra_eh_ending_object()  ; 0x40137d
  %eh.dtor.7 = call i64 @ra_eh_ending_destructor()  ; 0x40137d
  call void @ra_eh_leave_catch()  ; 0x40137d
  %eh.dies.7 = icmp ne i64 %eh.dtor.7, 0  ; 0x40137d
  br i1 %eh.dies.7, label %eh.destroy.40137d.1, label %eh.destroyed.40137d.1
eh.destroy.40137d.1:  ; 0x40137d
  %eh.this.7 = trunc i64 %eh.ending.7 to i32  ; 0x40137d
  %ecx.z.12 = zext i32 %eh.this.7 to i64  ; 0x40137d
  store i32 0, ptr %st.p.84, align 1  ; 0x40137d
  %fp.2 = inttoptr i64 %eh.dtor.7 to ptr  ; 0x40137d
  %sarg0.d.3 = load i32, ptr %st.p.11, align 4
  %sarg0.3 = zext i32 %sarg0.d.3 to i64
  %sarg1.p.2 = getelementptr inbounds i8, ptr %frame.top, i64 -48
  %sarg1.d.2 = load i32, ptr %sarg1.p.2, align 4
  %sarg1.2 = zext i32 %sarg1.d.2 to i64
  %sarg2.p.2 = getelementptr inbounds i8, ptr %frame.top, i64 -44
  %sarg2.d.2 = load i32, ptr %sarg2.p.2, align 4
  %sarg2.2 = zext i32 %sarg2.d.2 to i64
  %sarg3.p.2 = getelementptr inbounds i8, ptr %frame.top, i64 -40
  %sarg3.d.2 = load i32, ptr %sarg3.p.2, align 4
  %sarg3.2 = zext i32 %sarg3.d.2 to i64
  %sarg4.d.2 = load i32, ptr %st.p.10, align 4
  %sarg4.2 = zext i32 %sarg4.d.2 to i64
  %sarg5.d.2 = load i32, ptr %st.p.4, align 4
  %sarg5.2 = zext i32 %sarg5.d.2 to i64
  %sarg6.d.2 = load i32, ptr %st.p.7, align 4
  %sarg6.2 = zext i32 %sarg6.d.2 to i64
  %args.p.4 = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr  ; 0x40137d
  store i64 %sarg6.2, ptr %args.p.4, align 8  ; 0x40137d
  %sarg7.d.2 = load i32, ptr %st.p.6, align 4
  %sarg7.2 = zext i32 %sarg7.d.2 to i64
  %args.p.5 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr  ; 0x40137d
  store i64 %sarg7.2, ptr %args.p.5, align 8  ; 0x40137d
  %ret.7 = call i64 %fp.2(i64 %sarg0.3, i64 %sarg1.2, i64 %sarg2.2, i64 %sarg3.2, i64 %sarg4.2, i64 %sarg5.2, i64 %ecx.z.12, i64 %edx.z)  ; 0x40137d
  %pops.2 = call i64 @ra_pops_of(i64 %eh.dtor.7)  ; 0x40137d
  br label %eh.destroyed.40137d.1
eh.destroyed.40137d.1:  ; 0x40137d
  store i32 %eh.esp.12, ptr %eh.esp.p.20, align 1  ; 0x40137d
  br label %b_4012cc
eh.catch0_3.s0.1:  ; 0x40137d
  %eh.head.f8.10 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x40137d
  store i32 0, ptr %eh.head.f8.10, align 1  ; 0x40137d
  br label %eh.enter0_3.1
eh.next0_3.s0.1:  ; 0x40137d
  call void @ra_eh_suspend()  ; 0x401391
  %eh.head.f8.11 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x401391
  store i32 -1, ptr %eh.head.f8.11, align 1  ; 0x401391
  call void @ra_eh_resume()  ; 0x401391
  %eh.head.f0.1 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 0  ; 0x401391
  %eh.next.4 = load i32, ptr %eh.head.f0.1, align 1  ; 0x401391
  %teb.16 = call i64 @ra_teb()  ; 0x401391
  %teb.17 = trunc i64 %teb.16 to i32  ; 0x401391
  %addr.z.157 = zext i32 %teb.17 to i64  ; 0x401391
  %st.p.102 = inttoptr i64 %addr.z.157 to ptr  ; 0x401391
  store i32 %eh.next.4, ptr %st.p.102, align 1  ; 0x401391
  ret i64 %ret
eh.enter0_3.1:  ; 0x40137d
  call void @ra_eh_enter_catch()  ; 0x40137d
  %eh.head.f8.12 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x40137d
  store i32 1, ptr %eh.head.f8.12, align 1  ; 0x40137d
  %eh.esp.p.22 = sub i32 %eh.reg, 4  ; 0x40137d
  %addr.z.146 = zext i32 %eh.esp.p.22 to i64  ; 0x40137d
  %eh.esp.p.23 = inttoptr i64 %addr.z.146 to ptr  ; 0x40137d
  %eh.esp.14 = load i32, ptr %eh.esp.p.23, align 1  ; 0x40137d
  %eh.esp.15 = zext i32 %eh.esp.14 to i64  ; 0x40137d
  call void @ra_eh_catch_sp(i64 %eh.esp.15)  ; 0x40137d
  %sp.53 = sub i32 %sarg0.a, 4  ; 0x40137d
  %st.p.95 = getelementptr inbounds i8, ptr %frame.top, i64 -56  ; 0x40137d
  store i32 0, ptr %st.p.95, align 1  ; 0x40137d
  br label %f8_401380
f8_401380:  ; 0x401380
  %sp.54 = sub i32 %sp.53, 4  ; 0x401380
  %st.p.96 = getelementptr inbounds i8, ptr %frame.top, i64 -60  ; 0x401380
  store i32 %eh.frame.1, ptr %st.p.96, align 1  ; 0x401380
  %eax.39 = trunc i64 %ret to i32  ; 0x401381
  %sp.55 = sub i32 %sp.54, 4  ; 0x401381
  %st.p.97 = getelementptr inbounds i8, ptr %frame.top, i64 -64  ; 0x401381
  store i32 %eax.39, ptr %st.p.97, align 1  ; 0x401381
  %sum.20 = add i32 %eh.frame.1, 12  ; 0x401382
  %addr.41 = add i32 %sum.20, -28  ; 0x401385
  %addr.z.150 = zext i32 %addr.41 to i64  ; 0x401385
  %st.p.98 = inttoptr i64 %addr.z.150 to ptr  ; 0x401385
  store i32 %sp.55, ptr %st.p.98, align 1  ; 0x401385
  %eh.ending.8 = call i64 @ra_eh_ending_object()  ; 0x401391
  %eh.dtor.8 = call i64 @ra_eh_ending_destructor()  ; 0x401391
  call void @ra_eh_leave_catch()  ; 0x401391
  %eh.dies.8 = icmp ne i64 %eh.dtor.8, 0  ; 0x401391
  br i1 %eh.dies.8, label %eh.destroy.401391.1, label %eh.destroyed.401391.1
eh.destroy.401391.1:  ; 0x401391
  %eh.this.8 = trunc i64 %eh.ending.8 to i32  ; 0x401391
  %ecx.z.13 = zext i32 %eh.this.8 to i64  ; 0x401391
  store i32 0, ptr %st.p.95, align 1  ; 0x401391
  %fp.1 = inttoptr i64 %eh.dtor.8 to ptr  ; 0x401391
  %sarg0.d.2 = load i32, ptr %st.p.11, align 4
  %sarg0.2 = zext i32 %sarg0.d.2 to i64
  %sarg1.p.1 = getelementptr inbounds i8, ptr %frame.top, i64 -48
  %sarg1.d.1 = load i32, ptr %sarg1.p.1, align 4
  %sarg1.1 = zext i32 %sarg1.d.1 to i64
  %sarg2.p.1 = getelementptr inbounds i8, ptr %frame.top, i64 -44
  %sarg2.d.1 = load i32, ptr %sarg2.p.1, align 4
  %sarg2.1 = zext i32 %sarg2.d.1 to i64
  %sarg3.p.1 = getelementptr inbounds i8, ptr %frame.top, i64 -40
  %sarg3.d.1 = load i32, ptr %sarg3.p.1, align 4
  %sarg3.1 = zext i32 %sarg3.d.1 to i64
  %sarg4.d.1 = load i32, ptr %st.p.10, align 4
  %sarg4.1 = zext i32 %sarg4.d.1 to i64
  %sarg5.d.1 = load i32, ptr %st.p.4, align 4
  %sarg5.1 = zext i32 %sarg5.d.1 to i64
  %sarg6.d.1 = load i32, ptr %st.p.7, align 4
  %sarg6.1 = zext i32 %sarg6.d.1 to i64
  %args.p.2 = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr  ; 0x401391
  store i64 %sarg6.1, ptr %args.p.2, align 8  ; 0x401391
  %sarg7.d.1 = load i32, ptr %st.p.6, align 4
  %sarg7.1 = zext i32 %sarg7.d.1 to i64
  %args.p.3 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr  ; 0x401391
  store i64 %sarg7.1, ptr %args.p.3, align 8  ; 0x401391
  %ret.2 = call i64 %fp.1(i64 %sarg0.2, i64 %sarg1.1, i64 %sarg2.1, i64 %sarg3.1, i64 %sarg4.1, i64 %sarg5.1, i64 %ecx.z.13, i64 %edx.z)  ; 0x401391
  %pops.1 = call i64 @ra_pops_of(i64 %eh.dtor.8)  ; 0x401391
  br label %eh.destroyed.401391.1
eh.destroyed.401391.1:  ; 0x401391
  store i32 %eh.esp.14, ptr %eh.esp.p.23, align 1  ; 0x401391
  br label %b_4012d4
eh.state1.1:  ; 0x401391
  call void @ra_eh_suspend()  ; 0x401391
  %eh.esp.saved.2 = call i64 @ra_eh_ending_sp()  ; 0x401391
  %eh.esp.saved.3 = trunc i64 %eh.esp.saved.2 to i32  ; 0x401391
  %eh.esp.p.25 = sub i32 %eh.reg, 4  ; 0x401391
  %addr.z.158 = zext i32 %eh.esp.p.25 to i64  ; 0x401391
  %st.p.103 = inttoptr i64 %addr.z.158 to ptr  ; 0x401391
  store i32 %eh.esp.saved.3, ptr %st.p.103, align 1  ; 0x401391
  %eh.ending.9 = call i64 @ra_eh_ending_object()  ; 0x401391
  %eh.dtor.9 = call i64 @ra_eh_ending_destructor()  ; 0x401391
  call void @ra_eh_leave_catch()  ; 0x401391
  %eh.dies.9 = icmp ne i64 %eh.dtor.9, 0  ; 0x401391
  br i1 %eh.dies.9, label %eh.destroy.401260.1, label %eh.destroyed.401260.1
eh.destroy.401260.1:  ; 0x401391
  %eh.this.9 = trunc i64 %eh.ending.9 to i32  ; 0x401391
  %ecx.z.14 = zext i32 %eh.this.9 to i64  ; 0x401391
  %st.p.104 = getelementptr inbounds i8, ptr %frame.top, i64 -56  ; 0x401391
  store i32 0, ptr %st.p.104, align 1  ; 0x401391
  %fp = inttoptr i64 %eh.dtor.9 to ptr  ; 0x401391
  %sarg0.d.1 = load i32, ptr %st.p.11, align 4
  %sarg0.1 = zext i32 %sarg0.d.1 to i64
  %sarg1.p = getelementptr inbounds i8, ptr %frame.top, i64 -48
  %sarg1.d = load i32, ptr %sarg1.p, align 4
  %sarg1 = zext i32 %sarg1.d to i64
  %sarg2.p = getelementptr inbounds i8, ptr %frame.top, i64 -44
  %sarg2.d = load i32, ptr %sarg2.p, align 4
  %sarg2 = zext i32 %sarg2.d to i64
  %sarg3.p = getelementptr inbounds i8, ptr %frame.top, i64 -40
  %sarg3.d = load i32, ptr %sarg3.p, align 4
  %sarg3 = zext i32 %sarg3.d to i64
  %sarg4.d = load i32, ptr %st.p.10, align 4
  %sarg4 = zext i32 %sarg4.d to i64
  %sarg5.d = load i32, ptr %st.p.4, align 4
  %sarg5 = zext i32 %sarg5.d to i64
  %sarg6.d = load i32, ptr %st.p.7, align 4
  %sarg6 = zext i32 %sarg6.d to i64
  %args.p = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr  ; 0x401391
  store i64 %sarg6, ptr %args.p, align 8  ; 0x401391
  %sarg7.d = load i32, ptr %st.p.6, align 4
  %sarg7 = zext i32 %sarg7.d to i64
  %args.p.1 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr  ; 0x401391
  store i64 %sarg7, ptr %args.p.1, align 8  ; 0x401391
  %ret.1 = call i64 %fp(i64 %sarg0.1, i64 %sarg1, i64 %sarg2, i64 %sarg3, i64 %sarg4, i64 %sarg5, i64 %ecx.z.14, i64 %edx.z)  ; 0x401391
  %pops = call i64 @ra_pops_of(i64 %eh.dtor.9)  ; 0x401391
  br label %eh.destroyed.401260.1
eh.destroyed.401260.1:  ; 0x401391
  %rax.phi = phi i64 [ %ret, %eh.state1.1 ], [ %ret.1, %eh.destroy.401260.1 ]  ; 0x401391
  %eh.head.f8.13 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x401391
  store i32 -1, ptr %eh.head.f8.13, align 1  ; 0x401391
  call void @ra_eh_resume()  ; 0x401391
  %eh.head.f0.2 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 0  ; 0x401391
  %eh.next.5 = load i32, ptr %eh.head.f0.2, align 1  ; 0x401391
  %teb.18 = call i64 @ra_teb()  ; 0x401391
  %teb.19 = trunc i64 %teb.18 to i32  ; 0x401391
  %addr.z.162 = zext i32 %teb.19 to i64  ; 0x401391
  %st.p.106 = inttoptr i64 %addr.z.162 to ptr  ; 0x401391
  store i32 %eh.next.5, ptr %st.p.106, align 1  ; 0x401391
  ret i64 %rax.phi
eh.ok.4012a0:  ; 0x401391
  br label %b_4012a8
vcall_87:  ; 0x401367
  %ret.3 = call i64 @Derived_vf1()  ; 0x401367
  br label %f7_401350.85
vcall.test_91:  ; 0x401367
  %is.fn.1 = icmp eq i64 %eax.z.24, ptrtoint (ptr @U_00401b20 to i64)  ; 0x401367
  %is.at.1 = icmp eq i32 %m.18, 4201248  ; 0x401367
  %is.1 = or i1 %is.fn.1, %is.at.1  ; 0x401367
  br i1 %is.1, label %vcall_88, label %vcall.test_92
vcall_88:  ; 0x401367
  %ret.4 = call i64 @Base_vf1()  ; 0x401367
  br label %f7_401350.85
vcall.test_92:  ; 0x401367
  %is.fn.2 = icmp eq i64 %eax.z.24, ptrtoint (ptr @U_00401b50 to i64)  ; 0x401367
  %is.at.2 = icmp eq i32 %m.18, 4201296  ; 0x401367
  %is.2 = or i1 %is.fn.2, %is.at.2  ; 0x401367
  br i1 %is.2, label %vcall_89, label %vcall.test_93
vcall_89:  ; 0x401367
  %ret.5 = call i64 @Problem_vf1()  ; 0x401367
  br label %f7_401350.85
vcall.test_93:  ; 0x401367
  %is.fn.3 = icmp eq i64 %eax.z.24, ptrtoint (ptr @U_00401b60 to i64)  ; 0x401367
  %is.at.3 = icmp eq i32 %m.18, 4201312  ; 0x401367
  %is.3 = or i1 %is.fn.3, %is.at.3  ; 0x401367
  br i1 %is.3, label %vcall_90, label %vcall.trap_86
vcall_90:  ; 0x401367
  %ret.6 = call i64 @std_exception_vf1(i64 %m.16.s.w)  ; 0x401367
  br label %f7_401350.85
vcall.trap_86:  ; 0x401367
  call void @ra_trap(i64 4199271, i64 10)  ; 0x401367
  unreachable
f7_401350.85:  ; 0x401350
  %rax.phi.1 = phi i64 [ %ret.3, %vcall_87 ], [ %ret.4, %vcall_88 ], [ %ret.5, %vcall_89 ], [ %ret.6, %vcall_90 ]  ; 0x401367
  %rsp.89 = add i32 %sp.49, 4  ; 0x401367
  %eh.pending.4 = call i64 @ra_eh_pending()  ; 0x401367
  %eh.thrown.4 = icmp ne i64 %eh.pending.4, 0  ; 0x401367
  br i1 %eh.thrown.4, label %eh.landing.1, label %eh.ok.401367.1
vcall_96:  ; 0x40132d
  %ret.8 = call i64 @Derived_vf1()  ; 0x40132d
  br label %f2_401310.94
vcall.test_100:  ; 0x40132d
  %is.fn.5 = icmp eq i64 %eax.z.5, ptrtoint (ptr @U_00401b20 to i64)  ; 0x40132d
  %is.at.5 = icmp eq i32 %m.6, 4201248  ; 0x40132d
  %is.5 = or i1 %is.fn.5, %is.at.5  ; 0x40132d
  br i1 %is.5, label %vcall_97, label %vcall.test_101
vcall_97:  ; 0x40132d
  %ret.9 = call i64 @Base_vf1()  ; 0x40132d
  br label %f2_401310.94
vcall.test_101:  ; 0x40132d
  %is.fn.6 = icmp eq i64 %eax.z.5, ptrtoint (ptr @U_00401b50 to i64)  ; 0x40132d
  %is.at.6 = icmp eq i32 %m.6, 4201296  ; 0x40132d
  %is.6 = or i1 %is.fn.6, %is.at.6  ; 0x40132d
  br i1 %is.6, label %vcall_98, label %vcall.test_102
vcall_98:  ; 0x40132d
  %ret.10 = call i64 @Problem_vf1()  ; 0x40132d
  br label %f2_401310.94
vcall.test_102:  ; 0x40132d
  %is.fn.7 = icmp eq i64 %eax.z.5, ptrtoint (ptr @U_00401b60 to i64)  ; 0x40132d
  %is.at.7 = icmp eq i32 %m.6, 4201312  ; 0x40132d
  %is.7 = or i1 %is.fn.7, %is.at.7  ; 0x40132d
  br i1 %is.7, label %vcall_99, label %vcall.trap_95
vcall_99:  ; 0x40132d
  %ret.11 = call i64 @std_exception_vf1(i64 %m.3.s.w)  ; 0x40132d
  br label %f2_401310.94
vcall.trap_95:  ; 0x40132d
  call void @ra_trap(i64 4199213, i64 10)  ; 0x40132d
  unreachable
f2_401310.94:  ; 0x401310
  %rax.phi.2 = phi i64 [ %ret.8, %vcall_96 ], [ %ret.9, %vcall_97 ], [ %ret.10, %vcall_98 ], [ %ret.11, %vcall_99 ]  ; 0x40132d
  %rsp.91 = add i32 %sp.15, 4  ; 0x40132d
  %eh.pending.1 = call i64 @ra_eh_pending()  ; 0x40132d
  %eh.thrown.1 = icmp ne i64 %eh.pending.1, 0  ; 0x40132d
  br i1 %eh.thrown.1, label %eh.landing.1, label %eh.ok.40132d.1
vcall_105:  ; 0x401367
  %ret.15 = call i64 @Derived_vf1()  ; 0x401367
  br label %f5_401350.103
vcall.test_109:  ; 0x401367
  %is.fn.9 = icmp eq i64 %eax.z.15, ptrtoint (ptr @U_00401b20 to i64)  ; 0x401367
  %is.at.9 = icmp eq i32 %m.14, 4201248  ; 0x401367
  %is.9 = or i1 %is.fn.9, %is.at.9  ; 0x401367
  br i1 %is.9, label %vcall_106, label %vcall.test_110
vcall_106:  ; 0x401367
  %ret.16 = call i64 @Base_vf1()  ; 0x401367
  br label %f5_401350.103
vcall.test_110:  ; 0x401367
  %is.fn.10 = icmp eq i64 %eax.z.15, ptrtoint (ptr @U_00401b50 to i64)  ; 0x401367
  %is.at.10 = icmp eq i32 %m.14, 4201296  ; 0x401367
  %is.10 = or i1 %is.fn.10, %is.at.10  ; 0x401367
  br i1 %is.10, label %vcall_107, label %vcall.test_111
vcall_107:  ; 0x401367
  %ret.17 = call i64 @Problem_vf1()  ; 0x401367
  br label %f5_401350.103
vcall.test_111:  ; 0x401367
  %is.fn.11 = icmp eq i64 %eax.z.15, ptrtoint (ptr @U_00401b60 to i64)  ; 0x401367
  %is.at.11 = icmp eq i32 %m.14, 4201312  ; 0x401367
  %is.11 = or i1 %is.fn.11, %is.at.11  ; 0x401367
  br i1 %is.11, label %vcall_108, label %vcall.trap_104
vcall_108:  ; 0x401367
  %ret.18 = call i64 @std_exception_vf1(i64 %m.12.s.w)  ; 0x401367
  br label %f5_401350.103
vcall.trap_104:  ; 0x401367
  call void @ra_trap(i64 4199271, i64 10)  ; 0x401367
  unreachable
f5_401350.103:  ; 0x401350
  %rax.phi.5 = phi i64 [ %ret.15, %vcall_105 ], [ %ret.16, %vcall_106 ], [ %ret.17, %vcall_107 ], [ %ret.18, %vcall_108 ]  ; 0x401367
  %rsp.95 = add i32 %sp.32, 4  ; 0x401367
  %eh.pending.3 = call i64 @ra_eh_pending()  ; 0x401367
  %eh.thrown.3 = icmp ne i64 %eh.pending.3, 0  ; 0x401367
  br i1 %eh.thrown.3, label %eh.landing.1, label %eh.ok.401367
vcall_114:  ; 0x40132d
  %ret.20 = call i64 @Derived_vf1()  ; 0x40132d
  br label %f4_401310.112
vcall.test_118:  ; 0x40132d
  %is.fn.13 = icmp eq i64 %eax.z.10, ptrtoint (ptr @U_00401b20 to i64)  ; 0x40132d
  %is.at.13 = icmp eq i32 %m.11, 4201248  ; 0x40132d
  %is.13 = or i1 %is.fn.13, %is.at.13  ; 0x40132d
  br i1 %is.13, label %vcall_115, label %vcall.test_119
vcall_115:  ; 0x40132d
  %ret.21 = call i64 @Base_vf1()  ; 0x40132d
  br label %f4_401310.112
vcall.test_119:  ; 0x40132d
  %is.fn.14 = icmp eq i64 %eax.z.10, ptrtoint (ptr @U_00401b50 to i64)  ; 0x40132d
  %is.at.14 = icmp eq i32 %m.11, 4201296  ; 0x40132d
  %is.14 = or i1 %is.fn.14, %is.at.14  ; 0x40132d
  br i1 %is.14, label %vcall_116, label %vcall.test_120
vcall_116:  ; 0x40132d
  %ret.22 = call i64 @Problem_vf1()  ; 0x40132d
  br label %f4_401310.112
vcall.test_120:  ; 0x40132d
  %is.fn.15 = icmp eq i64 %eax.z.10, ptrtoint (ptr @U_00401b60 to i64)  ; 0x40132d
  %is.at.15 = icmp eq i32 %m.11, 4201312  ; 0x40132d
  %is.15 = or i1 %is.fn.15, %is.at.15  ; 0x40132d
  br i1 %is.15, label %vcall_117, label %vcall.trap_113
vcall_117:  ; 0x40132d
  %ret.23 = call i64 @std_exception_vf1(i64 %m.8.s.w)  ; 0x40132d
  br label %f4_401310.112
vcall.trap_113:  ; 0x40132d
  call void @ra_trap(i64 4199213, i64 10)  ; 0x40132d
  unreachable
f4_401310.112:  ; 0x401310
  %rax.phi.6 = phi i64 [ %ret.20, %vcall_114 ], [ %ret.21, %vcall_115 ], [ %ret.22, %vcall_116 ], [ %ret.23, %vcall_117 ]  ; 0x40132d
  %rsp.97 = add i32 %sp.25, 4  ; 0x40132d
  %eh.pending.2 = call i64 @ra_eh_pending()  ; 0x40132d
  %eh.thrown.2 = icmp ne i64 %eh.pending.2, 0  ; 0x40132d
  br i1 %eh.thrown.2, label %eh.landing.1, label %eh.ok.40132d
}

define i64 @F_004013a0(i32 %arg0) {  ; 0x4013a0
b_4013a0:  ; 0x4013a0
  %frame = alloca [4268 x i8], align 16
  %frame.top = getelementptr inbounds i8, ptr %frame, i64 4188
  %rsp0 = ptrtoint ptr %frame.top to i64
  %stack.arg0.p = getelementptr inbounds i8, ptr %frame.top, i64 4
  store i32 %arg0, ptr %stack.arg0.p, align 4
  %ebp = trunc i64 undef to i32  ; 0x4013a0
  %esp = trunc i64 %rsp0 to i32  ; 0x4013a0
  %sp = sub i32 %esp, 4  ; 0x4013a0
  %st.p = getelementptr inbounds i8, ptr %frame.top, i64 -4  ; 0x4013a0
  store i32 %ebp, ptr %st.p, align 1  ; 0x4013a0
  %sp.1 = sub i32 %sp, 4  ; 0x4013a3
  %st.p.1 = getelementptr inbounds i8, ptr %frame.top, i64 -8  ; 0x4013a3
  store i32 %ebp, ptr %st.p.1, align 1  ; 0x4013a3
  %sp.2 = sub i32 %sp.1, 4  ; 0x4013a4
  %st.p.2 = getelementptr inbounds i8, ptr %frame.top, i64 -12  ; 0x4013a4
  store i32 %ebp, ptr %st.p.2, align 1  ; 0x4013a4
  %sp.3 = sub i32 %sp.2, 4  ; 0x4013a5
  %st.p.3 = getelementptr inbounds i8, ptr %frame.top, i64 -16  ; 0x4013a5
  store i32 %ebp, ptr %st.p.3, align 1  ; 0x4013a5
  %diff = sub i32 %sp.3, 32  ; 0x4013a6
  %st.p.4 = getelementptr inbounds i8, ptr %frame.top, i64 -32  ; 0x4013ac
  store i32 %diff, ptr %st.p.4, align 1  ; 0x4013ac
  %st.p.5 = getelementptr inbounds i8, ptr %frame.top, i64 -20  ; 0x4013af
  store i32 -1, ptr %st.p.5, align 1  ; 0x4013af
  %addr.3 = add i32 %sp, -24  ; 0x4013b6
  %st.p.6 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x4013b9
  store i32 ptrtoint (ptr @U_00401bf0 to i32), ptr %st.p.6, align 1  ; 0x4013b9
  %teb = call i64 @ra_teb()  ; 0x4013c0
  %teb.1 = trunc i64 %teb to i32  ; 0x4013c0
  %addr.z.8 = zext i32 %teb.1 to i64  ; 0x4013c0
  %m.p.1 = inttoptr i64 %addr.z.8 to ptr  ; 0x4013c0
  %m.1 = load i32, ptr %m.p.1, align 1  ; 0x4013c0
  %st.p.7 = getelementptr inbounds i8, ptr %frame.top, i64 -28  ; 0x4013c7
  store i32 %m.1, ptr %st.p.7, align 1  ; 0x4013c7
  %teb.2 = call i64 @ra_teb()  ; 0x4013ca
  %teb.3 = trunc i64 %teb.2 to i32  ; 0x4013ca
  %addr.z.10 = zext i32 %teb.3 to i64  ; 0x4013ca
  %st.p.8 = inttoptr i64 %addr.z.10 to ptr  ; 0x4013ca
  store i32 %addr.3, ptr %st.p.8, align 1  ; 0x4013ca
  %addr.6 = add i32 %arg0, 110  ; 0x4013d0
  %sp.4 = sub i32 %diff, 4  ; 0x4013d3
  %st.p.9 = getelementptr inbounds i8, ptr %frame.top, i64 -52  ; 0x4013d3
  store i32 %addr.6, ptr %st.p.9, align 1  ; 0x4013d3
  %sp.5 = sub i32 %sp.4, 4  ; 0x4013d4
  %sarg0.a = add i32 %sp.5, 4
  %ret = call i64 @F_00401000(i32 %addr.6)  ; 0x4013d4
  %eh.pending = call i64 @ra_eh_pending()  ; 0x4013d4
  %eh.thrown = icmp ne i64 %eh.pending, 0  ; 0x4013d4
  br i1 %eh.thrown, label %eh.landing, label %eh.ok.4013d4
b_4013e0:  ; 0x4013e0
  %addr.9 = add i32 %arg0, -1  ; 0x4013e0
  store i32 0, ptr %st.p.5, align 1  ; 0x4013e3
  %sp.18 = sub i32 %sum.6, 4  ; 0x4013ea
  store i32 %addr.9, ptr %st.p.9, align 1  ; 0x4013ea
  %sp.19 = sub i32 %sp.18, 4  ; 0x4013eb
  %sarg0.a.1 = add i32 %sp.19, 4
  %ret.1 = call i64 @F_004013a0(i32 %addr.9)  ; 0x4013eb
  %eh.pending.3 = call i64 @ra_eh_pending()  ; 0x4013eb
  %eh.thrown.3 = icmp ne i64 %eh.pending.3, 0  ; 0x4013eb
  br i1 %eh.thrown.3, label %eh.landing, label %eh.ok.4013eb
b_40141b:  ; 0x40141b
  %st.p.35 = getelementptr inbounds i8, ptr %frame.top, i64 -40  ; 0x40141b
  store i32 0, ptr %st.p.35, align 1  ; 0x40141b
  %st.p.36 = getelementptr inbounds i8, ptr %frame.top, i64 -44  ; 0x401422
  store i32 0, ptr %st.p.36, align 1  ; 0x401422
  %addr.15 = add i32 %sp, -44  ; 0x401429
  %addr.z.62 = zext i32 %addr.15 to i64  ; 0x401429
  %st.p.37 = getelementptr inbounds i8, ptr %frame.top, i64 -48  ; 0x401429
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 184) to i32), ptr %st.p.37, align 1  ; 0x401429
  %st.p.38 = getelementptr inbounds i8, ptr %frame.top, i64 -36  ; 0x401430
  store i32 42, ptr %st.p.38, align 1  ; 0x401430
  store i32 0, ptr %st.p.5, align 1  ; 0x401437
  %sp.27 = sub i32 %sum.6, 4  ; 0x401441
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 2792) to i32), ptr %st.p.9, align 1  ; 0x401441
  %sp.28 = sub i32 %sp.27, 4  ; 0x401446
  %st.p.41 = getelementptr inbounds i8, ptr %frame.top, i64 -56  ; 0x401446
  store i32 %addr.15, ptr %st.p.41, align 1  ; 0x401446
  call void @ra_eh_throw(i64 %addr.z.62, i64 16)  ; 0x401447
  call void @ra_eh_throw_destructor(i64 4198976)  ; 0x401447
  call void @ra_eh_throw_type(i64 7474415530116504195, i64 0, i64 16)  ; 0x401447
  call void @ra_eh_throw_copy(i64 4198848)  ; 0x401447
  call void @ra_eh_throw_type(i64 -5932227318556162533, i64 0, i64 12)  ; 0x401447
  call void @ra_eh_throw_copy(i64 4198912)  ; 0x401447
  br label %eh.landing
eh.landing:  ; 0x4013d4
  %rax.phi = phi i64 [ %ret, %b_4013a0 ], [ %ret.1, %b_4013e0 ], [ %addr.z.62, %b_40141b ], [ %ret.2, %eh.ok.4013eb ]  ; 0x4013d4
  %rsp.phi = phi i32 [ %sarg0.a, %b_4013a0 ], [ %sarg0.a.1, %b_4013e0 ], [ %sp.28, %b_40141b ], [ %sarg0.a.2, %eh.ok.4013eb ]  ; 0x4013d4
  %teb.4 = call i64 @ra_teb()  ; 0x4013d4
  %teb.5 = trunc i64 %teb.4 to i32  ; 0x4013d4
  %addr.z.13 = zext i32 %teb.5 to i64  ; 0x4013d4
  %eh.head.p = inttoptr i64 %addr.z.13 to ptr  ; 0x4013d4
  %eh.head = load i32, ptr %eh.head.p, align 1  ; 0x4013d4
  %eh.head.s.w = zext i32 %eh.head to i64  ; 0x4013d4
  %eh.head.s = inttoptr i64 %eh.head.s.w to ptr  ; 0x4013d4
  %eh.frame = sub i32 %eh.head, -24  ; 0x4013d4
  %eh.reg = add i32 %eh.frame, -24  ; 0x4013d4
  %eh.head.f4 = getelementptr inbounds %S1, ptr %eh.head.s, i64 0, i32 1  ; 0x4013d4
  %eh.handler = load i32, ptr %eh.head.f4, align 1  ; 0x4013d4
  %eh.ours = icmp eq i32 %eh.handler, 4201456  ; 0x4013d4
  %eh.ours.1 = icmp eq i32 %eh.handler, ptrtoint (ptr @U_00401bf0 to i32)  ; 0x4013d4
  %eh.ours.2 = or i1 %eh.ours, %eh.ours.1  ; 0x4013d4
  br i1 %eh.ours.2, label %eh.dispatch, label %eh.foreign
eh.dispatch:  ; 0x4013d4
  %eh.head.f8 = getelementptr inbounds %S1, ptr %eh.head.s, i64 0, i32 2  ; 0x4013d4
  %eh.state = load i32, ptr %eh.head.f8, align 1  ; 0x4013d4
  switch i32 %eh.state, label %eh.state.bad [ i32 -1, label %eh.state_none i32 0, label %eh.state0 ]
eh.foreign:  ; 0x4013d4
  call void @ra_trap(i64 4199328, i64 8)  ; 0x4013d4
  unreachable
eh.state.bad:  ; 0x4013d4
  call void @ra_trap(i64 4199328, i64 7)  ; 0x4013d4
  unreachable
eh.state_none:  ; 0x4013d4
  call void @ra_eh_suspend()  ; 0x4013d4
  %eh.head.f8.1 = getelementptr inbounds %S1, ptr %eh.head.s, i64 0, i32 2  ; 0x4013d4
  store i32 -1, ptr %eh.head.f8.1, align 1  ; 0x4013d4
  call void @ra_eh_resume()  ; 0x4013d4
  %eh.head.f0 = getelementptr inbounds %S1, ptr %eh.head.s, i64 0, i32 0  ; 0x4013d4
  %eh.next = load i32, ptr %eh.head.f0, align 1  ; 0x4013d4
  %teb.6 = call i64 @ra_teb()  ; 0x4013d4
  %teb.7 = trunc i64 %teb.6 to i32  ; 0x4013d4
  %addr.z.18 = zext i32 %teb.7 to i64  ; 0x4013d4
  %st.p.12 = inttoptr i64 %addr.z.18 to ptr  ; 0x4013d4
  store i32 %eh.next, ptr %st.p.12, align 1  ; 0x4013d4
  ret i64 %rax.phi
eh.state0:  ; 0x4013d4
  call void @ra_eh_suspend()  ; 0x4013d4
  %eh.head.f8.2 = getelementptr inbounds %S1, ptr %eh.head.s, i64 0, i32 2  ; 0x4013d4
  store i32 -1, ptr %eh.head.f8.2, align 1  ; 0x4013d4
  %eh.frame.1 = add i32 %eh.reg, 12  ; 0x4013d4
  %sp.6 = sub i32 %rsp.phi, 4  ; 0x4013d4
  %addr.z.20 = zext i32 %sp.6 to i64  ; 0x4013d4
  %st.p.14 = inttoptr i64 %addr.z.20 to ptr  ; 0x4013d4
  store i32 0, ptr %st.p.14, align 1  ; 0x4013d4
  br label %f1_401450
eh.cleaned.401450:  ; 0x4013d4
  %eh.head.f8.3 = getelementptr inbounds %S1, ptr %eh.head.s, i64 0, i32 2  ; 0x401469
  store i32 -1, ptr %eh.head.f8.3, align 1  ; 0x401469
  call void @ra_eh_resume()  ; 0x401469
  %eh.head.f0.1 = getelementptr inbounds %S1, ptr %eh.head.s, i64 0, i32 0  ; 0x401469
  %eh.next.3 = load i32, ptr %eh.head.f0.1, align 1  ; 0x401469
  %teb.14 = call i64 @ra_teb()  ; 0x401469
  %teb.15 = trunc i64 %teb.14 to i32  ; 0x401469
  %addr.z.46 = zext i32 %teb.15 to i64  ; 0x401469
  %st.p.28 = inttoptr i64 %addr.z.46 to ptr  ; 0x401469
  store i32 %eh.next.3, ptr %st.p.28, align 1  ; 0x401469
  ret i64 %ret.3
f1_401450:  ; 0x401450
  %sp.7 = sub i32 %sp.6, 4  ; 0x401450
  %addr.z.21 = zext i32 %sp.7 to i64  ; 0x401450
  %st.p.15 = inttoptr i64 %addr.z.21 to ptr  ; 0x401450
  store i32 %eh.frame.1, ptr %st.p.15, align 1  ; 0x401450
  %diff.1 = sub i32 %sp.7, 8  ; 0x401451
  %eh.head.f32 = getelementptr inbounds %S1, ptr %eh.head.s, i64 0, i32 4  ; 0x401457
  %m.2 = load i32, ptr %eh.head.f32, align 1  ; 0x401457
  %sum.1 = add i32 %m.2, 210  ; 0x40145a
  %sp.8 = sub i32 %diff.1, 4  ; 0x40145f
  %addr.z.23 = zext i32 %sp.8 to i64  ; 0x40145f
  %st.p.16 = inttoptr i64 %addr.z.23 to ptr  ; 0x40145f
  store i32 %sum.1, ptr %st.p.16, align 1  ; 0x40145f
  %sp.9 = sub i32 %sp.8, 4  ; 0x401460
  %sarg0.a.3 = add i32 %sp.9, 4
  %ret.3 = call i64 @F_00401000(i32 %sum.1)  ; 0x401460
  %eh.pending.1 = call i64 @ra_eh_pending()  ; 0x401460
  %eh.thrown.1 = icmp ne i64 %eh.pending.1, 0  ; 0x401460
  br i1 %eh.thrown.1, label %eh.landing.1, label %eh.ok.401460.1
eh.landing.1:  ; 0x401460
  %rax.phi.1 = phi i64 [ %ret.3, %f1_401450 ], [ %ret.4, %f2_401450 ]  ; 0x401460
  %rsp.phi.1 = phi i32 [ %sarg0.a.3, %f1_401450 ], [ %sarg0.a.4, %f2_401450 ]  ; 0x401460
  %teb.8 = call i64 @ra_teb()  ; 0x401460
  %teb.9 = trunc i64 %teb.8 to i32  ; 0x401460
  %addr.z.25 = zext i32 %teb.9 to i64  ; 0x401460
  %eh.head.p.1 = inttoptr i64 %addr.z.25 to ptr  ; 0x401460
  %eh.head.1 = load i32, ptr %eh.head.p.1, align 1  ; 0x401460
  %eh.head.1.s.w = zext i32 %eh.head.1 to i64  ; 0x401460
  %eh.head.1.s = inttoptr i64 %eh.head.1.s.w to ptr  ; 0x401460
  %eh.frame.2 = sub i32 %eh.head.1, -24  ; 0x401460
  %eh.reg.1 = add i32 %eh.frame.2, -24  ; 0x401460
  %eh.head.1.f4 = getelementptr inbounds %S1, ptr %eh.head.1.s, i64 0, i32 1  ; 0x401460
  %eh.handler.1 = load i32, ptr %eh.head.1.f4, align 1  ; 0x401460
  %eh.ours.3 = icmp eq i32 %eh.handler.1, 4201456  ; 0x401460
  %eh.ours.4 = icmp eq i32 %eh.handler.1, ptrtoint (ptr @U_00401bf0 to i32)  ; 0x401460
  %eh.ours.5 = or i1 %eh.ours.3, %eh.ours.4  ; 0x401460
  br i1 %eh.ours.5, label %eh.dispatch.1, label %eh.foreign.1
eh.dispatch.1:  ; 0x401460
  %eh.head.1.f8 = getelementptr inbounds %S1, ptr %eh.head.1.s, i64 0, i32 2  ; 0x401460
  %eh.state.1 = load i32, ptr %eh.head.1.f8, align 1  ; 0x401460
  switch i32 %eh.state.1, label %eh.state.bad.1 [ i32 -1, label %eh.state_none.1 i32 0, label %eh.state0.1 ]
eh.foreign.1:  ; 0x401460
  call void @ra_trap(i64 4199328, i64 8)  ; 0x401460
  unreachable
eh.state.bad.1:  ; 0x401460
  call void @ra_trap(i64 4199328, i64 7)  ; 0x401460
  unreachable
eh.state_none.1:  ; 0x401460
  call void @ra_eh_suspend()  ; 0x401460
  %eh.head.1.f8.1 = getelementptr inbounds %S1, ptr %eh.head.1.s, i64 0, i32 2  ; 0x401460
  store i32 -1, ptr %eh.head.1.f8.1, align 1  ; 0x401460
  call void @ra_eh_resume()  ; 0x401460
  %eh.head.1.f0 = getelementptr inbounds %S1, ptr %eh.head.1.s, i64 0, i32 0  ; 0x401460
  %eh.next.1 = load i32, ptr %eh.head.1.f0, align 1  ; 0x401460
  %teb.10 = call i64 @ra_teb()  ; 0x401460
  %teb.11 = trunc i64 %teb.10 to i32  ; 0x401460
  %addr.z.30 = zext i32 %teb.11 to i64  ; 0x401460
  %st.p.19 = inttoptr i64 %addr.z.30 to ptr  ; 0x401460
  store i32 %eh.next.1, ptr %st.p.19, align 1  ; 0x401460
  ret i64 %rax.phi.1
eh.state0.1:  ; 0x401460
  call void @ra_eh_suspend()  ; 0x401460
  %eh.head.1.f8.2 = getelementptr inbounds %S1, ptr %eh.head.1.s, i64 0, i32 2  ; 0x401460
  store i32 -1, ptr %eh.head.1.f8.2, align 1  ; 0x401460
  %eh.frame.3 = add i32 %eh.reg.1, 12  ; 0x401460
  %sp.10 = sub i32 %rsp.phi.1, 4  ; 0x401460
  %addr.z.32 = zext i32 %sp.10 to i64  ; 0x401460
  %st.p.21 = inttoptr i64 %addr.z.32 to ptr  ; 0x401460
  store i32 0, ptr %st.p.21, align 1  ; 0x401460
  br label %f2_401450
eh.cleaned.401450.1:  ; 0x401460
  %eh.head.1.f8.3 = getelementptr inbounds %S1, ptr %eh.head.1.s, i64 0, i32 2  ; 0x401469
  store i32 -1, ptr %eh.head.1.f8.3, align 1  ; 0x401469
  call void @ra_eh_resume()  ; 0x401469
  %eh.head.1.f0.1 = getelementptr inbounds %S1, ptr %eh.head.1.s, i64 0, i32 0  ; 0x401469
  %eh.next.2 = load i32, ptr %eh.head.1.f0.1, align 1  ; 0x401469
  %teb.12 = call i64 @ra_teb()  ; 0x401469
  %teb.13 = trunc i64 %teb.12 to i32  ; 0x401469
  %addr.z.41 = zext i32 %teb.13 to i64  ; 0x401469
  %st.p.26 = inttoptr i64 %addr.z.41 to ptr  ; 0x401469
  store i32 %eh.next.2, ptr %st.p.26, align 1  ; 0x401469
  ret i64 %ret.4
f2_401450:  ; 0x401450
  %sp.11 = sub i32 %sp.10, 4  ; 0x401450
  %addr.z.33 = zext i32 %sp.11 to i64  ; 0x401450
  %st.p.22 = inttoptr i64 %addr.z.33 to ptr  ; 0x401450
  store i32 %eh.frame.3, ptr %st.p.22, align 1  ; 0x401450
  %diff.2 = sub i32 %sp.11, 8  ; 0x401451
  %eh.head.1.f32 = getelementptr inbounds %S1, ptr %eh.head.1.s, i64 0, i32 4  ; 0x401457
  %m.3 = load i32, ptr %eh.head.1.f32, align 1  ; 0x401457
  %sum.3 = add i32 %m.3, 210  ; 0x40145a
  %sp.12 = sub i32 %diff.2, 4  ; 0x40145f
  %addr.z.35 = zext i32 %sp.12 to i64  ; 0x40145f
  %st.p.23 = inttoptr i64 %addr.z.35 to ptr  ; 0x40145f
  store i32 %sum.3, ptr %st.p.23, align 1  ; 0x40145f
  %sp.13 = sub i32 %sp.12, 4  ; 0x401460
  %sarg0.a.4 = add i32 %sp.13, 4
  %ret.4 = call i64 @F_00401000(i32 %sum.3)  ; 0x401460
  %eh.pending.2 = call i64 @ra_eh_pending()  ; 0x401460
  %eh.thrown.2 = icmp ne i64 %eh.pending.2, 0  ; 0x401460
  br i1 %eh.thrown.2, label %eh.landing.1, label %eh.ok.401460
eh.ok.401460:  ; 0x401460
  br label %eh.cleaned.401450.1
eh.ok.401460.1:  ; 0x401469
  br label %eh.cleaned.401450
eh.ok.4013d4:  ; 0x401469
  %sum.6 = add i32 %sarg0.a, 4  ; 0x4013d9
  %zf.10 = icmp eq i32 %arg0, 0  ; 0x4013dc
  br i1 %zf.10, label %b_40141b, label %b_4013e0
eh.ok.4013eb:  ; 0x4013eb
  %sum.7 = add i32 %sarg0.a.1, 4  ; 0x4013f0
  %eax.7 = trunc i64 %ret.1 to i32  ; 0x4013f3
  %sum.8 = add i32 %eax.7, 1  ; 0x4013f3
  %m.4 = load i32, ptr %stack.arg0.p, align 1  ; 0x4013f4
  %sum.9 = add i32 %m.4, 210  ; 0x4013f7
  %sp.20 = sub i32 %sum.7, 4  ; 0x4013fd
  store i32 %sum.9, ptr %st.p.9, align 1  ; 0x4013fd
  %sp.21 = sub i32 %sp.20, 4  ; 0x401400
  %sarg0.a.2 = add i32 %sp.21, 4
  %ret.2 = call i64 @F_00401000(i32 %sum.9)  ; 0x401400
  %eh.pending.4 = call i64 @ra_eh_pending()  ; 0x401400
  %eh.thrown.4 = icmp ne i64 %eh.pending.4, 0  ; 0x401400
  br i1 %eh.thrown.4, label %eh.landing, label %eh.ok.401400
eh.ok.401400:  ; 0x401400
  %m.5 = load i32, ptr %st.p.7, align 1  ; 0x401408
  %teb.16 = call i64 @ra_teb()  ; 0x40140b
  %teb.17 = trunc i64 %teb.16 to i32  ; 0x40140b
  %addr.z.54 = zext i32 %teb.17 to i64  ; 0x40140b
  %st.p.34 = inttoptr i64 %addr.z.54 to ptr  ; 0x40140b
  store i32 %m.5, ptr %st.p.34, align 1  ; 0x40140b
  %eax.z.9 = zext i32 %sum.8 to i64  ; 0x401411
  ret i64 %eax.z.9
}

define i64 @F_00401470(i32 %arg0) {  ; 0x401470
b_401470:  ; 0x401470
  %frame = alloca [4316 x i8], align 16
  %frame.top = getelementptr inbounds i8, ptr %frame, i64 4236
  %rsp0 = ptrtoint ptr %frame.top to i64
  %stack.arg0.p = getelementptr inbounds i8, ptr %frame.top, i64 4
  store i32 %arg0, ptr %stack.arg0.p, align 4
  %ebp = trunc i64 undef to i32  ; 0x401470
  %esp = trunc i64 %rsp0 to i32  ; 0x401470
  %sp = sub i32 %esp, 4  ; 0x401470
  %st.p = getelementptr inbounds i8, ptr %frame.top, i64 -4  ; 0x401470
  store i32 %ebp, ptr %st.p, align 1  ; 0x401470
  %sp.1 = sub i32 %sp, 4  ; 0x401473
  %st.p.1 = getelementptr inbounds i8, ptr %frame.top, i64 -8  ; 0x401473
  store i32 %ebp, ptr %st.p.1, align 1  ; 0x401473
  %sp.2 = sub i32 %sp.1, 4  ; 0x401474
  %st.p.2 = getelementptr inbounds i8, ptr %frame.top, i64 -12  ; 0x401474
  store i32 %ebp, ptr %st.p.2, align 1  ; 0x401474
  %sp.3 = sub i32 %sp.2, 4  ; 0x401475
  %st.p.3 = getelementptr inbounds i8, ptr %frame.top, i64 -16  ; 0x401475
  store i32 %ebp, ptr %st.p.3, align 1  ; 0x401475
  %diff = sub i32 %sp.3, 32  ; 0x401476
  %st.p.4 = getelementptr inbounds i8, ptr %frame.top, i64 -32  ; 0x40147c
  store i32 %diff, ptr %st.p.4, align 1  ; 0x40147c
  %st.p.5 = getelementptr inbounds i8, ptr %frame.top, i64 -20  ; 0x40147f
  store i32 -1, ptr %st.p.5, align 1  ; 0x40147f
  %addr.3 = add i32 %sp, -24  ; 0x401486
  %st.p.6 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x401489
  store i32 ptrtoint (ptr @U_00401c00 to i32), ptr %st.p.6, align 1  ; 0x401489
  %teb = call i64 @ra_teb()  ; 0x401490
  %teb.1 = trunc i64 %teb to i32  ; 0x401490
  %addr.z.8 = zext i32 %teb.1 to i64  ; 0x401490
  %m.p.1 = inttoptr i64 %addr.z.8 to ptr  ; 0x401490
  %m.1 = load i32, ptr %m.p.1, align 1  ; 0x401490
  %st.p.7 = getelementptr inbounds i8, ptr %frame.top, i64 -28  ; 0x401497
  store i32 %m.1, ptr %st.p.7, align 1  ; 0x401497
  %teb.2 = call i64 @ra_teb()  ; 0x40149a
  %teb.3 = trunc i64 %teb.2 to i32  ; 0x40149a
  %addr.z.10 = zext i32 %teb.3 to i64  ; 0x40149a
  %st.p.8 = inttoptr i64 %addr.z.10 to ptr  ; 0x40149a
  store i32 %addr.3, ptr %st.p.8, align 1  ; 0x40149a
  %logic = and i32 %arg0, 3  ; 0x4014a1
  %st.p.9 = getelementptr inbounds i8, ptr %frame.top, i64 -40  ; 0x4014a4
  store i32 0, ptr %st.p.9, align 1  ; 0x4014a4
  store i32 1, ptr %st.p.5, align 1  ; 0x4014ab
  %st.p.11 = getelementptr inbounds i8, ptr %frame.top, i64 -36  ; 0x4014b2
  store i32 0, ptr %st.p.11, align 1  ; 0x4014b2
  %sp.4 = sub i32 %diff, 4  ; 0x4014b9
  %st.p.12 = getelementptr inbounds i8, ptr %frame.top, i64 -52  ; 0x4014b9
  store i32 %logic, ptr %st.p.12, align 1  ; 0x4014b9
  %sp.5 = sub i32 %sp.4, 4  ; 0x4014ba
  %sarg0.a = add i32 %sp.5, 4
  %ret = call i64 @F_004013a0(i32 %logic)  ; 0x4014ba
  %eh.pending = call i64 @ra_eh_pending()  ; 0x4014ba
  %eh.thrown = icmp ne i64 %eh.pending, 0  ; 0x4014ba
  br i1 %eh.thrown, label %eh.landing, label %eh.ok.4014ba
b_4014c2:  ; 0x4014c2
  %rbp.phi.1 = phi i32 [ %sum.10, %b_4014d6 ], [ %sp, %eh.ok.4014ba ]  ; 0x4014c2
  %addr.33 = add i32 %rbp.phi.1, -24  ; 0x4014c2
  %addr.z.122 = zext i32 %addr.33 to i64  ; 0x4014c2
  %m.p.12 = inttoptr i64 %addr.z.122 to ptr  ; 0x4014c2
  %m.12 = load i32, ptr %m.p.12, align 1  ; 0x4014c2
  %teb.28 = call i64 @ra_teb()  ; 0x4014c5
  %teb.29 = trunc i64 %teb.28 to i32  ; 0x4014c5
  %addr.z.123 = zext i32 %teb.29 to i64  ; 0x4014c5
  %st.p.86 = inttoptr i64 %addr.z.123 to ptr  ; 0x4014c5
  store i32 %m.12, ptr %st.p.86, align 1  ; 0x4014c5
  %addr.34 = add i32 %rbp.phi.1, -32  ; 0x4014cb
  %addr.z.124 = zext i32 %addr.34 to i64  ; 0x4014cb
  %m.p.13 = inttoptr i64 %addr.z.124 to ptr  ; 0x4014cb
  %m.13 = load i32, ptr %m.p.13, align 1  ; 0x4014cb
  %eax.z.9 = zext i32 %m.13 to i64  ; 0x4014cb
  ret i64 %eax.z.9
b_4014d6:  ; 0x4014d6
  %rbp.phi = phi i32 [ %eh.frame.2, %eh.destroyed.401500 ], [ %eh.frame.7, %eh.destroyed.401500.1 ]  ; 0x4014d6
  %sum.10 = add i32 %rbp.phi, 12  ; 0x4014d6
  br label %b_4014c2
eh.landing:  ; 0x4014ba
  %teb.4 = call i64 @ra_teb()  ; 0x4014ba
  %teb.5 = trunc i64 %teb.4 to i32  ; 0x4014ba
  %addr.z.16 = zext i32 %teb.5 to i64  ; 0x4014ba
  %eh.head.p = inttoptr i64 %addr.z.16 to ptr  ; 0x4014ba
  %eh.head = load i32, ptr %eh.head.p, align 1  ; 0x4014ba
  %eh.head.s.w = zext i32 %eh.head to i64  ; 0x4014ba
  %eh.head.s = inttoptr i64 %eh.head.s.w to ptr  ; 0x4014ba
  %eh.frame = sub i32 %eh.head, -24  ; 0x4014ba
  %eh.reg = add i32 %eh.frame, -24  ; 0x4014ba
  %eh.head.f4 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 1  ; 0x4014ba
  %eh.handler = load i32, ptr %eh.head.f4, align 1  ; 0x4014ba
  %eh.ours = icmp eq i32 %eh.handler, 4201472  ; 0x4014ba
  %eh.ours.1 = icmp eq i32 %eh.handler, ptrtoint (ptr @U_00401c00 to i32)  ; 0x4014ba
  %eh.ours.2 = or i1 %eh.ours, %eh.ours.1  ; 0x4014ba
  br i1 %eh.ours.2, label %eh.dispatch, label %eh.foreign
eh.dispatch:  ; 0x4014ba
  %eh.head.f8 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x4014ba
  %eh.state = load i32, ptr %eh.head.f8, align 1  ; 0x4014ba
  switch i32 %eh.state, label %eh.state.bad [ i32 -1, label %eh.state_none i32 0, label %eh.state0 i32 1, label %eh.state1 i32 2, label %eh.state2.1 i32 3, label %eh.state3.1 ]
eh.foreign:  ; 0x4014ba
  call void @ra_trap(i64 4199536, i64 8)  ; 0x4014ba
  unreachable
eh.state.bad:  ; 0x4014ba
  call void @ra_trap(i64 4199536, i64 7)  ; 0x4014ba
  unreachable
eh.state_none:  ; 0x4014ba
  call void @ra_eh_suspend()  ; 0x4014ba
  %eh.head.f8.1 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x4014ba
  store i32 -1, ptr %eh.head.f8.1, align 1  ; 0x4014ba
  call void @ra_eh_resume()  ; 0x4014ba
  %eh.head.f0 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 0  ; 0x4014ba
  %eh.next = load i32, ptr %eh.head.f0, align 1  ; 0x4014ba
  %teb.6 = call i64 @ra_teb()  ; 0x4014ba
  %teb.7 = trunc i64 %teb.6 to i32  ; 0x4014ba
  %addr.z.21 = zext i32 %teb.7 to i64  ; 0x4014ba
  %st.p.15 = inttoptr i64 %addr.z.21 to ptr  ; 0x4014ba
  store i32 %eh.next, ptr %st.p.15, align 1  ; 0x4014ba
  ret i64 %ret
eh.state0:  ; 0x4014ba
  %eh.frame.1 = add i32 %eh.reg, 12  ; 0x4014ba
  %eh.catch.obj = add i32 %eh.frame.1, -28  ; 0x4014ba
  %eh.catch.obj.1 = zext i32 %eh.catch.obj to i64  ; 0x4014ba
  %eh.caught = call i64 @ra_eh_catches(i64 7474415530116504195, i64 %eh.catch.obj.1, i64 8200)  ; 0x4014ba
  %eh.hit = icmp ne i64 %eh.caught, 0  ; 0x4014ba
  br i1 %eh.hit, label %eh.catch1_0.s0, label %eh.next1_0.s0
eh.catch1_0.s0:  ; 0x4014ba
  %eh.head.f8.2 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x4014ba
  store i32 0, ptr %eh.head.f8.2, align 1  ; 0x4014ba
  br label %eh.enter1_0
eh.next1_0.s0:  ; 0x4014ba
  call void @ra_eh_suspend()  ; 0x401500
  %eh.head.f8.3 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x401500
  store i32 -1, ptr %eh.head.f8.3, align 1  ; 0x401500
  call void @ra_eh_resume()  ; 0x401500
  %eh.head.f0.1 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 0  ; 0x401500
  %eh.next.1 = load i32, ptr %eh.head.f0.1, align 1  ; 0x401500
  %teb.8 = call i64 @ra_teb()  ; 0x401500
  %teb.9 = trunc i64 %teb.8 to i32  ; 0x401500
  %addr.z.38 = zext i32 %teb.9 to i64  ; 0x401500
  %st.p.25 = inttoptr i64 %addr.z.38 to ptr  ; 0x401500
  store i32 %eh.next.1, ptr %st.p.25, align 1  ; 0x401500
  ret i64 %ret
eh.enter1_0:  ; 0x4014ba
  %rsp.phi.4 = phi i32 [ %sarg0.a, %eh.catch1_0.s0 ], [ %sarg0.a, %eh.catch1_0.s1.1 ], [ %rsp.phi, %eh.destroyed.401470.3 ]  ; 0x4014ba
  call void @ra_eh_enter_catch()  ; 0x4014ba
  %eh.head.f8.4 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x4014ba
  store i32 3, ptr %eh.head.f8.4, align 1  ; 0x4014ba
  %eh.esp.p = sub i32 %eh.reg, 4  ; 0x4014ba
  %addr.z.24 = zext i32 %eh.esp.p to i64  ; 0x4014ba
  %eh.esp.p.1 = inttoptr i64 %addr.z.24 to ptr  ; 0x4014ba
  %eh.esp = load i32, ptr %eh.esp.p.1, align 1  ; 0x4014ba
  %eh.esp.1 = zext i32 %eh.esp to i64  ; 0x4014ba
  call void @ra_eh_catch_sp(i64 %eh.esp.1)  ; 0x4014ba
  %eh.frame.2 = add i32 %eh.reg, 12  ; 0x4014ba
  %sp.6 = sub i32 %rsp.phi.4, 4  ; 0x4014ba
  %addr.z.25 = zext i32 %sp.6 to i64  ; 0x4014ba
  %st.p.18 = inttoptr i64 %addr.z.25 to ptr  ; 0x4014ba
  store i32 0, ptr %st.p.18, align 1  ; 0x4014ba
  br label %f1_4014e0
f1_4014e0:  ; 0x4014e0
  %sp.7 = sub i32 %sp.6, 4  ; 0x4014e0
  %addr.z.26 = zext i32 %sp.7 to i64  ; 0x4014e0
  %st.p.19 = inttoptr i64 %addr.z.26 to ptr  ; 0x4014e0
  store i32 %eh.frame.2, ptr %st.p.19, align 1  ; 0x4014e0
  %diff.1 = sub i32 %sp.7, 8  ; 0x4014e1
  %sum = add i32 %eh.frame.2, 12  ; 0x4014e4
  %addr.9 = add i32 %sum, -28  ; 0x4014e7
  %addr.z.27 = zext i32 %addr.9 to i64  ; 0x4014e7
  %st.p.20 = inttoptr i64 %addr.z.27 to ptr  ; 0x4014e7
  store i32 %diff.1, ptr %st.p.20, align 1  ; 0x4014e7
  %addr.10 = add i32 %sum, -36  ; 0x4014ea
  %addr.z.28 = zext i32 %addr.10 to i64  ; 0x4014ea
  %m.p.2 = inttoptr i64 %addr.z.28 to ptr  ; 0x4014ea
  %m.2 = load i32, ptr %m.p.2, align 1  ; 0x4014ea
  %product = mul i32 %m.2, 100  ; 0x4014ea
  %addr.11 = add i32 %sum, -40  ; 0x4014ee
  %addr.z.29 = zext i32 %addr.11 to i64  ; 0x4014ee
  %m.p.3 = inttoptr i64 %addr.z.29 to ptr  ; 0x4014ee
  %m.3 = load i32, ptr %m.p.3, align 1  ; 0x4014ee
  %addr.12 = add i32 %m.3, 12  ; 0x4014f1
  %addr.z.30 = zext i32 %addr.12 to i64  ; 0x4014f1
  %m.p.4 = inttoptr i64 %addr.z.30 to ptr  ; 0x4014f1
  %m.4 = load i32, ptr %m.p.4, align 1  ; 0x4014f1
  %sum.1 = add i32 %product, %m.4  ; 0x4014f1
  %addr.13 = add i32 %sum, -32  ; 0x4014f4
  %addr.z.31 = zext i32 %addr.13 to i64  ; 0x4014f4
  %st.p.21 = inttoptr i64 %addr.z.31 to ptr  ; 0x4014f4
  store i32 %sum.1, ptr %st.p.21, align 1  ; 0x4014f4
  %sum.2 = add i32 %diff.1, 8  ; 0x4014fc
  %sp.8 = add i32 %sum.2, 4  ; 0x4014ff
  %sp.9 = add i32 %sp.8, 4  ; 0x401500
  %eh.ending = call i64 @ra_eh_ending_object()  ; 0x401500
  %eh.dtor = call i64 @ra_eh_ending_destructor()  ; 0x401500
  call void @ra_eh_leave_catch()  ; 0x401500
  %eh.dies = icmp ne i64 %eh.dtor, 0  ; 0x401500
  br i1 %eh.dies, label %eh.destroy.401500, label %eh.destroyed.401500
eh.destroy.401500:  ; 0x401500
  %eh.this = trunc i64 %eh.ending to i32  ; 0x401500
  %ecx.z.3 = zext i32 %eh.this to i64  ; 0x401500
  %sp.10 = sub i32 %sp.9, 4  ; 0x401500
  %addr.z.34 = zext i32 %sp.10 to i64  ; 0x401500
  %st.p.22 = inttoptr i64 %addr.z.34 to ptr  ; 0x401500
  store i32 0, ptr %st.p.22, align 1  ; 0x401500
  %fp.7 = inttoptr i64 %eh.dtor to ptr  ; 0x401500
  %sarg0.a.8 = add i64 %addr.z.34, 4
  %sarg0.p.8 = inttoptr i64 %sarg0.a.8 to ptr
  %sarg0.d.8 = load i32, ptr %sarg0.p.8, align 4
  %sarg0.8 = zext i32 %sarg0.d.8 to i64
  %sarg1.a.7 = add i64 %addr.z.34, 8
  %sarg1.p.7 = inttoptr i64 %sarg1.a.7 to ptr
  %sarg1.d.7 = load i32, ptr %sarg1.p.7, align 4
  %sarg1.7 = zext i32 %sarg1.d.7 to i64
  %sarg2.a.7 = add i64 %addr.z.34, 12
  %sarg2.p.7 = inttoptr i64 %sarg2.a.7 to ptr
  %sarg2.d.7 = load i32, ptr %sarg2.p.7, align 4
  %sarg2.7 = zext i32 %sarg2.d.7 to i64
  %sarg3.a.7 = add i64 %addr.z.34, 16
  %sarg3.p.7 = inttoptr i64 %sarg3.a.7 to ptr
  %sarg3.d.7 = load i32, ptr %sarg3.p.7, align 4
  %sarg3.7 = zext i32 %sarg3.d.7 to i64
  %sarg4.a.7 = add i64 %addr.z.34, 20
  %sarg4.p.7 = inttoptr i64 %sarg4.a.7 to ptr
  %sarg4.d.7 = load i32, ptr %sarg4.p.7, align 4
  %sarg4.7 = zext i32 %sarg4.d.7 to i64
  %sarg5.a.7 = add i64 %addr.z.34, 24
  %sarg5.p.7 = inttoptr i64 %sarg5.a.7 to ptr
  %sarg5.d.7 = load i32, ptr %sarg5.p.7, align 4
  %sarg5.7 = zext i32 %sarg5.d.7 to i64
  %sarg6.a.7 = add i64 %addr.z.34, 28
  %sarg6.p.7 = inttoptr i64 %sarg6.a.7 to ptr
  %sarg6.d.7 = load i32, ptr %sarg6.p.7, align 4
  %sarg6.7 = zext i32 %sarg6.d.7 to i64
  %args.p.14 = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr  ; 0x401500
  store i64 %sarg6.7, ptr %args.p.14, align 8  ; 0x401500
  %sarg7.a.7 = add i64 %addr.z.34, 32
  %sarg7.p.7 = inttoptr i64 %sarg7.a.7 to ptr
  %sarg7.d.7 = load i32, ptr %sarg7.p.7, align 4
  %sarg7.7 = zext i32 %sarg7.d.7 to i64
  %args.p.15 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr  ; 0x401500
  store i64 %sarg7.7, ptr %args.p.15, align 8  ; 0x401500
  %ret.8 = call i64 %fp.7(i64 %sarg0.8, i64 %sarg1.7, i64 %sarg2.7, i64 %sarg3.7, i64 %sarg4.7, i64 %sarg5.7, i64 %ecx.z.3, i64 undef)  ; 0x401500
  %pops.7 = call i64 @ra_pops_of(i64 %eh.dtor)  ; 0x401500
  br label %eh.destroyed.401500
eh.destroyed.401500:  ; 0x401500
  store i32 %eh.esp, ptr %eh.esp.p.1, align 1  ; 0x401500
  br label %b_4014d6
eh.state1:  ; 0x401500
  %eh.frame.3 = add i32 %eh.reg, 12  ; 0x401500
  %eh.catch.obj.2 = add i32 %eh.frame.3, -32  ; 0x401500
  %eh.catch.obj.3 = zext i32 %eh.catch.obj.2 to i64  ; 0x401500
  %eh.caught.1 = call i64 @ra_eh_catches(i64 7474415530116504195, i64 %eh.catch.obj.3, i64 8200)  ; 0x401500
  %eh.hit.1 = icmp ne i64 %eh.caught.1, 0  ; 0x401500
  br i1 %eh.hit.1, label %eh.catch0_0.s1, label %eh.next0_0.s1
eh.catch0_0.s1:  ; 0x401500
  %eh.head.f8.5 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x401500
  store i32 1, ptr %eh.head.f8.5, align 1  ; 0x401500
  br label %eh.enter0_0
eh.next0_0.s1:  ; 0x401500
  %eh.catch.obj.12 = add i32 %eh.frame.3, -28  ; 0x401534
  %eh.catch.obj.13 = zext i32 %eh.catch.obj.12 to i64  ; 0x401534
  %eh.caught.6 = call i64 @ra_eh_catches(i64 7474415530116504195, i64 %eh.catch.obj.13, i64 8200)  ; 0x401534
  %eh.hit.6 = icmp ne i64 %eh.caught.6, 0  ; 0x401534
  br i1 %eh.hit.6, label %eh.catch1_0.s1.1, label %eh.next1_0.s1.1
eh.enter0_0:  ; 0x401500
  call void @ra_eh_enter_catch()  ; 0x401500
  %eh.head.f8.6 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x401500
  store i32 2, ptr %eh.head.f8.6, align 1  ; 0x401500
  %eh.esp.p.3 = sub i32 %eh.reg, 4  ; 0x401500
  %addr.z.41 = zext i32 %eh.esp.p.3 to i64  ; 0x401500
  %eh.esp.p.4 = inttoptr i64 %addr.z.41 to ptr  ; 0x401500
  %eh.esp.2 = load i32, ptr %eh.esp.p.4, align 1  ; 0x401500
  %eh.esp.3 = zext i32 %eh.esp.2 to i64  ; 0x401500
  call void @ra_eh_catch_sp(i64 %eh.esp.3)  ; 0x401500
  %sp.11 = sub i32 %sarg0.a, 4  ; 0x401500
  %st.p.28 = getelementptr inbounds i8, ptr %frame.top, i64 -56  ; 0x401500
  store i32 0, ptr %st.p.28, align 1  ; 0x401500
  br label %f2_401510
f2_401510:  ; 0x401510
  %sp.12 = sub i32 %sp.11, 4  ; 0x401510
  %st.p.29 = getelementptr inbounds i8, ptr %frame.top, i64 -60  ; 0x401510
  store i32 %eh.frame.3, ptr %st.p.29, align 1  ; 0x401510
  %diff.2 = sub i32 %sp.12, 8  ; 0x401511
  %sum.3 = add i32 %eh.frame.3, 12  ; 0x401514
  %addr.14 = add i32 %sum.3, -28  ; 0x401517
  %addr.z.44 = zext i32 %addr.14 to i64  ; 0x401517
  %st.p.30 = inttoptr i64 %addr.z.44 to ptr  ; 0x401517
  store i32 %diff.2, ptr %st.p.30, align 1  ; 0x401517
  %addr.15 = add i32 %sum.3, -44  ; 0x40151a
  %addr.z.45 = zext i32 %addr.15 to i64  ; 0x40151a
  %m.p.5 = inttoptr i64 %addr.z.45 to ptr  ; 0x40151a
  %m.5 = load i32, ptr %m.p.5, align 1  ; 0x40151a
  %eax.z.4 = zext i32 %m.5 to i64  ; 0x40151a
  %addr.16 = add i32 %m.5, 12  ; 0x40151d
  %addr.z.46 = zext i32 %addr.16 to i64  ; 0x40151d
  %m.p.6 = inttoptr i64 %addr.z.46 to ptr  ; 0x40151d
  %m.6 = load i32, ptr %m.p.6, align 1  ; 0x40151d
  %addr.17 = add i32 %m.6, 1  ; 0x401520
  store i32 %addr.17, ptr %m.p.6, align 1  ; 0x401523
  %addr.19 = add i32 %sum.3, -36  ; 0x401526
  %addr.z.48 = zext i32 %addr.19 to i64  ; 0x401526
  %st.p.32 = inttoptr i64 %addr.z.48 to ptr  ; 0x401526
  store i32 %m.6, ptr %st.p.32, align 1  ; 0x401526
  %eh.head.f8.7 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x401529
  store i32 2, ptr %eh.head.f8.7, align 1  ; 0x401529
  %sp.13 = sub i32 %diff.2, 4  ; 0x401530
  %st.p.34 = getelementptr inbounds i8, ptr %frame.top, i64 -72  ; 0x401530
  store i32 0, ptr %st.p.34, align 1  ; 0x401530
  %sp.14 = sub i32 %sp.13, 4  ; 0x401532
  %st.p.35 = getelementptr inbounds i8, ptr %frame.top, i64 -76  ; 0x401532
  store i32 0, ptr %st.p.35, align 1  ; 0x401532
  call void @ra_eh_rethrow()  ; 0x401534
  br label %eh.landing.1
eh.landing.1:  ; 0x401534
  %rax.phi.2 = phi i64 [ %eax.z.4, %f2_401510 ], [ %eax.z.7, %f4_401510 ]  ; 0x401534
  %rsp.phi.1 = phi i32 [ %sp.14, %f2_401510 ], [ %sp.23, %f4_401510 ]  ; 0x401534
  %teb.10 = call i64 @ra_teb()  ; 0x401534
  %teb.11 = trunc i64 %teb.10 to i32  ; 0x401534
  %addr.z.52 = zext i32 %teb.11 to i64  ; 0x401534
  %eh.head.p.1 = inttoptr i64 %addr.z.52 to ptr  ; 0x401534
  %eh.head.1 = load i32, ptr %eh.head.p.1, align 1  ; 0x401534
  %eh.head.1.s.w = zext i32 %eh.head.1 to i64  ; 0x401534
  %eh.head.1.s = inttoptr i64 %eh.head.1.s.w to ptr  ; 0x401534
  %eh.frame.5 = sub i32 %eh.head.1, -24  ; 0x401534
  %eh.reg.1 = add i32 %eh.frame.5, -24  ; 0x401534
  %eh.head.1.f4 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 1  ; 0x401534
  %eh.handler.1 = load i32, ptr %eh.head.1.f4, align 1  ; 0x401534
  %eh.ours.3 = icmp eq i32 %eh.handler.1, 4201472  ; 0x401534
  %eh.ours.4 = icmp eq i32 %eh.handler.1, ptrtoint (ptr @U_00401c00 to i32)  ; 0x401534
  %eh.ours.5 = or i1 %eh.ours.3, %eh.ours.4  ; 0x401534
  br i1 %eh.ours.5, label %eh.dispatch.1, label %eh.foreign.1
eh.dispatch.1:  ; 0x401534
  %eh.head.1.f8 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x401534
  %eh.state.1 = load i32, ptr %eh.head.1.f8, align 1  ; 0x401534
  switch i32 %eh.state.1, label %eh.state.bad.1 [ i32 -1, label %eh.state_none.1 i32 0, label %eh.state0.1 i32 1, label %eh.state1.1 i32 2, label %eh.state2 i32 3, label %eh.state3 ]
eh.foreign.1:  ; 0x401534
  call void @ra_trap(i64 4199536, i64 8)  ; 0x401534
  unreachable
eh.state.bad.1:  ; 0x401534
  call void @ra_trap(i64 4199536, i64 7)  ; 0x401534
  unreachable
eh.state_none.1:  ; 0x401534
  call void @ra_eh_suspend()  ; 0x401534
  %eh.head.1.f8.1 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x401534
  store i32 -1, ptr %eh.head.1.f8.1, align 1  ; 0x401534
  call void @ra_eh_resume()  ; 0x401534
  %eh.head.1.f0 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 0  ; 0x401534
  %eh.next.2 = load i32, ptr %eh.head.1.f0, align 1  ; 0x401534
  %teb.12 = call i64 @ra_teb()  ; 0x401534
  %teb.13 = trunc i64 %teb.12 to i32  ; 0x401534
  %addr.z.57 = zext i32 %teb.13 to i64  ; 0x401534
  %st.p.37 = inttoptr i64 %addr.z.57 to ptr  ; 0x401534
  store i32 %eh.next.2, ptr %st.p.37, align 1  ; 0x401534
  ret i64 %rax.phi.2
eh.state0.1:  ; 0x401534
  %eh.frame.6 = add i32 %eh.reg.1, 12  ; 0x401534
  %eh.catch.obj.4 = add i32 %eh.frame.6, -28  ; 0x401534
  %eh.catch.obj.5 = zext i32 %eh.catch.obj.4 to i64  ; 0x401534
  %eh.caught.2 = call i64 @ra_eh_catches(i64 7474415530116504195, i64 %eh.catch.obj.5, i64 8200)  ; 0x401534
  %eh.hit.2 = icmp ne i64 %eh.caught.2, 0  ; 0x401534
  br i1 %eh.hit.2, label %eh.catch1_0.s0.1, label %eh.next1_0.s0.1
eh.catch1_0.s0.1:  ; 0x401534
  %eh.head.1.f8.2 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x401534
  store i32 0, ptr %eh.head.1.f8.2, align 1  ; 0x401534
  br label %eh.enter1_0.1
eh.next1_0.s0.1:  ; 0x401534
  call void @ra_eh_suspend()  ; 0x401500
  %eh.head.1.f8.3 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x401500
  store i32 -1, ptr %eh.head.1.f8.3, align 1  ; 0x401500
  call void @ra_eh_resume()  ; 0x401500
  %eh.head.1.f0.1 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 0  ; 0x401500
  %eh.next.3 = load i32, ptr %eh.head.1.f0.1, align 1  ; 0x401500
  %teb.14 = call i64 @ra_teb()  ; 0x401500
  %teb.15 = trunc i64 %teb.14 to i32  ; 0x401500
  %addr.z.74 = zext i32 %teb.15 to i64  ; 0x401500
  %st.p.47 = inttoptr i64 %addr.z.74 to ptr  ; 0x401500
  store i32 %eh.next.3, ptr %st.p.47, align 1  ; 0x401500
  ret i64 %rax.phi.2
eh.enter1_0.1:  ; 0x401534
  %rsp.phi.3 = phi i32 [ %rsp.phi.1, %eh.catch1_0.s0.1 ], [ %rsp.phi.1, %eh.catch1_0.s1 ], [ %rsp.phi.2, %eh.destroyed.401470 ]  ; 0x401534
  call void @ra_eh_enter_catch()  ; 0x401534
  %eh.head.1.f8.4 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x401534
  store i32 3, ptr %eh.head.1.f8.4, align 1  ; 0x401534
  %eh.esp.p.5 = sub i32 %eh.reg.1, 4  ; 0x401534
  %addr.z.60 = zext i32 %eh.esp.p.5 to i64  ; 0x401534
  %eh.esp.p.6 = inttoptr i64 %addr.z.60 to ptr  ; 0x401534
  %eh.esp.4 = load i32, ptr %eh.esp.p.6, align 1  ; 0x401534
  %eh.esp.5 = zext i32 %eh.esp.4 to i64  ; 0x401534
  call void @ra_eh_catch_sp(i64 %eh.esp.5)  ; 0x401534
  %eh.frame.7 = add i32 %eh.reg.1, 12  ; 0x401534
  %sp.15 = sub i32 %rsp.phi.3, 4  ; 0x401534
  %addr.z.61 = zext i32 %sp.15 to i64  ; 0x401534
  %st.p.40 = inttoptr i64 %addr.z.61 to ptr  ; 0x401534
  store i32 0, ptr %st.p.40, align 1  ; 0x401534
  br label %f3_4014e0
f3_4014e0:  ; 0x4014e0
  %sp.16 = sub i32 %sp.15, 4  ; 0x4014e0
  %addr.z.62 = zext i32 %sp.16 to i64  ; 0x4014e0
  %st.p.41 = inttoptr i64 %addr.z.62 to ptr  ; 0x4014e0
  store i32 %eh.frame.7, ptr %st.p.41, align 1  ; 0x4014e0
  %diff.3 = sub i32 %sp.16, 8  ; 0x4014e1
  %sum.4 = add i32 %eh.frame.7, 12  ; 0x4014e4
  %addr.21 = add i32 %sum.4, -28  ; 0x4014e7
  %addr.z.63 = zext i32 %addr.21 to i64  ; 0x4014e7
  %st.p.42 = inttoptr i64 %addr.z.63 to ptr  ; 0x4014e7
  store i32 %diff.3, ptr %st.p.42, align 1  ; 0x4014e7
  %addr.22 = add i32 %sum.4, -36  ; 0x4014ea
  %addr.z.64 = zext i32 %addr.22 to i64  ; 0x4014ea
  %m.p.7 = inttoptr i64 %addr.z.64 to ptr  ; 0x4014ea
  %m.7 = load i32, ptr %m.p.7, align 1  ; 0x4014ea
  %product.1 = mul i32 %m.7, 100  ; 0x4014ea
  %addr.23 = add i32 %sum.4, -40  ; 0x4014ee
  %addr.z.65 = zext i32 %addr.23 to i64  ; 0x4014ee
  %m.p.8 = inttoptr i64 %addr.z.65 to ptr  ; 0x4014ee
  %m.8 = load i32, ptr %m.p.8, align 1  ; 0x4014ee
  %addr.24 = add i32 %m.8, 12  ; 0x4014f1
  %addr.z.66 = zext i32 %addr.24 to i64  ; 0x4014f1
  %m.p.9 = inttoptr i64 %addr.z.66 to ptr  ; 0x4014f1
  %m.9 = load i32, ptr %m.p.9, align 1  ; 0x4014f1
  %sum.5 = add i32 %product.1, %m.9  ; 0x4014f1
  %addr.25 = add i32 %sum.4, -32  ; 0x4014f4
  %addr.z.67 = zext i32 %addr.25 to i64  ; 0x4014f4
  %st.p.43 = inttoptr i64 %addr.z.67 to ptr  ; 0x4014f4
  store i32 %sum.5, ptr %st.p.43, align 1  ; 0x4014f4
  %sum.6 = add i32 %diff.3, 8  ; 0x4014fc
  %sp.17 = add i32 %sum.6, 4  ; 0x4014ff
  %sp.18 = add i32 %sp.17, 4  ; 0x401500
  %eh.ending.1 = call i64 @ra_eh_ending_object()  ; 0x401500
  %eh.dtor.1 = call i64 @ra_eh_ending_destructor()  ; 0x401500
  call void @ra_eh_leave_catch()  ; 0x401500
  %eh.dies.1 = icmp ne i64 %eh.dtor.1, 0  ; 0x401500
  br i1 %eh.dies.1, label %eh.destroy.401500.1, label %eh.destroyed.401500.1
eh.destroy.401500.1:  ; 0x401500
  %eh.this.1 = trunc i64 %eh.ending.1 to i32  ; 0x401500
  %ecx.z.7 = zext i32 %eh.this.1 to i64  ; 0x401500
  %sp.19 = sub i32 %sp.18, 4  ; 0x401500
  %addr.z.70 = zext i32 %sp.19 to i64  ; 0x401500
  %st.p.44 = inttoptr i64 %addr.z.70 to ptr  ; 0x401500
  store i32 0, ptr %st.p.44, align 1  ; 0x401500
  %fp.6 = inttoptr i64 %eh.dtor.1 to ptr  ; 0x401500
  %sarg0.a.7 = add i64 %addr.z.70, 4
  %sarg0.p.7 = inttoptr i64 %sarg0.a.7 to ptr
  %sarg0.d.7 = load i32, ptr %sarg0.p.7, align 4
  %sarg0.7 = zext i32 %sarg0.d.7 to i64
  %sarg1.a.6 = add i64 %addr.z.70, 8
  %sarg1.p.6 = inttoptr i64 %sarg1.a.6 to ptr
  %sarg1.d.6 = load i32, ptr %sarg1.p.6, align 4
  %sarg1.6 = zext i32 %sarg1.d.6 to i64
  %sarg2.a.6 = add i64 %addr.z.70, 12
  %sarg2.p.6 = inttoptr i64 %sarg2.a.6 to ptr
  %sarg2.d.6 = load i32, ptr %sarg2.p.6, align 4
  %sarg2.6 = zext i32 %sarg2.d.6 to i64
  %sarg3.a.6 = add i64 %addr.z.70, 16
  %sarg3.p.6 = inttoptr i64 %sarg3.a.6 to ptr
  %sarg3.d.6 = load i32, ptr %sarg3.p.6, align 4
  %sarg3.6 = zext i32 %sarg3.d.6 to i64
  %sarg4.a.6 = add i64 %addr.z.70, 20
  %sarg4.p.6 = inttoptr i64 %sarg4.a.6 to ptr
  %sarg4.d.6 = load i32, ptr %sarg4.p.6, align 4
  %sarg4.6 = zext i32 %sarg4.d.6 to i64
  %sarg5.a.6 = add i64 %addr.z.70, 24
  %sarg5.p.6 = inttoptr i64 %sarg5.a.6 to ptr
  %sarg5.d.6 = load i32, ptr %sarg5.p.6, align 4
  %sarg5.6 = zext i32 %sarg5.d.6 to i64
  %sarg6.a.6 = add i64 %addr.z.70, 28
  %sarg6.p.6 = inttoptr i64 %sarg6.a.6 to ptr
  %sarg6.d.6 = load i32, ptr %sarg6.p.6, align 4
  %sarg6.6 = zext i32 %sarg6.d.6 to i64
  %args.p.12 = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr  ; 0x401500
  store i64 %sarg6.6, ptr %args.p.12, align 8  ; 0x401500
  %sarg7.a.6 = add i64 %addr.z.70, 32
  %sarg7.p.6 = inttoptr i64 %sarg7.a.6 to ptr
  %sarg7.d.6 = load i32, ptr %sarg7.p.6, align 4
  %sarg7.6 = zext i32 %sarg7.d.6 to i64
  %args.p.13 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr  ; 0x401500
  store i64 %sarg7.6, ptr %args.p.13, align 8  ; 0x401500
  %ret.7 = call i64 %fp.6(i64 %sarg0.7, i64 %sarg1.6, i64 %sarg2.6, i64 %sarg3.6, i64 %sarg4.6, i64 %sarg5.6, i64 %ecx.z.7, i64 undef)  ; 0x401500
  %pops.6 = call i64 @ra_pops_of(i64 %eh.dtor.1)  ; 0x401500
  br label %eh.destroyed.401500.1
eh.destroyed.401500.1:  ; 0x401500
  store i32 %eh.esp.4, ptr %eh.esp.p.6, align 1  ; 0x401500
  br label %b_4014d6
eh.state1.1:  ; 0x401500
  %eh.frame.8 = add i32 %eh.reg.1, 12  ; 0x401500
  %eh.catch.obj.6 = add i32 %eh.frame.8, -32  ; 0x401500
  %eh.catch.obj.7 = zext i32 %eh.catch.obj.6 to i64  ; 0x401500
  %eh.caught.3 = call i64 @ra_eh_catches(i64 7474415530116504195, i64 %eh.catch.obj.7, i64 8200)  ; 0x401500
  %eh.hit.3 = icmp ne i64 %eh.caught.3, 0  ; 0x401500
  br i1 %eh.hit.3, label %eh.catch0_0.s1.1, label %eh.next0_0.s1.1
eh.catch0_0.s1.1:  ; 0x401500
  %eh.head.1.f8.5 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x401500
  store i32 1, ptr %eh.head.1.f8.5, align 1  ; 0x401500
  br label %eh.enter0_0.1
eh.next0_0.s1.1:  ; 0x401500
  %eh.catch.obj.8 = add i32 %eh.frame.8, -28  ; 0x401534
  %eh.catch.obj.9 = zext i32 %eh.catch.obj.8 to i64  ; 0x401534
  %eh.caught.4 = call i64 @ra_eh_catches(i64 7474415530116504195, i64 %eh.catch.obj.9, i64 8200)  ; 0x401534
  %eh.hit.4 = icmp ne i64 %eh.caught.4, 0  ; 0x401534
  br i1 %eh.hit.4, label %eh.catch1_0.s1, label %eh.next1_0.s1
eh.enter0_0.1:  ; 0x401500
  call void @ra_eh_enter_catch()  ; 0x401500
  %eh.head.1.f8.6 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x401500
  store i32 2, ptr %eh.head.1.f8.6, align 1  ; 0x401500
  %eh.esp.p.8 = sub i32 %eh.reg.1, 4  ; 0x401500
  %addr.z.77 = zext i32 %eh.esp.p.8 to i64  ; 0x401500
  %eh.esp.p.9 = inttoptr i64 %addr.z.77 to ptr  ; 0x401500
  %eh.esp.6 = load i32, ptr %eh.esp.p.9, align 1  ; 0x401500
  %eh.esp.7 = zext i32 %eh.esp.6 to i64  ; 0x401500
  call void @ra_eh_catch_sp(i64 %eh.esp.7)  ; 0x401500
  %sp.20 = sub i32 %rsp.phi.1, 4  ; 0x401500
  %addr.z.78 = zext i32 %sp.20 to i64  ; 0x401500
  %st.p.50 = inttoptr i64 %addr.z.78 to ptr  ; 0x401500
  store i32 0, ptr %st.p.50, align 1  ; 0x401500
  br label %f4_401510
f4_401510:  ; 0x401510
  %sp.21 = sub i32 %sp.20, 4  ; 0x401510
  %addr.z.79 = zext i32 %sp.21 to i64  ; 0x401510
  %st.p.51 = inttoptr i64 %addr.z.79 to ptr  ; 0x401510
  store i32 %eh.frame.8, ptr %st.p.51, align 1  ; 0x401510
  %diff.4 = sub i32 %sp.21, 8  ; 0x401511
  %sum.7 = add i32 %eh.frame.8, 12  ; 0x401514
  %addr.26 = add i32 %sum.7, -28  ; 0x401517
  %addr.z.80 = zext i32 %addr.26 to i64  ; 0x401517
  %st.p.52 = inttoptr i64 %addr.z.80 to ptr  ; 0x401517
  store i32 %diff.4, ptr %st.p.52, align 1  ; 0x401517
  %addr.27 = add i32 %sum.7, -44  ; 0x40151a
  %addr.z.81 = zext i32 %addr.27 to i64  ; 0x40151a
  %m.p.10 = inttoptr i64 %addr.z.81 to ptr  ; 0x40151a
  %m.10 = load i32, ptr %m.p.10, align 1  ; 0x40151a
  %eax.z.7 = zext i32 %m.10 to i64  ; 0x40151a
  %addr.28 = add i32 %m.10, 12  ; 0x40151d
  %addr.z.82 = zext i32 %addr.28 to i64  ; 0x40151d
  %m.p.11 = inttoptr i64 %addr.z.82 to ptr  ; 0x40151d
  %m.11 = load i32, ptr %m.p.11, align 1  ; 0x40151d
  %addr.29 = add i32 %m.11, 1  ; 0x401520
  store i32 %addr.29, ptr %m.p.11, align 1  ; 0x401523
  %addr.31 = add i32 %sum.7, -36  ; 0x401526
  %addr.z.84 = zext i32 %addr.31 to i64  ; 0x401526
  %st.p.54 = inttoptr i64 %addr.z.84 to ptr  ; 0x401526
  store i32 %m.11, ptr %st.p.54, align 1  ; 0x401526
  %eh.head.1.f8.7 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x401529
  store i32 2, ptr %eh.head.1.f8.7, align 1  ; 0x401529
  %sp.22 = sub i32 %diff.4, 4  ; 0x401530
  %addr.z.86 = zext i32 %sp.22 to i64  ; 0x401530
  %st.p.56 = inttoptr i64 %addr.z.86 to ptr  ; 0x401530
  store i32 0, ptr %st.p.56, align 1  ; 0x401530
  %sp.23 = sub i32 %sp.22, 4  ; 0x401532
  %addr.z.87 = zext i32 %sp.23 to i64  ; 0x401532
  %st.p.57 = inttoptr i64 %addr.z.87 to ptr  ; 0x401532
  store i32 0, ptr %st.p.57, align 1  ; 0x401532
  call void @ra_eh_rethrow()  ; 0x401534
  br label %eh.landing.1
eh.catch1_0.s1:  ; 0x401534
  %eh.head.1.f8.8 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x401534
  store i32 0, ptr %eh.head.1.f8.8, align 1  ; 0x401534
  br label %eh.enter1_0.1
eh.next1_0.s1:  ; 0x401534
  call void @ra_eh_suspend()  ; 0x401534
  %eh.head.1.f8.9 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x401534
  store i32 -1, ptr %eh.head.1.f8.9, align 1  ; 0x401534
  call void @ra_eh_resume()  ; 0x401534
  %eh.head.1.f0.2 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 0  ; 0x401534
  %eh.next.4 = load i32, ptr %eh.head.1.f0.2, align 1  ; 0x401534
  %teb.16 = call i64 @ra_teb()  ; 0x401534
  %teb.17 = trunc i64 %teb.16 to i32  ; 0x401534
  %addr.z.91 = zext i32 %teb.17 to i64  ; 0x401534
  %st.p.60 = inttoptr i64 %addr.z.91 to ptr  ; 0x401534
  store i32 %eh.next.4, ptr %st.p.60, align 1  ; 0x401534
  ret i64 %rax.phi.2
eh.state2:  ; 0x401534
  %eh.frame.11 = add i32 %eh.reg.1, 12  ; 0x401534
  %eh.catch.obj.10 = add i32 %eh.frame.11, -28  ; 0x401534
  %eh.catch.obj.11 = zext i32 %eh.catch.obj.10 to i64  ; 0x401534
  %eh.caught.5 = call i64 @ra_eh_catches(i64 7474415530116504195, i64 %eh.catch.obj.11, i64 8200)  ; 0x401534
  %eh.hit.5 = icmp ne i64 %eh.caught.5, 0  ; 0x401534
  br i1 %eh.hit.5, label %eh.catch1_0.s2, label %eh.next1_0.s2
eh.catch1_0.s2:  ; 0x401534
  %eh.esp.saved = call i64 @ra_eh_ending_sp()  ; 0x401534
  %eh.esp.saved.1 = trunc i64 %eh.esp.saved to i32  ; 0x401534
  %eh.esp.p.10 = sub i32 %eh.reg.1, 4  ; 0x401534
  %addr.z.92 = zext i32 %eh.esp.p.10 to i64  ; 0x401534
  %st.p.61 = inttoptr i64 %addr.z.92 to ptr  ; 0x401534
  store i32 %eh.esp.saved.1, ptr %st.p.61, align 1  ; 0x401534
  %eh.ending.2 = call i64 @ra_eh_ending_object()  ; 0x401534
  %eh.dtor.2 = call i64 @ra_eh_ending_destructor()  ; 0x401534
  call void @ra_eh_leave_catch()  ; 0x401534
  %eh.dies.2 = icmp ne i64 %eh.dtor.2, 0  ; 0x401534
  br i1 %eh.dies.2, label %eh.destroy.401470, label %eh.destroyed.401470
eh.next1_0.s2:  ; 0x401534
  call void @ra_eh_suspend()  ; 0x401534
  %eh.esp.saved.2 = call i64 @ra_eh_ending_sp()  ; 0x401534
  %eh.esp.saved.3 = trunc i64 %eh.esp.saved.2 to i32  ; 0x401534
  %eh.esp.p.11 = sub i32 %eh.reg.1, 4  ; 0x401534
  %addr.z.95 = zext i32 %eh.esp.p.11 to i64  ; 0x401534
  %st.p.64 = inttoptr i64 %addr.z.95 to ptr  ; 0x401534
  store i32 %eh.esp.saved.3, ptr %st.p.64, align 1  ; 0x401534
  %eh.ending.3 = call i64 @ra_eh_ending_object()  ; 0x401534
  %eh.dtor.3 = call i64 @ra_eh_ending_destructor()  ; 0x401534
  call void @ra_eh_leave_catch()  ; 0x401534
  %eh.dies.3 = icmp ne i64 %eh.dtor.3, 0  ; 0x401534
  br i1 %eh.dies.3, label %eh.destroy.401470.1, label %eh.destroyed.401470.1
eh.destroy.401470:  ; 0x401534
  %eh.this.2 = trunc i64 %eh.ending.2 to i32  ; 0x401534
  %ecx.z.9 = zext i32 %eh.this.2 to i64  ; 0x401534
  %sp.24 = sub i32 %rsp.phi.1, 4  ; 0x401534
  %addr.z.93 = zext i32 %sp.24 to i64  ; 0x401534
  %st.p.62 = inttoptr i64 %addr.z.93 to ptr  ; 0x401534
  store i32 0, ptr %st.p.62, align 1  ; 0x401534
  %fp.5 = inttoptr i64 %eh.dtor.2 to ptr  ; 0x401534
  %sarg0.a.6 = add i64 %addr.z.93, 4
  %sarg0.p.6 = inttoptr i64 %sarg0.a.6 to ptr
  %sarg0.d.6 = load i32, ptr %sarg0.p.6, align 4
  %sarg0.6 = zext i32 %sarg0.d.6 to i64
  %sarg1.a.5 = add i64 %addr.z.93, 8
  %sarg1.p.5 = inttoptr i64 %sarg1.a.5 to ptr
  %sarg1.d.5 = load i32, ptr %sarg1.p.5, align 4
  %sarg1.5 = zext i32 %sarg1.d.5 to i64
  %sarg2.a.5 = add i64 %addr.z.93, 12
  %sarg2.p.5 = inttoptr i64 %sarg2.a.5 to ptr
  %sarg2.d.5 = load i32, ptr %sarg2.p.5, align 4
  %sarg2.5 = zext i32 %sarg2.d.5 to i64
  %sarg3.a.5 = add i64 %addr.z.93, 16
  %sarg3.p.5 = inttoptr i64 %sarg3.a.5 to ptr
  %sarg3.d.5 = load i32, ptr %sarg3.p.5, align 4
  %sarg3.5 = zext i32 %sarg3.d.5 to i64
  %sarg4.a.5 = add i64 %addr.z.93, 20
  %sarg4.p.5 = inttoptr i64 %sarg4.a.5 to ptr
  %sarg4.d.5 = load i32, ptr %sarg4.p.5, align 4
  %sarg4.5 = zext i32 %sarg4.d.5 to i64
  %sarg5.a.5 = add i64 %addr.z.93, 24
  %sarg5.p.5 = inttoptr i64 %sarg5.a.5 to ptr
  %sarg5.d.5 = load i32, ptr %sarg5.p.5, align 4
  %sarg5.5 = zext i32 %sarg5.d.5 to i64
  %sarg6.a.5 = add i64 %addr.z.93, 28
  %sarg6.p.5 = inttoptr i64 %sarg6.a.5 to ptr
  %sarg6.d.5 = load i32, ptr %sarg6.p.5, align 4
  %sarg6.5 = zext i32 %sarg6.d.5 to i64
  %args.p.10 = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr  ; 0x401534
  store i64 %sarg6.5, ptr %args.p.10, align 8  ; 0x401534
  %sarg7.a.5 = add i64 %addr.z.93, 32
  %sarg7.p.5 = inttoptr i64 %sarg7.a.5 to ptr
  %sarg7.d.5 = load i32, ptr %sarg7.p.5, align 4
  %sarg7.5 = zext i32 %sarg7.d.5 to i64
  %args.p.11 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr  ; 0x401534
  store i64 %sarg7.5, ptr %args.p.11, align 8  ; 0x401534
  %ret.6 = call i64 %fp.5(i64 %sarg0.6, i64 %sarg1.5, i64 %sarg2.5, i64 %sarg3.5, i64 %sarg4.5, i64 %sarg5.5, i64 %ecx.z.9, i64 undef)  ; 0x401534
  %pops.5 = call i64 @ra_pops_of(i64 %eh.dtor.2)  ; 0x401534
  %n.1 = trunc i64 %pops.5 to i32
  %pops.a.5 = add i32 %n.1, 4  ; 0x401534
  %rsp.56 = add i32 %sp.24, %pops.a.5  ; 0x401534
  br label %eh.destroyed.401470
eh.destroyed.401470:  ; 0x401534
  %rsp.phi.2 = phi i32 [ %rsp.phi.1, %eh.catch1_0.s2 ], [ %rsp.56, %eh.destroy.401470 ]  ; 0x401534
  %eh.head.1.f8.10 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x401534
  store i32 0, ptr %eh.head.1.f8.10, align 1  ; 0x401534
  br label %eh.enter1_0.1
eh.destroy.401470.1:  ; 0x401534
  %eh.this.3 = trunc i64 %eh.ending.3 to i32  ; 0x401534
  %ecx.z.10 = zext i32 %eh.this.3 to i64  ; 0x401534
  %sp.25 = sub i32 %rsp.phi.1, 4  ; 0x401534
  %addr.z.96 = zext i32 %sp.25 to i64  ; 0x401534
  %st.p.65 = inttoptr i64 %addr.z.96 to ptr  ; 0x401534
  store i32 0, ptr %st.p.65, align 1  ; 0x401534
  %fp.4 = inttoptr i64 %eh.dtor.3 to ptr  ; 0x401534
  %sarg0.a.5 = add i64 %addr.z.96, 4
  %sarg0.p.5 = inttoptr i64 %sarg0.a.5 to ptr
  %sarg0.d.5 = load i32, ptr %sarg0.p.5, align 4
  %sarg0.5 = zext i32 %sarg0.d.5 to i64
  %sarg1.a.4 = add i64 %addr.z.96, 8
  %sarg1.p.4 = inttoptr i64 %sarg1.a.4 to ptr
  %sarg1.d.4 = load i32, ptr %sarg1.p.4, align 4
  %sarg1.4 = zext i32 %sarg1.d.4 to i64
  %sarg2.a.4 = add i64 %addr.z.96, 12
  %sarg2.p.4 = inttoptr i64 %sarg2.a.4 to ptr
  %sarg2.d.4 = load i32, ptr %sarg2.p.4, align 4
  %sarg2.4 = zext i32 %sarg2.d.4 to i64
  %sarg3.a.4 = add i64 %addr.z.96, 16
  %sarg3.p.4 = inttoptr i64 %sarg3.a.4 to ptr
  %sarg3.d.4 = load i32, ptr %sarg3.p.4, align 4
  %sarg3.4 = zext i32 %sarg3.d.4 to i64
  %sarg4.a.4 = add i64 %addr.z.96, 20
  %sarg4.p.4 = inttoptr i64 %sarg4.a.4 to ptr
  %sarg4.d.4 = load i32, ptr %sarg4.p.4, align 4
  %sarg4.4 = zext i32 %sarg4.d.4 to i64
  %sarg5.a.4 = add i64 %addr.z.96, 24
  %sarg5.p.4 = inttoptr i64 %sarg5.a.4 to ptr
  %sarg5.d.4 = load i32, ptr %sarg5.p.4, align 4
  %sarg5.4 = zext i32 %sarg5.d.4 to i64
  %sarg6.a.4 = add i64 %addr.z.96, 28
  %sarg6.p.4 = inttoptr i64 %sarg6.a.4 to ptr
  %sarg6.d.4 = load i32, ptr %sarg6.p.4, align 4
  %sarg6.4 = zext i32 %sarg6.d.4 to i64
  %args.p.8 = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr  ; 0x401534
  store i64 %sarg6.4, ptr %args.p.8, align 8  ; 0x401534
  %sarg7.a.4 = add i64 %addr.z.96, 32
  %sarg7.p.4 = inttoptr i64 %sarg7.a.4 to ptr
  %sarg7.d.4 = load i32, ptr %sarg7.p.4, align 4
  %sarg7.4 = zext i32 %sarg7.d.4 to i64
  %args.p.9 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr  ; 0x401534
  store i64 %sarg7.4, ptr %args.p.9, align 8  ; 0x401534
  %ret.5 = call i64 %fp.4(i64 %sarg0.5, i64 %sarg1.4, i64 %sarg2.4, i64 %sarg3.4, i64 %sarg4.4, i64 %sarg5.4, i64 %ecx.z.10, i64 undef)  ; 0x401534
  %pops.4 = call i64 @ra_pops_of(i64 %eh.dtor.3)  ; 0x401534
  br label %eh.destroyed.401470.1
eh.destroyed.401470.1:  ; 0x401534
  %rax.phi.4 = phi i64 [ %rax.phi.2, %eh.next1_0.s2 ], [ %ret.5, %eh.destroy.401470.1 ]  ; 0x401534
  %eh.head.1.f8.11 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x401534
  store i32 -1, ptr %eh.head.1.f8.11, align 1  ; 0x401534
  call void @ra_eh_resume()  ; 0x401534
  %eh.head.1.f0.3 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 0  ; 0x401534
  %eh.next.5 = load i32, ptr %eh.head.1.f0.3, align 1  ; 0x401534
  %teb.18 = call i64 @ra_teb()  ; 0x401534
  %teb.19 = trunc i64 %teb.18 to i32  ; 0x401534
  %addr.z.99 = zext i32 %teb.19 to i64  ; 0x401534
  %st.p.67 = inttoptr i64 %addr.z.99 to ptr  ; 0x401534
  store i32 %eh.next.5, ptr %st.p.67, align 1  ; 0x401534
  ret i64 %rax.phi.4
eh.state3:  ; 0x401534
  call void @ra_eh_suspend()  ; 0x401534
  %eh.esp.saved.4 = call i64 @ra_eh_ending_sp()  ; 0x401534
  %eh.esp.saved.5 = trunc i64 %eh.esp.saved.4 to i32  ; 0x401534
  %eh.esp.p.12 = sub i32 %eh.reg.1, 4  ; 0x401534
  %addr.z.100 = zext i32 %eh.esp.p.12 to i64  ; 0x401534
  %st.p.68 = inttoptr i64 %addr.z.100 to ptr  ; 0x401534
  store i32 %eh.esp.saved.5, ptr %st.p.68, align 1  ; 0x401534
  %eh.ending.4 = call i64 @ra_eh_ending_object()  ; 0x401534
  %eh.dtor.4 = call i64 @ra_eh_ending_destructor()  ; 0x401534
  call void @ra_eh_leave_catch()  ; 0x401534
  %eh.dies.4 = icmp ne i64 %eh.dtor.4, 0  ; 0x401534
  br i1 %eh.dies.4, label %eh.destroy.401470.2, label %eh.destroyed.401470.2
eh.destroy.401470.2:  ; 0x401534
  %eh.this.4 = trunc i64 %eh.ending.4 to i32  ; 0x401534
  %ecx.z.11 = zext i32 %eh.this.4 to i64  ; 0x401534
  %sp.26 = sub i32 %rsp.phi.1, 4  ; 0x401534
  %addr.z.101 = zext i32 %sp.26 to i64  ; 0x401534
  %st.p.69 = inttoptr i64 %addr.z.101 to ptr  ; 0x401534
  store i32 0, ptr %st.p.69, align 1  ; 0x401534
  %fp.3 = inttoptr i64 %eh.dtor.4 to ptr  ; 0x401534
  %sarg0.a.4 = add i64 %addr.z.101, 4
  %sarg0.p.4 = inttoptr i64 %sarg0.a.4 to ptr
  %sarg0.d.4 = load i32, ptr %sarg0.p.4, align 4
  %sarg0.4 = zext i32 %sarg0.d.4 to i64
  %sarg1.a.3 = add i64 %addr.z.101, 8
  %sarg1.p.3 = inttoptr i64 %sarg1.a.3 to ptr
  %sarg1.d.3 = load i32, ptr %sarg1.p.3, align 4
  %sarg1.3 = zext i32 %sarg1.d.3 to i64
  %sarg2.a.3 = add i64 %addr.z.101, 12
  %sarg2.p.3 = inttoptr i64 %sarg2.a.3 to ptr
  %sarg2.d.3 = load i32, ptr %sarg2.p.3, align 4
  %sarg2.3 = zext i32 %sarg2.d.3 to i64
  %sarg3.a.3 = add i64 %addr.z.101, 16
  %sarg3.p.3 = inttoptr i64 %sarg3.a.3 to ptr
  %sarg3.d.3 = load i32, ptr %sarg3.p.3, align 4
  %sarg3.3 = zext i32 %sarg3.d.3 to i64
  %sarg4.a.3 = add i64 %addr.z.101, 20
  %sarg4.p.3 = inttoptr i64 %sarg4.a.3 to ptr
  %sarg4.d.3 = load i32, ptr %sarg4.p.3, align 4
  %sarg4.3 = zext i32 %sarg4.d.3 to i64
  %sarg5.a.3 = add i64 %addr.z.101, 24
  %sarg5.p.3 = inttoptr i64 %sarg5.a.3 to ptr
  %sarg5.d.3 = load i32, ptr %sarg5.p.3, align 4
  %sarg5.3 = zext i32 %sarg5.d.3 to i64
  %sarg6.a.3 = add i64 %addr.z.101, 28
  %sarg6.p.3 = inttoptr i64 %sarg6.a.3 to ptr
  %sarg6.d.3 = load i32, ptr %sarg6.p.3, align 4
  %sarg6.3 = zext i32 %sarg6.d.3 to i64
  %args.p.6 = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr  ; 0x401534
  store i64 %sarg6.3, ptr %args.p.6, align 8  ; 0x401534
  %sarg7.a.3 = add i64 %addr.z.101, 32
  %sarg7.p.3 = inttoptr i64 %sarg7.a.3 to ptr
  %sarg7.d.3 = load i32, ptr %sarg7.p.3, align 4
  %sarg7.3 = zext i32 %sarg7.d.3 to i64
  %args.p.7 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr  ; 0x401534
  store i64 %sarg7.3, ptr %args.p.7, align 8  ; 0x401534
  %ret.4 = call i64 %fp.3(i64 %sarg0.4, i64 %sarg1.3, i64 %sarg2.3, i64 %sarg3.3, i64 %sarg4.3, i64 %sarg5.3, i64 %ecx.z.11, i64 undef)  ; 0x401534
  %pops.3 = call i64 @ra_pops_of(i64 %eh.dtor.4)  ; 0x401534
  br label %eh.destroyed.401470.2
eh.destroyed.401470.2:  ; 0x401534
  %rax.phi.3 = phi i64 [ %rax.phi.2, %eh.state3 ], [ %ret.4, %eh.destroy.401470.2 ]  ; 0x401534
  %eh.head.1.f8.12 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x401534
  store i32 -1, ptr %eh.head.1.f8.12, align 1  ; 0x401534
  call void @ra_eh_resume()  ; 0x401534
  %eh.head.1.f0.4 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 0  ; 0x401534
  %eh.next.6 = load i32, ptr %eh.head.1.f0.4, align 1  ; 0x401534
  %teb.20 = call i64 @ra_teb()  ; 0x401534
  %teb.21 = trunc i64 %teb.20 to i32  ; 0x401534
  %addr.z.104 = zext i32 %teb.21 to i64  ; 0x401534
  %st.p.71 = inttoptr i64 %addr.z.104 to ptr  ; 0x401534
  store i32 %eh.next.6, ptr %st.p.71, align 1  ; 0x401534
  ret i64 %rax.phi.3
eh.catch1_0.s1.1:  ; 0x401534
  %eh.head.f8.8 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x401534
  store i32 0, ptr %eh.head.f8.8, align 1  ; 0x401534
  br label %eh.enter1_0
eh.next1_0.s1.1:  ; 0x401534
  call void @ra_eh_suspend()  ; 0x401534
  %eh.head.f8.9 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x401534
  store i32 -1, ptr %eh.head.f8.9, align 1  ; 0x401534
  call void @ra_eh_resume()  ; 0x401534
  %eh.head.f0.2 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 0  ; 0x401534
  %eh.next.7 = load i32, ptr %eh.head.f0.2, align 1  ; 0x401534
  %teb.22 = call i64 @ra_teb()  ; 0x401534
  %teb.23 = trunc i64 %teb.22 to i32  ; 0x401534
  %addr.z.108 = zext i32 %teb.23 to i64  ; 0x401534
  %st.p.74 = inttoptr i64 %addr.z.108 to ptr  ; 0x401534
  store i32 %eh.next.7, ptr %st.p.74, align 1  ; 0x401534
  ret i64 %ret
eh.state2.1:  ; 0x401534
  %eh.frame.13 = add i32 %eh.reg, 12  ; 0x401534
  %eh.catch.obj.14 = add i32 %eh.frame.13, -28  ; 0x401534
  %eh.catch.obj.15 = zext i32 %eh.catch.obj.14 to i64  ; 0x401534
  %eh.caught.7 = call i64 @ra_eh_catches(i64 7474415530116504195, i64 %eh.catch.obj.15, i64 8200)  ; 0x401534
  %eh.hit.7 = icmp ne i64 %eh.caught.7, 0  ; 0x401534
  br i1 %eh.hit.7, label %eh.catch1_0.s2.1, label %eh.next1_0.s2.1
eh.catch1_0.s2.1:  ; 0x401534
  %eh.esp.saved.6 = call i64 @ra_eh_ending_sp()  ; 0x401534
  %eh.esp.saved.7 = trunc i64 %eh.esp.saved.6 to i32  ; 0x401534
  %eh.esp.p.13 = sub i32 %eh.reg, 4  ; 0x401534
  %addr.z.109 = zext i32 %eh.esp.p.13 to i64  ; 0x401534
  %st.p.75 = inttoptr i64 %addr.z.109 to ptr  ; 0x401534
  store i32 %eh.esp.saved.7, ptr %st.p.75, align 1  ; 0x401534
  %eh.ending.5 = call i64 @ra_eh_ending_object()  ; 0x401534
  %eh.dtor.5 = call i64 @ra_eh_ending_destructor()  ; 0x401534
  call void @ra_eh_leave_catch()  ; 0x401534
  %eh.dies.5 = icmp ne i64 %eh.dtor.5, 0  ; 0x401534
  br i1 %eh.dies.5, label %eh.destroy.401470.3, label %eh.destroyed.401470.3
eh.next1_0.s2.1:  ; 0x401534
  call void @ra_eh_suspend()  ; 0x401534
  %eh.esp.saved.8 = call i64 @ra_eh_ending_sp()  ; 0x401534
  %eh.esp.saved.9 = trunc i64 %eh.esp.saved.8 to i32  ; 0x401534
  %eh.esp.p.14 = sub i32 %eh.reg, 4  ; 0x401534
  %addr.z.112 = zext i32 %eh.esp.p.14 to i64  ; 0x401534
  %st.p.78 = inttoptr i64 %addr.z.112 to ptr  ; 0x401534
  store i32 %eh.esp.saved.9, ptr %st.p.78, align 1  ; 0x401534
  %eh.ending.6 = call i64 @ra_eh_ending_object()  ; 0x401534
  %eh.dtor.6 = call i64 @ra_eh_ending_destructor()  ; 0x401534
  call void @ra_eh_leave_catch()  ; 0x401534
  %eh.dies.6 = icmp ne i64 %eh.dtor.6, 0  ; 0x401534
  br i1 %eh.dies.6, label %eh.destroy.401470.4, label %eh.destroyed.401470.4
eh.destroy.401470.3:  ; 0x401534
  %eh.this.5 = trunc i64 %eh.ending.5 to i32  ; 0x401534
  %ecx.z.12 = zext i32 %eh.this.5 to i64  ; 0x401534
  %sp.27 = sub i32 %sarg0.a, 4  ; 0x401534
  %st.p.76 = getelementptr inbounds i8, ptr %frame.top, i64 -56  ; 0x401534
  store i32 0, ptr %st.p.76, align 1  ; 0x401534
  %fp.2 = inttoptr i64 %eh.dtor.5 to ptr  ; 0x401534
  %sarg0.d.3 = load i32, ptr %st.p.12, align 4
  %sarg0.3 = zext i32 %sarg0.d.3 to i64
  %sarg1.p.2 = getelementptr inbounds i8, ptr %frame.top, i64 -48
  %sarg1.d.2 = load i32, ptr %sarg1.p.2, align 4
  %sarg1.2 = zext i32 %sarg1.d.2 to i64
  %sarg2.p.2 = getelementptr inbounds i8, ptr %frame.top, i64 -44
  %sarg2.d.2 = load i32, ptr %sarg2.p.2, align 4
  %sarg2.2 = zext i32 %sarg2.d.2 to i64
  %sarg3.d.2 = load i32, ptr %st.p.9, align 4
  %sarg3.2 = zext i32 %sarg3.d.2 to i64
  %sarg4.d.2 = load i32, ptr %st.p.11, align 4
  %sarg4.2 = zext i32 %sarg4.d.2 to i64
  %sarg5.d.2 = load i32, ptr %st.p.4, align 4
  %sarg5.2 = zext i32 %sarg5.d.2 to i64
  %sarg6.d.2 = load i32, ptr %st.p.7, align 4
  %sarg6.2 = zext i32 %sarg6.d.2 to i64
  %args.p.4 = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr  ; 0x401534
  store i64 %sarg6.2, ptr %args.p.4, align 8  ; 0x401534
  %sarg7.d.2 = load i32, ptr %st.p.6, align 4
  %sarg7.2 = zext i32 %sarg7.d.2 to i64
  %args.p.5 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr  ; 0x401534
  store i64 %sarg7.2, ptr %args.p.5, align 8  ; 0x401534
  %ret.3 = call i64 %fp.2(i64 %sarg0.3, i64 %sarg1.2, i64 %sarg2.2, i64 %sarg3.2, i64 %sarg4.2, i64 %sarg5.2, i64 %ecx.z.12, i64 undef)  ; 0x401534
  %pops.2 = call i64 @ra_pops_of(i64 %eh.dtor.5)  ; 0x401534
  %n.3 = trunc i64 %pops.2 to i32
  %pops.a.2 = add i32 %n.3, 4  ; 0x401534
  %rsp.53 = add i32 %sp.27, %pops.a.2  ; 0x401534
  br label %eh.destroyed.401470.3
eh.destroyed.401470.3:  ; 0x401534
  %rsp.phi = phi i32 [ %sarg0.a, %eh.catch1_0.s2.1 ], [ %rsp.53, %eh.destroy.401470.3 ]  ; 0x401534
  %eh.head.f8.10 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x401534
  store i32 0, ptr %eh.head.f8.10, align 1  ; 0x401534
  br label %eh.enter1_0
eh.destroy.401470.4:  ; 0x401534
  %eh.this.6 = trunc i64 %eh.ending.6 to i32  ; 0x401534
  %ecx.z.13 = zext i32 %eh.this.6 to i64  ; 0x401534
  %st.p.79 = getelementptr inbounds i8, ptr %frame.top, i64 -56  ; 0x401534
  store i32 0, ptr %st.p.79, align 1  ; 0x401534
  %fp.1 = inttoptr i64 %eh.dtor.6 to ptr  ; 0x401534
  %sarg0.d.2 = load i32, ptr %st.p.12, align 4
  %sarg0.2 = zext i32 %sarg0.d.2 to i64
  %sarg1.p.1 = getelementptr inbounds i8, ptr %frame.top, i64 -48
  %sarg1.d.1 = load i32, ptr %sarg1.p.1, align 4
  %sarg1.1 = zext i32 %sarg1.d.1 to i64
  %sarg2.p.1 = getelementptr inbounds i8, ptr %frame.top, i64 -44
  %sarg2.d.1 = load i32, ptr %sarg2.p.1, align 4
  %sarg2.1 = zext i32 %sarg2.d.1 to i64
  %sarg3.d.1 = load i32, ptr %st.p.9, align 4
  %sarg3.1 = zext i32 %sarg3.d.1 to i64
  %sarg4.d.1 = load i32, ptr %st.p.11, align 4
  %sarg4.1 = zext i32 %sarg4.d.1 to i64
  %sarg5.d.1 = load i32, ptr %st.p.4, align 4
  %sarg5.1 = zext i32 %sarg5.d.1 to i64
  %sarg6.d.1 = load i32, ptr %st.p.7, align 4
  %sarg6.1 = zext i32 %sarg6.d.1 to i64
  %args.p.2 = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr  ; 0x401534
  store i64 %sarg6.1, ptr %args.p.2, align 8  ; 0x401534
  %sarg7.d.1 = load i32, ptr %st.p.6, align 4
  %sarg7.1 = zext i32 %sarg7.d.1 to i64
  %args.p.3 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr  ; 0x401534
  store i64 %sarg7.1, ptr %args.p.3, align 8  ; 0x401534
  %ret.2 = call i64 %fp.1(i64 %sarg0.2, i64 %sarg1.1, i64 %sarg2.1, i64 %sarg3.1, i64 %sarg4.1, i64 %sarg5.1, i64 %ecx.z.13, i64 undef)  ; 0x401534
  %pops.1 = call i64 @ra_pops_of(i64 %eh.dtor.6)  ; 0x401534
  br label %eh.destroyed.401470.4
eh.destroyed.401470.4:  ; 0x401534
  %rax.phi.1 = phi i64 [ %ret, %eh.next1_0.s2.1 ], [ %ret.2, %eh.destroy.401470.4 ]  ; 0x401534
  %eh.head.f8.11 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x401534
  store i32 -1, ptr %eh.head.f8.11, align 1  ; 0x401534
  call void @ra_eh_resume()  ; 0x401534
  %eh.head.f0.3 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 0  ; 0x401534
  %eh.next.8 = load i32, ptr %eh.head.f0.3, align 1  ; 0x401534
  %teb.24 = call i64 @ra_teb()  ; 0x401534
  %teb.25 = trunc i64 %teb.24 to i32  ; 0x401534
  %addr.z.116 = zext i32 %teb.25 to i64  ; 0x401534
  %st.p.81 = inttoptr i64 %addr.z.116 to ptr  ; 0x401534
  store i32 %eh.next.8, ptr %st.p.81, align 1  ; 0x401534
  ret i64 %rax.phi.1
eh.state3.1:  ; 0x401534
  call void @ra_eh_suspend()  ; 0x401534
  %eh.esp.saved.10 = call i64 @ra_eh_ending_sp()  ; 0x401534
  %eh.esp.saved.11 = trunc i64 %eh.esp.saved.10 to i32  ; 0x401534
  %eh.esp.p.15 = sub i32 %eh.reg, 4  ; 0x401534
  %addr.z.117 = zext i32 %eh.esp.p.15 to i64  ; 0x401534
  %st.p.82 = inttoptr i64 %addr.z.117 to ptr  ; 0x401534
  store i32 %eh.esp.saved.11, ptr %st.p.82, align 1  ; 0x401534
  %eh.ending.7 = call i64 @ra_eh_ending_object()  ; 0x401534
  %eh.dtor.7 = call i64 @ra_eh_ending_destructor()  ; 0x401534
  call void @ra_eh_leave_catch()  ; 0x401534
  %eh.dies.7 = icmp ne i64 %eh.dtor.7, 0  ; 0x401534
  br i1 %eh.dies.7, label %eh.destroy.401470.5, label %eh.destroyed.401470.5
eh.destroy.401470.5:  ; 0x401534
  %eh.this.7 = trunc i64 %eh.ending.7 to i32  ; 0x401534
  %ecx.z.14 = zext i32 %eh.this.7 to i64  ; 0x401534
  %st.p.83 = getelementptr inbounds i8, ptr %frame.top, i64 -56  ; 0x401534
  store i32 0, ptr %st.p.83, align 1  ; 0x401534
  %fp = inttoptr i64 %eh.dtor.7 to ptr  ; 0x401534
  %sarg0.d.1 = load i32, ptr %st.p.12, align 4
  %sarg0.1 = zext i32 %sarg0.d.1 to i64
  %sarg1.p = getelementptr inbounds i8, ptr %frame.top, i64 -48
  %sarg1.d = load i32, ptr %sarg1.p, align 4
  %sarg1 = zext i32 %sarg1.d to i64
  %sarg2.p = getelementptr inbounds i8, ptr %frame.top, i64 -44
  %sarg2.d = load i32, ptr %sarg2.p, align 4
  %sarg2 = zext i32 %sarg2.d to i64
  %sarg3.d = load i32, ptr %st.p.9, align 4
  %sarg3 = zext i32 %sarg3.d to i64
  %sarg4.d = load i32, ptr %st.p.11, align 4
  %sarg4 = zext i32 %sarg4.d to i64
  %sarg5.d = load i32, ptr %st.p.4, align 4
  %sarg5 = zext i32 %sarg5.d to i64
  %sarg6.d = load i32, ptr %st.p.7, align 4
  %sarg6 = zext i32 %sarg6.d to i64
  %args.p = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr  ; 0x401534
  store i64 %sarg6, ptr %args.p, align 8  ; 0x401534
  %sarg7.d = load i32, ptr %st.p.6, align 4
  %sarg7 = zext i32 %sarg7.d to i64
  %args.p.1 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr  ; 0x401534
  store i64 %sarg7, ptr %args.p.1, align 8  ; 0x401534
  %ret.1 = call i64 %fp(i64 %sarg0.1, i64 %sarg1, i64 %sarg2, i64 %sarg3, i64 %sarg4, i64 %sarg5, i64 %ecx.z.14, i64 undef)  ; 0x401534
  %pops = call i64 @ra_pops_of(i64 %eh.dtor.7)  ; 0x401534
  br label %eh.destroyed.401470.5
eh.destroyed.401470.5:  ; 0x401534
  %rax.phi = phi i64 [ %ret, %eh.state3.1 ], [ %ret.1, %eh.destroy.401470.5 ]  ; 0x401534
  %eh.head.f8.12 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x401534
  store i32 -1, ptr %eh.head.f8.12, align 1  ; 0x401534
  call void @ra_eh_resume()  ; 0x401534
  %eh.head.f0.4 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 0  ; 0x401534
  %eh.next.9 = load i32, ptr %eh.head.f0.4, align 1  ; 0x401534
  %teb.26 = call i64 @ra_teb()  ; 0x401534
  %teb.27 = trunc i64 %teb.26 to i32  ; 0x401534
  %addr.z.121 = zext i32 %teb.27 to i64  ; 0x401534
  %st.p.85 = inttoptr i64 %addr.z.121 to ptr  ; 0x401534
  store i32 %eh.next.9, ptr %st.p.85, align 1  ; 0x401534
  ret i64 %rax.phi
eh.ok.4014ba:  ; 0x401534
  br label %b_4014c2
}

define i64 @F_00401540(i32 %arg0) {  ; 0x401540
b_401540:  ; 0x401540
  %frame = alloca [4268 x i8], align 16
  %frame.top = getelementptr inbounds i8, ptr %frame, i64 4188
  %rsp0 = ptrtoint ptr %frame.top to i64
  %stack.arg0.p = getelementptr inbounds i8, ptr %frame.top, i64 4
  store i32 %arg0, ptr %stack.arg0.p, align 4
  %ebp = trunc i64 undef to i32  ; 0x401540
  %esp = trunc i64 %rsp0 to i32  ; 0x401540
  %sp = sub i32 %esp, 4  ; 0x401540
  %st.p = getelementptr inbounds i8, ptr %frame.top, i64 -4  ; 0x401540
  store i32 %ebp, ptr %st.p, align 1  ; 0x401540
  %sp.1 = sub i32 %sp, 4  ; 0x401543
  %st.p.1 = getelementptr inbounds i8, ptr %frame.top, i64 -8  ; 0x401543
  store i32 %ebp, ptr %st.p.1, align 1  ; 0x401543
  %sp.2 = sub i32 %sp.1, 4  ; 0x401544
  %st.p.2 = getelementptr inbounds i8, ptr %frame.top, i64 -12  ; 0x401544
  store i32 %ebp, ptr %st.p.2, align 1  ; 0x401544
  %sp.3 = sub i32 %sp.2, 4  ; 0x401545
  %st.p.3 = getelementptr inbounds i8, ptr %frame.top, i64 -16  ; 0x401545
  store i32 %ebp, ptr %st.p.3, align 1  ; 0x401545
  %diff = sub i32 %sp.3, 40  ; 0x401546
  %st.p.4 = getelementptr inbounds i8, ptr %frame.top, i64 -32  ; 0x40154c
  store i32 %diff, ptr %st.p.4, align 1  ; 0x40154c
  %st.p.5 = getelementptr inbounds i8, ptr %frame.top, i64 -20  ; 0x40154f
  store i32 -1, ptr %st.p.5, align 1  ; 0x40154f
  %addr.3 = add i32 %sp, -24  ; 0x401556
  %st.p.6 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x401559
  store i32 ptrtoint (ptr @U_00401c10 to i32), ptr %st.p.6, align 1  ; 0x401559
  %teb = call i64 @ra_teb()  ; 0x401560
  %teb.1 = trunc i64 %teb to i32  ; 0x401560
  %addr.z.8 = zext i32 %teb.1 to i64  ; 0x401560
  %m.p.1 = inttoptr i64 %addr.z.8 to ptr  ; 0x401560
  %m.1 = load i32, ptr %m.p.1, align 1  ; 0x401560
  %st.p.7 = getelementptr inbounds i8, ptr %frame.top, i64 -28  ; 0x401567
  store i32 %m.1, ptr %st.p.7, align 1  ; 0x401567
  %teb.2 = call i64 @ra_teb()  ; 0x40156a
  %teb.3 = trunc i64 %teb.2 to i32  ; 0x40156a
  %addr.z.10 = zext i32 %teb.3 to i64  ; 0x40156a
  %st.p.8 = inttoptr i64 %addr.z.10 to ptr  ; 0x40156a
  store i32 %addr.3, ptr %st.p.8, align 1  ; 0x40156a
  %sp.4 = sub i32 %diff, 4  ; 0x401570
  %st.p.9 = getelementptr inbounds i8, ptr %frame.top, i64 -60  ; 0x401570
  store i32 120, ptr %st.p.9, align 1  ; 0x401570
  %sp.5 = sub i32 %sp.4, 4  ; 0x401572
  %sarg0.a = add i32 %sp.5, 4
  %ret = call i64 @F_00401000(i32 120)  ; 0x401572
  %eh.pending = call i64 @ra_eh_pending()  ; 0x401572
  %eh.thrown = icmp ne i64 %eh.pending, 0  ; 0x401572
  br i1 %eh.thrown, label %eh.landing, label %eh.ok.401572
b_401588:  ; 0x401588
  %sp.220 = sub i32 %sum.83, 4  ; 0x401588
  store i32 221, ptr %st.p.9, align 1  ; 0x401588
  %sp.221 = sub i32 %sp.220, 4  ; 0x40158d
  %sarg0.a.2 = add i32 %sp.221, 4
  %ret.2 = call i64 @F_00401000(i32 221)  ; 0x40158d
  %eh.pending.34 = call i64 @ra_eh_pending()  ; 0x40158d
  %eh.thrown.34 = icmp ne i64 %eh.pending.34, 0  ; 0x40158d
  br i1 %eh.thrown.34, label %eh.landing, label %eh.ok.40158d
b_4015a4:  ; 0x4015a4
  %rax.phi.4 = phi i64 [ %eax.z.41, %b_4015d1 ], [ 0, %eh.ok.40159a ]  ; 0x4015a4
  %rbp.phi.1 = phi i32 [ %sum.87, %b_4015d1 ], [ %sp, %eh.ok.40159a ]  ; 0x4015a4
  %addr.82 = add i32 %rbp.phi.1, -24  ; 0x4015a4
  %addr.z.427 = zext i32 %addr.82 to i64  ; 0x4015a4
  %m.p.34 = inttoptr i64 %addr.z.427 to ptr  ; 0x4015a4
  %m.34 = load i32, ptr %m.p.34, align 1  ; 0x4015a4
  %teb.40 = call i64 @ra_teb()  ; 0x4015a7
  %teb.41 = trunc i64 %teb.40 to i32  ; 0x4015a7
  %addr.z.428 = zext i32 %teb.41 to i64  ; 0x4015a7
  %st.p.301 = inttoptr i64 %addr.z.428 to ptr  ; 0x4015a7
  store i32 %m.34, ptr %st.p.301, align 1  ; 0x4015a7
  ret i64 %rax.phi.4
b_4015b6:  ; 0x4015b6
  %addr.83 = add i32 %sp, -48  ; 0x4015b6
  %addr.z.434 = zext i32 %addr.83 to i64  ; 0x4015b6
  %st.p.302 = getelementptr inbounds i8, ptr %frame.top, i64 -52  ; 0x4015b6
  store i32 %arg0, ptr %st.p.302, align 1  ; 0x4015b6
  store i32 3, ptr %st.p.5, align 1  ; 0x4015b9
  %sp.229 = sub i32 %sum.83, 4  ; 0x4015c3
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 2916) to i32), ptr %st.p.9, align 1  ; 0x4015c3
  %sp.230 = sub i32 %sp.229, 4  ; 0x4015c8
  %st.p.305 = getelementptr inbounds i8, ptr %frame.top, i64 -64  ; 0x4015c8
  store i32 %addr.83, ptr %st.p.305, align 1  ; 0x4015c8
  call void @ra_eh_throw(i64 %addr.z.434, i64 4)  ; 0x4015c9
  call void @ra_eh_throw_type(i64 565891180120823400, i64 0, i64 4)  ; 0x4015c9
  br label %eh.landing
b_4015d1:  ; 0x4015d1
  %rbp.phi = phi i32 [ %eh.frame.2, %eh.destroyed.401660 ], [ %eh.frame.7, %eh.destroyed.401660.1 ]  ; 0x4015d1
  %sum.87 = add i32 %rbp.phi, 12  ; 0x4015d1
  %addr.86 = add i32 %sum.87, -32  ; 0x4015d4
  %addr.z.439 = zext i32 %addr.86 to i64  ; 0x4015d4
  %m.p.35 = inttoptr i64 %addr.z.439 to ptr  ; 0x4015d4
  %m.35 = load i32, ptr %m.p.35, align 1  ; 0x4015d4
  %eax.z.41 = zext i32 %m.35 to i64  ; 0x4015d4
  br label %b_4015a4
eh.landing:  ; 0x401572
  %rax.phi = phi i64 [ %ret, %b_401540 ], [ %ret.2, %b_401588 ], [ %addr.z.434, %b_4015b6 ], [ %ret.1, %eh.ok.401572 ], [ %ret.3, %eh.ok.40158d ]  ; 0x401572
  %rsp.phi = phi i32 [ %sarg0.a, %b_401540 ], [ %sarg0.a.2, %b_401588 ], [ %sp.230, %b_4015b6 ], [ %sarg0.a.1, %eh.ok.401572 ], [ %sarg0.a.3, %eh.ok.40158d ]  ; 0x401572
  %teb.4 = call i64 @ra_teb()  ; 0x401572
  %teb.5 = trunc i64 %teb.4 to i32  ; 0x401572
  %addr.z.13 = zext i32 %teb.5 to i64  ; 0x401572
  %eh.head.p = inttoptr i64 %addr.z.13 to ptr  ; 0x401572
  %eh.head = load i32, ptr %eh.head.p, align 1  ; 0x401572
  %eh.head.s.w = zext i32 %eh.head to i64  ; 0x401572
  %eh.head.s = inttoptr i64 %eh.head.s.w to ptr  ; 0x401572
  %eh.frame = sub i32 %eh.head, -24  ; 0x401572
  %eh.reg = add i32 %eh.frame, -24  ; 0x401572
  %eh.head.f4 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 1  ; 0x401572
  %eh.handler = load i32, ptr %eh.head.f4, align 1  ; 0x401572
  %eh.ours = icmp eq i32 %eh.handler, 4201488  ; 0x401572
  %eh.ours.1 = icmp eq i32 %eh.handler, ptrtoint (ptr @U_00401c10 to i32)  ; 0x401572
  %eh.ours.2 = or i1 %eh.ours, %eh.ours.1  ; 0x401572
  br i1 %eh.ours.2, label %eh.dispatch, label %eh.foreign
eh.dispatch:  ; 0x401572
  %eh.head.f8 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x401572
  %eh.state = load i32, ptr %eh.head.f8, align 1  ; 0x401572
  switch i32 %eh.state, label %eh.state.bad [ i32 -1, label %eh.state_none i32 0, label %eh.state0 i32 1, label %eh.state1 i32 2, label %eh.state2.1 i32 3, label %eh.state3.1 i32 4, label %eh.state4.1 i32 5, label %eh.state5.1 i32 6, label %eh.state6.1 ]
eh.foreign:  ; 0x401572
  call void @ra_trap(i64 4199744, i64 8)  ; 0x401572
  unreachable
eh.state.bad:  ; 0x401572
  call void @ra_trap(i64 4199744, i64 7)  ; 0x401572
  unreachable
eh.state_none:  ; 0x401572
  call void @ra_eh_suspend()  ; 0x401572
  %eh.head.f8.1 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x401572
  store i32 -1, ptr %eh.head.f8.1, align 1  ; 0x401572
  call void @ra_eh_resume()  ; 0x401572
  %eh.head.f0 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 0  ; 0x401572
  %eh.next = load i32, ptr %eh.head.f0, align 1  ; 0x401572
  %teb.6 = call i64 @ra_teb()  ; 0x401572
  %teb.7 = trunc i64 %teb.6 to i32  ; 0x401572
  %addr.z.18 = zext i32 %teb.7 to i64  ; 0x401572
  %st.p.12 = inttoptr i64 %addr.z.18 to ptr  ; 0x401572
  store i32 %eh.next, ptr %st.p.12, align 1  ; 0x401572
  ret i64 %rax.phi
eh.state0:  ; 0x401572
  %eh.frame.1 = add i32 %eh.reg, 12  ; 0x401572
  %eh.catch.obj = add i32 %eh.frame.1, -40  ; 0x401572
  %eh.catch.obj.1 = zext i32 %eh.catch.obj to i64  ; 0x401572
  %eh.caught = call i64 @ra_eh_catches(i64 565892279632451611, i64 %eh.catch.obj.1, i64 8192)  ; 0x401572
  %eh.hit = icmp ne i64 %eh.caught, 0  ; 0x401572
  br i1 %eh.hit, label %eh.catch1_0.s0, label %eh.next1_0.s0
eh.catch1_0.s0:  ; 0x401572
  %eh.head.f8.2 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x401572
  store i32 0, ptr %eh.head.f8.2, align 1  ; 0x401572
  br label %eh.enter1_0
eh.next1_0.s0:  ; 0x401572
  call void @ra_eh_suspend()  ; 0x401660
  %eh.head.f8.3 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x401660
  store i32 -1, ptr %eh.head.f8.3, align 1  ; 0x401660
  call void @ra_eh_resume()  ; 0x401660
  %eh.head.f0.1 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 0  ; 0x401660
  %eh.next.1 = load i32, ptr %eh.head.f0.1, align 1  ; 0x401660
  %teb.8 = call i64 @ra_teb()  ; 0x401660
  %teb.9 = trunc i64 %teb.8 to i32  ; 0x401660
  %addr.z.34 = zext i32 %teb.9 to i64  ; 0x401660
  %st.p.22 = inttoptr i64 %addr.z.34 to ptr  ; 0x401660
  store i32 %eh.next.1, ptr %st.p.22, align 1  ; 0x401660
  ret i64 %rax.phi
eh.enter1_0:  ; 0x401572
  %rsp.phi.13 = phi i32 [ %rsp.phi, %eh.catch1_0.s0 ], [ %sp.122, %eh.cleaned.401620 ], [ %sp.140, %eh.cleaned.401620.12 ], [ %sp.164, %eh.cleaned.401620.14 ], [ %sp.183, %eh.cleaned.401620.16 ], [ %sp.203, %eh.cleaned.401620.18 ]  ; 0x401572
  call void @ra_eh_enter_catch()  ; 0x401572
  %eh.head.f8.4 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x401572
  store i32 6, ptr %eh.head.f8.4, align 1  ; 0x401572
  %eh.esp.p = sub i32 %eh.reg, 4  ; 0x401572
  %addr.z.21 = zext i32 %eh.esp.p to i64  ; 0x401572
  %eh.esp.p.1 = inttoptr i64 %addr.z.21 to ptr  ; 0x401572
  %eh.esp = load i32, ptr %eh.esp.p.1, align 1  ; 0x401572
  %eh.esp.1 = zext i32 %eh.esp to i64  ; 0x401572
  call void @ra_eh_catch_sp(i64 %eh.esp.1)  ; 0x401572
  %eh.frame.2 = add i32 %eh.reg, 12  ; 0x401572
  %sp.6 = sub i32 %rsp.phi.13, 4  ; 0x401572
  %addr.z.22 = zext i32 %sp.6 to i64  ; 0x401572
  %st.p.15 = inttoptr i64 %addr.z.22 to ptr  ; 0x401572
  store i32 0, ptr %st.p.15, align 1  ; 0x401572
  br label %f1_401640
f1_401640:  ; 0x401640
  %sp.7 = sub i32 %sp.6, 4  ; 0x401640
  %addr.z.23 = zext i32 %sp.7 to i64  ; 0x401640
  %st.p.16 = inttoptr i64 %addr.z.23 to ptr  ; 0x401640
  store i32 %eh.frame.2, ptr %st.p.16, align 1  ; 0x401640
  %diff.1 = sub i32 %sp.7, 8  ; 0x401641
  %sum = add i32 %eh.frame.2, 12  ; 0x401644
  %addr.6 = add i32 %sum, -28  ; 0x401647
  %addr.z.24 = zext i32 %addr.6 to i64  ; 0x401647
  %st.p.17 = inttoptr i64 %addr.z.24 to ptr  ; 0x401647
  store i32 %diff.1, ptr %st.p.17, align 1  ; 0x401647
  %addr.7 = add i32 %sum, -32  ; 0x40164a
  %addr.z.25 = zext i32 %addr.7 to i64  ; 0x40164a
  %m.p.2 = inttoptr i64 %addr.z.25 to ptr  ; 0x40164a
  %m.2 = load i32, ptr %m.p.2, align 1  ; 0x40164a
  %product = mul i32 %m.2, 1000  ; 0x40164a
  %addr.8 = add i32 %sum, -52  ; 0x401651
  %addr.z.26 = zext i32 %addr.8 to i64  ; 0x401651
  %m.p.3 = inttoptr i64 %addr.z.26 to ptr  ; 0x401651
  %m.3 = load i32, ptr %m.p.3, align 1  ; 0x401651
  %sum.1 = add i32 %product, %m.3  ; 0x401651
  store i32 %sum.1, ptr %m.p.2, align 1  ; 0x401654
  %sum.2 = add i32 %diff.1, 8  ; 0x40165c
  %sp.8 = add i32 %sum.2, 4  ; 0x40165f
  %sp.9 = add i32 %sp.8, 4  ; 0x401660
  %eh.ending = call i64 @ra_eh_ending_object()  ; 0x401660
  %eh.dtor = call i64 @ra_eh_ending_destructor()  ; 0x401660
  call void @ra_eh_leave_catch()  ; 0x401660
  %eh.dies = icmp ne i64 %eh.dtor, 0  ; 0x401660
  br i1 %eh.dies, label %eh.destroy.401660, label %eh.destroyed.401660
eh.destroy.401660:  ; 0x401660
  %eh.this = trunc i64 %eh.ending to i32  ; 0x401660
  %ecx.z.1 = zext i32 %eh.this to i64  ; 0x401660
  %sp.10 = sub i32 %sp.9, 4  ; 0x401660
  %addr.z.30 = zext i32 %sp.10 to i64  ; 0x401660
  %st.p.19 = inttoptr i64 %addr.z.30 to ptr  ; 0x401660
  store i32 0, ptr %st.p.19, align 1  ; 0x401660
  %fp.11 = inttoptr i64 %eh.dtor to ptr  ; 0x401660
  %sarg0.a.47 = add i64 %addr.z.30, 4
  %sarg0.p.47 = inttoptr i64 %sarg0.a.47 to ptr
  %sarg0.d.47 = load i32, ptr %sarg0.p.47, align 4
  %sarg0.47 = zext i32 %sarg0.d.47 to i64
  %sarg1.a.11 = add i64 %addr.z.30, 8
  %sarg1.p.11 = inttoptr i64 %sarg1.a.11 to ptr
  %sarg1.d.11 = load i32, ptr %sarg1.p.11, align 4
  %sarg1.11 = zext i32 %sarg1.d.11 to i64
  %sarg2.a.11 = add i64 %addr.z.30, 12
  %sarg2.p.11 = inttoptr i64 %sarg2.a.11 to ptr
  %sarg2.d.11 = load i32, ptr %sarg2.p.11, align 4
  %sarg2.11 = zext i32 %sarg2.d.11 to i64
  %sarg3.a.11 = add i64 %addr.z.30, 16
  %sarg3.p.11 = inttoptr i64 %sarg3.a.11 to ptr
  %sarg3.d.11 = load i32, ptr %sarg3.p.11, align 4
  %sarg3.11 = zext i32 %sarg3.d.11 to i64
  %sarg4.a.11 = add i64 %addr.z.30, 20
  %sarg4.p.11 = inttoptr i64 %sarg4.a.11 to ptr
  %sarg4.d.11 = load i32, ptr %sarg4.p.11, align 4
  %sarg4.11 = zext i32 %sarg4.d.11 to i64
  %sarg5.a.11 = add i64 %addr.z.30, 24
  %sarg5.p.11 = inttoptr i64 %sarg5.a.11 to ptr
  %sarg5.d.11 = load i32, ptr %sarg5.p.11, align 4
  %sarg5.11 = zext i32 %sarg5.d.11 to i64
  %sarg6.a.11 = add i64 %addr.z.30, 28
  %sarg6.p.11 = inttoptr i64 %sarg6.a.11 to ptr
  %sarg6.d.11 = load i32, ptr %sarg6.p.11, align 4
  %sarg6.11 = zext i32 %sarg6.d.11 to i64
  %args.p.22 = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr  ; 0x401660
  store i64 %sarg6.11, ptr %args.p.22, align 8  ; 0x401660
  %sarg7.a.11 = add i64 %addr.z.30, 32
  %sarg7.p.11 = inttoptr i64 %sarg7.a.11 to ptr
  %sarg7.d.11 = load i32, ptr %sarg7.p.11, align 4
  %sarg7.11 = zext i32 %sarg7.d.11 to i64
  %args.p.23 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr  ; 0x401660
  store i64 %sarg7.11, ptr %args.p.23, align 8  ; 0x401660
  %ret.47 = call i64 %fp.11(i64 %sarg0.47, i64 %sarg1.11, i64 %sarg2.11, i64 %sarg3.11, i64 %sarg4.11, i64 %sarg5.11, i64 %ecx.z.1, i64 undef)  ; 0x401660
  %pops.11 = call i64 @ra_pops_of(i64 %eh.dtor)  ; 0x401660
  br label %eh.destroyed.401660
eh.destroyed.401660:  ; 0x401660
  store i32 %eh.esp, ptr %eh.esp.p.1, align 1  ; 0x401660
  br label %b_4015d1
eh.state1:  ; 0x401660
  %eh.frame.3 = add i32 %eh.reg, 12  ; 0x401660
  %eh.catch.obj.2 = add i32 %eh.frame.3, -40  ; 0x401660
  %eh.catch.obj.3 = zext i32 %eh.catch.obj.2 to i64  ; 0x401660
  %eh.caught.1 = call i64 @ra_eh_catches(i64 565892279632451611, i64 %eh.catch.obj.3, i64 8192)  ; 0x401660
  %eh.hit.1 = icmp ne i64 %eh.caught.1, 0  ; 0x401660
  br i1 %eh.hit.1, label %eh.catch1_0.s1, label %eh.next1_0.s1
eh.catch1_0.s1:  ; 0x401660
  %eh.head.f8.5 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x401660
  store i32 0, ptr %eh.head.f8.5, align 1  ; 0x401660
  %sp.11 = sub i32 %rsp.phi, 4  ; 0x401660
  %addr.z.36 = zext i32 %sp.11 to i64  ; 0x401660
  %st.p.24 = inttoptr i64 %addr.z.36 to ptr  ; 0x401660
  store i32 0, ptr %st.p.24, align 1  ; 0x401660
  br label %f2_401620
eh.next1_0.s1:  ; 0x401660
  call void @ra_eh_suspend()  ; 0x40163b
  %eh.head.f8.6 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x40163b
  store i32 0, ptr %eh.head.f8.6, align 1  ; 0x40163b
  %sp.123 = sub i32 %rsp.phi, 4  ; 0x40163b
  %addr.z.250 = zext i32 %sp.123 to i64  ; 0x40163b
  %st.p.173 = inttoptr i64 %addr.z.250 to ptr  ; 0x40163b
  store i32 0, ptr %st.p.173, align 1  ; 0x40163b
  br label %f20_401620
eh.cleaned.401620:  ; 0x401660
  %eh.head.f8.7 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x40163b
  store i32 0, ptr %eh.head.f8.7, align 1  ; 0x40163b
  br label %eh.enter1_0
f2_401620:  ; 0x401620
  %sp.12 = sub i32 %sp.11, 4  ; 0x401620
  %addr.z.37 = zext i32 %sp.12 to i64  ; 0x401620
  %st.p.25 = inttoptr i64 %addr.z.37 to ptr  ; 0x401620
  store i32 %eh.frame.3, ptr %st.p.25, align 1  ; 0x401620
  %diff.2 = sub i32 %sp.12, 8  ; 0x401621
  %sum.3 = add i32 %eh.frame.3, 12  ; 0x401624
  %addr.10 = add i32 %sum.3, -36  ; 0x401627
  %addr.z.38 = zext i32 %addr.10 to i64  ; 0x401627
  %m.p.4 = inttoptr i64 %addr.z.38 to ptr  ; 0x401627
  %m.4 = load i32, ptr %m.p.4, align 1  ; 0x401627
  %addr.11 = add i32 %sum.3, -32  ; 0x40162a
  %addr.z.39 = zext i32 %addr.11 to i64  ; 0x40162a
  %st.p.26 = inttoptr i64 %addr.z.39 to ptr  ; 0x40162a
  store i32 %m.4, ptr %st.p.26, align 1  ; 0x40162a
  %sp.13 = sub i32 %diff.2, 4  ; 0x40162d
  %addr.z.40 = zext i32 %sp.13 to i64  ; 0x40162d
  %st.p.27 = inttoptr i64 %addr.z.40 to ptr  ; 0x40162d
  store i32 220, ptr %st.p.27, align 1  ; 0x40162d
  %sp.14 = sub i32 %sp.13, 4  ; 0x401632
  %sarg0.a.24 = add i32 %sp.14, 4
  %ret.24 = call i64 @F_00401000(i32 220)  ; 0x401632
  %eh.pending.1 = call i64 @ra_eh_pending()  ; 0x401632
  %eh.thrown.1 = icmp ne i64 %eh.pending.1, 0  ; 0x401632
  br i1 %eh.thrown.1, label %eh.landing.1, label %eh.ok.401632.10
eh.landing.1:  ; 0x401632
  %rax.phi.2 = phi i64 [ %ret.24, %f2_401620 ], [ %ret.45, %f4_401620 ], [ %ret.44, %f5_401620 ], [ %ret.43, %f6_401670 ], [ %addr.z.97, %eh.ok.401682 ], [ %ret.42, %f7_401620 ], [ %ret.41, %f8_401620 ], [ %ret.40, %f9_4015e0 ], [ %ret.38, %f10_4015e0 ], [ %ret.39, %f11_401620 ], [ %ret.36, %f12_4015e0 ], [ %ret.37, %f13_401620 ], [ %ret.35, %f14_401620 ], [ %ret.33, %f15_401620 ], [ %ret.29, %f16_401600 ], [ %ret.31, %f17_401620 ], [ %ret.26, %f18_401600 ], [ %ret.28, %f19_401620 ], [ %ret.23, %f20_401620 ], [ %ret.22, %f21_401670 ], [ %addr.z.272, %eh.ok.401682.1 ], [ %ret.21, %f22_401620 ], [ %ret.20, %f23_401620 ], [ %ret.19, %f24_4015e0 ], [ %ret.17, %f25_4015e0 ], [ %ret.18, %f26_401620 ], [ %ret.15, %f27_4015e0 ], [ %ret.16, %f28_401620 ], [ %ret.14, %f29_401620 ], [ %ret.12, %f30_401620 ], [ %ret.8, %f31_401600 ], [ %ret.10, %f32_401620 ], [ %ret.5, %f33_401600 ], [ %ret.7, %f34_401620 ]  ; 0x401632
  %rsp.phi.6 = phi i32 [ %sarg0.a.24, %f2_401620 ], [ %sarg0.a.45, %f4_401620 ], [ %sarg0.a.44, %f5_401620 ], [ %sarg0.a.43, %f6_401670 ], [ %sp.37, %eh.ok.401682 ], [ %sarg0.a.42, %f7_401620 ], [ %sarg0.a.41, %f8_401620 ], [ %sarg0.a.40, %f9_4015e0 ], [ %sarg0.a.38, %f10_4015e0 ], [ %sarg0.a.39, %f11_401620 ], [ %sarg0.a.36, %f12_4015e0 ], [ %sarg0.a.37, %f13_401620 ], [ %sarg0.a.35, %f14_401620 ], [ %sarg0.a.33, %f15_401620 ], [ %sarg0.a.29, %f16_401600 ], [ %sarg0.a.31, %f17_401620 ], [ %sarg0.a.26, %f18_401600 ], [ %sarg0.a.28, %f19_401620 ], [ %sarg0.a.23, %f20_401620 ], [ %sarg0.a.22, %f21_401670 ], [ %sp.134, %eh.ok.401682.1 ], [ %sarg0.a.21, %f22_401620 ], [ %sarg0.a.20, %f23_401620 ], [ %sarg0.a.19, %f24_4015e0 ], [ %sarg0.a.17, %f25_4015e0 ], [ %sarg0.a.18, %f26_401620 ], [ %sarg0.a.15, %f27_4015e0 ], [ %sarg0.a.16, %f28_401620 ], [ %sarg0.a.14, %f29_401620 ], [ %sarg0.a.12, %f30_401620 ], [ %sarg0.a.8, %f31_401600 ], [ %sarg0.a.10, %f32_401620 ], [ %sarg0.a.5, %f33_401600 ], [ %sarg0.a.7, %f34_401620 ]  ; 0x401632
  %teb.10 = call i64 @ra_teb()  ; 0x401632
  %teb.11 = trunc i64 %teb.10 to i32  ; 0x401632
  %addr.z.42 = zext i32 %teb.11 to i64  ; 0x401632
  %eh.head.p.1 = inttoptr i64 %addr.z.42 to ptr  ; 0x401632
  %eh.head.1 = load i32, ptr %eh.head.p.1, align 1  ; 0x401632
  %eh.head.1.s.w = zext i32 %eh.head.1 to i64  ; 0x401632
  %eh.head.1.s = inttoptr i64 %eh.head.1.s.w to ptr  ; 0x401632
  %eh.frame.5 = sub i32 %eh.head.1, -24  ; 0x401632
  %eh.reg.1 = add i32 %eh.frame.5, -24  ; 0x401632
  %eh.head.1.f4 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 1  ; 0x401632
  %eh.handler.1 = load i32, ptr %eh.head.1.f4, align 1  ; 0x401632
  %eh.ours.3 = icmp eq i32 %eh.handler.1, 4201488  ; 0x401632
  %eh.ours.4 = icmp eq i32 %eh.handler.1, ptrtoint (ptr @U_00401c10 to i32)  ; 0x401632
  %eh.ours.5 = or i1 %eh.ours.3, %eh.ours.4  ; 0x401632
  br i1 %eh.ours.5, label %eh.dispatch.1, label %eh.foreign.1
eh.dispatch.1:  ; 0x401632
  %eh.head.1.f8 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x401632
  %eh.state.1 = load i32, ptr %eh.head.1.f8, align 1  ; 0x401632
  switch i32 %eh.state.1, label %eh.state.bad.1 [ i32 -1, label %eh.state_none.1 i32 0, label %eh.state0.1 i32 1, label %eh.state1.1 i32 2, label %eh.state2 i32 3, label %eh.state3 i32 4, label %eh.state4 i32 5, label %eh.state5 i32 6, label %eh.state6 ]
eh.foreign.1:  ; 0x401632
  call void @ra_trap(i64 4199744, i64 8)  ; 0x401632
  unreachable
eh.state.bad.1:  ; 0x401632
  call void @ra_trap(i64 4199744, i64 7)  ; 0x401632
  unreachable
eh.state_none.1:  ; 0x401632
  call void @ra_eh_suspend()  ; 0x401632
  %eh.head.1.f8.1 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x401632
  store i32 -1, ptr %eh.head.1.f8.1, align 1  ; 0x401632
  call void @ra_eh_resume()  ; 0x401632
  %eh.head.1.f0 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 0  ; 0x401632
  %eh.next.2 = load i32, ptr %eh.head.1.f0, align 1  ; 0x401632
  %teb.12 = call i64 @ra_teb()  ; 0x401632
  %teb.13 = trunc i64 %teb.12 to i32  ; 0x401632
  %addr.z.47 = zext i32 %teb.13 to i64  ; 0x401632
  %st.p.30 = inttoptr i64 %addr.z.47 to ptr  ; 0x401632
  store i32 %eh.next.2, ptr %st.p.30, align 1  ; 0x401632
  ret i64 %rax.phi.2
eh.state0.1:  ; 0x401632
  %eh.frame.6 = add i32 %eh.reg.1, 12  ; 0x401632
  %eh.catch.obj.4 = add i32 %eh.frame.6, -40  ; 0x401632
  %eh.catch.obj.5 = zext i32 %eh.catch.obj.4 to i64  ; 0x401632
  %eh.caught.2 = call i64 @ra_eh_catches(i64 565892279632451611, i64 %eh.catch.obj.5, i64 8192)  ; 0x401632
  %eh.hit.2 = icmp ne i64 %eh.caught.2, 0  ; 0x401632
  br i1 %eh.hit.2, label %eh.catch1_0.s0.1, label %eh.next1_0.s0.1
eh.catch1_0.s0.1:  ; 0x401632
  %eh.head.1.f8.2 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x401632
  store i32 0, ptr %eh.head.1.f8.2, align 1  ; 0x401632
  br label %eh.enter1_0.1
eh.next1_0.s0.1:  ; 0x401632
  call void @ra_eh_suspend()  ; 0x401660
  %eh.head.1.f8.3 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x401660
  store i32 -1, ptr %eh.head.1.f8.3, align 1  ; 0x401660
  call void @ra_eh_resume()  ; 0x401660
  %eh.head.1.f0.1 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 0  ; 0x401660
  %eh.next.3 = load i32, ptr %eh.head.1.f0.1, align 1  ; 0x401660
  %teb.14 = call i64 @ra_teb()  ; 0x401660
  %teb.15 = trunc i64 %teb.14 to i32  ; 0x401660
  %addr.z.63 = zext i32 %teb.15 to i64  ; 0x401660
  %st.p.40 = inttoptr i64 %addr.z.63 to ptr  ; 0x401660
  store i32 %eh.next.3, ptr %st.p.40, align 1  ; 0x401660
  ret i64 %rax.phi.2
eh.enter1_0.1:  ; 0x401632
  %rsp.phi.12 = phi i32 [ %rsp.phi.6, %eh.catch1_0.s0.1 ], [ %sp.25, %eh.cleaned.401620.1 ], [ %sp.43, %eh.cleaned.401620.3 ], [ %sp.67, %eh.cleaned.401620.5 ], [ %sp.86, %eh.cleaned.401620.7 ], [ %sp.106, %eh.cleaned.401620.9 ]  ; 0x401632
  call void @ra_eh_enter_catch()  ; 0x401632
  %eh.head.1.f8.4 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x401632
  store i32 6, ptr %eh.head.1.f8.4, align 1  ; 0x401632
  %eh.esp.p.3 = sub i32 %eh.reg.1, 4  ; 0x401632
  %addr.z.50 = zext i32 %eh.esp.p.3 to i64  ; 0x401632
  %eh.esp.p.4 = inttoptr i64 %addr.z.50 to ptr  ; 0x401632
  %eh.esp.2 = load i32, ptr %eh.esp.p.4, align 1  ; 0x401632
  %eh.esp.3 = zext i32 %eh.esp.2 to i64  ; 0x401632
  call void @ra_eh_catch_sp(i64 %eh.esp.3)  ; 0x401632
  %eh.frame.7 = add i32 %eh.reg.1, 12  ; 0x401632
  %sp.15 = sub i32 %rsp.phi.12, 4  ; 0x401632
  %addr.z.51 = zext i32 %sp.15 to i64  ; 0x401632
  %st.p.33 = inttoptr i64 %addr.z.51 to ptr  ; 0x401632
  store i32 0, ptr %st.p.33, align 1  ; 0x401632
  br label %f3_401640
f3_401640:  ; 0x401640
  %sp.16 = sub i32 %sp.15, 4  ; 0x401640
  %addr.z.52 = zext i32 %sp.16 to i64  ; 0x401640
  %st.p.34 = inttoptr i64 %addr.z.52 to ptr  ; 0x401640
  store i32 %eh.frame.7, ptr %st.p.34, align 1  ; 0x401640
  %diff.3 = sub i32 %sp.16, 8  ; 0x401641
  %sum.4 = add i32 %eh.frame.7, 12  ; 0x401644
  %addr.12 = add i32 %sum.4, -28  ; 0x401647
  %addr.z.53 = zext i32 %addr.12 to i64  ; 0x401647
  %st.p.35 = inttoptr i64 %addr.z.53 to ptr  ; 0x401647
  store i32 %diff.3, ptr %st.p.35, align 1  ; 0x401647
  %addr.13 = add i32 %sum.4, -32  ; 0x40164a
  %addr.z.54 = zext i32 %addr.13 to i64  ; 0x40164a
  %m.p.5 = inttoptr i64 %addr.z.54 to ptr  ; 0x40164a
  %m.5 = load i32, ptr %m.p.5, align 1  ; 0x40164a
  %product.1 = mul i32 %m.5, 1000  ; 0x40164a
  %addr.14 = add i32 %sum.4, -52  ; 0x401651
  %addr.z.55 = zext i32 %addr.14 to i64  ; 0x401651
  %m.p.6 = inttoptr i64 %addr.z.55 to ptr  ; 0x401651
  %m.6 = load i32, ptr %m.p.6, align 1  ; 0x401651
  %sum.5 = add i32 %product.1, %m.6  ; 0x401651
  store i32 %sum.5, ptr %m.p.5, align 1  ; 0x401654
  %sum.6 = add i32 %diff.3, 8  ; 0x40165c
  %sp.17 = add i32 %sum.6, 4  ; 0x40165f
  %sp.18 = add i32 %sp.17, 4  ; 0x401660
  %eh.ending.1 = call i64 @ra_eh_ending_object()  ; 0x401660
  %eh.dtor.1 = call i64 @ra_eh_ending_destructor()  ; 0x401660
  call void @ra_eh_leave_catch()  ; 0x401660
  %eh.dies.1 = icmp ne i64 %eh.dtor.1, 0  ; 0x401660
  br i1 %eh.dies.1, label %eh.destroy.401660.1, label %eh.destroyed.401660.1
eh.destroy.401660.1:  ; 0x401660
  %eh.this.1 = trunc i64 %eh.ending.1 to i32  ; 0x401660
  %ecx.z.2 = zext i32 %eh.this.1 to i64  ; 0x401660
  %sp.19 = sub i32 %sp.18, 4  ; 0x401660
  %addr.z.59 = zext i32 %sp.19 to i64  ; 0x401660
  %st.p.37 = inttoptr i64 %addr.z.59 to ptr  ; 0x401660
  store i32 0, ptr %st.p.37, align 1  ; 0x401660
  %fp.10 = inttoptr i64 %eh.dtor.1 to ptr  ; 0x401660
  %sarg0.a.46 = add i64 %addr.z.59, 4
  %sarg0.p.46 = inttoptr i64 %sarg0.a.46 to ptr
  %sarg0.d.46 = load i32, ptr %sarg0.p.46, align 4
  %sarg0.46 = zext i32 %sarg0.d.46 to i64
  %sarg1.a.10 = add i64 %addr.z.59, 8
  %sarg1.p.10 = inttoptr i64 %sarg1.a.10 to ptr
  %sarg1.d.10 = load i32, ptr %sarg1.p.10, align 4
  %sarg1.10 = zext i32 %sarg1.d.10 to i64
  %sarg2.a.10 = add i64 %addr.z.59, 12
  %sarg2.p.10 = inttoptr i64 %sarg2.a.10 to ptr
  %sarg2.d.10 = load i32, ptr %sarg2.p.10, align 4
  %sarg2.10 = zext i32 %sarg2.d.10 to i64
  %sarg3.a.10 = add i64 %addr.z.59, 16
  %sarg3.p.10 = inttoptr i64 %sarg3.a.10 to ptr
  %sarg3.d.10 = load i32, ptr %sarg3.p.10, align 4
  %sarg3.10 = zext i32 %sarg3.d.10 to i64
  %sarg4.a.10 = add i64 %addr.z.59, 20
  %sarg4.p.10 = inttoptr i64 %sarg4.a.10 to ptr
  %sarg4.d.10 = load i32, ptr %sarg4.p.10, align 4
  %sarg4.10 = zext i32 %sarg4.d.10 to i64
  %sarg5.a.10 = add i64 %addr.z.59, 24
  %sarg5.p.10 = inttoptr i64 %sarg5.a.10 to ptr
  %sarg5.d.10 = load i32, ptr %sarg5.p.10, align 4
  %sarg5.10 = zext i32 %sarg5.d.10 to i64
  %sarg6.a.10 = add i64 %addr.z.59, 28
  %sarg6.p.10 = inttoptr i64 %sarg6.a.10 to ptr
  %sarg6.d.10 = load i32, ptr %sarg6.p.10, align 4
  %sarg6.10 = zext i32 %sarg6.d.10 to i64
  %args.p.20 = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr  ; 0x401660
  store i64 %sarg6.10, ptr %args.p.20, align 8  ; 0x401660
  %sarg7.a.10 = add i64 %addr.z.59, 32
  %sarg7.p.10 = inttoptr i64 %sarg7.a.10 to ptr
  %sarg7.d.10 = load i32, ptr %sarg7.p.10, align 4
  %sarg7.10 = zext i32 %sarg7.d.10 to i64
  %args.p.21 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr  ; 0x401660
  store i64 %sarg7.10, ptr %args.p.21, align 8  ; 0x401660
  %ret.46 = call i64 %fp.10(i64 %sarg0.46, i64 %sarg1.10, i64 %sarg2.10, i64 %sarg3.10, i64 %sarg4.10, i64 %sarg5.10, i64 %ecx.z.2, i64 undef)  ; 0x401660
  %pops.10 = call i64 @ra_pops_of(i64 %eh.dtor.1)  ; 0x401660
  br label %eh.destroyed.401660.1
eh.destroyed.401660.1:  ; 0x401660
  store i32 %eh.esp.2, ptr %eh.esp.p.4, align 1  ; 0x401660
  br label %b_4015d1
eh.state1.1:  ; 0x401660
  %eh.frame.8 = add i32 %eh.reg.1, 12  ; 0x401660
  %eh.catch.obj.6 = add i32 %eh.frame.8, -40  ; 0x401660
  %eh.catch.obj.7 = zext i32 %eh.catch.obj.6 to i64  ; 0x401660
  %eh.caught.3 = call i64 @ra_eh_catches(i64 565892279632451611, i64 %eh.catch.obj.7, i64 8192)  ; 0x401660
  %eh.hit.3 = icmp ne i64 %eh.caught.3, 0  ; 0x401660
  br i1 %eh.hit.3, label %eh.catch1_0.s1.1, label %eh.next1_0.s1.1
eh.catch1_0.s1.1:  ; 0x401660
  %eh.head.1.f8.5 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x401660
  store i32 0, ptr %eh.head.1.f8.5, align 1  ; 0x401660
  %sp.20 = sub i32 %rsp.phi.6, 4  ; 0x401660
  %addr.z.65 = zext i32 %sp.20 to i64  ; 0x401660
  %st.p.42 = inttoptr i64 %addr.z.65 to ptr  ; 0x401660
  store i32 0, ptr %st.p.42, align 1  ; 0x401660
  br label %f4_401620
eh.next1_0.s1.1:  ; 0x401660
  call void @ra_eh_suspend()  ; 0x40163b
  %eh.head.1.f8.6 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x40163b
  store i32 0, ptr %eh.head.1.f8.6, align 1  ; 0x40163b
  %sp.26 = sub i32 %rsp.phi.6, 4  ; 0x40163b
  %addr.z.75 = zext i32 %sp.26 to i64  ; 0x40163b
  %st.p.49 = inttoptr i64 %addr.z.75 to ptr  ; 0x40163b
  store i32 0, ptr %st.p.49, align 1  ; 0x40163b
  br label %f5_401620
eh.cleaned.401620.1:  ; 0x401660
  %eh.head.1.f8.7 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x40163b
  store i32 0, ptr %eh.head.1.f8.7, align 1  ; 0x40163b
  br label %eh.enter1_0.1
f4_401620:  ; 0x401620
  %sp.21 = sub i32 %sp.20, 4  ; 0x401620
  %addr.z.66 = zext i32 %sp.21 to i64  ; 0x401620
  %st.p.43 = inttoptr i64 %addr.z.66 to ptr  ; 0x401620
  store i32 %eh.frame.8, ptr %st.p.43, align 1  ; 0x401620
  %diff.4 = sub i32 %sp.21, 8  ; 0x401621
  %sum.7 = add i32 %eh.frame.8, 12  ; 0x401624
  %addr.16 = add i32 %sum.7, -36  ; 0x401627
  %addr.z.67 = zext i32 %addr.16 to i64  ; 0x401627
  %m.p.7 = inttoptr i64 %addr.z.67 to ptr  ; 0x401627
  %m.7 = load i32, ptr %m.p.7, align 1  ; 0x401627
  %addr.17 = add i32 %sum.7, -32  ; 0x40162a
  %addr.z.68 = zext i32 %addr.17 to i64  ; 0x40162a
  %st.p.44 = inttoptr i64 %addr.z.68 to ptr  ; 0x40162a
  store i32 %m.7, ptr %st.p.44, align 1  ; 0x40162a
  %sp.22 = sub i32 %diff.4, 4  ; 0x40162d
  %addr.z.69 = zext i32 %sp.22 to i64  ; 0x40162d
  %st.p.45 = inttoptr i64 %addr.z.69 to ptr  ; 0x40162d
  store i32 220, ptr %st.p.45, align 1  ; 0x40162d
  %sp.23 = sub i32 %sp.22, 4  ; 0x401632
  %sarg0.a.45 = add i32 %sp.23, 4
  %ret.45 = call i64 @F_00401000(i32 220)  ; 0x401632
  %eh.pending.2 = call i64 @ra_eh_pending()  ; 0x401632
  %eh.thrown.2 = icmp ne i64 %eh.pending.2, 0  ; 0x401632
  br i1 %eh.thrown.2, label %eh.landing.1, label %eh.ok.401632
eh.ok.401632:  ; 0x401632
  %sum.8 = add i32 %sarg0.a.45, 12  ; 0x401637
  %sp.24 = add i32 %sum.8, 4  ; 0x40163a
  %sp.25 = add i32 %sp.24, 4  ; 0x40163b
  br label %eh.cleaned.401620.1
eh.cleaned.401620.2:  ; 0x40163b
  %eh.head.1.f8.8 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x40163b
  store i32 -1, ptr %eh.head.1.f8.8, align 1  ; 0x40163b
  call void @ra_eh_resume()  ; 0x40163b
  %eh.head.1.f0.2 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 0  ; 0x40163b
  %eh.next.4 = load i32, ptr %eh.head.1.f0.2, align 1  ; 0x40163b
  %teb.16 = call i64 @ra_teb()  ; 0x40163b
  %teb.17 = trunc i64 %teb.16 to i32  ; 0x40163b
  %addr.z.85 = zext i32 %teb.17 to i64  ; 0x40163b
  %st.p.55 = inttoptr i64 %addr.z.85 to ptr  ; 0x40163b
  store i32 %eh.next.4, ptr %st.p.55, align 1  ; 0x40163b
  ret i64 %ret.44
f5_401620:  ; 0x401620
  %sp.27 = sub i32 %sp.26, 4  ; 0x401620
  %addr.z.76 = zext i32 %sp.27 to i64  ; 0x401620
  %st.p.50 = inttoptr i64 %addr.z.76 to ptr  ; 0x401620
  store i32 %eh.frame.8, ptr %st.p.50, align 1  ; 0x401620
  %diff.5 = sub i32 %sp.27, 8  ; 0x401621
  %sum.9 = add i32 %eh.frame.8, 12  ; 0x401624
  %addr.18 = add i32 %sum.9, -36  ; 0x401627
  %addr.z.77 = zext i32 %addr.18 to i64  ; 0x401627
  %m.p.8 = inttoptr i64 %addr.z.77 to ptr  ; 0x401627
  %m.8 = load i32, ptr %m.p.8, align 1  ; 0x401627
  %addr.19 = add i32 %sum.9, -32  ; 0x40162a
  %addr.z.78 = zext i32 %addr.19 to i64  ; 0x40162a
  %st.p.51 = inttoptr i64 %addr.z.78 to ptr  ; 0x40162a
  store i32 %m.8, ptr %st.p.51, align 1  ; 0x40162a
  %sp.28 = sub i32 %diff.5, 4  ; 0x40162d
  %addr.z.79 = zext i32 %sp.28 to i64  ; 0x40162d
  %st.p.52 = inttoptr i64 %addr.z.79 to ptr  ; 0x40162d
  store i32 220, ptr %st.p.52, align 1  ; 0x40162d
  %sp.29 = sub i32 %sp.28, 4  ; 0x401632
  %sarg0.a.44 = add i32 %sp.29, 4
  %ret.44 = call i64 @F_00401000(i32 220)  ; 0x401632
  %eh.pending.3 = call i64 @ra_eh_pending()  ; 0x401632
  %eh.thrown.3 = icmp ne i64 %eh.pending.3, 0  ; 0x401632
  br i1 %eh.thrown.3, label %eh.landing.1, label %eh.ok.401632.1
eh.ok.401632.1:  ; 0x401632
  br label %eh.cleaned.401620.2
eh.state2:  ; 0x40163b
  %eh.frame.11 = add i32 %eh.reg.1, 12  ; 0x40163b
  %eh.catch.obj.8 = add i32 %eh.frame.11, -32  ; 0x40163b
  %eh.catch.obj.9 = zext i32 %eh.catch.obj.8 to i64  ; 0x40163b
  %eh.caught.4 = call i64 @ra_eh_catches(i64 565891180120823400, i64 %eh.catch.obj.9, i64 8192)  ; 0x40163b
  %eh.hit.4 = icmp ne i64 %eh.caught.4, 0  ; 0x40163b
  br i1 %eh.hit.4, label %eh.catch0_0.s2, label %eh.next0_0.s2
eh.catch0_0.s2:  ; 0x40163b
  %eh.head.1.f8.9 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x40163b
  store i32 2, ptr %eh.head.1.f8.9, align 1  ; 0x40163b
  br label %eh.enter0_0
eh.next0_0.s2:  ; 0x40163b
  %eh.catch.obj.10 = add i32 %eh.frame.11, -40  ; 0x4016a2
  %eh.catch.obj.11 = zext i32 %eh.catch.obj.10 to i64  ; 0x4016a2
  %eh.caught.5 = call i64 @ra_eh_catches(i64 565892279632451611, i64 %eh.catch.obj.11, i64 8192)  ; 0x4016a2
  %eh.hit.5 = icmp ne i64 %eh.caught.5, 0  ; 0x4016a2
  br i1 %eh.hit.5, label %eh.catch1_0.s2, label %eh.next1_0.s2
eh.enter0_0:  ; 0x40163b
  %rsp.phi.11 = phi i32 [ %rsp.phi.6, %eh.catch0_0.s2 ], [ %sp.55, %eh.cleaned.4015e0 ]  ; 0x40163b
  call void @ra_eh_enter_catch()  ; 0x40163b
  %eh.head.1.f8.10 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x40163b
  store i32 4, ptr %eh.head.1.f8.10, align 1  ; 0x40163b
  %eh.esp.p.6 = sub i32 %eh.reg.1, 4  ; 0x40163b
  %addr.z.88 = zext i32 %eh.esp.p.6 to i64  ; 0x40163b
  %eh.esp.p.7 = inttoptr i64 %addr.z.88 to ptr  ; 0x40163b
  %eh.esp.4 = load i32, ptr %eh.esp.p.7, align 1  ; 0x40163b
  %eh.esp.5 = zext i32 %eh.esp.4 to i64  ; 0x40163b
  call void @ra_eh_catch_sp(i64 %eh.esp.5)  ; 0x40163b
  %eh.frame.12 = add i32 %eh.reg.1, 12  ; 0x40163b
  %sp.32 = sub i32 %rsp.phi.11, 4  ; 0x40163b
  %addr.z.89 = zext i32 %sp.32 to i64  ; 0x40163b
  %st.p.58 = inttoptr i64 %addr.z.89 to ptr  ; 0x40163b
  store i32 0, ptr %st.p.58, align 1  ; 0x40163b
  br label %f6_401670
f6_401670:  ; 0x401670
  %sp.33 = sub i32 %sp.32, 4  ; 0x401670
  %addr.z.90 = zext i32 %sp.33 to i64  ; 0x401670
  %st.p.59 = inttoptr i64 %addr.z.90 to ptr  ; 0x401670
  store i32 %eh.frame.12, ptr %st.p.59, align 1  ; 0x401670
  %diff.6 = sub i32 %sp.33, 8  ; 0x401671
  %sum.11 = add i32 %eh.frame.12, 12  ; 0x401674
  %addr.20 = add i32 %sum.11, -28  ; 0x401677
  %addr.z.91 = zext i32 %addr.20 to i64  ; 0x401677
  %st.p.60 = inttoptr i64 %addr.z.91 to ptr  ; 0x401677
  store i32 %diff.6, ptr %st.p.60, align 1  ; 0x401677
  %addr.21 = add i32 %sum.11, -44  ; 0x40167a
  %addr.z.92 = zext i32 %addr.21 to i64  ; 0x40167a
  %m.p.9 = inttoptr i64 %addr.z.92 to ptr  ; 0x40167a
  %m.9 = load i32, ptr %m.p.9, align 1  ; 0x40167a
  %addr.22 = add i32 %sum.11, -32  ; 0x40167d
  %addr.z.93 = zext i32 %addr.22 to i64  ; 0x40167d
  %st.p.61 = inttoptr i64 %addr.z.93 to ptr  ; 0x40167d
  store i32 %m.9, ptr %st.p.61, align 1  ; 0x40167d
  %sp.34 = sub i32 %diff.6, 4  ; 0x401680
  %addr.z.94 = zext i32 %sp.34 to i64  ; 0x401680
  %st.p.62 = inttoptr i64 %addr.z.94 to ptr  ; 0x401680
  store i32 122, ptr %st.p.62, align 1  ; 0x401680
  %sp.35 = sub i32 %sp.34, 4  ; 0x401682
  %sarg0.a.43 = add i32 %sp.35, 4
  %ret.43 = call i64 @F_00401000(i32 122)  ; 0x401682
  %eh.pending.4 = call i64 @ra_eh_pending()  ; 0x401682
  %eh.thrown.4 = icmp ne i64 %eh.pending.4, 0  ; 0x401682
  br i1 %eh.thrown.4, label %eh.landing.1, label %eh.ok.401682
eh.ok.401682:  ; 0x401682
  %sum.12 = add i32 %sarg0.a.43, 4  ; 0x401687
  %m.10 = load i32, ptr %m.p.9, align 1  ; 0x40168a
  %sum.13 = mul i32 %m.10, 2  ; 0x40168d
  %addr.24 = add i32 %sum.11, -40  ; 0x40168f
  %addr.z.97 = zext i32 %addr.24 to i64  ; 0x40168f
  %st.p.64 = inttoptr i64 %addr.z.97 to ptr  ; 0x40168f
  store i32 %sum.13, ptr %st.p.64, align 1  ; 0x40168f
  %eh.head.1.f8.11 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x401692
  store i32 5, ptr %eh.head.1.f8.11, align 1  ; 0x401692
  %sp.36 = sub i32 %sum.12, 4  ; 0x40169c
  %addr.z.99 = zext i32 %sp.36 to i64  ; 0x40169c
  %st.p.66 = inttoptr i64 %addr.z.99 to ptr  ; 0x40169c
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 2596) to i32), ptr %st.p.66, align 1  ; 0x40169c
  %sp.37 = sub i32 %sp.36, 4  ; 0x4016a1
  %addr.z.100 = zext i32 %sp.37 to i64  ; 0x4016a1
  %st.p.67 = inttoptr i64 %addr.z.100 to ptr  ; 0x4016a1
  store i32 %addr.24, ptr %st.p.67, align 1  ; 0x4016a1
  call void @ra_eh_throw(i64 %addr.z.97, i64 4)  ; 0x4016a2
  call void @ra_eh_throw_type(i64 565892279632451611, i64 0, i64 4)  ; 0x4016a2
  br label %eh.landing.1
eh.catch1_0.s2:  ; 0x4016a2
  %eh.head.1.f8.12 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x4016a2
  store i32 0, ptr %eh.head.1.f8.12, align 1  ; 0x4016a2
  %sp.38 = sub i32 %rsp.phi.6, 4  ; 0x4016a2
  %addr.z.103 = zext i32 %sp.38 to i64  ; 0x4016a2
  %st.p.69 = inttoptr i64 %addr.z.103 to ptr  ; 0x4016a2
  store i32 0, ptr %st.p.69, align 1  ; 0x4016a2
  br label %f7_401620
eh.next1_0.s2:  ; 0x4016a2
  call void @ra_eh_suspend()  ; 0x40163b
  %eh.head.1.f8.13 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x40163b
  store i32 0, ptr %eh.head.1.f8.13, align 1  ; 0x40163b
  %sp.44 = sub i32 %rsp.phi.6, 4  ; 0x40163b
  %addr.z.113 = zext i32 %sp.44 to i64  ; 0x40163b
  %st.p.76 = inttoptr i64 %addr.z.113 to ptr  ; 0x40163b
  store i32 0, ptr %st.p.76, align 1  ; 0x40163b
  br label %f8_401620
eh.cleaned.401620.3:  ; 0x4016a2
  %eh.head.1.f8.14 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x40163b
  store i32 0, ptr %eh.head.1.f8.14, align 1  ; 0x40163b
  br label %eh.enter1_0.1
f7_401620:  ; 0x401620
  %sp.39 = sub i32 %sp.38, 4  ; 0x401620
  %addr.z.104 = zext i32 %sp.39 to i64  ; 0x401620
  %st.p.70 = inttoptr i64 %addr.z.104 to ptr  ; 0x401620
  store i32 %eh.frame.11, ptr %st.p.70, align 1  ; 0x401620
  %diff.7 = sub i32 %sp.39, 8  ; 0x401621
  %sum.14 = add i32 %eh.frame.11, 12  ; 0x401624
  %addr.27 = add i32 %sum.14, -36  ; 0x401627
  %addr.z.105 = zext i32 %addr.27 to i64  ; 0x401627
  %m.p.11 = inttoptr i64 %addr.z.105 to ptr  ; 0x401627
  %m.11 = load i32, ptr %m.p.11, align 1  ; 0x401627
  %addr.28 = add i32 %sum.14, -32  ; 0x40162a
  %addr.z.106 = zext i32 %addr.28 to i64  ; 0x40162a
  %st.p.71 = inttoptr i64 %addr.z.106 to ptr  ; 0x40162a
  store i32 %m.11, ptr %st.p.71, align 1  ; 0x40162a
  %sp.40 = sub i32 %diff.7, 4  ; 0x40162d
  %addr.z.107 = zext i32 %sp.40 to i64  ; 0x40162d
  %st.p.72 = inttoptr i64 %addr.z.107 to ptr  ; 0x40162d
  store i32 220, ptr %st.p.72, align 1  ; 0x40162d
  %sp.41 = sub i32 %sp.40, 4  ; 0x401632
  %sarg0.a.42 = add i32 %sp.41, 4
  %ret.42 = call i64 @F_00401000(i32 220)  ; 0x401632
  %eh.pending.5 = call i64 @ra_eh_pending()  ; 0x401632
  %eh.thrown.5 = icmp ne i64 %eh.pending.5, 0  ; 0x401632
  br i1 %eh.thrown.5, label %eh.landing.1, label %eh.ok.401632.2
eh.ok.401632.2:  ; 0x401632
  %sum.15 = add i32 %sarg0.a.42, 12  ; 0x401637
  %sp.42 = add i32 %sum.15, 4  ; 0x40163a
  %sp.43 = add i32 %sp.42, 4  ; 0x40163b
  br label %eh.cleaned.401620.3
eh.cleaned.401620.4:  ; 0x40163b
  %eh.head.1.f8.15 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x40163b
  store i32 -1, ptr %eh.head.1.f8.15, align 1  ; 0x40163b
  call void @ra_eh_resume()  ; 0x40163b
  %eh.head.1.f0.3 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 0  ; 0x40163b
  %eh.next.5 = load i32, ptr %eh.head.1.f0.3, align 1  ; 0x40163b
  %teb.18 = call i64 @ra_teb()  ; 0x40163b
  %teb.19 = trunc i64 %teb.18 to i32  ; 0x40163b
  %addr.z.123 = zext i32 %teb.19 to i64  ; 0x40163b
  %st.p.82 = inttoptr i64 %addr.z.123 to ptr  ; 0x40163b
  store i32 %eh.next.5, ptr %st.p.82, align 1  ; 0x40163b
  ret i64 %ret.41
f8_401620:  ; 0x401620
  %sp.45 = sub i32 %sp.44, 4  ; 0x401620
  %addr.z.114 = zext i32 %sp.45 to i64  ; 0x401620
  %st.p.77 = inttoptr i64 %addr.z.114 to ptr  ; 0x401620
  store i32 %eh.frame.11, ptr %st.p.77, align 1  ; 0x401620
  %diff.8 = sub i32 %sp.45, 8  ; 0x401621
  %sum.16 = add i32 %eh.frame.11, 12  ; 0x401624
  %addr.29 = add i32 %sum.16, -36  ; 0x401627
  %addr.z.115 = zext i32 %addr.29 to i64  ; 0x401627
  %m.p.12 = inttoptr i64 %addr.z.115 to ptr  ; 0x401627
  %m.12 = load i32, ptr %m.p.12, align 1  ; 0x401627
  %addr.30 = add i32 %sum.16, -32  ; 0x40162a
  %addr.z.116 = zext i32 %addr.30 to i64  ; 0x40162a
  %st.p.78 = inttoptr i64 %addr.z.116 to ptr  ; 0x40162a
  store i32 %m.12, ptr %st.p.78, align 1  ; 0x40162a
  %sp.46 = sub i32 %diff.8, 4  ; 0x40162d
  %addr.z.117 = zext i32 %sp.46 to i64  ; 0x40162d
  %st.p.79 = inttoptr i64 %addr.z.117 to ptr  ; 0x40162d
  store i32 220, ptr %st.p.79, align 1  ; 0x40162d
  %sp.47 = sub i32 %sp.46, 4  ; 0x401632
  %sarg0.a.41 = add i32 %sp.47, 4
  %ret.41 = call i64 @F_00401000(i32 220)  ; 0x401632
  %eh.pending.6 = call i64 @ra_eh_pending()  ; 0x401632
  %eh.thrown.6 = icmp ne i64 %eh.pending.6, 0  ; 0x401632
  br i1 %eh.thrown.6, label %eh.landing.1, label %eh.ok.401632.3
eh.ok.401632.3:  ; 0x401632
  br label %eh.cleaned.401620.4
eh.state3:  ; 0x40163b
  %eh.frame.16 = add i32 %eh.reg.1, 12  ; 0x40163b
  %eh.catch.obj.12 = add i32 %eh.frame.16, -32  ; 0x40163b
  %eh.catch.obj.13 = zext i32 %eh.catch.obj.12 to i64  ; 0x40163b
  %eh.caught.6 = call i64 @ra_eh_catches(i64 565891180120823400, i64 %eh.catch.obj.13, i64 8192)  ; 0x40163b
  %eh.hit.6 = icmp ne i64 %eh.caught.6, 0  ; 0x40163b
  br i1 %eh.hit.6, label %eh.catch0_0.s3, label %eh.next0_0.s3
eh.catch0_0.s3:  ; 0x40163b
  %eh.head.1.f8.16 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x40163b
  store i32 2, ptr %eh.head.1.f8.16, align 1  ; 0x40163b
  %sp.50 = sub i32 %rsp.phi.6, 4  ; 0x40163b
  %addr.z.125 = zext i32 %sp.50 to i64  ; 0x40163b
  %st.p.84 = inttoptr i64 %addr.z.125 to ptr  ; 0x40163b
  store i32 0, ptr %st.p.84, align 1  ; 0x40163b
  br label %f9_4015e0
eh.next0_0.s3:  ; 0x40163b
  %eh.catch.obj.14 = add i32 %eh.frame.16, -40  ; 0x4015ff
  %eh.catch.obj.15 = zext i32 %eh.catch.obj.14 to i64  ; 0x4015ff
  %eh.caught.7 = call i64 @ra_eh_catches(i64 565892279632451611, i64 %eh.catch.obj.15, i64 8192)  ; 0x4015ff
  %eh.hit.7 = icmp ne i64 %eh.caught.7, 0  ; 0x4015ff
  br i1 %eh.hit.7, label %eh.catch1_0.s3, label %eh.next1_0.s3
eh.cleaned.4015e0:  ; 0x40163b
  %eh.head.1.f8.17 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x4015ff
  store i32 2, ptr %eh.head.1.f8.17, align 1  ; 0x4015ff
  br label %eh.enter0_0
f9_4015e0:  ; 0x4015e0
  %sp.51 = sub i32 %sp.50, 4  ; 0x4015e0
  %addr.z.126 = zext i32 %sp.51 to i64  ; 0x4015e0
  %st.p.85 = inttoptr i64 %addr.z.126 to ptr  ; 0x4015e0
  store i32 %eh.frame.16, ptr %st.p.85, align 1  ; 0x4015e0
  %diff.9 = sub i32 %sp.51, 8  ; 0x4015e1
  %sum.18 = add i32 %eh.frame.16, 12  ; 0x4015e4
  %sp.52 = sub i32 %diff.9, 4  ; 0x4015e7
  %addr.z.127 = zext i32 %sp.52 to i64  ; 0x4015e7
  %st.p.86 = inttoptr i64 %addr.z.127 to ptr  ; 0x4015e7
  store i32 221, ptr %st.p.86, align 1  ; 0x4015e7
  %sp.53 = sub i32 %sp.52, 4  ; 0x4015ec
  %sarg0.a.40 = add i32 %sp.53, 4
  %ret.40 = call i64 @F_00401000(i32 221)  ; 0x4015ec
  %eh.pending.7 = call i64 @ra_eh_pending()  ; 0x4015ec
  %eh.thrown.7 = icmp ne i64 %eh.pending.7, 0  ; 0x4015ec
  br i1 %eh.thrown.7, label %eh.landing.1, label %eh.ok.4015ec
eh.ok.4015ec:  ; 0x4015ec
  %sum.19 = add i32 %sarg0.a.40, 4  ; 0x4015f1
  %addr.31 = add i32 %sum.18, -36  ; 0x4015f4
  %addr.z.129 = zext i32 %addr.31 to i64  ; 0x4015f4
  %st.p.88 = inttoptr i64 %addr.z.129 to ptr  ; 0x4015f4
  store i32 0, ptr %st.p.88, align 1  ; 0x4015f4
  %sum.20 = add i32 %sum.19, 8  ; 0x4015fb
  %sp.54 = add i32 %sum.20, 4  ; 0x4015fe
  %sp.55 = add i32 %sp.54, 4  ; 0x4015ff
  br label %eh.cleaned.4015e0
eh.catch1_0.s3:  ; 0x4015ff
  %eh.head.1.f8.18 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x4015ff
  store i32 2, ptr %eh.head.1.f8.18, align 1  ; 0x4015ff
  %sp.56 = sub i32 %rsp.phi.6, 4  ; 0x4015ff
  %addr.z.134 = zext i32 %sp.56 to i64  ; 0x4015ff
  %st.p.91 = inttoptr i64 %addr.z.134 to ptr  ; 0x4015ff
  store i32 0, ptr %st.p.91, align 1  ; 0x4015ff
  br label %f10_4015e0
eh.next1_0.s3:  ; 0x4015ff
  call void @ra_eh_suspend()  ; 0x40163b
  %eh.head.1.f8.19 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x40163b
  store i32 2, ptr %eh.head.1.f8.19, align 1  ; 0x40163b
  %sp.68 = sub i32 %rsp.phi.6, 4  ; 0x40163b
  %addr.z.152 = zext i32 %sp.68 to i64  ; 0x40163b
  %st.p.104 = inttoptr i64 %addr.z.152 to ptr  ; 0x40163b
  store i32 0, ptr %st.p.104, align 1  ; 0x40163b
  br label %f12_4015e0
eh.cleaned.4015e0.1:  ; 0x4015ff
  %eh.head.1.f8.20 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x4015ff
  store i32 0, ptr %eh.head.1.f8.20, align 1  ; 0x4015ff
  %sp.62 = sub i32 %sp.61, 4  ; 0x4015ff
  %addr.z.142 = zext i32 %sp.62 to i64  ; 0x4015ff
  %st.p.97 = inttoptr i64 %addr.z.142 to ptr  ; 0x4015ff
  store i32 0, ptr %st.p.97, align 1  ; 0x4015ff
  br label %f11_401620
f10_4015e0:  ; 0x4015e0
  %sp.57 = sub i32 %sp.56, 4  ; 0x4015e0
  %addr.z.135 = zext i32 %sp.57 to i64  ; 0x4015e0
  %st.p.92 = inttoptr i64 %addr.z.135 to ptr  ; 0x4015e0
  store i32 %eh.frame.16, ptr %st.p.92, align 1  ; 0x4015e0
  %diff.10 = sub i32 %sp.57, 8  ; 0x4015e1
  %sum.21 = add i32 %eh.frame.16, 12  ; 0x4015e4
  %sp.58 = sub i32 %diff.10, 4  ; 0x4015e7
  %addr.z.136 = zext i32 %sp.58 to i64  ; 0x4015e7
  %st.p.93 = inttoptr i64 %addr.z.136 to ptr  ; 0x4015e7
  store i32 221, ptr %st.p.93, align 1  ; 0x4015e7
  %sp.59 = sub i32 %sp.58, 4  ; 0x4015ec
  %sarg0.a.38 = add i32 %sp.59, 4
  %ret.38 = call i64 @F_00401000(i32 221)  ; 0x4015ec
  %eh.pending.8 = call i64 @ra_eh_pending()  ; 0x4015ec
  %eh.thrown.8 = icmp ne i64 %eh.pending.8, 0  ; 0x4015ec
  br i1 %eh.thrown.8, label %eh.landing.1, label %eh.ok.4015ec.1
eh.ok.4015ec.1:  ; 0x4015ec
  %sum.22 = add i32 %sarg0.a.38, 4  ; 0x4015f1
  %addr.32 = add i32 %sum.21, -36  ; 0x4015f4
  %addr.z.138 = zext i32 %addr.32 to i64  ; 0x4015f4
  %st.p.95 = inttoptr i64 %addr.z.138 to ptr  ; 0x4015f4
  store i32 0, ptr %st.p.95, align 1  ; 0x4015f4
  %sum.23 = add i32 %sum.22, 8  ; 0x4015fb
  %sp.60 = add i32 %sum.23, 4  ; 0x4015fe
  %sp.61 = add i32 %sp.60, 4  ; 0x4015ff
  br label %eh.cleaned.4015e0.1
eh.cleaned.401620.5:  ; 0x4015ff
  %eh.head.1.f8.21 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x40163b
  store i32 0, ptr %eh.head.1.f8.21, align 1  ; 0x40163b
  br label %eh.enter1_0.1
f11_401620:  ; 0x401620
  %sp.63 = sub i32 %sp.62, 4  ; 0x401620
  %addr.z.143 = zext i32 %sp.63 to i64  ; 0x401620
  %st.p.98 = inttoptr i64 %addr.z.143 to ptr  ; 0x401620
  store i32 %eh.frame.16, ptr %st.p.98, align 1  ; 0x401620
  %diff.11 = sub i32 %sp.63, 8  ; 0x401621
  %m.13 = load i32, ptr %st.p.95, align 1  ; 0x401627
  %addr.34 = add i32 %sum.21, -32  ; 0x40162a
  %addr.z.145 = zext i32 %addr.34 to i64  ; 0x40162a
  %st.p.99 = inttoptr i64 %addr.z.145 to ptr  ; 0x40162a
  store i32 %m.13, ptr %st.p.99, align 1  ; 0x40162a
  %sp.64 = sub i32 %diff.11, 4  ; 0x40162d
  %addr.z.146 = zext i32 %sp.64 to i64  ; 0x40162d
  %st.p.100 = inttoptr i64 %addr.z.146 to ptr  ; 0x40162d
  store i32 220, ptr %st.p.100, align 1  ; 0x40162d
  %sp.65 = sub i32 %sp.64, 4  ; 0x401632
  %sarg0.a.39 = add i32 %sp.65, 4
  %ret.39 = call i64 @F_00401000(i32 220)  ; 0x401632
  %eh.pending.9 = call i64 @ra_eh_pending()  ; 0x401632
  %eh.thrown.9 = icmp ne i64 %eh.pending.9, 0  ; 0x401632
  br i1 %eh.thrown.9, label %eh.landing.1, label %eh.ok.401632.4
eh.ok.401632.4:  ; 0x401632
  %sum.25 = add i32 %sarg0.a.39, 12  ; 0x401637
  %sp.66 = add i32 %sum.25, 4  ; 0x40163a
  %sp.67 = add i32 %sp.66, 4  ; 0x40163b
  br label %eh.cleaned.401620.5
eh.cleaned.4015e0.2:  ; 0x40163b
  %eh.head.1.f8.22 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x4015ff
  store i32 0, ptr %eh.head.1.f8.22, align 1  ; 0x4015ff
  %sp.74 = sub i32 %sp.73, 4  ; 0x4015ff
  %addr.z.160 = zext i32 %sp.74 to i64  ; 0x4015ff
  %st.p.110 = inttoptr i64 %addr.z.160 to ptr  ; 0x4015ff
  store i32 0, ptr %st.p.110, align 1  ; 0x4015ff
  br label %f13_401620
f12_4015e0:  ; 0x4015e0
  %sp.69 = sub i32 %sp.68, 4  ; 0x4015e0
  %addr.z.153 = zext i32 %sp.69 to i64  ; 0x4015e0
  %st.p.105 = inttoptr i64 %addr.z.153 to ptr  ; 0x4015e0
  store i32 %eh.frame.16, ptr %st.p.105, align 1  ; 0x4015e0
  %diff.12 = sub i32 %sp.69, 8  ; 0x4015e1
  %sum.26 = add i32 %eh.frame.16, 12  ; 0x4015e4
  %sp.70 = sub i32 %diff.12, 4  ; 0x4015e7
  %addr.z.154 = zext i32 %sp.70 to i64  ; 0x4015e7
  %st.p.106 = inttoptr i64 %addr.z.154 to ptr  ; 0x4015e7
  store i32 221, ptr %st.p.106, align 1  ; 0x4015e7
  %sp.71 = sub i32 %sp.70, 4  ; 0x4015ec
  %sarg0.a.36 = add i32 %sp.71, 4
  %ret.36 = call i64 @F_00401000(i32 221)  ; 0x4015ec
  %eh.pending.10 = call i64 @ra_eh_pending()  ; 0x4015ec
  %eh.thrown.10 = icmp ne i64 %eh.pending.10, 0  ; 0x4015ec
  br i1 %eh.thrown.10, label %eh.landing.1, label %eh.ok.4015ec.2
eh.ok.4015ec.2:  ; 0x4015ec
  %sum.27 = add i32 %sarg0.a.36, 4  ; 0x4015f1
  %addr.35 = add i32 %sum.26, -36  ; 0x4015f4
  %addr.z.156 = zext i32 %addr.35 to i64  ; 0x4015f4
  %st.p.108 = inttoptr i64 %addr.z.156 to ptr  ; 0x4015f4
  store i32 0, ptr %st.p.108, align 1  ; 0x4015f4
  %sum.28 = add i32 %sum.27, 8  ; 0x4015fb
  %sp.72 = add i32 %sum.28, 4  ; 0x4015fe
  %sp.73 = add i32 %sp.72, 4  ; 0x4015ff
  br label %eh.cleaned.4015e0.2
eh.cleaned.401620.6:  ; 0x4015ff
  %eh.head.1.f8.23 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x40163b
  store i32 -1, ptr %eh.head.1.f8.23, align 1  ; 0x40163b
  call void @ra_eh_resume()  ; 0x40163b
  %eh.head.1.f0.4 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 0  ; 0x40163b
  %eh.next.6 = load i32, ptr %eh.head.1.f0.4, align 1  ; 0x40163b
  %teb.20 = call i64 @ra_teb()  ; 0x40163b
  %teb.21 = trunc i64 %teb.20 to i32  ; 0x40163b
  %addr.z.170 = zext i32 %teb.21 to i64  ; 0x40163b
  %st.p.116 = inttoptr i64 %addr.z.170 to ptr  ; 0x40163b
  store i32 %eh.next.6, ptr %st.p.116, align 1  ; 0x40163b
  ret i64 %ret.37
f13_401620:  ; 0x401620
  %sp.75 = sub i32 %sp.74, 4  ; 0x401620
  %addr.z.161 = zext i32 %sp.75 to i64  ; 0x401620
  %st.p.111 = inttoptr i64 %addr.z.161 to ptr  ; 0x401620
  store i32 %eh.frame.16, ptr %st.p.111, align 1  ; 0x401620
  %diff.13 = sub i32 %sp.75, 8  ; 0x401621
  %m.14 = load i32, ptr %st.p.108, align 1  ; 0x401627
  %addr.37 = add i32 %sum.26, -32  ; 0x40162a
  %addr.z.163 = zext i32 %addr.37 to i64  ; 0x40162a
  %st.p.112 = inttoptr i64 %addr.z.163 to ptr  ; 0x40162a
  store i32 %m.14, ptr %st.p.112, align 1  ; 0x40162a
  %sp.76 = sub i32 %diff.13, 4  ; 0x40162d
  %addr.z.164 = zext i32 %sp.76 to i64  ; 0x40162d
  %st.p.113 = inttoptr i64 %addr.z.164 to ptr  ; 0x40162d
  store i32 220, ptr %st.p.113, align 1  ; 0x40162d
  %sp.77 = sub i32 %sp.76, 4  ; 0x401632
  %sarg0.a.37 = add i32 %sp.77, 4
  %ret.37 = call i64 @F_00401000(i32 220)  ; 0x401632
  %eh.pending.11 = call i64 @ra_eh_pending()  ; 0x401632
  %eh.thrown.11 = icmp ne i64 %eh.pending.11, 0  ; 0x401632
  br i1 %eh.thrown.11, label %eh.landing.1, label %eh.ok.401632.5
eh.ok.401632.5:  ; 0x401632
  br label %eh.cleaned.401620.6
eh.state4:  ; 0x40163b
  %eh.frame.23 = add i32 %eh.reg.1, 12  ; 0x40163b
  %eh.catch.obj.16 = add i32 %eh.frame.23, -40  ; 0x40163b
  %eh.catch.obj.17 = zext i32 %eh.catch.obj.16 to i64  ; 0x40163b
  %eh.caught.8 = call i64 @ra_eh_catches(i64 565892279632451611, i64 %eh.catch.obj.17, i64 8192)  ; 0x40163b
  %eh.hit.8 = icmp ne i64 %eh.caught.8, 0  ; 0x40163b
  br i1 %eh.hit.8, label %eh.catch1_0.s4, label %eh.next1_0.s4
eh.catch1_0.s4:  ; 0x40163b
  %eh.esp.saved = call i64 @ra_eh_ending_sp()  ; 0x40163b
  %eh.esp.saved.1 = trunc i64 %eh.esp.saved to i32  ; 0x40163b
  %eh.esp.p.8 = sub i32 %eh.reg.1, 4  ; 0x40163b
  %addr.z.171 = zext i32 %eh.esp.p.8 to i64  ; 0x40163b
  %st.p.117 = inttoptr i64 %addr.z.171 to ptr  ; 0x40163b
  store i32 %eh.esp.saved.1, ptr %st.p.117, align 1  ; 0x40163b
  %eh.ending.2 = call i64 @ra_eh_ending_object()  ; 0x40163b
  %eh.dtor.2 = call i64 @ra_eh_ending_destructor()  ; 0x40163b
  call void @ra_eh_leave_catch()  ; 0x40163b
  %eh.dies.2 = icmp ne i64 %eh.dtor.2, 0  ; 0x40163b
  br i1 %eh.dies.2, label %eh.destroy.401540, label %eh.destroyed.401540
eh.next1_0.s4:  ; 0x40163b
  call void @ra_eh_suspend()  ; 0x40163b
  %eh.esp.saved.2 = call i64 @ra_eh_ending_sp()  ; 0x40163b
  %eh.esp.saved.3 = trunc i64 %eh.esp.saved.2 to i32  ; 0x40163b
  %eh.esp.p.9 = sub i32 %eh.reg.1, 4  ; 0x40163b
  %addr.z.183 = zext i32 %eh.esp.p.9 to i64  ; 0x40163b
  %st.p.126 = inttoptr i64 %addr.z.183 to ptr  ; 0x40163b
  store i32 %eh.esp.saved.3, ptr %st.p.126, align 1  ; 0x40163b
  %eh.ending.3 = call i64 @ra_eh_ending_object()  ; 0x40163b
  %eh.dtor.3 = call i64 @ra_eh_ending_destructor()  ; 0x40163b
  call void @ra_eh_leave_catch()  ; 0x40163b
  %eh.dies.3 = icmp ne i64 %eh.dtor.3, 0  ; 0x40163b
  br i1 %eh.dies.3, label %eh.destroy.401540.1, label %eh.destroyed.401540.1
eh.destroy.401540:  ; 0x40163b
  %eh.this.2 = trunc i64 %eh.ending.2 to i32  ; 0x40163b
  %ecx.z.3 = zext i32 %eh.this.2 to i64  ; 0x40163b
  %sp.80 = sub i32 %rsp.phi.6, 4  ; 0x40163b
  %addr.z.172 = zext i32 %sp.80 to i64  ; 0x40163b
  %st.p.118 = inttoptr i64 %addr.z.172 to ptr  ; 0x40163b
  store i32 0, ptr %st.p.118, align 1  ; 0x40163b
  %fp.9 = inttoptr i64 %eh.dtor.2 to ptr  ; 0x40163b
  %sarg0.a.34 = add i64 %addr.z.172, 4
  %sarg0.p.34 = inttoptr i64 %sarg0.a.34 to ptr
  %sarg0.d.34 = load i32, ptr %sarg0.p.34, align 4
  %sarg0.34 = zext i32 %sarg0.d.34 to i64
  %sarg1.a.9 = add i64 %addr.z.172, 8
  %sarg1.p.9 = inttoptr i64 %sarg1.a.9 to ptr
  %sarg1.d.9 = load i32, ptr %sarg1.p.9, align 4
  %sarg1.9 = zext i32 %sarg1.d.9 to i64
  %sarg2.a.9 = add i64 %addr.z.172, 12
  %sarg2.p.9 = inttoptr i64 %sarg2.a.9 to ptr
  %sarg2.d.9 = load i32, ptr %sarg2.p.9, align 4
  %sarg2.9 = zext i32 %sarg2.d.9 to i64
  %sarg3.a.9 = add i64 %addr.z.172, 16
  %sarg3.p.9 = inttoptr i64 %sarg3.a.9 to ptr
  %sarg3.d.9 = load i32, ptr %sarg3.p.9, align 4
  %sarg3.9 = zext i32 %sarg3.d.9 to i64
  %sarg4.a.9 = add i64 %addr.z.172, 20
  %sarg4.p.9 = inttoptr i64 %sarg4.a.9 to ptr
  %sarg4.d.9 = load i32, ptr %sarg4.p.9, align 4
  %sarg4.9 = zext i32 %sarg4.d.9 to i64
  %sarg5.a.9 = add i64 %addr.z.172, 24
  %sarg5.p.9 = inttoptr i64 %sarg5.a.9 to ptr
  %sarg5.d.9 = load i32, ptr %sarg5.p.9, align 4
  %sarg5.9 = zext i32 %sarg5.d.9 to i64
  %sarg6.a.9 = add i64 %addr.z.172, 28
  %sarg6.p.9 = inttoptr i64 %sarg6.a.9 to ptr
  %sarg6.d.9 = load i32, ptr %sarg6.p.9, align 4
  %sarg6.9 = zext i32 %sarg6.d.9 to i64
  %args.p.18 = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr  ; 0x40163b
  store i64 %sarg6.9, ptr %args.p.18, align 8  ; 0x40163b
  %sarg7.a.9 = add i64 %addr.z.172, 32
  %sarg7.p.9 = inttoptr i64 %sarg7.a.9 to ptr
  %sarg7.d.9 = load i32, ptr %sarg7.p.9, align 4
  %sarg7.9 = zext i32 %sarg7.d.9 to i64
  %args.p.19 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr  ; 0x40163b
  store i64 %sarg7.9, ptr %args.p.19, align 8  ; 0x40163b
  %ret.34 = call i64 %fp.9(i64 %sarg0.34, i64 %sarg1.9, i64 %sarg2.9, i64 %sarg3.9, i64 %sarg4.9, i64 %sarg5.9, i64 %ecx.z.3, i64 undef)  ; 0x40163b
  %pops.9 = call i64 @ra_pops_of(i64 %eh.dtor.2)  ; 0x40163b
  %n.2 = trunc i64 %pops.9 to i32
  %pops.a.9 = add i32 %n.2, 4  ; 0x40163b
  %rsp.358 = add i32 %sp.80, %pops.a.9  ; 0x40163b
  br label %eh.destroyed.401540
eh.destroyed.401540:  ; 0x40163b
  %rsp.phi.10 = phi i32 [ %rsp.phi.6, %eh.catch1_0.s4 ], [ %rsp.358, %eh.destroy.401540 ]  ; 0x40163b
  %eh.head.1.f8.24 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x40163b
  store i32 0, ptr %eh.head.1.f8.24, align 1  ; 0x40163b
  %sp.81 = sub i32 %rsp.phi.10, 4  ; 0x40163b
  %addr.z.174 = zext i32 %sp.81 to i64  ; 0x40163b
  %st.p.120 = inttoptr i64 %addr.z.174 to ptr  ; 0x40163b
  store i32 0, ptr %st.p.120, align 1  ; 0x40163b
  br label %f14_401620
eh.cleaned.401620.7:  ; 0x40163b
  %eh.head.1.f8.25 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x40163b
  store i32 0, ptr %eh.head.1.f8.25, align 1  ; 0x40163b
  br label %eh.enter1_0.1
f14_401620:  ; 0x401620
  %sp.82 = sub i32 %sp.81, 4  ; 0x401620
  %addr.z.175 = zext i32 %sp.82 to i64  ; 0x401620
  %st.p.121 = inttoptr i64 %addr.z.175 to ptr  ; 0x401620
  store i32 %eh.frame.23, ptr %st.p.121, align 1  ; 0x401620
  %diff.14 = sub i32 %sp.82, 8  ; 0x401621
  %sum.31 = add i32 %eh.frame.23, 12  ; 0x401624
  %addr.38 = add i32 %sum.31, -36  ; 0x401627
  %addr.z.176 = zext i32 %addr.38 to i64  ; 0x401627
  %m.p.15 = inttoptr i64 %addr.z.176 to ptr  ; 0x401627
  %m.15 = load i32, ptr %m.p.15, align 1  ; 0x401627
  %addr.39 = add i32 %sum.31, -32  ; 0x40162a
  %addr.z.177 = zext i32 %addr.39 to i64  ; 0x40162a
  %st.p.122 = inttoptr i64 %addr.z.177 to ptr  ; 0x40162a
  store i32 %m.15, ptr %st.p.122, align 1  ; 0x40162a
  %sp.83 = sub i32 %diff.14, 4  ; 0x40162d
  %addr.z.178 = zext i32 %sp.83 to i64  ; 0x40162d
  %st.p.123 = inttoptr i64 %addr.z.178 to ptr  ; 0x40162d
  store i32 220, ptr %st.p.123, align 1  ; 0x40162d
  %sp.84 = sub i32 %sp.83, 4  ; 0x401632
  %sarg0.a.35 = add i32 %sp.84, 4
  %ret.35 = call i64 @F_00401000(i32 220)  ; 0x401632
  %eh.pending.12 = call i64 @ra_eh_pending()  ; 0x401632
  %eh.thrown.12 = icmp ne i64 %eh.pending.12, 0  ; 0x401632
  br i1 %eh.thrown.12, label %eh.landing.1, label %eh.ok.401632.6
eh.ok.401632.6:  ; 0x401632
  %sum.32 = add i32 %sarg0.a.35, 12  ; 0x401637
  %sp.85 = add i32 %sum.32, 4  ; 0x40163a
  %sp.86 = add i32 %sp.85, 4  ; 0x40163b
  br label %eh.cleaned.401620.7
eh.destroy.401540.1:  ; 0x40163b
  %eh.this.3 = trunc i64 %eh.ending.3 to i32  ; 0x40163b
  %ecx.z.4 = zext i32 %eh.this.3 to i64  ; 0x40163b
  %sp.87 = sub i32 %rsp.phi.6, 4  ; 0x40163b
  %addr.z.184 = zext i32 %sp.87 to i64  ; 0x40163b
  %st.p.127 = inttoptr i64 %addr.z.184 to ptr  ; 0x40163b
  store i32 0, ptr %st.p.127, align 1  ; 0x40163b
  %fp.8 = inttoptr i64 %eh.dtor.3 to ptr  ; 0x40163b
  %sarg0.a.32 = add i64 %addr.z.184, 4
  %sarg0.p.32 = inttoptr i64 %sarg0.a.32 to ptr
  %sarg0.d.32 = load i32, ptr %sarg0.p.32, align 4
  %sarg0.32 = zext i32 %sarg0.d.32 to i64
  %sarg1.a.8 = add i64 %addr.z.184, 8
  %sarg1.p.8 = inttoptr i64 %sarg1.a.8 to ptr
  %sarg1.d.8 = load i32, ptr %sarg1.p.8, align 4
  %sarg1.8 = zext i32 %sarg1.d.8 to i64
  %sarg2.a.8 = add i64 %addr.z.184, 12
  %sarg2.p.8 = inttoptr i64 %sarg2.a.8 to ptr
  %sarg2.d.8 = load i32, ptr %sarg2.p.8, align 4
  %sarg2.8 = zext i32 %sarg2.d.8 to i64
  %sarg3.a.8 = add i64 %addr.z.184, 16
  %sarg3.p.8 = inttoptr i64 %sarg3.a.8 to ptr
  %sarg3.d.8 = load i32, ptr %sarg3.p.8, align 4
  %sarg3.8 = zext i32 %sarg3.d.8 to i64
  %sarg4.a.8 = add i64 %addr.z.184, 20
  %sarg4.p.8 = inttoptr i64 %sarg4.a.8 to ptr
  %sarg4.d.8 = load i32, ptr %sarg4.p.8, align 4
  %sarg4.8 = zext i32 %sarg4.d.8 to i64
  %sarg5.a.8 = add i64 %addr.z.184, 24
  %sarg5.p.8 = inttoptr i64 %sarg5.a.8 to ptr
  %sarg5.d.8 = load i32, ptr %sarg5.p.8, align 4
  %sarg5.8 = zext i32 %sarg5.d.8 to i64
  %sarg6.a.8 = add i64 %addr.z.184, 28
  %sarg6.p.8 = inttoptr i64 %sarg6.a.8 to ptr
  %sarg6.d.8 = load i32, ptr %sarg6.p.8, align 4
  %sarg6.8 = zext i32 %sarg6.d.8 to i64
  %args.p.16 = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr  ; 0x40163b
  store i64 %sarg6.8, ptr %args.p.16, align 8  ; 0x40163b
  %sarg7.a.8 = add i64 %addr.z.184, 32
  %sarg7.p.8 = inttoptr i64 %sarg7.a.8 to ptr
  %sarg7.d.8 = load i32, ptr %sarg7.p.8, align 4
  %sarg7.8 = zext i32 %sarg7.d.8 to i64
  %args.p.17 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr  ; 0x40163b
  store i64 %sarg7.8, ptr %args.p.17, align 8  ; 0x40163b
  %ret.32 = call i64 %fp.8(i64 %sarg0.32, i64 %sarg1.8, i64 %sarg2.8, i64 %sarg3.8, i64 %sarg4.8, i64 %sarg5.8, i64 %ecx.z.4, i64 undef)  ; 0x40163b
  %pops.8 = call i64 @ra_pops_of(i64 %eh.dtor.3)  ; 0x40163b
  %n.4 = trunc i64 %pops.8 to i32
  %pops.a.8 = add i32 %n.4, 4  ; 0x40163b
  %rsp.356 = add i32 %sp.87, %pops.a.8  ; 0x40163b
  br label %eh.destroyed.401540.1
eh.destroyed.401540.1:  ; 0x40163b
  %rsp.phi.9 = phi i32 [ %rsp.phi.6, %eh.next1_0.s4 ], [ %rsp.356, %eh.destroy.401540.1 ]  ; 0x40163b
  %eh.head.1.f8.26 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x40163b
  store i32 0, ptr %eh.head.1.f8.26, align 1  ; 0x40163b
  %sp.88 = sub i32 %rsp.phi.9, 4  ; 0x40163b
  %addr.z.186 = zext i32 %sp.88 to i64  ; 0x40163b
  %st.p.129 = inttoptr i64 %addr.z.186 to ptr  ; 0x40163b
  store i32 0, ptr %st.p.129, align 1  ; 0x40163b
  br label %f15_401620
eh.cleaned.401620.8:  ; 0x40163b
  %eh.head.1.f8.27 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x40163b
  store i32 -1, ptr %eh.head.1.f8.27, align 1  ; 0x40163b
  call void @ra_eh_resume()  ; 0x40163b
  %eh.head.1.f0.5 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 0  ; 0x40163b
  %eh.next.7 = load i32, ptr %eh.head.1.f0.5, align 1  ; 0x40163b
  %teb.22 = call i64 @ra_teb()  ; 0x40163b
  %teb.23 = trunc i64 %teb.22 to i32  ; 0x40163b
  %addr.z.196 = zext i32 %teb.23 to i64  ; 0x40163b
  %st.p.135 = inttoptr i64 %addr.z.196 to ptr  ; 0x40163b
  store i32 %eh.next.7, ptr %st.p.135, align 1  ; 0x40163b
  ret i64 %ret.33
f15_401620:  ; 0x401620
  %sp.89 = sub i32 %sp.88, 4  ; 0x401620
  %addr.z.187 = zext i32 %sp.89 to i64  ; 0x401620
  %st.p.130 = inttoptr i64 %addr.z.187 to ptr  ; 0x401620
  store i32 %eh.frame.23, ptr %st.p.130, align 1  ; 0x401620
  %diff.15 = sub i32 %sp.89, 8  ; 0x401621
  %sum.33 = add i32 %eh.frame.23, 12  ; 0x401624
  %addr.40 = add i32 %sum.33, -36  ; 0x401627
  %addr.z.188 = zext i32 %addr.40 to i64  ; 0x401627
  %m.p.16 = inttoptr i64 %addr.z.188 to ptr  ; 0x401627
  %m.16 = load i32, ptr %m.p.16, align 1  ; 0x401627
  %addr.41 = add i32 %sum.33, -32  ; 0x40162a
  %addr.z.189 = zext i32 %addr.41 to i64  ; 0x40162a
  %st.p.131 = inttoptr i64 %addr.z.189 to ptr  ; 0x40162a
  store i32 %m.16, ptr %st.p.131, align 1  ; 0x40162a
  %sp.90 = sub i32 %diff.15, 4  ; 0x40162d
  %addr.z.190 = zext i32 %sp.90 to i64  ; 0x40162d
  %st.p.132 = inttoptr i64 %addr.z.190 to ptr  ; 0x40162d
  store i32 220, ptr %st.p.132, align 1  ; 0x40162d
  %sp.91 = sub i32 %sp.90, 4  ; 0x401632
  %sarg0.a.33 = add i32 %sp.91, 4
  %ret.33 = call i64 @F_00401000(i32 220)  ; 0x401632
  %eh.pending.13 = call i64 @ra_eh_pending()  ; 0x401632
  %eh.thrown.13 = icmp ne i64 %eh.pending.13, 0  ; 0x401632
  br i1 %eh.thrown.13, label %eh.landing.1, label %eh.ok.401632.7
eh.ok.401632.7:  ; 0x401632
  br label %eh.cleaned.401620.8
eh.state5:  ; 0x40163b
  %eh.frame.26 = add i32 %eh.reg.1, 12  ; 0x40163b
  %eh.catch.obj.18 = add i32 %eh.frame.26, -40  ; 0x40163b
  %eh.catch.obj.19 = zext i32 %eh.catch.obj.18 to i64  ; 0x40163b
  %eh.caught.9 = call i64 @ra_eh_catches(i64 565892279632451611, i64 %eh.catch.obj.19, i64 8192)  ; 0x40163b
  %eh.hit.9 = icmp ne i64 %eh.caught.9, 0  ; 0x40163b
  br i1 %eh.hit.9, label %eh.catch1_0.s5, label %eh.next1_0.s5
eh.catch1_0.s5:  ; 0x40163b
  %eh.head.1.f8.28 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x40163b
  store i32 4, ptr %eh.head.1.f8.28, align 1  ; 0x40163b
  %sp.94 = sub i32 %rsp.phi.6, 4  ; 0x40163b
  %addr.z.198 = zext i32 %sp.94 to i64  ; 0x40163b
  %st.p.137 = inttoptr i64 %addr.z.198 to ptr  ; 0x40163b
  store i32 0, ptr %st.p.137, align 1  ; 0x40163b
  br label %f16_401600
eh.next1_0.s5:  ; 0x40163b
  call void @ra_eh_suspend()  ; 0x40163b
  %eh.head.1.f8.29 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x40163b
  store i32 4, ptr %eh.head.1.f8.29, align 1  ; 0x40163b
  %sp.107 = sub i32 %rsp.phi.6, 4  ; 0x40163b
  %addr.z.219 = zext i32 %sp.107 to i64  ; 0x40163b
  %st.p.152 = inttoptr i64 %addr.z.219 to ptr  ; 0x40163b
  store i32 0, ptr %st.p.152, align 1  ; 0x40163b
  br label %f18_401600
eh.cleaned.401600:  ; 0x40163b
  %eh.esp.saved.4 = call i64 @ra_eh_ending_sp()  ; 0x40161e
  %eh.esp.saved.5 = trunc i64 %eh.esp.saved.4 to i32  ; 0x40161e
  %eh.esp.p.10 = sub i32 %eh.reg.1, 4  ; 0x40161e
  %addr.z.206 = zext i32 %eh.esp.p.10 to i64  ; 0x40161e
  %st.p.142 = inttoptr i64 %addr.z.206 to ptr  ; 0x40161e
  store i32 %eh.esp.saved.5, ptr %st.p.142, align 1  ; 0x40161e
  %eh.ending.4 = call i64 @ra_eh_ending_object()  ; 0x40161e
  %eh.dtor.4 = call i64 @ra_eh_ending_destructor()  ; 0x40161e
  call void @ra_eh_leave_catch()  ; 0x40161e
  %eh.dies.4 = icmp ne i64 %eh.dtor.4, 0  ; 0x40161e
  br i1 %eh.dies.4, label %eh.destroy.401540.2, label %eh.destroyed.401540.2
f16_401600:  ; 0x401600
  %sp.95 = sub i32 %sp.94, 4  ; 0x401600
  %addr.z.199 = zext i32 %sp.95 to i64  ; 0x401600
  %st.p.138 = inttoptr i64 %addr.z.199 to ptr  ; 0x401600
  store i32 %eh.frame.26, ptr %st.p.138, align 1  ; 0x401600
  %diff.16 = sub i32 %sp.95, 8  ; 0x401601
  %sum.35 = add i32 %eh.frame.26, 12  ; 0x401604
  %sp.96 = sub i32 %diff.16, 4  ; 0x401607
  %addr.z.200 = zext i32 %sp.96 to i64  ; 0x401607
  %st.p.139 = inttoptr i64 %addr.z.200 to ptr  ; 0x401607
  store i32 222, ptr %st.p.139, align 1  ; 0x401607
  %sp.97 = sub i32 %sp.96, 4  ; 0x40160c
  %sarg0.a.29 = add i32 %sp.97, 4
  %ret.29 = call i64 @F_00401000(i32 222)  ; 0x40160c
  %eh.pending.14 = call i64 @ra_eh_pending()  ; 0x40160c
  %eh.thrown.14 = icmp ne i64 %eh.pending.14, 0  ; 0x40160c
  br i1 %eh.thrown.14, label %eh.landing.1, label %eh.ok.40160c
eh.ok.40160c:  ; 0x40160c
  %sum.36 = add i32 %sarg0.a.29, 4  ; 0x401611
  %addr.42 = add i32 %sum.35, -32  ; 0x401614
  %addr.z.202 = zext i32 %addr.42 to i64  ; 0x401614
  %m.p.17 = inttoptr i64 %addr.z.202 to ptr  ; 0x401614
  %m.17 = load i32, ptr %m.p.17, align 1  ; 0x401614
  %addr.43 = add i32 %sum.35, -36  ; 0x401617
  %addr.z.203 = zext i32 %addr.43 to i64  ; 0x401617
  %st.p.141 = inttoptr i64 %addr.z.203 to ptr  ; 0x401617
  store i32 %m.17, ptr %st.p.141, align 1  ; 0x401617
  %sum.37 = add i32 %sum.36, 8  ; 0x40161a
  %sp.98 = add i32 %sum.37, 4  ; 0x40161d
  %sp.99 = add i32 %sp.98, 4  ; 0x40161e
  br label %eh.cleaned.401600
eh.destroy.401540.2:  ; 0x40161e
  %eh.this.4 = trunc i64 %eh.ending.4 to i32  ; 0x40161e
  %ecx.z.5 = zext i32 %eh.this.4 to i64  ; 0x40161e
  %sp.100 = sub i32 %sp.99, 4  ; 0x40161e
  %addr.z.207 = zext i32 %sp.100 to i64  ; 0x40161e
  %st.p.143 = inttoptr i64 %addr.z.207 to ptr  ; 0x40161e
  store i32 0, ptr %st.p.143, align 1  ; 0x40161e
  %fp.7 = inttoptr i64 %eh.dtor.4 to ptr  ; 0x40161e
  %sarg0.a.30 = add i64 %addr.z.207, 4
  %sarg0.p.30 = inttoptr i64 %sarg0.a.30 to ptr
  %sarg0.d.30 = load i32, ptr %sarg0.p.30, align 4
  %sarg0.30 = zext i32 %sarg0.d.30 to i64
  %sarg1.a.7 = add i64 %addr.z.207, 8
  %sarg1.p.7 = inttoptr i64 %sarg1.a.7 to ptr
  %sarg1.d.7 = load i32, ptr %sarg1.p.7, align 4
  %sarg1.7 = zext i32 %sarg1.d.7 to i64
  %sarg2.a.7 = add i64 %addr.z.207, 12
  %sarg2.p.7 = inttoptr i64 %sarg2.a.7 to ptr
  %sarg2.d.7 = load i32, ptr %sarg2.p.7, align 4
  %sarg2.7 = zext i32 %sarg2.d.7 to i64
  %sarg3.a.7 = add i64 %addr.z.207, 16
  %sarg3.p.7 = inttoptr i64 %sarg3.a.7 to ptr
  %sarg3.d.7 = load i32, ptr %sarg3.p.7, align 4
  %sarg3.7 = zext i32 %sarg3.d.7 to i64
  %sarg4.a.7 = add i64 %addr.z.207, 20
  %sarg4.p.7 = inttoptr i64 %sarg4.a.7 to ptr
  %sarg4.d.7 = load i32, ptr %sarg4.p.7, align 4
  %sarg4.7 = zext i32 %sarg4.d.7 to i64
  %sarg5.a.7 = add i64 %addr.z.207, 24
  %sarg5.p.7 = inttoptr i64 %sarg5.a.7 to ptr
  %sarg5.d.7 = load i32, ptr %sarg5.p.7, align 4
  %sarg5.7 = zext i32 %sarg5.d.7 to i64
  %sarg6.a.7 = add i64 %addr.z.207, 28
  %sarg6.p.7 = inttoptr i64 %sarg6.a.7 to ptr
  %sarg6.d.7 = load i32, ptr %sarg6.p.7, align 4
  %sarg6.7 = zext i32 %sarg6.d.7 to i64
  %args.p.14 = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr  ; 0x40161e
  store i64 %sarg6.7, ptr %args.p.14, align 8  ; 0x40161e
  %sarg7.a.7 = add i64 %addr.z.207, 32
  %sarg7.p.7 = inttoptr i64 %sarg7.a.7 to ptr
  %sarg7.d.7 = load i32, ptr %sarg7.p.7, align 4
  %sarg7.7 = zext i32 %sarg7.d.7 to i64
  %args.p.15 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr  ; 0x40161e
  store i64 %sarg7.7, ptr %args.p.15, align 8  ; 0x40161e
  %ret.30 = call i64 %fp.7(i64 %sarg0.30, i64 %sarg1.7, i64 %sarg2.7, i64 %sarg3.7, i64 %sarg4.7, i64 %sarg5.7, i64 %ecx.z.5, i64 undef)  ; 0x40161e
  %pops.7 = call i64 @ra_pops_of(i64 %eh.dtor.4)  ; 0x40161e
  %n.6 = trunc i64 %pops.7 to i32
  %pops.a.7 = add i32 %n.6, 4  ; 0x40161e
  %rsp.354 = add i32 %sp.100, %pops.a.7  ; 0x40161e
  br label %eh.destroyed.401540.2
eh.destroyed.401540.2:  ; 0x40161e
  %rsp.phi.8 = phi i32 [ %sp.99, %eh.cleaned.401600 ], [ %rsp.354, %eh.destroy.401540.2 ]  ; 0x40161e
  %eh.head.1.f8.30 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x40161e
  store i32 0, ptr %eh.head.1.f8.30, align 1  ; 0x40161e
  %sp.101 = sub i32 %rsp.phi.8, 4  ; 0x40161e
  %addr.z.209 = zext i32 %sp.101 to i64  ; 0x40161e
  %st.p.145 = inttoptr i64 %addr.z.209 to ptr  ; 0x40161e
  store i32 0, ptr %st.p.145, align 1  ; 0x40161e
  br label %f17_401620
eh.cleaned.401620.9:  ; 0x40161e
  %eh.head.1.f8.31 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x40163b
  store i32 0, ptr %eh.head.1.f8.31, align 1  ; 0x40163b
  br label %eh.enter1_0.1
f17_401620:  ; 0x401620
  %sp.102 = sub i32 %sp.101, 4  ; 0x401620
  %addr.z.210 = zext i32 %sp.102 to i64  ; 0x401620
  %st.p.146 = inttoptr i64 %addr.z.210 to ptr  ; 0x401620
  store i32 %eh.frame.26, ptr %st.p.146, align 1  ; 0x401620
  %diff.17 = sub i32 %sp.102, 8  ; 0x401621
  %m.18 = load i32, ptr %st.p.141, align 1  ; 0x401627
  store i32 %m.18, ptr %m.p.17, align 1  ; 0x40162a
  %sp.103 = sub i32 %diff.17, 4  ; 0x40162d
  %addr.z.213 = zext i32 %sp.103 to i64  ; 0x40162d
  %st.p.148 = inttoptr i64 %addr.z.213 to ptr  ; 0x40162d
  store i32 220, ptr %st.p.148, align 1  ; 0x40162d
  %sp.104 = sub i32 %sp.103, 4  ; 0x401632
  %sarg0.a.31 = add i32 %sp.104, 4
  %ret.31 = call i64 @F_00401000(i32 220)  ; 0x401632
  %eh.pending.15 = call i64 @ra_eh_pending()  ; 0x401632
  %eh.thrown.15 = icmp ne i64 %eh.pending.15, 0  ; 0x401632
  br i1 %eh.thrown.15, label %eh.landing.1, label %eh.ok.401632.8
eh.ok.401632.8:  ; 0x401632
  %sum.39 = add i32 %sarg0.a.31, 12  ; 0x401637
  %sp.105 = add i32 %sum.39, 4  ; 0x40163a
  %sp.106 = add i32 %sp.105, 4  ; 0x40163b
  br label %eh.cleaned.401620.9
eh.cleaned.401600.1:  ; 0x40163b
  %eh.esp.saved.6 = call i64 @ra_eh_ending_sp()  ; 0x40161e
  %eh.esp.saved.7 = trunc i64 %eh.esp.saved.6 to i32  ; 0x40161e
  %eh.esp.p.11 = sub i32 %eh.reg.1, 4  ; 0x40161e
  %addr.z.227 = zext i32 %eh.esp.p.11 to i64  ; 0x40161e
  %st.p.157 = inttoptr i64 %addr.z.227 to ptr  ; 0x40161e
  store i32 %eh.esp.saved.7, ptr %st.p.157, align 1  ; 0x40161e
  %eh.ending.5 = call i64 @ra_eh_ending_object()  ; 0x40161e
  %eh.dtor.5 = call i64 @ra_eh_ending_destructor()  ; 0x40161e
  call void @ra_eh_leave_catch()  ; 0x40161e
  %eh.dies.5 = icmp ne i64 %eh.dtor.5, 0  ; 0x40161e
  br i1 %eh.dies.5, label %eh.destroy.401540.3, label %eh.destroyed.401540.3
f18_401600:  ; 0x401600
  %sp.108 = sub i32 %sp.107, 4  ; 0x401600
  %addr.z.220 = zext i32 %sp.108 to i64  ; 0x401600
  %st.p.153 = inttoptr i64 %addr.z.220 to ptr  ; 0x401600
  store i32 %eh.frame.26, ptr %st.p.153, align 1  ; 0x401600
  %diff.18 = sub i32 %sp.108, 8  ; 0x401601
  %sum.40 = add i32 %eh.frame.26, 12  ; 0x401604
  %sp.109 = sub i32 %diff.18, 4  ; 0x401607
  %addr.z.221 = zext i32 %sp.109 to i64  ; 0x401607
  %st.p.154 = inttoptr i64 %addr.z.221 to ptr  ; 0x401607
  store i32 222, ptr %st.p.154, align 1  ; 0x401607
  %sp.110 = sub i32 %sp.109, 4  ; 0x40160c
  %sarg0.a.26 = add i32 %sp.110, 4
  %ret.26 = call i64 @F_00401000(i32 222)  ; 0x40160c
  %eh.pending.16 = call i64 @ra_eh_pending()  ; 0x40160c
  %eh.thrown.16 = icmp ne i64 %eh.pending.16, 0  ; 0x40160c
  br i1 %eh.thrown.16, label %eh.landing.1, label %eh.ok.40160c.1
eh.ok.40160c.1:  ; 0x40160c
  %sum.41 = add i32 %sarg0.a.26, 4  ; 0x401611
  %addr.46 = add i32 %sum.40, -32  ; 0x401614
  %addr.z.223 = zext i32 %addr.46 to i64  ; 0x401614
  %m.p.19 = inttoptr i64 %addr.z.223 to ptr  ; 0x401614
  %m.19 = load i32, ptr %m.p.19, align 1  ; 0x401614
  %addr.47 = add i32 %sum.40, -36  ; 0x401617
  %addr.z.224 = zext i32 %addr.47 to i64  ; 0x401617
  %st.p.156 = inttoptr i64 %addr.z.224 to ptr  ; 0x401617
  store i32 %m.19, ptr %st.p.156, align 1  ; 0x401617
  %sum.42 = add i32 %sum.41, 8  ; 0x40161a
  %sp.111 = add i32 %sum.42, 4  ; 0x40161d
  %sp.112 = add i32 %sp.111, 4  ; 0x40161e
  br label %eh.cleaned.401600.1
eh.destroy.401540.3:  ; 0x40161e
  %eh.this.5 = trunc i64 %eh.ending.5 to i32  ; 0x40161e
  %ecx.z.6 = zext i32 %eh.this.5 to i64  ; 0x40161e
  %sp.113 = sub i32 %sp.112, 4  ; 0x40161e
  %addr.z.228 = zext i32 %sp.113 to i64  ; 0x40161e
  %st.p.158 = inttoptr i64 %addr.z.228 to ptr  ; 0x40161e
  store i32 0, ptr %st.p.158, align 1  ; 0x40161e
  %fp.6 = inttoptr i64 %eh.dtor.5 to ptr  ; 0x40161e
  %sarg0.a.27 = add i64 %addr.z.228, 4
  %sarg0.p.27 = inttoptr i64 %sarg0.a.27 to ptr
  %sarg0.d.27 = load i32, ptr %sarg0.p.27, align 4
  %sarg0.27 = zext i32 %sarg0.d.27 to i64
  %sarg1.a.6 = add i64 %addr.z.228, 8
  %sarg1.p.6 = inttoptr i64 %sarg1.a.6 to ptr
  %sarg1.d.6 = load i32, ptr %sarg1.p.6, align 4
  %sarg1.6 = zext i32 %sarg1.d.6 to i64
  %sarg2.a.6 = add i64 %addr.z.228, 12
  %sarg2.p.6 = inttoptr i64 %sarg2.a.6 to ptr
  %sarg2.d.6 = load i32, ptr %sarg2.p.6, align 4
  %sarg2.6 = zext i32 %sarg2.d.6 to i64
  %sarg3.a.6 = add i64 %addr.z.228, 16
  %sarg3.p.6 = inttoptr i64 %sarg3.a.6 to ptr
  %sarg3.d.6 = load i32, ptr %sarg3.p.6, align 4
  %sarg3.6 = zext i32 %sarg3.d.6 to i64
  %sarg4.a.6 = add i64 %addr.z.228, 20
  %sarg4.p.6 = inttoptr i64 %sarg4.a.6 to ptr
  %sarg4.d.6 = load i32, ptr %sarg4.p.6, align 4
  %sarg4.6 = zext i32 %sarg4.d.6 to i64
  %sarg5.a.6 = add i64 %addr.z.228, 24
  %sarg5.p.6 = inttoptr i64 %sarg5.a.6 to ptr
  %sarg5.d.6 = load i32, ptr %sarg5.p.6, align 4
  %sarg5.6 = zext i32 %sarg5.d.6 to i64
  %sarg6.a.6 = add i64 %addr.z.228, 28
  %sarg6.p.6 = inttoptr i64 %sarg6.a.6 to ptr
  %sarg6.d.6 = load i32, ptr %sarg6.p.6, align 4
  %sarg6.6 = zext i32 %sarg6.d.6 to i64
  %args.p.12 = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr  ; 0x40161e
  store i64 %sarg6.6, ptr %args.p.12, align 8  ; 0x40161e
  %sarg7.a.6 = add i64 %addr.z.228, 32
  %sarg7.p.6 = inttoptr i64 %sarg7.a.6 to ptr
  %sarg7.d.6 = load i32, ptr %sarg7.p.6, align 4
  %sarg7.6 = zext i32 %sarg7.d.6 to i64
  %args.p.13 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr  ; 0x40161e
  store i64 %sarg7.6, ptr %args.p.13, align 8  ; 0x40161e
  %ret.27 = call i64 %fp.6(i64 %sarg0.27, i64 %sarg1.6, i64 %sarg2.6, i64 %sarg3.6, i64 %sarg4.6, i64 %sarg5.6, i64 %ecx.z.6, i64 undef)  ; 0x40161e
  %pops.6 = call i64 @ra_pops_of(i64 %eh.dtor.5)  ; 0x40161e
  %n.8 = trunc i64 %pops.6 to i32
  %pops.a.6 = add i32 %n.8, 4  ; 0x40161e
  %rsp.351 = add i32 %sp.113, %pops.a.6  ; 0x40161e
  br label %eh.destroyed.401540.3
eh.destroyed.401540.3:  ; 0x40161e
  %rsp.phi.7 = phi i32 [ %sp.112, %eh.cleaned.401600.1 ], [ %rsp.351, %eh.destroy.401540.3 ]  ; 0x40161e
  %eh.head.1.f8.32 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x40161e
  store i32 0, ptr %eh.head.1.f8.32, align 1  ; 0x40161e
  %sp.114 = sub i32 %rsp.phi.7, 4  ; 0x40161e
  %addr.z.230 = zext i32 %sp.114 to i64  ; 0x40161e
  %st.p.160 = inttoptr i64 %addr.z.230 to ptr  ; 0x40161e
  store i32 0, ptr %st.p.160, align 1  ; 0x40161e
  br label %f19_401620
eh.cleaned.401620.10:  ; 0x40161e
  %eh.head.1.f8.33 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x40163b
  store i32 -1, ptr %eh.head.1.f8.33, align 1  ; 0x40163b
  call void @ra_eh_resume()  ; 0x40163b
  %eh.head.1.f0.6 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 0  ; 0x40163b
  %eh.next.8 = load i32, ptr %eh.head.1.f0.6, align 1  ; 0x40163b
  %teb.24 = call i64 @ra_teb()  ; 0x40163b
  %teb.25 = trunc i64 %teb.24 to i32  ; 0x40163b
  %addr.z.240 = zext i32 %teb.25 to i64  ; 0x40163b
  %st.p.166 = inttoptr i64 %addr.z.240 to ptr  ; 0x40163b
  store i32 %eh.next.8, ptr %st.p.166, align 1  ; 0x40163b
  ret i64 %ret.28
f19_401620:  ; 0x401620
  %sp.115 = sub i32 %sp.114, 4  ; 0x401620
  %addr.z.231 = zext i32 %sp.115 to i64  ; 0x401620
  %st.p.161 = inttoptr i64 %addr.z.231 to ptr  ; 0x401620
  store i32 %eh.frame.26, ptr %st.p.161, align 1  ; 0x401620
  %diff.19 = sub i32 %sp.115, 8  ; 0x401621
  %m.20 = load i32, ptr %st.p.156, align 1  ; 0x401627
  store i32 %m.20, ptr %m.p.19, align 1  ; 0x40162a
  %sp.116 = sub i32 %diff.19, 4  ; 0x40162d
  %addr.z.234 = zext i32 %sp.116 to i64  ; 0x40162d
  %st.p.163 = inttoptr i64 %addr.z.234 to ptr  ; 0x40162d
  store i32 220, ptr %st.p.163, align 1  ; 0x40162d
  %sp.117 = sub i32 %sp.116, 4  ; 0x401632
  %sarg0.a.28 = add i32 %sp.117, 4
  %ret.28 = call i64 @F_00401000(i32 220)  ; 0x401632
  %eh.pending.17 = call i64 @ra_eh_pending()  ; 0x401632
  %eh.thrown.17 = icmp ne i64 %eh.pending.17, 0  ; 0x401632
  br i1 %eh.thrown.17, label %eh.landing.1, label %eh.ok.401632.9
eh.ok.401632.9:  ; 0x401632
  br label %eh.cleaned.401620.10
eh.state6:  ; 0x40163b
  call void @ra_eh_suspend()  ; 0x40163b
  %eh.esp.saved.8 = call i64 @ra_eh_ending_sp()  ; 0x40163b
  %eh.esp.saved.9 = trunc i64 %eh.esp.saved.8 to i32  ; 0x40163b
  %eh.esp.p.12 = sub i32 %eh.reg.1, 4  ; 0x40163b
  %addr.z.241 = zext i32 %eh.esp.p.12 to i64  ; 0x40163b
  %st.p.167 = inttoptr i64 %addr.z.241 to ptr  ; 0x40163b
  store i32 %eh.esp.saved.9, ptr %st.p.167, align 1  ; 0x40163b
  %eh.ending.6 = call i64 @ra_eh_ending_object()  ; 0x40163b
  %eh.dtor.6 = call i64 @ra_eh_ending_destructor()  ; 0x40163b
  call void @ra_eh_leave_catch()  ; 0x40163b
  %eh.dies.6 = icmp ne i64 %eh.dtor.6, 0  ; 0x40163b
  br i1 %eh.dies.6, label %eh.destroy.401540.4, label %eh.destroyed.401540.4
eh.destroy.401540.4:  ; 0x40163b
  %eh.this.6 = trunc i64 %eh.ending.6 to i32  ; 0x40163b
  %ecx.z.7 = zext i32 %eh.this.6 to i64  ; 0x40163b
  %sp.120 = sub i32 %rsp.phi.6, 4  ; 0x40163b
  %addr.z.242 = zext i32 %sp.120 to i64  ; 0x40163b
  %st.p.168 = inttoptr i64 %addr.z.242 to ptr  ; 0x40163b
  store i32 0, ptr %st.p.168, align 1  ; 0x40163b
  %fp.5 = inttoptr i64 %eh.dtor.6 to ptr  ; 0x40163b
  %sarg0.a.25 = add i64 %addr.z.242, 4
  %sarg0.p.25 = inttoptr i64 %sarg0.a.25 to ptr
  %sarg0.d.25 = load i32, ptr %sarg0.p.25, align 4
  %sarg0.25 = zext i32 %sarg0.d.25 to i64
  %sarg1.a.5 = add i64 %addr.z.242, 8
  %sarg1.p.5 = inttoptr i64 %sarg1.a.5 to ptr
  %sarg1.d.5 = load i32, ptr %sarg1.p.5, align 4
  %sarg1.5 = zext i32 %sarg1.d.5 to i64
  %sarg2.a.5 = add i64 %addr.z.242, 12
  %sarg2.p.5 = inttoptr i64 %sarg2.a.5 to ptr
  %sarg2.d.5 = load i32, ptr %sarg2.p.5, align 4
  %sarg2.5 = zext i32 %sarg2.d.5 to i64
  %sarg3.a.5 = add i64 %addr.z.242, 16
  %sarg3.p.5 = inttoptr i64 %sarg3.a.5 to ptr
  %sarg3.d.5 = load i32, ptr %sarg3.p.5, align 4
  %sarg3.5 = zext i32 %sarg3.d.5 to i64
  %sarg4.a.5 = add i64 %addr.z.242, 20
  %sarg4.p.5 = inttoptr i64 %sarg4.a.5 to ptr
  %sarg4.d.5 = load i32, ptr %sarg4.p.5, align 4
  %sarg4.5 = zext i32 %sarg4.d.5 to i64
  %sarg5.a.5 = add i64 %addr.z.242, 24
  %sarg5.p.5 = inttoptr i64 %sarg5.a.5 to ptr
  %sarg5.d.5 = load i32, ptr %sarg5.p.5, align 4
  %sarg5.5 = zext i32 %sarg5.d.5 to i64
  %sarg6.a.5 = add i64 %addr.z.242, 28
  %sarg6.p.5 = inttoptr i64 %sarg6.a.5 to ptr
  %sarg6.d.5 = load i32, ptr %sarg6.p.5, align 4
  %sarg6.5 = zext i32 %sarg6.d.5 to i64
  %args.p.10 = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr  ; 0x40163b
  store i64 %sarg6.5, ptr %args.p.10, align 8  ; 0x40163b
  %sarg7.a.5 = add i64 %addr.z.242, 32
  %sarg7.p.5 = inttoptr i64 %sarg7.a.5 to ptr
  %sarg7.d.5 = load i32, ptr %sarg7.p.5, align 4
  %sarg7.5 = zext i32 %sarg7.d.5 to i64
  %args.p.11 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr  ; 0x40163b
  store i64 %sarg7.5, ptr %args.p.11, align 8  ; 0x40163b
  %ret.25 = call i64 %fp.5(i64 %sarg0.25, i64 %sarg1.5, i64 %sarg2.5, i64 %sarg3.5, i64 %sarg4.5, i64 %sarg5.5, i64 %ecx.z.7, i64 undef)  ; 0x40163b
  %pops.5 = call i64 @ra_pops_of(i64 %eh.dtor.6)  ; 0x40163b
  br label %eh.destroyed.401540.4
eh.destroyed.401540.4:  ; 0x40163b
  %rax.phi.3 = phi i64 [ %rax.phi.2, %eh.state6 ], [ %ret.25, %eh.destroy.401540.4 ]  ; 0x40163b
  %eh.head.1.f8.34 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 2  ; 0x40163b
  store i32 -1, ptr %eh.head.1.f8.34, align 1  ; 0x40163b
  call void @ra_eh_resume()  ; 0x40163b
  %eh.head.1.f0.7 = getelementptr inbounds %S0, ptr %eh.head.1.s, i64 0, i32 0  ; 0x40163b
  %eh.next.9 = load i32, ptr %eh.head.1.f0.7, align 1  ; 0x40163b
  %teb.26 = call i64 @ra_teb()  ; 0x40163b
  %teb.27 = trunc i64 %teb.26 to i32  ; 0x40163b
  %addr.z.245 = zext i32 %teb.27 to i64  ; 0x40163b
  %st.p.170 = inttoptr i64 %addr.z.245 to ptr  ; 0x40163b
  store i32 %eh.next.9, ptr %st.p.170, align 1  ; 0x40163b
  ret i64 %rax.phi.3
eh.ok.401632.10:  ; 0x40163b
  %sum.45 = add i32 %sarg0.a.24, 12  ; 0x401637
  %sp.121 = add i32 %sum.45, 4  ; 0x40163a
  %sp.122 = add i32 %sp.121, 4  ; 0x40163b
  br label %eh.cleaned.401620
eh.cleaned.401620.11:  ; 0x40163b
  %eh.head.f8.8 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x40163b
  store i32 -1, ptr %eh.head.f8.8, align 1  ; 0x40163b
  call void @ra_eh_resume()  ; 0x40163b
  %eh.head.f0.2 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 0  ; 0x40163b
  %eh.next.10 = load i32, ptr %eh.head.f0.2, align 1  ; 0x40163b
  %teb.28 = call i64 @ra_teb()  ; 0x40163b
  %teb.29 = trunc i64 %teb.28 to i32  ; 0x40163b
  %addr.z.260 = zext i32 %teb.29 to i64  ; 0x40163b
  %st.p.179 = inttoptr i64 %addr.z.260 to ptr  ; 0x40163b
  store i32 %eh.next.10, ptr %st.p.179, align 1  ; 0x40163b
  ret i64 %ret.23
f20_401620:  ; 0x401620
  %sp.124 = sub i32 %sp.123, 4  ; 0x401620
  %addr.z.251 = zext i32 %sp.124 to i64  ; 0x401620
  %st.p.174 = inttoptr i64 %addr.z.251 to ptr  ; 0x401620
  store i32 %eh.frame.3, ptr %st.p.174, align 1  ; 0x401620
  %diff.20 = sub i32 %sp.124, 8  ; 0x401621
  %sum.46 = add i32 %eh.frame.3, 12  ; 0x401624
  %addr.50 = add i32 %sum.46, -36  ; 0x401627
  %addr.z.252 = zext i32 %addr.50 to i64  ; 0x401627
  %m.p.21 = inttoptr i64 %addr.z.252 to ptr  ; 0x401627
  %m.21 = load i32, ptr %m.p.21, align 1  ; 0x401627
  %addr.51 = add i32 %sum.46, -32  ; 0x40162a
  %addr.z.253 = zext i32 %addr.51 to i64  ; 0x40162a
  %st.p.175 = inttoptr i64 %addr.z.253 to ptr  ; 0x40162a
  store i32 %m.21, ptr %st.p.175, align 1  ; 0x40162a
  %sp.125 = sub i32 %diff.20, 4  ; 0x40162d
  %addr.z.254 = zext i32 %sp.125 to i64  ; 0x40162d
  %st.p.176 = inttoptr i64 %addr.z.254 to ptr  ; 0x40162d
  store i32 220, ptr %st.p.176, align 1  ; 0x40162d
  %sp.126 = sub i32 %sp.125, 4  ; 0x401632
  %sarg0.a.23 = add i32 %sp.126, 4
  %ret.23 = call i64 @F_00401000(i32 220)  ; 0x401632
  %eh.pending.18 = call i64 @ra_eh_pending()  ; 0x401632
  %eh.thrown.18 = icmp ne i64 %eh.pending.18, 0  ; 0x401632
  br i1 %eh.thrown.18, label %eh.landing.1, label %eh.ok.401632.11
eh.ok.401632.11:  ; 0x401632
  br label %eh.cleaned.401620.11
eh.state2.1:  ; 0x40163b
  %eh.frame.32 = add i32 %eh.reg, 12  ; 0x40163b
  %eh.catch.obj.20 = add i32 %eh.frame.32, -32  ; 0x40163b
  %eh.catch.obj.21 = zext i32 %eh.catch.obj.20 to i64  ; 0x40163b
  %eh.caught.10 = call i64 @ra_eh_catches(i64 565891180120823400, i64 %eh.catch.obj.21, i64 8192)  ; 0x40163b
  %eh.hit.10 = icmp ne i64 %eh.caught.10, 0  ; 0x40163b
  br i1 %eh.hit.10, label %eh.catch0_0.s2.1, label %eh.next0_0.s2.1
eh.catch0_0.s2.1:  ; 0x40163b
  %eh.head.f8.9 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x40163b
  store i32 2, ptr %eh.head.f8.9, align 1  ; 0x40163b
  br label %eh.enter0_0.1
eh.next0_0.s2.1:  ; 0x40163b
  %eh.catch.obj.22 = add i32 %eh.frame.32, -40  ; 0x4016a2
  %eh.catch.obj.23 = zext i32 %eh.catch.obj.22 to i64  ; 0x4016a2
  %eh.caught.11 = call i64 @ra_eh_catches(i64 565892279632451611, i64 %eh.catch.obj.23, i64 8192)  ; 0x4016a2
  %eh.hit.11 = icmp ne i64 %eh.caught.11, 0  ; 0x4016a2
  br i1 %eh.hit.11, label %eh.catch1_0.s2.1, label %eh.next1_0.s2.1
eh.enter0_0.1:  ; 0x40163b
  %rsp.phi.5 = phi i32 [ %rsp.phi, %eh.catch0_0.s2.1 ], [ %sp.152, %eh.cleaned.4015e0.3 ]  ; 0x40163b
  call void @ra_eh_enter_catch()  ; 0x40163b
  %eh.head.f8.10 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x40163b
  store i32 4, ptr %eh.head.f8.10, align 1  ; 0x40163b
  %eh.esp.p.13 = sub i32 %eh.reg, 4  ; 0x40163b
  %addr.z.263 = zext i32 %eh.esp.p.13 to i64  ; 0x40163b
  %eh.esp.p.14 = inttoptr i64 %addr.z.263 to ptr  ; 0x40163b
  %eh.esp.6 = load i32, ptr %eh.esp.p.14, align 1  ; 0x40163b
  %eh.esp.7 = zext i32 %eh.esp.6 to i64  ; 0x40163b
  call void @ra_eh_catch_sp(i64 %eh.esp.7)  ; 0x40163b
  %eh.frame.33 = add i32 %eh.reg, 12  ; 0x40163b
  %sp.129 = sub i32 %rsp.phi.5, 4  ; 0x40163b
  %addr.z.264 = zext i32 %sp.129 to i64  ; 0x40163b
  %st.p.182 = inttoptr i64 %addr.z.264 to ptr  ; 0x40163b
  store i32 0, ptr %st.p.182, align 1  ; 0x40163b
  br label %f21_401670
f21_401670:  ; 0x401670
  %sp.130 = sub i32 %sp.129, 4  ; 0x401670
  %addr.z.265 = zext i32 %sp.130 to i64  ; 0x401670
  %st.p.183 = inttoptr i64 %addr.z.265 to ptr  ; 0x401670
  store i32 %eh.frame.33, ptr %st.p.183, align 1  ; 0x401670
  %diff.21 = sub i32 %sp.130, 8  ; 0x401671
  %sum.48 = add i32 %eh.frame.33, 12  ; 0x401674
  %addr.52 = add i32 %sum.48, -28  ; 0x401677
  %addr.z.266 = zext i32 %addr.52 to i64  ; 0x401677
  %st.p.184 = inttoptr i64 %addr.z.266 to ptr  ; 0x401677
  store i32 %diff.21, ptr %st.p.184, align 1  ; 0x401677
  %addr.53 = add i32 %sum.48, -44  ; 0x40167a
  %addr.z.267 = zext i32 %addr.53 to i64  ; 0x40167a
  %m.p.22 = inttoptr i64 %addr.z.267 to ptr  ; 0x40167a
  %m.22 = load i32, ptr %m.p.22, align 1  ; 0x40167a
  %addr.54 = add i32 %sum.48, -32  ; 0x40167d
  %addr.z.268 = zext i32 %addr.54 to i64  ; 0x40167d
  %st.p.185 = inttoptr i64 %addr.z.268 to ptr  ; 0x40167d
  store i32 %m.22, ptr %st.p.185, align 1  ; 0x40167d
  %sp.131 = sub i32 %diff.21, 4  ; 0x401680
  %addr.z.269 = zext i32 %sp.131 to i64  ; 0x401680
  %st.p.186 = inttoptr i64 %addr.z.269 to ptr  ; 0x401680
  store i32 122, ptr %st.p.186, align 1  ; 0x401680
  %sp.132 = sub i32 %sp.131, 4  ; 0x401682
  %sarg0.a.22 = add i32 %sp.132, 4
  %ret.22 = call i64 @F_00401000(i32 122)  ; 0x401682
  %eh.pending.19 = call i64 @ra_eh_pending()  ; 0x401682
  %eh.thrown.19 = icmp ne i64 %eh.pending.19, 0  ; 0x401682
  br i1 %eh.thrown.19, label %eh.landing.1, label %eh.ok.401682.1
eh.ok.401682.1:  ; 0x401682
  %sum.49 = add i32 %sarg0.a.22, 4  ; 0x401687
  %m.23 = load i32, ptr %m.p.22, align 1  ; 0x40168a
  %sum.50 = mul i32 %m.23, 2  ; 0x40168d
  %addr.56 = add i32 %sum.48, -40  ; 0x40168f
  %addr.z.272 = zext i32 %addr.56 to i64  ; 0x40168f
  %st.p.188 = inttoptr i64 %addr.z.272 to ptr  ; 0x40168f
  store i32 %sum.50, ptr %st.p.188, align 1  ; 0x40168f
  %eh.head.f8.11 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x401692
  store i32 5, ptr %eh.head.f8.11, align 1  ; 0x401692
  %sp.133 = sub i32 %sum.49, 4  ; 0x40169c
  %addr.z.274 = zext i32 %sp.133 to i64  ; 0x40169c
  %st.p.190 = inttoptr i64 %addr.z.274 to ptr  ; 0x40169c
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 2596) to i32), ptr %st.p.190, align 1  ; 0x40169c
  %sp.134 = sub i32 %sp.133, 4  ; 0x4016a1
  %addr.z.275 = zext i32 %sp.134 to i64  ; 0x4016a1
  %st.p.191 = inttoptr i64 %addr.z.275 to ptr  ; 0x4016a1
  store i32 %addr.56, ptr %st.p.191, align 1  ; 0x4016a1
  call void @ra_eh_throw(i64 %addr.z.272, i64 4)  ; 0x4016a2
  call void @ra_eh_throw_type(i64 565892279632451611, i64 0, i64 4)  ; 0x4016a2
  br label %eh.landing.1
eh.catch1_0.s2.1:  ; 0x4016a2
  %eh.head.f8.12 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x4016a2
  store i32 0, ptr %eh.head.f8.12, align 1  ; 0x4016a2
  %sp.135 = sub i32 %rsp.phi, 4  ; 0x4016a2
  %addr.z.278 = zext i32 %sp.135 to i64  ; 0x4016a2
  %st.p.193 = inttoptr i64 %addr.z.278 to ptr  ; 0x4016a2
  store i32 0, ptr %st.p.193, align 1  ; 0x4016a2
  br label %f22_401620
eh.next1_0.s2.1:  ; 0x4016a2
  call void @ra_eh_suspend()  ; 0x40163b
  %eh.head.f8.13 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x40163b
  store i32 0, ptr %eh.head.f8.13, align 1  ; 0x40163b
  %sp.141 = sub i32 %rsp.phi, 4  ; 0x40163b
  %addr.z.288 = zext i32 %sp.141 to i64  ; 0x40163b
  %st.p.200 = inttoptr i64 %addr.z.288 to ptr  ; 0x40163b
  store i32 0, ptr %st.p.200, align 1  ; 0x40163b
  br label %f23_401620
eh.cleaned.401620.12:  ; 0x4016a2
  %eh.head.f8.14 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x40163b
  store i32 0, ptr %eh.head.f8.14, align 1  ; 0x40163b
  br label %eh.enter1_0
f22_401620:  ; 0x401620
  %sp.136 = sub i32 %sp.135, 4  ; 0x401620
  %addr.z.279 = zext i32 %sp.136 to i64  ; 0x401620
  %st.p.194 = inttoptr i64 %addr.z.279 to ptr  ; 0x401620
  store i32 %eh.frame.32, ptr %st.p.194, align 1  ; 0x401620
  %diff.22 = sub i32 %sp.136, 8  ; 0x401621
  %sum.51 = add i32 %eh.frame.32, 12  ; 0x401624
  %addr.59 = add i32 %sum.51, -36  ; 0x401627
  %addr.z.280 = zext i32 %addr.59 to i64  ; 0x401627
  %m.p.24 = inttoptr i64 %addr.z.280 to ptr  ; 0x401627
  %m.24 = load i32, ptr %m.p.24, align 1  ; 0x401627
  %addr.60 = add i32 %sum.51, -32  ; 0x40162a
  %addr.z.281 = zext i32 %addr.60 to i64  ; 0x40162a
  %st.p.195 = inttoptr i64 %addr.z.281 to ptr  ; 0x40162a
  store i32 %m.24, ptr %st.p.195, align 1  ; 0x40162a
  %sp.137 = sub i32 %diff.22, 4  ; 0x40162d
  %addr.z.282 = zext i32 %sp.137 to i64  ; 0x40162d
  %st.p.196 = inttoptr i64 %addr.z.282 to ptr  ; 0x40162d
  store i32 220, ptr %st.p.196, align 1  ; 0x40162d
  %sp.138 = sub i32 %sp.137, 4  ; 0x401632
  %sarg0.a.21 = add i32 %sp.138, 4
  %ret.21 = call i64 @F_00401000(i32 220)  ; 0x401632
  %eh.pending.20 = call i64 @ra_eh_pending()  ; 0x401632
  %eh.thrown.20 = icmp ne i64 %eh.pending.20, 0  ; 0x401632
  br i1 %eh.thrown.20, label %eh.landing.1, label %eh.ok.401632.12
eh.ok.401632.12:  ; 0x401632
  %sum.52 = add i32 %sarg0.a.21, 12  ; 0x401637
  %sp.139 = add i32 %sum.52, 4  ; 0x40163a
  %sp.140 = add i32 %sp.139, 4  ; 0x40163b
  br label %eh.cleaned.401620.12
eh.cleaned.401620.13:  ; 0x40163b
  %eh.head.f8.15 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x40163b
  store i32 -1, ptr %eh.head.f8.15, align 1  ; 0x40163b
  call void @ra_eh_resume()  ; 0x40163b
  %eh.head.f0.3 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 0  ; 0x40163b
  %eh.next.11 = load i32, ptr %eh.head.f0.3, align 1  ; 0x40163b
  %teb.30 = call i64 @ra_teb()  ; 0x40163b
  %teb.31 = trunc i64 %teb.30 to i32  ; 0x40163b
  %addr.z.298 = zext i32 %teb.31 to i64  ; 0x40163b
  %st.p.206 = inttoptr i64 %addr.z.298 to ptr  ; 0x40163b
  store i32 %eh.next.11, ptr %st.p.206, align 1  ; 0x40163b
  ret i64 %ret.20
f23_401620:  ; 0x401620
  %sp.142 = sub i32 %sp.141, 4  ; 0x401620
  %addr.z.289 = zext i32 %sp.142 to i64  ; 0x401620
  %st.p.201 = inttoptr i64 %addr.z.289 to ptr  ; 0x401620
  store i32 %eh.frame.32, ptr %st.p.201, align 1  ; 0x401620
  %diff.23 = sub i32 %sp.142, 8  ; 0x401621
  %sum.53 = add i32 %eh.frame.32, 12  ; 0x401624
  %addr.61 = add i32 %sum.53, -36  ; 0x401627
  %addr.z.290 = zext i32 %addr.61 to i64  ; 0x401627
  %m.p.25 = inttoptr i64 %addr.z.290 to ptr  ; 0x401627
  %m.25 = load i32, ptr %m.p.25, align 1  ; 0x401627
  %addr.62 = add i32 %sum.53, -32  ; 0x40162a
  %addr.z.291 = zext i32 %addr.62 to i64  ; 0x40162a
  %st.p.202 = inttoptr i64 %addr.z.291 to ptr  ; 0x40162a
  store i32 %m.25, ptr %st.p.202, align 1  ; 0x40162a
  %sp.143 = sub i32 %diff.23, 4  ; 0x40162d
  %addr.z.292 = zext i32 %sp.143 to i64  ; 0x40162d
  %st.p.203 = inttoptr i64 %addr.z.292 to ptr  ; 0x40162d
  store i32 220, ptr %st.p.203, align 1  ; 0x40162d
  %sp.144 = sub i32 %sp.143, 4  ; 0x401632
  %sarg0.a.20 = add i32 %sp.144, 4
  %ret.20 = call i64 @F_00401000(i32 220)  ; 0x401632
  %eh.pending.21 = call i64 @ra_eh_pending()  ; 0x401632
  %eh.thrown.21 = icmp ne i64 %eh.pending.21, 0  ; 0x401632
  br i1 %eh.thrown.21, label %eh.landing.1, label %eh.ok.401632.13
eh.ok.401632.13:  ; 0x401632
  br label %eh.cleaned.401620.13
eh.state3.1:  ; 0x40163b
  %eh.frame.37 = add i32 %eh.reg, 12  ; 0x40163b
  %eh.catch.obj.24 = add i32 %eh.frame.37, -32  ; 0x40163b
  %eh.catch.obj.25 = zext i32 %eh.catch.obj.24 to i64  ; 0x40163b
  %eh.caught.12 = call i64 @ra_eh_catches(i64 565891180120823400, i64 %eh.catch.obj.25, i64 8192)  ; 0x40163b
  %eh.hit.12 = icmp ne i64 %eh.caught.12, 0  ; 0x40163b
  br i1 %eh.hit.12, label %eh.catch0_0.s3.1, label %eh.next0_0.s3.1
eh.catch0_0.s3.1:  ; 0x40163b
  %eh.head.f8.16 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x40163b
  store i32 2, ptr %eh.head.f8.16, align 1  ; 0x40163b
  %sp.147 = sub i32 %rsp.phi, 4  ; 0x40163b
  %addr.z.300 = zext i32 %sp.147 to i64  ; 0x40163b
  %st.p.208 = inttoptr i64 %addr.z.300 to ptr  ; 0x40163b
  store i32 0, ptr %st.p.208, align 1  ; 0x40163b
  br label %f24_4015e0
eh.next0_0.s3.1:  ; 0x40163b
  %eh.catch.obj.26 = add i32 %eh.frame.37, -40  ; 0x4015ff
  %eh.catch.obj.27 = zext i32 %eh.catch.obj.26 to i64  ; 0x4015ff
  %eh.caught.13 = call i64 @ra_eh_catches(i64 565892279632451611, i64 %eh.catch.obj.27, i64 8192)  ; 0x4015ff
  %eh.hit.13 = icmp ne i64 %eh.caught.13, 0  ; 0x4015ff
  br i1 %eh.hit.13, label %eh.catch1_0.s3.1, label %eh.next1_0.s3.1
eh.cleaned.4015e0.3:  ; 0x40163b
  %eh.head.f8.17 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x4015ff
  store i32 2, ptr %eh.head.f8.17, align 1  ; 0x4015ff
  br label %eh.enter0_0.1
f24_4015e0:  ; 0x4015e0
  %sp.148 = sub i32 %sp.147, 4  ; 0x4015e0
  %addr.z.301 = zext i32 %sp.148 to i64  ; 0x4015e0
  %st.p.209 = inttoptr i64 %addr.z.301 to ptr  ; 0x4015e0
  store i32 %eh.frame.37, ptr %st.p.209, align 1  ; 0x4015e0
  %diff.24 = sub i32 %sp.148, 8  ; 0x4015e1
  %sum.55 = add i32 %eh.frame.37, 12  ; 0x4015e4
  %sp.149 = sub i32 %diff.24, 4  ; 0x4015e7
  %addr.z.302 = zext i32 %sp.149 to i64  ; 0x4015e7
  %st.p.210 = inttoptr i64 %addr.z.302 to ptr  ; 0x4015e7
  store i32 221, ptr %st.p.210, align 1  ; 0x4015e7
  %sp.150 = sub i32 %sp.149, 4  ; 0x4015ec
  %sarg0.a.19 = add i32 %sp.150, 4
  %ret.19 = call i64 @F_00401000(i32 221)  ; 0x4015ec
  %eh.pending.22 = call i64 @ra_eh_pending()  ; 0x4015ec
  %eh.thrown.22 = icmp ne i64 %eh.pending.22, 0  ; 0x4015ec
  br i1 %eh.thrown.22, label %eh.landing.1, label %eh.ok.4015ec.3
eh.ok.4015ec.3:  ; 0x4015ec
  %sum.56 = add i32 %sarg0.a.19, 4  ; 0x4015f1
  %addr.63 = add i32 %sum.55, -36  ; 0x4015f4
  %addr.z.304 = zext i32 %addr.63 to i64  ; 0x4015f4
  %st.p.212 = inttoptr i64 %addr.z.304 to ptr  ; 0x4015f4
  store i32 0, ptr %st.p.212, align 1  ; 0x4015f4
  %sum.57 = add i32 %sum.56, 8  ; 0x4015fb
  %sp.151 = add i32 %sum.57, 4  ; 0x4015fe
  %sp.152 = add i32 %sp.151, 4  ; 0x4015ff
  br label %eh.cleaned.4015e0.3
eh.catch1_0.s3.1:  ; 0x4015ff
  %eh.head.f8.18 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x4015ff
  store i32 2, ptr %eh.head.f8.18, align 1  ; 0x4015ff
  %sp.153 = sub i32 %rsp.phi, 4  ; 0x4015ff
  %addr.z.309 = zext i32 %sp.153 to i64  ; 0x4015ff
  %st.p.215 = inttoptr i64 %addr.z.309 to ptr  ; 0x4015ff
  store i32 0, ptr %st.p.215, align 1  ; 0x4015ff
  br label %f25_4015e0
eh.next1_0.s3.1:  ; 0x4015ff
  call void @ra_eh_suspend()  ; 0x40163b
  %eh.head.f8.19 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x40163b
  store i32 2, ptr %eh.head.f8.19, align 1  ; 0x40163b
  %sp.165 = sub i32 %rsp.phi, 4  ; 0x40163b
  %addr.z.327 = zext i32 %sp.165 to i64  ; 0x40163b
  %st.p.228 = inttoptr i64 %addr.z.327 to ptr  ; 0x40163b
  store i32 0, ptr %st.p.228, align 1  ; 0x40163b
  br label %f27_4015e0
eh.cleaned.4015e0.4:  ; 0x4015ff
  %eh.head.f8.20 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x4015ff
  store i32 0, ptr %eh.head.f8.20, align 1  ; 0x4015ff
  %sp.159 = sub i32 %sp.158, 4  ; 0x4015ff
  %addr.z.317 = zext i32 %sp.159 to i64  ; 0x4015ff
  %st.p.221 = inttoptr i64 %addr.z.317 to ptr  ; 0x4015ff
  store i32 0, ptr %st.p.221, align 1  ; 0x4015ff
  br label %f26_401620
f25_4015e0:  ; 0x4015e0
  %sp.154 = sub i32 %sp.153, 4  ; 0x4015e0
  %addr.z.310 = zext i32 %sp.154 to i64  ; 0x4015e0
  %st.p.216 = inttoptr i64 %addr.z.310 to ptr  ; 0x4015e0
  store i32 %eh.frame.37, ptr %st.p.216, align 1  ; 0x4015e0
  %diff.25 = sub i32 %sp.154, 8  ; 0x4015e1
  %sum.58 = add i32 %eh.frame.37, 12  ; 0x4015e4
  %sp.155 = sub i32 %diff.25, 4  ; 0x4015e7
  %addr.z.311 = zext i32 %sp.155 to i64  ; 0x4015e7
  %st.p.217 = inttoptr i64 %addr.z.311 to ptr  ; 0x4015e7
  store i32 221, ptr %st.p.217, align 1  ; 0x4015e7
  %sp.156 = sub i32 %sp.155, 4  ; 0x4015ec
  %sarg0.a.17 = add i32 %sp.156, 4
  %ret.17 = call i64 @F_00401000(i32 221)  ; 0x4015ec
  %eh.pending.23 = call i64 @ra_eh_pending()  ; 0x4015ec
  %eh.thrown.23 = icmp ne i64 %eh.pending.23, 0  ; 0x4015ec
  br i1 %eh.thrown.23, label %eh.landing.1, label %eh.ok.4015ec.4
eh.ok.4015ec.4:  ; 0x4015ec
  %sum.59 = add i32 %sarg0.a.17, 4  ; 0x4015f1
  %addr.64 = add i32 %sum.58, -36  ; 0x4015f4
  %addr.z.313 = zext i32 %addr.64 to i64  ; 0x4015f4
  %st.p.219 = inttoptr i64 %addr.z.313 to ptr  ; 0x4015f4
  store i32 0, ptr %st.p.219, align 1  ; 0x4015f4
  %sum.60 = add i32 %sum.59, 8  ; 0x4015fb
  %sp.157 = add i32 %sum.60, 4  ; 0x4015fe
  %sp.158 = add i32 %sp.157, 4  ; 0x4015ff
  br label %eh.cleaned.4015e0.4
eh.cleaned.401620.14:  ; 0x4015ff
  %eh.head.f8.21 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x40163b
  store i32 0, ptr %eh.head.f8.21, align 1  ; 0x40163b
  br label %eh.enter1_0
f26_401620:  ; 0x401620
  %sp.160 = sub i32 %sp.159, 4  ; 0x401620
  %addr.z.318 = zext i32 %sp.160 to i64  ; 0x401620
  %st.p.222 = inttoptr i64 %addr.z.318 to ptr  ; 0x401620
  store i32 %eh.frame.37, ptr %st.p.222, align 1  ; 0x401620
  %diff.26 = sub i32 %sp.160, 8  ; 0x401621
  %m.26 = load i32, ptr %st.p.219, align 1  ; 0x401627
  %addr.66 = add i32 %sum.58, -32  ; 0x40162a
  %addr.z.320 = zext i32 %addr.66 to i64  ; 0x40162a
  %st.p.223 = inttoptr i64 %addr.z.320 to ptr  ; 0x40162a
  store i32 %m.26, ptr %st.p.223, align 1  ; 0x40162a
  %sp.161 = sub i32 %diff.26, 4  ; 0x40162d
  %addr.z.321 = zext i32 %sp.161 to i64  ; 0x40162d
  %st.p.224 = inttoptr i64 %addr.z.321 to ptr  ; 0x40162d
  store i32 220, ptr %st.p.224, align 1  ; 0x40162d
  %sp.162 = sub i32 %sp.161, 4  ; 0x401632
  %sarg0.a.18 = add i32 %sp.162, 4
  %ret.18 = call i64 @F_00401000(i32 220)  ; 0x401632
  %eh.pending.24 = call i64 @ra_eh_pending()  ; 0x401632
  %eh.thrown.24 = icmp ne i64 %eh.pending.24, 0  ; 0x401632
  br i1 %eh.thrown.24, label %eh.landing.1, label %eh.ok.401632.14
eh.ok.401632.14:  ; 0x401632
  %sum.62 = add i32 %sarg0.a.18, 12  ; 0x401637
  %sp.163 = add i32 %sum.62, 4  ; 0x40163a
  %sp.164 = add i32 %sp.163, 4  ; 0x40163b
  br label %eh.cleaned.401620.14
eh.cleaned.4015e0.5:  ; 0x40163b
  %eh.head.f8.22 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x4015ff
  store i32 0, ptr %eh.head.f8.22, align 1  ; 0x4015ff
  %sp.171 = sub i32 %sp.170, 4  ; 0x4015ff
  %addr.z.335 = zext i32 %sp.171 to i64  ; 0x4015ff
  %st.p.234 = inttoptr i64 %addr.z.335 to ptr  ; 0x4015ff
  store i32 0, ptr %st.p.234, align 1  ; 0x4015ff
  br label %f28_401620
f27_4015e0:  ; 0x4015e0
  %sp.166 = sub i32 %sp.165, 4  ; 0x4015e0
  %addr.z.328 = zext i32 %sp.166 to i64  ; 0x4015e0
  %st.p.229 = inttoptr i64 %addr.z.328 to ptr  ; 0x4015e0
  store i32 %eh.frame.37, ptr %st.p.229, align 1  ; 0x4015e0
  %diff.27 = sub i32 %sp.166, 8  ; 0x4015e1
  %sum.63 = add i32 %eh.frame.37, 12  ; 0x4015e4
  %sp.167 = sub i32 %diff.27, 4  ; 0x4015e7
  %addr.z.329 = zext i32 %sp.167 to i64  ; 0x4015e7
  %st.p.230 = inttoptr i64 %addr.z.329 to ptr  ; 0x4015e7
  store i32 221, ptr %st.p.230, align 1  ; 0x4015e7
  %sp.168 = sub i32 %sp.167, 4  ; 0x4015ec
  %sarg0.a.15 = add i32 %sp.168, 4
  %ret.15 = call i64 @F_00401000(i32 221)  ; 0x4015ec
  %eh.pending.25 = call i64 @ra_eh_pending()  ; 0x4015ec
  %eh.thrown.25 = icmp ne i64 %eh.pending.25, 0  ; 0x4015ec
  br i1 %eh.thrown.25, label %eh.landing.1, label %eh.ok.4015ec.5
eh.ok.4015ec.5:  ; 0x4015ec
  %sum.64 = add i32 %sarg0.a.15, 4  ; 0x4015f1
  %addr.67 = add i32 %sum.63, -36  ; 0x4015f4
  %addr.z.331 = zext i32 %addr.67 to i64  ; 0x4015f4
  %st.p.232 = inttoptr i64 %addr.z.331 to ptr  ; 0x4015f4
  store i32 0, ptr %st.p.232, align 1  ; 0x4015f4
  %sum.65 = add i32 %sum.64, 8  ; 0x4015fb
  %sp.169 = add i32 %sum.65, 4  ; 0x4015fe
  %sp.170 = add i32 %sp.169, 4  ; 0x4015ff
  br label %eh.cleaned.4015e0.5
eh.cleaned.401620.15:  ; 0x4015ff
  %eh.head.f8.23 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x40163b
  store i32 -1, ptr %eh.head.f8.23, align 1  ; 0x40163b
  call void @ra_eh_resume()  ; 0x40163b
  %eh.head.f0.4 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 0  ; 0x40163b
  %eh.next.12 = load i32, ptr %eh.head.f0.4, align 1  ; 0x40163b
  %teb.32 = call i64 @ra_teb()  ; 0x40163b
  %teb.33 = trunc i64 %teb.32 to i32  ; 0x40163b
  %addr.z.345 = zext i32 %teb.33 to i64  ; 0x40163b
  %st.p.240 = inttoptr i64 %addr.z.345 to ptr  ; 0x40163b
  store i32 %eh.next.12, ptr %st.p.240, align 1  ; 0x40163b
  ret i64 %ret.16
f28_401620:  ; 0x401620
  %sp.172 = sub i32 %sp.171, 4  ; 0x401620
  %addr.z.336 = zext i32 %sp.172 to i64  ; 0x401620
  %st.p.235 = inttoptr i64 %addr.z.336 to ptr  ; 0x401620
  store i32 %eh.frame.37, ptr %st.p.235, align 1  ; 0x401620
  %diff.28 = sub i32 %sp.172, 8  ; 0x401621
  %m.27 = load i32, ptr %st.p.232, align 1  ; 0x401627
  %addr.69 = add i32 %sum.63, -32  ; 0x40162a
  %addr.z.338 = zext i32 %addr.69 to i64  ; 0x40162a
  %st.p.236 = inttoptr i64 %addr.z.338 to ptr  ; 0x40162a
  store i32 %m.27, ptr %st.p.236, align 1  ; 0x40162a
  %sp.173 = sub i32 %diff.28, 4  ; 0x40162d
  %addr.z.339 = zext i32 %sp.173 to i64  ; 0x40162d
  %st.p.237 = inttoptr i64 %addr.z.339 to ptr  ; 0x40162d
  store i32 220, ptr %st.p.237, align 1  ; 0x40162d
  %sp.174 = sub i32 %sp.173, 4  ; 0x401632
  %sarg0.a.16 = add i32 %sp.174, 4
  %ret.16 = call i64 @F_00401000(i32 220)  ; 0x401632
  %eh.pending.26 = call i64 @ra_eh_pending()  ; 0x401632
  %eh.thrown.26 = icmp ne i64 %eh.pending.26, 0  ; 0x401632
  br i1 %eh.thrown.26, label %eh.landing.1, label %eh.ok.401632.15
eh.ok.401632.15:  ; 0x401632
  br label %eh.cleaned.401620.15
eh.state4.1:  ; 0x40163b
  %eh.frame.44 = add i32 %eh.reg, 12  ; 0x40163b
  %eh.catch.obj.28 = add i32 %eh.frame.44, -40  ; 0x40163b
  %eh.catch.obj.29 = zext i32 %eh.catch.obj.28 to i64  ; 0x40163b
  %eh.caught.14 = call i64 @ra_eh_catches(i64 565892279632451611, i64 %eh.catch.obj.29, i64 8192)  ; 0x40163b
  %eh.hit.14 = icmp ne i64 %eh.caught.14, 0  ; 0x40163b
  br i1 %eh.hit.14, label %eh.catch1_0.s4.1, label %eh.next1_0.s4.1
eh.catch1_0.s4.1:  ; 0x40163b
  %eh.esp.saved.10 = call i64 @ra_eh_ending_sp()  ; 0x40163b
  %eh.esp.saved.11 = trunc i64 %eh.esp.saved.10 to i32  ; 0x40163b
  %eh.esp.p.15 = sub i32 %eh.reg, 4  ; 0x40163b
  %addr.z.346 = zext i32 %eh.esp.p.15 to i64  ; 0x40163b
  %st.p.241 = inttoptr i64 %addr.z.346 to ptr  ; 0x40163b
  store i32 %eh.esp.saved.11, ptr %st.p.241, align 1  ; 0x40163b
  %eh.ending.7 = call i64 @ra_eh_ending_object()  ; 0x40163b
  %eh.dtor.7 = call i64 @ra_eh_ending_destructor()  ; 0x40163b
  call void @ra_eh_leave_catch()  ; 0x40163b
  %eh.dies.7 = icmp ne i64 %eh.dtor.7, 0  ; 0x40163b
  br i1 %eh.dies.7, label %eh.destroy.401540.5, label %eh.destroyed.401540.5
eh.next1_0.s4.1:  ; 0x40163b
  call void @ra_eh_suspend()  ; 0x40163b
  %eh.esp.saved.12 = call i64 @ra_eh_ending_sp()  ; 0x40163b
  %eh.esp.saved.13 = trunc i64 %eh.esp.saved.12 to i32  ; 0x40163b
  %eh.esp.p.16 = sub i32 %eh.reg, 4  ; 0x40163b
  %addr.z.358 = zext i32 %eh.esp.p.16 to i64  ; 0x40163b
  %st.p.250 = inttoptr i64 %addr.z.358 to ptr  ; 0x40163b
  store i32 %eh.esp.saved.13, ptr %st.p.250, align 1  ; 0x40163b
  %eh.ending.8 = call i64 @ra_eh_ending_object()  ; 0x40163b
  %eh.dtor.8 = call i64 @ra_eh_ending_destructor()  ; 0x40163b
  call void @ra_eh_leave_catch()  ; 0x40163b
  %eh.dies.8 = icmp ne i64 %eh.dtor.8, 0  ; 0x40163b
  br i1 %eh.dies.8, label %eh.destroy.401540.6, label %eh.destroyed.401540.6
eh.destroy.401540.5:  ; 0x40163b
  %eh.this.7 = trunc i64 %eh.ending.7 to i32  ; 0x40163b
  %ecx.z.8 = zext i32 %eh.this.7 to i64  ; 0x40163b
  %sp.177 = sub i32 %rsp.phi, 4  ; 0x40163b
  %addr.z.347 = zext i32 %sp.177 to i64  ; 0x40163b
  %st.p.242 = inttoptr i64 %addr.z.347 to ptr  ; 0x40163b
  store i32 0, ptr %st.p.242, align 1  ; 0x40163b
  %fp.4 = inttoptr i64 %eh.dtor.7 to ptr  ; 0x40163b
  %sarg0.a.13 = add i64 %addr.z.347, 4
  %sarg0.p.13 = inttoptr i64 %sarg0.a.13 to ptr
  %sarg0.d.13 = load i32, ptr %sarg0.p.13, align 4
  %sarg0.13 = zext i32 %sarg0.d.13 to i64
  %sarg1.a.4 = add i64 %addr.z.347, 8
  %sarg1.p.4 = inttoptr i64 %sarg1.a.4 to ptr
  %sarg1.d.4 = load i32, ptr %sarg1.p.4, align 4
  %sarg1.4 = zext i32 %sarg1.d.4 to i64
  %sarg2.a.4 = add i64 %addr.z.347, 12
  %sarg2.p.4 = inttoptr i64 %sarg2.a.4 to ptr
  %sarg2.d.4 = load i32, ptr %sarg2.p.4, align 4
  %sarg2.4 = zext i32 %sarg2.d.4 to i64
  %sarg3.a.4 = add i64 %addr.z.347, 16
  %sarg3.p.4 = inttoptr i64 %sarg3.a.4 to ptr
  %sarg3.d.4 = load i32, ptr %sarg3.p.4, align 4
  %sarg3.4 = zext i32 %sarg3.d.4 to i64
  %sarg4.a.4 = add i64 %addr.z.347, 20
  %sarg4.p.4 = inttoptr i64 %sarg4.a.4 to ptr
  %sarg4.d.4 = load i32, ptr %sarg4.p.4, align 4
  %sarg4.4 = zext i32 %sarg4.d.4 to i64
  %sarg5.a.4 = add i64 %addr.z.347, 24
  %sarg5.p.4 = inttoptr i64 %sarg5.a.4 to ptr
  %sarg5.d.4 = load i32, ptr %sarg5.p.4, align 4
  %sarg5.4 = zext i32 %sarg5.d.4 to i64
  %sarg6.a.4 = add i64 %addr.z.347, 28
  %sarg6.p.4 = inttoptr i64 %sarg6.a.4 to ptr
  %sarg6.d.4 = load i32, ptr %sarg6.p.4, align 4
  %sarg6.4 = zext i32 %sarg6.d.4 to i64
  %args.p.8 = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr  ; 0x40163b
  store i64 %sarg6.4, ptr %args.p.8, align 8  ; 0x40163b
  %sarg7.a.4 = add i64 %addr.z.347, 32
  %sarg7.p.4 = inttoptr i64 %sarg7.a.4 to ptr
  %sarg7.d.4 = load i32, ptr %sarg7.p.4, align 4
  %sarg7.4 = zext i32 %sarg7.d.4 to i64
  %args.p.9 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr  ; 0x40163b
  store i64 %sarg7.4, ptr %args.p.9, align 8  ; 0x40163b
  %ret.13 = call i64 %fp.4(i64 %sarg0.13, i64 %sarg1.4, i64 %sarg2.4, i64 %sarg3.4, i64 %sarg4.4, i64 %sarg5.4, i64 %ecx.z.8, i64 undef)  ; 0x40163b
  %pops.4 = call i64 @ra_pops_of(i64 %eh.dtor.7)  ; 0x40163b
  %n.10 = trunc i64 %pops.4 to i32
  %pops.a.4 = add i32 %n.10, 4  ; 0x40163b
  %rsp.337 = add i32 %sp.177, %pops.a.4  ; 0x40163b
  br label %eh.destroyed.401540.5
eh.destroyed.401540.5:  ; 0x40163b
  %rsp.phi.4 = phi i32 [ %rsp.phi, %eh.catch1_0.s4.1 ], [ %rsp.337, %eh.destroy.401540.5 ]  ; 0x40163b
  %eh.head.f8.24 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x40163b
  store i32 0, ptr %eh.head.f8.24, align 1  ; 0x40163b
  %sp.178 = sub i32 %rsp.phi.4, 4  ; 0x40163b
  %addr.z.349 = zext i32 %sp.178 to i64  ; 0x40163b
  %st.p.244 = inttoptr i64 %addr.z.349 to ptr  ; 0x40163b
  store i32 0, ptr %st.p.244, align 1  ; 0x40163b
  br label %f29_401620
eh.cleaned.401620.16:  ; 0x40163b
  %eh.head.f8.25 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x40163b
  store i32 0, ptr %eh.head.f8.25, align 1  ; 0x40163b
  br label %eh.enter1_0
f29_401620:  ; 0x401620
  %sp.179 = sub i32 %sp.178, 4  ; 0x401620
  %addr.z.350 = zext i32 %sp.179 to i64  ; 0x401620
  %st.p.245 = inttoptr i64 %addr.z.350 to ptr  ; 0x401620
  store i32 %eh.frame.44, ptr %st.p.245, align 1  ; 0x401620
  %diff.29 = sub i32 %sp.179, 8  ; 0x401621
  %sum.68 = add i32 %eh.frame.44, 12  ; 0x401624
  %addr.70 = add i32 %sum.68, -36  ; 0x401627
  %addr.z.351 = zext i32 %addr.70 to i64  ; 0x401627
  %m.p.28 = inttoptr i64 %addr.z.351 to ptr  ; 0x401627
  %m.28 = load i32, ptr %m.p.28, align 1  ; 0x401627
  %addr.71 = add i32 %sum.68, -32  ; 0x40162a
  %addr.z.352 = zext i32 %addr.71 to i64  ; 0x40162a
  %st.p.246 = inttoptr i64 %addr.z.352 to ptr  ; 0x40162a
  store i32 %m.28, ptr %st.p.246, align 1  ; 0x40162a
  %sp.180 = sub i32 %diff.29, 4  ; 0x40162d
  %addr.z.353 = zext i32 %sp.180 to i64  ; 0x40162d
  %st.p.247 = inttoptr i64 %addr.z.353 to ptr  ; 0x40162d
  store i32 220, ptr %st.p.247, align 1  ; 0x40162d
  %sp.181 = sub i32 %sp.180, 4  ; 0x401632
  %sarg0.a.14 = add i32 %sp.181, 4
  %ret.14 = call i64 @F_00401000(i32 220)  ; 0x401632
  %eh.pending.27 = call i64 @ra_eh_pending()  ; 0x401632
  %eh.thrown.27 = icmp ne i64 %eh.pending.27, 0  ; 0x401632
  br i1 %eh.thrown.27, label %eh.landing.1, label %eh.ok.401632.16
eh.ok.401632.16:  ; 0x401632
  %sum.69 = add i32 %sarg0.a.14, 12  ; 0x401637
  %sp.182 = add i32 %sum.69, 4  ; 0x40163a
  %sp.183 = add i32 %sp.182, 4  ; 0x40163b
  br label %eh.cleaned.401620.16
eh.destroy.401540.6:  ; 0x40163b
  %eh.this.8 = trunc i64 %eh.ending.8 to i32  ; 0x40163b
  %ecx.z.9 = zext i32 %eh.this.8 to i64  ; 0x40163b
  %sp.184 = sub i32 %rsp.phi, 4  ; 0x40163b
  %addr.z.359 = zext i32 %sp.184 to i64  ; 0x40163b
  %st.p.251 = inttoptr i64 %addr.z.359 to ptr  ; 0x40163b
  store i32 0, ptr %st.p.251, align 1  ; 0x40163b
  %fp.3 = inttoptr i64 %eh.dtor.8 to ptr  ; 0x40163b
  %sarg0.a.11 = add i64 %addr.z.359, 4
  %sarg0.p.11 = inttoptr i64 %sarg0.a.11 to ptr
  %sarg0.d.11 = load i32, ptr %sarg0.p.11, align 4
  %sarg0.11 = zext i32 %sarg0.d.11 to i64
  %sarg1.a.3 = add i64 %addr.z.359, 8
  %sarg1.p.3 = inttoptr i64 %sarg1.a.3 to ptr
  %sarg1.d.3 = load i32, ptr %sarg1.p.3, align 4
  %sarg1.3 = zext i32 %sarg1.d.3 to i64
  %sarg2.a.3 = add i64 %addr.z.359, 12
  %sarg2.p.3 = inttoptr i64 %sarg2.a.3 to ptr
  %sarg2.d.3 = load i32, ptr %sarg2.p.3, align 4
  %sarg2.3 = zext i32 %sarg2.d.3 to i64
  %sarg3.a.3 = add i64 %addr.z.359, 16
  %sarg3.p.3 = inttoptr i64 %sarg3.a.3 to ptr
  %sarg3.d.3 = load i32, ptr %sarg3.p.3, align 4
  %sarg3.3 = zext i32 %sarg3.d.3 to i64
  %sarg4.a.3 = add i64 %addr.z.359, 20
  %sarg4.p.3 = inttoptr i64 %sarg4.a.3 to ptr
  %sarg4.d.3 = load i32, ptr %sarg4.p.3, align 4
  %sarg4.3 = zext i32 %sarg4.d.3 to i64
  %sarg5.a.3 = add i64 %addr.z.359, 24
  %sarg5.p.3 = inttoptr i64 %sarg5.a.3 to ptr
  %sarg5.d.3 = load i32, ptr %sarg5.p.3, align 4
  %sarg5.3 = zext i32 %sarg5.d.3 to i64
  %sarg6.a.3 = add i64 %addr.z.359, 28
  %sarg6.p.3 = inttoptr i64 %sarg6.a.3 to ptr
  %sarg6.d.3 = load i32, ptr %sarg6.p.3, align 4
  %sarg6.3 = zext i32 %sarg6.d.3 to i64
  %args.p.6 = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr  ; 0x40163b
  store i64 %sarg6.3, ptr %args.p.6, align 8  ; 0x40163b
  %sarg7.a.3 = add i64 %addr.z.359, 32
  %sarg7.p.3 = inttoptr i64 %sarg7.a.3 to ptr
  %sarg7.d.3 = load i32, ptr %sarg7.p.3, align 4
  %sarg7.3 = zext i32 %sarg7.d.3 to i64
  %args.p.7 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr  ; 0x40163b
  store i64 %sarg7.3, ptr %args.p.7, align 8  ; 0x40163b
  %ret.11 = call i64 %fp.3(i64 %sarg0.11, i64 %sarg1.3, i64 %sarg2.3, i64 %sarg3.3, i64 %sarg4.3, i64 %sarg5.3, i64 %ecx.z.9, i64 undef)  ; 0x40163b
  %pops.3 = call i64 @ra_pops_of(i64 %eh.dtor.8)  ; 0x40163b
  %n.12 = trunc i64 %pops.3 to i32
  %pops.a.3 = add i32 %n.12, 4  ; 0x40163b
  %rsp.335 = add i32 %sp.184, %pops.a.3  ; 0x40163b
  br label %eh.destroyed.401540.6
eh.destroyed.401540.6:  ; 0x40163b
  %rsp.phi.3 = phi i32 [ %rsp.phi, %eh.next1_0.s4.1 ], [ %rsp.335, %eh.destroy.401540.6 ]  ; 0x40163b
  %eh.head.f8.26 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x40163b
  store i32 0, ptr %eh.head.f8.26, align 1  ; 0x40163b
  %sp.185 = sub i32 %rsp.phi.3, 4  ; 0x40163b
  %addr.z.361 = zext i32 %sp.185 to i64  ; 0x40163b
  %st.p.253 = inttoptr i64 %addr.z.361 to ptr  ; 0x40163b
  store i32 0, ptr %st.p.253, align 1  ; 0x40163b
  br label %f30_401620
eh.cleaned.401620.17:  ; 0x40163b
  %eh.head.f8.27 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x40163b
  store i32 -1, ptr %eh.head.f8.27, align 1  ; 0x40163b
  call void @ra_eh_resume()  ; 0x40163b
  %eh.head.f0.5 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 0  ; 0x40163b
  %eh.next.13 = load i32, ptr %eh.head.f0.5, align 1  ; 0x40163b
  %teb.34 = call i64 @ra_teb()  ; 0x40163b
  %teb.35 = trunc i64 %teb.34 to i32  ; 0x40163b
  %addr.z.371 = zext i32 %teb.35 to i64  ; 0x40163b
  %st.p.259 = inttoptr i64 %addr.z.371 to ptr  ; 0x40163b
  store i32 %eh.next.13, ptr %st.p.259, align 1  ; 0x40163b
  ret i64 %ret.12
f30_401620:  ; 0x401620
  %sp.186 = sub i32 %sp.185, 4  ; 0x401620
  %addr.z.362 = zext i32 %sp.186 to i64  ; 0x401620
  %st.p.254 = inttoptr i64 %addr.z.362 to ptr  ; 0x401620
  store i32 %eh.frame.44, ptr %st.p.254, align 1  ; 0x401620
  %diff.30 = sub i32 %sp.186, 8  ; 0x401621
  %sum.70 = add i32 %eh.frame.44, 12  ; 0x401624
  %addr.72 = add i32 %sum.70, -36  ; 0x401627
  %addr.z.363 = zext i32 %addr.72 to i64  ; 0x401627
  %m.p.29 = inttoptr i64 %addr.z.363 to ptr  ; 0x401627
  %m.29 = load i32, ptr %m.p.29, align 1  ; 0x401627
  %addr.73 = add i32 %sum.70, -32  ; 0x40162a
  %addr.z.364 = zext i32 %addr.73 to i64  ; 0x40162a
  %st.p.255 = inttoptr i64 %addr.z.364 to ptr  ; 0x40162a
  store i32 %m.29, ptr %st.p.255, align 1  ; 0x40162a
  %sp.187 = sub i32 %diff.30, 4  ; 0x40162d
  %addr.z.365 = zext i32 %sp.187 to i64  ; 0x40162d
  %st.p.256 = inttoptr i64 %addr.z.365 to ptr  ; 0x40162d
  store i32 220, ptr %st.p.256, align 1  ; 0x40162d
  %sp.188 = sub i32 %sp.187, 4  ; 0x401632
  %sarg0.a.12 = add i32 %sp.188, 4
  %ret.12 = call i64 @F_00401000(i32 220)  ; 0x401632
  %eh.pending.28 = call i64 @ra_eh_pending()  ; 0x401632
  %eh.thrown.28 = icmp ne i64 %eh.pending.28, 0  ; 0x401632
  br i1 %eh.thrown.28, label %eh.landing.1, label %eh.ok.401632.17
eh.ok.401632.17:  ; 0x401632
  br label %eh.cleaned.401620.17
eh.state5.1:  ; 0x40163b
  %eh.frame.47 = add i32 %eh.reg, 12  ; 0x40163b
  %eh.catch.obj.30 = add i32 %eh.frame.47, -40  ; 0x40163b
  %eh.catch.obj.31 = zext i32 %eh.catch.obj.30 to i64  ; 0x40163b
  %eh.caught.15 = call i64 @ra_eh_catches(i64 565892279632451611, i64 %eh.catch.obj.31, i64 8192)  ; 0x40163b
  %eh.hit.15 = icmp ne i64 %eh.caught.15, 0  ; 0x40163b
  br i1 %eh.hit.15, label %eh.catch1_0.s5.1, label %eh.next1_0.s5.1
eh.catch1_0.s5.1:  ; 0x40163b
  %eh.head.f8.28 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x40163b
  store i32 4, ptr %eh.head.f8.28, align 1  ; 0x40163b
  %sp.191 = sub i32 %rsp.phi, 4  ; 0x40163b
  %addr.z.373 = zext i32 %sp.191 to i64  ; 0x40163b
  %st.p.261 = inttoptr i64 %addr.z.373 to ptr  ; 0x40163b
  store i32 0, ptr %st.p.261, align 1  ; 0x40163b
  br label %f31_401600
eh.next1_0.s5.1:  ; 0x40163b
  call void @ra_eh_suspend()  ; 0x40163b
  %eh.head.f8.29 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x40163b
  store i32 4, ptr %eh.head.f8.29, align 1  ; 0x40163b
  %sp.204 = sub i32 %rsp.phi, 4  ; 0x40163b
  %addr.z.394 = zext i32 %sp.204 to i64  ; 0x40163b
  %st.p.276 = inttoptr i64 %addr.z.394 to ptr  ; 0x40163b
  store i32 0, ptr %st.p.276, align 1  ; 0x40163b
  br label %f33_401600
eh.cleaned.401600.2:  ; 0x40163b
  %eh.esp.saved.14 = call i64 @ra_eh_ending_sp()  ; 0x40161e
  %eh.esp.saved.15 = trunc i64 %eh.esp.saved.14 to i32  ; 0x40161e
  %eh.esp.p.17 = sub i32 %eh.reg, 4  ; 0x40161e
  %addr.z.381 = zext i32 %eh.esp.p.17 to i64  ; 0x40161e
  %st.p.266 = inttoptr i64 %addr.z.381 to ptr  ; 0x40161e
  store i32 %eh.esp.saved.15, ptr %st.p.266, align 1  ; 0x40161e
  %eh.ending.9 = call i64 @ra_eh_ending_object()  ; 0x40161e
  %eh.dtor.9 = call i64 @ra_eh_ending_destructor()  ; 0x40161e
  call void @ra_eh_leave_catch()  ; 0x40161e
  %eh.dies.9 = icmp ne i64 %eh.dtor.9, 0  ; 0x40161e
  br i1 %eh.dies.9, label %eh.destroy.401540.7, label %eh.destroyed.401540.7
f31_401600:  ; 0x401600
  %sp.192 = sub i32 %sp.191, 4  ; 0x401600
  %addr.z.374 = zext i32 %sp.192 to i64  ; 0x401600
  %st.p.262 = inttoptr i64 %addr.z.374 to ptr  ; 0x401600
  store i32 %eh.frame.47, ptr %st.p.262, align 1  ; 0x401600
  %diff.31 = sub i32 %sp.192, 8  ; 0x401601
  %sum.72 = add i32 %eh.frame.47, 12  ; 0x401604
  %sp.193 = sub i32 %diff.31, 4  ; 0x401607
  %addr.z.375 = zext i32 %sp.193 to i64  ; 0x401607
  %st.p.263 = inttoptr i64 %addr.z.375 to ptr  ; 0x401607
  store i32 222, ptr %st.p.263, align 1  ; 0x401607
  %sp.194 = sub i32 %sp.193, 4  ; 0x40160c
  %sarg0.a.8 = add i32 %sp.194, 4
  %ret.8 = call i64 @F_00401000(i32 222)  ; 0x40160c
  %eh.pending.29 = call i64 @ra_eh_pending()  ; 0x40160c
  %eh.thrown.29 = icmp ne i64 %eh.pending.29, 0  ; 0x40160c
  br i1 %eh.thrown.29, label %eh.landing.1, label %eh.ok.40160c.2
eh.ok.40160c.2:  ; 0x40160c
  %sum.73 = add i32 %sarg0.a.8, 4  ; 0x401611
  %addr.74 = add i32 %sum.72, -32  ; 0x401614
  %addr.z.377 = zext i32 %addr.74 to i64  ; 0x401614
  %m.p.30 = inttoptr i64 %addr.z.377 to ptr  ; 0x401614
  %m.30 = load i32, ptr %m.p.30, align 1  ; 0x401614
  %addr.75 = add i32 %sum.72, -36  ; 0x401617
  %addr.z.378 = zext i32 %addr.75 to i64  ; 0x401617
  %st.p.265 = inttoptr i64 %addr.z.378 to ptr  ; 0x401617
  store i32 %m.30, ptr %st.p.265, align 1  ; 0x401617
  %sum.74 = add i32 %sum.73, 8  ; 0x40161a
  %sp.195 = add i32 %sum.74, 4  ; 0x40161d
  %sp.196 = add i32 %sp.195, 4  ; 0x40161e
  br label %eh.cleaned.401600.2
eh.destroy.401540.7:  ; 0x40161e
  %eh.this.9 = trunc i64 %eh.ending.9 to i32  ; 0x40161e
  %ecx.z.10 = zext i32 %eh.this.9 to i64  ; 0x40161e
  %sp.197 = sub i32 %sp.196, 4  ; 0x40161e
  %addr.z.382 = zext i32 %sp.197 to i64  ; 0x40161e
  %st.p.267 = inttoptr i64 %addr.z.382 to ptr  ; 0x40161e
  store i32 0, ptr %st.p.267, align 1  ; 0x40161e
  %fp.2 = inttoptr i64 %eh.dtor.9 to ptr  ; 0x40161e
  %sarg0.a.9 = add i64 %addr.z.382, 4
  %sarg0.p.9 = inttoptr i64 %sarg0.a.9 to ptr
  %sarg0.d.9 = load i32, ptr %sarg0.p.9, align 4
  %sarg0.9 = zext i32 %sarg0.d.9 to i64
  %sarg1.a.2 = add i64 %addr.z.382, 8
  %sarg1.p.2 = inttoptr i64 %sarg1.a.2 to ptr
  %sarg1.d.2 = load i32, ptr %sarg1.p.2, align 4
  %sarg1.2 = zext i32 %sarg1.d.2 to i64
  %sarg2.a.2 = add i64 %addr.z.382, 12
  %sarg2.p.2 = inttoptr i64 %sarg2.a.2 to ptr
  %sarg2.d.2 = load i32, ptr %sarg2.p.2, align 4
  %sarg2.2 = zext i32 %sarg2.d.2 to i64
  %sarg3.a.2 = add i64 %addr.z.382, 16
  %sarg3.p.2 = inttoptr i64 %sarg3.a.2 to ptr
  %sarg3.d.2 = load i32, ptr %sarg3.p.2, align 4
  %sarg3.2 = zext i32 %sarg3.d.2 to i64
  %sarg4.a.2 = add i64 %addr.z.382, 20
  %sarg4.p.2 = inttoptr i64 %sarg4.a.2 to ptr
  %sarg4.d.2 = load i32, ptr %sarg4.p.2, align 4
  %sarg4.2 = zext i32 %sarg4.d.2 to i64
  %sarg5.a.2 = add i64 %addr.z.382, 24
  %sarg5.p.2 = inttoptr i64 %sarg5.a.2 to ptr
  %sarg5.d.2 = load i32, ptr %sarg5.p.2, align 4
  %sarg5.2 = zext i32 %sarg5.d.2 to i64
  %sarg6.a.2 = add i64 %addr.z.382, 28
  %sarg6.p.2 = inttoptr i64 %sarg6.a.2 to ptr
  %sarg6.d.2 = load i32, ptr %sarg6.p.2, align 4
  %sarg6.2 = zext i32 %sarg6.d.2 to i64
  %args.p.4 = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr  ; 0x40161e
  store i64 %sarg6.2, ptr %args.p.4, align 8  ; 0x40161e
  %sarg7.a.2 = add i64 %addr.z.382, 32
  %sarg7.p.2 = inttoptr i64 %sarg7.a.2 to ptr
  %sarg7.d.2 = load i32, ptr %sarg7.p.2, align 4
  %sarg7.2 = zext i32 %sarg7.d.2 to i64
  %args.p.5 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr  ; 0x40161e
  store i64 %sarg7.2, ptr %args.p.5, align 8  ; 0x40161e
  %ret.9 = call i64 %fp.2(i64 %sarg0.9, i64 %sarg1.2, i64 %sarg2.2, i64 %sarg3.2, i64 %sarg4.2, i64 %sarg5.2, i64 %ecx.z.10, i64 undef)  ; 0x40161e
  %pops.2 = call i64 @ra_pops_of(i64 %eh.dtor.9)  ; 0x40161e
  %n.14 = trunc i64 %pops.2 to i32
  %pops.a.2 = add i32 %n.14, 4  ; 0x40161e
  %rsp.333 = add i32 %sp.197, %pops.a.2  ; 0x40161e
  br label %eh.destroyed.401540.7
eh.destroyed.401540.7:  ; 0x40161e
  %rsp.phi.2 = phi i32 [ %sp.196, %eh.cleaned.401600.2 ], [ %rsp.333, %eh.destroy.401540.7 ]  ; 0x40161e
  %eh.head.f8.30 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x40161e
  store i32 0, ptr %eh.head.f8.30, align 1  ; 0x40161e
  %sp.198 = sub i32 %rsp.phi.2, 4  ; 0x40161e
  %addr.z.384 = zext i32 %sp.198 to i64  ; 0x40161e
  %st.p.269 = inttoptr i64 %addr.z.384 to ptr  ; 0x40161e
  store i32 0, ptr %st.p.269, align 1  ; 0x40161e
  br label %f32_401620
eh.cleaned.401620.18:  ; 0x40161e
  %eh.head.f8.31 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x40163b
  store i32 0, ptr %eh.head.f8.31, align 1  ; 0x40163b
  br label %eh.enter1_0
f32_401620:  ; 0x401620
  %sp.199 = sub i32 %sp.198, 4  ; 0x401620
  %addr.z.385 = zext i32 %sp.199 to i64  ; 0x401620
  %st.p.270 = inttoptr i64 %addr.z.385 to ptr  ; 0x401620
  store i32 %eh.frame.47, ptr %st.p.270, align 1  ; 0x401620
  %diff.32 = sub i32 %sp.199, 8  ; 0x401621
  %m.31 = load i32, ptr %st.p.265, align 1  ; 0x401627
  store i32 %m.31, ptr %m.p.30, align 1  ; 0x40162a
  %sp.200 = sub i32 %diff.32, 4  ; 0x40162d
  %addr.z.388 = zext i32 %sp.200 to i64  ; 0x40162d
  %st.p.272 = inttoptr i64 %addr.z.388 to ptr  ; 0x40162d
  store i32 220, ptr %st.p.272, align 1  ; 0x40162d
  %sp.201 = sub i32 %sp.200, 4  ; 0x401632
  %sarg0.a.10 = add i32 %sp.201, 4
  %ret.10 = call i64 @F_00401000(i32 220)  ; 0x401632
  %eh.pending.30 = call i64 @ra_eh_pending()  ; 0x401632
  %eh.thrown.30 = icmp ne i64 %eh.pending.30, 0  ; 0x401632
  br i1 %eh.thrown.30, label %eh.landing.1, label %eh.ok.401632.18
eh.ok.401632.18:  ; 0x401632
  %sum.76 = add i32 %sarg0.a.10, 12  ; 0x401637
  %sp.202 = add i32 %sum.76, 4  ; 0x40163a
  %sp.203 = add i32 %sp.202, 4  ; 0x40163b
  br label %eh.cleaned.401620.18
eh.cleaned.401600.3:  ; 0x40163b
  %eh.esp.saved.16 = call i64 @ra_eh_ending_sp()  ; 0x40161e
  %eh.esp.saved.17 = trunc i64 %eh.esp.saved.16 to i32  ; 0x40161e
  %eh.esp.p.18 = sub i32 %eh.reg, 4  ; 0x40161e
  %addr.z.402 = zext i32 %eh.esp.p.18 to i64  ; 0x40161e
  %st.p.281 = inttoptr i64 %addr.z.402 to ptr  ; 0x40161e
  store i32 %eh.esp.saved.17, ptr %st.p.281, align 1  ; 0x40161e
  %eh.ending.10 = call i64 @ra_eh_ending_object()  ; 0x40161e
  %eh.dtor.10 = call i64 @ra_eh_ending_destructor()  ; 0x40161e
  call void @ra_eh_leave_catch()  ; 0x40161e
  %eh.dies.10 = icmp ne i64 %eh.dtor.10, 0  ; 0x40161e
  br i1 %eh.dies.10, label %eh.destroy.401540.8, label %eh.destroyed.401540.8
f33_401600:  ; 0x401600
  %sp.205 = sub i32 %sp.204, 4  ; 0x401600
  %addr.z.395 = zext i32 %sp.205 to i64  ; 0x401600
  %st.p.277 = inttoptr i64 %addr.z.395 to ptr  ; 0x401600
  store i32 %eh.frame.47, ptr %st.p.277, align 1  ; 0x401600
  %diff.33 = sub i32 %sp.205, 8  ; 0x401601
  %sum.77 = add i32 %eh.frame.47, 12  ; 0x401604
  %sp.206 = sub i32 %diff.33, 4  ; 0x401607
  %addr.z.396 = zext i32 %sp.206 to i64  ; 0x401607
  %st.p.278 = inttoptr i64 %addr.z.396 to ptr  ; 0x401607
  store i32 222, ptr %st.p.278, align 1  ; 0x401607
  %sp.207 = sub i32 %sp.206, 4  ; 0x40160c
  %sarg0.a.5 = add i32 %sp.207, 4
  %ret.5 = call i64 @F_00401000(i32 222)  ; 0x40160c
  %eh.pending.31 = call i64 @ra_eh_pending()  ; 0x40160c
  %eh.thrown.31 = icmp ne i64 %eh.pending.31, 0  ; 0x40160c
  br i1 %eh.thrown.31, label %eh.landing.1, label %eh.ok.40160c.3
eh.ok.40160c.3:  ; 0x40160c
  %sum.78 = add i32 %sarg0.a.5, 4  ; 0x401611
  %addr.78 = add i32 %sum.77, -32  ; 0x401614
  %addr.z.398 = zext i32 %addr.78 to i64  ; 0x401614
  %m.p.32 = inttoptr i64 %addr.z.398 to ptr  ; 0x401614
  %m.32 = load i32, ptr %m.p.32, align 1  ; 0x401614
  %addr.79 = add i32 %sum.77, -36  ; 0x401617
  %addr.z.399 = zext i32 %addr.79 to i64  ; 0x401617
  %st.p.280 = inttoptr i64 %addr.z.399 to ptr  ; 0x401617
  store i32 %m.32, ptr %st.p.280, align 1  ; 0x401617
  %sum.79 = add i32 %sum.78, 8  ; 0x40161a
  %sp.208 = add i32 %sum.79, 4  ; 0x40161d
  %sp.209 = add i32 %sp.208, 4  ; 0x40161e
  br label %eh.cleaned.401600.3
eh.destroy.401540.8:  ; 0x40161e
  %eh.this.10 = trunc i64 %eh.ending.10 to i32  ; 0x40161e
  %ecx.z.11 = zext i32 %eh.this.10 to i64  ; 0x40161e
  %sp.210 = sub i32 %sp.209, 4  ; 0x40161e
  %addr.z.403 = zext i32 %sp.210 to i64  ; 0x40161e
  %st.p.282 = inttoptr i64 %addr.z.403 to ptr  ; 0x40161e
  store i32 0, ptr %st.p.282, align 1  ; 0x40161e
  %fp.1 = inttoptr i64 %eh.dtor.10 to ptr  ; 0x40161e
  %sarg0.a.6 = add i64 %addr.z.403, 4
  %sarg0.p.6 = inttoptr i64 %sarg0.a.6 to ptr
  %sarg0.d.6 = load i32, ptr %sarg0.p.6, align 4
  %sarg0.6 = zext i32 %sarg0.d.6 to i64
  %sarg1.a.1 = add i64 %addr.z.403, 8
  %sarg1.p.1 = inttoptr i64 %sarg1.a.1 to ptr
  %sarg1.d.1 = load i32, ptr %sarg1.p.1, align 4
  %sarg1.1 = zext i32 %sarg1.d.1 to i64
  %sarg2.a.1 = add i64 %addr.z.403, 12
  %sarg2.p.1 = inttoptr i64 %sarg2.a.1 to ptr
  %sarg2.d.1 = load i32, ptr %sarg2.p.1, align 4
  %sarg2.1 = zext i32 %sarg2.d.1 to i64
  %sarg3.a.1 = add i64 %addr.z.403, 16
  %sarg3.p.1 = inttoptr i64 %sarg3.a.1 to ptr
  %sarg3.d.1 = load i32, ptr %sarg3.p.1, align 4
  %sarg3.1 = zext i32 %sarg3.d.1 to i64
  %sarg4.a.1 = add i64 %addr.z.403, 20
  %sarg4.p.1 = inttoptr i64 %sarg4.a.1 to ptr
  %sarg4.d.1 = load i32, ptr %sarg4.p.1, align 4
  %sarg4.1 = zext i32 %sarg4.d.1 to i64
  %sarg5.a.1 = add i64 %addr.z.403, 24
  %sarg5.p.1 = inttoptr i64 %sarg5.a.1 to ptr
  %sarg5.d.1 = load i32, ptr %sarg5.p.1, align 4
  %sarg5.1 = zext i32 %sarg5.d.1 to i64
  %sarg6.a.1 = add i64 %addr.z.403, 28
  %sarg6.p.1 = inttoptr i64 %sarg6.a.1 to ptr
  %sarg6.d.1 = load i32, ptr %sarg6.p.1, align 4
  %sarg6.1 = zext i32 %sarg6.d.1 to i64
  %args.p.2 = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr  ; 0x40161e
  store i64 %sarg6.1, ptr %args.p.2, align 8  ; 0x40161e
  %sarg7.a.1 = add i64 %addr.z.403, 32
  %sarg7.p.1 = inttoptr i64 %sarg7.a.1 to ptr
  %sarg7.d.1 = load i32, ptr %sarg7.p.1, align 4
  %sarg7.1 = zext i32 %sarg7.d.1 to i64
  %args.p.3 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr  ; 0x40161e
  store i64 %sarg7.1, ptr %args.p.3, align 8  ; 0x40161e
  %ret.6 = call i64 %fp.1(i64 %sarg0.6, i64 %sarg1.1, i64 %sarg2.1, i64 %sarg3.1, i64 %sarg4.1, i64 %sarg5.1, i64 %ecx.z.11, i64 undef)  ; 0x40161e
  %pops.1 = call i64 @ra_pops_of(i64 %eh.dtor.10)  ; 0x40161e
  %n.16 = trunc i64 %pops.1 to i32
  %pops.a.1 = add i32 %n.16, 4  ; 0x40161e
  %rsp.330 = add i32 %sp.210, %pops.a.1  ; 0x40161e
  br label %eh.destroyed.401540.8
eh.destroyed.401540.8:  ; 0x40161e
  %rsp.phi.1 = phi i32 [ %sp.209, %eh.cleaned.401600.3 ], [ %rsp.330, %eh.destroy.401540.8 ]  ; 0x40161e
  %eh.head.f8.32 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x40161e
  store i32 0, ptr %eh.head.f8.32, align 1  ; 0x40161e
  %sp.211 = sub i32 %rsp.phi.1, 4  ; 0x40161e
  %addr.z.405 = zext i32 %sp.211 to i64  ; 0x40161e
  %st.p.284 = inttoptr i64 %addr.z.405 to ptr  ; 0x40161e
  store i32 0, ptr %st.p.284, align 1  ; 0x40161e
  br label %f34_401620
eh.cleaned.401620.19:  ; 0x40161e
  %eh.head.f8.33 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x40163b
  store i32 -1, ptr %eh.head.f8.33, align 1  ; 0x40163b
  call void @ra_eh_resume()  ; 0x40163b
  %eh.head.f0.6 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 0  ; 0x40163b
  %eh.next.14 = load i32, ptr %eh.head.f0.6, align 1  ; 0x40163b
  %teb.36 = call i64 @ra_teb()  ; 0x40163b
  %teb.37 = trunc i64 %teb.36 to i32  ; 0x40163b
  %addr.z.415 = zext i32 %teb.37 to i64  ; 0x40163b
  %st.p.290 = inttoptr i64 %addr.z.415 to ptr  ; 0x40163b
  store i32 %eh.next.14, ptr %st.p.290, align 1  ; 0x40163b
  ret i64 %ret.7
f34_401620:  ; 0x401620
  %sp.212 = sub i32 %sp.211, 4  ; 0x401620
  %addr.z.406 = zext i32 %sp.212 to i64  ; 0x401620
  %st.p.285 = inttoptr i64 %addr.z.406 to ptr  ; 0x401620
  store i32 %eh.frame.47, ptr %st.p.285, align 1  ; 0x401620
  %diff.34 = sub i32 %sp.212, 8  ; 0x401621
  %m.33 = load i32, ptr %st.p.280, align 1  ; 0x401627
  store i32 %m.33, ptr %m.p.32, align 1  ; 0x40162a
  %sp.213 = sub i32 %diff.34, 4  ; 0x40162d
  %addr.z.409 = zext i32 %sp.213 to i64  ; 0x40162d
  %st.p.287 = inttoptr i64 %addr.z.409 to ptr  ; 0x40162d
  store i32 220, ptr %st.p.287, align 1  ; 0x40162d
  %sp.214 = sub i32 %sp.213, 4  ; 0x401632
  %sarg0.a.7 = add i32 %sp.214, 4
  %ret.7 = call i64 @F_00401000(i32 220)  ; 0x401632
  %eh.pending.32 = call i64 @ra_eh_pending()  ; 0x401632
  %eh.thrown.32 = icmp ne i64 %eh.pending.32, 0  ; 0x401632
  br i1 %eh.thrown.32, label %eh.landing.1, label %eh.ok.401632.19
eh.ok.401632.19:  ; 0x401632
  br label %eh.cleaned.401620.19
eh.state6.1:  ; 0x40163b
  call void @ra_eh_suspend()  ; 0x40163b
  %eh.esp.saved.18 = call i64 @ra_eh_ending_sp()  ; 0x40163b
  %eh.esp.saved.19 = trunc i64 %eh.esp.saved.18 to i32  ; 0x40163b
  %eh.esp.p.19 = sub i32 %eh.reg, 4  ; 0x40163b
  %addr.z.416 = zext i32 %eh.esp.p.19 to i64  ; 0x40163b
  %st.p.291 = inttoptr i64 %addr.z.416 to ptr  ; 0x40163b
  store i32 %eh.esp.saved.19, ptr %st.p.291, align 1  ; 0x40163b
  %eh.ending.11 = call i64 @ra_eh_ending_object()  ; 0x40163b
  %eh.dtor.11 = call i64 @ra_eh_ending_destructor()  ; 0x40163b
  call void @ra_eh_leave_catch()  ; 0x40163b
  %eh.dies.11 = icmp ne i64 %eh.dtor.11, 0  ; 0x40163b
  br i1 %eh.dies.11, label %eh.destroy.401540.9, label %eh.destroyed.401540.9
eh.destroy.401540.9:  ; 0x40163b
  %eh.this.11 = trunc i64 %eh.ending.11 to i32  ; 0x40163b
  %ecx.z.12 = zext i32 %eh.this.11 to i64  ; 0x40163b
  %sp.217 = sub i32 %rsp.phi, 4  ; 0x40163b
  %addr.z.417 = zext i32 %sp.217 to i64  ; 0x40163b
  %st.p.292 = inttoptr i64 %addr.z.417 to ptr  ; 0x40163b
  store i32 0, ptr %st.p.292, align 1  ; 0x40163b
  %fp = inttoptr i64 %eh.dtor.11 to ptr  ; 0x40163b
  %sarg0.a.4 = add i64 %addr.z.417, 4
  %sarg0.p.4 = inttoptr i64 %sarg0.a.4 to ptr
  %sarg0.d.4 = load i32, ptr %sarg0.p.4, align 4
  %sarg0.4 = zext i32 %sarg0.d.4 to i64
  %sarg1.a = add i64 %addr.z.417, 8
  %sarg1.p = inttoptr i64 %sarg1.a to ptr
  %sarg1.d = load i32, ptr %sarg1.p, align 4
  %sarg1 = zext i32 %sarg1.d to i64
  %sarg2.a = add i64 %addr.z.417, 12
  %sarg2.p = inttoptr i64 %sarg2.a to ptr
  %sarg2.d = load i32, ptr %sarg2.p, align 4
  %sarg2 = zext i32 %sarg2.d to i64
  %sarg3.a = add i64 %addr.z.417, 16
  %sarg3.p = inttoptr i64 %sarg3.a to ptr
  %sarg3.d = load i32, ptr %sarg3.p, align 4
  %sarg3 = zext i32 %sarg3.d to i64
  %sarg4.a = add i64 %addr.z.417, 20
  %sarg4.p = inttoptr i64 %sarg4.a to ptr
  %sarg4.d = load i32, ptr %sarg4.p, align 4
  %sarg4 = zext i32 %sarg4.d to i64
  %sarg5.a = add i64 %addr.z.417, 24
  %sarg5.p = inttoptr i64 %sarg5.a to ptr
  %sarg5.d = load i32, ptr %sarg5.p, align 4
  %sarg5 = zext i32 %sarg5.d to i64
  %sarg6.a = add i64 %addr.z.417, 28
  %sarg6.p = inttoptr i64 %sarg6.a to ptr
  %sarg6.d = load i32, ptr %sarg6.p, align 4
  %sarg6 = zext i32 %sarg6.d to i64
  %args.p = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr  ; 0x40163b
  store i64 %sarg6, ptr %args.p, align 8  ; 0x40163b
  %sarg7.a = add i64 %addr.z.417, 32
  %sarg7.p = inttoptr i64 %sarg7.a to ptr
  %sarg7.d = load i32, ptr %sarg7.p, align 4
  %sarg7 = zext i32 %sarg7.d to i64
  %args.p.1 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr  ; 0x40163b
  store i64 %sarg7, ptr %args.p.1, align 8  ; 0x40163b
  %ret.4 = call i64 %fp(i64 %sarg0.4, i64 %sarg1, i64 %sarg2, i64 %sarg3, i64 %sarg4, i64 %sarg5, i64 %ecx.z.12, i64 undef)  ; 0x40163b
  %pops = call i64 @ra_pops_of(i64 %eh.dtor.11)  ; 0x40163b
  br label %eh.destroyed.401540.9
eh.destroyed.401540.9:  ; 0x40163b
  %rax.phi.1 = phi i64 [ %rax.phi, %eh.state6.1 ], [ %ret.4, %eh.destroy.401540.9 ]  ; 0x40163b
  %eh.head.f8.34 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 2  ; 0x40163b
  store i32 -1, ptr %eh.head.f8.34, align 1  ; 0x40163b
  call void @ra_eh_resume()  ; 0x40163b
  %eh.head.f0.7 = getelementptr inbounds %S0, ptr %eh.head.s, i64 0, i32 0  ; 0x40163b
  %eh.next.15 = load i32, ptr %eh.head.f0.7, align 1  ; 0x40163b
  %teb.38 = call i64 @ra_teb()  ; 0x40163b
  %teb.39 = trunc i64 %teb.38 to i32  ; 0x40163b
  %addr.z.420 = zext i32 %teb.39 to i64  ; 0x40163b
  %st.p.294 = inttoptr i64 %addr.z.420 to ptr  ; 0x40163b
  store i32 %eh.next.15, ptr %st.p.294, align 1  ; 0x40163b
  ret i64 %rax.phi.1
eh.ok.401572:  ; 0x40163b
  %sum.82 = add i32 %sarg0.a, 4  ; 0x401577
  %sp.218 = sub i32 %sum.82, 4  ; 0x40157a
  store i32 121, ptr %st.p.9, align 1  ; 0x40157a
  %sp.219 = sub i32 %sp.218, 4  ; 0x40157c
  %sarg0.a.1 = add i32 %sp.219, 4
  %ret.1 = call i64 @F_00401000(i32 121)  ; 0x40157c
  %eh.pending.33 = call i64 @ra_eh_pending()  ; 0x40157c
  %eh.thrown.33 = icmp ne i64 %eh.pending.33, 0  ; 0x40157c
  br i1 %eh.thrown.33, label %eh.landing, label %eh.ok.40157c
eh.ok.40157c:  ; 0x40157c
  %sum.83 = add i32 %sarg0.a.1, 4  ; 0x401581
  %not = icmp ne i32 %arg0, 0  ; 0x401586
  br i1 %not, label %b_4015b6, label %b_401588
eh.ok.40158d:  ; 0x40158d
  %sum.84 = add i32 %sarg0.a.2, 4  ; 0x401592
  %sp.222 = sub i32 %sum.84, 4  ; 0x401595
  store i32 220, ptr %st.p.9, align 1  ; 0x401595
  %sp.223 = sub i32 %sp.222, 4  ; 0x40159a
  %sarg0.a.3 = add i32 %sp.223, 4
  %ret.3 = call i64 @F_00401000(i32 220)  ; 0x40159a
  %eh.pending.35 = call i64 @ra_eh_pending()  ; 0x40159a
  %eh.thrown.35 = icmp ne i64 %eh.pending.35, 0  ; 0x40159a
  br i1 %eh.thrown.35, label %eh.landing, label %eh.ok.40159a
eh.ok.40159a:  ; 0x40159a
  br label %b_4015a4
}

define i64 @F_004016b0(i32 %arg0) {  ; 0x4016b0
b_4016b0:  ; 0x4016b0
  %frame = alloca [4268 x i8], align 16
  %frame.top = getelementptr inbounds i8, ptr %frame, i64 4188
  %rsp0 = ptrtoint ptr %frame.top to i64
  %stack.arg0.p = getelementptr inbounds i8, ptr %frame.top, i64 4
  store i32 %arg0, ptr %stack.arg0.p, align 4
  %ebp = trunc i64 undef to i32  ; 0x4016b0
  %esp = trunc i64 %rsp0 to i32  ; 0x4016b0
  %sp = sub i32 %esp, 4  ; 0x4016b0
  %st.p = getelementptr inbounds i8, ptr %frame.top, i64 -4  ; 0x4016b0
  store i32 %ebp, ptr %st.p, align 1  ; 0x4016b0
  %sp.1 = sub i32 %sp, 4  ; 0x4016b3
  %st.p.1 = getelementptr inbounds i8, ptr %frame.top, i64 -8  ; 0x4016b3
  store i32 %ebp, ptr %st.p.1, align 1  ; 0x4016b3
  %sp.2 = sub i32 %sp.1, 4  ; 0x4016b4
  %st.p.2 = getelementptr inbounds i8, ptr %frame.top, i64 -12  ; 0x4016b4
  store i32 %ebp, ptr %st.p.2, align 1  ; 0x4016b4
  %sp.3 = sub i32 %sp.2, 4  ; 0x4016b5
  %st.p.3 = getelementptr inbounds i8, ptr %frame.top, i64 -16  ; 0x4016b5
  store i32 %ebp, ptr %st.p.3, align 1  ; 0x4016b5
  %diff = sub i32 %sp.3, 32  ; 0x4016b6
  %st.p.4 = getelementptr inbounds i8, ptr %frame.top, i64 -32  ; 0x4016bc
  store i32 %diff, ptr %st.p.4, align 1  ; 0x4016bc
  %st.p.5 = getelementptr inbounds i8, ptr %frame.top, i64 -20  ; 0x4016bf
  store i32 -1, ptr %st.p.5, align 1  ; 0x4016bf
  %addr.3 = add i32 %sp, -24  ; 0x4016c6
  %st.p.6 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x4016c9
  store i32 ptrtoint (ptr @U_00401c20 to i32), ptr %st.p.6, align 1  ; 0x4016c9
  %teb = call i64 @ra_teb()  ; 0x4016d0
  %teb.1 = trunc i64 %teb to i32  ; 0x4016d0
  %addr.z.8 = zext i32 %teb.1 to i64  ; 0x4016d0
  %m.p.1 = inttoptr i64 %addr.z.8 to ptr  ; 0x4016d0
  %m.1 = load i32, ptr %m.p.1, align 1  ; 0x4016d0
  %st.p.7 = getelementptr inbounds i8, ptr %frame.top, i64 -28  ; 0x4016d7
  store i32 %m.1, ptr %st.p.7, align 1  ; 0x4016d7
  %teb.2 = call i64 @ra_teb()  ; 0x4016da
  %teb.3 = trunc i64 %teb.2 to i32  ; 0x4016da
  %addr.z.10 = zext i32 %teb.3 to i64  ; 0x4016da
  %st.p.8 = inttoptr i64 %addr.z.10 to ptr  ; 0x4016da
  store i32 %addr.3, ptr %st.p.8, align 1  ; 0x4016da
  %sp.4 = sub i32 %diff, 4  ; 0x4016e0
  %st.p.9 = getelementptr inbounds i8, ptr %frame.top, i64 -52  ; 0x4016e0
  store i32 130, ptr %st.p.9, align 1  ; 0x4016e0
  %sp.5 = sub i32 %sp.4, 4  ; 0x4016e5
  %sarg0.a = add i32 %sp.5, 4
  %ret = call i64 @F_00401000(i32 130)  ; 0x4016e5
  %eh.pending = call i64 @ra_eh_pending()  ; 0x4016e5
  %eh.thrown = icmp ne i64 %eh.pending, 0  ; 0x4016e5
  br i1 %eh.thrown, label %eh.landing, label %eh.ok.4016e5
b_401704:  ; 0x401704
  %sum.22 = add i32 %arg0, 230  ; 0x401704
  %sp.52 = sub i32 %sum.21, 4  ; 0x40170a
  store i32 %sum.22, ptr %st.p.9, align 1  ; 0x40170a
  %sp.53 = sub i32 %sp.52, 4  ; 0x40170b
  %sarg0.a.2 = add i32 %sp.53, 4
  %ret.2 = call i64 @F_00401000(i32 %sum.22)  ; 0x40170b
  %eh.pending.10 = call i64 @ra_eh_pending()  ; 0x40170b
  %eh.thrown.10 = icmp ne i64 %eh.pending.10, 0  ; 0x40170b
  br i1 %eh.thrown.10, label %eh.landing, label %eh.ok.40170b
b_401725:  ; 0x401725
  %rax.phi.4 = phi i64 [ %eax.z.19, %b_401759 ], [ 1, %eh.ok.401718 ]  ; 0x401725
  %rbp.phi.1 = phi i32 [ %sum.26, %b_401759 ], [ %sp, %eh.ok.401718 ]  ; 0x401725
  %addr.20 = add i32 %rbp.phi.1, -24  ; 0x401725
  %addr.z.121 = zext i32 %addr.20 to i64  ; 0x401725
  %m.p.10 = inttoptr i64 %addr.z.121 to ptr  ; 0x401725
  %m.10 = load i32, ptr %m.p.10, align 1  ; 0x401725
  %teb.24 = call i64 @ra_teb()  ; 0x401728
  %teb.25 = trunc i64 %teb.24 to i32  ; 0x401728
  %addr.z.122 = zext i32 %teb.25 to i64  ; 0x401728
  %st.p.83 = inttoptr i64 %addr.z.122 to ptr  ; 0x401728
  store i32 %m.10, ptr %st.p.83, align 1  ; 0x401728
  ret i64 %rax.phi.4
b_401737:  ; 0x401737
  %addr.21 = add i32 %sp, -40  ; 0x401737
  %addr.z.128 = zext i32 %addr.21 to i64  ; 0x401737
  %st.p.84 = getelementptr inbounds i8, ptr %frame.top, i64 -44  ; 0x401737
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 152) to i32), ptr %st.p.84, align 1  ; 0x401737
  %st.p.85 = getelementptr inbounds i8, ptr %frame.top, i64 -40  ; 0x40173e
  store i32 %addr.19, ptr %st.p.85, align 1  ; 0x40173e
  store i32 1, ptr %st.p.5, align 1  ; 0x401741
  %sp.61 = sub i32 %sum.21, 4  ; 0x40174b
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 2940) to i32), ptr %st.p.9, align 1  ; 0x40174b
  %sp.62 = sub i32 %sp.61, 4  ; 0x401750
  %st.p.88 = getelementptr inbounds i8, ptr %frame.top, i64 -56  ; 0x401750
  store i32 %addr.21, ptr %st.p.88, align 1  ; 0x401750
  call void @ra_eh_throw(i64 %addr.z.128, i64 8)  ; 0x401751
  call void @ra_eh_throw_destructor(i64 4198832)  ; 0x401751
  call void @ra_eh_throw_type(i64 -1082765518775566335, i64 0, i64 8)  ; 0x401751
  call void @ra_eh_throw_copy(i64 4198800)  ; 0x401751
  br label %eh.landing
b_401759:  ; 0x401759
  %rbp.phi = phi i32 [ %eh.frame.2, %eh.destroyed.40178c ], [ %eh.frame.7, %eh.destroyed.40178c.1 ]  ; 0x401759
  %sum.26 = add i32 %rbp.phi, 12  ; 0x401759
  %addr.25 = add i32 %sum.26, -32  ; 0x40175c
  %addr.z.134 = zext i32 %addr.25 to i64  ; 0x40175c
  %m.p.11 = inttoptr i64 %addr.z.134 to ptr  ; 0x40175c
  %m.11 = load i32, ptr %m.p.11, align 1  ; 0x40175c
  %eax.z.19 = zext i32 %m.11 to i64  ; 0x40175c
  br label %b_401725
eh.landing:  ; 0x4016e5
  %rax.phi = phi i64 [ %ret, %b_4016b0 ], [ %ret.2, %b_401704 ], [ %addr.z.128, %b_401737 ], [ %ret.1, %eh.ok.4016e5 ], [ %ret.3, %eh.ok.40170b ]  ; 0x4016e5
  %rsp.phi = phi i32 [ %sarg0.a, %b_4016b0 ], [ %sarg0.a.2, %b_401704 ], [ %sp.62, %b_401737 ], [ %sarg0.a.1, %eh.ok.4016e5 ], [ %sarg0.a.3, %eh.ok.40170b ]  ; 0x4016e5
  %teb.4 = call i64 @ra_teb()  ; 0x4016e5
  %teb.5 = trunc i64 %teb.4 to i32  ; 0x4016e5
  %addr.z.13 = zext i32 %teb.5 to i64  ; 0x4016e5
  %eh.head.p = inttoptr i64 %addr.z.13 to ptr  ; 0x4016e5
  %eh.head = load i32, ptr %eh.head.p, align 1  ; 0x4016e5
  %eh.head.s.w = zext i32 %eh.head to i64  ; 0x4016e5
  %eh.head.s = inttoptr i64 %eh.head.s.w to ptr  ; 0x4016e5
  %eh.frame = sub i32 %eh.head, -24  ; 0x4016e5
  %eh.reg = add i32 %eh.frame, -24  ; 0x4016e5
  %eh.head.f4 = getelementptr inbounds %S1, ptr %eh.head.s, i64 0, i32 1  ; 0x4016e5
  %eh.handler = load i32, ptr %eh.head.f4, align 1  ; 0x4016e5
  %eh.ours = icmp eq i32 %eh.handler, 4201504  ; 0x4016e5
  %eh.ours.1 = icmp eq i32 %eh.handler, ptrtoint (ptr @U_00401c20 to i32)  ; 0x4016e5
  %eh.ours.2 = or i1 %eh.ours, %eh.ours.1  ; 0x4016e5
  br i1 %eh.ours.2, label %eh.dispatch, label %eh.foreign
eh.dispatch:  ; 0x4016e5
  %eh.head.f8 = getelementptr inbounds %S1, ptr %eh.head.s, i64 0, i32 2  ; 0x4016e5
  %eh.state = load i32, ptr %eh.head.f8, align 1  ; 0x4016e5
  switch i32 %eh.state, label %eh.state.bad [ i32 -1, label %eh.state_none i32 0, label %eh.state0 i32 1, label %eh.state1 i32 2, label %eh.state2.1 ]
eh.foreign:  ; 0x4016e5
  call void @ra_trap(i64 4200112, i64 8)  ; 0x4016e5
  unreachable
eh.state.bad:  ; 0x4016e5
  call void @ra_trap(i64 4200112, i64 7)  ; 0x4016e5
  unreachable
eh.state_none:  ; 0x4016e5
  call void @ra_eh_suspend()  ; 0x4016e5
  %eh.head.f8.1 = getelementptr inbounds %S1, ptr %eh.head.s, i64 0, i32 2  ; 0x4016e5
  store i32 -1, ptr %eh.head.f8.1, align 1  ; 0x4016e5
  call void @ra_eh_resume()  ; 0x4016e5
  %eh.head.f0 = getelementptr inbounds %S1, ptr %eh.head.s, i64 0, i32 0  ; 0x4016e5
  %eh.next = load i32, ptr %eh.head.f0, align 1  ; 0x4016e5
  %teb.6 = call i64 @ra_teb()  ; 0x4016e5
  %teb.7 = trunc i64 %teb.6 to i32  ; 0x4016e5
  %addr.z.18 = zext i32 %teb.7 to i64  ; 0x4016e5
  %st.p.12 = inttoptr i64 %addr.z.18 to ptr  ; 0x4016e5
  store i32 %eh.next, ptr %st.p.12, align 1  ; 0x4016e5
  ret i64 %rax.phi
eh.state0:  ; 0x4016e5
  %eh.frame.1 = add i32 %eh.reg, 12  ; 0x4016e5
  %eh.catch.obj = add i32 %eh.frame.1, -32  ; 0x4016e5
  %eh.catch.obj.1 = zext i32 %eh.catch.obj to i64  ; 0x4016e5
  %eh.caught = call i64 @ra_eh_catches(i64 -1082765518775566335, i64 %eh.catch.obj.1, i64 8200)  ; 0x4016e5
  %eh.hit = icmp ne i64 %eh.caught, 0  ; 0x4016e5
  br i1 %eh.hit, label %eh.catch0_0.s0, label %eh.next0_0.s0
eh.catch0_0.s0:  ; 0x4016e5
  %eh.head.f8.2 = getelementptr inbounds %S1, ptr %eh.head.s, i64 0, i32 2  ; 0x4016e5
  store i32 0, ptr %eh.head.f8.2, align 1  ; 0x4016e5
  br label %eh.enter0_0
eh.next0_0.s0:  ; 0x4016e5
  call void @ra_eh_suspend()  ; 0x40178c
  %eh.head.f8.3 = getelementptr inbounds %S1, ptr %eh.head.s, i64 0, i32 2  ; 0x40178c
  store i32 -1, ptr %eh.head.f8.3, align 1  ; 0x40178c
  call void @ra_eh_resume()  ; 0x40178c
  %eh.head.f0.1 = getelementptr inbounds %S1, ptr %eh.head.s, i64 0, i32 0  ; 0x40178c
  %eh.next.1 = load i32, ptr %eh.head.f0.1, align 1  ; 0x40178c
  %teb.8 = call i64 @ra_teb()  ; 0x40178c
  %teb.9 = trunc i64 %teb.8 to i32  ; 0x40178c
  %addr.z.34 = zext i32 %teb.9 to i64  ; 0x40178c
  %st.p.22 = inttoptr i64 %addr.z.34 to ptr  ; 0x40178c
  store i32 %eh.next.1, ptr %st.p.22, align 1  ; 0x40178c
  ret i64 %rax.phi
eh.enter0_0:  ; 0x4016e5
  %rsp.phi.3 = phi i32 [ %rsp.phi, %eh.catch0_0.s0 ], [ %sp.40, %eh.cleaned.401790 ]  ; 0x4016e5
  call void @ra_eh_enter_catch()  ; 0x4016e5
  %eh.head.f8.4 = getelementptr inbounds %S1, ptr %eh.head.s, i64 0, i32 2  ; 0x4016e5
  store i32 2, ptr %eh.head.f8.4, align 1  ; 0x4016e5
  %eh.esp.p = sub i32 %eh.reg, 4  ; 0x4016e5
  %addr.z.21 = zext i32 %eh.esp.p to i64  ; 0x4016e5
  %eh.esp.p.1 = inttoptr i64 %addr.z.21 to ptr  ; 0x4016e5
  %eh.esp = load i32, ptr %eh.esp.p.1, align 1  ; 0x4016e5
  %eh.esp.1 = zext i32 %eh.esp to i64  ; 0x4016e5
  call void @ra_eh_catch_sp(i64 %eh.esp.1)  ; 0x4016e5
  %eh.frame.2 = add i32 %eh.reg, 12  ; 0x4016e5
  %sp.6 = sub i32 %rsp.phi.3, 4  ; 0x4016e5
  %addr.z.22 = zext i32 %sp.6 to i64  ; 0x4016e5
  %st.p.15 = inttoptr i64 %addr.z.22 to ptr  ; 0x4016e5
  store i32 0, ptr %st.p.15, align 1  ; 0x4016e5
  br label %f1_401770
f1_401770:  ; 0x401770
  %sp.7 = sub i32 %sp.6, 4  ; 0x401770
  %addr.z.23 = zext i32 %sp.7 to i64  ; 0x401770
  %st.p.16 = inttoptr i64 %addr.z.23 to ptr  ; 0x401770
  store i32 %eh.frame.2, ptr %st.p.16, align 1  ; 0x401770
  %diff.1 = sub i32 %sp.7, 8  ; 0x401771
  %sum = add i32 %eh.frame.2, 12  ; 0x401774
  %addr.6 = add i32 %sum, -28  ; 0x401777
  %addr.z.24 = zext i32 %addr.6 to i64  ; 0x401777
  %st.p.17 = inttoptr i64 %addr.z.24 to ptr  ; 0x401777
  store i32 %diff.1, ptr %st.p.17, align 1  ; 0x401777
  %addr.7 = add i32 %sum, -44  ; 0x40177a
  %addr.z.25 = zext i32 %addr.7 to i64  ; 0x40177a
  %m.p.2 = inttoptr i64 %addr.z.25 to ptr  ; 0x40177a
  %m.2 = load i32, ptr %m.p.2, align 1  ; 0x40177a
  %addr.8 = add i32 %m.2, 4  ; 0x40177d
  %addr.z.26 = zext i32 %addr.8 to i64  ; 0x40177d
  %m.p.3 = inttoptr i64 %addr.z.26 to ptr  ; 0x40177d
  %m.3 = load i32, ptr %m.p.3, align 1  ; 0x40177d
  %addr.9 = add i32 %sum, -32  ; 0x401780
  %addr.z.27 = zext i32 %addr.9 to i64  ; 0x401780
  %st.p.18 = inttoptr i64 %addr.z.27 to ptr  ; 0x401780
  store i32 %m.3, ptr %st.p.18, align 1  ; 0x401780
  %sum.1 = add i32 %diff.1, 8  ; 0x401788
  %sp.8 = add i32 %sum.1, 4  ; 0x40178b
  %sp.9 = add i32 %sp.8, 4  ; 0x40178c
  %eh.ending = call i64 @ra_eh_ending_object()  ; 0x40178c
  %eh.dtor = call i64 @ra_eh_ending_destructor()  ; 0x40178c
  call void @ra_eh_leave_catch()  ; 0x40178c
  %eh.dies = icmp ne i64 %eh.dtor, 0  ; 0x40178c
  br i1 %eh.dies, label %eh.destroy.40178c, label %eh.destroyed.40178c
eh.destroy.40178c:  ; 0x40178c
  %eh.this = trunc i64 %eh.ending to i32  ; 0x40178c
  %ecx.z.1 = zext i32 %eh.this to i64  ; 0x40178c
  %sp.10 = sub i32 %sp.9, 4  ; 0x40178c
  %addr.z.30 = zext i32 %sp.10 to i64  ; 0x40178c
  %st.p.19 = inttoptr i64 %addr.z.30 to ptr  ; 0x40178c
  store i32 0, ptr %st.p.19, align 1  ; 0x40178c
  %fp.3 = inttoptr i64 %eh.dtor to ptr  ; 0x40178c
  %sarg0.a.15 = add i64 %addr.z.30, 4
  %sarg0.p.15 = inttoptr i64 %sarg0.a.15 to ptr
  %sarg0.d.15 = load i32, ptr %sarg0.p.15, align 4
  %sarg0.15 = zext i32 %sarg0.d.15 to i64
  %sarg1.a.3 = add i64 %addr.z.30, 8
  %sarg1.p.3 = inttoptr i64 %sarg1.a.3 to ptr
  %sarg1.d.3 = load i32, ptr %sarg1.p.3, align 4
  %sarg1.3 = zext i32 %sarg1.d.3 to i64
  %sarg2.a.3 = add i64 %addr.z.30, 12
  %sarg2.p.3 = inttoptr i64 %sarg2.a.3 to ptr
  %sarg2.d.3 = load i32, ptr %sarg2.p.3, align 4
  %sarg2.3 = zext i32 %sarg2.d.3 to i64
  %sarg3.a.3 = add i64 %addr.z.30, 16
  %sarg3.p.3 = inttoptr i64 %sarg3.a.3 to ptr
  %sarg3.d.3 = load i32, ptr %sarg3.p.3, align 4
  %sarg3.3 = zext i32 %sarg3.d.3 to i64
  %sarg4.a.3 = add i64 %addr.z.30, 20
  %sarg4.p.3 = inttoptr i64 %sarg4.a.3 to ptr
  %sarg4.d.3 = load i32, ptr %sarg4.p.3, align 4
  %sarg4.3 = zext i32 %sarg4.d.3 to i64
  %sarg5.a.3 = add i64 %addr.z.30, 24
  %sarg5.p.3 = inttoptr i64 %sarg5.a.3 to ptr
  %sarg5.d.3 = load i32, ptr %sarg5.p.3, align 4
  %sarg5.3 = zext i32 %sarg5.d.3 to i64
  %sarg6.a.3 = add i64 %addr.z.30, 28
  %sarg6.p.3 = inttoptr i64 %sarg6.a.3 to ptr
  %sarg6.d.3 = load i32, ptr %sarg6.p.3, align 4
  %sarg6.3 = zext i32 %sarg6.d.3 to i64
  %args.p.6 = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr  ; 0x40178c
  store i64 %sarg6.3, ptr %args.p.6, align 8  ; 0x40178c
  %sarg7.a.3 = add i64 %addr.z.30, 32
  %sarg7.p.3 = inttoptr i64 %sarg7.a.3 to ptr
  %sarg7.d.3 = load i32, ptr %sarg7.p.3, align 4
  %sarg7.3 = zext i32 %sarg7.d.3 to i64
  %args.p.7 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr  ; 0x40178c
  store i64 %sarg7.3, ptr %args.p.7, align 8  ; 0x40178c
  %ret.15 = call i64 %fp.3(i64 %sarg0.15, i64 %sarg1.3, i64 %sarg2.3, i64 %sarg3.3, i64 %sarg4.3, i64 %sarg5.3, i64 %ecx.z.1, i64 undef)  ; 0x40178c
  %pops.3 = call i64 @ra_pops_of(i64 %eh.dtor)  ; 0x40178c
  br label %eh.destroyed.40178c
eh.destroyed.40178c:  ; 0x40178c
  store i32 %eh.esp, ptr %eh.esp.p.1, align 1  ; 0x40178c
  br label %b_401759
eh.state1:  ; 0x40178c
  %eh.frame.3 = add i32 %eh.reg, 12  ; 0x40178c
  %eh.catch.obj.2 = add i32 %eh.frame.3, -32  ; 0x40178c
  %eh.catch.obj.3 = zext i32 %eh.catch.obj.2 to i64  ; 0x40178c
  %eh.caught.1 = call i64 @ra_eh_catches(i64 -1082765518775566335, i64 %eh.catch.obj.3, i64 8200)  ; 0x40178c
  %eh.hit.1 = icmp ne i64 %eh.caught.1, 0  ; 0x40178c
  br i1 %eh.hit.1, label %eh.catch0_0.s1, label %eh.next0_0.s1
eh.catch0_0.s1:  ; 0x40178c
  %eh.head.f8.5 = getelementptr inbounds %S1, ptr %eh.head.s, i64 0, i32 2  ; 0x40178c
  store i32 0, ptr %eh.head.f8.5, align 1  ; 0x40178c
  %sp.11 = sub i32 %rsp.phi, 4  ; 0x40178c
  %addr.z.36 = zext i32 %sp.11 to i64  ; 0x40178c
  %st.p.24 = inttoptr i64 %addr.z.36 to ptr  ; 0x40178c
  store i32 0, ptr %st.p.24, align 1  ; 0x40178c
  br label %f2_401790
eh.next0_0.s1:  ; 0x40178c
  call void @ra_eh_suspend()  ; 0x4017b6
  %eh.head.f8.6 = getelementptr inbounds %S1, ptr %eh.head.s, i64 0, i32 2  ; 0x4017b6
  store i32 0, ptr %eh.head.f8.6, align 1  ; 0x4017b6
  %sp.41 = sub i32 %rsp.phi, 4  ; 0x4017b6
  %addr.z.98 = zext i32 %sp.41 to i64  ; 0x4017b6
  %st.p.65 = inttoptr i64 %addr.z.98 to ptr  ; 0x4017b6
  store i32 0, ptr %st.p.65, align 1  ; 0x4017b6
  br label %f6_401790
eh.cleaned.401790:  ; 0x40178c
  %eh.head.f8.7 = getelementptr inbounds %S1, ptr %eh.head.s, i64 0, i32 2  ; 0x4017b6
  store i32 0, ptr %eh.head.f8.7, align 1  ; 0x4017b6
  br label %eh.enter0_0
f2_401790:  ; 0x401790
  %sp.12 = sub i32 %sp.11, 4  ; 0x401790
  %addr.z.37 = zext i32 %sp.12 to i64  ; 0x401790
  %st.p.25 = inttoptr i64 %addr.z.37 to ptr  ; 0x401790
  store i32 %eh.frame.3, ptr %st.p.25, align 1  ; 0x401790
  %diff.2 = sub i32 %sp.12, 8  ; 0x401791
  %eh.head.f32 = getelementptr inbounds %S1, ptr %eh.head.s, i64 0, i32 4  ; 0x401797
  %m.4 = load i32, ptr %eh.head.f32, align 1  ; 0x401797
  %sum.3 = add i32 %m.4, 230  ; 0x40179a
  %sp.13 = sub i32 %diff.2, 4  ; 0x40179f
  %addr.z.39 = zext i32 %sp.13 to i64  ; 0x40179f
  %st.p.26 = inttoptr i64 %addr.z.39 to ptr  ; 0x40179f
  store i32 %sum.3, ptr %st.p.26, align 1  ; 0x40179f
  %sp.14 = sub i32 %sp.13, 4  ; 0x4017a0
  %sarg0.a.7 = add i32 %sp.14, 4
  %ret.7 = call i64 @F_00401000(i32 %sum.3)  ; 0x4017a0
  %eh.pending.1 = call i64 @ra_eh_pending()  ; 0x4017a0
  %eh.thrown.1 = icmp ne i64 %eh.pending.1, 0  ; 0x4017a0
  br i1 %eh.thrown.1, label %eh.landing.1, label %eh.ok.4017a0.2
eh.landing.1:  ; 0x4017a0
  %rax.phi.2 = phi i64 [ %ret.7, %f2_401790 ], [ %ret.12, %f4_401790 ], [ %ret.13, %eh.ok.4017a0 ], [ %ret.10, %f5_401790 ], [ %ret.11, %eh.ok.4017a0.1 ], [ %ret.8, %eh.ok.4017a0.2 ], [ %ret.5, %f6_401790 ], [ %ret.6, %eh.ok.4017a0.3 ]  ; 0x4017a0
  %rsp.phi.1 = phi i32 [ %sarg0.a.7, %f2_401790 ], [ %sarg0.a.12, %f4_401790 ], [ %sarg0.a.13, %eh.ok.4017a0 ], [ %sarg0.a.10, %f5_401790 ], [ %sarg0.a.11, %eh.ok.4017a0.1 ], [ %sarg0.a.8, %eh.ok.4017a0.2 ], [ %sarg0.a.5, %f6_401790 ], [ %sarg0.a.6, %eh.ok.4017a0.3 ]  ; 0x4017a0
  %teb.10 = call i64 @ra_teb()  ; 0x4017a0
  %teb.11 = trunc i64 %teb.10 to i32  ; 0x4017a0
  %addr.z.41 = zext i32 %teb.11 to i64  ; 0x4017a0
  %eh.head.p.1 = inttoptr i64 %addr.z.41 to ptr  ; 0x4017a0
  %eh.head.1 = load i32, ptr %eh.head.p.1, align 1  ; 0x4017a0
  %eh.head.1.s.w = zext i32 %eh.head.1 to i64  ; 0x4017a0
  %eh.head.1.s = inttoptr i64 %eh.head.1.s.w to ptr  ; 0x4017a0
  %eh.frame.5 = sub i32 %eh.head.1, -24  ; 0x4017a0
  %eh.reg.1 = add i32 %eh.frame.5, -24  ; 0x4017a0
  %eh.head.1.f4 = getelementptr inbounds %S1, ptr %eh.head.1.s, i64 0, i32 1  ; 0x4017a0
  %eh.handler.1 = load i32, ptr %eh.head.1.f4, align 1  ; 0x4017a0
  %eh.ours.3 = icmp eq i32 %eh.handler.1, 4201504  ; 0x4017a0
  %eh.ours.4 = icmp eq i32 %eh.handler.1, ptrtoint (ptr @U_00401c20 to i32)  ; 0x4017a0
  %eh.ours.5 = or i1 %eh.ours.3, %eh.ours.4  ; 0x4017a0
  br i1 %eh.ours.5, label %eh.dispatch.1, label %eh.foreign.1
eh.dispatch.1:  ; 0x4017a0
  %eh.head.1.f8 = getelementptr inbounds %S1, ptr %eh.head.1.s, i64 0, i32 2  ; 0x4017a0
  %eh.state.1 = load i32, ptr %eh.head.1.f8, align 1  ; 0x4017a0
  switch i32 %eh.state.1, label %eh.state.bad.1 [ i32 -1, label %eh.state_none.1 i32 0, label %eh.state0.1 i32 1, label %eh.state1.1 i32 2, label %eh.state2 ]
eh.foreign.1:  ; 0x4017a0
  call void @ra_trap(i64 4200112, i64 8)  ; 0x4017a0
  unreachable
eh.state.bad.1:  ; 0x4017a0
  call void @ra_trap(i64 4200112, i64 7)  ; 0x4017a0
  unreachable
eh.state_none.1:  ; 0x4017a0
  call void @ra_eh_suspend()  ; 0x4017a0
  %eh.head.1.f8.1 = getelementptr inbounds %S1, ptr %eh.head.1.s, i64 0, i32 2  ; 0x4017a0
  store i32 -1, ptr %eh.head.1.f8.1, align 1  ; 0x4017a0
  call void @ra_eh_resume()  ; 0x4017a0
  %eh.head.1.f0 = getelementptr inbounds %S1, ptr %eh.head.1.s, i64 0, i32 0  ; 0x4017a0
  %eh.next.2 = load i32, ptr %eh.head.1.f0, align 1  ; 0x4017a0
  %teb.12 = call i64 @ra_teb()  ; 0x4017a0
  %teb.13 = trunc i64 %teb.12 to i32  ; 0x4017a0
  %addr.z.46 = zext i32 %teb.13 to i64  ; 0x4017a0
  %st.p.29 = inttoptr i64 %addr.z.46 to ptr  ; 0x4017a0
  store i32 %eh.next.2, ptr %st.p.29, align 1  ; 0x4017a0
  ret i64 %rax.phi.2
eh.state0.1:  ; 0x4017a0
  %eh.frame.6 = add i32 %eh.reg.1, 12  ; 0x4017a0
  %eh.catch.obj.4 = add i32 %eh.frame.6, -32  ; 0x4017a0
  %eh.catch.obj.5 = zext i32 %eh.catch.obj.4 to i64  ; 0x4017a0
  %eh.caught.2 = call i64 @ra_eh_catches(i64 -1082765518775566335, i64 %eh.catch.obj.5, i64 8200)  ; 0x4017a0
  %eh.hit.2 = icmp ne i64 %eh.caught.2, 0  ; 0x4017a0
  br i1 %eh.hit.2, label %eh.catch0_0.s0.1, label %eh.next0_0.s0.1
eh.catch0_0.s0.1:  ; 0x4017a0
  %eh.head.1.f8.2 = getelementptr inbounds %S1, ptr %eh.head.1.s, i64 0, i32 2  ; 0x4017a0
  store i32 0, ptr %eh.head.1.f8.2, align 1  ; 0x4017a0
  br label %eh.enter0_0.1
eh.next0_0.s0.1:  ; 0x4017a0
  call void @ra_eh_suspend()  ; 0x40178c
  %eh.head.1.f8.3 = getelementptr inbounds %S1, ptr %eh.head.1.s, i64 0, i32 2  ; 0x40178c
  store i32 -1, ptr %eh.head.1.f8.3, align 1  ; 0x40178c
  call void @ra_eh_resume()  ; 0x40178c
  %eh.head.1.f0.1 = getelementptr inbounds %S1, ptr %eh.head.1.s, i64 0, i32 0  ; 0x40178c
  %eh.next.3 = load i32, ptr %eh.head.1.f0.1, align 1  ; 0x40178c
  %teb.14 = call i64 @ra_teb()  ; 0x40178c
  %teb.15 = trunc i64 %teb.14 to i32  ; 0x40178c
  %addr.z.62 = zext i32 %teb.15 to i64  ; 0x40178c
  %st.p.39 = inttoptr i64 %addr.z.62 to ptr  ; 0x40178c
  store i32 %eh.next.3, ptr %st.p.39, align 1  ; 0x40178c
  ret i64 %rax.phi.2
eh.enter0_0.1:  ; 0x4017a0
  %rsp.phi.2 = phi i32 [ %rsp.phi.1, %eh.catch0_0.s0.1 ], [ %sp.27, %eh.cleaned.401790.1 ]  ; 0x4017a0
  call void @ra_eh_enter_catch()  ; 0x4017a0
  %eh.head.1.f8.4 = getelementptr inbounds %S1, ptr %eh.head.1.s, i64 0, i32 2  ; 0x4017a0
  store i32 2, ptr %eh.head.1.f8.4, align 1  ; 0x4017a0
  %eh.esp.p.3 = sub i32 %eh.reg.1, 4  ; 0x4017a0
  %addr.z.49 = zext i32 %eh.esp.p.3 to i64  ; 0x4017a0
  %eh.esp.p.4 = inttoptr i64 %addr.z.49 to ptr  ; 0x4017a0
  %eh.esp.2 = load i32, ptr %eh.esp.p.4, align 1  ; 0x4017a0
  %eh.esp.3 = zext i32 %eh.esp.2 to i64  ; 0x4017a0
  call void @ra_eh_catch_sp(i64 %eh.esp.3)  ; 0x4017a0
  %eh.frame.7 = add i32 %eh.reg.1, 12  ; 0x4017a0
  %sp.15 = sub i32 %rsp.phi.2, 4  ; 0x4017a0
  %addr.z.50 = zext i32 %sp.15 to i64  ; 0x4017a0
  %st.p.32 = inttoptr i64 %addr.z.50 to ptr  ; 0x4017a0
  store i32 0, ptr %st.p.32, align 1  ; 0x4017a0
  br label %f3_401770
f3_401770:  ; 0x401770
  %sp.16 = sub i32 %sp.15, 4  ; 0x401770
  %addr.z.51 = zext i32 %sp.16 to i64  ; 0x401770
  %st.p.33 = inttoptr i64 %addr.z.51 to ptr  ; 0x401770
  store i32 %eh.frame.7, ptr %st.p.33, align 1  ; 0x401770
  %diff.3 = sub i32 %sp.16, 8  ; 0x401771
  %sum.4 = add i32 %eh.frame.7, 12  ; 0x401774
  %addr.11 = add i32 %sum.4, -28  ; 0x401777
  %addr.z.52 = zext i32 %addr.11 to i64  ; 0x401777
  %st.p.34 = inttoptr i64 %addr.z.52 to ptr  ; 0x401777
  store i32 %diff.3, ptr %st.p.34, align 1  ; 0x401777
  %addr.12 = add i32 %sum.4, -44  ; 0x40177a
  %addr.z.53 = zext i32 %addr.12 to i64  ; 0x40177a
  %m.p.5 = inttoptr i64 %addr.z.53 to ptr  ; 0x40177a
  %m.5 = load i32, ptr %m.p.5, align 1  ; 0x40177a
  %addr.13 = add i32 %m.5, 4  ; 0x40177d
  %addr.z.54 = zext i32 %addr.13 to i64  ; 0x40177d
  %m.p.6 = inttoptr i64 %addr.z.54 to ptr  ; 0x40177d
  %m.6 = load i32, ptr %m.p.6, align 1  ; 0x40177d
  %addr.14 = add i32 %sum.4, -32  ; 0x401780
  %addr.z.55 = zext i32 %addr.14 to i64  ; 0x401780
  %st.p.35 = inttoptr i64 %addr.z.55 to ptr  ; 0x401780
  store i32 %m.6, ptr %st.p.35, align 1  ; 0x401780
  %sum.5 = add i32 %diff.3, 8  ; 0x401788
  %sp.17 = add i32 %sum.5, 4  ; 0x40178b
  %sp.18 = add i32 %sp.17, 4  ; 0x40178c
  %eh.ending.1 = call i64 @ra_eh_ending_object()  ; 0x40178c
  %eh.dtor.1 = call i64 @ra_eh_ending_destructor()  ; 0x40178c
  call void @ra_eh_leave_catch()  ; 0x40178c
  %eh.dies.1 = icmp ne i64 %eh.dtor.1, 0  ; 0x40178c
  br i1 %eh.dies.1, label %eh.destroy.40178c.1, label %eh.destroyed.40178c.1
eh.destroy.40178c.1:  ; 0x40178c
  %eh.this.1 = trunc i64 %eh.ending.1 to i32  ; 0x40178c
  %ecx.z.2 = zext i32 %eh.this.1 to i64  ; 0x40178c
  %sp.19 = sub i32 %sp.18, 4  ; 0x40178c
  %addr.z.58 = zext i32 %sp.19 to i64  ; 0x40178c
  %st.p.36 = inttoptr i64 %addr.z.58 to ptr  ; 0x40178c
  store i32 0, ptr %st.p.36, align 1  ; 0x40178c
  %fp.2 = inttoptr i64 %eh.dtor.1 to ptr  ; 0x40178c
  %sarg0.a.14 = add i64 %addr.z.58, 4
  %sarg0.p.14 = inttoptr i64 %sarg0.a.14 to ptr
  %sarg0.d.14 = load i32, ptr %sarg0.p.14, align 4
  %sarg0.14 = zext i32 %sarg0.d.14 to i64
  %sarg1.a.2 = add i64 %addr.z.58, 8
  %sarg1.p.2 = inttoptr i64 %sarg1.a.2 to ptr
  %sarg1.d.2 = load i32, ptr %sarg1.p.2, align 4
  %sarg1.2 = zext i32 %sarg1.d.2 to i64
  %sarg2.a.2 = add i64 %addr.z.58, 12
  %sarg2.p.2 = inttoptr i64 %sarg2.a.2 to ptr
  %sarg2.d.2 = load i32, ptr %sarg2.p.2, align 4
  %sarg2.2 = zext i32 %sarg2.d.2 to i64
  %sarg3.a.2 = add i64 %addr.z.58, 16
  %sarg3.p.2 = inttoptr i64 %sarg3.a.2 to ptr
  %sarg3.d.2 = load i32, ptr %sarg3.p.2, align 4
  %sarg3.2 = zext i32 %sarg3.d.2 to i64
  %sarg4.a.2 = add i64 %addr.z.58, 20
  %sarg4.p.2 = inttoptr i64 %sarg4.a.2 to ptr
  %sarg4.d.2 = load i32, ptr %sarg4.p.2, align 4
  %sarg4.2 = zext i32 %sarg4.d.2 to i64
  %sarg5.a.2 = add i64 %addr.z.58, 24
  %sarg5.p.2 = inttoptr i64 %sarg5.a.2 to ptr
  %sarg5.d.2 = load i32, ptr %sarg5.p.2, align 4
  %sarg5.2 = zext i32 %sarg5.d.2 to i64
  %sarg6.a.2 = add i64 %addr.z.58, 28
  %sarg6.p.2 = inttoptr i64 %sarg6.a.2 to ptr
  %sarg6.d.2 = load i32, ptr %sarg6.p.2, align 4
  %sarg6.2 = zext i32 %sarg6.d.2 to i64
  %args.p.4 = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr  ; 0x40178c
  store i64 %sarg6.2, ptr %args.p.4, align 8  ; 0x40178c
  %sarg7.a.2 = add i64 %addr.z.58, 32
  %sarg7.p.2 = inttoptr i64 %sarg7.a.2 to ptr
  %sarg7.d.2 = load i32, ptr %sarg7.p.2, align 4
  %sarg7.2 = zext i32 %sarg7.d.2 to i64
  %args.p.5 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr  ; 0x40178c
  store i64 %sarg7.2, ptr %args.p.5, align 8  ; 0x40178c
  %ret.14 = call i64 %fp.2(i64 %sarg0.14, i64 %sarg1.2, i64 %sarg2.2, i64 %sarg3.2, i64 %sarg4.2, i64 %sarg5.2, i64 %ecx.z.2, i64 undef)  ; 0x40178c
  %pops.2 = call i64 @ra_pops_of(i64 %eh.dtor.1)  ; 0x40178c
  br label %eh.destroyed.40178c.1
eh.destroyed.40178c.1:  ; 0x40178c
  store i32 %eh.esp.2, ptr %eh.esp.p.4, align 1  ; 0x40178c
  br label %b_401759
eh.state1.1:  ; 0x40178c
  %eh.frame.8 = add i32 %eh.reg.1, 12  ; 0x40178c
  %eh.catch.obj.6 = add i32 %eh.frame.8, -32  ; 0x40178c
  %eh.catch.obj.7 = zext i32 %eh.catch.obj.6 to i64  ; 0x40178c
  %eh.caught.3 = call i64 @ra_eh_catches(i64 -1082765518775566335, i64 %eh.catch.obj.7, i64 8200)  ; 0x40178c
  %eh.hit.3 = icmp ne i64 %eh.caught.3, 0  ; 0x40178c
  br i1 %eh.hit.3, label %eh.catch0_0.s1.1, label %eh.next0_0.s1.1
eh.catch0_0.s1.1:  ; 0x40178c
  %eh.head.1.f8.5 = getelementptr inbounds %S1, ptr %eh.head.1.s, i64 0, i32 2  ; 0x40178c
  store i32 0, ptr %eh.head.1.f8.5, align 1  ; 0x40178c
  %sp.20 = sub i32 %rsp.phi.1, 4  ; 0x40178c
  %addr.z.64 = zext i32 %sp.20 to i64  ; 0x40178c
  %st.p.41 = inttoptr i64 %addr.z.64 to ptr  ; 0x40178c
  store i32 0, ptr %st.p.41, align 1  ; 0x40178c
  br label %f4_401790
eh.next0_0.s1.1:  ; 0x40178c
  call void @ra_eh_suspend()  ; 0x4017b6
  %eh.head.1.f8.6 = getelementptr inbounds %S1, ptr %eh.head.1.s, i64 0, i32 2  ; 0x4017b6
  store i32 0, ptr %eh.head.1.f8.6, align 1  ; 0x4017b6
  %sp.28 = sub i32 %rsp.phi.1, 4  ; 0x4017b6
  %addr.z.75 = zext i32 %sp.28 to i64  ; 0x4017b6
  %st.p.49 = inttoptr i64 %addr.z.75 to ptr  ; 0x4017b6
  store i32 0, ptr %st.p.49, align 1  ; 0x4017b6
  br label %f5_401790
eh.cleaned.401790.1:  ; 0x40178c
  %eh.head.1.f8.7 = getelementptr inbounds %S1, ptr %eh.head.1.s, i64 0, i32 2  ; 0x4017b6
  store i32 0, ptr %eh.head.1.f8.7, align 1  ; 0x4017b6
  br label %eh.enter0_0.1
f4_401790:  ; 0x401790
  %sp.21 = sub i32 %sp.20, 4  ; 0x401790
  %addr.z.65 = zext i32 %sp.21 to i64  ; 0x401790
  %st.p.42 = inttoptr i64 %addr.z.65 to ptr  ; 0x401790
  store i32 %eh.frame.8, ptr %st.p.42, align 1  ; 0x401790
  %diff.4 = sub i32 %sp.21, 8  ; 0x401791
  %eh.head.1.f32 = getelementptr inbounds %S1, ptr %eh.head.1.s, i64 0, i32 4  ; 0x401797
  %m.7 = load i32, ptr %eh.head.1.f32, align 1  ; 0x401797
  %sum.7 = add i32 %m.7, 230  ; 0x40179a
  %sp.22 = sub i32 %diff.4, 4  ; 0x40179f
  %addr.z.67 = zext i32 %sp.22 to i64  ; 0x40179f
  %st.p.43 = inttoptr i64 %addr.z.67 to ptr  ; 0x40179f
  store i32 %sum.7, ptr %st.p.43, align 1  ; 0x40179f
  %sp.23 = sub i32 %sp.22, 4  ; 0x4017a0
  %sarg0.a.12 = add i32 %sp.23, 4
  %ret.12 = call i64 @F_00401000(i32 %sum.7)  ; 0x4017a0
  %eh.pending.2 = call i64 @ra_eh_pending()  ; 0x4017a0
  %eh.thrown.2 = icmp ne i64 %eh.pending.2, 0  ; 0x4017a0
  br i1 %eh.thrown.2, label %eh.landing.1, label %eh.ok.4017a0
eh.ok.4017a0:  ; 0x4017a0
  %sum.8 = add i32 %sarg0.a.12, 4  ; 0x4017a5
  %sp.24 = sub i32 %sum.8, 4  ; 0x4017a8
  %addr.z.69 = zext i32 %sp.24 to i64  ; 0x4017a8
  %st.p.45 = inttoptr i64 %addr.z.69 to ptr  ; 0x4017a8
  store i32 230, ptr %st.p.45, align 1  ; 0x4017a8
  %sp.25 = sub i32 %sp.24, 4  ; 0x4017ad
  %sarg0.a.13 = add i32 %sp.25, 4
  %ret.13 = call i64 @F_00401000(i32 230)  ; 0x4017ad
  %eh.pending.3 = call i64 @ra_eh_pending()  ; 0x4017ad
  %eh.thrown.3 = icmp ne i64 %eh.pending.3, 0  ; 0x4017ad
  br i1 %eh.thrown.3, label %eh.landing.1, label %eh.ok.4017ad
eh.ok.4017ad:  ; 0x4017ad
  %sum.9 = add i32 %sarg0.a.13, 12  ; 0x4017b2
  %sp.26 = add i32 %sum.9, 4  ; 0x4017b5
  %sp.27 = add i32 %sp.26, 4  ; 0x4017b6
  br label %eh.cleaned.401790.1
eh.cleaned.401790.2:  ; 0x4017b6
  %eh.head.1.f8.8 = getelementptr inbounds %S1, ptr %eh.head.1.s, i64 0, i32 2  ; 0x4017b6
  store i32 -1, ptr %eh.head.1.f8.8, align 1  ; 0x4017b6
  call void @ra_eh_resume()  ; 0x4017b6
  %eh.head.1.f0.2 = getelementptr inbounds %S1, ptr %eh.head.1.s, i64 0, i32 0  ; 0x4017b6
  %eh.next.4 = load i32, ptr %eh.head.1.f0.2, align 1  ; 0x4017b6
  %teb.16 = call i64 @ra_teb()  ; 0x4017b6
  %teb.17 = trunc i64 %teb.16 to i32  ; 0x4017b6
  %addr.z.86 = zext i32 %teb.17 to i64  ; 0x4017b6
  %st.p.56 = inttoptr i64 %addr.z.86 to ptr  ; 0x4017b6
  store i32 %eh.next.4, ptr %st.p.56, align 1  ; 0x4017b6
  ret i64 %ret.11
f5_401790:  ; 0x401790
  %sp.29 = sub i32 %sp.28, 4  ; 0x401790
  %addr.z.76 = zext i32 %sp.29 to i64  ; 0x401790
  %st.p.50 = inttoptr i64 %addr.z.76 to ptr  ; 0x401790
  store i32 %eh.frame.8, ptr %st.p.50, align 1  ; 0x401790
  %diff.5 = sub i32 %sp.29, 8  ; 0x401791
  %eh.head.1.f32.1 = getelementptr inbounds %S1, ptr %eh.head.1.s, i64 0, i32 4  ; 0x401797
  %m.8 = load i32, ptr %eh.head.1.f32.1, align 1  ; 0x401797
  %sum.11 = add i32 %m.8, 230  ; 0x40179a
  %sp.30 = sub i32 %diff.5, 4  ; 0x40179f
  %addr.z.78 = zext i32 %sp.30 to i64  ; 0x40179f
  %st.p.51 = inttoptr i64 %addr.z.78 to ptr  ; 0x40179f
  store i32 %sum.11, ptr %st.p.51, align 1  ; 0x40179f
  %sp.31 = sub i32 %sp.30, 4  ; 0x4017a0
  %sarg0.a.10 = add i32 %sp.31, 4
  %ret.10 = call i64 @F_00401000(i32 %sum.11)  ; 0x4017a0
  %eh.pending.4 = call i64 @ra_eh_pending()  ; 0x4017a0
  %eh.thrown.4 = icmp ne i64 %eh.pending.4, 0  ; 0x4017a0
  br i1 %eh.thrown.4, label %eh.landing.1, label %eh.ok.4017a0.1
eh.ok.4017a0.1:  ; 0x4017a0
  %sum.12 = add i32 %sarg0.a.10, 4  ; 0x4017a5
  %sp.32 = sub i32 %sum.12, 4  ; 0x4017a8
  %addr.z.80 = zext i32 %sp.32 to i64  ; 0x4017a8
  %st.p.53 = inttoptr i64 %addr.z.80 to ptr  ; 0x4017a8
  store i32 230, ptr %st.p.53, align 1  ; 0x4017a8
  %sp.33 = sub i32 %sp.32, 4  ; 0x4017ad
  %sarg0.a.11 = add i32 %sp.33, 4
  %ret.11 = call i64 @F_00401000(i32 230)  ; 0x4017ad
  %eh.pending.5 = call i64 @ra_eh_pending()  ; 0x4017ad
  %eh.thrown.5 = icmp ne i64 %eh.pending.5, 0  ; 0x4017ad
  br i1 %eh.thrown.5, label %eh.landing.1, label %eh.ok.4017ad.1
eh.ok.4017ad.1:  ; 0x4017ad
  br label %eh.cleaned.401790.2
eh.state2:  ; 0x4017b6
  call void @ra_eh_suspend()  ; 0x4017b6
  %eh.esp.saved = call i64 @ra_eh_ending_sp()  ; 0x4017b6
  %eh.esp.saved.1 = trunc i64 %eh.esp.saved to i32  ; 0x4017b6
  %eh.esp.p.6 = sub i32 %eh.reg.1, 4  ; 0x4017b6
  %addr.z.87 = zext i32 %eh.esp.p.6 to i64  ; 0x4017b6
  %st.p.57 = inttoptr i64 %addr.z.87 to ptr  ; 0x4017b6
  store i32 %eh.esp.saved.1, ptr %st.p.57, align 1  ; 0x4017b6
  %eh.ending.2 = call i64 @ra_eh_ending_object()  ; 0x4017b6
  %eh.dtor.2 = call i64 @ra_eh_ending_destructor()  ; 0x4017b6
  call void @ra_eh_leave_catch()  ; 0x4017b6
  %eh.dies.2 = icmp ne i64 %eh.dtor.2, 0  ; 0x4017b6
  br i1 %eh.dies.2, label %eh.destroy.4016b0, label %eh.destroyed.4016b0
eh.destroy.4016b0:  ; 0x4017b6
  %eh.this.2 = trunc i64 %eh.ending.2 to i32  ; 0x4017b6
  %ecx.z.3 = zext i32 %eh.this.2 to i64  ; 0x4017b6
  %sp.36 = sub i32 %rsp.phi.1, 4  ; 0x4017b6
  %addr.z.88 = zext i32 %sp.36 to i64  ; 0x4017b6
  %st.p.58 = inttoptr i64 %addr.z.88 to ptr  ; 0x4017b6
  store i32 0, ptr %st.p.58, align 1  ; 0x4017b6
  %fp.1 = inttoptr i64 %eh.dtor.2 to ptr  ; 0x4017b6
  %sarg0.a.9 = add i64 %addr.z.88, 4
  %sarg0.p.9 = inttoptr i64 %sarg0.a.9 to ptr
  %sarg0.d.9 = load i32, ptr %sarg0.p.9, align 4
  %sarg0.9 = zext i32 %sarg0.d.9 to i64
  %sarg1.a.1 = add i64 %addr.z.88, 8
  %sarg1.p.1 = inttoptr i64 %sarg1.a.1 to ptr
  %sarg1.d.1 = load i32, ptr %sarg1.p.1, align 4
  %sarg1.1 = zext i32 %sarg1.d.1 to i64
  %sarg2.a.1 = add i64 %addr.z.88, 12
  %sarg2.p.1 = inttoptr i64 %sarg2.a.1 to ptr
  %sarg2.d.1 = load i32, ptr %sarg2.p.1, align 4
  %sarg2.1 = zext i32 %sarg2.d.1 to i64
  %sarg3.a.1 = add i64 %addr.z.88, 16
  %sarg3.p.1 = inttoptr i64 %sarg3.a.1 to ptr
  %sarg3.d.1 = load i32, ptr %sarg3.p.1, align 4
  %sarg3.1 = zext i32 %sarg3.d.1 to i64
  %sarg4.a.1 = add i64 %addr.z.88, 20
  %sarg4.p.1 = inttoptr i64 %sarg4.a.1 to ptr
  %sarg4.d.1 = load i32, ptr %sarg4.p.1, align 4
  %sarg4.1 = zext i32 %sarg4.d.1 to i64
  %sarg5.a.1 = add i64 %addr.z.88, 24
  %sarg5.p.1 = inttoptr i64 %sarg5.a.1 to ptr
  %sarg5.d.1 = load i32, ptr %sarg5.p.1, align 4
  %sarg5.1 = zext i32 %sarg5.d.1 to i64
  %sarg6.a.1 = add i64 %addr.z.88, 28
  %sarg6.p.1 = inttoptr i64 %sarg6.a.1 to ptr
  %sarg6.d.1 = load i32, ptr %sarg6.p.1, align 4
  %sarg6.1 = zext i32 %sarg6.d.1 to i64
  %args.p.2 = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr  ; 0x4017b6
  store i64 %sarg6.1, ptr %args.p.2, align 8  ; 0x4017b6
  %sarg7.a.1 = add i64 %addr.z.88, 32
  %sarg7.p.1 = inttoptr i64 %sarg7.a.1 to ptr
  %sarg7.d.1 = load i32, ptr %sarg7.p.1, align 4
  %sarg7.1 = zext i32 %sarg7.d.1 to i64
  %args.p.3 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr  ; 0x4017b6
  store i64 %sarg7.1, ptr %args.p.3, align 8  ; 0x4017b6
  %ret.9 = call i64 %fp.1(i64 %sarg0.9, i64 %sarg1.1, i64 %sarg2.1, i64 %sarg3.1, i64 %sarg4.1, i64 %sarg5.1, i64 %ecx.z.3, i64 undef)  ; 0x4017b6
  %pops.1 = call i64 @ra_pops_of(i64 %eh.dtor.2)  ; 0x4017b6
  br label %eh.destroyed.4016b0
eh.destroyed.4016b0:  ; 0x4017b6
  %rax.phi.3 = phi i64 [ %rax.phi.2, %eh.state2 ], [ %ret.9, %eh.destroy.4016b0 ]  ; 0x4017b6
  %eh.head.1.f8.9 = getelementptr inbounds %S1, ptr %eh.head.1.s, i64 0, i32 2  ; 0x4017b6
  store i32 -1, ptr %eh.head.1.f8.9, align 1  ; 0x4017b6
  call void @ra_eh_resume()  ; 0x4017b6
  %eh.head.1.f0.3 = getelementptr inbounds %S1, ptr %eh.head.1.s, i64 0, i32 0  ; 0x4017b6
  %eh.next.5 = load i32, ptr %eh.head.1.f0.3, align 1  ; 0x4017b6
  %teb.18 = call i64 @ra_teb()  ; 0x4017b6
  %teb.19 = trunc i64 %teb.18 to i32  ; 0x4017b6
  %addr.z.91 = zext i32 %teb.19 to i64  ; 0x4017b6
  %st.p.60 = inttoptr i64 %addr.z.91 to ptr  ; 0x4017b6
  store i32 %eh.next.5, ptr %st.p.60, align 1  ; 0x4017b6
  ret i64 %rax.phi.3
eh.ok.4017a0.2:  ; 0x4017b6
  %sum.14 = add i32 %sarg0.a.7, 4  ; 0x4017a5
  %sp.37 = sub i32 %sum.14, 4  ; 0x4017a8
  %addr.z.92 = zext i32 %sp.37 to i64  ; 0x4017a8
  %st.p.61 = inttoptr i64 %addr.z.92 to ptr  ; 0x4017a8
  store i32 230, ptr %st.p.61, align 1  ; 0x4017a8
  %sp.38 = sub i32 %sp.37, 4  ; 0x4017ad
  %sarg0.a.8 = add i32 %sp.38, 4
  %ret.8 = call i64 @F_00401000(i32 230)  ; 0x4017ad
  %eh.pending.6 = call i64 @ra_eh_pending()  ; 0x4017ad
  %eh.thrown.6 = icmp ne i64 %eh.pending.6, 0  ; 0x4017ad
  br i1 %eh.thrown.6, label %eh.landing.1, label %eh.ok.4017ad.2
eh.ok.4017ad.2:  ; 0x4017ad
  %sum.15 = add i32 %sarg0.a.8, 12  ; 0x4017b2
  %sp.39 = add i32 %sum.15, 4  ; 0x4017b5
  %sp.40 = add i32 %sp.39, 4  ; 0x4017b6
  br label %eh.cleaned.401790
eh.cleaned.401790.3:  ; 0x4017b6
  %eh.head.f8.8 = getelementptr inbounds %S1, ptr %eh.head.s, i64 0, i32 2  ; 0x4017b6
  store i32 -1, ptr %eh.head.f8.8, align 1  ; 0x4017b6
  call void @ra_eh_resume()  ; 0x4017b6
  %eh.head.f0.2 = getelementptr inbounds %S1, ptr %eh.head.s, i64 0, i32 0  ; 0x4017b6
  %eh.next.6 = load i32, ptr %eh.head.f0.2, align 1  ; 0x4017b6
  %teb.20 = call i64 @ra_teb()  ; 0x4017b6
  %teb.21 = trunc i64 %teb.20 to i32  ; 0x4017b6
  %addr.z.109 = zext i32 %teb.21 to i64  ; 0x4017b6
  %st.p.72 = inttoptr i64 %addr.z.109 to ptr  ; 0x4017b6
  store i32 %eh.next.6, ptr %st.p.72, align 1  ; 0x4017b6
  ret i64 %ret.6
f6_401790:  ; 0x401790
  %sp.42 = sub i32 %sp.41, 4  ; 0x401790
  %addr.z.99 = zext i32 %sp.42 to i64  ; 0x401790
  %st.p.66 = inttoptr i64 %addr.z.99 to ptr  ; 0x401790
  store i32 %eh.frame.3, ptr %st.p.66, align 1  ; 0x401790
  %diff.6 = sub i32 %sp.42, 8  ; 0x401791
  %eh.head.f32.1 = getelementptr inbounds %S1, ptr %eh.head.s, i64 0, i32 4  ; 0x401797
  %m.9 = load i32, ptr %eh.head.f32.1, align 1  ; 0x401797
  %sum.17 = add i32 %m.9, 230  ; 0x40179a
  %sp.43 = sub i32 %diff.6, 4  ; 0x40179f
  %addr.z.101 = zext i32 %sp.43 to i64  ; 0x40179f
  %st.p.67 = inttoptr i64 %addr.z.101 to ptr  ; 0x40179f
  store i32 %sum.17, ptr %st.p.67, align 1  ; 0x40179f
  %sp.44 = sub i32 %sp.43, 4  ; 0x4017a0
  %sarg0.a.5 = add i32 %sp.44, 4
  %ret.5 = call i64 @F_00401000(i32 %sum.17)  ; 0x4017a0
  %eh.pending.7 = call i64 @ra_eh_pending()  ; 0x4017a0
  %eh.thrown.7 = icmp ne i64 %eh.pending.7, 0  ; 0x4017a0
  br i1 %eh.thrown.7, label %eh.landing.1, label %eh.ok.4017a0.3
eh.ok.4017a0.3:  ; 0x4017a0
  %sum.18 = add i32 %sarg0.a.5, 4  ; 0x4017a5
  %sp.45 = sub i32 %sum.18, 4  ; 0x4017a8
  %addr.z.103 = zext i32 %sp.45 to i64  ; 0x4017a8
  %st.p.69 = inttoptr i64 %addr.z.103 to ptr  ; 0x4017a8
  store i32 230, ptr %st.p.69, align 1  ; 0x4017a8
  %sp.46 = sub i32 %sp.45, 4  ; 0x4017ad
  %sarg0.a.6 = add i32 %sp.46, 4
  %ret.6 = call i64 @F_00401000(i32 230)  ; 0x4017ad
  %eh.pending.8 = call i64 @ra_eh_pending()  ; 0x4017ad
  %eh.thrown.8 = icmp ne i64 %eh.pending.8, 0  ; 0x4017ad
  br i1 %eh.thrown.8, label %eh.landing.1, label %eh.ok.4017ad.3
eh.ok.4017ad.3:  ; 0x4017ad
  br label %eh.cleaned.401790.3
eh.state2.1:  ; 0x4017b6
  call void @ra_eh_suspend()  ; 0x4017b6
  %eh.esp.saved.2 = call i64 @ra_eh_ending_sp()  ; 0x4017b6
  %eh.esp.saved.3 = trunc i64 %eh.esp.saved.2 to i32  ; 0x4017b6
  %eh.esp.p.7 = sub i32 %eh.reg, 4  ; 0x4017b6
  %addr.z.110 = zext i32 %eh.esp.p.7 to i64  ; 0x4017b6
  %st.p.73 = inttoptr i64 %addr.z.110 to ptr  ; 0x4017b6
  store i32 %eh.esp.saved.3, ptr %st.p.73, align 1  ; 0x4017b6
  %eh.ending.3 = call i64 @ra_eh_ending_object()  ; 0x4017b6
  %eh.dtor.3 = call i64 @ra_eh_ending_destructor()  ; 0x4017b6
  call void @ra_eh_leave_catch()  ; 0x4017b6
  %eh.dies.3 = icmp ne i64 %eh.dtor.3, 0  ; 0x4017b6
  br i1 %eh.dies.3, label %eh.destroy.4016b0.1, label %eh.destroyed.4016b0.1
eh.destroy.4016b0.1:  ; 0x4017b6
  %eh.this.3 = trunc i64 %eh.ending.3 to i32  ; 0x4017b6
  %ecx.z.4 = zext i32 %eh.this.3 to i64  ; 0x4017b6
  %sp.49 = sub i32 %rsp.phi, 4  ; 0x4017b6
  %addr.z.111 = zext i32 %sp.49 to i64  ; 0x4017b6
  %st.p.74 = inttoptr i64 %addr.z.111 to ptr  ; 0x4017b6
  store i32 0, ptr %st.p.74, align 1  ; 0x4017b6
  %fp = inttoptr i64 %eh.dtor.3 to ptr  ; 0x4017b6
  %sarg0.a.4 = add i64 %addr.z.111, 4
  %sarg0.p.4 = inttoptr i64 %sarg0.a.4 to ptr
  %sarg0.d.4 = load i32, ptr %sarg0.p.4, align 4
  %sarg0.4 = zext i32 %sarg0.d.4 to i64
  %sarg1.a = add i64 %addr.z.111, 8
  %sarg1.p = inttoptr i64 %sarg1.a to ptr
  %sarg1.d = load i32, ptr %sarg1.p, align 4
  %sarg1 = zext i32 %sarg1.d to i64
  %sarg2.a = add i64 %addr.z.111, 12
  %sarg2.p = inttoptr i64 %sarg2.a to ptr
  %sarg2.d = load i32, ptr %sarg2.p, align 4
  %sarg2 = zext i32 %sarg2.d to i64
  %sarg3.a = add i64 %addr.z.111, 16
  %sarg3.p = inttoptr i64 %sarg3.a to ptr
  %sarg3.d = load i32, ptr %sarg3.p, align 4
  %sarg3 = zext i32 %sarg3.d to i64
  %sarg4.a = add i64 %addr.z.111, 20
  %sarg4.p = inttoptr i64 %sarg4.a to ptr
  %sarg4.d = load i32, ptr %sarg4.p, align 4
  %sarg4 = zext i32 %sarg4.d to i64
  %sarg5.a = add i64 %addr.z.111, 24
  %sarg5.p = inttoptr i64 %sarg5.a to ptr
  %sarg5.d = load i32, ptr %sarg5.p, align 4
  %sarg5 = zext i32 %sarg5.d to i64
  %sarg6.a = add i64 %addr.z.111, 28
  %sarg6.p = inttoptr i64 %sarg6.a to ptr
  %sarg6.d = load i32, ptr %sarg6.p, align 4
  %sarg6 = zext i32 %sarg6.d to i64
  %args.p = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr  ; 0x4017b6
  store i64 %sarg6, ptr %args.p, align 8  ; 0x4017b6
  %sarg7.a = add i64 %addr.z.111, 32
  %sarg7.p = inttoptr i64 %sarg7.a to ptr
  %sarg7.d = load i32, ptr %sarg7.p, align 4
  %sarg7 = zext i32 %sarg7.d to i64
  %args.p.1 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr  ; 0x4017b6
  store i64 %sarg7, ptr %args.p.1, align 8  ; 0x4017b6
  %ret.4 = call i64 %fp(i64 %sarg0.4, i64 %sarg1, i64 %sarg2, i64 %sarg3, i64 %sarg4, i64 %sarg5, i64 %ecx.z.4, i64 undef)  ; 0x4017b6
  %pops = call i64 @ra_pops_of(i64 %eh.dtor.3)  ; 0x4017b6
  br label %eh.destroyed.4016b0.1
eh.destroyed.4016b0.1:  ; 0x4017b6
  %rax.phi.1 = phi i64 [ %rax.phi, %eh.state2.1 ], [ %ret.4, %eh.destroy.4016b0.1 ]  ; 0x4017b6
  %eh.head.f8.9 = getelementptr inbounds %S1, ptr %eh.head.s, i64 0, i32 2  ; 0x4017b6
  store i32 -1, ptr %eh.head.f8.9, align 1  ; 0x4017b6
  call void @ra_eh_resume()  ; 0x4017b6
  %eh.head.f0.3 = getelementptr inbounds %S1, ptr %eh.head.s, i64 0, i32 0  ; 0x4017b6
  %eh.next.7 = load i32, ptr %eh.head.f0.3, align 1  ; 0x4017b6
  %teb.22 = call i64 @ra_teb()  ; 0x4017b6
  %teb.23 = trunc i64 %teb.22 to i32  ; 0x4017b6
  %addr.z.114 = zext i32 %teb.23 to i64  ; 0x4017b6
  %st.p.76 = inttoptr i64 %addr.z.114 to ptr  ; 0x4017b6
  store i32 %eh.next.7, ptr %st.p.76, align 1  ; 0x4017b6
  ret i64 %rax.phi.1
eh.ok.4016e5:  ; 0x4017b6
  %sum.20 = add i32 %sarg0.a, 4  ; 0x4016ea
  %addr.18 = add i32 %arg0, 130  ; 0x4016ed
  %sp.50 = sub i32 %sum.20, 4  ; 0x4016f3
  store i32 %addr.18, ptr %st.p.9, align 1  ; 0x4016f3
  %sp.51 = sub i32 %sp.50, 4  ; 0x4016f4
  %sarg0.a.1 = add i32 %sp.51, 4
  %ret.1 = call i64 @F_00401000(i32 %addr.18)  ; 0x4016f4
  %eh.pending.9 = call i64 @ra_eh_pending()  ; 0x4016f4
  %eh.thrown.9 = icmp ne i64 %eh.pending.9, 0  ; 0x4016f4
  br i1 %eh.thrown.9, label %eh.landing, label %eh.ok.4016f4
eh.ok.4016f4:  ; 0x4016f4
  %sum.21 = add i32 %sarg0.a.1, 4  ; 0x4016f9
  %addr.19 = add i32 %arg0, 60  ; 0x4016fc
  %not = icmp uge i32 %addr.19, 61  ; 0x401702
  br i1 %not, label %b_401737, label %b_401704
eh.ok.40170b:  ; 0x40170b
  %sum.23 = add i32 %sarg0.a.2, 4  ; 0x401710
  %sp.54 = sub i32 %sum.23, 4  ; 0x401713
  store i32 230, ptr %st.p.9, align 1  ; 0x401713
  %sp.55 = sub i32 %sp.54, 4  ; 0x401718
  %sarg0.a.3 = add i32 %sp.55, 4
  %ret.3 = call i64 @F_00401000(i32 230)  ; 0x401718
  %eh.pending.11 = call i64 @ra_eh_pending()  ; 0x401718
  %eh.thrown.11 = icmp ne i64 %eh.pending.11, 0  ; 0x401718
  br i1 %eh.thrown.11, label %eh.landing, label %eh.ok.401718
eh.ok.401718:  ; 0x401718
  br label %b_401725
}

define i64 @F_004017c0(i32 %arg0) {  ; 0x4017c0
b_4017c0:  ; 0x4017c0
  %local_p4 = alloca i32, align 4
  store i32 %arg0, ptr %local_p4, align 4
  %addr.z.1 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 320) to i32) to i64  ; 0x4017c4
  %st.p = inttoptr i64 %addr.z.1 to ptr  ; 0x4017c4
  store i32 %arg0, ptr %st.p, align 1  ; 0x4017c4
  %zf.1 = icmp eq i32 %arg0, -536866252  ; 0x4017cc
  %al.z = zext i1 %zf.1 to i64  ; 0x4017d2
  ret i64 %al.z
}

define i64 @F_004017e0(i32 %arg0) {  ; 0x4017e0
b_4017e0:  ; 0x4017e0
  %frame = alloca [4300 x i8], align 16
  %frame.top = getelementptr inbounds i8, ptr %frame, i64 4220
  %rsp0 = ptrtoint ptr %frame.top to i64
  %stack.arg0.p = getelementptr inbounds i8, ptr %frame.top, i64 4
  store i32 %arg0, ptr %stack.arg0.p, align 4
  %ebp = trunc i64 undef to i32  ; 0x4017e0
  %esp = trunc i64 %rsp0 to i32  ; 0x4017e0
  %sp = sub i32 %esp, 4  ; 0x4017e0
  %st.p = getelementptr inbounds i8, ptr %frame.top, i64 -4  ; 0x4017e0
  store i32 %ebp, ptr %st.p, align 1  ; 0x4017e0
  %sp.1 = sub i32 %sp, 4  ; 0x4017e3
  %st.p.1 = getelementptr inbounds i8, ptr %frame.top, i64 -8  ; 0x4017e3
  store i32 %ebp, ptr %st.p.1, align 1  ; 0x4017e3
  %sp.2 = sub i32 %sp.1, 4  ; 0x4017e4
  %st.p.2 = getelementptr inbounds i8, ptr %frame.top, i64 -12  ; 0x4017e4
  store i32 %ebp, ptr %st.p.2, align 1  ; 0x4017e4
  %sp.3 = sub i32 %sp.2, 4  ; 0x4017e5
  %st.p.3 = getelementptr inbounds i8, ptr %frame.top, i64 -16  ; 0x4017e5
  store i32 %ebp, ptr %st.p.3, align 1  ; 0x4017e5
  %diff = sub i32 %sp.3, 36  ; 0x4017e6
  %st.p.4 = getelementptr inbounds i8, ptr %frame.top, i64 -40  ; 0x4017ec
  store i32 %diff, ptr %st.p.4, align 1  ; 0x4017ec
  %st.p.5 = getelementptr inbounds i8, ptr %frame.top, i64 -20  ; 0x4017ef
  store i32 -1, ptr %st.p.5, align 1  ; 0x4017ef
  %st.p.6 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x4017f6
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 3580) to i32), ptr %st.p.6, align 1  ; 0x4017f6
  %addr.4 = add i32 %sp, -28  ; 0x4017fd
  %st.p.7 = getelementptr inbounds i8, ptr %frame.top, i64 -28  ; 0x401800
  store i32 ptrtoint (ptr @U_00403285 to i32), ptr %st.p.7, align 1  ; 0x401800
  %teb = call i64 @ra_teb()  ; 0x401807
  %teb.1 = trunc i64 %teb to i32  ; 0x401807
  %addr.z.9 = zext i32 %teb.1 to i64  ; 0x401807
  %m.p.1 = inttoptr i64 %addr.z.9 to ptr  ; 0x401807
  %m.1 = load i32, ptr %m.p.1, align 1  ; 0x401807
  %st.p.8 = getelementptr inbounds i8, ptr %frame.top, i64 -32  ; 0x40180e
  store i32 %m.1, ptr %st.p.8, align 1  ; 0x40180e
  %teb.2 = call i64 @ra_teb()  ; 0x401811
  %teb.3 = trunc i64 %teb.2 to i32  ; 0x401811
  %addr.z.11 = zext i32 %teb.3 to i64  ; 0x401811
  %st.p.9 = inttoptr i64 %addr.z.11 to ptr  ; 0x401811
  store i32 %addr.4, ptr %st.p.9, align 1  ; 0x401811
  %st.p.10 = getelementptr inbounds i8, ptr %frame.top, i64 -44  ; 0x401818
  store i32 %arg0, ptr %st.p.10, align 1  ; 0x401818
  %addr.8 = add i32 %sp, -44  ; 0x40181b
  %st.p.11 = getelementptr inbounds i8, ptr %frame.top, i64 -48  ; 0x40181b
  store i32 %arg0, ptr %st.p.11, align 1  ; 0x40181b
  store i32 1, ptr %st.p.5, align 1  ; 0x40181e
  %sp.4 = sub i32 %diff, 4  ; 0x401828
  %st.p.13 = getelementptr inbounds i8, ptr %frame.top, i64 -56  ; 0x401828
  store i32 %addr.8, ptr %st.p.13, align 1  ; 0x401828
  %sp.5 = sub i32 %sp.4, 4  ; 0x401829
  %st.p.14 = getelementptr inbounds i8, ptr %frame.top, i64 -60  ; 0x401829
  store i32 1, ptr %st.p.14, align 1  ; 0x401829
  %sp.6 = sub i32 %sp.5, 4  ; 0x40182b
  %st.p.15 = getelementptr inbounds i8, ptr %frame.top, i64 -64  ; 0x40182b
  store i32 0, ptr %st.p.15, align 1  ; 0x40182b
  %sp.7 = sub i32 %sp.6, 4  ; 0x40182d
  %st.p.16 = getelementptr inbounds i8, ptr %frame.top, i64 -68  ; 0x40182d
  store i32 -536866252, ptr %st.p.16, align 1  ; 0x40182d
  br label %eh.probe.401832
b_401857:  ; 0x401857
  %rax.phi = phi i32 [ %addr.45, %b_401876 ], [ %m.22, %eh.access.401854 ]  ; 0x401857
  %rsp.phi.4 = phi i32 [ %m.33, %b_401876 ], [ %rsp.phi.3, %eh.access.401854 ]  ; 0x401857
  %rbp.phi.5 = phi i32 [ %sum.16, %b_401876 ], [ %sp, %eh.access.401854 ]  ; 0x401857
  %rsi.phi.5 = phi i32 [ %rsi.phi.4, %b_401876 ], [ %rsi.phi.3, %eh.access.401854 ]  ; 0x401857
  br label %eh.probe.401857
b_401876:  ; 0x401876
  %rbp.phi.4 = phi i32 [ %eh.handler.frame, %eh.finally.401832.1 ], [ %eh.handler.frame.1, %eh.finally.401832.1.1 ], [ %eh.handler.frame.2, %eh.finally.401838.1 ], [ %eh.handler.frame.3, %eh.finally.40183f.1 ], [ %eh.handler.frame.4, %eh.except.401854.0 ], [ %eh.handler.frame.5, %eh.except.401857.0 ], [ %eh.handler.frame.6, %eh.except.40185d.0 ]  ; 0x401876
  %rsi.phi.4 = phi i32 [ %pop, %eh.finally.401832.1 ], [ %pop.3, %eh.finally.401832.1.1 ], [ %pop.6, %eh.finally.401838.1 ], [ %pop.9, %eh.finally.40183f.1 ], [ %pop.12, %eh.except.401854.0 ], [ %pop.14, %eh.except.401857.0 ], [ %pop.16, %eh.except.40185d.0 ]  ; 0x401876
  %m.33 = load i32, ptr %st.p.4, align 1  ; 0x401876
  %sum.16 = add i32 %rbp.phi.4, 12  ; 0x401879
  %m.34 = load i32, ptr %st.p.10, align 1  ; 0x40187c
  %scaled.1 = mul i32 %m.34, 10  ; 0x401882
  %addr.45 = add i32 %scaled.1, 7  ; 0x401882
  store i32 -1, ptr %st.p.5, align 1  ; 0x401889
  br label %b_401857
eh.probe.401832:  ; 0x401832
  %rsp.phi = phi i32 [ %sp.7, %b_4017e0 ], [ %sp.15, %eh.resume.401832.0 ]  ; 0x401832
  %rsi.phi = phi i32 [ undef, %b_4017e0 ], [ %pop, %eh.resume.401832.0 ]  ; 0x401832
  %eh.at = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 1284) to i32) to i64  ; 0x401832
  %eh.fault = call i64 @ra_eh_probe(i64 %eh.at, i64 4, i64 0, i64 4200498)  ; 0x401832
  %eh.faults = icmp ne i64 %eh.fault, 0  ; 0x401832
  br i1 %eh.faults, label %eh.seh.401832, label %eh.access.401832
eh.seh.401832:  ; 0x401832
  %eh.node = add i32 %sp, -28  ; 0x401832
  %eh.handler.frame = add i32 %eh.node, 16  ; 0x401832
  %eh.pointers = call i64 @ra_eh_pointers()  ; 0x401832
  %eh.pointers.1 = trunc i64 %eh.pointers to i32  ; 0x401832
  %st.p.17 = getelementptr inbounds i8, ptr %frame.top, i64 -36  ; 0x401832
  store i32 %eh.pointers.1, ptr %st.p.17, align 1  ; 0x401832
  %sp.8 = sub i32 %rsp.phi, 4  ; 0x401832
  %st.p.18 = getelementptr inbounds i8, ptr %frame.top, i64 -72  ; 0x401832
  store i32 0, ptr %st.p.18, align 1  ; 0x401832
  br label %f1_4018c0
eh.filtered.401832.0:  ; 0x401832
  %eax.6 = trunc i64 %ret.4 to i32  ; 0x4018ed
  %eh.execute = icmp sgt i32 %eax.6, 0  ; 0x4018ed
  br i1 %eh.execute, label %eh.except.401832.0, label %eh.verdict.401832.0
f1_4018c0:  ; 0x4018c0
  %sp.9 = sub i32 %sp.8, 4  ; 0x4018c0
  %st.p.19 = getelementptr inbounds i8, ptr %frame.top, i64 -76  ; 0x4018c0
  store i32 %eh.handler.frame, ptr %st.p.19, align 1  ; 0x4018c0
  %sp.10 = sub i32 %sp.9, 4  ; 0x4018c3
  %st.p.20 = getelementptr inbounds i8, ptr %frame.top, i64 -80  ; 0x4018c3
  store i32 %rsi.phi, ptr %st.p.20, align 1  ; 0x4018c3
  %addr.11 = add i32 %eh.handler.frame, -20  ; 0x4018cc
  %addr.z.24 = zext i32 %addr.11 to i64  ; 0x4018cc
  %m.p.3 = inttoptr i64 %addr.z.24 to ptr  ; 0x4018cc
  %m.3 = load i32, ptr %m.p.3, align 1  ; 0x4018cc
  %edx.z.1 = zext i32 %m.3 to i64  ; 0x4018cc
  %m.p.4 = inttoptr i64 %edx.z.1 to ptr  ; 0x4018cf
  %m.4 = load i32, ptr %m.p.4, align 1  ; 0x4018cf
  %edx.z.2 = zext i32 %m.4 to i64  ; 0x4018cf
  %m.p.5 = inttoptr i64 %edx.z.2 to ptr  ; 0x4018d1
  %m.5 = load i32, ptr %m.p.5, align 1  ; 0x4018d1
  %diff.1 = sub i32 %eh.handler.frame, -36  ; 0x4018d8
  %addr.12 = add i32 -48, %diff.1  ; 0x4018da
  %addr.13 = add i32 %addr.12, -24  ; 0x4018da
  %addr.z.27 = zext i32 %addr.13 to i64  ; 0x4018da
  %st.p.21 = inttoptr i64 %addr.z.27 to ptr  ; 0x4018da
  store i32 %m.5, ptr %st.p.21, align 1  ; 0x4018da
  %sp.11 = sub i32 %sp.10, 4  ; 0x4018de
  %st.p.22 = getelementptr inbounds i8, ptr %frame.top, i64 -84  ; 0x4018de
  store i32 -536866252, ptr %st.p.22, align 1  ; 0x4018de
  %sp.12 = sub i32 %sp.11, 4  ; 0x4018e3
  %sarg0.a.4 = add i32 %sp.12, 4
  %ret.4 = call i64 @F_004017c0(i32 -536866252)  ; 0x4018e3
  %eh.pending = call i64 @ra_eh_pending()  ; 0x4018e3
  %eh.thrown = icmp ne i64 %eh.pending, 0  ; 0x4018e3
  br i1 %eh.thrown, label %eh.landing, label %eh.ok.4018e3
eh.landing:  ; 0x4018e3
  call void @ra_trap(i64 4200675, i64 8)  ; 0x4018e3
  unreachable
eh.ok.4018e3:  ; 0x4018e3
  %sum = add i32 %sarg0.a.4, 4  ; 0x4018e8
  %pop = load i32, ptr %st.p.20, align 1  ; 0x4018eb
  %sp.13 = add i32 %sum, 4  ; 0x4018eb
  %sp.14 = add i32 %sp.13, 4  ; 0x4018ec
  %sp.15 = add i32 %sp.14, 4  ; 0x4018ed
  br label %eh.filtered.401832.0
eh.except.401832.0:  ; 0x4018ed
  store i32 0, ptr %st.p.5, align 1  ; 0x4018ed
  store i32 0, ptr %st.p.18, align 1  ; 0x4018ed
  br label %f2_4018a0
eh.verdict.401832.0:  ; 0x4018ed
  %eh.continue = icmp slt i32 %eax.6, 0  ; 0x4018b2
  br i1 %eh.continue, label %eh.resume.401832.0, label %eh.search.401832.0
eh.finally.401832.1:  ; 0x4018ed
  call void @ra_eh_handled()  ; 0x4018b2
  store i32 -1, ptr %st.p.5, align 1  ; 0x4018b2
  br label %b_401876
f2_4018a0:  ; 0x4018a0
  store i32 %eh.handler.frame, ptr %st.p.19, align 1  ; 0x4018a0
  %sum.1 = add i32 %eh.handler.frame, 12  ; 0x4018a4
  call void @F_004018f0(i32 %sum.1)  ; 0x4018a9
  %eh.pending.1 = call i64 @ra_eh_pending()  ; 0x4018a9
  %eh.thrown.1 = icmp ne i64 %eh.pending.1, 0  ; 0x4018a9
  br i1 %eh.thrown.1, label %eh.landing, label %eh.ok.4018a9
eh.ok.4018a9:  ; 0x4018a9
  br label %eh.finally.401832.1
eh.resume.401832.0:  ; 0x4018b2
  call void @ra_eh_handled()  ; 0x4018b2
  br label %eh.probe.401832
eh.search.401832.0:  ; 0x4018b2
  call void @ra_eh_unhandled(i64 4200498)  ; 0x4018b2
  unreachable
eh.access.401832:  ; 0x4018b2
  %eh.raise.arg = load i32, ptr %st.p.16, align 1  ; 0x4018b2
  %eh.raise.arg.1 = zext i32 %eh.raise.arg to i64  ; 0x4018b2
  %eh.raise.arg.2 = load i32, ptr %st.p.15, align 1  ; 0x4018b2
  %eh.raise.arg.3 = zext i32 %eh.raise.arg.2 to i64  ; 0x4018b2
  %eh.raise.arg.4 = load i32, ptr %st.p.14, align 1  ; 0x4018b2
  %eh.raise.arg.5 = zext i32 %eh.raise.arg.4 to i64  ; 0x4018b2
  %eh.raise.arg.6 = load i32, ptr %st.p.13, align 1  ; 0x4018b2
  %eh.raise.arg.7 = zext i32 %eh.raise.arg.6 to i64  ; 0x4018b2
  call void @ra_eh_raise(i64 %eh.raise.arg.1, i64 %eh.raise.arg.3, i64 %eh.raise.arg.5, i64 %eh.raise.arg.7, i64 4200498)  ; 0x4018b2
  br label %eh.seh.401832.1
eh.raised.401832:  ; 0x4018b2
  %sp.stdcall = add i32 %sp.28, 16  ; 0x4018b2
  br label %eh.probe.401838
eh.seh.401832.1:  ; 0x4018b2
  %eh.node.1 = add i32 %sp, -28  ; 0x4018b2
  %eh.handler.frame.1 = add i32 %eh.node.1, 16  ; 0x4018b2
  %eh.pointers.2 = call i64 @ra_eh_pointers()  ; 0x4018b2
  %eh.pointers.3 = trunc i64 %eh.pointers.2 to i32  ; 0x4018b2
  %st.p.29 = getelementptr inbounds i8, ptr %frame.top, i64 -36  ; 0x4018b2
  store i32 %eh.pointers.3, ptr %st.p.29, align 1  ; 0x4018b2
  %sp.21 = sub i32 %rsp.phi, 4  ; 0x4018b2
  %st.p.30 = getelementptr inbounds i8, ptr %frame.top, i64 -72  ; 0x4018b2
  store i32 0, ptr %st.p.30, align 1  ; 0x4018b2
  br label %f3_4018c0
eh.filtered.401832.0.1:  ; 0x4018b2
  %eax.10 = trunc i64 %ret to i32  ; 0x4018ed
  %eh.execute.1 = icmp sgt i32 %eax.10, 0  ; 0x4018ed
  br i1 %eh.execute.1, label %eh.except.401832.0.1, label %eh.verdict.401832.0.1
f3_4018c0:  ; 0x4018c0
  %sp.22 = sub i32 %sp.21, 4  ; 0x4018c0
  %st.p.31 = getelementptr inbounds i8, ptr %frame.top, i64 -76  ; 0x4018c0
  store i32 %eh.handler.frame.1, ptr %st.p.31, align 1  ; 0x4018c0
  %sp.23 = sub i32 %sp.22, 4  ; 0x4018c3
  %st.p.32 = getelementptr inbounds i8, ptr %frame.top, i64 -80  ; 0x4018c3
  store i32 %rsi.phi, ptr %st.p.32, align 1  ; 0x4018c3
  %addr.14 = add i32 %eh.handler.frame.1, -20  ; 0x4018cc
  %addr.z.49 = zext i32 %addr.14 to i64  ; 0x4018cc
  %m.p.7 = inttoptr i64 %addr.z.49 to ptr  ; 0x4018cc
  %m.7 = load i32, ptr %m.p.7, align 1  ; 0x4018cc
  %edx.z.4 = zext i32 %m.7 to i64  ; 0x4018cc
  %m.p.8 = inttoptr i64 %edx.z.4 to ptr  ; 0x4018cf
  %m.8 = load i32, ptr %m.p.8, align 1  ; 0x4018cf
  %edx.z.5 = zext i32 %m.8 to i64  ; 0x4018cf
  %m.p.9 = inttoptr i64 %edx.z.5 to ptr  ; 0x4018d1
  %m.9 = load i32, ptr %m.p.9, align 1  ; 0x4018d1
  %diff.3 = sub i32 %eh.handler.frame.1, -36  ; 0x4018d8
  %addr.15 = add i32 -48, %diff.3  ; 0x4018da
  %addr.16 = add i32 %addr.15, -24  ; 0x4018da
  %addr.z.52 = zext i32 %addr.16 to i64  ; 0x4018da
  %st.p.33 = inttoptr i64 %addr.z.52 to ptr  ; 0x4018da
  store i32 %m.9, ptr %st.p.33, align 1  ; 0x4018da
  %sp.24 = sub i32 %sp.23, 4  ; 0x4018de
  %st.p.34 = getelementptr inbounds i8, ptr %frame.top, i64 -84  ; 0x4018de
  store i32 -536866252, ptr %st.p.34, align 1  ; 0x4018de
  %sp.25 = sub i32 %sp.24, 4  ; 0x4018e3
  %sarg0.a = add i32 %sp.25, 4
  %ret = call i64 @F_004017c0(i32 -536866252)  ; 0x4018e3
  %eh.pending.2 = call i64 @ra_eh_pending()  ; 0x4018e3
  %eh.thrown.2 = icmp ne i64 %eh.pending.2, 0  ; 0x4018e3
  br i1 %eh.thrown.2, label %eh.landing, label %eh.ok.4018e3.1
eh.ok.4018e3.1:  ; 0x4018e3
  %sum.3 = add i32 %sarg0.a, 4  ; 0x4018e8
  %pop.3 = load i32, ptr %st.p.32, align 1  ; 0x4018eb
  %sp.26 = add i32 %sum.3, 4  ; 0x4018eb
  %sp.27 = add i32 %sp.26, 4  ; 0x4018ec
  %sp.28 = add i32 %sp.27, 4  ; 0x4018ed
  br label %eh.filtered.401832.0.1
eh.except.401832.0.1:  ; 0x4018ed
  store i32 0, ptr %st.p.5, align 1  ; 0x4018ed
  store i32 0, ptr %st.p.30, align 1  ; 0x4018ed
  br label %f4_4018a0
eh.verdict.401832.0.1:  ; 0x4018ed
  %eh.continue.1 = icmp slt i32 %eax.10, 0  ; 0x4018b2
  br i1 %eh.continue.1, label %eh.resume.401832.0.1, label %eh.search.401832.0.1
eh.finally.401832.1.1:  ; 0x4018ed
  call void @ra_eh_handled()  ; 0x4018b2
  store i32 -1, ptr %st.p.5, align 1  ; 0x4018b2
  br label %b_401876
f4_4018a0:  ; 0x4018a0
  store i32 %eh.handler.frame.1, ptr %st.p.31, align 1  ; 0x4018a0
  %sum.4 = add i32 %eh.handler.frame.1, 12  ; 0x4018a4
  call void @F_004018f0(i32 %sum.4)  ; 0x4018a9
  %eh.pending.3 = call i64 @ra_eh_pending()  ; 0x4018a9
  %eh.thrown.3 = icmp ne i64 %eh.pending.3, 0  ; 0x4018a9
  br i1 %eh.thrown.3, label %eh.landing, label %eh.ok.4018a9.1
eh.ok.4018a9.1:  ; 0x4018a9
  br label %eh.finally.401832.1.1
eh.resume.401832.0.1:  ; 0x4018b2
  call void @ra_eh_handled()  ; 0x4018b2
  br label %eh.raised.401832
eh.search.401832.0.1:  ; 0x4018b2
  call void @ra_eh_unhandled(i64 4200498)  ; 0x4018b2
  unreachable
eh.probe.401838:  ; 0x401838
  %rsp.phi.1 = phi i32 [ %sp.stdcall, %eh.raised.401832 ], [ %sp.41, %eh.resume.401838.0 ]  ; 0x401838
  %rsi.phi.1 = phi i32 [ %pop.3, %eh.raised.401832 ], [ %pop.6, %eh.resume.401838.0 ]  ; 0x401838
  %addr.17 = add i32 %sp, -40  ; 0x401838
  %eh.at.1 = zext i32 %addr.17 to i64  ; 0x401838
  %eh.fault.1 = call i64 @ra_eh_probe(i64 %eh.at.1, i64 4, i64 1, i64 4200504)  ; 0x401838
  %eh.faults.1 = icmp ne i64 %eh.fault.1, 0  ; 0x401838
  br i1 %eh.faults.1, label %eh.seh.401838, label %eh.access.401838
eh.seh.401838:  ; 0x401838
  %eh.node.2 = add i32 %sp, -28  ; 0x401838
  %eh.handler.frame.2 = add i32 %eh.node.2, 16  ; 0x401838
  %eh.pointers.4 = call i64 @ra_eh_pointers()  ; 0x401838
  %eh.pointers.5 = trunc i64 %eh.pointers.4 to i32  ; 0x401838
  store i32 %eh.pointers.5, ptr %st.p.29, align 1  ; 0x401838
  %sp.34 = sub i32 %rsp.phi.1, 4  ; 0x401838
  store i32 0, ptr %st.p.13, align 1  ; 0x401838
  br label %f5_4018c0
eh.filtered.401838.0:  ; 0x401838
  %eax.14 = trunc i64 %ret.3 to i32  ; 0x4018ed
  %eh.execute.2 = icmp sgt i32 %eax.14, 0  ; 0x4018ed
  br i1 %eh.execute.2, label %eh.except.401838.0, label %eh.verdict.401838.0
f5_4018c0:  ; 0x4018c0
  %sp.35 = sub i32 %sp.34, 4  ; 0x4018c0
  store i32 %eh.handler.frame.2, ptr %st.p.14, align 1  ; 0x4018c0
  %sp.36 = sub i32 %sp.35, 4  ; 0x4018c3
  store i32 %rsi.phi.1, ptr %st.p.15, align 1  ; 0x4018c3
  %addr.18 = add i32 %eh.handler.frame.2, -20  ; 0x4018cc
  %addr.z.70 = zext i32 %addr.18 to i64  ; 0x4018cc
  %m.p.11 = inttoptr i64 %addr.z.70 to ptr  ; 0x4018cc
  %m.11 = load i32, ptr %m.p.11, align 1  ; 0x4018cc
  %edx.z.7 = zext i32 %m.11 to i64  ; 0x4018cc
  %m.p.12 = inttoptr i64 %edx.z.7 to ptr  ; 0x4018cf
  %m.12 = load i32, ptr %m.p.12, align 1  ; 0x4018cf
  %edx.z.8 = zext i32 %m.12 to i64  ; 0x4018cf
  %m.p.13 = inttoptr i64 %edx.z.8 to ptr  ; 0x4018d1
  %m.13 = load i32, ptr %m.p.13, align 1  ; 0x4018d1
  %diff.5 = sub i32 %eh.handler.frame.2, -36  ; 0x4018d8
  %addr.19 = add i32 -48, %diff.5  ; 0x4018da
  %addr.20 = add i32 %addr.19, -24  ; 0x4018da
  %addr.z.73 = zext i32 %addr.20 to i64  ; 0x4018da
  %st.p.45 = inttoptr i64 %addr.z.73 to ptr  ; 0x4018da
  store i32 %m.13, ptr %st.p.45, align 1  ; 0x4018da
  %sp.37 = sub i32 %sp.36, 4  ; 0x4018de
  store i32 -536866252, ptr %st.p.16, align 1  ; 0x4018de
  %sp.38 = sub i32 %sp.37, 4  ; 0x4018e3
  %sarg0.a.3 = add i32 %sp.38, 4
  %ret.3 = call i64 @F_004017c0(i32 -536866252)  ; 0x4018e3
  %eh.pending.4 = call i64 @ra_eh_pending()  ; 0x4018e3
  %eh.thrown.4 = icmp ne i64 %eh.pending.4, 0  ; 0x4018e3
  br i1 %eh.thrown.4, label %eh.landing, label %eh.ok.4018e3.2
eh.ok.4018e3.2:  ; 0x4018e3
  %sum.6 = add i32 %sarg0.a.3, 4  ; 0x4018e8
  %pop.6 = load i32, ptr %st.p.15, align 1  ; 0x4018eb
  %sp.39 = add i32 %sum.6, 4  ; 0x4018eb
  %sp.40 = add i32 %sp.39, 4  ; 0x4018ec
  %sp.41 = add i32 %sp.40, 4  ; 0x4018ed
  br label %eh.filtered.401838.0
eh.except.401838.0:  ; 0x4018ed
  store i32 0, ptr %st.p.5, align 1  ; 0x4018ed
  store i32 0, ptr %st.p.13, align 1  ; 0x4018ed
  br label %f6_4018a0
eh.verdict.401838.0:  ; 0x4018ed
  %eh.continue.2 = icmp slt i32 %eax.14, 0  ; 0x4018b2
  br i1 %eh.continue.2, label %eh.resume.401838.0, label %eh.search.401838.0
eh.finally.401838.1:  ; 0x4018ed
  call void @ra_eh_handled()  ; 0x4018b2
  store i32 -1, ptr %st.p.5, align 1  ; 0x4018b2
  br label %b_401876
f6_4018a0:  ; 0x4018a0
  store i32 %eh.handler.frame.2, ptr %st.p.14, align 1  ; 0x4018a0
  %sum.7 = add i32 %eh.handler.frame.2, 12  ; 0x4018a4
  call void @F_004018f0(i32 %sum.7)  ; 0x4018a9
  %eh.pending.5 = call i64 @ra_eh_pending()  ; 0x4018a9
  %eh.thrown.5 = icmp ne i64 %eh.pending.5, 0  ; 0x4018a9
  br i1 %eh.thrown.5, label %eh.landing, label %eh.ok.4018a9.2
eh.ok.4018a9.2:  ; 0x4018a9
  br label %eh.finally.401838.1
eh.resume.401838.0:  ; 0x4018b2
  call void @ra_eh_handled()  ; 0x4018b2
  br label %eh.probe.401838
eh.search.401838.0:  ; 0x4018b2
  call void @ra_eh_unhandled(i64 4200504)  ; 0x4018b2
  unreachable
eh.access.401838:  ; 0x4018b2
  store i32 999, ptr %st.p.10, align 1  ; 0x4018b2
  br label %eh.probe.40183f
eh.probe.40183f:  ; 0x40183f
  %rsp.phi.2 = phi i32 [ %rsp.phi.1, %eh.access.401838 ], [ %sp.54, %eh.resume.40183f.0 ]  ; 0x40183f
  %rsi.phi.2 = phi i32 [ %rsi.phi.1, %eh.access.401838 ], [ %pop.9, %eh.resume.40183f.0 ]  ; 0x40183f
  %addr.22 = add i32 %sp, -16  ; 0x40183f
  %eh.at.2 = zext i32 %addr.22 to i64  ; 0x40183f
  %eh.fault.2 = call i64 @ra_eh_probe(i64 %eh.at.2, i64 4, i64 1, i64 4200511)  ; 0x40183f
  %eh.faults.2 = icmp ne i64 %eh.fault.2, 0  ; 0x40183f
  br i1 %eh.faults.2, label %eh.seh.40183f, label %eh.access.40183f
eh.seh.40183f:  ; 0x40183f
  %eh.node.3 = add i32 %sp, -28  ; 0x40183f
  %eh.handler.frame.3 = add i32 %eh.node.3, 16  ; 0x40183f
  %eh.pointers.6 = call i64 @ra_eh_pointers()  ; 0x40183f
  %eh.pointers.7 = trunc i64 %eh.pointers.6 to i32  ; 0x40183f
  store i32 %eh.pointers.7, ptr %st.p.29, align 1  ; 0x40183f
  %sp.47 = sub i32 %rsp.phi.2, 4  ; 0x40183f
  store i32 0, ptr %st.p.13, align 1  ; 0x40183f
  br label %f7_4018c0
eh.filtered.40183f.0:  ; 0x40183f
  %eax.18 = trunc i64 %ret.2 to i32  ; 0x4018ed
  %eh.execute.3 = icmp sgt i32 %eax.18, 0  ; 0x4018ed
  br i1 %eh.execute.3, label %eh.except.40183f.0, label %eh.verdict.40183f.0
f7_4018c0:  ; 0x4018c0
  %sp.48 = sub i32 %sp.47, 4  ; 0x4018c0
  store i32 %eh.handler.frame.3, ptr %st.p.14, align 1  ; 0x4018c0
  %sp.49 = sub i32 %sp.48, 4  ; 0x4018c3
  store i32 %rsi.phi.2, ptr %st.p.15, align 1  ; 0x4018c3
  %addr.23 = add i32 %eh.handler.frame.3, -20  ; 0x4018cc
  %addr.z.92 = zext i32 %addr.23 to i64  ; 0x4018cc
  %m.p.15 = inttoptr i64 %addr.z.92 to ptr  ; 0x4018cc
  %m.15 = load i32, ptr %m.p.15, align 1  ; 0x4018cc
  %edx.z.10 = zext i32 %m.15 to i64  ; 0x4018cc
  %m.p.16 = inttoptr i64 %edx.z.10 to ptr  ; 0x4018cf
  %m.16 = load i32, ptr %m.p.16, align 1  ; 0x4018cf
  %edx.z.11 = zext i32 %m.16 to i64  ; 0x4018cf
  %m.p.17 = inttoptr i64 %edx.z.11 to ptr  ; 0x4018d1
  %m.17 = load i32, ptr %m.p.17, align 1  ; 0x4018d1
  %diff.7 = sub i32 %eh.handler.frame.3, -36  ; 0x4018d8
  %addr.24 = add i32 -48, %diff.7  ; 0x4018da
  %addr.25 = add i32 %addr.24, -24  ; 0x4018da
  %addr.z.95 = zext i32 %addr.25 to i64  ; 0x4018da
  %st.p.58 = inttoptr i64 %addr.z.95 to ptr  ; 0x4018da
  store i32 %m.17, ptr %st.p.58, align 1  ; 0x4018da
  %sp.50 = sub i32 %sp.49, 4  ; 0x4018de
  store i32 -536866252, ptr %st.p.16, align 1  ; 0x4018de
  %sp.51 = sub i32 %sp.50, 4  ; 0x4018e3
  %sarg0.a.2 = add i32 %sp.51, 4
  %ret.2 = call i64 @F_004017c0(i32 -536866252)  ; 0x4018e3
  %eh.pending.6 = call i64 @ra_eh_pending()  ; 0x4018e3
  %eh.thrown.6 = icmp ne i64 %eh.pending.6, 0  ; 0x4018e3
  br i1 %eh.thrown.6, label %eh.landing, label %eh.ok.4018e3.3
eh.ok.4018e3.3:  ; 0x4018e3
  %sum.9 = add i32 %sarg0.a.2, 4  ; 0x4018e8
  %pop.9 = load i32, ptr %st.p.15, align 1  ; 0x4018eb
  %sp.52 = add i32 %sum.9, 4  ; 0x4018eb
  %sp.53 = add i32 %sp.52, 4  ; 0x4018ec
  %sp.54 = add i32 %sp.53, 4  ; 0x4018ed
  br label %eh.filtered.40183f.0
eh.except.40183f.0:  ; 0x4018ed
  store i32 0, ptr %st.p.5, align 1  ; 0x4018ed
  store i32 0, ptr %st.p.13, align 1  ; 0x4018ed
  br label %f8_4018a0
eh.verdict.40183f.0:  ; 0x4018ed
  %eh.continue.3 = icmp slt i32 %eax.18, 0  ; 0x4018b2
  br i1 %eh.continue.3, label %eh.resume.40183f.0, label %eh.search.40183f.0
eh.finally.40183f.1:  ; 0x4018ed
  call void @ra_eh_handled()  ; 0x4018b2
  store i32 -1, ptr %st.p.5, align 1  ; 0x4018b2
  br label %b_401876
f8_4018a0:  ; 0x4018a0
  store i32 %eh.handler.frame.3, ptr %st.p.14, align 1  ; 0x4018a0
  %sum.10 = add i32 %eh.handler.frame.3, 12  ; 0x4018a4
  call void @F_004018f0(i32 %sum.10)  ; 0x4018a9
  %eh.pending.7 = call i64 @ra_eh_pending()  ; 0x4018a9
  %eh.thrown.7 = icmp ne i64 %eh.pending.7, 0  ; 0x4018a9
  br i1 %eh.thrown.7, label %eh.landing, label %eh.ok.4018a9.3
eh.ok.4018a9.3:  ; 0x4018a9
  br label %eh.finally.40183f.1
eh.resume.40183f.0:  ; 0x4018b2
  call void @ra_eh_handled()  ; 0x4018b2
  br label %eh.probe.40183f
eh.search.40183f.0:  ; 0x4018b2
  call void @ra_eh_unhandled(i64 4200511)  ; 0x4018b2
  unreachable
eh.access.40183f:  ; 0x4018b2
  store i32 -1, ptr %st.p.5, align 1  ; 0x4018b2
  store i32 0, ptr %st.p.5, align 1  ; 0x401846
  %sp.60 = sub i32 %rsp.phi.2, 4  ; 0x40184f
  call void @F_004018f0(i32 %sp)  ; 0x40184f
  %rsp.119 = add i32 %sp.60, 4  ; 0x40184f
  %eh.pending.8 = call i64 @ra_eh_pending()  ; 0x40184f
  %eh.thrown.8 = icmp ne i64 %eh.pending.8, 0  ; 0x40184f
  br i1 %eh.thrown.8, label %eh.landing.1, label %eh.ok.40184f
eh.landing.1:  ; 0x40184f
  call void @ra_trap(i64 4200527, i64 8)  ; 0x40184f
  unreachable
eh.ok.40184f:  ; 0x40184f
  br label %eh.probe.401854
eh.probe.401854:  ; 0x401854
  %rsp.phi.3 = phi i32 [ %rsp.119, %eh.ok.40184f ], [ %sp.68, %eh.resume.401854.0 ]  ; 0x401854
  %rsi.phi.3 = phi i32 [ %rsi.phi.2, %eh.ok.40184f ], [ %pop.12, %eh.resume.401854.0 ]  ; 0x401854
  %addr.28 = add i32 %sp, -40  ; 0x401854
  %eh.at.3 = zext i32 %addr.28 to i64  ; 0x401854
  %eh.fault.3 = call i64 @ra_eh_probe(i64 %eh.at.3, i64 4, i64 0, i64 4200532)  ; 0x401854
  %eh.faults.3 = icmp ne i64 %eh.fault.3, 0  ; 0x401854
  br i1 %eh.faults.3, label %eh.seh.401854, label %eh.access.401854
eh.seh.401854:  ; 0x401854
  %eh.node.4 = add i32 %sp, -28  ; 0x401854
  %eh.handler.frame.4 = add i32 %eh.node.4, 16  ; 0x401854
  %eh.pointers.8 = call i64 @ra_eh_pointers()  ; 0x401854
  %eh.pointers.9 = trunc i64 %eh.pointers.8 to i32  ; 0x401854
  store i32 %eh.pointers.9, ptr %st.p.29, align 1  ; 0x401854
  %sp.61 = sub i32 %rsp.phi.3, 4  ; 0x401854
  store i32 0, ptr %st.p.13, align 1  ; 0x401854
  br label %f9_4018c0
eh.filtered.401854.0:  ; 0x401854
  %eax.22 = trunc i64 %ret.1 to i32  ; 0x4018ed
  %eh.execute.4 = icmp sgt i32 %eax.22, 0  ; 0x4018ed
  br i1 %eh.execute.4, label %eh.except.401854.0, label %eh.verdict.401854.0
f9_4018c0:  ; 0x4018c0
  %sp.62 = sub i32 %sp.61, 4  ; 0x4018c0
  store i32 %eh.handler.frame.4, ptr %st.p.14, align 1  ; 0x4018c0
  %sp.63 = sub i32 %sp.62, 4  ; 0x4018c3
  store i32 %rsi.phi.3, ptr %st.p.15, align 1  ; 0x4018c3
  %addr.29 = add i32 %eh.handler.frame.4, -20  ; 0x4018cc
  %addr.z.116 = zext i32 %addr.29 to i64  ; 0x4018cc
  %m.p.19 = inttoptr i64 %addr.z.116 to ptr  ; 0x4018cc
  %m.19 = load i32, ptr %m.p.19, align 1  ; 0x4018cc
  %edx.z.13 = zext i32 %m.19 to i64  ; 0x4018cc
  %m.p.20 = inttoptr i64 %edx.z.13 to ptr  ; 0x4018cf
  %m.20 = load i32, ptr %m.p.20, align 1  ; 0x4018cf
  %edx.z.14 = zext i32 %m.20 to i64  ; 0x4018cf
  %m.p.21 = inttoptr i64 %edx.z.14 to ptr  ; 0x4018d1
  %m.21 = load i32, ptr %m.p.21, align 1  ; 0x4018d1
  %diff.9 = sub i32 %eh.handler.frame.4, -36  ; 0x4018d8
  %addr.30 = add i32 -48, %diff.9  ; 0x4018da
  %addr.31 = add i32 %addr.30, -24  ; 0x4018da
  %addr.z.119 = zext i32 %addr.31 to i64  ; 0x4018da
  %st.p.73 = inttoptr i64 %addr.z.119 to ptr  ; 0x4018da
  store i32 %m.21, ptr %st.p.73, align 1  ; 0x4018da
  %sp.64 = sub i32 %sp.63, 4  ; 0x4018de
  store i32 -536866252, ptr %st.p.16, align 1  ; 0x4018de
  %sp.65 = sub i32 %sp.64, 4  ; 0x4018e3
  %sarg0.a.1 = add i32 %sp.65, 4
  %ret.1 = call i64 @F_004017c0(i32 -536866252)  ; 0x4018e3
  %eh.pending.9 = call i64 @ra_eh_pending()  ; 0x4018e3
  %eh.thrown.9 = icmp ne i64 %eh.pending.9, 0  ; 0x4018e3
  br i1 %eh.thrown.9, label %eh.landing, label %eh.ok.4018e3.4
eh.ok.4018e3.4:  ; 0x4018e3
  %sum.12 = add i32 %sarg0.a.1, 4  ; 0x4018e8
  %pop.12 = load i32, ptr %st.p.15, align 1  ; 0x4018eb
  %sp.66 = add i32 %sum.12, 4  ; 0x4018eb
  %sp.67 = add i32 %sp.66, 4  ; 0x4018ec
  %sp.68 = add i32 %sp.67, 4  ; 0x4018ed
  br label %eh.filtered.401854.0
eh.except.401854.0:  ; 0x4018ed
  call void @ra_eh_handled()  ; 0x4018ed
  store i32 -1, ptr %st.p.5, align 1  ; 0x4018ed
  br label %b_401876
eh.verdict.401854.0:  ; 0x4018ed
  %eh.continue.4 = icmp slt i32 %eax.22, 0  ; 0x4018ed
  br i1 %eh.continue.4, label %eh.resume.401854.0, label %eh.search.401854.0
eh.resume.401854.0:  ; 0x4018ed
  call void @ra_eh_handled()  ; 0x4018ed
  br label %eh.probe.401854
eh.search.401854.0:  ; 0x4018ed
  call void @ra_eh_unhandled(i64 4200532)  ; 0x4018ed
  unreachable
eh.access.401854:  ; 0x4018ed
  %m.22 = load i32, ptr %st.p.10, align 1  ; 0x4018ed
  br label %b_401857
eh.probe.401857:  ; 0x401857
  %rax.phi.1 = phi i32 [ %rax.phi, %b_401857 ], [ %n, %eh.resume.401857.0 ]  ; 0x401857
  %rsp.phi.5 = phi i32 [ %rsp.phi.4, %b_401857 ], [ %sp.76, %eh.resume.401857.0 ]  ; 0x401857
  %rsi.phi.6 = phi i32 [ %rsi.phi.5, %b_401857 ], [ %pop.14, %eh.resume.401857.0 ]  ; 0x401857
  %eh.at.4 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 320) to i32) to i64  ; 0x401857
  %eh.fault.4 = call i64 @ra_eh_probe(i64 %eh.at.4, i64 4, i64 0, i64 4200535)  ; 0x401857
  %eh.faults.4 = icmp ne i64 %eh.fault.4, 0  ; 0x401857
  br i1 %eh.faults.4, label %eh.seh.401857, label %eh.access.401857
eh.seh.401857:  ; 0x401857
  %eh.node.5 = add i32 %rbp.phi.5, -28  ; 0x401857
  %eh.handler.frame.5 = add i32 %eh.node.5, 16  ; 0x401857
  %eh.pointers.10 = call i64 @ra_eh_pointers()  ; 0x401857
  %eh.pointers.11 = trunc i64 %eh.pointers.10 to i32  ; 0x401857
  %st.p.77 = getelementptr inbounds i8, ptr %frame.top, i64 -36  ; 0x401857
  store i32 %eh.pointers.11, ptr %st.p.77, align 1  ; 0x401857
  %sp.69 = sub i32 %rsp.phi.5, 4  ; 0x401857
  %addr.z.128 = zext i32 %sp.69 to i64  ; 0x401857
  %st.p.78 = inttoptr i64 %addr.z.128 to ptr  ; 0x401857
  store i32 0, ptr %st.p.78, align 1  ; 0x401857
  br label %f10_4018c0
eh.filtered.401857.0:  ; 0x401857
  %eax.26 = trunc i64 %ret.6 to i32  ; 0x4018ed
  %eh.execute.5 = icmp sgt i32 %eax.26, 0  ; 0x4018ed
  br i1 %eh.execute.5, label %eh.except.401857.0, label %eh.verdict.401857.0
f10_4018c0:  ; 0x4018c0
  %sp.70 = sub i32 %sp.69, 4  ; 0x4018c0
  %addr.z.129 = zext i32 %sp.70 to i64  ; 0x4018c0
  %st.p.79 = inttoptr i64 %addr.z.129 to ptr  ; 0x4018c0
  store i32 %eh.handler.frame.5, ptr %st.p.79, align 1  ; 0x4018c0
  %sp.71 = sub i32 %sp.70, 4  ; 0x4018c3
  %addr.z.130 = zext i32 %sp.71 to i64  ; 0x4018c3
  %st.p.80 = inttoptr i64 %addr.z.130 to ptr  ; 0x4018c3
  store i32 %rsi.phi.6, ptr %st.p.80, align 1  ; 0x4018c3
  %addr.33 = add i32 %eh.handler.frame.5, -20  ; 0x4018cc
  %addr.z.132 = zext i32 %addr.33 to i64  ; 0x4018cc
  %m.p.24 = inttoptr i64 %addr.z.132 to ptr  ; 0x4018cc
  %m.24 = load i32, ptr %m.p.24, align 1  ; 0x4018cc
  %edx.z.16 = zext i32 %m.24 to i64  ; 0x4018cc
  %m.p.25 = inttoptr i64 %edx.z.16 to ptr  ; 0x4018cf
  %m.25 = load i32, ptr %m.p.25, align 1  ; 0x4018cf
  %edx.z.17 = zext i32 %m.25 to i64  ; 0x4018cf
  %m.p.26 = inttoptr i64 %edx.z.17 to ptr  ; 0x4018d1
  %m.26 = load i32, ptr %m.p.26, align 1  ; 0x4018d1
  %diff.10 = sub i32 %eh.handler.frame.5, -36  ; 0x4018d8
  %addr.34 = add i32 -48, %diff.10  ; 0x4018da
  %addr.35 = add i32 %addr.34, -24  ; 0x4018da
  %addr.z.135 = zext i32 %addr.35 to i64  ; 0x4018da
  %st.p.81 = inttoptr i64 %addr.z.135 to ptr  ; 0x4018da
  store i32 %m.26, ptr %st.p.81, align 1  ; 0x4018da
  %sp.72 = sub i32 %sp.71, 4  ; 0x4018de
  %addr.z.136 = zext i32 %sp.72 to i64  ; 0x4018de
  %st.p.82 = inttoptr i64 %addr.z.136 to ptr  ; 0x4018de
  store i32 -536866252, ptr %st.p.82, align 1  ; 0x4018de
  %sp.73 = sub i32 %sp.72, 4  ; 0x4018e3
  %sarg0.a.6 = add i32 %sp.73, 4
  %ret.6 = call i64 @F_004017c0(i32 -536866252)  ; 0x4018e3
  %eh.pending.10 = call i64 @ra_eh_pending()  ; 0x4018e3
  %eh.thrown.10 = icmp ne i64 %eh.pending.10, 0  ; 0x4018e3
  br i1 %eh.thrown.10, label %eh.landing, label %eh.ok.4018e3.5
eh.ok.4018e3.5:  ; 0x4018e3
  %sum.13 = add i32 %sarg0.a.6, 4  ; 0x4018e8
  %esp.z.89 = zext i32 %sum.13 to i64  ; 0x4018e8
  %pop.p.14 = inttoptr i64 %esp.z.89 to ptr  ; 0x4018eb
  %pop.14 = load i32, ptr %pop.p.14, align 1  ; 0x4018eb
  %sp.74 = add i32 %sum.13, 4  ; 0x4018eb
  %sp.75 = add i32 %sp.74, 4  ; 0x4018ec
  %sp.76 = add i32 %sp.75, 4  ; 0x4018ed
  br label %eh.filtered.401857.0
eh.except.401857.0:  ; 0x4018ed
  call void @ra_eh_handled()  ; 0x4018ed
  store i32 -1, ptr %st.p.5, align 1  ; 0x4018ed
  br label %b_401876
eh.verdict.401857.0:  ; 0x4018ed
  %eh.continue.5 = icmp slt i32 %eax.26, 0  ; 0x4018ed
  br i1 %eh.continue.5, label %eh.resume.401857.0, label %eh.search.401857.0
eh.resume.401857.0:  ; 0x4018ed
  call void @ra_eh_handled()  ; 0x4018ed
  %n = trunc i64 %ret.6 to i32
  br label %eh.probe.401857
eh.search.401857.0:  ; 0x4018ed
  call void @ra_eh_unhandled(i64 4200535)  ; 0x4018ed
  unreachable
eh.access.401857:  ; 0x4018ed
  %m.p.27 = inttoptr i64 %eh.at.4 to ptr  ; 0x4018ed
  %m.27 = load i32, ptr %m.p.27, align 1  ; 0x4018ed
  %logic = xor i32 %rax.phi.1, %m.27  ; 0x4018ed
  %eax.z.15 = zext i32 %logic to i64  ; 0x4018ed
  br label %eh.probe.40185d
eh.probe.40185d:  ; 0x40185d
  %rax.phi.2 = phi i64 [ %eax.z.15, %eh.access.401857 ], [ %ret.5, %eh.resume.40185d.0 ]  ; 0x40185d
  %rsp.phi.6 = phi i32 [ %rsp.phi.5, %eh.access.401857 ], [ %sp.84, %eh.resume.40185d.0 ]  ; 0x40185d
  %rsi.phi.7 = phi i32 [ %rsi.phi.6, %eh.access.401857 ], [ %pop.16, %eh.resume.40185d.0 ]  ; 0x40185d
  %addr.36 = add i32 %rbp.phi.5, -16  ; 0x40185d
  %eh.at.5 = zext i32 %addr.36 to i64  ; 0x40185d
  %eh.fault.5 = call i64 @ra_eh_probe(i64 %eh.at.5, i64 4, i64 1, i64 4200541)  ; 0x40185d
  %eh.faults.5 = icmp ne i64 %eh.fault.5, 0  ; 0x40185d
  br i1 %eh.faults.5, label %eh.seh.40185d, label %eh.access.40185d
eh.seh.40185d:  ; 0x40185d
  %eh.node.6 = add i32 %rbp.phi.5, -28  ; 0x40185d
  %eh.handler.frame.6 = add i32 %eh.node.6, 16  ; 0x40185d
  %eh.pointers.12 = call i64 @ra_eh_pointers()  ; 0x40185d
  %eh.pointers.13 = trunc i64 %eh.pointers.12 to i32  ; 0x40185d
  %st.p.85 = getelementptr inbounds i8, ptr %frame.top, i64 -36  ; 0x40185d
  store i32 %eh.pointers.13, ptr %st.p.85, align 1  ; 0x40185d
  %sp.77 = sub i32 %rsp.phi.6, 4  ; 0x40185d
  %addr.z.144 = zext i32 %sp.77 to i64  ; 0x40185d
  %st.p.86 = inttoptr i64 %addr.z.144 to ptr  ; 0x40185d
  store i32 0, ptr %st.p.86, align 1  ; 0x40185d
  br label %f11_4018c0
eh.filtered.40185d.0:  ; 0x40185d
  %eax.31 = trunc i64 %ret.5 to i32  ; 0x4018ed
  %eh.execute.6 = icmp sgt i32 %eax.31, 0  ; 0x4018ed
  br i1 %eh.execute.6, label %eh.except.40185d.0, label %eh.verdict.40185d.0
f11_4018c0:  ; 0x4018c0
  %sp.78 = sub i32 %sp.77, 4  ; 0x4018c0
  %addr.z.145 = zext i32 %sp.78 to i64  ; 0x4018c0
  %st.p.87 = inttoptr i64 %addr.z.145 to ptr  ; 0x4018c0
  store i32 %eh.handler.frame.6, ptr %st.p.87, align 1  ; 0x4018c0
  %sp.79 = sub i32 %sp.78, 4  ; 0x4018c3
  %addr.z.146 = zext i32 %sp.79 to i64  ; 0x4018c3
  %st.p.88 = inttoptr i64 %addr.z.146 to ptr  ; 0x4018c3
  store i32 %rsi.phi.7, ptr %st.p.88, align 1  ; 0x4018c3
  %addr.37 = add i32 %eh.handler.frame.6, -20  ; 0x4018cc
  %addr.z.148 = zext i32 %addr.37 to i64  ; 0x4018cc
  %m.p.29 = inttoptr i64 %addr.z.148 to ptr  ; 0x4018cc
  %m.29 = load i32, ptr %m.p.29, align 1  ; 0x4018cc
  %edx.z.19 = zext i32 %m.29 to i64  ; 0x4018cc
  %m.p.30 = inttoptr i64 %edx.z.19 to ptr  ; 0x4018cf
  %m.30 = load i32, ptr %m.p.30, align 1  ; 0x4018cf
  %edx.z.20 = zext i32 %m.30 to i64  ; 0x4018cf
  %m.p.31 = inttoptr i64 %edx.z.20 to ptr  ; 0x4018d1
  %m.31 = load i32, ptr %m.p.31, align 1  ; 0x4018d1
  %diff.11 = sub i32 %eh.handler.frame.6, -36  ; 0x4018d8
  %addr.38 = add i32 -48, %diff.11  ; 0x4018da
  %addr.39 = add i32 %addr.38, -24  ; 0x4018da
  %addr.z.151 = zext i32 %addr.39 to i64  ; 0x4018da
  %st.p.89 = inttoptr i64 %addr.z.151 to ptr  ; 0x4018da
  store i32 %m.31, ptr %st.p.89, align 1  ; 0x4018da
  %sp.80 = sub i32 %sp.79, 4  ; 0x4018de
  %addr.z.152 = zext i32 %sp.80 to i64  ; 0x4018de
  %st.p.90 = inttoptr i64 %addr.z.152 to ptr  ; 0x4018de
  store i32 -536866252, ptr %st.p.90, align 1  ; 0x4018de
  %sp.81 = sub i32 %sp.80, 4  ; 0x4018e3
  %sarg0.a.5 = add i32 %sp.81, 4
  %ret.5 = call i64 @F_004017c0(i32 -536866252)  ; 0x4018e3
  %eh.pending.11 = call i64 @ra_eh_pending()  ; 0x4018e3
  %eh.thrown.11 = icmp ne i64 %eh.pending.11, 0  ; 0x4018e3
  br i1 %eh.thrown.11, label %eh.landing, label %eh.ok.4018e3.6
eh.ok.4018e3.6:  ; 0x4018e3
  %sum.14 = add i32 %sarg0.a.5, 4  ; 0x4018e8
  %esp.z.98 = zext i32 %sum.14 to i64  ; 0x4018e8
  %pop.p.16 = inttoptr i64 %esp.z.98 to ptr  ; 0x4018eb
  %pop.16 = load i32, ptr %pop.p.16, align 1  ; 0x4018eb
  %sp.82 = add i32 %sum.14, 4  ; 0x4018eb
  %sp.83 = add i32 %sp.82, 4  ; 0x4018ec
  %sp.84 = add i32 %sp.83, 4  ; 0x4018ed
  br label %eh.filtered.40185d.0
eh.except.40185d.0:  ; 0x4018ed
  call void @ra_eh_handled()  ; 0x4018ed
  store i32 -1, ptr %st.p.5, align 1  ; 0x4018ed
  br label %b_401876
eh.verdict.40185d.0:  ; 0x4018ed
  %eh.continue.6 = icmp slt i32 %eax.31, 0  ; 0x4018ed
  br i1 %eh.continue.6, label %eh.resume.40185d.0, label %eh.search.40185d.0
eh.resume.40185d.0:  ; 0x4018ed
  call void @ra_eh_handled()  ; 0x4018ed
  br label %eh.probe.40185d
eh.search.40185d.0:  ; 0x4018ed
  call void @ra_eh_unhandled(i64 4200541)  ; 0x4018ed
  unreachable
eh.access.40185d:  ; 0x4018ed
  store i32 -1, ptr %st.p.5, align 1  ; 0x4018ed
  %m.32 = load i32, ptr %st.p.8, align 1  ; 0x401864
  %teb.4 = call i64 @ra_teb()  ; 0x401867
  %teb.5 = trunc i64 %teb.4 to i32  ; 0x401867
  %addr.z.160 = zext i32 %teb.5 to i64  ; 0x401867
  %st.p.94 = inttoptr i64 %addr.z.160 to ptr  ; 0x401867
  store i32 %m.32, ptr %st.p.94, align 1  ; 0x401867
  ret i64 %rax.phi.2
}

define i64 @F_004018c0() {  ; 0x4018c0
b_4018c0:  ; 0x4018c0
  %local_mc = alloca i32, align 4
  %local_m8 = alloca i32, align 4
  %local_m4 = alloca i32, align 4
  %ebp = trunc i64 undef to i32  ; 0x4018c0
  store i32 %ebp, ptr %local_m4, align 1  ; 0x4018c0
  store i32 %ebp, ptr %local_m8, align 1  ; 0x4018c3
  %addr = add i32 %ebp, -20  ; 0x4018cc
  %addr.z.3 = zext i32 %addr to i64  ; 0x4018cc
  %m.p.1 = inttoptr i64 %addr.z.3 to ptr  ; 0x4018cc
  %m.1 = load i32, ptr %m.p.1, align 1  ; 0x4018cc
  %edx.z = zext i32 %m.1 to i64  ; 0x4018cc
  %m.p.2 = inttoptr i64 %edx.z to ptr  ; 0x4018cf
  %m.2 = load i32, ptr %m.p.2, align 1  ; 0x4018cf
  %edx.z.1 = zext i32 %m.2 to i64  ; 0x4018cf
  %m.p.3 = inttoptr i64 %edx.z.1 to ptr  ; 0x4018d1
  %m.3 = load i32, ptr %m.p.3, align 1  ; 0x4018d1
  %diff = sub i32 %ebp, -36  ; 0x4018d8
  %addr.1 = add i32 -48, %diff  ; 0x4018da
  %addr.2 = add i32 %addr.1, -24  ; 0x4018da
  %addr.z.6 = zext i32 %addr.2 to i64  ; 0x4018da
  %st.p.2 = inttoptr i64 %addr.z.6 to ptr  ; 0x4018da
  store i32 %m.3, ptr %st.p.2, align 1  ; 0x4018da
  store i32 -536866252, ptr %local_mc, align 1  ; 0x4018de
  %ret = call i64 @F_004017c0(i32 -536866252)  ; 0x4018e3
  %eh.pending = call i64 @ra_eh_pending()  ; 0x4018e3
  %eh.thrown = icmp ne i64 %eh.pending, 0  ; 0x4018e3
  br i1 %eh.thrown, label %eh.landing, label %eh.ok.4018e3
eh.landing:  ; 0x4018e3
  ret i64 %ret
eh.ok.4018e3:  ; 0x4018e3
  ret i64 %ret
}

define void @F_004018f0(i32 %arg0) {  ; 0x4018f0
b_4018f0:  ; 0x4018f0
  %addr = add i32 %arg0, -40  ; 0x4018f0
  %addr.z = zext i32 %addr to i64  ; 0x4018f0
  %m.p = inttoptr i64 %addr.z to ptr  ; 0x4018f0
  %m = load i32, ptr %m.p, align 1  ; 0x4018f0
  %sum = add i32 %m, 100  ; 0x4018f0
  store i32 %sum, ptr %m.p, align 1  ; 0x4018f0
  ret void
}

define i64 @F_00401900(i32 %arg0, i32 %arg1) {  ; 0x401900
b_401900:  ; 0x401900
  %frame = alloca [128 x i8], align 16
  %frame.top = getelementptr inbounds i8, ptr %frame, i64 44
  %rsp0 = ptrtoint ptr %frame.top to i64
  %stack.arg0.p = getelementptr inbounds i8, ptr %frame.top, i64 4
  store i32 %arg0, ptr %stack.arg0.p, align 4
  %stack.arg1.p = getelementptr inbounds i8, ptr %frame.top, i64 8
  store i32 %arg1, ptr %stack.arg1.p, align 4
  %esp = trunc i64 %rsp0 to i32  ; 0x401900
  %diff = sub i32 %esp, 8  ; 0x401900
  %eax.z = trunc i32 %arg1 to i8  ; 0x401903
  %addr.z.1 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 192) to i32) to i64  ; 0x401907
  %m.p.1 = inttoptr i64 %addr.z.1 to ptr  ; 0x401907
  %m.1 = load i32, ptr %m.p.1, align 1  ; 0x401907
  %logic = xor i32 %m.1, %diff  ; 0x40190d
  %st.p = getelementptr inbounds i8, ptr %frame.top, i64 -4  ; 0x40190f
  store i32 %logic, ptr %st.p, align 1  ; 0x40190f
  %ecx.z.2 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 328) to i32) to i64  ; 0x401913
  %logic.1 = and i8 %eax.z, 1  ; 0x401918
  %not = icmp ne i8 %logic.1, 0  ; 0x40191a
  br i1 %not, label %b_401920, label %b_40191c
b_40191c:  ; 0x40191c
  %m.2 = load i32, ptr %stack.arg0.p, align 1  ; 0x40191c
  br label %b_401920
b_401920:  ; 0x401920
  %rcx.phi = phi i32 [ ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 328) to i32), %b_401900 ], [ %m.2, %b_40191c ]  ; 0x401920
  %st.p.1 = getelementptr inbounds i8, ptr %frame.top, i64 -8  ; 0x401920
  store i32 %rcx.phi, ptr %st.p.1, align 1  ; 0x401920
  %addr.z.5 = zext i32 %rcx.phi to i64  ; 0x401923
  %m.p.3 = inttoptr i64 %addr.z.5 to ptr  ; 0x401923
  %m.3 = load i32, ptr %m.p.3, align 1  ; 0x401923
  %sum = add i32 %m.3, %arg1  ; 0x401923
  store i32 %sum, ptr %m.p.3, align 1  ; 0x401923
  %m.4 = load i32, ptr %st.p.1, align 1  ; 0x401925
  %eax.z.1 = zext i32 %m.4 to i64  ; 0x401925
  %m.p.5 = inttoptr i64 %eax.z.1 to ptr  ; 0x40192c
  %m.5 = load i32, ptr %m.p.5, align 1  ; 0x40192c
  %addr.3 = mul i32 %m.5, 3  ; 0x40192e
  %addr.z.9 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 324) to i32) to i64  ; 0x401931
  %m.p.6 = inttoptr i64 %addr.z.9 to ptr  ; 0x401931
  %m.6 = load i32, ptr %m.p.6, align 1  ; 0x401931
  %sum.1 = add i32 %addr.3, %m.6  ; 0x401931
  store i32 %sum.1, ptr %m.p.6, align 1  ; 0x401937
  %product = mul i32 %sum.1, 100  ; 0x40193c
  %m.p.7 = inttoptr i64 %ecx.z.2 to ptr  ; 0x40193f
  %m.7 = load i32, ptr %m.p.7, align 1  ; 0x40193f
  %sum.2 = add i32 %product, %m.7  ; 0x40193f
  %eax.z.6 = zext i32 %sum.2 to i64  ; 0x40193f
  %m.8 = load i32, ptr %st.p, align 1  ; 0x401945
  %logic.2 = xor i32 %m.8, %diff  ; 0x401949
  %m.9 = load i32, ptr %m.p.1, align 1  ; 0x40194b
  %not.1 = icmp ne i32 %m.9, %logic.2  ; 0x401953
  br i1 %not.1, label %b_401959, label %b_401955
b_401955:  ; 0x401955
  ret i64 %eax.z.6
b_401959:  ; 0x401959
  %m.10 = load i32, ptr %st.p, align 1  ; 0x401959
  %logic.3 = xor i32 %m.10, %diff  ; 0x40195d
  %ecx.z.7 = zext i32 %logic.3 to i64  ; 0x40195d
  call void @F_00402570(i64 %ecx.z.7)  ; 0x40195f
  %eh.pending = call i64 @ra_eh_pending()  ; 0x40195f
  %eh.thrown = icmp ne i64 %eh.pending, 0  ; 0x40195f
  br i1 %eh.thrown, label %eh.landing, label %eh.ok.40195f
eh.landing:  ; 0x40195f
  ret i64 %eax.z.6
eh.ok.40195f:  ; 0x40195f
  call void @ra_trap(i64 4200804, i64 1)  ; 0x401964
  call void @ra_trap(i64 4200804, i64 1)  ; 0x401964
  unreachable
}

define i64 @F_00401970() {  ; 0x401970
b_401970:  ; 0x401970
  %local_mc = alloca i32, align 4
  %local_m8 = alloca i32, align 4
  %local_m4 = alloca i32, align 4
  %esi = trunc i64 undef to i32  ; 0x401970
  store i32 %esi, ptr %local_m4, align 1  ; 0x401970
  %ret = call i64 @F_00401c30()  ; 0x401971
  %eh.pending = call i64 @ra_eh_pending()  ; 0x401971
  %eh.thrown = icmp ne i64 %eh.pending, 0  ; 0x401971
  br i1 %eh.thrown, label %eh.landing, label %eh.ok.401971
eh.landing:  ; 0x401971
  %rax.phi = phi i64 [ %ret, %b_401970 ], [ %ret.1, %eh.ok.401971 ], [ %ret.2, %eh.ok.40197c ], [ %ret.3, %eh.ok.40198a ], [ %ret.4, %eh.ok.40199d ], [ %ret.5, %eh.ok.4019a9 ], [ %ret.6, %eh.ok.4019b7 ], [ %ret.7, %eh.ok.4019ca ], [ %ret.8, %eh.ok.4019d6 ], [ %ret.9, %eh.ok.4019e4 ], [ %ret.10, %eh.ok.4019f7 ], [ %ret.11, %eh.ok.401a03 ], [ %ret.12, %eh.ok.401a11 ], [ %ret.13, %eh.ok.401a24 ], [ %ret.14, %eh.ok.401a30 ], [ %ret.15, %eh.ok.401a3e ], [ %ret.16, %eh.ok.401a51 ], [ %ret.17, %eh.ok.401a5a ], [ %ret.18, %eh.ok.401a68 ], [ %ret.19, %eh.ok.401a7b ], [ %ret.20, %eh.ok.401a84 ], [ %ret.21, %eh.ok.401a92 ], [ %ret.22, %eh.ok.401aa5 ], [ %ret.23, %eh.ok.401aae ], [ %ret.24, %eh.ok.401abc ], [ %ret.25, %eh.ok.401acf ], [ %ret.26, %eh.ok.401ad8 ], [ %ret.27, %eh.ok.401ae6 ], [ %ret.28, %eh.ok.401af4 ]  ; 0x401971
  ret i64 %rax.phi
eh.ok.401971:  ; 0x401971
  %eax = trunc i64 %ret to i32  ; 0x401976
  %sum = add i32 %eax, -6  ; 0x401978
  store i32 %sum, ptr %local_m8, align 1  ; 0x40197b
  %ret.1 = call i64 @F_00401260(i32 %sum)  ; 0x40197c
  %eh.pending.1 = call i64 @ra_eh_pending()  ; 0x40197c
  %eh.thrown.1 = icmp ne i64 %eh.pending.1, 0  ; 0x40197c
  br i1 %eh.thrown.1, label %eh.landing, label %eh.ok.40197c
eh.ok.40197c:  ; 0x40197c
  %eax.3 = trunc i64 %ret.1 to i32  ; 0x401984
  store i32 %eax.3, ptr %local_m8, align 1  ; 0x401984
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 655) to i32), ptr %local_mc, align 1  ; 0x401985
  %ret.2 = call i64 @F_00401c40(i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 655) to i32), i32 %eax.3)  ; 0x40198a
  %eh.pending.2 = call i64 @ra_eh_pending()  ; 0x40198a
  %eh.thrown.2 = icmp ne i64 %eh.pending.2, 0  ; 0x40198a
  br i1 %eh.thrown.2, label %eh.landing, label %eh.ok.40198a
eh.ok.40198a:  ; 0x40198a
  %addr.z.7 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 316) to i32) to i64  ; 0x401992
  %m.p = inttoptr i64 %addr.z.7 to ptr  ; 0x401992
  %m = load i32, ptr %m.p, align 1  ; 0x401992
  store i32 %m, ptr %local_m8, align 1  ; 0x401992
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 671) to i32), ptr %local_mc, align 1  ; 0x401998
  %ret.3 = call i64 @F_00401c40(i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 671) to i32), i32 %m)  ; 0x40199d
  %eh.pending.3 = call i64 @ra_eh_pending()  ; 0x40199d
  %eh.thrown.3 = icmp ne i64 %eh.pending.3, 0  ; 0x40199d
  br i1 %eh.thrown.3, label %eh.landing, label %eh.ok.40199d
eh.ok.40199d:  ; 0x40199d
  %addr = add i32 %eax, -5  ; 0x4019a5
  store i32 %addr, ptr %local_m8, align 1  ; 0x4019a8
  %ret.4 = call i64 @F_00401260(i32 %addr)  ; 0x4019a9
  %eh.pending.4 = call i64 @ra_eh_pending()  ; 0x4019a9
  %eh.thrown.4 = icmp ne i64 %eh.pending.4, 0  ; 0x4019a9
  br i1 %eh.thrown.4, label %eh.landing, label %eh.ok.4019a9
eh.ok.4019a9:  ; 0x4019a9
  %eax.5 = trunc i64 %ret.4 to i32  ; 0x4019b1
  store i32 %eax.5, ptr %local_m8, align 1  ; 0x4019b1
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 744) to i32), ptr %local_mc, align 1  ; 0x4019b2
  %ret.5 = call i64 @F_00401c40(i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 744) to i32), i32 %eax.5)  ; 0x4019b7
  %eh.pending.5 = call i64 @ra_eh_pending()  ; 0x4019b7
  %eh.thrown.5 = icmp ne i64 %eh.pending.5, 0  ; 0x4019b7
  br i1 %eh.thrown.5, label %eh.landing, label %eh.ok.4019b7
eh.ok.4019b7:  ; 0x4019b7
  %m.1 = load i32, ptr %m.p, align 1  ; 0x4019bf
  store i32 %m.1, ptr %local_m8, align 1  ; 0x4019bf
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 671) to i32), ptr %local_mc, align 1  ; 0x4019c5
  %ret.6 = call i64 @F_00401c40(i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 671) to i32), i32 %m.1)  ; 0x4019ca
  %eh.pending.6 = call i64 @ra_eh_pending()  ; 0x4019ca
  %eh.thrown.6 = icmp ne i64 %eh.pending.6, 0  ; 0x4019ca
  br i1 %eh.thrown.6, label %eh.landing, label %eh.ok.4019ca
eh.ok.4019ca:  ; 0x4019ca
  %addr.1 = add i32 %eax, -4  ; 0x4019d2
  store i32 %addr.1, ptr %local_m8, align 1  ; 0x4019d5
  %ret.7 = call i64 @F_00401260(i32 %addr.1)  ; 0x4019d6
  %eh.pending.7 = call i64 @ra_eh_pending()  ; 0x4019d6
  %eh.thrown.7 = icmp ne i64 %eh.pending.7, 0  ; 0x4019d6
  br i1 %eh.thrown.7, label %eh.landing, label %eh.ok.4019d6
eh.ok.4019d6:  ; 0x4019d6
  %eax.7 = trunc i64 %ret.7 to i32  ; 0x4019de
  store i32 %eax.7, ptr %local_m8, align 1  ; 0x4019de
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 760) to i32), ptr %local_mc, align 1  ; 0x4019df
  %ret.8 = call i64 @F_00401c40(i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 760) to i32), i32 %eax.7)  ; 0x4019e4
  %eh.pending.8 = call i64 @ra_eh_pending()  ; 0x4019e4
  %eh.thrown.8 = icmp ne i64 %eh.pending.8, 0  ; 0x4019e4
  br i1 %eh.thrown.8, label %eh.landing, label %eh.ok.4019e4
eh.ok.4019e4:  ; 0x4019e4
  %m.2 = load i32, ptr %m.p, align 1  ; 0x4019ec
  store i32 %m.2, ptr %local_m8, align 1  ; 0x4019ec
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 671) to i32), ptr %local_mc, align 1  ; 0x4019f2
  %ret.9 = call i64 @F_00401c40(i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 671) to i32), i32 %m.2)  ; 0x4019f7
  %eh.pending.9 = call i64 @ra_eh_pending()  ; 0x4019f7
  %eh.thrown.9 = icmp ne i64 %eh.pending.9, 0  ; 0x4019f7
  br i1 %eh.thrown.9, label %eh.landing, label %eh.ok.4019f7
eh.ok.4019f7:  ; 0x4019f7
  %addr.2 = add i32 %eax, -3  ; 0x4019ff
  store i32 %addr.2, ptr %local_m8, align 1  ; 0x401a02
  %ret.10 = call i64 @F_00401260(i32 %addr.2)  ; 0x401a03
  %eh.pending.10 = call i64 @ra_eh_pending()  ; 0x401a03
  %eh.thrown.10 = icmp ne i64 %eh.pending.10, 0  ; 0x401a03
  br i1 %eh.thrown.10, label %eh.landing, label %eh.ok.401a03
eh.ok.401a03:  ; 0x401a03
  %eax.9 = trunc i64 %ret.10 to i32  ; 0x401a0b
  store i32 %eax.9, ptr %local_m8, align 1  ; 0x401a0b
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 648) to i32), ptr %local_mc, align 1  ; 0x401a0c
  %ret.11 = call i64 @F_00401c40(i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 648) to i32), i32 %eax.9)  ; 0x401a11
  %eh.pending.11 = call i64 @ra_eh_pending()  ; 0x401a11
  %eh.thrown.11 = icmp ne i64 %eh.pending.11, 0  ; 0x401a11
  br i1 %eh.thrown.11, label %eh.landing, label %eh.ok.401a11
eh.ok.401a11:  ; 0x401a11
  %m.3 = load i32, ptr %m.p, align 1  ; 0x401a19
  store i32 %m.3, ptr %local_m8, align 1  ; 0x401a19
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 671) to i32), ptr %local_mc, align 1  ; 0x401a1f
  %ret.12 = call i64 @F_00401c40(i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 671) to i32), i32 %m.3)  ; 0x401a24
  %eh.pending.12 = call i64 @ra_eh_pending()  ; 0x401a24
  %eh.thrown.12 = icmp ne i64 %eh.pending.12, 0  ; 0x401a24
  br i1 %eh.thrown.12, label %eh.landing, label %eh.ok.401a24
eh.ok.401a24:  ; 0x401a24
  %addr.3 = add i32 %eax, -2  ; 0x401a2c
  store i32 %addr.3, ptr %local_m8, align 1  ; 0x401a2f
  %ret.13 = call i64 @F_00401260(i32 %addr.3)  ; 0x401a30
  %eh.pending.13 = call i64 @ra_eh_pending()  ; 0x401a30
  %eh.thrown.13 = icmp ne i64 %eh.pending.13, 0  ; 0x401a30
  br i1 %eh.thrown.13, label %eh.landing, label %eh.ok.401a30
eh.ok.401a30:  ; 0x401a30
  %eax.11 = trunc i64 %ret.13 to i32  ; 0x401a38
  store i32 %eax.11, ptr %local_m8, align 1  ; 0x401a38
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 752) to i32), ptr %local_mc, align 1  ; 0x401a39
  %ret.14 = call i64 @F_00401c40(i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 752) to i32), i32 %eax.11)  ; 0x401a3e
  %eh.pending.14 = call i64 @ra_eh_pending()  ; 0x401a3e
  %eh.thrown.14 = icmp ne i64 %eh.pending.14, 0  ; 0x401a3e
  br i1 %eh.thrown.14, label %eh.landing, label %eh.ok.401a3e
eh.ok.401a3e:  ; 0x401a3e
  %m.4 = load i32, ptr %m.p, align 1  ; 0x401a46
  store i32 %m.4, ptr %local_m8, align 1  ; 0x401a46
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 671) to i32), ptr %local_mc, align 1  ; 0x401a4c
  %ret.15 = call i64 @F_00401c40(i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 671) to i32), i32 %m.4)  ; 0x401a51
  %eh.pending.15 = call i64 @ra_eh_pending()  ; 0x401a51
  %eh.thrown.15 = icmp ne i64 %eh.pending.15, 0  ; 0x401a51
  br i1 %eh.thrown.15, label %eh.landing, label %eh.ok.401a51
eh.ok.401a51:  ; 0x401a51
  store i32 %eax, ptr %local_m8, align 1  ; 0x401a59
  %ret.16 = call i64 @F_00401470(i32 %eax)  ; 0x401a5a
  %eh.pending.16 = call i64 @ra_eh_pending()  ; 0x401a5a
  %eh.thrown.16 = icmp ne i64 %eh.pending.16, 0  ; 0x401a5a
  br i1 %eh.thrown.16, label %eh.landing, label %eh.ok.401a5a
eh.ok.401a5a:  ; 0x401a5a
  %eax.12 = trunc i64 %ret.16 to i32  ; 0x401a62
  store i32 %eax.12, ptr %local_m8, align 1  ; 0x401a62
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 662) to i32), ptr %local_mc, align 1  ; 0x401a63
  %ret.17 = call i64 @F_00401c40(i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 662) to i32), i32 %eax.12)  ; 0x401a68
  %eh.pending.17 = call i64 @ra_eh_pending()  ; 0x401a68
  %eh.thrown.17 = icmp ne i64 %eh.pending.17, 0  ; 0x401a68
  br i1 %eh.thrown.17, label %eh.landing, label %eh.ok.401a68
eh.ok.401a68:  ; 0x401a68
  %m.5 = load i32, ptr %m.p, align 1  ; 0x401a70
  store i32 %m.5, ptr %local_m8, align 1  ; 0x401a70
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 671) to i32), ptr %local_mc, align 1  ; 0x401a76
  %ret.18 = call i64 @F_00401c40(i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 671) to i32), i32 %m.5)  ; 0x401a7b
  %eh.pending.18 = call i64 @ra_eh_pending()  ; 0x401a7b
  %eh.thrown.18 = icmp ne i64 %eh.pending.18, 0  ; 0x401a7b
  br i1 %eh.thrown.18, label %eh.landing, label %eh.ok.401a7b
eh.ok.401a7b:  ; 0x401a7b
  store i32 %eax, ptr %local_m8, align 1  ; 0x401a83
  %ret.19 = call i64 @F_00401540(i32 %eax)  ; 0x401a84
  %eh.pending.19 = call i64 @ra_eh_pending()  ; 0x401a84
  %eh.thrown.19 = icmp ne i64 %eh.pending.19, 0  ; 0x401a84
  br i1 %eh.thrown.19, label %eh.landing, label %eh.ok.401a84
eh.ok.401a84:  ; 0x401a84
  %eax.13 = trunc i64 %ret.19 to i32  ; 0x401a8c
  store i32 %eax.13, ptr %local_m8, align 1  ; 0x401a8c
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 767) to i32), ptr %local_mc, align 1  ; 0x401a8d
  %ret.20 = call i64 @F_00401c40(i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 767) to i32), i32 %eax.13)  ; 0x401a92
  %eh.pending.20 = call i64 @ra_eh_pending()  ; 0x401a92
  %eh.thrown.20 = icmp ne i64 %eh.pending.20, 0  ; 0x401a92
  br i1 %eh.thrown.20, label %eh.landing, label %eh.ok.401a92
eh.ok.401a92:  ; 0x401a92
  %m.6 = load i32, ptr %m.p, align 1  ; 0x401a9a
  store i32 %m.6, ptr %local_m8, align 1  ; 0x401a9a
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 671) to i32), ptr %local_mc, align 1  ; 0x401aa0
  %ret.21 = call i64 @F_00401c40(i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 671) to i32), i32 %m.6)  ; 0x401aa5
  %eh.pending.21 = call i64 @ra_eh_pending()  ; 0x401aa5
  %eh.thrown.21 = icmp ne i64 %eh.pending.21, 0  ; 0x401aa5
  br i1 %eh.thrown.21, label %eh.landing, label %eh.ok.401aa5
eh.ok.401aa5:  ; 0x401aa5
  store i32 %eax, ptr %local_m8, align 1  ; 0x401aad
  %ret.22 = call i64 @F_004016b0(i32 %eax)  ; 0x401aae
  %eh.pending.22 = call i64 @ra_eh_pending()  ; 0x401aae
  %eh.thrown.22 = icmp ne i64 %eh.pending.22, 0  ; 0x401aae
  br i1 %eh.thrown.22, label %eh.landing, label %eh.ok.401aae
eh.ok.401aae:  ; 0x401aae
  %eax.14 = trunc i64 %ret.22 to i32  ; 0x401ab6
  store i32 %eax.14, ptr %local_m8, align 1  ; 0x401ab6
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 678) to i32), ptr %local_mc, align 1  ; 0x401ab7
  %ret.23 = call i64 @F_00401c40(i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 678) to i32), i32 %eax.14)  ; 0x401abc
  %eh.pending.23 = call i64 @ra_eh_pending()  ; 0x401abc
  %eh.thrown.23 = icmp ne i64 %eh.pending.23, 0  ; 0x401abc
  br i1 %eh.thrown.23, label %eh.landing, label %eh.ok.401abc
eh.ok.401abc:  ; 0x401abc
  %m.7 = load i32, ptr %m.p, align 1  ; 0x401ac4
  store i32 %m.7, ptr %local_m8, align 1  ; 0x401ac4
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 671) to i32), ptr %local_mc, align 1  ; 0x401aca
  %ret.24 = call i64 @F_00401c40(i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 671) to i32), i32 %m.7)  ; 0x401acf
  %eh.pending.24 = call i64 @ra_eh_pending()  ; 0x401acf
  %eh.thrown.24 = icmp ne i64 %eh.pending.24, 0  ; 0x401acf
  br i1 %eh.thrown.24, label %eh.landing, label %eh.ok.401acf
eh.ok.401acf:  ; 0x401acf
  store i32 %eax, ptr %local_m8, align 1  ; 0x401ad7
  %ret.25 = call i64 @F_004017e0(i32 %eax)  ; 0x401ad8
  %eh.pending.25 = call i64 @ra_eh_pending()  ; 0x401ad8
  %eh.thrown.25 = icmp ne i64 %eh.pending.25, 0  ; 0x401ad8
  br i1 %eh.thrown.25, label %eh.landing, label %eh.ok.401ad8
eh.ok.401ad8:  ; 0x401ad8
  %eax.15 = trunc i64 %ret.25 to i32  ; 0x401ae0
  store i32 %eax.15, ptr %local_m8, align 1  ; 0x401ae0
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 723) to i32), ptr %local_mc, align 1  ; 0x401ae1
  %ret.26 = call i64 @F_00401c40(i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 723) to i32), i32 %eax.15)  ; 0x401ae6
  %eh.pending.26 = call i64 @ra_eh_pending()  ; 0x401ae6
  %eh.thrown.26 = icmp ne i64 %eh.pending.26, 0  ; 0x401ae6
  br i1 %eh.thrown.26, label %eh.landing, label %eh.ok.401ae6
eh.ok.401ae6:  ; 0x401ae6
  store i32 %eax, ptr %local_m8, align 1  ; 0x401aee
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 324) to i32), ptr %local_mc, align 1  ; 0x401aef
  %ret.27 = call i64 @F_00401900(i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 324) to i32), i32 %eax)  ; 0x401af4
  %eh.pending.27 = call i64 @ra_eh_pending()  ; 0x401af4
  %eh.thrown.27 = icmp ne i64 %eh.pending.27, 0  ; 0x401af4
  br i1 %eh.thrown.27, label %eh.landing, label %eh.ok.401af4
eh.ok.401af4:  ; 0x401af4
  %eax.16 = trunc i64 %ret.27 to i32  ; 0x401afc
  store i32 %eax.16, ptr %local_m8, align 1  ; 0x401afc
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 709) to i32), ptr %local_mc, align 1  ; 0x401afd
  %ret.28 = call i64 @F_00401c40(i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 709) to i32), i32 %eax.16)  ; 0x401b02
  %eh.pending.28 = call i64 @ra_eh_pending()  ; 0x401b02
  %eh.thrown.28 = icmp ne i64 %eh.pending.28, 0  ; 0x401b02
  br i1 %eh.thrown.28, label %eh.landing, label %eh.ok.401b02
eh.ok.401b02:  ; 0x401b02
  ret i64 %ret.28
}

define i64 @Derived_vf1() {  ; 0x401b10
b_401b10:  ; 0x401b10
  ret i64 2
}

define i64 @Base_vf1() {  ; 0x401b20
b_401b20:  ; 0x401b20
  ret i64 1
}

define i64 @Base_vf0(i64 %arg0, i64 %arg1) {  ; 0x401b30
b_401b30:  ; 0x401b30
  %frame = alloca [124 x i8], align 16
  %frame.top = getelementptr inbounds i8, ptr %frame, i64 44
  %local_mc = alloca i32, align 4
  %local_m8 = alloca i32, align 4
  %local_m4 = alloca i32, align 4
  %stack.arg0.p = getelementptr inbounds i8, ptr %frame.top, i64 4
  %stack.arg0.lo = trunc i64 %arg1 to i32
  store i32 %stack.arg0.lo, ptr %stack.arg0.p, align 4
  %esi = trunc i64 undef to i32  ; 0x401b30
  store i32 %esi, ptr %local_m4, align 1  ; 0x401b30
  %ecx = trunc i64 %arg0 to i32  ; 0x401b31
  %m = load i8, ptr %stack.arg0.p, align 1  ; 0x401b33
  %logic = and i8 %m, 1  ; 0x401b33
  %zf = icmp eq i8 %logic, 0  ; 0x401b33
  br i1 %zf, label %b_401b45, label %b_401b3a
b_401b3a:  ; 0x401b3a
  store i32 8, ptr %local_m8, align 1  ; 0x401b3a
  store i32 %ecx, ptr %local_mc, align 1  ; 0x401b3c
  %ret = call i64 @F_00402530(i32 %ecx)  ; 0x401b3d
  %eh.pending = call i64 @ra_eh_pending()  ; 0x401b3d
  %eh.thrown = icmp ne i64 %eh.pending, 0  ; 0x401b3d
  br i1 %eh.thrown, label %eh.landing, label %eh.ok.401b3d
b_401b45:  ; 0x401b45
  %eax.z = zext i32 %ecx to i64  ; 0x401b45
  ret i64 %eax.z
eh.landing:  ; 0x401b3d
  ret i64 %ret
eh.ok.401b3d:  ; 0x401b3d
  br label %b_401b45
}

define i64 @Problem_vf1() {  ; 0x401b50
b_401b50:  ; 0x401b50
  %eax.z = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 701) to i32) to i64  ; 0x401b50
  ret i64 %eax.z
}

define i64 @std_exception_vf1(i64 %arg0) {  ; 0x401b60
b_401b60:  ; 0x401b60
  %ecx = trunc i64 %arg0 to i32  ; 0x401b60
  %addr = add i32 %ecx, 4  ; 0x401b60
  %addr.z = zext i32 %addr to i64  ; 0x401b60
  %m.p = inttoptr i64 %addr.z to ptr  ; 0x401b60
  %m = load i32, ptr %m.p, align 1  ; 0x401b60
  %not = icmp ne i32 %m, 0  ; 0x401b6a
  %cmov = select i1 %not, i32 %m, i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 683) to i32)  ; 0x401b6a
  %eax.z.1 = zext i32 %cmov to i64  ; 0x401b6a
  ret i64 %eax.z.1
}

define i64 @std_exception_vf0(i64 %arg0, i64 %arg1, i64 %arg2, i64 %arg3, i64 %arg4, i64 %arg5) {  ; 0x401b70
b_401b70:  ; 0x401b70
  %frame = alloca [140 x i8], align 16
  %frame.top = getelementptr inbounds i8, ptr %frame, i64 44
  %local_mc = alloca i32, align 4
  %local_m8 = alloca i32, align 4
  %local_m4 = alloca i32, align 4
  %local_p0 = alloca i32, align 4
  %local_p8 = alloca i32, align 4
  %local_pc = alloca i32, align 4
  %local_p10 = alloca i32, align 4
  %local_p14 = alloca i32, align 4
  %stack.arg0.p = getelementptr inbounds i8, ptr %frame.top, i64 4
  %stack.arg0.lo = trunc i64 %arg1 to i32
  store i32 %stack.arg0.lo, ptr %stack.arg0.p, align 4
  %stack.arg1.lo = trunc i64 %arg2 to i32
  store i32 %stack.arg1.lo, ptr %local_p8, align 4
  %stack.arg2.lo = trunc i64 %arg3 to i32
  store i32 %stack.arg2.lo, ptr %local_pc, align 4
  %stack.arg3.lo = trunc i64 %arg4 to i32
  store i32 %stack.arg3.lo, ptr %local_p10, align 4
  %stack.arg4.lo = trunc i64 %arg5 to i32
  store i32 %stack.arg4.lo, ptr %local_p14, align 4
  %esi = trunc i64 undef to i32  ; 0x401b70
  store i32 %esi, ptr %local_m4, align 1  ; 0x401b70
  %ecx = trunc i64 %arg0 to i32  ; 0x401b71
  %esi.z = zext i32 %ecx to i64  ; 0x401b71
  %st.p.1 = inttoptr i64 %esi.z to ptr  ; 0x401b73
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 328) to i32), ptr %st.p.1, align 1  ; 0x401b73
  %addr = add i32 %ecx, 4  ; 0x401b79
  store i32 %addr, ptr %local_m8, align 1  ; 0x401b7c
  %sarg0 = zext i32 %addr to i64
  %sarg1.d = load i32, ptr %local_m4, align 4
  %sarg1 = zext i32 %sarg1.d to i64
  %sarg2.d = load i32, ptr %local_p0, align 4
  %sarg2 = zext i32 %sarg2.d to i64
  %sarg3.d = load i32, ptr %stack.arg0.p, align 4
  %sarg3 = zext i32 %sarg3.d to i64
  %sarg4.d = load i32, ptr %local_p8, align 4
  %sarg4 = zext i32 %sarg4.d to i64
  %sarg5.d = load i32, ptr %local_pc, align 4
  %sarg5 = zext i32 %sarg5.d to i64
  %sarg6.d = load i32, ptr %local_p10, align 4
  %sarg6 = zext i32 %sarg6.d to i64
  %sarg7.d = load i32, ptr %local_p14, align 4
  %sarg7 = zext i32 %sarg7.d to i64
  %ret = call i64 @thunk___std_exception_destroy(i64 %arg0, i64 %sarg0, i64 %sarg1, i64 %sarg2, i64 %sarg3, i64 %sarg4, i64 %sarg5, i64 %sarg6, i64 %sarg7)  ; 0x401b7d
  %eh.pending = call i64 @ra_eh_pending()  ; 0x401b7d
  %eh.thrown = icmp ne i64 %eh.pending, 0  ; 0x401b7d
  br i1 %eh.thrown, label %eh.landing, label %eh.ok.401b7d
b_401b8c:  ; 0x401b8c
  store i32 12, ptr %local_m8, align 1  ; 0x401b8c
  store i32 %ecx, ptr %local_mc, align 1  ; 0x401b8e
  %ret.1 = call i64 @F_00402530(i32 %ecx)  ; 0x401b8f
  %eh.pending.1 = call i64 @ra_eh_pending()  ; 0x401b8f
  %eh.thrown.1 = icmp ne i64 %eh.pending.1, 0  ; 0x401b8f
  br i1 %eh.thrown.1, label %eh.landing, label %eh.ok.401b8f
b_401b97:  ; 0x401b97
  ret i64 %esi.z
eh.landing:  ; 0x401b7d
  %rax.phi = phi i64 [ %ret, %b_401b70 ], [ %ret.1, %b_401b8c ]  ; 0x401b7d
  ret i64 %rax.phi
eh.ok.401b7d:  ; 0x401b7d
  %m = load i8, ptr %stack.arg0.p, align 1  ; 0x401b85
  %logic = and i8 %m, 1  ; 0x401b85
  %zf.1 = icmp eq i8 %logic, 0  ; 0x401b85
  br i1 %zf.1, label %b_401b97, label %b_401b8c
eh.ok.401b8f:  ; 0x401b8f
  br label %b_401b97
}

define i64 @Problem_vf0(i64 %arg0, i64 %arg1, i64 %arg2, i64 %arg3, i64 %arg4, i64 %arg5) {  ; 0x401ba0
b_401ba0:  ; 0x401ba0
  %frame = alloca [140 x i8], align 16
  %frame.top = getelementptr inbounds i8, ptr %frame, i64 44
  %local_mc = alloca i32, align 4
  %local_m8 = alloca i32, align 4
  %local_m4 = alloca i32, align 4
  %local_p0 = alloca i32, align 4
  %local_p8 = alloca i32, align 4
  %local_pc = alloca i32, align 4
  %local_p10 = alloca i32, align 4
  %local_p14 = alloca i32, align 4
  %stack.arg0.p = getelementptr inbounds i8, ptr %frame.top, i64 4
  %stack.arg0.lo = trunc i64 %arg1 to i32
  store i32 %stack.arg0.lo, ptr %stack.arg0.p, align 4
  %stack.arg1.lo = trunc i64 %arg2 to i32
  store i32 %stack.arg1.lo, ptr %local_p8, align 4
  %stack.arg2.lo = trunc i64 %arg3 to i32
  store i32 %stack.arg2.lo, ptr %local_pc, align 4
  %stack.arg3.lo = trunc i64 %arg4 to i32
  store i32 %stack.arg3.lo, ptr %local_p10, align 4
  %stack.arg4.lo = trunc i64 %arg5 to i32
  store i32 %stack.arg4.lo, ptr %local_p14, align 4
  %esi = trunc i64 undef to i32  ; 0x401ba0
  store i32 %esi, ptr %local_m4, align 1  ; 0x401ba0
  %ecx = trunc i64 %arg0 to i32  ; 0x401ba1
  %esi.z = zext i32 %ecx to i64  ; 0x401ba1
  %st.p.1 = inttoptr i64 %esi.z to ptr  ; 0x401ba3
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 328) to i32), ptr %st.p.1, align 1  ; 0x401ba3
  %addr = add i32 %ecx, 4  ; 0x401ba9
  store i32 %addr, ptr %local_m8, align 1  ; 0x401bac
  %sarg0 = zext i32 %addr to i64
  %sarg1.d = load i32, ptr %local_m4, align 4
  %sarg1 = zext i32 %sarg1.d to i64
  %sarg2.d = load i32, ptr %local_p0, align 4
  %sarg2 = zext i32 %sarg2.d to i64
  %sarg3.d = load i32, ptr %stack.arg0.p, align 4
  %sarg3 = zext i32 %sarg3.d to i64
  %sarg4.d = load i32, ptr %local_p8, align 4
  %sarg4 = zext i32 %sarg4.d to i64
  %sarg5.d = load i32, ptr %local_pc, align 4
  %sarg5 = zext i32 %sarg5.d to i64
  %sarg6.d = load i32, ptr %local_p10, align 4
  %sarg6 = zext i32 %sarg6.d to i64
  %sarg7.d = load i32, ptr %local_p14, align 4
  %sarg7 = zext i32 %sarg7.d to i64
  %ret = call i64 @thunk___std_exception_destroy(i64 %arg0, i64 %sarg0, i64 %sarg1, i64 %sarg2, i64 %sarg3, i64 %sarg4, i64 %sarg5, i64 %sarg6, i64 %sarg7)  ; 0x401bad
  %eh.pending = call i64 @ra_eh_pending()  ; 0x401bad
  %eh.thrown = icmp ne i64 %eh.pending, 0  ; 0x401bad
  br i1 %eh.thrown, label %eh.landing, label %eh.ok.401bad
b_401bbc:  ; 0x401bbc
  store i32 16, ptr %local_m8, align 1  ; 0x401bbc
  store i32 %ecx, ptr %local_mc, align 1  ; 0x401bbe
  %ret.1 = call i64 @F_00402530(i32 %ecx)  ; 0x401bbf
  %eh.pending.1 = call i64 @ra_eh_pending()  ; 0x401bbf
  %eh.thrown.1 = icmp ne i64 %eh.pending.1, 0  ; 0x401bbf
  br i1 %eh.thrown.1, label %eh.landing, label %eh.ok.401bbf
b_401bc7:  ; 0x401bc7
  ret i64 %esi.z
eh.landing:  ; 0x401bad
  %rax.phi = phi i64 [ %ret, %b_401ba0 ], [ %ret.1, %b_401bbc ]  ; 0x401bad
  ret i64 %rax.phi
eh.ok.401bad:  ; 0x401bad
  %m = load i8, ptr %stack.arg0.p, align 1  ; 0x401bb5
  %logic = and i8 %m, 1  ; 0x401bb5
  %zf.1 = icmp eq i8 %logic, 0  ; 0x401bb5
  br i1 %zf.1, label %b_401bc7, label %b_401bbc
eh.ok.401bbf:  ; 0x401bbf
  br label %b_401bc7
}

define i64 @F_00401bd0(i64 %arg0, i64 %arg1, i64 %arg2, i64 %arg3) {  ; 0x401bd0
b_401bd0:  ; 0x401bd0
  %local_p4 = alloca i32, align 4
  %local_p8 = alloca i32, align 4
  %local_pc = alloca i32, align 4
  %local_p10 = alloca i32, align 4
  %stack.arg0.lo = trunc i64 %arg0 to i32
  store i32 %stack.arg0.lo, ptr %local_p4, align 4
  %stack.arg1.lo = trunc i64 %arg1 to i32
  store i32 %stack.arg1.lo, ptr %local_p8, align 4
  %stack.arg2.lo = trunc i64 %arg2 to i32
  store i32 %stack.arg2.lo, ptr %local_pc, align 4
  %stack.arg3.lo = trunc i64 %arg3 to i32
  store i32 %stack.arg3.lo, ptr %local_p10, align 4
  %sarg0 = zext i32 %stack.arg0.lo to i64
  %sarg1 = zext i32 %stack.arg1.lo to i64
  %sarg2 = zext i32 %stack.arg2.lo to i64
  %sarg3 = zext i32 %stack.arg3.lo to i64
  %ret = call i64 @thunk___CxxFrameHandler3(i64 %sarg0, i64 %sarg1, i64 %sarg2, i64 %sarg3)  ; 0x401bd5
  ret i64 %ret
}

define i64 @F_00401be0(i64 %arg0, i64 %arg1, i64 %arg2, i64 %arg3) {  ; 0x401be0
b_401be0:  ; 0x401be0
  %local_p4 = alloca i32, align 4
  %local_p8 = alloca i32, align 4
  %local_pc = alloca i32, align 4
  %local_p10 = alloca i32, align 4
  %stack.arg0.lo = trunc i64 %arg0 to i32
  store i32 %stack.arg0.lo, ptr %local_p4, align 4
  %stack.arg1.lo = trunc i64 %arg1 to i32
  store i32 %stack.arg1.lo, ptr %local_p8, align 4
  %stack.arg2.lo = trunc i64 %arg2 to i32
  store i32 %stack.arg2.lo, ptr %local_pc, align 4
  %stack.arg3.lo = trunc i64 %arg3 to i32
  store i32 %stack.arg3.lo, ptr %local_p10, align 4
  %sarg0 = zext i32 %stack.arg0.lo to i64
  %sarg1 = zext i32 %stack.arg1.lo to i64
  %sarg2 = zext i32 %stack.arg2.lo to i64
  %sarg3 = zext i32 %stack.arg3.lo to i64
  %ret = call i64 @thunk___CxxFrameHandler3(i64 %sarg0, i64 %sarg1, i64 %sarg2, i64 %sarg3)  ; 0x401be5
  ret i64 %ret
}

define i64 @F_00401bf0(i64 %arg0, i64 %arg1, i64 %arg2, i64 %arg3) {  ; 0x401bf0
b_401bf0:  ; 0x401bf0
  %local_p4 = alloca i32, align 4
  %local_p8 = alloca i32, align 4
  %local_pc = alloca i32, align 4
  %local_p10 = alloca i32, align 4
  %stack.arg0.lo = trunc i64 %arg0 to i32
  store i32 %stack.arg0.lo, ptr %local_p4, align 4
  %stack.arg1.lo = trunc i64 %arg1 to i32
  store i32 %stack.arg1.lo, ptr %local_p8, align 4
  %stack.arg2.lo = trunc i64 %arg2 to i32
  store i32 %stack.arg2.lo, ptr %local_pc, align 4
  %stack.arg3.lo = trunc i64 %arg3 to i32
  store i32 %stack.arg3.lo, ptr %local_p10, align 4
  %sarg0 = zext i32 %stack.arg0.lo to i64
  %sarg1 = zext i32 %stack.arg1.lo to i64
  %sarg2 = zext i32 %stack.arg2.lo to i64
  %sarg3 = zext i32 %stack.arg3.lo to i64
  %ret = call i64 @thunk___CxxFrameHandler3(i64 %sarg0, i64 %sarg1, i64 %sarg2, i64 %sarg3)  ; 0x401bf5
  ret i64 %ret
}

define i64 @F_00401c00(i64 %arg0, i64 %arg1, i64 %arg2, i64 %arg3) {  ; 0x401c00
b_401c00:  ; 0x401c00
  %local_p4 = alloca i32, align 4
  %local_p8 = alloca i32, align 4
  %local_pc = alloca i32, align 4
  %local_p10 = alloca i32, align 4
  %stack.arg0.lo = trunc i64 %arg0 to i32
  store i32 %stack.arg0.lo, ptr %local_p4, align 4
  %stack.arg1.lo = trunc i64 %arg1 to i32
  store i32 %stack.arg1.lo, ptr %local_p8, align 4
  %stack.arg2.lo = trunc i64 %arg2 to i32
  store i32 %stack.arg2.lo, ptr %local_pc, align 4
  %stack.arg3.lo = trunc i64 %arg3 to i32
  store i32 %stack.arg3.lo, ptr %local_p10, align 4
  %sarg0 = zext i32 %stack.arg0.lo to i64
  %sarg1 = zext i32 %stack.arg1.lo to i64
  %sarg2 = zext i32 %stack.arg2.lo to i64
  %sarg3 = zext i32 %stack.arg3.lo to i64
  %ret = call i64 @thunk___CxxFrameHandler3(i64 %sarg0, i64 %sarg1, i64 %sarg2, i64 %sarg3)  ; 0x401c05
  ret i64 %ret
}

define i64 @F_00401c10(i64 %arg0, i64 %arg1, i64 %arg2, i64 %arg3) {  ; 0x401c10
b_401c10:  ; 0x401c10
  %local_p4 = alloca i32, align 4
  %local_p8 = alloca i32, align 4
  %local_pc = alloca i32, align 4
  %local_p10 = alloca i32, align 4
  %stack.arg0.lo = trunc i64 %arg0 to i32
  store i32 %stack.arg0.lo, ptr %local_p4, align 4
  %stack.arg1.lo = trunc i64 %arg1 to i32
  store i32 %stack.arg1.lo, ptr %local_p8, align 4
  %stack.arg2.lo = trunc i64 %arg2 to i32
  store i32 %stack.arg2.lo, ptr %local_pc, align 4
  %stack.arg3.lo = trunc i64 %arg3 to i32
  store i32 %stack.arg3.lo, ptr %local_p10, align 4
  %sarg0 = zext i32 %stack.arg0.lo to i64
  %sarg1 = zext i32 %stack.arg1.lo to i64
  %sarg2 = zext i32 %stack.arg2.lo to i64
  %sarg3 = zext i32 %stack.arg3.lo to i64
  %ret = call i64 @thunk___CxxFrameHandler3(i64 %sarg0, i64 %sarg1, i64 %sarg2, i64 %sarg3)  ; 0x401c15
  ret i64 %ret
}

define i64 @F_00401c20(i64 %arg0, i64 %arg1, i64 %arg2, i64 %arg3) {  ; 0x401c20
b_401c20:  ; 0x401c20
  %local_p4 = alloca i32, align 4
  %local_p8 = alloca i32, align 4
  %local_pc = alloca i32, align 4
  %local_p10 = alloca i32, align 4
  %stack.arg0.lo = trunc i64 %arg0 to i32
  store i32 %stack.arg0.lo, ptr %local_p4, align 4
  %stack.arg1.lo = trunc i64 %arg1 to i32
  store i32 %stack.arg1.lo, ptr %local_p8, align 4
  %stack.arg2.lo = trunc i64 %arg2 to i32
  store i32 %stack.arg2.lo, ptr %local_pc, align 4
  %stack.arg3.lo = trunc i64 %arg3 to i32
  store i32 %stack.arg3.lo, ptr %local_p10, align 4
  %sarg0 = zext i32 %stack.arg0.lo to i64
  %sarg1 = zext i32 %stack.arg1.lo to i64
  %sarg2 = zext i32 %stack.arg2.lo to i64
  %sarg3 = zext i32 %stack.arg3.lo to i64
  %ret = call i64 @thunk___CxxFrameHandler3(i64 %sarg0, i64 %sarg1, i64 %sarg2, i64 %sarg3)  ; 0x401c25
  ret i64 %ret
}

define i64 @F_00401c30() {  ; 0x401c30
b_401c30:  ; 0x401c30
  %addr.z = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 164) to i32) to i64  ; 0x401c30
  %m.p = inttoptr i64 %addr.z to ptr  ; 0x401c30
  %m = load i32, ptr %m.p, align 1  ; 0x401c30
  %eax.z = zext i32 %m to i64  ; 0x401c30
  ret i64 %eax.z
}

define i64 @F_00401c40(i32 %arg0, i32 %arg1) {  ; 0x401c40
b_401c40:  ; 0x401c40
  %frame = alloca [176 x i8], align 16
  %frame.top = getelementptr inbounds i8, ptr %frame, i64 92
  %rsp0 = ptrtoint ptr %frame.top to i64
  %stack.arg0.p = getelementptr inbounds i8, ptr %frame.top, i64 4
  store i32 %arg0, ptr %stack.arg0.p, align 4
  %stack.arg1.p = getelementptr inbounds i8, ptr %frame.top, i64 8
  store i32 %arg1, ptr %stack.arg1.p, align 4
  %ebp = trunc i64 undef to i32  ; 0x401c40
  %esp = trunc i64 %rsp0 to i32  ; 0x401c40
  %sp = sub i32 %esp, 4  ; 0x401c40
  %st.p = getelementptr inbounds i8, ptr %frame.top, i64 -4  ; 0x401c40
  store i32 %ebp, ptr %st.p, align 1  ; 0x401c40
  %sp.1 = sub i32 %sp, 4  ; 0x401c41
  %st.p.1 = getelementptr inbounds i8, ptr %frame.top, i64 -8  ; 0x401c41
  store i32 %ebp, ptr %st.p.1, align 1  ; 0x401c41
  %sp.2 = sub i32 %sp.1, 4  ; 0x401c42
  %st.p.2 = getelementptr inbounds i8, ptr %frame.top, i64 -12  ; 0x401c42
  store i32 %ebp, ptr %st.p.2, align 1  ; 0x401c42
  %sp.3 = sub i32 %sp.2, 4  ; 0x401c43
  %st.p.3 = getelementptr inbounds i8, ptr %frame.top, i64 -16  ; 0x401c43
  store i32 %ebp, ptr %st.p.3, align 1  ; 0x401c43
  %diff = sub i32 %sp.3, 20  ; 0x401c44
  %esi.z = zext i32 %arg0 to i64  ; 0x401c47
  %addr.z.5 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 192) to i32) to i64  ; 0x401c4b
  %m.p.1 = inttoptr i64 %addr.z.5 to ptr  ; 0x401c4b
  %m.1 = load i32, ptr %m.p.1, align 1  ; 0x401c4b
  %logic = xor i32 %m.1, %diff  ; 0x401c50
  %st.p.4 = getelementptr inbounds i8, ptr %frame.top, i64 -20  ; 0x401c52
  store i32 %logic, ptr %st.p.4, align 1  ; 0x401c52
  %m.p.2 = inttoptr i64 %esi.z to ptr  ; 0x401c56
  %m.2 = load i8, ptr %m.p.2, align 1  ; 0x401c56
  %addr.z.8 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 332) to i32) to i64  ; 0x401c59
  %m.p.3 = inttoptr i64 %addr.z.8 to ptr  ; 0x401c59
  %m.3 = load i32, ptr %m.p.3, align 1  ; 0x401c59
  %zf.2 = icmp eq i8 %m.2, 0  ; 0x401c5f
  br i1 %zf.2, label %b_401cbd, label %b_401c63
b_401c63:  ; 0x401c63
  %sum = add i32 %arg0, 1  ; 0x401c63
  %addr.2 = add i32 %diff, 4  ; 0x401c6a
  br label %b_401c99
b_401c80:  ; 0x401c80
  %rax.phi.1 = phi i8 [ %rax.phi, %b_401c99 ], [ %m.7, %b_401ca1 ]  ; 0x401c80
  %rcx.phi.1 = phi i32 [ %rcx.phi, %b_401c99 ], [ 0, %b_401ca1 ]  ; 0x401c80
  %rsp.phi.1 = phi i32 [ %rsp.phi, %b_401c99 ], [ %sp.11, %b_401ca1 ]  ; 0x401c80
  %addr.3 = add i32 %rcx.phi.1, 1  ; 0x401c80
  store i32 %addr.3, ptr %m.p.3, align 1  ; 0x401c83
  %addr.4 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), %rcx.phi.1  ; 0x401c89
  %addr.z.12 = zext i32 %addr.4 to i64  ; 0x401c89
  %st.p.6 = inttoptr i64 %addr.z.12 to ptr  ; 0x401c89
  store i8 %rax.phi.1, ptr %st.p.6, align 1  ; 0x401c89
  %addr.z.13 = zext i32 %rsi.phi to i64  ; 0x401c8f
  %m.p.6 = inttoptr i64 %addr.z.13 to ptr  ; 0x401c8f
  %m.6 = load i8, ptr %m.p.6, align 1  ; 0x401c8f
  %sum.1 = add i32 %rsi.phi, 1  ; 0x401c92
  %zf.6 = icmp eq i8 %m.6, 0  ; 0x401c95
  br i1 %zf.6, label %b_401cbf, label %b_401c99
b_401c99:  ; 0x401c99
  %rax.phi = phi i8 [ %m.2, %b_401c63 ], [ %m.6, %b_401c80 ]  ; 0x401c99
  %rcx.phi = phi i32 [ %m.3, %b_401c63 ], [ %addr.3, %b_401c80 ]  ; 0x401c99
  %rsp.phi = phi i32 [ %diff, %b_401c63 ], [ %rsp.phi.1, %b_401c80 ]  ; 0x401c99
  %rsi.phi = phi i32 [ %sum, %b_401c63 ], [ %sum.1, %b_401c80 ]  ; 0x401c99
  %not = icmp ne i32 %rcx.phi, 512  ; 0x401c9f
  br i1 %not, label %b_401c80, label %b_401ca1
b_401ca1:  ; 0x401ca1
  %sp.4 = sub i32 %rsp.phi, 4  ; 0x401ca1
  %st.p.7 = getelementptr inbounds i8, ptr %frame.top, i64 -40  ; 0x401ca1
  store i32 -11, ptr %st.p.7, align 1  ; 0x401ca1
  %ret = call i64 @GetStdHandle(i64 4294967285)  ; 0x401ca3
  %sp.5 = add i32 %sp.4, 4  ; 0x401ca3
  %sp.6 = sub i32 %sp.5, 4  ; 0x401ca5
  store i32 0, ptr %st.p.7, align 1  ; 0x401ca5
  %sp.7 = sub i32 %sp.6, 4  ; 0x401ca7
  %st.p.9 = getelementptr inbounds i8, ptr %frame.top, i64 -44  ; 0x401ca7
  store i32 %addr.2, ptr %st.p.9, align 1  ; 0x401ca7
  %sp.8 = sub i32 %sp.7, 4  ; 0x401ca8
  %st.p.10 = getelementptr inbounds i8, ptr %frame.top, i64 -48  ; 0x401ca8
  store i32 512, ptr %st.p.10, align 1  ; 0x401ca8
  %sp.9 = sub i32 %sp.8, 4  ; 0x401cad
  %st.p.11 = getelementptr inbounds i8, ptr %frame.top, i64 -52  ; 0x401cad
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), ptr %st.p.11, align 1  ; 0x401cad
  %eax.2 = trunc i64 %ret to i32  ; 0x401cb2
  %sp.10 = sub i32 %sp.9, 4  ; 0x401cb2
  %st.p.12 = getelementptr inbounds i8, ptr %frame.top, i64 -56  ; 0x401cb2
  store i32 %eax.2, ptr %st.p.12, align 1  ; 0x401cb2
  %sarg0.1 = zext i32 %eax.2 to i64
  %sarg1.1 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32) to i64
  %sarg3.1 = zext i32 %addr.2 to i64
  %ret.1 = call i64 @WriteFile(i64 %sarg0.1, i64 %sarg1.1, i64 512, i64 %sarg3.1, i64 0)  ; 0x401cb3
  %sp.11 = add i32 %sp.10, 20  ; 0x401cb3
  %addr.5 = add i32 %rsi.phi, -1  ; 0x401cb5
  %addr.z.20 = zext i32 %addr.5 to i64  ; 0x401cb5
  %m.p.7 = inttoptr i64 %addr.z.20 to ptr  ; 0x401cb5
  %m.7 = load i8, ptr %m.p.7, align 1  ; 0x401cb5
  br label %b_401c80
b_401cbd:  ; 0x401cbd
  br label %b_401cbf
b_401cbf:  ; 0x401cbf
  %rdx.phi = phi i32 [ %addr.3, %b_401c80 ], [ %m.3, %b_401cbd ]  ; 0x401cbf
  %rsp.phi.2 = phi i32 [ %rsp.phi.1, %b_401c80 ], [ %diff, %b_401cbd ]  ; 0x401cbf
  %m.8 = load i32, ptr %stack.arg1.p, align 1  ; 0x401cbf
  %not.1 = icmp ne i32 %rdx.phi, 512  ; 0x401cc9
  br i1 %not.1, label %b_401cf9, label %b_401ccb
b_401ccb:  ; 0x401ccb
  %sp.12 = sub i32 %rsp.phi.2, 4  ; 0x401ccb
  %st.p.13 = getelementptr inbounds i8, ptr %frame.top, i64 -40  ; 0x401ccb
  store i32 -11, ptr %st.p.13, align 1  ; 0x401ccb
  %ret.2 = call i64 @GetStdHandle(i64 4294967285)  ; 0x401ccd
  %sp.13 = add i32 %sp.12, 4  ; 0x401ccd
  %addr.7 = add i32 %sp.13, 4  ; 0x401cd3
  %sp.14 = sub i32 %sp.13, 4  ; 0x401cd7
  store i32 0, ptr %st.p.13, align 1  ; 0x401cd7
  %sp.15 = sub i32 %sp.14, 4  ; 0x401cd9
  %st.p.15 = getelementptr inbounds i8, ptr %frame.top, i64 -44  ; 0x401cd9
  store i32 %addr.7, ptr %st.p.15, align 1  ; 0x401cd9
  %sp.16 = sub i32 %sp.15, 4  ; 0x401cda
  %st.p.16 = getelementptr inbounds i8, ptr %frame.top, i64 -48  ; 0x401cda
  store i32 512, ptr %st.p.16, align 1  ; 0x401cda
  %sp.17 = sub i32 %sp.16, 4  ; 0x401cdf
  %st.p.17 = getelementptr inbounds i8, ptr %frame.top, i64 -52  ; 0x401cdf
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), ptr %st.p.17, align 1  ; 0x401cdf
  %eax.3 = trunc i64 %ret.2 to i32  ; 0x401ce4
  %sp.18 = sub i32 %sp.17, 4  ; 0x401ce4
  %st.p.18 = getelementptr inbounds i8, ptr %frame.top, i64 -56  ; 0x401ce4
  store i32 %eax.3, ptr %st.p.18, align 1  ; 0x401ce4
  %sarg0.3 = zext i32 %eax.3 to i64
  %sarg1.3 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32) to i64
  %sarg3.3 = zext i32 %addr.7 to i64
  %ret.3 = call i64 @WriteFile(i64 %sarg0.3, i64 %sarg1.3, i64 512, i64 %sarg3.3, i64 0)  ; 0x401ce5
  %sp.19 = add i32 %sp.18, 20  ; 0x401ce5
  %st.p.19 = inttoptr i64 %sarg1.3 to ptr  ; 0x401ceb
  store i8 32, ptr %st.p.19, align 1  ; 0x401ceb
  br label %b_401d35
b_401cf9:  ; 0x401cf9
  %addr.8 = add i32 %rdx.phi, 1  ; 0x401cf9
  store i32 %addr.8, ptr %m.p.3, align 1  ; 0x401cfc
  %addr.9 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), %rdx.phi  ; 0x401d01
  %addr.z.30 = zext i32 %addr.9 to i64  ; 0x401d01
  %st.p.21 = inttoptr i64 %addr.z.30 to ptr  ; 0x401d01
  store i8 32, ptr %st.p.21, align 1  ; 0x401d01
  %not.2 = icmp ne i32 %addr.8, 512  ; 0x401d0d
  br i1 %not.2, label %b_402022, label %b_401d13
b_401d13:  ; 0x401d13
  %sp.20 = sub i32 %rsp.phi.2, 4  ; 0x401d13
  %st.p.22 = getelementptr inbounds i8, ptr %frame.top, i64 -40  ; 0x401d13
  store i32 -11, ptr %st.p.22, align 1  ; 0x401d13
  %ret.4 = call i64 @GetStdHandle(i64 4294967285)  ; 0x401d15
  %sp.21 = add i32 %sp.20, 4  ; 0x401d15
  %addr.10 = add i32 %sp.21, 4  ; 0x401d1b
  %sp.22 = sub i32 %sp.21, 4  ; 0x401d1f
  store i32 0, ptr %st.p.22, align 1  ; 0x401d1f
  %sp.23 = sub i32 %sp.22, 4  ; 0x401d21
  %st.p.24 = getelementptr inbounds i8, ptr %frame.top, i64 -44  ; 0x401d21
  store i32 %addr.10, ptr %st.p.24, align 1  ; 0x401d21
  %sp.24 = sub i32 %sp.23, 4  ; 0x401d22
  %st.p.25 = getelementptr inbounds i8, ptr %frame.top, i64 -48  ; 0x401d22
  store i32 512, ptr %st.p.25, align 1  ; 0x401d22
  %sp.25 = sub i32 %sp.24, 4  ; 0x401d27
  %st.p.26 = getelementptr inbounds i8, ptr %frame.top, i64 -52  ; 0x401d27
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), ptr %st.p.26, align 1  ; 0x401d27
  %eax.6 = trunc i64 %ret.4 to i32  ; 0x401d2c
  %sp.26 = sub i32 %sp.25, 4  ; 0x401d2c
  %st.p.27 = getelementptr inbounds i8, ptr %frame.top, i64 -56  ; 0x401d2c
  store i32 %eax.6, ptr %st.p.27, align 1  ; 0x401d2c
  %sarg0.5 = zext i32 %eax.6 to i64
  %sarg1.5 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32) to i64
  %sarg3.5 = zext i32 %addr.10 to i64
  %ret.5 = call i64 @WriteFile(i64 %sarg0.5, i64 %sarg1.5, i64 512, i64 %sarg3.5, i64 0)  ; 0x401d2d
  %sp.27 = add i32 %sp.26, 20  ; 0x401d2d
  br label %b_401d35
b_401d35:  ; 0x401d35
  %rax.phi.2 = phi i32 [ 1, %b_401ccb ], [ 0, %b_401d13 ]  ; 0x401d35
  %rsp.phi.3 = phi i32 [ %sp.19, %b_401ccb ], [ %sp.27, %b_401d13 ]  ; 0x401d35
  %addr.11 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), %rax.phi.2  ; 0x401d35
  %addr.z.37 = zext i32 %addr.11 to i64  ; 0x401d35
  %st.p.28 = inttoptr i64 %addr.z.37 to ptr  ; 0x401d35
  store i8 61, ptr %st.p.28, align 1  ; 0x401d35
  %sum.2 = add i32 %rax.phi.2, 1  ; 0x401d3c
  br label %b_401d3d
b_401d3d:  ; 0x401d3d
  %rax.phi.3 = phi i32 [ %sum.2, %b_401d35 ], [ %addr.45, %b_402022 ], [ 0, %b_40203c ]  ; 0x401d3d
  %rsp.phi.4 = phi i32 [ %rsp.phi.3, %b_401d35 ], [ %rsp.phi.2, %b_402022 ], [ %sp.105, %b_40203c ]  ; 0x401d3d
  %addr.12 = add i32 %rax.phi.3, 1  ; 0x401d3d
  store i32 %addr.12, ptr %m.p.3, align 1  ; 0x401d40
  %addr.13 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), %rax.phi.3  ; 0x401d46
  %addr.z.39 = zext i32 %addr.13 to i64  ; 0x401d46
  %st.p.30 = inttoptr i64 %addr.z.39 to ptr  ; 0x401d46
  store i8 32, ptr %st.p.30, align 1  ; 0x401d46
  %sf.12 = icmp slt i32 %m.8, 0  ; 0x401d4d
  br i1 %sf.12, label %b_401de1, label %b_401d55
b_401d55:  ; 0x401d55
  %st.p.31 = getelementptr inbounds i8, ptr %frame.top, i64 -21  ; 0x401d55
  store i8 0, ptr %st.p.31, align 1  ; 0x401d55
  %addr.15 = add i32 %rsp.phi.4, 16  ; 0x401d5a
  br label %b_401d70
b_401d70:  ; 0x401d70
  %rcx.phi.2 = phi i32 [ %m.8, %b_401d55 ], [ %shifted.w, %b_401d70 ]  ; 0x401d70
  %rdi.phi = phi i32 [ %addr.15, %b_401d55 ], [ %diff.5, %b_401d70 ]  ; 0x401d70
  %shifted.w = udiv i32 %rcx.phi.2, 10  ; 0x401d74
  %diff.4 = urem i32 %rcx.phi.2, 10  ; 0x401d7f
  %eax.z.13 = trunc i32 %diff.4 to i8  ; 0x401d7f
  %logic.4 = or i8 %eax.z.13, 48  ; 0x401d81
  %addr.18 = add i32 %rdi.phi, -2  ; 0x401d83
  %addr.z.41 = zext i32 %addr.18 to i64  ; 0x401d83
  %st.p.32 = inttoptr i64 %addr.z.41 to ptr  ; 0x401d83
  store i8 %logic.4, ptr %st.p.32, align 1  ; 0x401d83
  %diff.5 = sub i32 %rdi.phi, 1  ; 0x401d86
  %not.3 = icmp ugt i32 %rcx.phi.2, 9  ; 0x401d8c
  br i1 %not.3, label %b_401d70, label %b_401d8e
b_401d8e:  ; 0x401d8e
  br label %b_401db9
b_401da0:  ; 0x401da0
  %rax.phi.5 = phi i8 [ %rax.phi.4, %b_401db9 ], [ %m.11, %b_401dc1 ]  ; 0x401da0
  %rsp.phi.7 = phi i32 [ %rsp.phi.6, %b_401db9 ], [ %sp.35, %b_401dc1 ]  ; 0x401da0
  %rsi.phi.4 = phi i32 [ %rsi.phi.3, %b_401db9 ], [ 0, %b_401dc1 ]  ; 0x401da0
  %addr.19 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), %rsi.phi.4  ; 0x401da0
  %addr.z.43 = zext i32 %addr.19 to i64  ; 0x401da0
  %st.p.33 = inttoptr i64 %addr.z.43 to ptr  ; 0x401da0
  store i8 %rax.phi.5, ptr %st.p.33, align 1  ; 0x401da0
  %sum.3 = add i32 %rsi.phi.4, 1  ; 0x401da6
  store i32 %sum.3, ptr %m.p.3, align 1  ; 0x401da7
  %addr.z.45 = zext i32 %rdi.phi.1 to i64  ; 0x401dad
  %m.p.10 = inttoptr i64 %addr.z.45 to ptr  ; 0x401dad
  %m.10 = load i8, ptr %m.p.10, align 1  ; 0x401dad
  %sum.4 = add i32 %rdi.phi.1, 1  ; 0x401db0
  %zf.26 = icmp eq i8 %m.10, 0  ; 0x401db1
  br i1 %zf.26, label %b_401e9d, label %b_401db9
b_401db9:  ; 0x401db9
  %rax.phi.4 = phi i8 [ %logic.4, %b_401d8e ], [ %m.10, %b_401da0 ]  ; 0x401db9
  %rsp.phi.6 = phi i32 [ %rsp.phi.4, %b_401d8e ], [ %rsp.phi.7, %b_401da0 ]  ; 0x401db9
  %rsi.phi.3 = phi i32 [ %addr.12, %b_401d8e ], [ %sum.3, %b_401da0 ]  ; 0x401db9
  %rdi.phi.1 = phi i32 [ %diff.5, %b_401d8e ], [ %sum.4, %b_401da0 ]  ; 0x401db9
  %not.4 = icmp ne i32 %rsi.phi.3, 512  ; 0x401dbf
  br i1 %not.4, label %b_401da0, label %b_401dc1
b_401dc1:  ; 0x401dc1
  %sp.28 = sub i32 %rsp.phi.6, 4  ; 0x401dc1
  %st.p.35 = getelementptr inbounds i8, ptr %frame.top, i64 -40  ; 0x401dc1
  store i32 -11, ptr %st.p.35, align 1  ; 0x401dc1
  %ret.8 = call i64 @GetStdHandle(i64 4294967285)  ; 0x401dc3
  %sp.29 = add i32 %sp.28, 4  ; 0x401dc3
  %sp.30 = sub i32 %sp.29, 4  ; 0x401dc9
  store i32 0, ptr %st.p.35, align 1  ; 0x401dc9
  %sp.31 = sub i32 %sp.30, 4  ; 0x401dcb
  %st.p.37 = getelementptr inbounds i8, ptr %frame.top, i64 -44  ; 0x401dcb
  store i32 %rsp.phi.4, ptr %st.p.37, align 1  ; 0x401dcb
  %sp.32 = sub i32 %sp.31, 4  ; 0x401dcc
  %st.p.38 = getelementptr inbounds i8, ptr %frame.top, i64 -48  ; 0x401dcc
  store i32 512, ptr %st.p.38, align 1  ; 0x401dcc
  %sp.33 = sub i32 %sp.32, 4  ; 0x401dd1
  %st.p.39 = getelementptr inbounds i8, ptr %frame.top, i64 -52  ; 0x401dd1
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), ptr %st.p.39, align 1  ; 0x401dd1
  %eax.15 = trunc i64 %ret.8 to i32  ; 0x401dd6
  %sp.34 = sub i32 %sp.33, 4  ; 0x401dd6
  %st.p.40 = getelementptr inbounds i8, ptr %frame.top, i64 -56  ; 0x401dd6
  store i32 %eax.15, ptr %st.p.40, align 1  ; 0x401dd6
  %sarg0.9 = zext i32 %eax.15 to i64
  %sarg1.9 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32) to i64
  %sarg3.9 = zext i32 %rsp.phi.4 to i64
  %ret.9 = call i64 @WriteFile(i64 %sarg0.9, i64 %sarg1.9, i64 512, i64 %sarg3.9, i64 0)  ; 0x401dd7
  %sp.35 = add i32 %sp.34, 20  ; 0x401dd7
  %addr.20 = add i32 %rdi.phi.1, -1  ; 0x401dd9
  %addr.z.52 = zext i32 %addr.20 to i64  ; 0x401dd9
  %m.p.11 = inttoptr i64 %addr.z.52 to ptr  ; 0x401dd9
  %m.11 = load i8, ptr %m.p.11, align 1  ; 0x401dd9
  br label %b_401da0
b_401de1:  ; 0x401de1
  %not.5 = icmp ne i32 %addr.12, 512  ; 0x401de7
  br i1 %not.5, label %b_401e0b, label %b_401de9
b_401de9:  ; 0x401de9
  %sp.36 = sub i32 %rsp.phi.4, 4  ; 0x401de9
  %st.p.41 = getelementptr inbounds i8, ptr %frame.top, i64 -40  ; 0x401de9
  store i32 -11, ptr %st.p.41, align 1  ; 0x401de9
  %ret.10 = call i64 @GetStdHandle(i64 4294967285)  ; 0x401deb
  %sp.37 = add i32 %sp.36, 4  ; 0x401deb
  %addr.21 = add i32 %sp.37, 4  ; 0x401df1
  %sp.38 = sub i32 %sp.37, 4  ; 0x401df5
  store i32 0, ptr %st.p.41, align 1  ; 0x401df5
  %sp.39 = sub i32 %sp.38, 4  ; 0x401df7
  %st.p.43 = getelementptr inbounds i8, ptr %frame.top, i64 -44  ; 0x401df7
  store i32 %addr.21, ptr %st.p.43, align 1  ; 0x401df7
  %sp.40 = sub i32 %sp.39, 4  ; 0x401df8
  %st.p.44 = getelementptr inbounds i8, ptr %frame.top, i64 -48  ; 0x401df8
  store i32 512, ptr %st.p.44, align 1  ; 0x401df8
  %sp.41 = sub i32 %sp.40, 4  ; 0x401dfd
  %st.p.45 = getelementptr inbounds i8, ptr %frame.top, i64 -52  ; 0x401dfd
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), ptr %st.p.45, align 1  ; 0x401dfd
  %eax.16 = trunc i64 %ret.10 to i32  ; 0x401e02
  %sp.42 = sub i32 %sp.41, 4  ; 0x401e02
  %st.p.46 = getelementptr inbounds i8, ptr %frame.top, i64 -56  ; 0x401e02
  store i32 %eax.16, ptr %st.p.46, align 1  ; 0x401e02
  %sarg0.11 = zext i32 %eax.16 to i64
  %sarg1.11 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32) to i64
  %sarg3.11 = zext i32 %addr.21 to i64
  %ret.11 = call i64 @WriteFile(i64 %sarg0.11, i64 %sarg1.11, i64 512, i64 %sarg3.11, i64 0)  ; 0x401e03
  %sp.43 = add i32 %sp.42, 20  ; 0x401e03
  br label %b_401e0b
b_401e0b:  ; 0x401e0b
  %rsp.phi.8 = phi i32 [ %rsp.phi.4, %b_401de1 ], [ %sp.43, %b_401de9 ]  ; 0x401e0b
  %rsi.phi.5 = phi i32 [ %addr.12, %b_401de1 ], [ 0, %b_401de9 ]  ; 0x401e0b
  %addr.22 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), %rsi.phi.5  ; 0x401e0b
  %addr.z.59 = zext i32 %addr.22 to i64  ; 0x401e0b
  %st.p.47 = inttoptr i64 %addr.z.59 to ptr  ; 0x401e0b
  store i8 45, ptr %st.p.47, align 1  ; 0x401e0b
  %sum.5 = add i32 %rsi.phi.5, 1  ; 0x401e12
  store i32 %sum.5, ptr %m.p.3, align 1  ; 0x401e13
  %diff.9 = sub i32 0, %m.8  ; 0x401e1b
  %st.p.49 = getelementptr inbounds i8, ptr %frame.top, i64 -21  ; 0x401e1d
  store i8 0, ptr %st.p.49, align 1  ; 0x401e1d
  %addr.24 = add i32 %rsp.phi.8, 16  ; 0x401e22
  br label %b_401e30
b_401e30:  ; 0x401e30
  %rcx.phi.3 = phi i32 [ %diff.9, %b_401e0b ], [ %shifted.w.1, %b_401e30 ]  ; 0x401e30
  %rdi.phi.3 = phi i32 [ %addr.24, %b_401e0b ], [ %diff.11, %b_401e30 ]  ; 0x401e30
  %shifted.w.1 = udiv i32 %rcx.phi.3, 10  ; 0x401e34
  %diff.10 = urem i32 %rcx.phi.3, 10  ; 0x401e3f
  %eax.z.20 = trunc i32 %diff.10 to i8  ; 0x401e3f
  %logic.6 = or i8 %eax.z.20, 48  ; 0x401e41
  %addr.27 = add i32 %rdi.phi.3, -2  ; 0x401e43
  %addr.z.62 = zext i32 %addr.27 to i64  ; 0x401e43
  %st.p.50 = inttoptr i64 %addr.z.62 to ptr  ; 0x401e43
  store i8 %logic.6, ptr %st.p.50, align 1  ; 0x401e43
  %diff.11 = sub i32 %rdi.phi.3, 1  ; 0x401e46
  %not.6 = icmp ugt i32 %rcx.phi.3, 9  ; 0x401e4c
  br i1 %not.6, label %b_401e30, label %b_401e4e
b_401e4e:  ; 0x401e4e
  br label %b_401e75
b_401e60:  ; 0x401e60
  %rax.phi.7 = phi i8 [ %rax.phi.6, %b_401e75 ], [ %m.14, %b_401e7d ]  ; 0x401e60
  %rsp.phi.11 = phi i32 [ %rsp.phi.10, %b_401e75 ], [ %sp.51, %b_401e7d ]  ; 0x401e60
  %rsi.phi.8 = phi i32 [ %rsi.phi.7, %b_401e75 ], [ 0, %b_401e7d ]  ; 0x401e60
  %addr.28 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), %rsi.phi.8  ; 0x401e60
  %addr.z.64 = zext i32 %addr.28 to i64  ; 0x401e60
  %st.p.51 = inttoptr i64 %addr.z.64 to ptr  ; 0x401e60
  store i8 %rax.phi.7, ptr %st.p.51, align 1  ; 0x401e60
  %sum.6 = add i32 %rsi.phi.8, 1  ; 0x401e66
  store i32 %sum.6, ptr %m.p.3, align 1  ; 0x401e67
  %addr.z.66 = zext i32 %rdi.phi.4 to i64  ; 0x401e6d
  %m.p.13 = inttoptr i64 %addr.z.66 to ptr  ; 0x401e6d
  %m.13 = load i8, ptr %m.p.13, align 1  ; 0x401e6d
  %sum.7 = add i32 %rdi.phi.4, 1  ; 0x401e70
  %zf.44 = icmp eq i8 %m.13, 0  ; 0x401e71
  br i1 %zf.44, label %b_401e9d, label %b_401e75
b_401e75:  ; 0x401e75
  %rax.phi.6 = phi i8 [ %logic.6, %b_401e4e ], [ %m.13, %b_401e60 ]  ; 0x401e75
  %rsp.phi.10 = phi i32 [ %rsp.phi.8, %b_401e4e ], [ %rsp.phi.11, %b_401e60 ]  ; 0x401e75
  %rsi.phi.7 = phi i32 [ %sum.5, %b_401e4e ], [ %sum.6, %b_401e60 ]  ; 0x401e75
  %rdi.phi.4 = phi i32 [ %diff.11, %b_401e4e ], [ %sum.7, %b_401e60 ]  ; 0x401e75
  %not.7 = icmp ne i32 %rsi.phi.7, 512  ; 0x401e7b
  br i1 %not.7, label %b_401e60, label %b_401e7d
b_401e7d:  ; 0x401e7d
  %sp.44 = sub i32 %rsp.phi.10, 4  ; 0x401e7d
  %st.p.53 = getelementptr inbounds i8, ptr %frame.top, i64 -40  ; 0x401e7d
  store i32 -11, ptr %st.p.53, align 1  ; 0x401e7d
  %ret.12 = call i64 @GetStdHandle(i64 4294967285)  ; 0x401e7f
  %sp.45 = add i32 %sp.44, 4  ; 0x401e7f
  %sp.46 = sub i32 %sp.45, 4  ; 0x401e85
  store i32 0, ptr %st.p.53, align 1  ; 0x401e85
  %sp.47 = sub i32 %sp.46, 4  ; 0x401e87
  %st.p.55 = getelementptr inbounds i8, ptr %frame.top, i64 -44  ; 0x401e87
  store i32 %rsp.phi.8, ptr %st.p.55, align 1  ; 0x401e87
  %sp.48 = sub i32 %sp.47, 4  ; 0x401e88
  %st.p.56 = getelementptr inbounds i8, ptr %frame.top, i64 -48  ; 0x401e88
  store i32 512, ptr %st.p.56, align 1  ; 0x401e88
  %sp.49 = sub i32 %sp.48, 4  ; 0x401e8d
  %st.p.57 = getelementptr inbounds i8, ptr %frame.top, i64 -52  ; 0x401e8d
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), ptr %st.p.57, align 1  ; 0x401e8d
  %eax.21 = trunc i64 %ret.12 to i32  ; 0x401e92
  %sp.50 = sub i32 %sp.49, 4  ; 0x401e92
  %st.p.58 = getelementptr inbounds i8, ptr %frame.top, i64 -56  ; 0x401e92
  store i32 %eax.21, ptr %st.p.58, align 1  ; 0x401e92
  %sarg0.13 = zext i32 %eax.21 to i64
  %sarg1.13 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32) to i64
  %sarg3.13 = zext i32 %rsp.phi.8 to i64
  %ret.13 = call i64 @WriteFile(i64 %sarg0.13, i64 %sarg1.13, i64 512, i64 %sarg3.13, i64 0)  ; 0x401e93
  %sp.51 = add i32 %sp.50, 20  ; 0x401e93
  %addr.29 = add i32 %rdi.phi.4, -1  ; 0x401e95
  %addr.z.73 = zext i32 %addr.29 to i64  ; 0x401e95
  %m.p.14 = inttoptr i64 %addr.z.73 to ptr  ; 0x401e95
  %m.14 = load i8, ptr %m.p.14, align 1  ; 0x401e95
  br label %b_401e60
b_401e9d:  ; 0x401e9d
  %rsp.phi.12 = phi i32 [ %rsp.phi.7, %b_401da0 ], [ %rsp.phi.11, %b_401e60 ]  ; 0x401e9d
  %rsi.phi.9 = phi i32 [ %sum.3, %b_401da0 ], [ %sum.6, %b_401e60 ]  ; 0x401e9d
  %m.15 = load i32, ptr %stack.arg1.p, align 1  ; 0x401ea3
  %ebx.z.6 = zext i32 %m.15 to i64  ; 0x401ea3
  %not.8 = icmp ne i32 %rsi.phi.9, 512  ; 0x401ea7
  br i1 %not.8, label %b_401ed7, label %b_401ea9
b_401ea9:  ; 0x401ea9
  %sp.52 = sub i32 %rsp.phi.12, 4  ; 0x401ea9
  %st.p.59 = getelementptr inbounds i8, ptr %frame.top, i64 -40  ; 0x401ea9
  store i32 -11, ptr %st.p.59, align 1  ; 0x401ea9
  %ret.14 = call i64 @GetStdHandle(i64 4294967285)  ; 0x401eab
  %sp.53 = add i32 %sp.52, 4  ; 0x401eab
  %addr.31 = add i32 %sp.53, 4  ; 0x401eb1
  %sp.54 = sub i32 %sp.53, 4  ; 0x401eb5
  store i32 0, ptr %st.p.59, align 1  ; 0x401eb5
  %sp.55 = sub i32 %sp.54, 4  ; 0x401eb7
  %st.p.61 = getelementptr inbounds i8, ptr %frame.top, i64 -44  ; 0x401eb7
  store i32 %addr.31, ptr %st.p.61, align 1  ; 0x401eb7
  %sp.56 = sub i32 %sp.55, 4  ; 0x401eb8
  %st.p.62 = getelementptr inbounds i8, ptr %frame.top, i64 -48  ; 0x401eb8
  store i32 512, ptr %st.p.62, align 1  ; 0x401eb8
  %sp.57 = sub i32 %sp.56, 4  ; 0x401ebd
  %st.p.63 = getelementptr inbounds i8, ptr %frame.top, i64 -52  ; 0x401ebd
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), ptr %st.p.63, align 1  ; 0x401ebd
  %eax.22 = trunc i64 %ret.14 to i32  ; 0x401ec2
  %sp.58 = sub i32 %sp.57, 4  ; 0x401ec2
  %st.p.64 = getelementptr inbounds i8, ptr %frame.top, i64 -56  ; 0x401ec2
  store i32 %eax.22, ptr %st.p.64, align 1  ; 0x401ec2
  %sarg0.15 = zext i32 %eax.22 to i64
  %sarg1.15 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32) to i64
  %sarg3.15 = zext i32 %addr.31 to i64
  %ret.15 = call i64 @WriteFile(i64 %sarg0.15, i64 %sarg1.15, i64 512, i64 %sarg3.15, i64 0)  ; 0x401ec3
  %sp.59 = add i32 %sp.58, 20  ; 0x401ec3
  %st.p.65 = inttoptr i64 %sarg1.15 to ptr  ; 0x401ec9
  store i8 32, ptr %st.p.65, align 1  ; 0x401ec9
  br label %b_401f0f
b_401ed7:  ; 0x401ed7
  %addr.32 = add i32 %rsi.phi.9, 1  ; 0x401ed7
  store i32 %addr.32, ptr %m.p.3, align 1  ; 0x401eda
  %addr.33 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), %rsi.phi.9  ; 0x401edf
  %addr.z.83 = zext i32 %addr.33 to i64  ; 0x401edf
  %st.p.67 = inttoptr i64 %addr.z.83 to ptr  ; 0x401edf
  store i8 32, ptr %st.p.67, align 1  ; 0x401edf
  %not.9 = icmp ne i32 %addr.32, 512  ; 0x401eeb
  br i1 %not.9, label %b_401f0f, label %b_401eed
b_401eed:  ; 0x401eed
  %sp.60 = sub i32 %rsp.phi.12, 4  ; 0x401eed
  %st.p.68 = getelementptr inbounds i8, ptr %frame.top, i64 -40  ; 0x401eed
  store i32 -11, ptr %st.p.68, align 1  ; 0x401eed
  %ret.16 = call i64 @GetStdHandle(i64 4294967285)  ; 0x401eef
  %sp.61 = add i32 %sp.60, 4  ; 0x401eef
  %addr.34 = add i32 %sp.61, 4  ; 0x401ef5
  %sp.62 = sub i32 %sp.61, 4  ; 0x401ef9
  store i32 0, ptr %st.p.68, align 1  ; 0x401ef9
  %sp.63 = sub i32 %sp.62, 4  ; 0x401efb
  %st.p.70 = getelementptr inbounds i8, ptr %frame.top, i64 -44  ; 0x401efb
  store i32 %addr.34, ptr %st.p.70, align 1  ; 0x401efb
  %sp.64 = sub i32 %sp.63, 4  ; 0x401efc
  %st.p.71 = getelementptr inbounds i8, ptr %frame.top, i64 -48  ; 0x401efc
  store i32 512, ptr %st.p.71, align 1  ; 0x401efc
  %sp.65 = sub i32 %sp.64, 4  ; 0x401f01
  %st.p.72 = getelementptr inbounds i8, ptr %frame.top, i64 -52  ; 0x401f01
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), ptr %st.p.72, align 1  ; 0x401f01
  %eax.25 = trunc i64 %ret.16 to i32  ; 0x401f06
  %sp.66 = sub i32 %sp.65, 4  ; 0x401f06
  %st.p.73 = getelementptr inbounds i8, ptr %frame.top, i64 -56  ; 0x401f06
  store i32 %eax.25, ptr %st.p.73, align 1  ; 0x401f06
  %sarg0.17 = zext i32 %eax.25 to i64
  %sarg1.17 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32) to i64
  %sarg3.17 = zext i32 %addr.34 to i64
  %ret.17 = call i64 @WriteFile(i64 %sarg0.17, i64 %sarg1.17, i64 512, i64 %sarg3.17, i64 0)  ; 0x401f07
  %sp.67 = add i32 %sp.66, 20  ; 0x401f07
  br label %b_401f0f
b_401f0f:  ; 0x401f0f
  %rax.phi.8 = phi i32 [ 1, %b_401ea9 ], [ %addr.32, %b_401ed7 ], [ 0, %b_401eed ]  ; 0x401f0f
  %rsp.phi.13 = phi i32 [ %sp.59, %b_401ea9 ], [ %rsp.phi.12, %b_401ed7 ], [ %sp.67, %b_401eed ]  ; 0x401f0f
  %addr.35 = add i32 %rax.phi.8, 1  ; 0x401f0f
  store i32 %addr.35, ptr %m.p.3, align 1  ; 0x401f12
  %addr.36 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), %rax.phi.8  ; 0x401f18
  %addr.z.91 = zext i32 %addr.36 to i64  ; 0x401f18
  %st.p.75 = inttoptr i64 %addr.z.91 to ptr  ; 0x401f18
  store i8 40, ptr %st.p.75, align 1  ; 0x401f18
  %sp.68 = sub i32 %rsp.phi.13, 4  ; 0x401f21
  %ret.18 = call i64 @F_00402070(i32 %m.15)  ; 0x401f21
  %rsp.134 = add i32 %sp.68, 4  ; 0x401f21
  %eh.pending = call i64 @ra_eh_pending()  ; 0x401f21
  %eh.thrown = icmp ne i64 %eh.pending, 0  ; 0x401f21
  br i1 %eh.thrown, label %eh.landing, label %eh.ok.401f21
b_401f34:  ; 0x401f34
  %sp.69 = sub i32 %rsp.134, 4  ; 0x401f34
  %st.p.77 = getelementptr inbounds i8, ptr %frame.top, i64 -40  ; 0x401f34
  store i32 -11, ptr %st.p.77, align 1  ; 0x401f34
  %ret.19 = call i64 @GetStdHandle(i64 4294967285)  ; 0x401f36
  %sp.70 = add i32 %sp.69, 4  ; 0x401f36
  %addr.37 = add i32 %sp.70, 4  ; 0x401f3c
  %sp.71 = sub i32 %sp.70, 4  ; 0x401f40
  store i32 0, ptr %st.p.77, align 1  ; 0x401f40
  %sp.72 = sub i32 %sp.71, 4  ; 0x401f42
  %st.p.79 = getelementptr inbounds i8, ptr %frame.top, i64 -44  ; 0x401f42
  store i32 %addr.37, ptr %st.p.79, align 1  ; 0x401f42
  %sp.73 = sub i32 %sp.72, 4  ; 0x401f43
  %st.p.80 = getelementptr inbounds i8, ptr %frame.top, i64 -48  ; 0x401f43
  store i32 512, ptr %st.p.80, align 1  ; 0x401f43
  %sp.74 = sub i32 %sp.73, 4  ; 0x401f48
  %st.p.81 = getelementptr inbounds i8, ptr %frame.top, i64 -52  ; 0x401f48
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), ptr %st.p.81, align 1  ; 0x401f48
  %eax.28 = trunc i64 %ret.19 to i32  ; 0x401f4d
  %sp.75 = sub i32 %sp.74, 4  ; 0x401f4d
  %st.p.82 = getelementptr inbounds i8, ptr %frame.top, i64 -56  ; 0x401f4d
  store i32 %eax.28, ptr %st.p.82, align 1  ; 0x401f4d
  %sarg0.19 = zext i32 %eax.28 to i64
  %sarg1.19 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32) to i64
  %sarg3.19 = zext i32 %addr.37 to i64
  %ret.20 = call i64 @WriteFile(i64 %sarg0.19, i64 %sarg1.19, i64 512, i64 %sarg3.19, i64 0)  ; 0x401f4e
  %sp.76 = add i32 %sp.75, 20  ; 0x401f4e
  %st.p.83 = inttoptr i64 %sarg1.19 to ptr  ; 0x401f54
  store i8 41, ptr %st.p.83, align 1  ; 0x401f54
  br label %b_401f9a
b_401f62:  ; 0x401f62
  %addr.38 = add i32 %m.16, 1  ; 0x401f62
  store i32 %addr.38, ptr %m.p.3, align 1  ; 0x401f65
  %addr.39 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), %m.16  ; 0x401f6a
  %addr.z.102 = zext i32 %addr.39 to i64  ; 0x401f6a
  %st.p.85 = inttoptr i64 %addr.z.102 to ptr  ; 0x401f6a
  store i8 41, ptr %st.p.85, align 1  ; 0x401f6a
  %not.11 = icmp ne i32 %addr.38, 512  ; 0x401f76
  br i1 %not.11, label %b_401f9a, label %b_401f78
b_401f78:  ; 0x401f78
  %sp.77 = sub i32 %rsp.134, 4  ; 0x401f78
  %st.p.86 = getelementptr inbounds i8, ptr %frame.top, i64 -40  ; 0x401f78
  store i32 -11, ptr %st.p.86, align 1  ; 0x401f78
  %ret.21 = call i64 @GetStdHandle(i64 4294967285)  ; 0x401f7a
  %sp.78 = add i32 %sp.77, 4  ; 0x401f7a
  %addr.40 = add i32 %sp.78, 4  ; 0x401f80
  %sp.79 = sub i32 %sp.78, 4  ; 0x401f84
  store i32 0, ptr %st.p.86, align 1  ; 0x401f84
  %sp.80 = sub i32 %sp.79, 4  ; 0x401f86
  %st.p.88 = getelementptr inbounds i8, ptr %frame.top, i64 -44  ; 0x401f86
  store i32 %addr.40, ptr %st.p.88, align 1  ; 0x401f86
  %sp.81 = sub i32 %sp.80, 4  ; 0x401f87
  %st.p.89 = getelementptr inbounds i8, ptr %frame.top, i64 -48  ; 0x401f87
  store i32 512, ptr %st.p.89, align 1  ; 0x401f87
  %sp.82 = sub i32 %sp.81, 4  ; 0x401f8c
  %st.p.90 = getelementptr inbounds i8, ptr %frame.top, i64 -52  ; 0x401f8c
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), ptr %st.p.90, align 1  ; 0x401f8c
  %eax.31 = trunc i64 %ret.21 to i32  ; 0x401f91
  %sp.83 = sub i32 %sp.82, 4  ; 0x401f91
  %st.p.91 = getelementptr inbounds i8, ptr %frame.top, i64 -56  ; 0x401f91
  store i32 %eax.31, ptr %st.p.91, align 1  ; 0x401f91
  %sarg0.21 = zext i32 %eax.31 to i64
  %sarg1.21 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32) to i64
  %sarg3.21 = zext i32 %addr.40 to i64
  %ret.22 = call i64 @WriteFile(i64 %sarg0.21, i64 %sarg1.21, i64 512, i64 %sarg3.21, i64 0)  ; 0x401f92
  %sp.84 = add i32 %sp.83, 20  ; 0x401f92
  br label %b_401f9a
b_401f9a:  ; 0x401f9a
  %rax.phi.9 = phi i32 [ 1, %b_401f34 ], [ %addr.38, %b_401f62 ], [ 0, %b_401f78 ]  ; 0x401f9a
  %rsp.phi.14 = phi i32 [ %sp.76, %b_401f34 ], [ %rsp.134, %b_401f62 ], [ %sp.84, %b_401f78 ]  ; 0x401f9a
  %addr.41 = add i32 %rax.phi.9, 1  ; 0x401f9a
  store i32 %addr.41, ptr %m.p.3, align 1  ; 0x401f9d
  %addr.42 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), %rax.phi.9  ; 0x401fa3
  %addr.z.110 = zext i32 %addr.42 to i64  ; 0x401fa3
  %st.p.93 = inttoptr i64 %addr.z.110 to ptr  ; 0x401fa3
  store i8 10, ptr %st.p.93, align 1  ; 0x401fa3
  %bl = trunc i64 %ebx.z.6 to i8  ; 0x401faa
  %eax.z.29 = zext i8 %bl to i32  ; 0x401faa
  %addr.z.111 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 168) to i32) to i64  ; 0x401fad
  %m.p.17 = inttoptr i64 %addr.z.111 to ptr  ; 0x401fad
  %m.17 = load i32, ptr %m.p.17, align 1  ; 0x401fad
  %logic.8 = xor i32 %eax.z.29, %m.17  ; 0x401fad
  %product.2 = mul i32 %logic.8, 16777619  ; 0x401fb3
  %hi = lshr i64 %ebx.z.6, 8  ; 0x401fb9
  %bh = trunc i64 %hi to i8  ; 0x401fb9
  %ecx.z.18 = zext i8 %bh to i32  ; 0x401fb9
  %logic.9 = xor i32 %ecx.z.18, %product.2  ; 0x401fbc
  %product.3 = mul i32 %logic.9, 16777619  ; 0x401fbe
  %shifted.w.2 = lshr i32 %m.15, 16  ; 0x401fc6
  %ecx.z.21 = trunc i32 %shifted.w.2 to i8  ; 0x401fc6
  %ecx.z.22 = zext i8 %ecx.z.21 to i32  ; 0x401fc9
  %logic.10 = xor i32 %ecx.z.22, %product.3  ; 0x401fcc
  %product.4 = mul i32 %logic.10, 16777619  ; 0x401fce
  %shifted.w.3 = lshr i32 %m.15, 24  ; 0x401fd4
  %logic.11 = xor i32 %shifted.w.3, %product.4  ; 0x401fd7
  %product.5 = mul i32 %logic.11, 16777619  ; 0x401fd9
  store i32 %product.5, ptr %m.p.17, align 1  ; 0x401fdf
  %sp.85 = sub i32 %rsp.phi.14, 4  ; 0x401fe4
  %st.p.95 = getelementptr inbounds i8, ptr %frame.top, i64 -40  ; 0x401fe4
  store i32 -11, ptr %st.p.95, align 1  ; 0x401fe4
  %ret.23 = call i64 @GetStdHandle(i64 4294967285)  ; 0x401fe6
  %sp.86 = add i32 %sp.85, 4  ; 0x401fe6
  %addr.43 = add i32 %sp.86, 4  ; 0x401fec
  %sp.87 = sub i32 %sp.86, 4  ; 0x401ff0
  store i32 0, ptr %st.p.95, align 1  ; 0x401ff0
  %sp.88 = sub i32 %sp.87, 4  ; 0x401ff2
  %st.p.97 = getelementptr inbounds i8, ptr %frame.top, i64 -44  ; 0x401ff2
  store i32 %addr.43, ptr %st.p.97, align 1  ; 0x401ff2
  %sp.89 = sub i32 %sp.88, 4  ; 0x401ff3
  %st.p.98 = getelementptr inbounds i8, ptr %frame.top, i64 -48  ; 0x401ff3
  store i32 %addr.41, ptr %st.p.98, align 1  ; 0x401ff3
  %sp.90 = sub i32 %sp.89, 4  ; 0x401ff4
  %st.p.99 = getelementptr inbounds i8, ptr %frame.top, i64 -52  ; 0x401ff4
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), ptr %st.p.99, align 1  ; 0x401ff4
  %eax.40 = trunc i64 %ret.23 to i32  ; 0x401ff9
  %sp.91 = sub i32 %sp.90, 4  ; 0x401ff9
  %st.p.100 = getelementptr inbounds i8, ptr %frame.top, i64 -56  ; 0x401ff9
  store i32 %eax.40, ptr %st.p.100, align 1  ; 0x401ff9
  %sarg0.23 = zext i32 %eax.40 to i64
  %sarg1.23 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32) to i64
  %sarg2.23 = zext i32 %addr.41 to i64
  %sarg3.23 = zext i32 %addr.43 to i64
  %ret.24 = call i64 @WriteFile(i64 %sarg0.23, i64 %sarg1.23, i64 %sarg2.23, i64 %sarg3.23, i64 0)  ; 0x401ffa
  %sp.92 = add i32 %sp.91, 20  ; 0x401ffa
  store i32 0, ptr %m.p.3, align 1  ; 0x402000
  %m.18 = load i32, ptr %st.p.4, align 1  ; 0x40200a
  %logic.12 = xor i32 %m.18, %sp.92  ; 0x40200e
  %eax.z.36 = zext i32 %logic.12 to i64  ; 0x40200e
  %m.19 = load i32, ptr %m.p.1, align 1  ; 0x402010
  %not.12 = icmp ne i32 %m.19, %logic.12  ; 0x402018
  br i1 %not.12, label %b_402063, label %b_40201a
b_40201a:  ; 0x40201a
  ret i64 %eax.z.36
b_402022:  ; 0x402022
  %addr.45 = add i32 %rdx.phi, 2  ; 0x402022
  store i32 %addr.45, ptr %m.p.3, align 1  ; 0x402025
  %addr.46 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 337) to i32), %rdx.phi  ; 0x40202a
  %addr.z.128 = zext i32 %addr.46 to i64  ; 0x40202a
  %st.p.103 = inttoptr i64 %addr.z.128 to ptr  ; 0x40202a
  store i8 61, ptr %st.p.103, align 1  ; 0x40202a
  %not.13 = icmp ne i32 %addr.45, 512  ; 0x402036
  br i1 %not.13, label %b_401d3d, label %b_40203c
b_40203c:  ; 0x40203c
  %sp.98 = sub i32 %rsp.phi.2, 4  ; 0x40203c
  %st.p.104 = getelementptr inbounds i8, ptr %frame.top, i64 -40  ; 0x40203c
  store i32 -11, ptr %st.p.104, align 1  ; 0x40203c
  %ret.6 = call i64 @GetStdHandle(i64 4294967285)  ; 0x40203e
  %sp.99 = add i32 %sp.98, 4  ; 0x40203e
  %addr.47 = add i32 %sp.99, 4  ; 0x402044
  %sp.100 = sub i32 %sp.99, 4  ; 0x402048
  store i32 0, ptr %st.p.104, align 1  ; 0x402048
  %sp.101 = sub i32 %sp.100, 4  ; 0x40204a
  %st.p.106 = getelementptr inbounds i8, ptr %frame.top, i64 -44  ; 0x40204a
  store i32 %addr.47, ptr %st.p.106, align 1  ; 0x40204a
  %sp.102 = sub i32 %sp.101, 4  ; 0x40204b
  %st.p.107 = getelementptr inbounds i8, ptr %frame.top, i64 -48  ; 0x40204b
  store i32 512, ptr %st.p.107, align 1  ; 0x40204b
  %sp.103 = sub i32 %sp.102, 4  ; 0x402050
  %st.p.108 = getelementptr inbounds i8, ptr %frame.top, i64 -52  ; 0x402050
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), ptr %st.p.108, align 1  ; 0x402050
  %eax.45 = trunc i64 %ret.6 to i32  ; 0x402055
  %sp.104 = sub i32 %sp.103, 4  ; 0x402055
  %st.p.109 = getelementptr inbounds i8, ptr %frame.top, i64 -56  ; 0x402055
  store i32 %eax.45, ptr %st.p.109, align 1  ; 0x402055
  %sarg0.7 = zext i32 %eax.45 to i64
  %sarg1.7 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32) to i64
  %sarg3.7 = zext i32 %addr.47 to i64
  %ret.7 = call i64 @WriteFile(i64 %sarg0.7, i64 %sarg1.7, i64 512, i64 %sarg3.7, i64 0)  ; 0x402056
  %sp.105 = add i32 %sp.104, 20  ; 0x402056
  br label %b_401d3d
b_402063:  ; 0x402063
  %m.20 = load i32, ptr %st.p.4, align 1  ; 0x402063
  %logic.13 = xor i32 %m.20, %sp.92  ; 0x402067
  %ecx.z.28 = zext i32 %logic.13 to i64  ; 0x402067
  call void @F_00402570(i64 %ecx.z.28)  ; 0x402069
  %eh.pending.1 = call i64 @ra_eh_pending()  ; 0x402069
  %eh.thrown.1 = icmp ne i64 %eh.pending.1, 0  ; 0x402069
  br i1 %eh.thrown.1, label %eh.landing, label %eh.ok.402069
eh.landing:  ; 0x401f21
  %rax.phi.10 = phi i64 [ %ret.18, %b_401f0f ], [ %eax.z.36, %b_402063 ]  ; 0x401f21
  ret i64 %rax.phi.10
eh.ok.401f21:  ; 0x401f21
  %m.16 = load i32, ptr %m.p.3, align 1  ; 0x401f26
  %not.10 = icmp ne i32 %m.16, 512  ; 0x401f32
  br i1 %not.10, label %b_401f62, label %b_401f34
eh.ok.402069:  ; 0x402069
  call void @ra_trap(i64 4202606, i64 1)  ; 0x40206e
  call void @ra_trap(i64 4202606, i64 1)  ; 0x40206e
  unreachable
}

define i64 @F_00402070(i32 %arg0) {  ; 0x402070
b_402070:  ; 0x402070
  %frame = alloca [168 x i8], align 16
  %frame.top = getelementptr inbounds i8, ptr %frame, i64 92
  %rsp0 = ptrtoint ptr %frame.top to i64
  %ebp = trunc i64 undef to i32  ; 0x402070
  %esp = trunc i64 %rsp0 to i32  ; 0x402070
  %sp = sub i32 %esp, 4  ; 0x402070
  %st.p = getelementptr inbounds i8, ptr %frame.top, i64 -4  ; 0x402070
  store i32 %ebp, ptr %st.p, align 1  ; 0x402070
  %sp.1 = sub i32 %sp, 4  ; 0x402071
  %st.p.1 = getelementptr inbounds i8, ptr %frame.top, i64 -8  ; 0x402071
  store i32 %ebp, ptr %st.p.1, align 1  ; 0x402071
  %sp.2 = sub i32 %sp.1, 4  ; 0x402072
  %st.p.2 = getelementptr inbounds i8, ptr %frame.top, i64 -12  ; 0x402072
  store i32 %ebp, ptr %st.p.2, align 1  ; 0x402072
  %sp.3 = sub i32 %sp.2, 4  ; 0x402073
  %st.p.3 = getelementptr inbounds i8, ptr %frame.top, i64 -16  ; 0x402073
  store i32 %ebp, ptr %st.p.3, align 1  ; 0x402073
  %diff = sub i32 %sp.3, 20  ; 0x402074
  %esp.z.4 = zext i32 %diff to i64  ; 0x402074
  %addr.z.4 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 192) to i32) to i64  ; 0x402077
  %m.p = inttoptr i64 %addr.z.4 to ptr  ; 0x402077
  %m = load i32, ptr %m.p, align 1  ; 0x402077
  %logic = xor i32 %m, %diff  ; 0x40207c
  %st.p.4 = getelementptr inbounds i8, ptr %frame.top, i64 -20  ; 0x40207e
  store i32 %logic, ptr %st.p.4, align 1  ; 0x40207e
  %st.p.5 = getelementptr inbounds i8, ptr %frame.top, i64 -31  ; 0x402082
  store i16 30768, ptr %st.p.5, align 1  ; 0x402082
  %addr.2 = add i32 %diff, 6  ; 0x402089
  %st.p.6 = getelementptr inbounds i8, ptr %frame.top, i64 -21  ; 0x40208d
  store i8 0, ptr %st.p.6, align 1  ; 0x40208d
  %logic.1 = and i32 %arg0, 15  ; 0x402094
  %addr.4 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 727) to i32), %logic.1  ; 0x402097
  %addr.z.8 = zext i32 %addr.4 to i64  ; 0x402097
  %m.p.1 = inttoptr i64 %addr.z.8 to ptr  ; 0x402097
  %m.1 = load i8, ptr %m.p.1, align 1  ; 0x402097
  %st.p.7 = getelementptr inbounds i8, ptr %frame.top, i64 -22  ; 0x40209e
  store i8 %m.1, ptr %st.p.7, align 1  ; 0x40209e
  %shifted.w = lshr i32 %arg0, 4  ; 0x4020a4
  %logic.2 = and i32 %shifted.w, 15  ; 0x4020a7
  %addr.6 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 727) to i32), %logic.2  ; 0x4020aa
  %addr.z.10 = zext i32 %addr.6 to i64  ; 0x4020aa
  %m.p.2 = inttoptr i64 %addr.z.10 to ptr  ; 0x4020aa
  %m.2 = load i8, ptr %m.p.2, align 1  ; 0x4020aa
  %st.p.8 = getelementptr inbounds i8, ptr %frame.top, i64 -23  ; 0x4020b1
  store i8 %m.2, ptr %st.p.8, align 1  ; 0x4020b1
  %shifted.w.1 = lshr i32 %arg0, 8  ; 0x4020b7
  %logic.3 = and i32 %shifted.w.1, 15  ; 0x4020ba
  %addr.8 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 727) to i32), %logic.3  ; 0x4020bd
  %addr.z.12 = zext i32 %addr.8 to i64  ; 0x4020bd
  %m.p.3 = inttoptr i64 %addr.z.12 to ptr  ; 0x4020bd
  %m.3 = load i8, ptr %m.p.3, align 1  ; 0x4020bd
  %st.p.9 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x4020c4
  store i8 %m.3, ptr %st.p.9, align 1  ; 0x4020c4
  %shifted.w.2 = lshr i32 %arg0, 12  ; 0x4020ca
  %logic.4 = and i32 %shifted.w.2, 15  ; 0x4020cd
  %addr.10 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 727) to i32), %logic.4  ; 0x4020d0
  %addr.z.14 = zext i32 %addr.10 to i64  ; 0x4020d0
  %m.p.4 = inttoptr i64 %addr.z.14 to ptr  ; 0x4020d0
  %m.4 = load i8, ptr %m.p.4, align 1  ; 0x4020d0
  %st.p.10 = getelementptr inbounds i8, ptr %frame.top, i64 -25  ; 0x4020d7
  store i8 %m.4, ptr %st.p.10, align 1  ; 0x4020d7
  %shifted.w.3 = lshr i32 %arg0, 16  ; 0x4020dd
  %logic.5 = and i32 %shifted.w.3, 15  ; 0x4020e0
  %addr.12 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 727) to i32), %logic.5  ; 0x4020e3
  %addr.z.16 = zext i32 %addr.12 to i64  ; 0x4020e3
  %m.p.5 = inttoptr i64 %addr.z.16 to ptr  ; 0x4020e3
  %m.5 = load i8, ptr %m.p.5, align 1  ; 0x4020e3
  %st.p.11 = getelementptr inbounds i8, ptr %frame.top, i64 -26  ; 0x4020ea
  store i8 %m.5, ptr %st.p.11, align 1  ; 0x4020ea
  %shifted.w.4 = lshr i32 %arg0, 20  ; 0x4020f0
  %logic.6 = and i32 %shifted.w.4, 15  ; 0x4020f3
  %addr.14 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 727) to i32), %logic.6  ; 0x4020f6
  %addr.z.18 = zext i32 %addr.14 to i64  ; 0x4020f6
  %m.p.6 = inttoptr i64 %addr.z.18 to ptr  ; 0x4020f6
  %m.6 = load i8, ptr %m.p.6, align 1  ; 0x4020f6
  %st.p.12 = getelementptr inbounds i8, ptr %frame.top, i64 -27  ; 0x4020fd
  store i8 %m.6, ptr %st.p.12, align 1  ; 0x4020fd
  %shifted.w.5 = lshr i32 %arg0, 24  ; 0x402103
  %logic.7 = and i32 %shifted.w.5, 15  ; 0x402106
  %addr.16 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 727) to i32), %logic.7  ; 0x402109
  %addr.z.20 = zext i32 %addr.16 to i64  ; 0x402109
  %m.p.7 = inttoptr i64 %addr.z.20 to ptr  ; 0x402109
  %m.7 = load i8, ptr %m.p.7, align 1  ; 0x402109
  %st.p.13 = getelementptr inbounds i8, ptr %frame.top, i64 -28  ; 0x402110
  store i8 %m.7, ptr %st.p.13, align 1  ; 0x402110
  %shifted.w.6 = lshr i32 %arg0, 28  ; 0x402114
  %addr.18 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 727) to i32), %shifted.w.6  ; 0x402117
  %addr.z.22 = zext i32 %addr.18 to i64  ; 0x402117
  %m.p.8 = inttoptr i64 %addr.z.22 to ptr  ; 0x402117
  %m.8 = load i8, ptr %m.p.8, align 1  ; 0x402117
  %st.p.14 = getelementptr inbounds i8, ptr %frame.top, i64 -29  ; 0x40211e
  store i8 %m.8, ptr %st.p.14, align 1  ; 0x40211e
  %addr.z.24 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 332) to i32) to i64  ; 0x402122
  %m.p.9 = inttoptr i64 %addr.z.24 to ptr  ; 0x402122
  %m.9 = load i32, ptr %m.p.9, align 1  ; 0x402122
  br label %b_402155
b_402140:  ; 0x402140
  %rax.phi.1 = phi i8 [ %rax.phi, %b_402155 ], [ %m.13, %b_40215d ]  ; 0x402140
  %rcx.phi.1 = phi i32 [ %rcx.phi, %b_402155 ], [ 0, %b_40215d ]  ; 0x402140
  %rsp.phi.1 = phi i32 [ %rsp.phi, %b_402155 ], [ %sp.11, %b_40215d ]  ; 0x402140
  %addr.20 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), %rcx.phi.1  ; 0x402140
  %addr.z.27 = zext i32 %addr.20 to i64  ; 0x402140
  %st.p.15 = inttoptr i64 %addr.z.27 to ptr  ; 0x402140
  store i8 %rax.phi.1, ptr %st.p.15, align 1  ; 0x402140
  %sum = add i32 %rcx.phi.1, 1  ; 0x402146
  store i32 %sum, ptr %m.p.9, align 1  ; 0x402147
  %addr.z.29 = zext i32 %rsi.phi to i64  ; 0x40214d
  %m.p.12 = inttoptr i64 %addr.z.29 to ptr  ; 0x40214d
  %m.12 = load i8, ptr %m.p.12, align 1  ; 0x40214d
  %sum.1 = add i32 %rsi.phi, 1  ; 0x402150
  %zf.18 = icmp eq i8 %m.12, 0  ; 0x402151
  br i1 %zf.18, label %b_402179, label %b_402155
b_402155:  ; 0x402155
  %rax.phi = phi i8 [ 48, %b_402070 ], [ %m.12, %b_402140 ]  ; 0x402155
  %rcx.phi = phi i32 [ %m.9, %b_402070 ], [ %sum, %b_402140 ]  ; 0x402155
  %rsp.phi = phi i32 [ %diff, %b_402070 ], [ %rsp.phi.1, %b_402140 ]  ; 0x402155
  %rsi.phi = phi i32 [ %addr.2, %b_402070 ], [ %sum.1, %b_402140 ]  ; 0x402155
  %not = icmp ne i32 %rcx.phi, 512  ; 0x40215b
  br i1 %not, label %b_402140, label %b_40215d
b_40215d:  ; 0x40215d
  %sp.4 = sub i32 %rsp.phi, 4  ; 0x40215d
  %st.p.17 = getelementptr inbounds i8, ptr %frame.top, i64 -40  ; 0x40215d
  store i32 -11, ptr %st.p.17, align 1  ; 0x40215d
  %ret = call i64 @GetStdHandle(i64 4294967285)  ; 0x40215f
  %sp.5 = add i32 %sp.4, 4  ; 0x40215f
  %sp.6 = sub i32 %sp.5, 4  ; 0x402161
  store i32 0, ptr %st.p.17, align 1  ; 0x402161
  %sp.7 = sub i32 %sp.6, 4  ; 0x402163
  %st.p.19 = getelementptr inbounds i8, ptr %frame.top, i64 -44  ; 0x402163
  store i32 %diff, ptr %st.p.19, align 1  ; 0x402163
  %sp.8 = sub i32 %sp.7, 4  ; 0x402164
  %st.p.20 = getelementptr inbounds i8, ptr %frame.top, i64 -48  ; 0x402164
  store i32 512, ptr %st.p.20, align 1  ; 0x402164
  %sp.9 = sub i32 %sp.8, 4  ; 0x402169
  %st.p.21 = getelementptr inbounds i8, ptr %frame.top, i64 -52  ; 0x402169
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), ptr %st.p.21, align 1  ; 0x402169
  %eax.22 = trunc i64 %ret to i32  ; 0x40216e
  %sp.10 = sub i32 %sp.9, 4  ; 0x40216e
  %st.p.22 = getelementptr inbounds i8, ptr %frame.top, i64 -56  ; 0x40216e
  store i32 %eax.22, ptr %st.p.22, align 1  ; 0x40216e
  %sarg0.1 = zext i32 %eax.22 to i64
  %sarg1.1 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32) to i64
  %ret.1 = call i64 @WriteFile(i64 %sarg0.1, i64 %sarg1.1, i64 512, i64 %esp.z.4, i64 0)  ; 0x40216f
  %sp.11 = add i32 %sp.10, 20  ; 0x40216f
  %addr.21 = add i32 %rsi.phi, -1  ; 0x402171
  %addr.z.36 = zext i32 %addr.21 to i64  ; 0x402171
  %m.p.13 = inttoptr i64 %addr.z.36 to ptr  ; 0x402171
  %m.13 = load i8, ptr %m.p.13, align 1  ; 0x402171
  br label %b_402140
b_402179:  ; 0x402179
  %m.14 = load i32, ptr %st.p.4, align 1  ; 0x402179
  %logic.9 = xor i32 %m.14, %rsp.phi.1  ; 0x40217d
  %eax.z.33 = zext i32 %logic.9 to i64  ; 0x40217d
  %m.15 = load i32, ptr %m.p, align 1  ; 0x40217f
  %not.1 = icmp ne i32 %m.15, %logic.9  ; 0x402187
  br i1 %not.1, label %b_402191, label %b_402189
b_402189:  ; 0x402189
  ret i64 %eax.z.33
b_402191:  ; 0x402191
  %m.16 = load i32, ptr %st.p.4, align 1  ; 0x402191
  %logic.10 = xor i32 %m.16, %rsp.phi.1  ; 0x402195
  %ecx.z.6 = zext i32 %logic.10 to i64  ; 0x402195
  call void @F_00402570(i64 %ecx.z.6)  ; 0x402197
  %eh.pending = call i64 @ra_eh_pending()  ; 0x402197
  %eh.thrown = icmp ne i64 %eh.pending, 0  ; 0x402197
  br i1 %eh.thrown, label %eh.landing, label %eh.ok.402197
eh.landing:  ; 0x402197
  ret i64 %eax.z.33
eh.ok.402197:  ; 0x402197
  call void @ra_trap(i64 4202908, i64 1)  ; 0x40219c
  call void @ra_trap(i64 4202908, i64 1)  ; 0x40219c
  unreachable
}

define i64 @F_004021a0() {  ; 0x4021a0
b_4021a0:  ; 0x4021a0
  %frame = alloca [136 x i8], align 16
  %frame.top = getelementptr inbounds i8, ptr %frame, i64 60
  %rsp0 = ptrtoint ptr %frame.top to i64
  %esi = trunc i64 undef to i32  ; 0x4021a0
  %esp = trunc i64 %rsp0 to i32  ; 0x4021a0
  %sp = sub i32 %esp, 4  ; 0x4021a0
  %st.p = getelementptr inbounds i8, ptr %frame.top, i64 -4  ; 0x4021a0
  store i32 %esi, ptr %st.p, align 1  ; 0x4021a0
  %diff = sub i32 %sp, 8  ; 0x4021a1
  %addr.z.1 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 192) to i32) to i64  ; 0x4021a4
  %m.p = inttoptr i64 %addr.z.1 to ptr  ; 0x4021a4
  %m = load i32, ptr %m.p, align 1  ; 0x4021a4
  %logic = xor i32 %m, %diff  ; 0x4021a9
  %st.p.1 = getelementptr inbounds i8, ptr %frame.top, i64 -8  ; 0x4021ab
  store i32 %logic, ptr %st.p.1, align 1  ; 0x4021ab
  %sp.1 = sub i32 %diff, 4  ; 0x4021af
  %ret = call i64 @F_00401970()  ; 0x4021af
  %rsp.131 = add i32 %sp.1, 4  ; 0x4021af
  %eh.pending = call i64 @ra_eh_pending()  ; 0x4021af
  %eh.thrown = icmp ne i64 %eh.pending, 0  ; 0x4021af
  br i1 %eh.thrown, label %eh.landing, label %eh.ok.4021af
b_4021c2:  ; 0x4021c2
  %sp.2 = sub i32 %rsp.131, 4  ; 0x4021c2
  %st.p.3 = getelementptr inbounds i8, ptr %frame.top, i64 -16  ; 0x4021c2
  store i32 -11, ptr %st.p.3, align 1  ; 0x4021c2
  %ret.1 = call i64 @GetStdHandle(i64 4294967285)  ; 0x4021c4
  %sp.3 = add i32 %sp.2, 4  ; 0x4021c4
  %sp.4 = sub i32 %sp.3, 4  ; 0x4021cc
  store i32 0, ptr %st.p.3, align 1  ; 0x4021cc
  %sp.5 = sub i32 %sp.4, 4  ; 0x4021ce
  %st.p.5 = getelementptr inbounds i8, ptr %frame.top, i64 -20  ; 0x4021ce
  store i32 %sp.3, ptr %st.p.5, align 1  ; 0x4021ce
  %sp.6 = sub i32 %sp.5, 4  ; 0x4021cf
  %st.p.6 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x4021cf
  store i32 512, ptr %st.p.6, align 1  ; 0x4021cf
  %sp.7 = sub i32 %sp.6, 4  ; 0x4021d4
  %st.p.7 = getelementptr inbounds i8, ptr %frame.top, i64 -28  ; 0x4021d4
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), ptr %st.p.7, align 1  ; 0x4021d4
  %eax.2 = trunc i64 %ret.1 to i32  ; 0x4021d9
  %sp.8 = sub i32 %sp.7, 4  ; 0x4021d9
  %st.p.8 = getelementptr inbounds i8, ptr %frame.top, i64 -32  ; 0x4021d9
  store i32 %eax.2, ptr %st.p.8, align 1  ; 0x4021d9
  %sarg0.1 = zext i32 %eax.2 to i64
  %sarg1.1 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32) to i64
  %sarg3.1 = zext i32 %sp.3 to i64
  %ret.2 = call i64 @WriteFile(i64 %sarg0.1, i64 %sarg1.1, i64 512, i64 %sarg3.1, i64 0)  ; 0x4021da
  %sp.9 = add i32 %sp.8, 20  ; 0x4021da
  %st.p.9 = inttoptr i64 %sarg1.1 to ptr  ; 0x4021e0
  store i8 99, ptr %st.p.9, align 1  ; 0x4021e0
  br label %b_402228
b_4021ee:  ; 0x4021ee
  %addr.1 = add i32 %m.1, 1  ; 0x4021ee
  store i32 %addr.1, ptr %m.p.1, align 1  ; 0x4021f1
  %addr.2 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), %m.1  ; 0x4021f6
  %addr.z.13 = zext i32 %addr.2 to i64  ; 0x4021f6
  %st.p.11 = inttoptr i64 %addr.z.13 to ptr  ; 0x4021f6
  store i8 99, ptr %st.p.11, align 1  ; 0x4021f6
  %not.1 = icmp ne i32 %addr.1, 512  ; 0x402202
  br i1 %not.1, label %b_4022b4, label %b_402208
b_402208:  ; 0x402208
  %sp.10 = sub i32 %rsp.131, 4  ; 0x402208
  %st.p.12 = getelementptr inbounds i8, ptr %frame.top, i64 -16  ; 0x402208
  store i32 -11, ptr %st.p.12, align 1  ; 0x402208
  %ret.3 = call i64 @GetStdHandle(i64 4294967285)  ; 0x40220a
  %sp.11 = add i32 %sp.10, 4  ; 0x40220a
  %sp.12 = sub i32 %sp.11, 4  ; 0x402212
  store i32 0, ptr %st.p.12, align 1  ; 0x402212
  %sp.13 = sub i32 %sp.12, 4  ; 0x402214
  %st.p.14 = getelementptr inbounds i8, ptr %frame.top, i64 -20  ; 0x402214
  store i32 %sp.11, ptr %st.p.14, align 1  ; 0x402214
  %sp.14 = sub i32 %sp.13, 4  ; 0x402215
  %st.p.15 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x402215
  store i32 512, ptr %st.p.15, align 1  ; 0x402215
  %sp.15 = sub i32 %sp.14, 4  ; 0x40221a
  %st.p.16 = getelementptr inbounds i8, ptr %frame.top, i64 -28  ; 0x40221a
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), ptr %st.p.16, align 1  ; 0x40221a
  %eax.5 = trunc i64 %ret.3 to i32  ; 0x40221f
  %sp.16 = sub i32 %sp.15, 4  ; 0x40221f
  %st.p.17 = getelementptr inbounds i8, ptr %frame.top, i64 -32  ; 0x40221f
  store i32 %eax.5, ptr %st.p.17, align 1  ; 0x40221f
  %sarg0.3 = zext i32 %eax.5 to i64
  %sarg1.3 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32) to i64
  %sarg3.3 = zext i32 %sp.11 to i64
  %ret.4 = call i64 @WriteFile(i64 %sarg0.3, i64 %sarg1.3, i64 512, i64 %sarg3.3, i64 0)  ; 0x402220
  %sp.17 = add i32 %sp.16, 20  ; 0x402220
  br label %b_402228
b_402228:  ; 0x402228
  %rax.phi = phi i32 [ 1, %b_4021c2 ], [ 0, %b_402208 ]  ; 0x402228
  %rsp.phi = phi i32 [ %sp.9, %b_4021c2 ], [ %sp.17, %b_402208 ]  ; 0x402228
  %addr.3 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), %rax.phi  ; 0x402228
  %addr.z.20 = zext i32 %addr.3 to i64  ; 0x402228
  %st.p.18 = inttoptr i64 %addr.z.20 to ptr  ; 0x402228
  store i8 104, ptr %st.p.18, align 1  ; 0x402228
  %sum = add i32 %rax.phi, 1  ; 0x40222f
  br label %b_402230
b_402230:  ; 0x402230
  %rax.phi.1 = phi i32 [ %sum, %b_402228 ], [ %addr.8, %b_4022b4 ]  ; 0x402230
  %rsp.phi.1 = phi i32 [ %rsp.phi, %b_402228 ], [ %rsp.131, %b_4022b4 ]  ; 0x402230
  %addr.4 = add i32 %rax.phi.1, 1  ; 0x402230
  store i32 %addr.4, ptr %m.p.1, align 1  ; 0x402233
  %addr.5 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), %rax.phi.1  ; 0x402239
  %addr.z.22 = zext i32 %addr.5 to i64  ; 0x402239
  %st.p.20 = inttoptr i64 %addr.z.22 to ptr  ; 0x402239
  store i8 101, ptr %st.p.20, align 1  ; 0x402239
  %not.2 = icmp ne i32 %addr.4, 512  ; 0x402246
  br i1 %not.2, label %b_40226d, label %b_402248
b_402248:  ; 0x402248
  %sp.18 = sub i32 %rsp.phi.1, 4  ; 0x402248
  %st.p.21 = getelementptr inbounds i8, ptr %frame.top, i64 -16  ; 0x402248
  store i32 -11, ptr %st.p.21, align 1  ; 0x402248
  %ret.7 = call i64 @GetStdHandle(i64 4294967285)  ; 0x40224a
  %sp.19 = add i32 %sp.18, 4  ; 0x40224a
  %sp.20 = sub i32 %sp.19, 4  ; 0x402252
  store i32 0, ptr %st.p.21, align 1  ; 0x402252
  %sp.21 = sub i32 %sp.20, 4  ; 0x402254
  %st.p.23 = getelementptr inbounds i8, ptr %frame.top, i64 -20  ; 0x402254
  store i32 %sp.19, ptr %st.p.23, align 1  ; 0x402254
  %sp.22 = sub i32 %sp.21, 4  ; 0x402255
  %st.p.24 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x402255
  store i32 512, ptr %st.p.24, align 1  ; 0x402255
  %sp.23 = sub i32 %sp.22, 4  ; 0x40225a
  %st.p.25 = getelementptr inbounds i8, ptr %frame.top, i64 -28  ; 0x40225a
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), ptr %st.p.25, align 1  ; 0x40225a
  %eax.10 = trunc i64 %ret.7 to i32  ; 0x40225f
  %sp.24 = sub i32 %sp.23, 4  ; 0x40225f
  %st.p.26 = getelementptr inbounds i8, ptr %frame.top, i64 -32  ; 0x40225f
  store i32 %eax.10, ptr %st.p.26, align 1  ; 0x40225f
  %sarg0.7 = zext i32 %eax.10 to i64
  %sarg1.7 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32) to i64
  %sarg3.7 = zext i32 %sp.19 to i64
  %ret.8 = call i64 @WriteFile(i64 %sarg0.7, i64 %sarg1.7, i64 512, i64 %sarg3.7, i64 0)  ; 0x402260
  %sp.25 = add i32 %sp.24, 20  ; 0x402260
  br label %b_4022f8
b_40226d:  ; 0x40226d
  %addr.6 = add i32 %rax.phi.1, 2  ; 0x40226d
  store i32 %addr.6, ptr %m.p.1, align 1  ; 0x402270
  %addr.7 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 337) to i32), %rax.phi.1  ; 0x402276
  %addr.z.30 = zext i32 %addr.7 to i64  ; 0x402276
  %st.p.28 = inttoptr i64 %addr.z.30 to ptr  ; 0x402276
  store i8 99, ptr %st.p.28, align 1  ; 0x402276
  %not.3 = icmp ne i32 %addr.6, 512  ; 0x402283
  br i1 %not.3, label %b_402300, label %b_402285
b_402285:  ; 0x402285
  %sp.26 = sub i32 %rsp.phi.1, 4  ; 0x402285
  %st.p.29 = getelementptr inbounds i8, ptr %frame.top, i64 -16  ; 0x402285
  store i32 -11, ptr %st.p.29, align 1  ; 0x402285
  %ret.9 = call i64 @GetStdHandle(i64 4294967285)  ; 0x402287
  %sp.27 = add i32 %sp.26, 4  ; 0x402287
  %sp.28 = sub i32 %sp.27, 4  ; 0x40228f
  store i32 0, ptr %st.p.29, align 1  ; 0x40228f
  %sp.29 = sub i32 %sp.28, 4  ; 0x402291
  %st.p.31 = getelementptr inbounds i8, ptr %frame.top, i64 -20  ; 0x402291
  store i32 %sp.27, ptr %st.p.31, align 1  ; 0x402291
  %sp.30 = sub i32 %sp.29, 4  ; 0x402292
  %st.p.32 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x402292
  store i32 512, ptr %st.p.32, align 1  ; 0x402292
  %sp.31 = sub i32 %sp.30, 4  ; 0x402297
  %st.p.33 = getelementptr inbounds i8, ptr %frame.top, i64 -28  ; 0x402297
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), ptr %st.p.33, align 1  ; 0x402297
  %eax.13 = trunc i64 %ret.9 to i32  ; 0x40229c
  %sp.32 = sub i32 %sp.31, 4  ; 0x40229c
  %st.p.34 = getelementptr inbounds i8, ptr %frame.top, i64 -32  ; 0x40229c
  store i32 %eax.13, ptr %st.p.34, align 1  ; 0x40229c
  %sarg0.9 = zext i32 %eax.13 to i64
  %sarg1.9 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32) to i64
  %sarg3.9 = zext i32 %sp.27 to i64
  %ret.10 = call i64 @WriteFile(i64 %sarg0.9, i64 %sarg1.9, i64 512, i64 %sarg3.9, i64 0)  ; 0x40229d
  %sp.33 = add i32 %sp.32, 20  ; 0x40229d
  %st.p.35 = inttoptr i64 %sarg1.9 to ptr  ; 0x4022a3
  store i8 107, ptr %st.p.35, align 1  ; 0x4022a3
  br label %b_402336
b_4022b4:  ; 0x4022b4
  %addr.8 = add i32 %m.1, 2  ; 0x4022b4
  store i32 %addr.8, ptr %m.p.1, align 1  ; 0x4022b7
  %addr.9 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 337) to i32), %m.1  ; 0x4022bc
  %addr.z.39 = zext i32 %addr.9 to i64  ; 0x4022bc
  %st.p.37 = inttoptr i64 %addr.z.39 to ptr  ; 0x4022bc
  store i8 104, ptr %st.p.37, align 1  ; 0x4022bc
  %not.4 = icmp ne i32 %addr.8, 512  ; 0x4022c8
  br i1 %not.4, label %b_402230, label %b_4022ce
b_4022ce:  ; 0x4022ce
  %sp.34 = sub i32 %rsp.131, 4  ; 0x4022ce
  %st.p.38 = getelementptr inbounds i8, ptr %frame.top, i64 -16  ; 0x4022ce
  store i32 -11, ptr %st.p.38, align 1  ; 0x4022ce
  %ret.5 = call i64 @GetStdHandle(i64 4294967285)  ; 0x4022d0
  %sp.35 = add i32 %sp.34, 4  ; 0x4022d0
  %sp.36 = sub i32 %sp.35, 4  ; 0x4022d8
  store i32 0, ptr %st.p.38, align 1  ; 0x4022d8
  %sp.37 = sub i32 %sp.36, 4  ; 0x4022da
  %st.p.40 = getelementptr inbounds i8, ptr %frame.top, i64 -20  ; 0x4022da
  store i32 %sp.35, ptr %st.p.40, align 1  ; 0x4022da
  %sp.38 = sub i32 %sp.37, 4  ; 0x4022db
  %st.p.41 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x4022db
  store i32 512, ptr %st.p.41, align 1  ; 0x4022db
  %sp.39 = sub i32 %sp.38, 4  ; 0x4022e0
  %st.p.42 = getelementptr inbounds i8, ptr %frame.top, i64 -28  ; 0x4022e0
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), ptr %st.p.42, align 1  ; 0x4022e0
  %eax.16 = trunc i64 %ret.5 to i32  ; 0x4022e5
  %sp.40 = sub i32 %sp.39, 4  ; 0x4022e5
  %st.p.43 = getelementptr inbounds i8, ptr %frame.top, i64 -32  ; 0x4022e5
  store i32 %eax.16, ptr %st.p.43, align 1  ; 0x4022e5
  %sarg0.5 = zext i32 %eax.16 to i64
  %sarg1.5 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32) to i64
  %sarg3.5 = zext i32 %sp.35 to i64
  %ret.6 = call i64 @WriteFile(i64 %sarg0.5, i64 %sarg1.5, i64 512, i64 %sarg3.5, i64 0)  ; 0x4022e6
  %sp.41 = add i32 %sp.40, 20  ; 0x4022e6
  %st.p.44 = inttoptr i64 %sarg1.5 to ptr  ; 0x4022ec
  store i8 101, ptr %st.p.44, align 1  ; 0x4022ec
  br label %b_4022f8
b_4022f8:  ; 0x4022f8
  %rcx.phi = phi i32 [ 0, %b_402248 ], [ 1, %b_4022ce ]  ; 0x4022f8
  %rsp.phi.2 = phi i32 [ %sp.25, %b_402248 ], [ %sp.41, %b_4022ce ]  ; 0x4022f8
  %addr.10 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), %rcx.phi  ; 0x4022f8
  %addr.z.47 = zext i32 %addr.10 to i64  ; 0x4022f8
  %st.p.45 = inttoptr i64 %addr.z.47 to ptr  ; 0x4022f8
  store i8 99, ptr %st.p.45, align 1  ; 0x4022f8
  %sum.1 = add i32 %rcx.phi, 1  ; 0x4022ff
  br label %b_402300
b_402300:  ; 0x402300
  %rcx.phi.1 = phi i32 [ %addr.6, %b_40226d ], [ %sum.1, %b_4022f8 ]  ; 0x402300
  %rsp.phi.3 = phi i32 [ %rsp.phi.1, %b_40226d ], [ %rsp.phi.2, %b_4022f8 ]  ; 0x402300
  %addr.11 = add i32 %rcx.phi.1, 1  ; 0x402300
  store i32 %addr.11, ptr %m.p.1, align 1  ; 0x402303
  %addr.12 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), %rcx.phi.1  ; 0x402308
  %addr.z.49 = zext i32 %addr.12 to i64  ; 0x402308
  %st.p.47 = inttoptr i64 %addr.z.49 to ptr  ; 0x402308
  store i8 107, ptr %st.p.47, align 1  ; 0x402308
  %not.5 = icmp ne i32 %addr.11, 512  ; 0x402314
  br i1 %not.5, label %b_402340, label %b_402316
b_402316:  ; 0x402316
  %sp.42 = sub i32 %rsp.phi.3, 4  ; 0x402316
  %st.p.48 = getelementptr inbounds i8, ptr %frame.top, i64 -16  ; 0x402316
  store i32 -11, ptr %st.p.48, align 1  ; 0x402316
  %ret.11 = call i64 @GetStdHandle(i64 4294967285)  ; 0x402318
  %sp.43 = add i32 %sp.42, 4  ; 0x402318
  %sp.44 = sub i32 %sp.43, 4  ; 0x402320
  store i32 0, ptr %st.p.48, align 1  ; 0x402320
  %sp.45 = sub i32 %sp.44, 4  ; 0x402322
  %st.p.50 = getelementptr inbounds i8, ptr %frame.top, i64 -20  ; 0x402322
  store i32 %sp.43, ptr %st.p.50, align 1  ; 0x402322
  %sp.46 = sub i32 %sp.45, 4  ; 0x402323
  %st.p.51 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x402323
  store i32 512, ptr %st.p.51, align 1  ; 0x402323
  %sp.47 = sub i32 %sp.46, 4  ; 0x402328
  %st.p.52 = getelementptr inbounds i8, ptr %frame.top, i64 -28  ; 0x402328
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), ptr %st.p.52, align 1  ; 0x402328
  %eax.19 = trunc i64 %ret.11 to i32  ; 0x40232d
  %sp.48 = sub i32 %sp.47, 4  ; 0x40232d
  %st.p.53 = getelementptr inbounds i8, ptr %frame.top, i64 -32  ; 0x40232d
  store i32 %eax.19, ptr %st.p.53, align 1  ; 0x40232d
  %sarg0.11 = zext i32 %eax.19 to i64
  %sarg1.11 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32) to i64
  %sarg3.11 = zext i32 %sp.43 to i64
  %ret.12 = call i64 @WriteFile(i64 %sarg0.11, i64 %sarg1.11, i64 512, i64 %sarg3.11, i64 0)  ; 0x40232e
  %sp.49 = add i32 %sp.48, 20  ; 0x40232e
  br label %b_402336
b_402336:  ; 0x402336
  %rax.phi.2 = phi i32 [ 1, %b_402285 ], [ 0, %b_402316 ]  ; 0x402336
  %rsp.phi.4 = phi i32 [ %sp.33, %b_402285 ], [ %sp.49, %b_402316 ]  ; 0x402336
  %addr.13 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), %rax.phi.2  ; 0x402336
  %addr.z.56 = zext i32 %addr.13 to i64  ; 0x402336
  %st.p.54 = inttoptr i64 %addr.z.56 to ptr  ; 0x402336
  store i8 115, ptr %st.p.54, align 1  ; 0x402336
  %sum.2 = add i32 %rax.phi.2, 1  ; 0x40233d
  br label %b_402382
b_402340:  ; 0x402340
  %addr.14 = add i32 %rcx.phi.1, 2  ; 0x402340
  store i32 %addr.14, ptr %m.p.1, align 1  ; 0x402343
  %addr.15 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 337) to i32), %rcx.phi.1  ; 0x402348
  %addr.z.58 = zext i32 %addr.15 to i64  ; 0x402348
  %st.p.56 = inttoptr i64 %addr.z.58 to ptr  ; 0x402348
  store i8 115, ptr %st.p.56, align 1  ; 0x402348
  %not.6 = icmp ne i32 %addr.14, 512  ; 0x402354
  br i1 %not.6, label %b_402382, label %b_402356
b_402356:  ; 0x402356
  %sp.50 = sub i32 %rsp.phi.3, 4  ; 0x402356
  %st.p.57 = getelementptr inbounds i8, ptr %frame.top, i64 -16  ; 0x402356
  store i32 -11, ptr %st.p.57, align 1  ; 0x402356
  %ret.13 = call i64 @GetStdHandle(i64 4294967285)  ; 0x402358
  %sp.51 = add i32 %sp.50, 4  ; 0x402358
  %sp.52 = sub i32 %sp.51, 4  ; 0x402360
  store i32 0, ptr %st.p.57, align 1  ; 0x402360
  %sp.53 = sub i32 %sp.52, 4  ; 0x402362
  %st.p.59 = getelementptr inbounds i8, ptr %frame.top, i64 -20  ; 0x402362
  store i32 %sp.51, ptr %st.p.59, align 1  ; 0x402362
  %sp.54 = sub i32 %sp.53, 4  ; 0x402363
  %st.p.60 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x402363
  store i32 512, ptr %st.p.60, align 1  ; 0x402363
  %sp.55 = sub i32 %sp.54, 4  ; 0x402368
  %st.p.61 = getelementptr inbounds i8, ptr %frame.top, i64 -28  ; 0x402368
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), ptr %st.p.61, align 1  ; 0x402368
  %eax.24 = trunc i64 %ret.13 to i32  ; 0x40236d
  %sp.56 = sub i32 %sp.55, 4  ; 0x40236d
  %st.p.62 = getelementptr inbounds i8, ptr %frame.top, i64 -32  ; 0x40236d
  store i32 %eax.24, ptr %st.p.62, align 1  ; 0x40236d
  %sarg0.13 = zext i32 %eax.24 to i64
  %sarg1.13 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32) to i64
  %sarg3.13 = zext i32 %sp.51 to i64
  %ret.14 = call i64 @WriteFile(i64 %sarg0.13, i64 %sarg1.13, i64 512, i64 %sarg3.13, i64 0)  ; 0x40236e
  %sp.57 = add i32 %sp.56, 20  ; 0x40236e
  %st.p.63 = inttoptr i64 %sarg1.13 to ptr  ; 0x402374
  store i8 117, ptr %st.p.63, align 1  ; 0x402374
  br label %b_4023ba
b_402382:  ; 0x402382
  %rax.phi.3 = phi i32 [ %sum.2, %b_402336 ], [ %addr.14, %b_402340 ]  ; 0x402382
  %rsp.phi.5 = phi i32 [ %rsp.phi.4, %b_402336 ], [ %rsp.phi.3, %b_402340 ]  ; 0x402382
  %addr.16 = add i32 %rax.phi.3, 1  ; 0x402382
  store i32 %addr.16, ptr %m.p.1, align 1  ; 0x402385
  %addr.17 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), %rax.phi.3  ; 0x40238b
  %addr.z.67 = zext i32 %addr.17 to i64  ; 0x40238b
  %st.p.65 = inttoptr i64 %addr.z.67 to ptr  ; 0x40238b
  store i8 117, ptr %st.p.65, align 1  ; 0x40238b
  %not.7 = icmp ne i32 %addr.16, 512  ; 0x402398
  br i1 %not.7, label %b_4023c4, label %b_40239a
b_40239a:  ; 0x40239a
  %sp.58 = sub i32 %rsp.phi.5, 4  ; 0x40239a
  %st.p.66 = getelementptr inbounds i8, ptr %frame.top, i64 -16  ; 0x40239a
  store i32 -11, ptr %st.p.66, align 1  ; 0x40239a
  %ret.15 = call i64 @GetStdHandle(i64 4294967285)  ; 0x40239c
  %sp.59 = add i32 %sp.58, 4  ; 0x40239c
  %sp.60 = sub i32 %sp.59, 4  ; 0x4023a4
  store i32 0, ptr %st.p.66, align 1  ; 0x4023a4
  %sp.61 = sub i32 %sp.60, 4  ; 0x4023a6
  %st.p.68 = getelementptr inbounds i8, ptr %frame.top, i64 -20  ; 0x4023a6
  store i32 %sp.59, ptr %st.p.68, align 1  ; 0x4023a6
  %sp.62 = sub i32 %sp.61, 4  ; 0x4023a7
  %st.p.69 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x4023a7
  store i32 512, ptr %st.p.69, align 1  ; 0x4023a7
  %sp.63 = sub i32 %sp.62, 4  ; 0x4023ac
  %st.p.70 = getelementptr inbounds i8, ptr %frame.top, i64 -28  ; 0x4023ac
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), ptr %st.p.70, align 1  ; 0x4023ac
  %eax.27 = trunc i64 %ret.15 to i32  ; 0x4023b1
  %sp.64 = sub i32 %sp.63, 4  ; 0x4023b1
  %st.p.71 = getelementptr inbounds i8, ptr %frame.top, i64 -32  ; 0x4023b1
  store i32 %eax.27, ptr %st.p.71, align 1  ; 0x4023b1
  %sarg0.15 = zext i32 %eax.27 to i64
  %sarg1.15 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32) to i64
  %sarg3.15 = zext i32 %sp.59 to i64
  %ret.16 = call i64 @WriteFile(i64 %sarg0.15, i64 %sarg1.15, i64 512, i64 %sarg3.15, i64 0)  ; 0x4023b2
  %sp.65 = add i32 %sp.64, 20  ; 0x4023b2
  br label %b_4023ba
b_4023ba:  ; 0x4023ba
  %rcx.phi.2 = phi i32 [ 1, %b_402356 ], [ 0, %b_40239a ]  ; 0x4023ba
  %rsp.phi.6 = phi i32 [ %sp.57, %b_402356 ], [ %sp.65, %b_40239a ]  ; 0x4023ba
  %addr.18 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), %rcx.phi.2  ; 0x4023ba
  %addr.z.74 = zext i32 %addr.18 to i64  ; 0x4023ba
  %st.p.72 = inttoptr i64 %addr.z.74 to ptr  ; 0x4023ba
  store i8 109, ptr %st.p.72, align 1  ; 0x4023ba
  %sum.3 = add i32 %rcx.phi.2, 1  ; 0x4023c1
  br label %b_402408
b_4023c4:  ; 0x4023c4
  %addr.19 = add i32 %rax.phi.3, 2  ; 0x4023c4
  store i32 %addr.19, ptr %m.p.1, align 1  ; 0x4023c7
  %addr.20 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 337) to i32), %rax.phi.3  ; 0x4023cd
  %addr.z.76 = zext i32 %addr.20 to i64  ; 0x4023cd
  %st.p.74 = inttoptr i64 %addr.z.76 to ptr  ; 0x4023cd
  store i8 109, ptr %st.p.74, align 1  ; 0x4023cd
  %not.8 = icmp ne i32 %addr.19, 512  ; 0x4023da
  br i1 %not.8, label %b_402408, label %b_4023dc
b_4023dc:  ; 0x4023dc
  %sp.66 = sub i32 %rsp.phi.5, 4  ; 0x4023dc
  %st.p.75 = getelementptr inbounds i8, ptr %frame.top, i64 -16  ; 0x4023dc
  store i32 -11, ptr %st.p.75, align 1  ; 0x4023dc
  %ret.17 = call i64 @GetStdHandle(i64 4294967285)  ; 0x4023de
  %sp.67 = add i32 %sp.66, 4  ; 0x4023de
  %sp.68 = sub i32 %sp.67, 4  ; 0x4023e6
  store i32 0, ptr %st.p.75, align 1  ; 0x4023e6
  %sp.69 = sub i32 %sp.68, 4  ; 0x4023e8
  %st.p.77 = getelementptr inbounds i8, ptr %frame.top, i64 -20  ; 0x4023e8
  store i32 %sp.67, ptr %st.p.77, align 1  ; 0x4023e8
  %sp.70 = sub i32 %sp.69, 4  ; 0x4023e9
  %st.p.78 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x4023e9
  store i32 512, ptr %st.p.78, align 1  ; 0x4023e9
  %sp.71 = sub i32 %sp.70, 4  ; 0x4023ee
  %st.p.79 = getelementptr inbounds i8, ptr %frame.top, i64 -28  ; 0x4023ee
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), ptr %st.p.79, align 1  ; 0x4023ee
  %eax.30 = trunc i64 %ret.17 to i32  ; 0x4023f3
  %sp.72 = sub i32 %sp.71, 4  ; 0x4023f3
  %st.p.80 = getelementptr inbounds i8, ptr %frame.top, i64 -32  ; 0x4023f3
  store i32 %eax.30, ptr %st.p.80, align 1  ; 0x4023f3
  %sarg0.17 = zext i32 %eax.30 to i64
  %sarg1.17 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32) to i64
  %sarg3.17 = zext i32 %sp.67 to i64
  %ret.18 = call i64 @WriteFile(i64 %sarg0.17, i64 %sarg1.17, i64 512, i64 %sarg3.17, i64 0)  ; 0x4023f4
  %sp.73 = add i32 %sp.72, 20  ; 0x4023f4
  %st.p.81 = inttoptr i64 %sarg1.17 to ptr  ; 0x4023fa
  store i8 32, ptr %st.p.81, align 1  ; 0x4023fa
  br label %b_40243e
b_402408:  ; 0x402408
  %rcx.phi.3 = phi i32 [ %sum.3, %b_4023ba ], [ %addr.19, %b_4023c4 ]  ; 0x402408
  %rsp.phi.7 = phi i32 [ %rsp.phi.6, %b_4023ba ], [ %rsp.phi.5, %b_4023c4 ]  ; 0x402408
  %addr.21 = add i32 %rcx.phi.3, 1  ; 0x402408
  store i32 %addr.21, ptr %m.p.1, align 1  ; 0x40240b
  %addr.22 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), %rcx.phi.3  ; 0x402410
  %addr.z.85 = zext i32 %addr.22 to i64  ; 0x402410
  %st.p.83 = inttoptr i64 %addr.z.85 to ptr  ; 0x402410
  store i8 32, ptr %st.p.83, align 1  ; 0x402410
  %not.9 = icmp ne i32 %addr.21, 512  ; 0x40241c
  br i1 %not.9, label %b_402448, label %b_40241e
b_40241e:  ; 0x40241e
  %sp.74 = sub i32 %rsp.phi.7, 4  ; 0x40241e
  %st.p.84 = getelementptr inbounds i8, ptr %frame.top, i64 -16  ; 0x40241e
  store i32 -11, ptr %st.p.84, align 1  ; 0x40241e
  %ret.19 = call i64 @GetStdHandle(i64 4294967285)  ; 0x402420
  %sp.75 = add i32 %sp.74, 4  ; 0x402420
  %sp.76 = sub i32 %sp.75, 4  ; 0x402428
  store i32 0, ptr %st.p.84, align 1  ; 0x402428
  %sp.77 = sub i32 %sp.76, 4  ; 0x40242a
  %st.p.86 = getelementptr inbounds i8, ptr %frame.top, i64 -20  ; 0x40242a
  store i32 %sp.75, ptr %st.p.86, align 1  ; 0x40242a
  %sp.78 = sub i32 %sp.77, 4  ; 0x40242b
  %st.p.87 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x40242b
  store i32 512, ptr %st.p.87, align 1  ; 0x40242b
  %sp.79 = sub i32 %sp.78, 4  ; 0x402430
  %st.p.88 = getelementptr inbounds i8, ptr %frame.top, i64 -28  ; 0x402430
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), ptr %st.p.88, align 1  ; 0x402430
  %eax.33 = trunc i64 %ret.19 to i32  ; 0x402435
  %sp.80 = sub i32 %sp.79, 4  ; 0x402435
  %st.p.89 = getelementptr inbounds i8, ptr %frame.top, i64 -32  ; 0x402435
  store i32 %eax.33, ptr %st.p.89, align 1  ; 0x402435
  %sarg0.19 = zext i32 %eax.33 to i64
  %sarg1.19 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32) to i64
  %sarg3.19 = zext i32 %sp.75 to i64
  %ret.20 = call i64 @WriteFile(i64 %sarg0.19, i64 %sarg1.19, i64 512, i64 %sarg3.19, i64 0)  ; 0x402436
  %sp.81 = add i32 %sp.80, 20  ; 0x402436
  br label %b_40243e
b_40243e:  ; 0x40243e
  %rax.phi.4 = phi i32 [ 1, %b_4023dc ], [ 0, %b_40241e ]  ; 0x40243e
  %rsp.phi.8 = phi i32 [ %sp.73, %b_4023dc ], [ %sp.81, %b_40241e ]  ; 0x40243e
  %addr.23 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), %rax.phi.4  ; 0x40243e
  %addr.z.92 = zext i32 %addr.23 to i64  ; 0x40243e
  %st.p.90 = inttoptr i64 %addr.z.92 to ptr  ; 0x40243e
  store i8 61, ptr %st.p.90, align 1  ; 0x40243e
  %sum.4 = add i32 %rax.phi.4, 1  ; 0x402445
  br label %b_40247e
b_402448:  ; 0x402448
  %addr.24 = add i32 %rcx.phi.3, 2  ; 0x402448
  store i32 %addr.24, ptr %m.p.1, align 1  ; 0x40244b
  %addr.25 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 337) to i32), %rcx.phi.3  ; 0x402450
  %addr.z.94 = zext i32 %addr.25 to i64  ; 0x402450
  %st.p.92 = inttoptr i64 %addr.z.94 to ptr  ; 0x402450
  store i8 61, ptr %st.p.92, align 1  ; 0x402450
  %not.10 = icmp ne i32 %addr.24, 512  ; 0x40245c
  br i1 %not.10, label %b_40247e, label %b_40245e
b_40245e:  ; 0x40245e
  %sp.82 = sub i32 %rsp.phi.7, 4  ; 0x40245e
  %st.p.93 = getelementptr inbounds i8, ptr %frame.top, i64 -16  ; 0x40245e
  store i32 -11, ptr %st.p.93, align 1  ; 0x40245e
  %ret.21 = call i64 @GetStdHandle(i64 4294967285)  ; 0x402460
  %sp.83 = add i32 %sp.82, 4  ; 0x402460
  %sp.84 = sub i32 %sp.83, 4  ; 0x402468
  store i32 0, ptr %st.p.93, align 1  ; 0x402468
  %sp.85 = sub i32 %sp.84, 4  ; 0x40246a
  %st.p.95 = getelementptr inbounds i8, ptr %frame.top, i64 -20  ; 0x40246a
  store i32 %sp.83, ptr %st.p.95, align 1  ; 0x40246a
  %sp.86 = sub i32 %sp.85, 4  ; 0x40246b
  %st.p.96 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x40246b
  store i32 512, ptr %st.p.96, align 1  ; 0x40246b
  %sp.87 = sub i32 %sp.86, 4  ; 0x402470
  %st.p.97 = getelementptr inbounds i8, ptr %frame.top, i64 -28  ; 0x402470
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), ptr %st.p.97, align 1  ; 0x402470
  %eax.38 = trunc i64 %ret.21 to i32  ; 0x402475
  %sp.88 = sub i32 %sp.87, 4  ; 0x402475
  %st.p.98 = getelementptr inbounds i8, ptr %frame.top, i64 -32  ; 0x402475
  store i32 %eax.38, ptr %st.p.98, align 1  ; 0x402475
  %sarg0.21 = zext i32 %eax.38 to i64
  %sarg1.21 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32) to i64
  %sarg3.21 = zext i32 %sp.83 to i64
  %ret.22 = call i64 @WriteFile(i64 %sarg0.21, i64 %sarg1.21, i64 512, i64 %sarg3.21, i64 0)  ; 0x402476
  %sp.89 = add i32 %sp.88, 20  ; 0x402476
  br label %b_40247e
b_40247e:  ; 0x40247e
  %rax.phi.5 = phi i32 [ %sum.4, %b_40243e ], [ %addr.24, %b_402448 ], [ 0, %b_40245e ]  ; 0x40247e
  %rsp.phi.9 = phi i32 [ %rsp.phi.8, %b_40243e ], [ %rsp.phi.7, %b_402448 ], [ %sp.89, %b_40245e ]  ; 0x40247e
  %addr.26 = add i32 %rax.phi.5, 1  ; 0x40247e
  store i32 %addr.26, ptr %m.p.1, align 1  ; 0x402481
  %addr.27 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), %rax.phi.5  ; 0x402487
  %addr.z.102 = zext i32 %addr.27 to i64  ; 0x402487
  %st.p.100 = inttoptr i64 %addr.z.102 to ptr  ; 0x402487
  store i8 32, ptr %st.p.100, align 1  ; 0x402487
  %addr.z.103 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 168) to i32) to i64  ; 0x40248e
  %m.p.2 = inttoptr i64 %addr.z.103 to ptr  ; 0x40248e
  %m.2 = load i32, ptr %m.p.2, align 1  ; 0x40248e
  %sp.90 = sub i32 %rsp.phi.9, 4  ; 0x402494
  %ret.23 = call i64 @F_00402070(i32 %m.2)  ; 0x402494
  %rsp.132 = add i32 %sp.90, 4  ; 0x402494
  %eh.pending.1 = call i64 @ra_eh_pending()  ; 0x402494
  %eh.thrown.1 = icmp ne i64 %eh.pending.1, 0  ; 0x402494
  br i1 %eh.thrown.1, label %eh.landing, label %eh.ok.402494
b_4024a5:  ; 0x4024a5
  %sp.91 = sub i32 %rsp.132, 4  ; 0x4024a5
  %st.p.102 = getelementptr inbounds i8, ptr %frame.top, i64 -16  ; 0x4024a5
  store i32 -11, ptr %st.p.102, align 1  ; 0x4024a5
  %ret.24 = call i64 @GetStdHandle(i64 4294967285)  ; 0x4024a7
  %sp.92 = add i32 %sp.91, 4  ; 0x4024a7
  %sp.93 = sub i32 %sp.92, 4  ; 0x4024af
  store i32 0, ptr %st.p.102, align 1  ; 0x4024af
  %sp.94 = sub i32 %sp.93, 4  ; 0x4024b1
  %st.p.104 = getelementptr inbounds i8, ptr %frame.top, i64 -20  ; 0x4024b1
  store i32 %sp.92, ptr %st.p.104, align 1  ; 0x4024b1
  %sp.95 = sub i32 %sp.94, 4  ; 0x4024b2
  %st.p.105 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x4024b2
  store i32 512, ptr %st.p.105, align 1  ; 0x4024b2
  %sp.96 = sub i32 %sp.95, 4  ; 0x4024b7
  %st.p.106 = getelementptr inbounds i8, ptr %frame.top, i64 -28  ; 0x4024b7
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), ptr %st.p.106, align 1  ; 0x4024b7
  %eax.42 = trunc i64 %ret.24 to i32  ; 0x4024bc
  %sp.97 = sub i32 %sp.96, 4  ; 0x4024bc
  %st.p.107 = getelementptr inbounds i8, ptr %frame.top, i64 -32  ; 0x4024bc
  store i32 %eax.42, ptr %st.p.107, align 1  ; 0x4024bc
  %sarg0.23 = zext i32 %eax.42 to i64
  %sarg1.23 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32) to i64
  %sarg3.23 = zext i32 %sp.92 to i64
  %ret.25 = call i64 @WriteFile(i64 %sarg0.23, i64 %sarg1.23, i64 512, i64 %sarg3.23, i64 0)  ; 0x4024bd
  %sp.98 = add i32 %sp.97, 20  ; 0x4024bd
  br label %b_4024c5
b_4024c5:  ; 0x4024c5
  %rax.phi.6 = phi i32 [ 0, %b_4024a5 ], [ %m.3, %eh.ok.402494 ]  ; 0x4024c5
  %rsp.phi.10 = phi i32 [ %sp.98, %b_4024a5 ], [ %rsp.132, %eh.ok.402494 ]  ; 0x4024c5
  %addr.28 = add i32 %rax.phi.6, 1  ; 0x4024c5
  store i32 %addr.28, ptr %m.p.1, align 1  ; 0x4024c8
  %addr.29 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), %rax.phi.6  ; 0x4024ce
  %addr.z.113 = zext i32 %addr.29 to i64  ; 0x4024ce
  %st.p.109 = inttoptr i64 %addr.z.113 to ptr  ; 0x4024ce
  store i8 10, ptr %st.p.109, align 1  ; 0x4024ce
  %sp.99 = sub i32 %rsp.phi.10, 4  ; 0x4024d5
  %st.p.110 = getelementptr inbounds i8, ptr %frame.top, i64 -16  ; 0x4024d5
  store i32 -11, ptr %st.p.110, align 1  ; 0x4024d5
  %ret.26 = call i64 @GetStdHandle(i64 4294967285)  ; 0x4024d7
  %sp.100 = add i32 %sp.99, 4  ; 0x4024d7
  %sp.101 = sub i32 %sp.100, 4  ; 0x4024df
  store i32 0, ptr %st.p.110, align 1  ; 0x4024df
  %sp.102 = sub i32 %sp.101, 4  ; 0x4024e1
  %st.p.112 = getelementptr inbounds i8, ptr %frame.top, i64 -20  ; 0x4024e1
  store i32 %sp.100, ptr %st.p.112, align 1  ; 0x4024e1
  %sp.103 = sub i32 %sp.102, 4  ; 0x4024e2
  %st.p.113 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x4024e2
  store i32 %addr.28, ptr %st.p.113, align 1  ; 0x4024e2
  %sp.104 = sub i32 %sp.103, 4  ; 0x4024e3
  %st.p.114 = getelementptr inbounds i8, ptr %frame.top, i64 -28  ; 0x4024e3
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32), ptr %st.p.114, align 1  ; 0x4024e3
  %eax.45 = trunc i64 %ret.26 to i32  ; 0x4024e8
  %sp.105 = sub i32 %sp.104, 4  ; 0x4024e8
  %st.p.115 = getelementptr inbounds i8, ptr %frame.top, i64 -32  ; 0x4024e8
  store i32 %eax.45, ptr %st.p.115, align 1  ; 0x4024e8
  %sarg0.25 = zext i32 %eax.45 to i64
  %sarg1.25 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 336) to i32) to i64
  %sarg2.25 = zext i32 %addr.28 to i64
  %sarg3.25 = zext i32 %sp.100 to i64
  %ret.27 = call i64 @WriteFile(i64 %sarg0.25, i64 %sarg1.25, i64 %sarg2.25, i64 %sarg3.25, i64 0)  ; 0x4024e9
  %sp.106 = add i32 %sp.105, 20  ; 0x4024e9
  store i32 0, ptr %m.p.1, align 1  ; 0x4024ef
  %m.4 = load i32, ptr %m.p.2, align 1  ; 0x4024f9
  %eax.z.20 = zext i32 %m.4 to i64  ; 0x4024f9
  %m.5 = load i32, ptr %st.p.1, align 1  ; 0x4024fe
  %logic.1 = xor i32 %m.5, %sp.106  ; 0x402502
  %m.6 = load i32, ptr %m.p, align 1  ; 0x402504
  %not.12 = icmp ne i32 %m.6, %logic.1  ; 0x40250c
  br i1 %not.12, label %b_402516, label %b_40250e
b_40250e:  ; 0x40250e
  %logic.2 = and i32 %m.4, 127  ; 0x40250e
  %eax.z.21 = zext i32 %logic.2 to i64  ; 0x40250e
  ret i64 %eax.z.21
b_402516:  ; 0x402516
  %m.7 = load i32, ptr %st.p.1, align 1  ; 0x402516
  %logic.3 = xor i32 %m.7, %sp.106  ; 0x40251a
  %ecx.z.29 = zext i32 %logic.3 to i64  ; 0x40251a
  call void @F_00402570(i64 %ecx.z.29)  ; 0x40251c
  %eh.pending.2 = call i64 @ra_eh_pending()  ; 0x40251c
  %eh.thrown.2 = icmp ne i64 %eh.pending.2, 0  ; 0x40251c
  br i1 %eh.thrown.2, label %eh.landing, label %eh.ok.40251c
eh.landing:  ; 0x4021af
  %rax.phi.7 = phi i64 [ %ret, %b_4021a0 ], [ %ret.23, %b_40247e ], [ %eax.z.20, %b_402516 ]  ; 0x4021af
  ret i64 %rax.phi.7
eh.ok.4021af:  ; 0x4021af
  %addr.z.4 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 332) to i32) to i64  ; 0x4021b4
  %m.p.1 = inttoptr i64 %addr.z.4 to ptr  ; 0x4021b4
  %m.1 = load i32, ptr %m.p.1, align 1  ; 0x4021b4
  %not = icmp ne i32 %m.1, 512  ; 0x4021c0
  br i1 %not, label %b_4021ee, label %b_4021c2
eh.ok.402494:  ; 0x402494
  %m.3 = load i32, ptr %m.p.1, align 1  ; 0x402499
  %not.11 = icmp ne i32 %m.3, 512  ; 0x4024a3
  br i1 %not.11, label %b_4024c5, label %b_4024a5
eh.ok.40251c:  ; 0x40251c
  call void @ra_trap(i64 4203809, i64 1)  ; 0x402521
  call void @ra_trap(i64 4203809, i64 1)  ; 0x402521
  unreachable
}

define i64 @F_00402530(i32 %arg0) {  ; 0x402530
b_402530:  ; 0x402530
  %local_m8 = alloca i32, align 4
  %local_m4 = alloca i32, align 4
  %local_p4 = alloca i32, align 4
  store i32 %arg0, ptr %local_p4, align 4
  %ebp = trunc i64 undef to i32  ; 0x402530
  store i32 %ebp, ptr %local_m4, align 1  ; 0x402530
  store i32 %arg0, ptr %local_m8, align 1  ; 0x402533
  %ret = call i64 @F_00402580(i32 %arg0)  ; 0x402536
  %eh.pending = call i64 @ra_eh_pending()  ; 0x402536
  %eh.thrown = icmp ne i64 %eh.pending, 0  ; 0x402536
  br i1 %eh.thrown, label %eh.landing, label %eh.ok.402536
eh.landing:  ; 0x402536
  ret i64 %ret
eh.ok.402536:  ; 0x402536
  ret i64 %ret
}

define i64 @type_info_vf0(i64 %arg0, i64 %arg1) {  ; 0x402540
b_402540:  ; 0x402540
  %frame = alloca [140 x i8], align 16
  %frame.top = getelementptr inbounds i8, ptr %frame, i64 60
  %local_m10 = alloca i32, align 4
  %local_mc = alloca i32, align 4
  %local_m8 = alloca i32, align 4
  %local_m4 = alloca i32, align 4
  %stack.arg0.p = getelementptr inbounds i8, ptr %frame.top, i64 4
  %stack.arg0.lo = trunc i64 %arg1 to i32
  store i32 %stack.arg0.lo, ptr %stack.arg0.p, align 4
  %ebp = trunc i64 undef to i32  ; 0x402540
  store i32 %ebp, ptr %local_m4, align 1  ; 0x402540
  %m = load i8, ptr %stack.arg0.p, align 1  ; 0x402543
  %logic = and i8 %m, 1  ; 0x402543
  %zf = icmp eq i8 %logic, 0  ; 0x402543
  store i32 %ebp, ptr %local_m8, align 1  ; 0x402547
  %ecx = trunc i64 %arg0 to i32  ; 0x402548
  %esi.z = zext i32 %ecx to i64  ; 0x402548
  %st.p.2 = inttoptr i64 %esi.z to ptr  ; 0x40254a
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 376) to i32), ptr %st.p.2, align 1  ; 0x40254a
  br i1 %zf, label %b_40255d, label %b_402552
b_402552:  ; 0x402552
  store i32 12, ptr %local_mc, align 1  ; 0x402552
  store i32 %ecx, ptr %local_m10, align 1  ; 0x402554
  %ret = call i64 @F_00402530(i32 %ecx)  ; 0x402555
  %eh.pending = call i64 @ra_eh_pending()  ; 0x402555
  %eh.thrown = icmp ne i64 %eh.pending, 0  ; 0x402555
  br i1 %eh.thrown, label %eh.landing, label %eh.ok.402555
b_40255d:  ; 0x40255d
  ret i64 %esi.z
eh.landing:  ; 0x402555
  ret i64 %ret
eh.ok.402555:  ; 0x402555
  br label %b_40255d
}

define void @F_00402570(i64 %arg0) {  ; 0x402570
b_402570:  ; 0x402570
  %ecx = trunc i64 %arg0 to i32  ; 0x402570
  %addr.z = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 192) to i32) to i64  ; 0x402570
  %m.p = inttoptr i64 %addr.z to ptr  ; 0x402570
  %m = load i32, ptr %m.p, align 1  ; 0x402570
  %not = icmp ne i32 %ecx, %m  ; 0x402576
  br i1 %not, label %b_402579, label %b_402578
b_402578:  ; 0x402578
  ret void
b_402579:  ; 0x402579
  call void @F_00402590()  ; 0x402579
  ret void
}

define i64 @F_00402580(i32 %arg0) {  ; 0x402580
b_402580:  ; 0x402580
  %local_p4 = alloca i32, align 4
  store i32 %arg0, ptr %local_p4, align 4
  %sarg0 = zext i32 %arg0 to i64
  %ret = call i64 @thunk_free(i64 %sarg0)  ; 0x402580
  ret i64 %ret
}

define void @F_00402590() {  ; 0x402590
b_402590:  ; 0x402590
  call void @ra_trap(i64 4203925, i64 6)  ; 0x402595
  call void @ra_trap(i64 4203925, i64 1)  ; 0x402595
  unreachable
}

define void @F_004025a0() {  ; 0x4025a0
b_4025a0:  ; 0x4025a0
  %local_m10 = alloca i32, align 4
  %local_mc = alloca i32, align 4
  %local_m8 = alloca i32, align 4
  %local_m4 = alloca i32, align 4
  %esi = trunc i64 undef to i32  ; 0x4025a0
  store i32 %esi, ptr %local_m4, align 1  ; 0x4025a0
  store i32 1, ptr %local_m8, align 1  ; 0x4025a1
  %ret = call i64 @thunk__set_app_type(i64 1)  ; 0x4025a3
  %eh.pending = call i64 @ra_eh_pending()  ; 0x4025a3
  %eh.thrown = icmp ne i64 %eh.pending, 0  ; 0x4025a3
  br i1 %eh.thrown, label %eh.landing, label %eh.ok.4025a3
b_4025d0:  ; 0x4025d0
  %ra_x87_sw.p = inttoptr i64 ptrtoint (ptr @ra_x87_sw to i64) to ptr  ; 0x4025d0
  %sw = load i16, ptr %ra_x87_sw.p, align 2  ; 0x4025d0
  %sw.c = and i16 %sw, -256  ; 0x4025d0
  store i16 %sw.c, ptr %ra_x87_sw.p, align 2  ; 0x4025d0
  call void @F_00402e10()  ; 0x4025d2
  %eh.pending.6 = call i64 @ra_eh_pending()  ; 0x4025d2
  %eh.thrown.6 = icmp ne i64 %eh.pending.6, 0  ; 0x4025d2
  br i1 %eh.thrown.6, label %eh.landing, label %eh.ok.4025d2
b_4025f3:  ; 0x4025f3
  call void @F_00402c60()  ; 0x4025f3
  %eh.pending.10 = call i64 @ra_eh_pending()  ; 0x4025f3
  %eh.thrown.10 = icmp ne i64 %eh.pending.10, 0  ; 0x4025f3
  br i1 %eh.thrown.10, label %eh.landing, label %eh.ok.4025f3
b_402601:  ; 0x402601
  store i32 ptrtoint (ptr @U_00402c30 to i32), ptr %local_m4, align 1  ; 0x402601
  %sarg0.5 = zext i32 ptrtoint (ptr @U_00402c30 to i32) to i64
  %ret.9 = call i64 @thunk___setusermatherr(i64 %sarg0.5)  ; 0x402606
  %eh.pending.12 = call i64 @ra_eh_pending()  ; 0x402606
  %eh.thrown.12 = icmp ne i64 %eh.pending.12, 0  ; 0x402606
  br i1 %eh.thrown.12, label %eh.landing, label %eh.ok.402606
b_40260e:  ; 0x40260e
  call void @F_00402cb0()  ; 0x40260e
  %eh.pending.13 = call i64 @ra_eh_pending()  ; 0x40260e
  %eh.thrown.13 = icmp ne i64 %eh.pending.13, 0  ; 0x40260e
  br i1 %eh.thrown.13, label %eh.landing, label %eh.ok.40260e
b_402634:  ; 0x402634
  %ret.13 = call i64 @thunk__initialize_narrow_environment()  ; 0x402634
  %eh.pending.19 = call i64 @ra_eh_pending()  ; 0x402634
  %eh.thrown.19 = icmp ne i64 %eh.pending.19, 0  ; 0x402634
  br i1 %eh.thrown.19, label %eh.landing, label %eh.ok.402634
b_402639:  ; 0x402639
  %ret.14 = call i64 @F_00402c30()  ; 0x402639
  %eh.pending.20 = call i64 @ra_eh_pending()  ; 0x402639
  %eh.thrown.20 = icmp ne i64 %eh.pending.20, 0  ; 0x402639
  br i1 %eh.thrown.20, label %eh.landing, label %eh.ok.402639
b_402647:  ; 0x402647
  ret void
b_402648:  ; 0x402648
  store i32 7, ptr %local_m4, align 1  ; 0x402648
  call void @F_00402da0(i32 7)  ; 0x40264a
  %eh.pending.22 = call i64 @ra_eh_pending()  ; 0x40264a
  %eh.thrown.22 = icmp ne i64 %eh.pending.22, 0  ; 0x40264a
  br i1 %eh.thrown.22, label %eh.landing, label %eh.ok.40264a
eh.landing:  ; 0x4025a3
  ret void
eh.ok.4025a3:  ; 0x4025a3
  %ret.1 = call i64 @F_00402c50()  ; 0x4025a8
  %eh.pending.1 = call i64 @ra_eh_pending()  ; 0x4025a8
  %eh.thrown.1 = icmp ne i64 %eh.pending.1, 0  ; 0x4025a8
  br i1 %eh.thrown.1, label %eh.landing, label %eh.ok.4025a8
eh.ok.4025a8:  ; 0x4025a8
  %eax = trunc i64 %ret.1 to i32  ; 0x4025ad
  store i32 %eax, ptr %local_mc, align 1  ; 0x4025ad
  %sarg0.1 = zext i32 %eax to i64
  %ret.2 = call i64 @thunk__set_fmode(i64 %sarg0.1)  ; 0x4025ae
  %eh.pending.2 = call i64 @ra_eh_pending()  ; 0x4025ae
  %eh.thrown.2 = icmp ne i64 %eh.pending.2, 0  ; 0x4025ae
  br i1 %eh.thrown.2, label %eh.landing, label %eh.ok.4025ae
eh.ok.4025ae:  ; 0x4025ae
  %ret.3 = call i64 @F_00402c30()  ; 0x4025b3
  %eh.pending.3 = call i64 @ra_eh_pending()  ; 0x4025b3
  %eh.thrown.3 = icmp ne i64 %eh.pending.3, 0  ; 0x4025b3
  br i1 %eh.thrown.3, label %eh.landing, label %eh.ok.4025b3
eh.ok.4025b3:  ; 0x4025b3
  %eax.1 = trunc i64 %ret.3 to i32  ; 0x4025b8
  %ret.4 = call i64 @thunk___p__commode()  ; 0x4025ba
  %eh.pending.4 = call i64 @ra_eh_pending()  ; 0x4025ba
  %eh.thrown.4 = icmp ne i64 %eh.pending.4, 0  ; 0x4025ba
  br i1 %eh.thrown.4, label %eh.landing, label %eh.ok.4025ba
eh.ok.4025ba:  ; 0x4025ba
  store i32 1, ptr %local_m10, align 1  ; 0x4025bf
  %eax.2 = trunc i64 %ret.4 to i32  ; 0x4025c1
  %addr.z.9 = zext i32 %eax.2 to i64  ; 0x4025c1
  %st.p.9 = inttoptr i64 %addr.z.9 to ptr  ; 0x4025c1
  store i32 %eax.1, ptr %st.p.9, align 1  ; 0x4025c1
  %sarg0.d.2 = load i32, ptr %local_m10, align 4
  %ret.5 = call i64 @F_00402a80(i32 %sarg0.d.2)  ; 0x4025c3
  %eh.pending.5 = call i64 @ra_eh_pending()  ; 0x4025c3
  %eh.thrown.5 = icmp ne i64 %eh.pending.5, 0  ; 0x4025c3
  br i1 %eh.thrown.5, label %eh.landing, label %eh.ok.4025c3
eh.ok.4025c3:  ; 0x4025c3
  %al = trunc i64 %ret.5 to i8  ; 0x4025cc
  %zf.1 = icmp eq i8 %al, 0  ; 0x4025cc
  br i1 %zf.1, label %b_402648, label %b_4025d0
eh.ok.4025d2:  ; 0x4025d2
  store i32 ptrtoint (ptr @U_00402e40 to i32), ptr %local_m4, align 1  ; 0x4025d7
  call void @F_00402880(i32 ptrtoint (ptr @U_00402e40 to i32))  ; 0x4025dc
  %eh.pending.7 = call i64 @ra_eh_pending()  ; 0x4025dc
  %eh.thrown.7 = icmp ne i64 %eh.pending.7, 0  ; 0x4025dc
  br i1 %eh.thrown.7, label %eh.landing, label %eh.ok.4025dc
eh.ok.4025dc:  ; 0x4025dc
  %ret.6 = call i64 @F_00402c40()  ; 0x4025e1
  %eh.pending.8 = call i64 @ra_eh_pending()  ; 0x4025e1
  %eh.thrown.8 = icmp ne i64 %eh.pending.8, 0  ; 0x4025e1
  br i1 %eh.thrown.8, label %eh.landing, label %eh.ok.4025e1
eh.ok.4025e1:  ; 0x4025e1
  %eax.3 = trunc i64 %ret.6 to i32  ; 0x4025e6
  store i32 %eax.3, ptr %local_m8, align 1  ; 0x4025e6
  %sarg0.4 = zext i32 %eax.3 to i64
  %ret.7 = call i64 @thunk__configure_narrow_argv(i64 %sarg0.4)  ; 0x4025e7
  %eh.pending.9 = call i64 @ra_eh_pending()  ; 0x4025e7
  %eh.thrown.9 = icmp ne i64 %eh.pending.9, 0  ; 0x4025e7
  br i1 %eh.thrown.9, label %eh.landing, label %eh.ok.4025e7
eh.ok.4025e7:  ; 0x4025e7
  %eax.4 = trunc i64 %ret.7 to i32  ; 0x4025ef
  %not = icmp ne i32 %eax.4, 0  ; 0x4025f1
  br i1 %not, label %b_402648, label %b_4025f3
eh.ok.4025f3:  ; 0x4025f3
  %ret.8 = call i64 @F_00402d00()  ; 0x4025f8
  %eh.pending.11 = call i64 @ra_eh_pending()  ; 0x4025f8
  %eh.thrown.11 = icmp ne i64 %eh.pending.11, 0  ; 0x4025f8
  br i1 %eh.thrown.11, label %eh.landing, label %eh.ok.4025f8
eh.ok.4025f8:  ; 0x4025f8
  %eax.6 = trunc i64 %ret.8 to i32  ; 0x4025fd
  %zf.6 = icmp eq i32 %eax.6, 0  ; 0x4025fd
  br i1 %zf.6, label %b_40260e, label %b_402601
eh.ok.402606:  ; 0x402606
  br label %b_40260e
eh.ok.40260e:  ; 0x40260e
  call void @F_00402cb0()  ; 0x402613
  %eh.pending.14 = call i64 @ra_eh_pending()  ; 0x402613
  %eh.thrown.14 = icmp ne i64 %eh.pending.14, 0  ; 0x402613
  br i1 %eh.thrown.14, label %eh.landing, label %eh.ok.402613
eh.ok.402613:  ; 0x402613
  call void @F_00402c80()  ; 0x402618
  %eh.pending.15 = call i64 @ra_eh_pending()  ; 0x402618
  %eh.thrown.15 = icmp ne i64 %eh.pending.15, 0  ; 0x402618
  br i1 %eh.thrown.15, label %eh.landing, label %eh.ok.402618
eh.ok.402618:  ; 0x402618
  %ret.10 = call i64 @F_00402c30()  ; 0x40261d
  %eh.pending.16 = call i64 @ra_eh_pending()  ; 0x40261d
  %eh.thrown.16 = icmp ne i64 %eh.pending.16, 0  ; 0x40261d
  br i1 %eh.thrown.16, label %eh.landing, label %eh.ok.40261d
eh.ok.40261d:  ; 0x40261d
  %eax.8 = trunc i64 %ret.10 to i32  ; 0x402622
  store i32 %eax.8, ptr %local_m4, align 1  ; 0x402622
  %sarg0.6 = zext i32 %eax.8 to i64
  %ret.11 = call i64 @thunk__configthreadlocale(i64 %sarg0.6)  ; 0x402623
  %eh.pending.17 = call i64 @ra_eh_pending()  ; 0x402623
  %eh.thrown.17 = icmp ne i64 %eh.pending.17, 0  ; 0x402623
  br i1 %eh.thrown.17, label %eh.landing, label %eh.ok.402623
eh.ok.402623:  ; 0x402623
  %ret.12 = call i8 @F_00402c70()  ; 0x40262b
  %eh.pending.18 = call i64 @ra_eh_pending()  ; 0x40262b
  %eh.thrown.18 = icmp ne i64 %eh.pending.18, 0  ; 0x40262b
  br i1 %eh.thrown.18, label %eh.landing, label %eh.ok.40262b
eh.ok.40262b:  ; 0x40262b
  %zf.10 = icmp eq i8 %ret.12, 0  ; 0x402630
  br i1 %zf.10, label %b_402639, label %b_402634
eh.ok.402634:  ; 0x402634
  br label %b_402639
eh.ok.402639:  ; 0x402639
  %ret.15 = call i64 @F_00402d80()  ; 0x40263e
  %eh.pending.21 = call i64 @ra_eh_pending()  ; 0x40263e
  %eh.thrown.21 = icmp ne i64 %eh.pending.21, 0  ; 0x40263e
  br i1 %eh.thrown.21, label %eh.landing, label %eh.ok.40263e
eh.ok.40263e:  ; 0x40263e
  %eax.9 = trunc i64 %ret.15 to i32  ; 0x402643
  %not.1 = icmp ne i32 %eax.9, 0  ; 0x402645
  br i1 %not.1, label %b_402648, label %b_402647
eh.ok.40264a:  ; 0x40264a
  unreachable
}

define i64 @F_00402650() {  ; 0x402650
b_402650:  ; 0x402650
  %ret = call i64 @F_00402cc0()  ; 0x402650
  %eh.pending = call i64 @ra_eh_pending()  ; 0x402650
  %eh.thrown = icmp ne i64 %eh.pending, 0  ; 0x402650
  br i1 %eh.thrown, label %eh.landing, label %eh.ok.402650
eh.landing:  ; 0x402650
  ret i64 %ret
eh.ok.402650:  ; 0x402650
  ret i64 0
}

define void @F_00402660() {  ; 0x402660
b_402660:  ; 0x402660
  %local_m4 = alloca i32, align 4
  call void @F_00402d90()  ; 0x402660
  %eh.pending = call i64 @ra_eh_pending()  ; 0x402660
  %eh.thrown = icmp ne i64 %eh.pending, 0  ; 0x402660
  br i1 %eh.thrown, label %eh.landing, label %eh.ok.402660
eh.landing:  ; 0x402660
  ret void
eh.ok.402660:  ; 0x402660
  %ret = call i64 @F_00402c30()  ; 0x402665
  %eh.pending.1 = call i64 @ra_eh_pending()  ; 0x402665
  %eh.thrown.1 = icmp ne i64 %eh.pending.1, 0  ; 0x402665
  br i1 %eh.thrown.1, label %eh.landing, label %eh.ok.402665
eh.ok.402665:  ; 0x402665
  %eax = trunc i64 %ret to i32  ; 0x40266a
  store i32 %eax, ptr %local_m4, align 1  ; 0x40266a
  %sarg0 = zext i32 %eax to i64
  %ret.1 = call i64 @thunk__set_new_mode(i64 %sarg0)  ; 0x40266b
  %eh.pending.2 = call i64 @ra_eh_pending()  ; 0x40266b
  %eh.thrown.2 = icmp ne i64 %eh.pending.2, 0  ; 0x40266b
  br i1 %eh.thrown.2, label %eh.landing, label %eh.ok.40266b
eh.ok.40266b:  ; 0x40266b
  ret void
}

define i64 @F_00402680() {  ; 0x402680
b_402680:  ; 0x402680
  %frame = alloca [4296 x i8], align 16
  %frame.top = getelementptr inbounds i8, ptr %frame, i64 4220
  %rsp0 = ptrtoint ptr %frame.top to i64
  %ebp = trunc i64 undef to i32  ; 0x402680
  %esp = trunc i64 %rsp0 to i32  ; 0x402680
  %sp = sub i32 %esp, 4  ; 0x402680
  %st.p = getelementptr inbounds i8, ptr %frame.top, i64 -4  ; 0x402680
  store i32 %ebp, ptr %st.p, align 1  ; 0x402680
  %sp.1 = sub i32 %sp, 4  ; 0x402683
  %st.p.1 = getelementptr inbounds i8, ptr %frame.top, i64 -8  ; 0x402683
  store i32 -2, ptr %st.p.1, align 1  ; 0x402683
  %sp.2 = sub i32 %sp.1, 4  ; 0x402685
  %st.p.2 = getelementptr inbounds i8, ptr %frame.top, i64 -12  ; 0x402685
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 3608) to i32), ptr %st.p.2, align 1  ; 0x402685
  %sp.3 = sub i32 %sp.2, 4  ; 0x40268a
  %st.p.3 = getelementptr inbounds i8, ptr %frame.top, i64 -16  ; 0x40268a
  store i32 ptrtoint (ptr @U_00402e70 to i32), ptr %st.p.3, align 1  ; 0x40268a
  %teb = call i64 @ra_teb()  ; 0x40268f
  %teb.1 = trunc i64 %teb to i32  ; 0x40268f
  %addr.z.4 = zext i32 %teb.1 to i64  ; 0x40268f
  %m.p = inttoptr i64 %addr.z.4 to ptr  ; 0x40268f
  %m = load i32, ptr %m.p, align 1  ; 0x40268f
  %sp.4 = sub i32 %sp.3, 4  ; 0x402695
  %st.p.4 = getelementptr inbounds i8, ptr %frame.top, i64 -20  ; 0x402695
  store i32 %m, ptr %st.p.4, align 1  ; 0x402695
  %diff = sub i32 %sp.4, 20  ; 0x402696
  %sp.5 = sub i32 %diff, 4  ; 0x402699
  %st.p.5 = getelementptr inbounds i8, ptr %frame.top, i64 -44  ; 0x402699
  store i32 %ebp, ptr %st.p.5, align 1  ; 0x402699
  %sp.6 = sub i32 %sp.5, 4  ; 0x40269a
  %st.p.6 = getelementptr inbounds i8, ptr %frame.top, i64 -48  ; 0x40269a
  store i32 %ebp, ptr %st.p.6, align 1  ; 0x40269a
  %sp.7 = sub i32 %sp.6, 4  ; 0x40269b
  %st.p.7 = getelementptr inbounds i8, ptr %frame.top, i64 -52  ; 0x40269b
  store i32 %ebp, ptr %st.p.7, align 1  ; 0x40269b
  %addr.z.9 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 192) to i32) to i64  ; 0x40269c
  %m.p.1 = inttoptr i64 %addr.z.9 to ptr  ; 0x40269c
  %m.1 = load i32, ptr %m.p.1, align 1  ; 0x40269c
  %m.2 = load i32, ptr %st.p.2, align 1  ; 0x4026a1
  %logic = xor i32 %m.2, %m.1  ; 0x4026a1
  store i32 %logic, ptr %st.p.2, align 1  ; 0x4026a1
  %logic.1 = xor i32 %m.1, %sp  ; 0x4026a4
  %sp.8 = sub i32 %sp.7, 4  ; 0x4026a6
  %st.p.9 = getelementptr inbounds i8, ptr %frame.top, i64 -56  ; 0x4026a6
  store i32 %logic.1, ptr %st.p.9, align 1  ; 0x4026a6
  %addr.2 = add i32 %sp, -16  ; 0x4026a7
  %teb.2 = call i64 @ra_teb()  ; 0x4026aa
  %teb.3 = trunc i64 %teb.2 to i32  ; 0x4026aa
  %addr.z.13 = zext i32 %teb.3 to i64  ; 0x4026aa
  %st.p.10 = inttoptr i64 %addr.z.13 to ptr  ; 0x4026aa
  store i32 %addr.2, ptr %st.p.10, align 1  ; 0x4026aa
  %st.p.11 = getelementptr inbounds i8, ptr %frame.top, i64 -28  ; 0x4026b0
  store i32 %sp.8, ptr %st.p.11, align 1  ; 0x4026b0
  %sp.9 = sub i32 %sp.8, 4  ; 0x4026b3
  %st.p.12 = getelementptr inbounds i8, ptr %frame.top, i64 -60  ; 0x4026b3
  store i32 1, ptr %st.p.12, align 1  ; 0x4026b3
  %sp.10 = sub i32 %sp.9, 4  ; 0x4026b5
  %sarg0.a = add i32 %sp.10, 4
  %ret = call i8 @F_00402a10(i32 1)  ; 0x4026b5
  %eh.pending = call i64 @ra_eh_pending()  ; 0x4026b5
  %eh.thrown = icmp ne i64 %eh.pending, 0  ; 0x4026b5
  br i1 %eh.thrown, label %eh.landing, label %eh.ok.4026b5
b_4026c5:  ; 0x4026c5
  %st.p.14 = getelementptr inbounds i8, ptr %frame.top, i64 -29  ; 0x4026c7
  store i8 0, ptr %st.p.14, align 1  ; 0x4026c7
  store i32 0, ptr %st.p.1, align 1  ; 0x4026ca
  %sp.11 = sub i32 %sum, 4  ; 0x4026d1
  %ret.1 = call i64 @F_004029b0()  ; 0x4026d1
  %rsp.176 = add i32 %sp.11, 4  ; 0x4026d1
  %eh.pending.1 = call i64 @ra_eh_pending()  ; 0x4026d1
  %eh.thrown.1 = icmp ne i64 %eh.pending.1, 0  ; 0x4026d1
  br i1 %eh.thrown.1, label %eh.landing, label %eh.ok.4026d1
b_4026e7:  ; 0x4026e7
  %not = icmp ne i32 %m.9, 0  ; 0x4026e9
  br i1 %not, label %b_402747, label %b_4026eb
b_4026eb:  ; 0x4026eb
  br label %eh.probe.4026eb
b_40270b:  ; 0x40270b
  br label %eh.probe.40270b
b_402729:  ; 0x402729
  %sp.41 = sub i32 %sum.4, 4  ; 0x402729
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 852) to i32), ptr %st.p.12, align 1  ; 0x402729
  %sp.42 = sub i32 %sp.41, 4  ; 0x40272e
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 844) to i32), ptr %st.p.41, align 1  ; 0x40272e
  %sp.43 = sub i32 %sp.42, 4  ; 0x402733
  %sarg0.a.3 = add i32 %sp.43, 4
  %sarg0.3 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 844) to i32) to i64
  %sarg1.2 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 852) to i32) to i64
  %ret.4 = call i64 @thunk__initterm(i64 %sarg0.3, i64 %sarg1.2)  ; 0x402733
  %eh.pending.7 = call i64 @ra_eh_pending()  ; 0x402733
  %eh.thrown.7 = icmp ne i64 %eh.pending.7, 0  ; 0x402733
  br i1 %eh.thrown.7, label %eh.landing, label %eh.ok.402733
b_402747:  ; 0x402747
  br label %eh.probe.402749
b_40274c:  ; 0x40274c
  %rbx.phi.5 = phi i8 [ 0, %eh.access.40273b ], [ 1, %eh.access.402749 ]  ; 0x40274c
  %rsp.phi.6 = phi i32 [ %rsp.phi.4, %eh.access.40273b ], [ %rsp.phi.5, %eh.access.402749 ]  ; 0x40274c
  br label %eh.probe.40274c
b_402763:  ; 0x402763
  %sp.67 = sub i32 %rsp.phi.8, 4  ; 0x402763
  store i32 %ret.7, ptr %st.p.12, align 1  ; 0x402763
  %sp.68 = sub i32 %sp.67, 4  ; 0x402764
  %varargs.a = add i32 %sp.68, 4
  %varargs = ptrtoint ptr %st.p.12 to i64
  %ret.8 = call i32 @F_004028d0(i64 %varargs)  ; 0x402764
  %eh.pending.14 = call i64 @ra_eh_pending()  ; 0x402764
  %eh.thrown.14 = icmp ne i64 %eh.pending.14, 0  ; 0x402764
  br i1 %eh.thrown.14, label %eh.landing, label %eh.ok.402764
b_402770:  ; 0x402770
  br label %eh.probe.402770
b_402782:  ; 0x402782
  %rsp.phi.11 = phi i32 [ %rsp.phi.8, %eh.access.40275e ], [ %sum.12, %eh.ok.402764 ], [ %rsp.186, %eh.ok.402780 ]  ; 0x402782
  %sp.84 = sub i32 %rsp.phi.11, 4  ; 0x402782
  %ret.13 = call i32 @F_00402d20()  ; 0x402782
  %rsp.189 = add i32 %sp.84, 4  ; 0x402782
  %eh.pending.19 = call i64 @ra_eh_pending()  ; 0x402782
  %eh.thrown.19 = icmp ne i64 %eh.pending.19, 0  ; 0x402782
  br i1 %eh.thrown.19, label %eh.landing, label %eh.ok.402782
b_40278e:  ; 0x40278e
  %sp.90 = sub i32 %rsp.phi.12, 4  ; 0x40278e
  %addr.z.177 = zext i32 %sp.90 to i64  ; 0x40278e
  %st.p.117 = inttoptr i64 %addr.z.177 to ptr  ; 0x40278e
  store i32 %ret.13, ptr %st.p.117, align 1  ; 0x40278e
  %sp.91 = sub i32 %sp.90, 4  ; 0x40278f
  %addr.z.178 = zext i32 %sp.91 to i64  ; 0x40278f
  %varargs.a.1 = add i64 %addr.z.178, 4
  %varargs.p.1 = inttoptr i64 %varargs.a.1 to ptr
  %varargs.1 = ptrtoint ptr %varargs.p.1 to i64
  %ret.14 = call i32 @F_004028d0(i64 %varargs.1)  ; 0x40278f
  %eh.pending.21 = call i64 @ra_eh_pending()  ; 0x40278f
  %eh.thrown.21 = icmp ne i64 %eh.pending.21, 0  ; 0x40278f
  br i1 %eh.thrown.21, label %eh.landing, label %eh.ok.40278f
b_40279b:  ; 0x40279b
  br label %eh.probe.40279b
b_4027a5:  ; 0x4027a5
  %rsp.phi.14 = phi i32 [ %rsp.phi.12, %eh.access.402789 ], [ %sum.16, %eh.ok.40278f ], [ %sum.18, %eh.ok.40279d ]  ; 0x4027a5
  %sp.99 = sub i32 %rsp.phi.14, 4  ; 0x4027a5
  %ret.17 = call i64 @thunk__get_initial_narrow_environment()  ; 0x4027a5
  %rsp.193 = add i32 %sp.99, 4  ; 0x4027a5
  %eh.pending.24 = call i64 @ra_eh_pending()  ; 0x4027a5
  %eh.thrown.24 = icmp ne i64 %eh.pending.24, 0  ; 0x4027a5
  br i1 %eh.thrown.24, label %eh.landing, label %eh.ok.4027a5
b_4027d3:  ; 0x4027d3
  %not.1 = icmp ne i8 %rbx.phi.5, 0  ; 0x4027d5
  br i1 %not.1, label %b_4027dc, label %b_4027d7
b_4027d7:  ; 0x4027d7
  %sp.117 = sub i32 %rsp.197, 4  ; 0x4027d7
  %ret.22 = call i64 @thunk__cexit()  ; 0x4027d7
  %rsp.198 = add i32 %sp.117, 4  ; 0x4027d7
  %eh.pending.31 = call i64 @ra_eh_pending()  ; 0x4027d7
  %eh.thrown.31 = icmp ne i64 %eh.pending.31, 0  ; 0x4027d7
  br i1 %eh.thrown.31, label %eh.landing, label %eh.ok.4027d7
b_4027dc:  ; 0x4027dc
  %rsp.phi.17 = phi i32 [ %rsp.197, %b_4027d3 ], [ %rsp.198, %eh.ok.4027d7 ]  ; 0x4027dc
  %sp.118 = sub i32 %rsp.phi.17, 4  ; 0x4027dc
  %addr.z.226 = zext i32 %sp.118 to i64  ; 0x4027dc
  %st.p.151 = inttoptr i64 %addr.z.226 to ptr  ; 0x4027dc
  store i32 0, ptr %st.p.151, align 1  ; 0x4027dc
  %sp.119 = sub i32 %sp.118, 4  ; 0x4027de
  %addr.z.227 = zext i32 %sp.119 to i64  ; 0x4027de
  %st.p.152 = inttoptr i64 %addr.z.227 to ptr  ; 0x4027de
  store i32 1, ptr %st.p.152, align 1  ; 0x4027de
  %sp.120 = sub i32 %sp.119, 4  ; 0x4027e0
  %sarg0.a.13 = add i32 %sp.120, 4
  call void @F_00402a50(i32 1, i32 0)  ; 0x4027e0
  %eh.pending.32 = call i64 @ra_eh_pending()  ; 0x4027e0
  %eh.thrown.32 = icmp ne i64 %eh.pending.32, 0  ; 0x4027e0
  br i1 %eh.thrown.32, label %eh.landing, label %eh.ok.4027e0
b_402818:  ; 0x402818
  %m.59 = load i32, ptr %st.p.11, align 1  ; 0x402818
  %sp.132 = sub i32 %m.59, 4  ; 0x40281b
  %ret.33 = call i64 @F_00402d30()  ; 0x40281b
  %rsp.211 = add i32 %sp.132, 4  ; 0x40281b
  %eh.pending.34 = call i64 @ra_eh_pending()  ; 0x40281b
  %eh.thrown.34 = icmp ne i64 %eh.pending.34, 0  ; 0x40281b
  br i1 %eh.thrown.34, label %eh.landing, label %eh.ok.40281b
b_402824:  ; 0x402824
  %m.60 = load i8, ptr %st.p.14, align 1  ; 0x402824
  %not.2 = icmp ne i8 %m.60, 0  ; 0x402828
  br i1 %not.2, label %b_40282f, label %b_40282a
b_40282a:  ; 0x40282a
  %ret.34 = call i64 @thunk__c_exit()  ; 0x40282a
  %eh.pending.35 = call i64 @ra_eh_pending()  ; 0x40282a
  %eh.thrown.35 = icmp ne i64 %eh.pending.35, 0  ; 0x40282a
  br i1 %eh.thrown.35, label %eh.landing, label %eh.ok.40282a
b_40282f:  ; 0x40282f
  store i32 -2, ptr %st.p.1, align 1  ; 0x40282f
  %m.p.61 = getelementptr inbounds i8, ptr %frame.top, i64 -36  ; 0x402836
  %m.61 = load i32, ptr %m.p.61, align 1  ; 0x402836
  %eax.z.37 = zext i32 %m.61 to i64  ; 0x402836
  %m.62 = load i32, ptr %st.p.4, align 1  ; 0x402839
  %teb.8 = call i64 @ra_teb()  ; 0x40283c
  %teb.9 = trunc i64 %teb.8 to i32  ; 0x40283c
  %addr.z.256 = zext i32 %teb.9 to i64  ; 0x40283c
  %st.p.166 = inttoptr i64 %addr.z.256 to ptr  ; 0x40283c
  store i32 %m.62, ptr %st.p.166, align 1  ; 0x40283c
  ret i64 %eax.z.37
b_40284b:  ; 0x40284b
  store i32 7, ptr %st.p.12, align 1  ; 0x40284b
  call void @F_00402da0(i32 7)  ; 0x40284d
  %eh.pending.36 = call i64 @ra_eh_pending()  ; 0x40284d
  %eh.thrown.36 = icmp ne i64 %eh.pending.36, 0  ; 0x40284d
  br i1 %eh.thrown.36, label %eh.landing, label %eh.ok.40284d
b_402852:  ; 0x402852
  store i32 7, ptr %st.p.12, align 1  ; 0x402852
  call void @F_00402da0(i32 7)  ; 0x402854
  %eh.pending.37 = call i64 @ra_eh_pending()  ; 0x402854
  %eh.thrown.37 = icmp ne i64 %eh.pending.37, 0  ; 0x402854
  br i1 %eh.thrown.37, label %eh.landing, label %eh.ok.402854
b_402859:  ; 0x402859
  %sp.144 = sub i32 %rsp.197, 4  ; 0x402859
  %addr.z.267 = zext i32 %sp.144 to i64  ; 0x402859
  %st.p.171 = inttoptr i64 %addr.z.267 to ptr  ; 0x402859
  store i32 %eax.73, ptr %st.p.171, align 1  ; 0x402859
  %sarg0.15 = zext i32 %eax.73 to i64
  %ret.24 = call i64 @thunk_exit(i64 %sarg0.15)  ; 0x40285a
  %eh.pending.38 = call i64 @ra_eh_pending()  ; 0x40285a
  %eh.thrown.38 = icmp ne i64 %eh.pending.38, 0  ; 0x40285a
  br i1 %eh.thrown.38, label %eh.landing, label %eh.ok.40285a
b_40285f:  ; 0x40285f
  %m.p.63 = getelementptr inbounds i8, ptr %frame.top, i64 -36  ; 0x40285f
  %m.63 = load i32, ptr %m.p.63, align 1  ; 0x40285f
  %sp.146 = sub i32 %rsp.211, 4  ; 0x40285f
  %addr.z.270 = zext i32 %sp.146 to i64  ; 0x40285f
  %st.p.173 = inttoptr i64 %addr.z.270 to ptr  ; 0x40285f
  store i32 %m.63, ptr %st.p.173, align 1  ; 0x40285f
  %sarg0.25 = zext i32 %m.63 to i64
  %ret.35 = call i64 @thunk__exit(i64 %sarg0.25)  ; 0x402862
  %eh.pending.39 = call i64 @ra_eh_pending()  ; 0x402862
  %eh.thrown.39 = icmp ne i64 %eh.pending.39, 0  ; 0x402862
  br i1 %eh.thrown.39, label %eh.landing, label %eh.ok.402862
eh.landing:  ; 0x4026b5
  call void @ra_trap(i64 4204213, i64 8)  ; 0x4026b5
  unreachable
eh.ok.4026b5:  ; 0x4026b5
  %sum = add i32 %sarg0.a, 4  ; 0x4026ba
  %zf.4 = icmp eq i8 %ret, 0  ; 0x4026bd
  br i1 %zf.4, label %b_40284b, label %b_4026c5
eh.ok.4026d1:  ; 0x4026d1
  %n = trunc i64 %ret.1 to i8
  br label %eh.probe.4026d6
eh.probe.4026d6:  ; 0x4026d6
  %rax.phi = phi i8 [ %n, %eh.ok.4026d1 ], [ %n.1, %eh.resume.4026d6.0 ]  ; 0x4026d6
  %rsp.phi = phi i32 [ %rsp.176, %eh.ok.4026d1 ], [ %sp.16, %eh.resume.4026d6.0 ]  ; 0x4026d6
  %addr.6 = add i32 %sp, -36  ; 0x4026d6
  %eh.at = zext i32 %addr.6 to i64  ; 0x4026d6
  %eh.fault = call i64 @ra_eh_probe(i64 %eh.at, i64 1, i64 1, i64 4204246)  ; 0x4026d6
  %eh.faults = icmp ne i64 %eh.fault, 0  ; 0x4026d6
  br i1 %eh.faults, label %eh.seh.4026d6, label %eh.access.4026d6
eh.seh.4026d6:  ; 0x4026d6
  %eh.pointers = call i64 @ra_eh_pointers()  ; 0x4026d6
  %eh.pointers.1 = trunc i64 %eh.pointers to i32  ; 0x4026d6
  %st.p.17 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x4026d6
  store i32 %eh.pointers.1, ptr %st.p.17, align 1  ; 0x4026d6
  %sp.12 = sub i32 %rsp.phi, 4  ; 0x4026d6
  store i32 0, ptr %st.p.12, align 1  ; 0x4026d6
  br label %f1_402803
eh.filtered.4026d6.0:  ; 0x4026d6
  %eax.8 = trunc i64 %ret.32 to i32  ; 0x402817
  %eh.execute = icmp sgt i32 %eax.8, 0  ; 0x402817
  br i1 %eh.execute, label %eh.except.4026d6.0, label %eh.verdict.4026d6.0
f1_402803:  ; 0x402803
  %m.3 = load i32, ptr %st.p.17, align 1  ; 0x402803
  %ecx.z = zext i32 %m.3 to i64  ; 0x402803
  %m.p.4 = inttoptr i64 %ecx.z to ptr  ; 0x402806
  %m.4 = load i32, ptr %m.p.4, align 1  ; 0x402806
  %eax.z.4 = zext i32 %m.4 to i64  ; 0x402806
  %m.p.5 = inttoptr i64 %eax.z.4 to ptr  ; 0x402808
  %m.5 = load i32, ptr %m.p.5, align 1  ; 0x402808
  %st.p.19 = getelementptr inbounds i8, ptr %frame.top, i64 -36  ; 0x40280a
  store i32 %m.5, ptr %st.p.19, align 1  ; 0x40280a
  %sp.13 = sub i32 %sp.12, 4  ; 0x40280d
  %st.p.20 = getelementptr inbounds i8, ptr %frame.top, i64 -64  ; 0x40280d
  store i32 %m.3, ptr %st.p.20, align 1  ; 0x40280d
  %sp.14 = sub i32 %sp.13, 4  ; 0x40280e
  %st.p.21 = getelementptr inbounds i8, ptr %frame.top, i64 -68  ; 0x40280e
  store i32 %m.5, ptr %st.p.21, align 1  ; 0x40280e
  %sp.15 = sub i32 %sp.14, 4  ; 0x40280f
  %sarg0.a.24 = add i32 %sp.15, 4
  %sarg0.24 = zext i32 %m.5 to i64
  %ret.32 = call i64 @thunk__seh_filter_exe(i64 %sarg0.24, i64 %ecx.z)  ; 0x40280f
  %eh.pending.2 = call i64 @ra_eh_pending()  ; 0x40280f
  %eh.thrown.2 = icmp ne i64 %eh.pending.2, 0  ; 0x40280f
  br i1 %eh.thrown.2, label %eh.landing.1, label %eh.ok.40280f
eh.landing.1:  ; 0x40280f
  call void @ra_trap(i64 4204559, i64 8)  ; 0x40280f
  unreachable
eh.ok.40280f:  ; 0x40280f
  %sum.1 = add i32 %sarg0.a.24, 8  ; 0x402814
  %sp.16 = add i32 %sum.1, 4  ; 0x402817
  br label %eh.filtered.4026d6.0
eh.except.4026d6.0:  ; 0x402817
  call void @ra_eh_handled()  ; 0x402817
  store i32 -2, ptr %st.p.1, align 1  ; 0x402817
  br label %b_402818
eh.verdict.4026d6.0:  ; 0x402817
  %eh.continue = icmp slt i32 %eax.8, 0  ; 0x402817
  br i1 %eh.continue, label %eh.resume.4026d6.0, label %eh.search.4026d6.0
eh.resume.4026d6.0:  ; 0x402817
  call void @ra_eh_handled()  ; 0x402817
  %n.1 = trunc i64 %ret.32 to i8
  br label %eh.probe.4026d6
eh.search.4026d6.0:  ; 0x402817
  call void @ra_eh_unhandled(i64 4204246)  ; 0x402817
  unreachable
eh.access.4026d6:  ; 0x402817
  %st.p.24 = getelementptr inbounds i8, ptr %frame.top, i64 -40  ; 0x402817
  store i8 %rax.phi, ptr %st.p.24, align 1  ; 0x402817
  br label %eh.probe.4026d9
eh.probe.4026d9:  ; 0x4026d9
  %rsp.phi.1 = phi i32 [ %rsp.phi, %eh.access.4026d6 ], [ %sp.21, %eh.resume.4026d9.0 ]  ; 0x4026d9
  %eh.at.1 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 848) to i32) to i64  ; 0x4026d9
  %eh.fault.1 = call i64 @ra_eh_probe(i64 %eh.at.1, i64 4, i64 0, i64 4204249)  ; 0x4026d9
  %eh.faults.1 = icmp ne i64 %eh.fault.1, 0  ; 0x4026d9
  br i1 %eh.faults.1, label %eh.seh.4026d9, label %eh.access.4026d9
eh.seh.4026d9:  ; 0x4026d9
  %eh.pointers.2 = call i64 @ra_eh_pointers()  ; 0x4026d9
  %eh.pointers.3 = trunc i64 %eh.pointers.2 to i32  ; 0x4026d9
  %st.p.25 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x4026d9
  store i32 %eh.pointers.3, ptr %st.p.25, align 1  ; 0x4026d9
  %sp.17 = sub i32 %rsp.phi.1, 4  ; 0x4026d9
  store i32 0, ptr %st.p.12, align 1  ; 0x4026d9
  br label %f2_402803
eh.filtered.4026d9.0:  ; 0x4026d9
  %eax.12 = trunc i64 %ret.31 to i32  ; 0x402817
  %eh.execute.1 = icmp sgt i32 %eax.12, 0  ; 0x402817
  br i1 %eh.execute.1, label %eh.except.4026d9.0, label %eh.verdict.4026d9.0
f2_402803:  ; 0x402803
  %m.6 = load i32, ptr %st.p.25, align 1  ; 0x402803
  %ecx.z.1 = zext i32 %m.6 to i64  ; 0x402803
  %m.p.7 = inttoptr i64 %ecx.z.1 to ptr  ; 0x402806
  %m.7 = load i32, ptr %m.p.7, align 1  ; 0x402806
  %eax.z.6 = zext i32 %m.7 to i64  ; 0x402806
  %m.p.8 = inttoptr i64 %eax.z.6 to ptr  ; 0x402808
  %m.8 = load i32, ptr %m.p.8, align 1  ; 0x402808
  %st.p.27 = getelementptr inbounds i8, ptr %frame.top, i64 -36  ; 0x40280a
  store i32 %m.8, ptr %st.p.27, align 1  ; 0x40280a
  %sp.18 = sub i32 %sp.17, 4  ; 0x40280d
  %st.p.28 = getelementptr inbounds i8, ptr %frame.top, i64 -64  ; 0x40280d
  store i32 %m.6, ptr %st.p.28, align 1  ; 0x40280d
  %sp.19 = sub i32 %sp.18, 4  ; 0x40280e
  %st.p.29 = getelementptr inbounds i8, ptr %frame.top, i64 -68  ; 0x40280e
  store i32 %m.8, ptr %st.p.29, align 1  ; 0x40280e
  %sp.20 = sub i32 %sp.19, 4  ; 0x40280f
  %sarg0.a.23 = add i32 %sp.20, 4
  %sarg0.23 = zext i32 %m.8 to i64
  %ret.31 = call i64 @thunk__seh_filter_exe(i64 %sarg0.23, i64 %ecx.z.1)  ; 0x40280f
  %eh.pending.3 = call i64 @ra_eh_pending()  ; 0x40280f
  %eh.thrown.3 = icmp ne i64 %eh.pending.3, 0  ; 0x40280f
  br i1 %eh.thrown.3, label %eh.landing.1, label %eh.ok.40280f.1
eh.ok.40280f.1:  ; 0x40280f
  %sum.2 = add i32 %sarg0.a.23, 8  ; 0x402814
  %sp.21 = add i32 %sum.2, 4  ; 0x402817
  br label %eh.filtered.4026d9.0
eh.except.4026d9.0:  ; 0x402817
  call void @ra_eh_handled()  ; 0x402817
  store i32 -2, ptr %st.p.1, align 1  ; 0x402817
  br label %b_402818
eh.verdict.4026d9.0:  ; 0x402817
  %eh.continue.1 = icmp slt i32 %eax.12, 0  ; 0x402817
  br i1 %eh.continue.1, label %eh.resume.4026d9.0, label %eh.search.4026d9.0
eh.resume.4026d9.0:  ; 0x402817
  call void @ra_eh_handled()  ; 0x402817
  br label %eh.probe.4026d9
eh.search.4026d9.0:  ; 0x402817
  call void @ra_eh_unhandled(i64 4204249)  ; 0x402817
  unreachable
eh.access.4026d9:  ; 0x402817
  %m.p.9 = inttoptr i64 %eh.at.1 to ptr  ; 0x402817
  %m.9 = load i32, ptr %m.p.9, align 1  ; 0x402817
  %zf.9 = icmp eq i32 %m.9, 1  ; 0x4026de
  br i1 %zf.9, label %b_402852, label %b_4026e7
eh.probe.4026eb:  ; 0x4026eb
  %rsp.phi.2 = phi i32 [ %rsp.phi.1, %b_4026eb ], [ %sp.26, %eh.resume.4026eb.0 ]  ; 0x4026eb
  %eh.fault.2 = call i64 @ra_eh_probe(i64 %eh.at.1, i64 4, i64 1, i64 4204267)  ; 0x4026eb
  %eh.faults.2 = icmp ne i64 %eh.fault.2, 0  ; 0x4026eb
  br i1 %eh.faults.2, label %eh.seh.4026eb, label %eh.access.4026eb
eh.seh.4026eb:  ; 0x4026eb
  %eh.pointers.4 = call i64 @ra_eh_pointers()  ; 0x4026eb
  %eh.pointers.5 = trunc i64 %eh.pointers.4 to i32  ; 0x4026eb
  %st.p.32 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x4026eb
  store i32 %eh.pointers.5, ptr %st.p.32, align 1  ; 0x4026eb
  %sp.22 = sub i32 %rsp.phi.2, 4  ; 0x4026eb
  store i32 0, ptr %st.p.12, align 1  ; 0x4026eb
  br label %f3_402803
eh.filtered.4026eb.0:  ; 0x4026eb
  %eax.19 = trunc i64 %ret.6 to i32  ; 0x402817
  %eh.execute.2 = icmp sgt i32 %eax.19, 0  ; 0x402817
  br i1 %eh.execute.2, label %eh.except.4026eb.0, label %eh.verdict.4026eb.0
f3_402803:  ; 0x402803
  %m.10 = load i32, ptr %st.p.32, align 1  ; 0x402803
  %ecx.z.2 = zext i32 %m.10 to i64  ; 0x402803
  %m.p.11 = inttoptr i64 %ecx.z.2 to ptr  ; 0x402806
  %m.11 = load i32, ptr %m.p.11, align 1  ; 0x402806
  %eax.z.9 = zext i32 %m.11 to i64  ; 0x402806
  %m.p.12 = inttoptr i64 %eax.z.9 to ptr  ; 0x402808
  %m.12 = load i32, ptr %m.p.12, align 1  ; 0x402808
  %st.p.34 = getelementptr inbounds i8, ptr %frame.top, i64 -36  ; 0x40280a
  store i32 %m.12, ptr %st.p.34, align 1  ; 0x40280a
  %sp.23 = sub i32 %sp.22, 4  ; 0x40280d
  %st.p.35 = getelementptr inbounds i8, ptr %frame.top, i64 -64  ; 0x40280d
  store i32 %m.10, ptr %st.p.35, align 1  ; 0x40280d
  %sp.24 = sub i32 %sp.23, 4  ; 0x40280e
  %st.p.36 = getelementptr inbounds i8, ptr %frame.top, i64 -68  ; 0x40280e
  store i32 %m.12, ptr %st.p.36, align 1  ; 0x40280e
  %sp.25 = sub i32 %sp.24, 4  ; 0x40280f
  %sarg0.a.5 = add i32 %sp.25, 4
  %sarg0.5 = zext i32 %m.12 to i64
  %ret.6 = call i64 @thunk__seh_filter_exe(i64 %sarg0.5, i64 %ecx.z.2)  ; 0x40280f
  %eh.pending.4 = call i64 @ra_eh_pending()  ; 0x40280f
  %eh.thrown.4 = icmp ne i64 %eh.pending.4, 0  ; 0x40280f
  br i1 %eh.thrown.4, label %eh.landing.1, label %eh.ok.40280f.2
eh.ok.40280f.2:  ; 0x40280f
  %sum.3 = add i32 %sarg0.a.5, 8  ; 0x402814
  %sp.26 = add i32 %sum.3, 4  ; 0x402817
  br label %eh.filtered.4026eb.0
eh.except.4026eb.0:  ; 0x402817
  call void @ra_eh_handled()  ; 0x402817
  store i32 -2, ptr %st.p.1, align 1  ; 0x402817
  br label %b_402818
eh.verdict.4026eb.0:  ; 0x402817
  %eh.continue.2 = icmp slt i32 %eax.19, 0  ; 0x402817
  br i1 %eh.continue.2, label %eh.resume.4026eb.0, label %eh.search.4026eb.0
eh.resume.4026eb.0:  ; 0x402817
  call void @ra_eh_handled()  ; 0x402817
  br label %eh.probe.4026eb
eh.search.4026eb.0:  ; 0x402817
  call void @ra_eh_unhandled(i64 4204267)  ; 0x402817
  unreachable
eh.access.4026eb:  ; 0x402817
  store i32 1, ptr %m.p.9, align 1  ; 0x402817
  %sp.27 = sub i32 %rsp.phi.2, 4  ; 0x4026f5
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 868) to i32), ptr %st.p.12, align 1  ; 0x4026f5
  %sp.28 = sub i32 %sp.27, 4  ; 0x4026fa
  %st.p.41 = getelementptr inbounds i8, ptr %frame.top, i64 -64  ; 0x4026fa
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 856) to i32), ptr %st.p.41, align 1  ; 0x4026fa
  %sp.29 = sub i32 %sp.28, 4  ; 0x4026ff
  %sarg0.a.1 = add i32 %sp.29, 4
  %sarg0.1 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 856) to i32) to i64
  %sarg1 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 868) to i32) to i64
  %ret.2 = call i64 @thunk__initterm_e(i64 %sarg0.1, i64 %sarg1)  ; 0x4026ff
  %eh.pending.5 = call i64 @ra_eh_pending()  ; 0x4026ff
  %eh.thrown.5 = icmp ne i64 %eh.pending.5, 0  ; 0x4026ff
  br i1 %eh.thrown.5, label %eh.landing, label %eh.ok.4026ff
eh.ok.4026ff:  ; 0x4026ff
  %sum.4 = add i32 %sarg0.a.1, 8  ; 0x402704
  %eax.20 = trunc i64 %ret.2 to i32  ; 0x402707
  %zf.15 = icmp eq i32 %eax.20, 0  ; 0x402707
  br i1 %zf.15, label %b_402729, label %b_40270b
eh.probe.40270b:  ; 0x40270b
  %addr.14 = add i32 %sp, -4  ; 0x40270b
  %eh.at.3 = zext i32 %addr.14 to i64  ; 0x40270b
  %eh.fault.3 = call i64 @ra_eh_probe(i64 %eh.at.3, i64 4, i64 1, i64 4204299)  ; 0x40270b
  %eh.faults.3 = icmp ne i64 %eh.fault.3, 0  ; 0x40270b
  br i1 %eh.faults.3, label %eh.seh.40270b, label %eh.access.40270b
eh.seh.40270b:  ; 0x40270b
  %eh.pointers.6 = call i64 @ra_eh_pointers()  ; 0x40270b
  %eh.pointers.7 = trunc i64 %eh.pointers.6 to i32  ; 0x40270b
  %st.p.43 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x40270b
  store i32 %eh.pointers.7, ptr %st.p.43, align 1  ; 0x40270b
  store i32 0, ptr %st.p.12, align 1  ; 0x40270b
  br label %f4_402803
eh.filtered.40270b.0:  ; 0x40270b
  %eax.25 = trunc i64 %ret.3 to i32  ; 0x402817
  %eh.execute.3 = icmp sgt i32 %eax.25, 0  ; 0x402817
  br i1 %eh.execute.3, label %eh.except.40270b.0, label %eh.verdict.40270b.0
f4_402803:  ; 0x402803
  %m.13 = load i32, ptr %st.p.43, align 1  ; 0x402803
  %ecx.z.3 = zext i32 %m.13 to i64  ; 0x402803
  %m.p.14 = inttoptr i64 %ecx.z.3 to ptr  ; 0x402806
  %m.14 = load i32, ptr %m.p.14, align 1  ; 0x402806
  %eax.z.11 = zext i32 %m.14 to i64  ; 0x402806
  %m.p.15 = inttoptr i64 %eax.z.11 to ptr  ; 0x402808
  %m.15 = load i32, ptr %m.p.15, align 1  ; 0x402808
  %st.p.45 = getelementptr inbounds i8, ptr %frame.top, i64 -36  ; 0x40280a
  store i32 %m.15, ptr %st.p.45, align 1  ; 0x40280a
  store i32 %m.13, ptr %st.p.41, align 1  ; 0x40280d
  %st.p.47 = getelementptr inbounds i8, ptr %frame.top, i64 -68  ; 0x40280e
  store i32 %m.15, ptr %st.p.47, align 1  ; 0x40280e
  %sarg0.2 = zext i32 %m.15 to i64
  %ret.3 = call i64 @thunk__seh_filter_exe(i64 %sarg0.2, i64 %ecx.z.3)  ; 0x40280f
  %eh.pending.6 = call i64 @ra_eh_pending()  ; 0x40280f
  %eh.thrown.6 = icmp ne i64 %eh.pending.6, 0  ; 0x40280f
  br i1 %eh.thrown.6, label %eh.landing.1, label %eh.ok.40280f.3
eh.ok.40280f.3:  ; 0x40280f
  br label %eh.filtered.40270b.0
eh.except.40270b.0:  ; 0x402817
  call void @ra_eh_handled()  ; 0x402817
  store i32 -2, ptr %st.p.1, align 1  ; 0x402817
  br label %b_402818
eh.verdict.40270b.0:  ; 0x402817
  %eh.continue.3 = icmp slt i32 %eax.25, 0  ; 0x402817
  br i1 %eh.continue.3, label %eh.resume.40270b.0, label %eh.search.40270b.0
eh.resume.40270b.0:  ; 0x402817
  call void @ra_eh_handled()  ; 0x402817
  br label %eh.probe.40270b
eh.search.40270b.0:  ; 0x402817
  call void @ra_eh_unhandled(i64 4204299)  ; 0x402817
  unreachable
eh.access.40270b:  ; 0x402817
  store i32 -2, ptr %st.p.1, align 1  ; 0x402817
  %m.16 = load i32, ptr %st.p.4, align 1  ; 0x402717
  %teb.4 = call i64 @ra_teb()  ; 0x40271a
  %teb.5 = trunc i64 %teb.4 to i32  ; 0x40271a
  %addr.z.72 = zext i32 %teb.5 to i64  ; 0x40271a
  %st.p.51 = inttoptr i64 %addr.z.72 to ptr  ; 0x40271a
  store i32 %m.16, ptr %st.p.51, align 1  ; 0x40271a
  ret i64 255
eh.ok.402733:  ; 0x402733
  %sum.6 = add i32 %sarg0.a.3, 8  ; 0x402738
  br label %eh.probe.40273b
eh.probe.40273b:  ; 0x40273b
  %rsp.phi.4 = phi i32 [ %sum.6, %eh.ok.402733 ], [ %sp.48, %eh.resume.40273b.0 ]  ; 0x40273b
  %eh.fault.4 = call i64 @ra_eh_probe(i64 %eh.at.1, i64 4, i64 1, i64 4204347)  ; 0x40273b
  %eh.faults.4 = icmp ne i64 %eh.fault.4, 0  ; 0x40273b
  br i1 %eh.faults.4, label %eh.seh.40273b, label %eh.access.40273b
eh.seh.40273b:  ; 0x40273b
  %eh.pointers.8 = call i64 @ra_eh_pointers()  ; 0x40273b
  %eh.pointers.9 = trunc i64 %eh.pointers.8 to i32  ; 0x40273b
  %st.p.55 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x40273b
  store i32 %eh.pointers.9, ptr %st.p.55, align 1  ; 0x40273b
  %sp.44 = sub i32 %rsp.phi.4, 4  ; 0x40273b
  store i32 0, ptr %st.p.12, align 1  ; 0x40273b
  br label %f5_402803
eh.filtered.40273b.0:  ; 0x40273b
  %eax.29 = trunc i64 %ret.5 to i32  ; 0x402817
  %eh.execute.4 = icmp sgt i32 %eax.29, 0  ; 0x402817
  br i1 %eh.execute.4, label %eh.except.40273b.0, label %eh.verdict.40273b.0
f5_402803:  ; 0x402803
  %m.17 = load i32, ptr %st.p.55, align 1  ; 0x402803
  %ecx.z.6 = zext i32 %m.17 to i64  ; 0x402803
  %m.p.18 = inttoptr i64 %ecx.z.6 to ptr  ; 0x402806
  %m.18 = load i32, ptr %m.p.18, align 1  ; 0x402806
  %eax.z.14 = zext i32 %m.18 to i64  ; 0x402806
  %m.p.19 = inttoptr i64 %eax.z.14 to ptr  ; 0x402808
  %m.19 = load i32, ptr %m.p.19, align 1  ; 0x402808
  %st.p.57 = getelementptr inbounds i8, ptr %frame.top, i64 -36  ; 0x40280a
  store i32 %m.19, ptr %st.p.57, align 1  ; 0x40280a
  %sp.45 = sub i32 %sp.44, 4  ; 0x40280d
  store i32 %m.17, ptr %st.p.41, align 1  ; 0x40280d
  %sp.46 = sub i32 %sp.45, 4  ; 0x40280e
  %st.p.59 = getelementptr inbounds i8, ptr %frame.top, i64 -68  ; 0x40280e
  store i32 %m.19, ptr %st.p.59, align 1  ; 0x40280e
  %sp.47 = sub i32 %sp.46, 4  ; 0x40280f
  %sarg0.a.4 = add i32 %sp.47, 4
  %sarg0.4 = zext i32 %m.19 to i64
  %ret.5 = call i64 @thunk__seh_filter_exe(i64 %sarg0.4, i64 %ecx.z.6)  ; 0x40280f
  %eh.pending.8 = call i64 @ra_eh_pending()  ; 0x40280f
  %eh.thrown.8 = icmp ne i64 %eh.pending.8, 0  ; 0x40280f
  br i1 %eh.thrown.8, label %eh.landing.1, label %eh.ok.40280f.4
eh.ok.40280f.4:  ; 0x40280f
  %sum.7 = add i32 %sarg0.a.4, 8  ; 0x402814
  %sp.48 = add i32 %sum.7, 4  ; 0x402817
  br label %eh.filtered.40273b.0
eh.except.40273b.0:  ; 0x402817
  call void @ra_eh_handled()  ; 0x402817
  store i32 -2, ptr %st.p.1, align 1  ; 0x402817
  br label %b_402818
eh.verdict.40273b.0:  ; 0x402817
  %eh.continue.4 = icmp slt i32 %eax.29, 0  ; 0x402817
  br i1 %eh.continue.4, label %eh.resume.40273b.0, label %eh.search.40273b.0
eh.resume.40273b.0:  ; 0x402817
  call void @ra_eh_handled()  ; 0x402817
  br label %eh.probe.40273b
eh.search.40273b.0:  ; 0x402817
  call void @ra_eh_unhandled(i64 4204347)  ; 0x402817
  unreachable
eh.access.40273b:  ; 0x402817
  store i32 2, ptr %m.p.9, align 1  ; 0x402817
  br label %b_40274c
eh.probe.402749:  ; 0x402749
  %rsp.phi.5 = phi i32 [ %rsp.phi.1, %b_402747 ], [ %sp.53, %eh.resume.402749.0 ]  ; 0x402749
  %addr.21 = add i32 %sp, -25  ; 0x402749
  %eh.at.5 = zext i32 %addr.21 to i64  ; 0x402749
  %eh.fault.5 = call i64 @ra_eh_probe(i64 %eh.at.5, i64 1, i64 1, i64 4204361)  ; 0x402749
  %eh.faults.5 = icmp ne i64 %eh.fault.5, 0  ; 0x402749
  br i1 %eh.faults.5, label %eh.seh.402749, label %eh.access.402749
eh.seh.402749:  ; 0x402749
  %eh.pointers.10 = call i64 @ra_eh_pointers()  ; 0x402749
  %eh.pointers.11 = trunc i64 %eh.pointers.10 to i32  ; 0x402749
  %st.p.63 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x402749
  store i32 %eh.pointers.11, ptr %st.p.63, align 1  ; 0x402749
  %sp.49 = sub i32 %rsp.phi.5, 4  ; 0x402749
  store i32 0, ptr %st.p.12, align 1  ; 0x402749
  br label %f6_402803
eh.filtered.402749.0:  ; 0x402749
  %eax.33 = trunc i64 %ret.30 to i32  ; 0x402817
  %eh.execute.5 = icmp sgt i32 %eax.33, 0  ; 0x402817
  br i1 %eh.execute.5, label %eh.except.402749.0, label %eh.verdict.402749.0
f6_402803:  ; 0x402803
  %m.20 = load i32, ptr %st.p.63, align 1  ; 0x402803
  %ecx.z.7 = zext i32 %m.20 to i64  ; 0x402803
  %m.p.21 = inttoptr i64 %ecx.z.7 to ptr  ; 0x402806
  %m.21 = load i32, ptr %m.p.21, align 1  ; 0x402806
  %eax.z.16 = zext i32 %m.21 to i64  ; 0x402806
  %m.p.22 = inttoptr i64 %eax.z.16 to ptr  ; 0x402808
  %m.22 = load i32, ptr %m.p.22, align 1  ; 0x402808
  %st.p.65 = getelementptr inbounds i8, ptr %frame.top, i64 -36  ; 0x40280a
  store i32 %m.22, ptr %st.p.65, align 1  ; 0x40280a
  %sp.50 = sub i32 %sp.49, 4  ; 0x40280d
  %st.p.66 = getelementptr inbounds i8, ptr %frame.top, i64 -64  ; 0x40280d
  store i32 %m.20, ptr %st.p.66, align 1  ; 0x40280d
  %sp.51 = sub i32 %sp.50, 4  ; 0x40280e
  %st.p.67 = getelementptr inbounds i8, ptr %frame.top, i64 -68  ; 0x40280e
  store i32 %m.22, ptr %st.p.67, align 1  ; 0x40280e
  %sp.52 = sub i32 %sp.51, 4  ; 0x40280f
  %sarg0.a.21 = add i32 %sp.52, 4
  %sarg0.21 = zext i32 %m.22 to i64
  %ret.30 = call i64 @thunk__seh_filter_exe(i64 %sarg0.21, i64 %ecx.z.7)  ; 0x40280f
  %eh.pending.9 = call i64 @ra_eh_pending()  ; 0x40280f
  %eh.thrown.9 = icmp ne i64 %eh.pending.9, 0  ; 0x40280f
  br i1 %eh.thrown.9, label %eh.landing.1, label %eh.ok.40280f.5
eh.ok.40280f.5:  ; 0x40280f
  %sum.8 = add i32 %sarg0.a.21, 8  ; 0x402814
  %sp.53 = add i32 %sum.8, 4  ; 0x402817
  br label %eh.filtered.402749.0
eh.except.402749.0:  ; 0x402817
  call void @ra_eh_handled()  ; 0x402817
  store i32 -2, ptr %st.p.1, align 1  ; 0x402817
  br label %b_402818
eh.verdict.402749.0:  ; 0x402817
  %eh.continue.5 = icmp slt i32 %eax.33, 0  ; 0x402817
  br i1 %eh.continue.5, label %eh.resume.402749.0, label %eh.search.402749.0
eh.resume.402749.0:  ; 0x402817
  call void @ra_eh_handled()  ; 0x402817
  br label %eh.probe.402749
eh.search.402749.0:  ; 0x402817
  call void @ra_eh_unhandled(i64 4204361)  ; 0x402817
  unreachable
eh.access.402749:  ; 0x402817
  store i8 1, ptr %st.p.14, align 1  ; 0x402817
  br label %b_40274c
eh.probe.40274c:  ; 0x40274c
  %rsp.phi.7 = phi i32 [ %rsp.phi.6, %b_40274c ], [ %sp.58, %eh.resume.40274c.0 ]  ; 0x40274c
  %addr.25 = add i32 %sp, -36  ; 0x40274c
  %eh.at.6 = zext i32 %addr.25 to i64  ; 0x40274c
  %eh.fault.6 = call i64 @ra_eh_probe(i64 %eh.at.6, i64 4, i64 0, i64 4204364)  ; 0x40274c
  %eh.faults.6 = icmp ne i64 %eh.fault.6, 0  ; 0x40274c
  br i1 %eh.faults.6, label %eh.seh.40274c, label %eh.access.40274c
eh.seh.40274c:  ; 0x40274c
  %eh.pointers.12 = call i64 @ra_eh_pointers()  ; 0x40274c
  %eh.pointers.13 = trunc i64 %eh.pointers.12 to i32  ; 0x40274c
  %st.p.71 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x40274c
  store i32 %eh.pointers.13, ptr %st.p.71, align 1  ; 0x40274c
  %sp.54 = sub i32 %rsp.phi.7, 4  ; 0x40274c
  store i32 0, ptr %st.p.12, align 1  ; 0x40274c
  br label %f7_402803
eh.filtered.40274c.0:  ; 0x40274c
  %eax.37 = trunc i64 %ret.29 to i32  ; 0x402817
  %eh.execute.6 = icmp sgt i32 %eax.37, 0  ; 0x402817
  br i1 %eh.execute.6, label %eh.except.40274c.0, label %eh.verdict.40274c.0
f7_402803:  ; 0x402803
  %m.23 = load i32, ptr %st.p.71, align 1  ; 0x402803
  %ecx.z.8 = zext i32 %m.23 to i64  ; 0x402803
  %m.p.24 = inttoptr i64 %ecx.z.8 to ptr  ; 0x402806
  %m.24 = load i32, ptr %m.p.24, align 1  ; 0x402806
  %eax.z.18 = zext i32 %m.24 to i64  ; 0x402806
  %m.p.25 = inttoptr i64 %eax.z.18 to ptr  ; 0x402808
  %m.25 = load i32, ptr %m.p.25, align 1  ; 0x402808
  %st.p.73 = getelementptr inbounds i8, ptr %frame.top, i64 -36  ; 0x40280a
  store i32 %m.25, ptr %st.p.73, align 1  ; 0x40280a
  %sp.55 = sub i32 %sp.54, 4  ; 0x40280d
  %st.p.74 = getelementptr inbounds i8, ptr %frame.top, i64 -64  ; 0x40280d
  store i32 %m.23, ptr %st.p.74, align 1  ; 0x40280d
  %sp.56 = sub i32 %sp.55, 4  ; 0x40280e
  %st.p.75 = getelementptr inbounds i8, ptr %frame.top, i64 -68  ; 0x40280e
  store i32 %m.25, ptr %st.p.75, align 1  ; 0x40280e
  %sp.57 = sub i32 %sp.56, 4  ; 0x40280f
  %sarg0.a.20 = add i32 %sp.57, 4
  %sarg0.20 = zext i32 %m.25 to i64
  %ret.29 = call i64 @thunk__seh_filter_exe(i64 %sarg0.20, i64 %ecx.z.8)  ; 0x40280f
  %eh.pending.10 = call i64 @ra_eh_pending()  ; 0x40280f
  %eh.thrown.10 = icmp ne i64 %eh.pending.10, 0  ; 0x40280f
  br i1 %eh.thrown.10, label %eh.landing.1, label %eh.ok.40280f.6
eh.ok.40280f.6:  ; 0x40280f
  %sum.9 = add i32 %sarg0.a.20, 8  ; 0x402814
  %sp.58 = add i32 %sum.9, 4  ; 0x402817
  br label %eh.filtered.40274c.0
eh.except.40274c.0:  ; 0x402817
  call void @ra_eh_handled()  ; 0x402817
  store i32 -2, ptr %st.p.1, align 1  ; 0x402817
  br label %b_402818
eh.verdict.40274c.0:  ; 0x402817
  %eh.continue.6 = icmp slt i32 %eax.37, 0  ; 0x402817
  br i1 %eh.continue.6, label %eh.resume.40274c.0, label %eh.search.40274c.0
eh.resume.40274c.0:  ; 0x402817
  call void @ra_eh_handled()  ; 0x402817
  br label %eh.probe.40274c
eh.search.40274c.0:  ; 0x402817
  call void @ra_eh_unhandled(i64 4204364)  ; 0x402817
  unreachable
eh.access.40274c:  ; 0x402817
  %m.26 = load i32, ptr %st.p.24, align 1  ; 0x402817
  %sp.59 = sub i32 %rsp.phi.7, 4  ; 0x402817
  store i32 %m.26, ptr %st.p.12, align 1  ; 0x402817
  %sp.60 = sub i32 %sp.59, 4  ; 0x40274f
  %sarg0.a.6 = add i32 %sp.60, 4
  call void @F_004029f0(i32 %m.26)  ; 0x40274f
  %eh.pending.11 = call i64 @ra_eh_pending()  ; 0x40274f
  %eh.thrown.11 = icmp ne i64 %eh.pending.11, 0  ; 0x40274f
  br i1 %eh.thrown.11, label %eh.landing, label %eh.ok.40274f
eh.ok.40274f:  ; 0x40274f
  %sum.10 = add i32 %sarg0.a.6, 4  ; 0x402754
  %sp.61 = sub i32 %sum.10, 4  ; 0x402757
  %ret.7 = call i32 @F_00402d10()  ; 0x402757
  %rsp.183 = add i32 %sp.61, 4  ; 0x402757
  %eh.pending.12 = call i64 @ra_eh_pending()  ; 0x402757
  %eh.thrown.12 = icmp ne i64 %eh.pending.12, 0  ; 0x402757
  br i1 %eh.thrown.12, label %eh.landing, label %eh.ok.402757
eh.ok.402757:  ; 0x402757
  br label %eh.probe.40275e
eh.probe.40275e:  ; 0x40275e
  %rsp.phi.8 = phi i32 [ %rsp.183, %eh.ok.402757 ], [ %sp.66, %eh.resume.40275e.0 ]  ; 0x40275e
  %eh.at.7 = zext i32 %ret.7 to i64  ; 0x40275e
  %eh.fault.7 = call i64 @ra_eh_probe(i64 %eh.at.7, i64 4, i64 0, i64 4204382)  ; 0x40275e
  %eh.faults.7 = icmp ne i64 %eh.fault.7, 0  ; 0x40275e
  br i1 %eh.faults.7, label %eh.seh.40275e, label %eh.access.40275e
eh.seh.40275e:  ; 0x40275e
  %eh.pointers.14 = call i64 @ra_eh_pointers()  ; 0x40275e
  %eh.pointers.15 = trunc i64 %eh.pointers.14 to i32  ; 0x40275e
  %st.p.81 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x40275e
  store i32 %eh.pointers.15, ptr %st.p.81, align 1  ; 0x40275e
  %sp.62 = sub i32 %rsp.phi.8, 4  ; 0x40275e
  store i32 0, ptr %st.p.12, align 1  ; 0x40275e
  br label %f8_402803
eh.filtered.40275e.0:  ; 0x40275e
  %eax.42 = trunc i64 %ret.28 to i32  ; 0x402817
  %eh.execute.7 = icmp sgt i32 %eax.42, 0  ; 0x402817
  br i1 %eh.execute.7, label %eh.except.40275e.0, label %eh.verdict.40275e.0
f8_402803:  ; 0x402803
  %m.27 = load i32, ptr %st.p.81, align 1  ; 0x402803
  %ecx.z.9 = zext i32 %m.27 to i64  ; 0x402803
  %m.p.28 = inttoptr i64 %ecx.z.9 to ptr  ; 0x402806
  %m.28 = load i32, ptr %m.p.28, align 1  ; 0x402806
  %eax.z.20 = zext i32 %m.28 to i64  ; 0x402806
  %m.p.29 = inttoptr i64 %eax.z.20 to ptr  ; 0x402808
  %m.29 = load i32, ptr %m.p.29, align 1  ; 0x402808
  %st.p.83 = getelementptr inbounds i8, ptr %frame.top, i64 -36  ; 0x40280a
  store i32 %m.29, ptr %st.p.83, align 1  ; 0x40280a
  %sp.63 = sub i32 %sp.62, 4  ; 0x40280d
  %st.p.84 = getelementptr inbounds i8, ptr %frame.top, i64 -64  ; 0x40280d
  store i32 %m.27, ptr %st.p.84, align 1  ; 0x40280d
  %sp.64 = sub i32 %sp.63, 4  ; 0x40280e
  %st.p.85 = getelementptr inbounds i8, ptr %frame.top, i64 -68  ; 0x40280e
  store i32 %m.29, ptr %st.p.85, align 1  ; 0x40280e
  %sp.65 = sub i32 %sp.64, 4  ; 0x40280f
  %sarg0.a.19 = add i32 %sp.65, 4
  %sarg0.19 = zext i32 %m.29 to i64
  %ret.28 = call i64 @thunk__seh_filter_exe(i64 %sarg0.19, i64 %ecx.z.9)  ; 0x40280f
  %eh.pending.13 = call i64 @ra_eh_pending()  ; 0x40280f
  %eh.thrown.13 = icmp ne i64 %eh.pending.13, 0  ; 0x40280f
  br i1 %eh.thrown.13, label %eh.landing.1, label %eh.ok.40280f.7
eh.ok.40280f.7:  ; 0x40280f
  %sum.11 = add i32 %sarg0.a.19, 8  ; 0x402814
  %sp.66 = add i32 %sum.11, 4  ; 0x402817
  br label %eh.filtered.40275e.0
eh.except.40275e.0:  ; 0x402817
  call void @ra_eh_handled()  ; 0x402817
  store i32 -2, ptr %st.p.1, align 1  ; 0x402817
  br label %b_402818
eh.verdict.40275e.0:  ; 0x402817
  %eh.continue.7 = icmp slt i32 %eax.42, 0  ; 0x402817
  br i1 %eh.continue.7, label %eh.resume.40275e.0, label %eh.search.40275e.0
eh.resume.40275e.0:  ; 0x402817
  call void @ra_eh_handled()  ; 0x402817
  br label %eh.probe.40275e
eh.search.40275e.0:  ; 0x402817
  call void @ra_eh_unhandled(i64 4204382)  ; 0x402817
  unreachable
eh.access.40275e:  ; 0x402817
  %m.p.30 = inttoptr i64 %eh.at.7 to ptr  ; 0x402817
  %m.30 = load i32, ptr %m.p.30, align 1  ; 0x402817
  %zf.24 = icmp eq i32 %m.30, 0  ; 0x402817
  br i1 %zf.24, label %b_402782, label %b_402763
eh.ok.402764:  ; 0x402764
  %sum.12 = add i32 %varargs.a, 4  ; 0x402769
  %al.3 = trunc i32 %ret.8 to i8  ; 0x40276c
  %zf.27 = icmp eq i8 %al.3, 0  ; 0x40276c
  br i1 %zf.27, label %b_402782, label %b_402770
eh.probe.402770:  ; 0x402770
  %rsp.phi.9 = phi i32 [ %sum.12, %b_402770 ], [ %sp.73, %eh.resume.402770.0 ]  ; 0x402770
  %eh.fault.8 = call i64 @ra_eh_probe(i64 %eh.at.7, i64 4, i64 0, i64 4204400)  ; 0x402770
  %eh.faults.8 = icmp ne i64 %eh.fault.8, 0  ; 0x402770
  br i1 %eh.faults.8, label %eh.seh.402770, label %eh.access.402770
eh.seh.402770:  ; 0x402770
  %eh.pointers.16 = call i64 @ra_eh_pointers()  ; 0x402770
  %eh.pointers.17 = trunc i64 %eh.pointers.16 to i32  ; 0x402770
  %st.p.90 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x402770
  store i32 %eh.pointers.17, ptr %st.p.90, align 1  ; 0x402770
  %sp.69 = sub i32 %rsp.phi.9, 4  ; 0x402770
  store i32 0, ptr %st.p.12, align 1  ; 0x402770
  br label %f9_402803
eh.filtered.402770.0:  ; 0x402770
  %eax.46 = trunc i64 %ret.12 to i32  ; 0x402817
  %eh.execute.8 = icmp sgt i32 %eax.46, 0  ; 0x402817
  br i1 %eh.execute.8, label %eh.except.402770.0, label %eh.verdict.402770.0
f9_402803:  ; 0x402803
  %m.31 = load i32, ptr %st.p.90, align 1  ; 0x402803
  %ecx.z.10 = zext i32 %m.31 to i64  ; 0x402803
  %m.p.32 = inttoptr i64 %ecx.z.10 to ptr  ; 0x402806
  %m.32 = load i32, ptr %m.p.32, align 1  ; 0x402806
  %eax.z.22 = zext i32 %m.32 to i64  ; 0x402806
  %m.p.33 = inttoptr i64 %eax.z.22 to ptr  ; 0x402808
  %m.33 = load i32, ptr %m.p.33, align 1  ; 0x402808
  %st.p.92 = getelementptr inbounds i8, ptr %frame.top, i64 -36  ; 0x40280a
  store i32 %m.33, ptr %st.p.92, align 1  ; 0x40280a
  %sp.70 = sub i32 %sp.69, 4  ; 0x40280d
  %st.p.93 = getelementptr inbounds i8, ptr %frame.top, i64 -64  ; 0x40280d
  store i32 %m.31, ptr %st.p.93, align 1  ; 0x40280d
  %sp.71 = sub i32 %sp.70, 4  ; 0x40280e
  %st.p.94 = getelementptr inbounds i8, ptr %frame.top, i64 -68  ; 0x40280e
  store i32 %m.33, ptr %st.p.94, align 1  ; 0x40280e
  %sp.72 = sub i32 %sp.71, 4  ; 0x40280f
  %sarg0.a.10 = add i32 %sp.72, 4
  %sarg0.10 = zext i32 %m.33 to i64
  %ret.12 = call i64 @thunk__seh_filter_exe(i64 %sarg0.10, i64 %ecx.z.10)  ; 0x40280f
  %eh.pending.15 = call i64 @ra_eh_pending()  ; 0x40280f
  %eh.thrown.15 = icmp ne i64 %eh.pending.15, 0  ; 0x40280f
  br i1 %eh.thrown.15, label %eh.landing.1, label %eh.ok.40280f.8
eh.ok.40280f.8:  ; 0x40280f
  %sum.13 = add i32 %sarg0.a.10, 8  ; 0x402814
  %sp.73 = add i32 %sum.13, 4  ; 0x402817
  br label %eh.filtered.402770.0
eh.except.402770.0:  ; 0x402817
  call void @ra_eh_handled()  ; 0x402817
  store i32 -2, ptr %st.p.1, align 1  ; 0x402817
  br label %b_402818
eh.verdict.402770.0:  ; 0x402817
  %eh.continue.8 = icmp slt i32 %eax.46, 0  ; 0x402817
  br i1 %eh.continue.8, label %eh.resume.402770.0, label %eh.search.402770.0
eh.resume.402770.0:  ; 0x402817
  call void @ra_eh_handled()  ; 0x402817
  br label %eh.probe.402770
eh.search.402770.0:  ; 0x402817
  call void @ra_eh_unhandled(i64 4204400)  ; 0x402817
  unreachable
eh.access.402770:  ; 0x402817
  %m.34 = load i32, ptr %m.p.30, align 1  ; 0x402817
  %esi.z.2 = zext i32 %m.34 to i64  ; 0x402817
  %sp.74 = sub i32 %rsp.phi.9, 4  ; 0x402772
  store i32 0, ptr %st.p.12, align 1  ; 0x402772
  %sp.75 = sub i32 %sp.74, 4  ; 0x402774
  %st.p.98 = getelementptr inbounds i8, ptr %frame.top, i64 -64  ; 0x402774
  store i32 2, ptr %st.p.98, align 1  ; 0x402774
  %sp.76 = sub i32 %sp.75, 4  ; 0x402776
  %st.p.99 = getelementptr inbounds i8, ptr %frame.top, i64 -68  ; 0x402776
  store i32 0, ptr %st.p.99, align 1  ; 0x402776
  br label %eh.probe.40277a
eh.probe.40277a:  ; 0x40277a
  %rcx.phi = phi i64 [ %esi.z.2, %eh.access.402770 ], [ undef, %eh.resume.40277a.0 ]  ; 0x40277a
  %rsp.phi.10 = phi i32 [ %sp.76, %eh.access.402770 ], [ %sp.81, %eh.resume.40277a.0 ]  ; 0x40277a
  %eh.at.9 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 836) to i32) to i64  ; 0x40277a
  %eh.fault.9 = call i64 @ra_eh_probe(i64 %eh.at.9, i64 4, i64 0, i64 4204410)  ; 0x40277a
  %eh.faults.9 = icmp ne i64 %eh.fault.9, 0  ; 0x40277a
  br i1 %eh.faults.9, label %eh.seh.40277a, label %eh.access.40277a
eh.seh.40277a:  ; 0x40277a
  %eh.pointers.18 = call i64 @ra_eh_pointers()  ; 0x40277a
  %eh.pointers.19 = trunc i64 %eh.pointers.18 to i32  ; 0x40277a
  %st.p.100 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x40277a
  store i32 %eh.pointers.19, ptr %st.p.100, align 1  ; 0x40277a
  %sp.77 = sub i32 %rsp.phi.10, 4  ; 0x40277a
  %st.p.101 = getelementptr inbounds i8, ptr %frame.top, i64 -72  ; 0x40277a
  store i32 0, ptr %st.p.101, align 1  ; 0x40277a
  br label %f10_402803
eh.filtered.40277a.0:  ; 0x40277a
  %eax.50 = trunc i64 %ret.11 to i32  ; 0x402817
  %eh.execute.9 = icmp sgt i32 %eax.50, 0  ; 0x402817
  br i1 %eh.execute.9, label %eh.except.40277a.0, label %eh.verdict.40277a.0
f10_402803:  ; 0x402803
  %m.35 = load i32, ptr %st.p.100, align 1  ; 0x402803
  %ecx.z.12 = zext i32 %m.35 to i64  ; 0x402803
  %m.p.36 = inttoptr i64 %ecx.z.12 to ptr  ; 0x402806
  %m.36 = load i32, ptr %m.p.36, align 1  ; 0x402806
  %eax.z.24 = zext i32 %m.36 to i64  ; 0x402806
  %m.p.37 = inttoptr i64 %eax.z.24 to ptr  ; 0x402808
  %m.37 = load i32, ptr %m.p.37, align 1  ; 0x402808
  %st.p.102 = getelementptr inbounds i8, ptr %frame.top, i64 -36  ; 0x40280a
  store i32 %m.37, ptr %st.p.102, align 1  ; 0x40280a
  %sp.78 = sub i32 %sp.77, 4  ; 0x40280d
  %st.p.103 = getelementptr inbounds i8, ptr %frame.top, i64 -76  ; 0x40280d
  store i32 %m.35, ptr %st.p.103, align 1  ; 0x40280d
  %sp.79 = sub i32 %sp.78, 4  ; 0x40280e
  %st.p.104 = getelementptr inbounds i8, ptr %frame.top, i64 -80  ; 0x40280e
  store i32 %m.37, ptr %st.p.104, align 1  ; 0x40280e
  %sp.80 = sub i32 %sp.79, 4  ; 0x40280f
  %sarg0.a.9 = add i32 %sp.80, 4
  %sarg0.9 = zext i32 %m.37 to i64
  %ret.11 = call i64 @thunk__seh_filter_exe(i64 %sarg0.9, i64 %ecx.z.12)  ; 0x40280f
  %eh.pending.16 = call i64 @ra_eh_pending()  ; 0x40280f
  %eh.thrown.16 = icmp ne i64 %eh.pending.16, 0  ; 0x40280f
  br i1 %eh.thrown.16, label %eh.landing.1, label %eh.ok.40280f.9
eh.ok.40280f.9:  ; 0x40280f
  %sum.14 = add i32 %sarg0.a.9, 8  ; 0x402814
  %sp.81 = add i32 %sum.14, 4  ; 0x402817
  br label %eh.filtered.40277a.0
eh.except.40277a.0:  ; 0x402817
  call void @ra_eh_handled()  ; 0x402817
  store i32 -2, ptr %st.p.1, align 1  ; 0x402817
  br label %b_402818
eh.verdict.40277a.0:  ; 0x402817
  %eh.continue.9 = icmp slt i32 %eax.50, 0  ; 0x402817
  br i1 %eh.continue.9, label %eh.resume.40277a.0, label %eh.search.40277a.0
eh.resume.40277a.0:  ; 0x402817
  call void @ra_eh_handled()  ; 0x402817
  br label %eh.probe.40277a
eh.search.40277a.0:  ; 0x402817
  call void @ra_eh_unhandled(i64 4204410)  ; 0x402817
  unreachable
eh.access.40277a:  ; 0x402817
  %m.p.38 = inttoptr i64 %eh.at.9 to ptr  ; 0x402817
  %m.38 = load i32, ptr %m.p.38, align 1  ; 0x402817
  %target = zext i32 %m.38 to i64  ; 0x402817
  %sp.82 = sub i32 %rsp.phi.10, 4  ; 0x402817
  %st.p.107 = getelementptr inbounds i8, ptr %frame.top, i64 -72  ; 0x402817
  store i32 4204416, ptr %st.p.107, align 1  ; 0x402817
  %fp = inttoptr i64 %target to ptr  ; 0x402817
  %sarg0.d.7 = load i32, ptr %st.p.99, align 4
  %sarg0.7 = zext i32 %sarg0.d.7 to i64
  %sarg1.d.5 = load i32, ptr %st.p.98, align 4
  %sarg1.5 = zext i32 %sarg1.d.5 to i64
  %sarg2.d = load i32, ptr %st.p.12, align 4
  %sarg2 = zext i32 %sarg2.d to i64
  %sarg3.d = load i32, ptr %st.p.9, align 4
  %sarg3 = zext i32 %sarg3.d to i64
  %sarg4.d = load i32, ptr %st.p.7, align 4
  %sarg4 = zext i32 %sarg4.d to i64
  %sarg5.d = load i32, ptr %st.p.6, align 4
  %sarg5 = zext i32 %sarg5.d to i64
  %sarg6.d = load i32, ptr %st.p.5, align 4
  %sarg6 = zext i32 %sarg6.d to i64
  %args.p = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr  ; 0x402817
  store i64 %sarg6, ptr %args.p, align 8  ; 0x402817
  %sarg7.d = load i32, ptr %st.p.24, align 4
  %sarg7 = zext i32 %sarg7.d to i64
  %args.p.1 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr  ; 0x402817
  store i64 %sarg7, ptr %args.p.1, align 8  ; 0x402817
  %ret.9 = call i64 %fp(i64 %sarg0.7, i64 %sarg1.5, i64 %sarg2, i64 %sarg3, i64 %sarg4, i64 %sarg5, i64 %rcx.phi, i64 undef)  ; 0x402817
  %pops = call i64 @ra_pops_of(i64 %target)  ; 0x402817
  %n.2 = trunc i64 %pops to i32
  %pops.a = add i32 %n.2, 4  ; 0x402817
  %rsp.185 = add i32 %sp.82, %pops.a  ; 0x402817
  %rsp.185.s.w = zext i32 %rsp.185 to i64  ; 0x402817
  %rsp.185.s = inttoptr i64 %rsp.185.s.w to ptr  ; 0x402817
  %ret.hi = lshr i64 %ret.9, 32  ; 0x402817
  %eh.pending.17 = call i64 @ra_eh_pending()  ; 0x402817
  %eh.thrown.17 = icmp ne i64 %eh.pending.17, 0  ; 0x402817
  br i1 %eh.thrown.17, label %eh.landing, label %eh.ok.40277a
eh.ok.40277a:  ; 0x402817
  %sp.83 = sub i32 %rsp.185, 4  ; 0x402780
  %fp.1 = inttoptr i64 %esi.z.2 to ptr  ; 0x402780
  %rsp.185.f0 = getelementptr inbounds %S3, ptr %rsp.185.s, i64 0, i32 0
  %sarg0.d.8 = load i32, ptr %rsp.185.f0, align 4
  %sarg0.8 = zext i32 %sarg0.d.8 to i64
  %rsp.185.f4 = getelementptr inbounds %S3, ptr %rsp.185.s, i64 0, i32 1
  %sarg1.d.6 = load i32, ptr %rsp.185.f4, align 4
  %sarg1.6 = zext i32 %sarg1.d.6 to i64
  %rsp.185.f8 = getelementptr inbounds %S3, ptr %rsp.185.s, i64 0, i32 2
  %sarg2.d.1 = load i32, ptr %rsp.185.f8, align 4
  %sarg2.1 = zext i32 %sarg2.d.1 to i64
  %rsp.185.f12 = getelementptr inbounds %S3, ptr %rsp.185.s, i64 0, i32 3
  %sarg3.d.1 = load i32, ptr %rsp.185.f12, align 4
  %sarg3.1 = zext i32 %sarg3.d.1 to i64
  %rsp.185.f16 = getelementptr inbounds %S3, ptr %rsp.185.s, i64 0, i32 4
  %sarg4.d.1 = load i32, ptr %rsp.185.f16, align 4
  %sarg4.1 = zext i32 %sarg4.d.1 to i64
  %rsp.185.f20 = getelementptr inbounds %S3, ptr %rsp.185.s, i64 0, i32 5
  %sarg5.d.1 = load i32, ptr %rsp.185.f20, align 4
  %sarg5.1 = zext i32 %sarg5.d.1 to i64
  %rsp.185.f24 = getelementptr inbounds %S3, ptr %rsp.185.s, i64 0, i32 6
  %sarg6.d.1 = load i32, ptr %rsp.185.f24, align 4
  %sarg6.1 = zext i32 %sarg6.d.1 to i64
  store i64 %sarg6.1, ptr %args.p, align 8  ; 0x402780
  %rsp.185.f28 = getelementptr inbounds %S3, ptr %rsp.185.s, i64 0, i32 7
  %sarg7.d.1 = load i32, ptr %rsp.185.f28, align 4
  %sarg7.1 = zext i32 %sarg7.d.1 to i64
  store i64 %sarg7.1, ptr %args.p.1, align 8  ; 0x402780
  %ret.10 = call i64 %fp.1(i64 %sarg0.8, i64 %sarg1.6, i64 %sarg2.1, i64 %sarg3.1, i64 %sarg4.1, i64 %sarg5.1, i64 undef, i64 %ret.hi)  ; 0x402780
  %pops.1 = call i64 @ra_pops_of(i64 %esi.z.2)  ; 0x402780
  %n.3 = trunc i64 %pops.1 to i32
  %pops.a.1 = add i32 %n.3, 4  ; 0x402780
  %rsp.186 = add i32 %sp.83, %pops.a.1  ; 0x402780
  %eh.pending.18 = call i64 @ra_eh_pending()  ; 0x402780
  %eh.thrown.18 = icmp ne i64 %eh.pending.18, 0  ; 0x402780
  br i1 %eh.thrown.18, label %eh.landing, label %eh.ok.402780
eh.ok.402780:  ; 0x402780
  br label %b_402782
eh.ok.402782:  ; 0x402782
  br label %eh.probe.402789
eh.probe.402789:  ; 0x402789
  %rsp.phi.12 = phi i32 [ %rsp.189, %eh.ok.402782 ], [ %sp.89, %eh.resume.402789.0 ]  ; 0x402789
  %eh.at.10 = zext i32 %ret.13 to i64  ; 0x402789
  %eh.fault.10 = call i64 @ra_eh_probe(i64 %eh.at.10, i64 4, i64 0, i64 4204425)  ; 0x402789
  %eh.faults.10 = icmp ne i64 %eh.fault.10, 0  ; 0x402789
  br i1 %eh.faults.10, label %eh.seh.402789, label %eh.access.402789
eh.seh.402789:  ; 0x402789
  %eh.pointers.20 = call i64 @ra_eh_pointers()  ; 0x402789
  %eh.pointers.21 = trunc i64 %eh.pointers.20 to i32  ; 0x402789
  %st.p.110 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x402789
  store i32 %eh.pointers.21, ptr %st.p.110, align 1  ; 0x402789
  %sp.85 = sub i32 %rsp.phi.12, 4  ; 0x402789
  %addr.z.166 = zext i32 %sp.85 to i64  ; 0x402789
  %st.p.111 = inttoptr i64 %addr.z.166 to ptr  ; 0x402789
  store i32 0, ptr %st.p.111, align 1  ; 0x402789
  br label %f11_402803
eh.filtered.402789.0:  ; 0x402789
  %eax.55 = trunc i64 %ret.27 to i32  ; 0x402817
  %eh.execute.10 = icmp sgt i32 %eax.55, 0  ; 0x402817
  br i1 %eh.execute.10, label %eh.except.402789.0, label %eh.verdict.402789.0
f11_402803:  ; 0x402803
  %m.39 = load i32, ptr %st.p.110, align 1  ; 0x402803
  %ecx.z.13 = zext i32 %m.39 to i64  ; 0x402803
  %m.p.40 = inttoptr i64 %ecx.z.13 to ptr  ; 0x402806
  %m.40 = load i32, ptr %m.p.40, align 1  ; 0x402806
  %eax.z.26 = zext i32 %m.40 to i64  ; 0x402806
  %m.p.41 = inttoptr i64 %eax.z.26 to ptr  ; 0x402808
  %m.41 = load i32, ptr %m.p.41, align 1  ; 0x402808
  %st.p.112 = getelementptr inbounds i8, ptr %frame.top, i64 -36  ; 0x40280a
  store i32 %m.41, ptr %st.p.112, align 1  ; 0x40280a
  %sp.86 = sub i32 %sp.85, 4  ; 0x40280d
  %addr.z.171 = zext i32 %sp.86 to i64  ; 0x40280d
  %st.p.113 = inttoptr i64 %addr.z.171 to ptr  ; 0x40280d
  store i32 %m.39, ptr %st.p.113, align 1  ; 0x40280d
  %sp.87 = sub i32 %sp.86, 4  ; 0x40280e
  %addr.z.172 = zext i32 %sp.87 to i64  ; 0x40280e
  %st.p.114 = inttoptr i64 %addr.z.172 to ptr  ; 0x40280e
  store i32 %m.41, ptr %st.p.114, align 1  ; 0x40280e
  %sp.88 = sub i32 %sp.87, 4  ; 0x40280f
  %sarg0.a.18 = add i32 %sp.88, 4
  %sarg0.18 = zext i32 %m.41 to i64
  %ret.27 = call i64 @thunk__seh_filter_exe(i64 %sarg0.18, i64 %ecx.z.13)  ; 0x40280f
  %eh.pending.20 = call i64 @ra_eh_pending()  ; 0x40280f
  %eh.thrown.20 = icmp ne i64 %eh.pending.20, 0  ; 0x40280f
  br i1 %eh.thrown.20, label %eh.landing.1, label %eh.ok.40280f.10
eh.ok.40280f.10:  ; 0x40280f
  %sum.15 = add i32 %sarg0.a.18, 8  ; 0x402814
  %sp.89 = add i32 %sum.15, 4  ; 0x402817
  br label %eh.filtered.402789.0
eh.except.402789.0:  ; 0x402817
  call void @ra_eh_handled()  ; 0x402817
  store i32 -2, ptr %st.p.1, align 1  ; 0x402817
  br label %b_402818
eh.verdict.402789.0:  ; 0x402817
  %eh.continue.10 = icmp slt i32 %eax.55, 0  ; 0x402817
  br i1 %eh.continue.10, label %eh.resume.402789.0, label %eh.search.402789.0
eh.resume.402789.0:  ; 0x402817
  call void @ra_eh_handled()  ; 0x402817
  br label %eh.probe.402789
eh.search.402789.0:  ; 0x402817
  call void @ra_eh_unhandled(i64 4204425)  ; 0x402817
  unreachable
eh.access.402789:  ; 0x402817
  %m.p.42 = inttoptr i64 %eh.at.10 to ptr  ; 0x402817
  %m.42 = load i32, ptr %m.p.42, align 1  ; 0x402817
  %zf.32 = icmp eq i32 %m.42, 0  ; 0x402817
  br i1 %zf.32, label %b_4027a5, label %b_40278e
eh.ok.40278f:  ; 0x40278f
  %esp.111 = trunc i64 %varargs.a.1 to i32  ; 0x402794
  %sum.16 = add i32 %esp.111, 4  ; 0x402794
  %al.5 = trunc i32 %ret.14 to i8  ; 0x402797
  %zf.35 = icmp eq i8 %al.5, 0  ; 0x402797
  br i1 %zf.35, label %b_4027a5, label %b_40279b
eh.probe.40279b:  ; 0x40279b
  %rsp.phi.13 = phi i32 [ %sum.16, %b_40279b ], [ %sp.96, %eh.resume.40279b.0 ]  ; 0x40279b
  %eh.fault.11 = call i64 @ra_eh_probe(i64 %eh.at.10, i64 4, i64 0, i64 4204443)  ; 0x40279b
  %eh.faults.11 = icmp ne i64 %eh.fault.11, 0  ; 0x40279b
  br i1 %eh.faults.11, label %eh.seh.40279b, label %eh.access.40279b
eh.seh.40279b:  ; 0x40279b
  %eh.pointers.22 = call i64 @ra_eh_pointers()  ; 0x40279b
  %eh.pointers.23 = trunc i64 %eh.pointers.22 to i32  ; 0x40279b
  %st.p.119 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x40279b
  store i32 %eh.pointers.23, ptr %st.p.119, align 1  ; 0x40279b
  %sp.92 = sub i32 %rsp.phi.13, 4  ; 0x40279b
  %addr.z.180 = zext i32 %sp.92 to i64  ; 0x40279b
  %st.p.120 = inttoptr i64 %addr.z.180 to ptr  ; 0x40279b
  store i32 0, ptr %st.p.120, align 1  ; 0x40279b
  br label %f12_402803
eh.filtered.40279b.0:  ; 0x40279b
  %eax.59 = trunc i64 %ret.16 to i32  ; 0x402817
  %eh.execute.11 = icmp sgt i32 %eax.59, 0  ; 0x402817
  br i1 %eh.execute.11, label %eh.except.40279b.0, label %eh.verdict.40279b.0
f12_402803:  ; 0x402803
  %m.43 = load i32, ptr %st.p.119, align 1  ; 0x402803
  %ecx.z.14 = zext i32 %m.43 to i64  ; 0x402803
  %m.p.44 = inttoptr i64 %ecx.z.14 to ptr  ; 0x402806
  %m.44 = load i32, ptr %m.p.44, align 1  ; 0x402806
  %eax.z.28 = zext i32 %m.44 to i64  ; 0x402806
  %m.p.45 = inttoptr i64 %eax.z.28 to ptr  ; 0x402808
  %m.45 = load i32, ptr %m.p.45, align 1  ; 0x402808
  %st.p.121 = getelementptr inbounds i8, ptr %frame.top, i64 -36  ; 0x40280a
  store i32 %m.45, ptr %st.p.121, align 1  ; 0x40280a
  %sp.93 = sub i32 %sp.92, 4  ; 0x40280d
  %addr.z.185 = zext i32 %sp.93 to i64  ; 0x40280d
  %st.p.122 = inttoptr i64 %addr.z.185 to ptr  ; 0x40280d
  store i32 %m.43, ptr %st.p.122, align 1  ; 0x40280d
  %sp.94 = sub i32 %sp.93, 4  ; 0x40280e
  %addr.z.186 = zext i32 %sp.94 to i64  ; 0x40280e
  %st.p.123 = inttoptr i64 %addr.z.186 to ptr  ; 0x40280e
  store i32 %m.45, ptr %st.p.123, align 1  ; 0x40280e
  %sp.95 = sub i32 %sp.94, 4  ; 0x40280f
  %sarg0.a.12 = add i32 %sp.95, 4
  %sarg0.12 = zext i32 %m.45 to i64
  %ret.16 = call i64 @thunk__seh_filter_exe(i64 %sarg0.12, i64 %ecx.z.14)  ; 0x40280f
  %eh.pending.22 = call i64 @ra_eh_pending()  ; 0x40280f
  %eh.thrown.22 = icmp ne i64 %eh.pending.22, 0  ; 0x40280f
  br i1 %eh.thrown.22, label %eh.landing.1, label %eh.ok.40280f.11
eh.ok.40280f.11:  ; 0x40280f
  %sum.17 = add i32 %sarg0.a.12, 8  ; 0x402814
  %sp.96 = add i32 %sum.17, 4  ; 0x402817
  br label %eh.filtered.40279b.0
eh.except.40279b.0:  ; 0x402817
  call void @ra_eh_handled()  ; 0x402817
  store i32 -2, ptr %st.p.1, align 1  ; 0x402817
  br label %b_402818
eh.verdict.40279b.0:  ; 0x402817
  %eh.continue.11 = icmp slt i32 %eax.59, 0  ; 0x402817
  br i1 %eh.continue.11, label %eh.resume.40279b.0, label %eh.search.40279b.0
eh.resume.40279b.0:  ; 0x402817
  call void @ra_eh_handled()  ; 0x402817
  br label %eh.probe.40279b
eh.search.40279b.0:  ; 0x402817
  call void @ra_eh_unhandled(i64 4204443)  ; 0x402817
  unreachable
eh.access.40279b:  ; 0x402817
  %m.46 = load i32, ptr %m.p.42, align 1  ; 0x402817
  %sp.97 = sub i32 %rsp.phi.13, 4  ; 0x402817
  %addr.z.191 = zext i32 %sp.97 to i64  ; 0x402817
  %st.p.126 = inttoptr i64 %addr.z.191 to ptr  ; 0x402817
  store i32 %m.46, ptr %st.p.126, align 1  ; 0x402817
  %sp.98 = sub i32 %sp.97, 4  ; 0x40279d
  %sarg0.a.11 = add i32 %sp.98, 4
  %sarg0.11 = zext i32 %m.46 to i64
  %ret.15 = call i64 @thunk__register_thread_local_exe_atexit_callback(i64 %sarg0.11)  ; 0x40279d
  %eh.pending.23 = call i64 @ra_eh_pending()  ; 0x40279d
  %eh.thrown.23 = icmp ne i64 %eh.pending.23, 0  ; 0x40279d
  br i1 %eh.thrown.23, label %eh.landing, label %eh.ok.40279d
eh.ok.40279d:  ; 0x40279d
  %sum.18 = add i32 %sarg0.a.11, 4  ; 0x4027a2
  br label %b_4027a5
eh.ok.4027a5:  ; 0x4027a5
  %eax.60 = trunc i64 %ret.17 to i32  ; 0x4027aa
  %sp.100 = sub i32 %rsp.193, 4  ; 0x4027ac
  %ret.18 = call i64 @thunk___p___argv()  ; 0x4027ac
  %rsp.194 = add i32 %sp.100, 4  ; 0x4027ac
  %eh.pending.25 = call i64 @ra_eh_pending()  ; 0x4027ac
  %eh.thrown.25 = icmp ne i64 %eh.pending.25, 0  ; 0x4027ac
  br i1 %eh.thrown.25, label %eh.landing, label %eh.ok.4027ac
eh.ok.4027ac:  ; 0x4027ac
  %n.5 = trunc i64 %ret.18 to i32
  br label %eh.probe.4027b1
eh.probe.4027b1:  ; 0x4027b1
  %rax.phi.1 = phi i32 [ %n.5, %eh.ok.4027ac ], [ %n.6, %eh.resume.4027b1.0 ]  ; 0x4027b1
  %rsp.phi.15 = phi i32 [ %rsp.194, %eh.ok.4027ac ], [ %sp.105, %eh.resume.4027b1.0 ]  ; 0x4027b1
  %eh.at.12 = zext i32 %rax.phi.1 to i64  ; 0x4027b1
  %eh.fault.12 = call i64 @ra_eh_probe(i64 %eh.at.12, i64 4, i64 0, i64 4204465)  ; 0x4027b1
  %eh.faults.12 = icmp ne i64 %eh.fault.12, 0  ; 0x4027b1
  br i1 %eh.faults.12, label %eh.seh.4027b1, label %eh.access.4027b1
eh.seh.4027b1:  ; 0x4027b1
  %eh.pointers.24 = call i64 @ra_eh_pointers()  ; 0x4027b1
  %eh.pointers.25 = trunc i64 %eh.pointers.24 to i32  ; 0x4027b1
  %st.p.130 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x4027b1
  store i32 %eh.pointers.25, ptr %st.p.130, align 1  ; 0x4027b1
  %sp.101 = sub i32 %rsp.phi.15, 4  ; 0x4027b1
  %addr.z.196 = zext i32 %sp.101 to i64  ; 0x4027b1
  %st.p.131 = inttoptr i64 %addr.z.196 to ptr  ; 0x4027b1
  store i32 0, ptr %st.p.131, align 1  ; 0x4027b1
  br label %f13_402803
eh.filtered.4027b1.0:  ; 0x4027b1
  %eax.65 = trunc i64 %ret.26 to i32  ; 0x402817
  %eh.execute.12 = icmp sgt i32 %eax.65, 0  ; 0x402817
  br i1 %eh.execute.12, label %eh.except.4027b1.0, label %eh.verdict.4027b1.0
f13_402803:  ; 0x402803
  %m.47 = load i32, ptr %st.p.130, align 1  ; 0x402803
  %ecx.z.15 = zext i32 %m.47 to i64  ; 0x402803
  %m.p.48 = inttoptr i64 %ecx.z.15 to ptr  ; 0x402806
  %m.48 = load i32, ptr %m.p.48, align 1  ; 0x402806
  %eax.z.30 = zext i32 %m.48 to i64  ; 0x402806
  %m.p.49 = inttoptr i64 %eax.z.30 to ptr  ; 0x402808
  %m.49 = load i32, ptr %m.p.49, align 1  ; 0x402808
  %st.p.132 = getelementptr inbounds i8, ptr %frame.top, i64 -36  ; 0x40280a
  store i32 %m.49, ptr %st.p.132, align 1  ; 0x40280a
  %sp.102 = sub i32 %sp.101, 4  ; 0x40280d
  %addr.z.201 = zext i32 %sp.102 to i64  ; 0x40280d
  %st.p.133 = inttoptr i64 %addr.z.201 to ptr  ; 0x40280d
  store i32 %m.47, ptr %st.p.133, align 1  ; 0x40280d
  %sp.103 = sub i32 %sp.102, 4  ; 0x40280e
  %addr.z.202 = zext i32 %sp.103 to i64  ; 0x40280e
  %st.p.134 = inttoptr i64 %addr.z.202 to ptr  ; 0x40280e
  store i32 %m.49, ptr %st.p.134, align 1  ; 0x40280e
  %sp.104 = sub i32 %sp.103, 4  ; 0x40280f
  %sarg0.a.17 = add i32 %sp.104, 4
  %sarg0.17 = zext i32 %m.49 to i64
  %ret.26 = call i64 @thunk__seh_filter_exe(i64 %sarg0.17, i64 %ecx.z.15)  ; 0x40280f
  %eh.pending.26 = call i64 @ra_eh_pending()  ; 0x40280f
  %eh.thrown.26 = icmp ne i64 %eh.pending.26, 0  ; 0x40280f
  br i1 %eh.thrown.26, label %eh.landing.1, label %eh.ok.40280f.12
eh.ok.40280f.12:  ; 0x40280f
  %sum.19 = add i32 %sarg0.a.17, 8  ; 0x402814
  %sp.105 = add i32 %sum.19, 4  ; 0x402817
  br label %eh.filtered.4027b1.0
eh.except.4027b1.0:  ; 0x402817
  call void @ra_eh_handled()  ; 0x402817
  store i32 -2, ptr %st.p.1, align 1  ; 0x402817
  br label %b_402818
eh.verdict.4027b1.0:  ; 0x402817
  %eh.continue.12 = icmp slt i32 %eax.65, 0  ; 0x402817
  br i1 %eh.continue.12, label %eh.resume.4027b1.0, label %eh.search.4027b1.0
eh.resume.4027b1.0:  ; 0x402817
  call void @ra_eh_handled()  ; 0x402817
  %n.6 = trunc i64 %ret.26 to i32
  br label %eh.probe.4027b1
eh.search.4027b1.0:  ; 0x402817
  call void @ra_eh_unhandled(i64 4204465)  ; 0x402817
  unreachable
eh.access.4027b1:  ; 0x402817
  %m.p.50 = inttoptr i64 %eh.at.12 to ptr  ; 0x402817
  %m.50 = load i32, ptr %m.p.50, align 1  ; 0x402817
  %sp.106 = sub i32 %rsp.phi.15, 4  ; 0x4027b3
  %ret.19 = call i64 @thunk___p___argc()  ; 0x4027b3
  %rsp.195 = add i32 %sp.106, 4  ; 0x4027b3
  %eh.pending.27 = call i64 @ra_eh_pending()  ; 0x4027b3
  %eh.thrown.27 = icmp ne i64 %eh.pending.27, 0  ; 0x4027b3
  br i1 %eh.thrown.27, label %eh.landing, label %eh.ok.4027b3
eh.ok.4027b3:  ; 0x4027b3
  %sp.107 = sub i32 %rsp.195, 4  ; 0x4027b8
  %addr.z.208 = zext i32 %sp.107 to i64  ; 0x4027b8
  %st.p.138 = inttoptr i64 %addr.z.208 to ptr  ; 0x4027b8
  store i32 %eax.60, ptr %st.p.138, align 1  ; 0x4027b8
  %sp.108 = sub i32 %sp.107, 4  ; 0x4027b9
  %addr.z.209 = zext i32 %sp.108 to i64  ; 0x4027b9
  %st.p.139 = inttoptr i64 %addr.z.209 to ptr  ; 0x4027b9
  store i32 %m.50, ptr %st.p.139, align 1  ; 0x4027b9
  %n.7 = trunc i64 %ret.19 to i32
  br label %eh.probe.4027ba
eh.probe.4027ba:  ; 0x4027ba
  %rax.phi.2 = phi i32 [ %n.7, %eh.ok.4027b3 ], [ %n.8, %eh.resume.4027ba.0 ]  ; 0x4027ba
  %rsp.phi.16 = phi i32 [ %sp.108, %eh.ok.4027b3 ], [ %sp.113, %eh.resume.4027ba.0 ]  ; 0x4027ba
  %eh.at.13 = zext i32 %rax.phi.2 to i64  ; 0x4027ba
  %eh.fault.13 = call i64 @ra_eh_probe(i64 %eh.at.13, i64 4, i64 0, i64 4204474)  ; 0x4027ba
  %eh.faults.13 = icmp ne i64 %eh.fault.13, 0  ; 0x4027ba
  br i1 %eh.faults.13, label %eh.seh.4027ba, label %eh.access.4027ba
eh.seh.4027ba:  ; 0x4027ba
  %eh.pointers.26 = call i64 @ra_eh_pointers()  ; 0x4027ba
  %eh.pointers.27 = trunc i64 %eh.pointers.26 to i32  ; 0x4027ba
  %st.p.140 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x4027ba
  store i32 %eh.pointers.27, ptr %st.p.140, align 1  ; 0x4027ba
  %sp.109 = sub i32 %rsp.phi.16, 4  ; 0x4027ba
  %addr.z.211 = zext i32 %sp.109 to i64  ; 0x4027ba
  %st.p.141 = inttoptr i64 %addr.z.211 to ptr  ; 0x4027ba
  store i32 0, ptr %st.p.141, align 1  ; 0x4027ba
  br label %f14_402803
eh.filtered.4027ba.0:  ; 0x4027ba
  %eax.71 = trunc i64 %ret.25 to i32  ; 0x402817
  %eh.execute.13 = icmp sgt i32 %eax.71, 0  ; 0x402817
  br i1 %eh.execute.13, label %eh.except.4027ba.0, label %eh.verdict.4027ba.0
f14_402803:  ; 0x402803
  %m.51 = load i32, ptr %st.p.140, align 1  ; 0x402803
  %ecx.z.16 = zext i32 %m.51 to i64  ; 0x402803
  %m.p.52 = inttoptr i64 %ecx.z.16 to ptr  ; 0x402806
  %m.52 = load i32, ptr %m.p.52, align 1  ; 0x402806
  %eax.z.32 = zext i32 %m.52 to i64  ; 0x402806
  %m.p.53 = inttoptr i64 %eax.z.32 to ptr  ; 0x402808
  %m.53 = load i32, ptr %m.p.53, align 1  ; 0x402808
  %st.p.142 = getelementptr inbounds i8, ptr %frame.top, i64 -36  ; 0x40280a
  store i32 %m.53, ptr %st.p.142, align 1  ; 0x40280a
  %sp.110 = sub i32 %sp.109, 4  ; 0x40280d
  %addr.z.216 = zext i32 %sp.110 to i64  ; 0x40280d
  %st.p.143 = inttoptr i64 %addr.z.216 to ptr  ; 0x40280d
  store i32 %m.51, ptr %st.p.143, align 1  ; 0x40280d
  %sp.111 = sub i32 %sp.110, 4  ; 0x40280e
  %addr.z.217 = zext i32 %sp.111 to i64  ; 0x40280e
  %st.p.144 = inttoptr i64 %addr.z.217 to ptr  ; 0x40280e
  store i32 %m.53, ptr %st.p.144, align 1  ; 0x40280e
  %sp.112 = sub i32 %sp.111, 4  ; 0x40280f
  %sarg0.a.16 = add i32 %sp.112, 4
  %sarg0.16 = zext i32 %m.53 to i64
  %ret.25 = call i64 @thunk__seh_filter_exe(i64 %sarg0.16, i64 %ecx.z.16)  ; 0x40280f
  %eh.pending.28 = call i64 @ra_eh_pending()  ; 0x40280f
  %eh.thrown.28 = icmp ne i64 %eh.pending.28, 0  ; 0x40280f
  br i1 %eh.thrown.28, label %eh.landing.1, label %eh.ok.40280f.13
eh.ok.40280f.13:  ; 0x40280f
  %sum.20 = add i32 %sarg0.a.16, 8  ; 0x402814
  %sp.113 = add i32 %sum.20, 4  ; 0x402817
  br label %eh.filtered.4027ba.0
eh.except.4027ba.0:  ; 0x402817
  call void @ra_eh_handled()  ; 0x402817
  store i32 -2, ptr %st.p.1, align 1  ; 0x402817
  br label %b_402818
eh.verdict.4027ba.0:  ; 0x402817
  %eh.continue.13 = icmp slt i32 %eax.71, 0  ; 0x402817
  br i1 %eh.continue.13, label %eh.resume.4027ba.0, label %eh.search.4027ba.0
eh.resume.4027ba.0:  ; 0x402817
  call void @ra_eh_handled()  ; 0x402817
  %n.8 = trunc i64 %ret.25 to i32
  br label %eh.probe.4027ba
eh.search.4027ba.0:  ; 0x402817
  call void @ra_eh_unhandled(i64 4204474)  ; 0x402817
  unreachable
eh.access.4027ba:  ; 0x402817
  %m.p.54 = inttoptr i64 %eh.at.13 to ptr  ; 0x402817
  %m.54 = load i32, ptr %m.p.54, align 1  ; 0x402817
  %sp.114 = sub i32 %rsp.phi.16, 4  ; 0x402817
  %addr.z.222 = zext i32 %sp.114 to i64  ; 0x402817
  %st.p.147 = inttoptr i64 %addr.z.222 to ptr  ; 0x402817
  store i32 %m.54, ptr %st.p.147, align 1  ; 0x402817
  %sp.115 = sub i32 %sp.114, 4  ; 0x4027bc
  %ret.20 = call i64 @F_004021a0()  ; 0x4027bc
  %rsp.196 = add i32 %sp.115, 4  ; 0x4027bc
  %eh.pending.29 = call i64 @ra_eh_pending()  ; 0x4027bc
  %eh.thrown.29 = icmp ne i64 %eh.pending.29, 0  ; 0x4027bc
  br i1 %eh.thrown.29, label %eh.landing, label %eh.ok.4027bc
eh.ok.4027bc:  ; 0x4027bc
  %sum.21 = add i32 %rsp.196, 12  ; 0x4027c1
  %eax.73 = trunc i64 %ret.20 to i32  ; 0x4027c4
  %sp.116 = sub i32 %sum.21, 4  ; 0x4027c6
  %ret.21 = call i64 @F_00402d30()  ; 0x4027c6
  %rsp.197 = add i32 %sp.116, 4  ; 0x4027c6
  %eh.pending.30 = call i64 @ra_eh_pending()  ; 0x4027c6
  %eh.thrown.30 = icmp ne i64 %eh.pending.30, 0  ; 0x4027c6
  br i1 %eh.thrown.30, label %eh.landing, label %eh.ok.4027c6
eh.ok.4027c6:  ; 0x4027c6
  %al.7 = trunc i64 %ret.21 to i8  ; 0x4027cb
  %zf.42 = icmp eq i8 %al.7, 0  ; 0x4027cb
  br i1 %zf.42, label %b_402859, label %b_4027d3
eh.ok.4027d7:  ; 0x4027d7
  br label %b_4027dc
eh.ok.4027e0:  ; 0x4027e0
  %sum.22 = add i32 %sarg0.a.13, 8  ; 0x4027e5
  br label %eh.probe.4027e8
eh.probe.4027e8:  ; 0x4027e8
  %rsp.phi.18 = phi i32 [ %sum.22, %eh.ok.4027e0 ], [ %sp.125, %eh.resume.4027e8.0 ]  ; 0x4027e8
  %addr.43 = add i32 %sp, -4  ; 0x4027e8
  %eh.at.14 = zext i32 %addr.43 to i64  ; 0x4027e8
  %eh.fault.14 = call i64 @ra_eh_probe(i64 %eh.at.14, i64 4, i64 1, i64 4204520)  ; 0x4027e8
  %eh.faults.14 = icmp ne i64 %eh.fault.14, 0  ; 0x4027e8
  br i1 %eh.faults.14, label %eh.seh.4027e8, label %eh.access.4027e8
eh.seh.4027e8:  ; 0x4027e8
  %eh.pointers.28 = call i64 @ra_eh_pointers()  ; 0x4027e8
  %eh.pointers.29 = trunc i64 %eh.pointers.28 to i32  ; 0x4027e8
  %st.p.154 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x4027e8
  store i32 %eh.pointers.29, ptr %st.p.154, align 1  ; 0x4027e8
  %sp.121 = sub i32 %rsp.phi.18, 4  ; 0x4027e8
  %addr.z.230 = zext i32 %sp.121 to i64  ; 0x4027e8
  %st.p.155 = inttoptr i64 %addr.z.230 to ptr  ; 0x4027e8
  store i32 0, ptr %st.p.155, align 1  ; 0x4027e8
  br label %f15_402803
eh.filtered.4027e8.0:  ; 0x4027e8
  %eax.77 = trunc i64 %ret.23 to i32  ; 0x402817
  %eh.execute.14 = icmp sgt i32 %eax.77, 0  ; 0x402817
  br i1 %eh.execute.14, label %eh.except.4027e8.0, label %eh.verdict.4027e8.0
f15_402803:  ; 0x402803
  %m.55 = load i32, ptr %st.p.154, align 1  ; 0x402803
  %ecx.z.17 = zext i32 %m.55 to i64  ; 0x402803
  %m.p.56 = inttoptr i64 %ecx.z.17 to ptr  ; 0x402806
  %m.56 = load i32, ptr %m.p.56, align 1  ; 0x402806
  %eax.z.34 = zext i32 %m.56 to i64  ; 0x402806
  %m.p.57 = inttoptr i64 %eax.z.34 to ptr  ; 0x402808
  %m.57 = load i32, ptr %m.p.57, align 1  ; 0x402808
  %st.p.156 = getelementptr inbounds i8, ptr %frame.top, i64 -36  ; 0x40280a
  store i32 %m.57, ptr %st.p.156, align 1  ; 0x40280a
  %sp.122 = sub i32 %sp.121, 4  ; 0x40280d
  %addr.z.235 = zext i32 %sp.122 to i64  ; 0x40280d
  %st.p.157 = inttoptr i64 %addr.z.235 to ptr  ; 0x40280d
  store i32 %m.55, ptr %st.p.157, align 1  ; 0x40280d
  %sp.123 = sub i32 %sp.122, 4  ; 0x40280e
  %addr.z.236 = zext i32 %sp.123 to i64  ; 0x40280e
  %st.p.158 = inttoptr i64 %addr.z.236 to ptr  ; 0x40280e
  store i32 %m.57, ptr %st.p.158, align 1  ; 0x40280e
  %sp.124 = sub i32 %sp.123, 4  ; 0x40280f
  %sarg0.a.14 = add i32 %sp.124, 4
  %sarg0.14 = zext i32 %m.57 to i64
  %ret.23 = call i64 @thunk__seh_filter_exe(i64 %sarg0.14, i64 %ecx.z.17)  ; 0x40280f
  %eh.pending.33 = call i64 @ra_eh_pending()  ; 0x40280f
  %eh.thrown.33 = icmp ne i64 %eh.pending.33, 0  ; 0x40280f
  br i1 %eh.thrown.33, label %eh.landing.1, label %eh.ok.40280f.14
eh.ok.40280f.14:  ; 0x40280f
  %sum.23 = add i32 %sarg0.a.14, 8  ; 0x402814
  %sp.125 = add i32 %sum.23, 4  ; 0x402817
  br label %eh.filtered.4027e8.0
eh.except.4027e8.0:  ; 0x402817
  call void @ra_eh_handled()  ; 0x402817
  store i32 -2, ptr %st.p.1, align 1  ; 0x402817
  br label %b_402818
eh.verdict.4027e8.0:  ; 0x402817
  %eh.continue.14 = icmp slt i32 %eax.77, 0  ; 0x402817
  br i1 %eh.continue.14, label %eh.resume.4027e8.0, label %eh.search.4027e8.0
eh.resume.4027e8.0:  ; 0x402817
  call void @ra_eh_handled()  ; 0x402817
  br label %eh.probe.4027e8
eh.search.4027e8.0:  ; 0x402817
  call void @ra_eh_unhandled(i64 4204520)  ; 0x402817
  unreachable
eh.access.4027e8:  ; 0x402817
  store i32 -2, ptr %st.p.1, align 1  ; 0x402817
  %eax.z.36 = zext i32 %eax.73 to i64  ; 0x4027ef
  %m.58 = load i32, ptr %st.p.4, align 1  ; 0x4027f1
  %teb.6 = call i64 @ra_teb()  ; 0x4027f4
  %teb.7 = trunc i64 %teb.6 to i32  ; 0x4027f4
  %addr.z.242 = zext i32 %teb.7 to i64  ; 0x4027f4
  %st.p.162 = inttoptr i64 %addr.z.242 to ptr  ; 0x4027f4
  store i32 %m.58, ptr %st.p.162, align 1  ; 0x4027f4
  ret i64 %eax.z.36
eh.ok.40281b:  ; 0x40281b
  %al.9 = trunc i64 %ret.33 to i8  ; 0x402820
  %zf.48 = icmp eq i8 %al.9, 0  ; 0x402820
  br i1 %zf.48, label %b_40285f, label %b_402824
eh.ok.40282a:  ; 0x40282a
  br label %b_40282f
eh.ok.40284d:  ; 0x40284d
  unreachable
eh.ok.402854:  ; 0x402854
  unreachable
eh.ok.40285a:  ; 0x40285a
  unreachable
eh.ok.402862:  ; 0x402862
  unreachable
}

define i64 @entry() {  ; 0x402870
b_402870:  ; 0x402870
  %ret = call i64 @F_00402b80()  ; 0x402870
  %eh.pending = call i64 @ra_eh_pending()  ; 0x402870
  %eh.thrown = icmp ne i64 %eh.pending, 0  ; 0x402870
  br i1 %eh.thrown, label %eh.landing, label %eh.ok.402870
eh.landing:  ; 0x402870
  ret i64 %ret
eh.ok.402870:  ; 0x402870
  %ret.1 = call i64 @F_00402680()  ; 0x402875
  ret i64 %ret.1
}

define void @F_00402880(i32 %arg0) {  ; 0x402880
b_402880:  ; 0x402880
  %local_m10 = alloca i32, align 4
  %local_mc = alloca i32, align 4
  %local_m8 = alloca i32, align 4
  %local_m4 = alloca i32, align 4
  %local_p4 = alloca i32, align 4
  store i32 %arg0, ptr %local_p4, align 4
  %ebp = trunc i64 undef to i32  ; 0x402880
  store i32 %ebp, ptr %local_m4, align 1  ; 0x402880
  %addr.z.1 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 860) to i32) to i64  ; 0x402883
  %m.p = inttoptr i64 %addr.z.1 to ptr  ; 0x402883
  %m = load i32, ptr %m.p, align 1  ; 0x402883
  store i32 %ebp, ptr %local_m8, align 1  ; 0x40288a
  store i32 %arg0, ptr %local_mc, align 1  ; 0x40288e
  %not = icmp ne i32 %m, -1  ; 0x40288f
  br i1 %not, label %b_4028ac, label %b_402891
b_402891:  ; 0x402891
  %sarg0.d = load i32, ptr %local_mc, align 4
  %sarg0 = zext i32 %sarg0.d to i64
  %ret = call i64 @thunk__crt_atexit(i64 %sarg0)  ; 0x402891
  %eh.pending = call i64 @ra_eh_pending()  ; 0x402891
  %eh.thrown = icmp ne i64 %eh.pending, 0  ; 0x402891
  br i1 %eh.thrown, label %eh.landing, label %eh.ok.402891
b_4028ac:  ; 0x4028ac
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 860) to i32), ptr %local_m10, align 1  ; 0x4028ac
  %sarg1.d = load i32, ptr %local_mc, align 4
  %sarg1 = zext i32 %sarg1.d to i64
  %ret.1 = call i64 @thunk__register_onexit_function(i64 %addr.z.1, i64 %sarg1)  ; 0x4028b1
  %eh.pending.1 = call i64 @ra_eh_pending()  ; 0x4028b1
  %eh.thrown.1 = icmp ne i64 %eh.pending.1, 0  ; 0x4028b1
  br i1 %eh.thrown.1, label %eh.landing, label %eh.ok.4028b1
eh.landing:  ; 0x402891
  ret void
eh.ok.402891:  ; 0x402891
  ret void
eh.ok.4028b1:  ; 0x4028b1
  ret void
}

define i32 @F_004028d0(i64 %varargs) {  ; 0x4028d0
b_4028d0:  ; 0x4028d0
  %frame = alloca [4264 x i8], align 16
  %frame.top = getelementptr inbounds i8, ptr %frame, i64 4188
  %rsp0 = ptrtoint ptr %frame.top to i64
  %ebp = trunc i64 undef to i32  ; 0x4028d0
  %esp = trunc i64 %rsp0 to i32  ; 0x4028d0
  %sp = sub i32 %esp, 4  ; 0x4028d0
  %st.p = getelementptr inbounds i8, ptr %frame.top, i64 -4  ; 0x4028d0
  store i32 %ebp, ptr %st.p, align 1  ; 0x4028d0
  %sp.1 = sub i32 %sp, 4  ; 0x4028d3
  %st.p.1 = getelementptr inbounds i8, ptr %frame.top, i64 -8  ; 0x4028d3
  store i32 -2, ptr %st.p.1, align 1  ; 0x4028d3
  %sp.2 = sub i32 %sp.1, 4  ; 0x4028d5
  %st.p.2 = getelementptr inbounds i8, ptr %frame.top, i64 -12  ; 0x4028d5
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 3640) to i32), ptr %st.p.2, align 1  ; 0x4028d5
  %sp.3 = sub i32 %sp.2, 4  ; 0x4028da
  %st.p.3 = getelementptr inbounds i8, ptr %frame.top, i64 -16  ; 0x4028da
  store i32 ptrtoint (ptr @U_00402e70 to i32), ptr %st.p.3, align 1  ; 0x4028da
  %teb = call i64 @ra_teb()  ; 0x4028df
  %teb.1 = trunc i64 %teb to i32  ; 0x4028df
  %addr.z.4 = zext i32 %teb.1 to i64  ; 0x4028df
  %m.p = inttoptr i64 %addr.z.4 to ptr  ; 0x4028df
  %m = load i32, ptr %m.p, align 1  ; 0x4028df
  %sp.4 = sub i32 %sp.3, 4  ; 0x4028e5
  %st.p.4 = getelementptr inbounds i8, ptr %frame.top, i64 -20  ; 0x4028e5
  store i32 %m, ptr %st.p.4, align 1  ; 0x4028e5
  %diff = sub i32 %sp.4, 8  ; 0x4028e6
  %sp.5 = sub i32 %diff, 4  ; 0x4028e9
  %st.p.5 = getelementptr inbounds i8, ptr %frame.top, i64 -32  ; 0x4028e9
  store i32 %ebp, ptr %st.p.5, align 1  ; 0x4028e9
  %sp.6 = sub i32 %sp.5, 4  ; 0x4028ea
  %st.p.6 = getelementptr inbounds i8, ptr %frame.top, i64 -36  ; 0x4028ea
  store i32 %ebp, ptr %st.p.6, align 1  ; 0x4028ea
  %sp.7 = sub i32 %sp.6, 4  ; 0x4028eb
  %st.p.7 = getelementptr inbounds i8, ptr %frame.top, i64 -40  ; 0x4028eb
  store i32 %ebp, ptr %st.p.7, align 1  ; 0x4028eb
  %addr.z.9 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 192) to i32) to i64  ; 0x4028ec
  %m.p.1 = inttoptr i64 %addr.z.9 to ptr  ; 0x4028ec
  %m.1 = load i32, ptr %m.p.1, align 1  ; 0x4028ec
  %m.2 = load i32, ptr %st.p.2, align 1  ; 0x4028f1
  %logic = xor i32 %m.2, %m.1  ; 0x4028f1
  store i32 %logic, ptr %st.p.2, align 1  ; 0x4028f1
  %logic.1 = xor i32 %m.1, %sp  ; 0x4028f4
  %sp.8 = sub i32 %sp.7, 4  ; 0x4028f6
  %st.p.9 = getelementptr inbounds i8, ptr %frame.top, i64 -44  ; 0x4028f6
  store i32 %logic.1, ptr %st.p.9, align 1  ; 0x4028f6
  %addr.2 = add i32 %sp, -16  ; 0x4028f7
  %teb.2 = call i64 @ra_teb()  ; 0x4028fa
  %teb.3 = trunc i64 %teb.2 to i32  ; 0x4028fa
  %addr.z.13 = zext i32 %teb.3 to i64  ; 0x4028fa
  %st.p.10 = inttoptr i64 %addr.z.13 to ptr  ; 0x4028fa
  store i32 %addr.2, ptr %st.p.10, align 1  ; 0x4028fa
  %st.p.11 = getelementptr inbounds i8, ptr %frame.top, i64 -28  ; 0x402900
  store i32 %sp.8, ptr %st.p.11, align 1  ; 0x402900
  store i32 0, ptr %st.p.1, align 1  ; 0x402903
  br label %eh.probe.40290f
b_402918:  ; 0x402918
  br label %eh.probe.402918
b_402929:  ; 0x402929
  br label %eh.probe.40292e
b_402937:  ; 0x402937
  br label %eh.probe.402937
b_402951:  ; 0x402951
  br label %eh.probe.402951
b_402957:  ; 0x402957
  br label %eh.probe.402957
b_402985:  ; 0x402985
  %rax.phi.5 = phi i32 [ %cl.z, %eh.except.40290f.0 ], [ %cl.z.1, %eh.except.402918.0 ], [ %cl.z.2, %eh.except.40291d.0 ], [ %cl.z.3, %eh.except.40292e.0 ], [ %cl.z.4, %eh.except.402937.0 ], [ %cl.z.5, %eh.except.402951.0 ], [ %cl.z.6, %eh.except.402957.0 ], [ %cl.z.7, %eh.except.402988.0 ]  ; 0x402985
  %rbp.phi.7 = phi i32 [ %eh.handler.frame, %eh.except.40290f.0 ], [ %eh.handler.frame.1, %eh.except.402918.0 ], [ %eh.handler.frame.2, %eh.except.40291d.0 ], [ %eh.handler.frame.3, %eh.except.40292e.0 ], [ %eh.handler.frame.4, %eh.except.402937.0 ], [ %eh.handler.frame.5, %eh.except.402951.0 ], [ %eh.handler.frame.6, %eh.except.402957.0 ], [ %eh.handler.frame.7, %eh.except.402988.0 ]  ; 0x402985
  %m.31 = load i32, ptr %st.p.11, align 1  ; 0x402985
  br label %b_402988
b_402988:  ; 0x402988
  %rax.phi.6 = phi i32 [ %rax.phi.5, %b_402985 ], [ %rax.phi, %eh.access.40290f ], [ %rax.phi.1, %eh.access.40291d ], [ %rax.phi.2, %eh.access.40292e ], [ %ret, %eh.ok.402945 ], [ %rax.phi.3, %eh.access.402951 ]  ; 0x402988
  %rsp.phi.7 = phi i32 [ %m.31, %b_402985 ], [ %rsp.phi, %eh.access.40290f ], [ %rsp.phi.2, %eh.access.40291d ], [ %rsp.phi.3, %eh.access.40292e ], [ %sum, %eh.ok.402945 ], [ %rsp.phi.5, %eh.access.402951 ]  ; 0x402988
  %rbp.phi.8 = phi i32 [ %rbp.phi.7, %b_402985 ], [ %sp, %eh.access.40290f ], [ %sp, %eh.access.40291d ], [ %sp, %eh.access.40292e ], [ %sp, %eh.ok.402945 ], [ %sp, %eh.access.402951 ]  ; 0x402988
  br label %eh.probe.402988
eh.probe.40290f:  ; 0x40290f
  %rax.phi = phi i32 [ 23117, %b_4028d0 ], [ %cl.z, %eh.resume.40290f.0 ]  ; 0x40290f
  %rsp.phi = phi i32 [ %sp.8, %b_4028d0 ], [ %sp.10, %eh.resume.40290f.0 ]  ; 0x40290f
  %eh.at = zext i32 ptrtoint (ptr @sec_headers to i32) to i64  ; 0x40290f
  %eh.fault = call i64 @ra_eh_probe(i64 %eh.at, i64 2, i64 0, i64 4204815)  ; 0x40290f
  %eh.faults = icmp ne i64 %eh.fault, 0  ; 0x40290f
  br i1 %eh.faults, label %eh.seh.40290f, label %eh.access.40290f
eh.seh.40290f:  ; 0x40290f
  %eh.node = add i32 %sp, -16  ; 0x40290f
  %eh.handler.frame = add i32 %eh.node, 16  ; 0x40290f
  %eh.pointers = call i64 @ra_eh_pointers()  ; 0x40290f
  %eh.pointers.1 = trunc i64 %eh.pointers to i32  ; 0x40290f
  %st.p.13 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x40290f
  store i32 %eh.pointers.1, ptr %st.p.13, align 1  ; 0x40290f
  %sp.9 = sub i32 %rsp.phi, 4  ; 0x40290f
  %st.p.14 = getelementptr inbounds i8, ptr %frame.top, i64 -48  ; 0x40290f
  store i32 0, ptr %st.p.14, align 1  ; 0x40290f
  br label %f1_402972
eh.filtered.40290f.0:  ; 0x40290f
  %eh.execute = icmp sgt i32 %cl.z, 0  ; 0x402984
  br i1 %eh.execute, label %eh.except.40290f.0, label %eh.verdict.40290f.0
f1_402972:  ; 0x402972
  %m.3 = load i32, ptr %st.p.13, align 1  ; 0x402972
  %eax.z.5 = zext i32 %m.3 to i64  ; 0x402972
  %m.p.4 = inttoptr i64 %eax.z.5 to ptr  ; 0x402975
  %m.4 = load i32, ptr %m.p.4, align 1  ; 0x402975
  %eax.z.6 = zext i32 %m.4 to i64  ; 0x402975
  %m.p.5 = inttoptr i64 %eax.z.6 to ptr  ; 0x402979
  %m.5 = load i32, ptr %m.p.5, align 1  ; 0x402979
  %zf.4 = icmp eq i32 %m.5, -1073741819  ; 0x402979
  %cl.z = zext i1 %zf.4 to i32  ; 0x40297f
  %sp.10 = add i32 %sp.9, 4  ; 0x402984
  br label %eh.filtered.40290f.0
eh.except.40290f.0:  ; 0x402984
  call void @ra_eh_handled()  ; 0x402984
  store i32 -2, ptr %st.p.1, align 1  ; 0x402984
  br label %b_402985
eh.verdict.40290f.0:  ; 0x402984
  %eh.continue = icmp slt i32 %cl.z, 0  ; 0x402984
  br i1 %eh.continue, label %eh.resume.40290f.0, label %eh.search.40290f.0
eh.resume.40290f.0:  ; 0x402984
  call void @ra_eh_handled()  ; 0x402984
  br label %eh.probe.40290f
eh.search.40290f.0:  ; 0x402984
  call void @ra_eh_unhandled(i64 4204815)  ; 0x402984
  unreachable
eh.access.40290f:  ; 0x402984
  %ax = trunc i32 %rax.phi to i16  ; 0x402984
  %not = icmp ne i16 23117, %ax  ; 0x402916
  br i1 %not, label %b_402988, label %b_402918
eh.probe.402918:  ; 0x402918
  %rsp.phi.1 = phi i32 [ %rsp.phi, %b_402918 ], [ %sp.12, %eh.resume.402918.0 ]  ; 0x402918
  %eh.at.1 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_headers, i64 60) to i32) to i64  ; 0x402918
  %eh.fault.1 = call i64 @ra_eh_probe(i64 %eh.at.1, i64 4, i64 0, i64 4204824)  ; 0x402918
  %eh.faults.1 = icmp ne i64 %eh.fault.1, 0  ; 0x402918
  br i1 %eh.faults.1, label %eh.seh.402918, label %eh.access.402918
eh.seh.402918:  ; 0x402918
  %eh.node.1 = add i32 %sp, -16  ; 0x402918
  %eh.handler.frame.1 = add i32 %eh.node.1, 16  ; 0x402918
  %eh.pointers.2 = call i64 @ra_eh_pointers()  ; 0x402918
  %eh.pointers.3 = trunc i64 %eh.pointers.2 to i32  ; 0x402918
  %st.p.16 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x402918
  store i32 %eh.pointers.3, ptr %st.p.16, align 1  ; 0x402918
  %sp.11 = sub i32 %rsp.phi.1, 4  ; 0x402918
  %st.p.17 = getelementptr inbounds i8, ptr %frame.top, i64 -48  ; 0x402918
  store i32 0, ptr %st.p.17, align 1  ; 0x402918
  br label %f2_402972
eh.filtered.402918.0:  ; 0x402918
  %eh.execute.1 = icmp sgt i32 %cl.z.1, 0  ; 0x402984
  br i1 %eh.execute.1, label %eh.except.402918.0, label %eh.verdict.402918.0
f2_402972:  ; 0x402972
  %m.7 = load i32, ptr %st.p.16, align 1  ; 0x402972
  %eax.z.8 = zext i32 %m.7 to i64  ; 0x402972
  %m.p.8 = inttoptr i64 %eax.z.8 to ptr  ; 0x402975
  %m.8 = load i32, ptr %m.p.8, align 1  ; 0x402975
  %eax.z.9 = zext i32 %m.8 to i64  ; 0x402975
  %m.p.9 = inttoptr i64 %eax.z.9 to ptr  ; 0x402979
  %m.9 = load i32, ptr %m.p.9, align 1  ; 0x402979
  %zf.9 = icmp eq i32 %m.9, -1073741819  ; 0x402979
  %cl.z.1 = zext i1 %zf.9 to i32  ; 0x40297f
  %sp.12 = add i32 %sp.11, 4  ; 0x402984
  br label %eh.filtered.402918.0
eh.except.402918.0:  ; 0x402984
  call void @ra_eh_handled()  ; 0x402984
  store i32 -2, ptr %st.p.1, align 1  ; 0x402984
  br label %b_402985
eh.verdict.402918.0:  ; 0x402984
  %eh.continue.1 = icmp slt i32 %cl.z.1, 0  ; 0x402984
  br i1 %eh.continue.1, label %eh.resume.402918.0, label %eh.search.402918.0
eh.resume.402918.0:  ; 0x402984
  call void @ra_eh_handled()  ; 0x402984
  br label %eh.probe.402918
eh.search.402918.0:  ; 0x402984
  call void @ra_eh_unhandled(i64 4204824)  ; 0x402984
  unreachable
eh.access.402918:  ; 0x402984
  br label %eh.probe.40291d
eh.probe.40291d:  ; 0x40291d
  %rax.phi.1 = phi i32 [ 120, %eh.access.402918 ], [ %cl.z.2, %eh.resume.40291d.0 ]  ; 0x40291d
  %rsp.phi.2 = phi i32 [ %rsp.phi.1, %eh.access.402918 ], [ %sp.14, %eh.resume.40291d.0 ]  ; 0x40291d
  %addr.7 = add i32 ptrtoint (ptr @sec_headers to i32), %rax.phi.1  ; 0x40291d
  %eh.at.2 = zext i32 %addr.7 to i64  ; 0x40291d
  %eh.fault.2 = call i64 @ra_eh_probe(i64 %eh.at.2, i64 4, i64 0, i64 4204829)  ; 0x40291d
  %eh.faults.2 = icmp ne i64 %eh.fault.2, 0  ; 0x40291d
  br i1 %eh.faults.2, label %eh.seh.40291d, label %eh.access.40291d
eh.seh.40291d:  ; 0x40291d
  %eh.node.2 = add i32 %sp, -16  ; 0x40291d
  %eh.handler.frame.2 = add i32 %eh.node.2, 16  ; 0x40291d
  %eh.pointers.4 = call i64 @ra_eh_pointers()  ; 0x40291d
  %eh.pointers.5 = trunc i64 %eh.pointers.4 to i32  ; 0x40291d
  %st.p.19 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x40291d
  store i32 %eh.pointers.5, ptr %st.p.19, align 1  ; 0x40291d
  %sp.13 = sub i32 %rsp.phi.2, 4  ; 0x40291d
  %st.p.20 = getelementptr inbounds i8, ptr %frame.top, i64 -48  ; 0x40291d
  store i32 0, ptr %st.p.20, align 1  ; 0x40291d
  br label %f3_402972
eh.filtered.40291d.0:  ; 0x40291d
  %eh.execute.2 = icmp sgt i32 %cl.z.2, 0  ; 0x402984
  br i1 %eh.execute.2, label %eh.except.40291d.0, label %eh.verdict.40291d.0
f3_402972:  ; 0x402972
  %m.11 = load i32, ptr %st.p.19, align 1  ; 0x402972
  %eax.z.12 = zext i32 %m.11 to i64  ; 0x402972
  %m.p.12 = inttoptr i64 %eax.z.12 to ptr  ; 0x402975
  %m.12 = load i32, ptr %m.p.12, align 1  ; 0x402975
  %eax.z.13 = zext i32 %m.12 to i64  ; 0x402975
  %m.p.13 = inttoptr i64 %eax.z.13 to ptr  ; 0x402979
  %m.13 = load i32, ptr %m.p.13, align 1  ; 0x402979
  %zf.12 = icmp eq i32 %m.13, -1073741819  ; 0x402979
  %cl.z.2 = zext i1 %zf.12 to i32  ; 0x40297f
  %sp.14 = add i32 %sp.13, 4  ; 0x402984
  br label %eh.filtered.40291d.0
eh.except.40291d.0:  ; 0x402984
  call void @ra_eh_handled()  ; 0x402984
  store i32 -2, ptr %st.p.1, align 1  ; 0x402984
  br label %b_402985
eh.verdict.40291d.0:  ; 0x402984
  %eh.continue.2 = icmp slt i32 %cl.z.2, 0  ; 0x402984
  br i1 %eh.continue.2, label %eh.resume.40291d.0, label %eh.search.40291d.0
eh.resume.40291d.0:  ; 0x402984
  call void @ra_eh_handled()  ; 0x402984
  br label %eh.probe.40291d
eh.search.40291d.0:  ; 0x402984
  call void @ra_eh_unhandled(i64 4204829)  ; 0x402984
  unreachable
eh.access.40291d:  ; 0x402984
  %m.p.14 = inttoptr i64 %eh.at.2 to ptr  ; 0x402984
  %m.14 = load i32, ptr %m.p.14, align 1  ; 0x402984
  %not.1 = icmp ne i32 %m.14, 17744  ; 0x402927
  br i1 %not.1, label %b_402988, label %b_402929
eh.probe.40292e:  ; 0x40292e
  %rax.phi.2 = phi i32 [ %rax.phi.1, %b_402929 ], [ %cl.z.3, %eh.resume.40292e.0 ]  ; 0x40292e
  %rcx.phi = phi i16 [ 267, %b_402929 ], [ %n, %eh.resume.40292e.0 ]  ; 0x40292e
  %rsp.phi.3 = phi i32 [ %rsp.phi.2, %b_402929 ], [ %sp.16, %eh.resume.40292e.0 ]  ; 0x40292e
  %addr.10 = add i32 ptrtoint (ptr getelementptr (i8, ptr @sec_headers, i64 24) to i32), %rax.phi.2  ; 0x40292e
  %eh.at.3 = zext i32 %addr.10 to i64  ; 0x40292e
  %eh.fault.3 = call i64 @ra_eh_probe(i64 %eh.at.3, i64 2, i64 0, i64 4204846)  ; 0x40292e
  %eh.faults.3 = icmp ne i64 %eh.fault.3, 0  ; 0x40292e
  br i1 %eh.faults.3, label %eh.seh.40292e, label %eh.access.40292e
eh.seh.40292e:  ; 0x40292e
  %eh.node.3 = add i32 %sp, -16  ; 0x40292e
  %eh.handler.frame.3 = add i32 %eh.node.3, 16  ; 0x40292e
  %eh.pointers.6 = call i64 @ra_eh_pointers()  ; 0x40292e
  %eh.pointers.7 = trunc i64 %eh.pointers.6 to i32  ; 0x40292e
  %st.p.22 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x40292e
  store i32 %eh.pointers.7, ptr %st.p.22, align 1  ; 0x40292e
  %sp.15 = sub i32 %rsp.phi.3, 4  ; 0x40292e
  %st.p.23 = getelementptr inbounds i8, ptr %frame.top, i64 -48  ; 0x40292e
  store i32 0, ptr %st.p.23, align 1  ; 0x40292e
  br label %f4_402972
eh.filtered.40292e.0:  ; 0x40292e
  %eh.execute.3 = icmp sgt i32 %cl.z.3, 0  ; 0x402984
  br i1 %eh.execute.3, label %eh.except.40292e.0, label %eh.verdict.40292e.0
f4_402972:  ; 0x402972
  %m.15 = load i32, ptr %st.p.22, align 1  ; 0x402972
  %eax.z.15 = zext i32 %m.15 to i64  ; 0x402972
  %m.p.16 = inttoptr i64 %eax.z.15 to ptr  ; 0x402975
  %m.16 = load i32, ptr %m.p.16, align 1  ; 0x402975
  %eax.z.16 = zext i32 %m.16 to i64  ; 0x402975
  %m.p.17 = inttoptr i64 %eax.z.16 to ptr  ; 0x402979
  %m.17 = load i32, ptr %m.p.17, align 1  ; 0x402979
  %zf.17 = icmp eq i32 %m.17, -1073741819  ; 0x402979
  %cl.z.3 = zext i1 %zf.17 to i32  ; 0x40297f
  %sp.16 = add i32 %sp.15, 4  ; 0x402984
  br label %eh.filtered.40292e.0
eh.except.40292e.0:  ; 0x402984
  call void @ra_eh_handled()  ; 0x402984
  store i32 -2, ptr %st.p.1, align 1  ; 0x402984
  br label %b_402985
eh.verdict.40292e.0:  ; 0x402984
  %eh.continue.3 = icmp slt i32 %cl.z.3, 0  ; 0x402984
  br i1 %eh.continue.3, label %eh.resume.40292e.0, label %eh.search.40292e.0
eh.resume.40292e.0:  ; 0x402984
  call void @ra_eh_handled()  ; 0x402984
  %n = trunc i32 %cl.z.3 to i16
  br label %eh.probe.40292e
eh.search.40292e.0:  ; 0x402984
  call void @ra_eh_unhandled(i64 4204846)  ; 0x402984
  unreachable
eh.access.40292e:  ; 0x402984
  %m.p.18 = inttoptr i64 %eh.at.3 to ptr  ; 0x402984
  %m.18 = load i16, ptr %m.p.18, align 1  ; 0x402984
  %not.2 = icmp ne i16 %m.18, %rcx.phi  ; 0x402935
  br i1 %not.2, label %b_402988, label %b_402937
eh.probe.402937:  ; 0x402937
  %rsp.phi.4 = phi i32 [ %rsp.phi.3, %b_402937 ], [ %sp.18, %eh.resume.402937.0 ]  ; 0x402937
  %eh.fault.4 = call i64 @ra_eh_probe(i64 %varargs, i64 4, i64 0, i64 4204855)  ; 0x402937
  %eh.faults.4 = icmp ne i64 %eh.fault.4, 0  ; 0x402937
  br i1 %eh.faults.4, label %eh.seh.402937, label %eh.access.402937
eh.seh.402937:  ; 0x402937
  %eh.node.4 = add i32 %sp, -16  ; 0x402937
  %eh.handler.frame.4 = add i32 %eh.node.4, 16  ; 0x402937
  %eh.pointers.8 = call i64 @ra_eh_pointers()  ; 0x402937
  %eh.pointers.9 = trunc i64 %eh.pointers.8 to i32  ; 0x402937
  %st.p.25 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x402937
  store i32 %eh.pointers.9, ptr %st.p.25, align 1  ; 0x402937
  %sp.17 = sub i32 %rsp.phi.4, 4  ; 0x402937
  %st.p.26 = getelementptr inbounds i8, ptr %frame.top, i64 -48  ; 0x402937
  store i32 0, ptr %st.p.26, align 1  ; 0x402937
  br label %f5_402972
eh.filtered.402937.0:  ; 0x402937
  %eh.execute.4 = icmp sgt i32 %cl.z.4, 0  ; 0x402984
  br i1 %eh.execute.4, label %eh.except.402937.0, label %eh.verdict.402937.0
f5_402972:  ; 0x402972
  %m.19 = load i32, ptr %st.p.25, align 1  ; 0x402972
  %eax.z.18 = zext i32 %m.19 to i64  ; 0x402972
  %m.p.20 = inttoptr i64 %eax.z.18 to ptr  ; 0x402975
  %m.20 = load i32, ptr %m.p.20, align 1  ; 0x402975
  %eax.z.19 = zext i32 %m.20 to i64  ; 0x402975
  %m.p.21 = inttoptr i64 %eax.z.19 to ptr  ; 0x402979
  %m.21 = load i32, ptr %m.p.21, align 1  ; 0x402979
  %zf.22 = icmp eq i32 %m.21, -1073741819  ; 0x402979
  %cl.z.4 = zext i1 %zf.22 to i32  ; 0x40297f
  %sp.18 = add i32 %sp.17, 4  ; 0x402984
  br label %eh.filtered.402937.0
eh.except.402937.0:  ; 0x402984
  call void @ra_eh_handled()  ; 0x402984
  store i32 -2, ptr %st.p.1, align 1  ; 0x402984
  br label %b_402985
eh.verdict.402937.0:  ; 0x402984
  %eh.continue.4 = icmp slt i32 %cl.z.4, 0  ; 0x402984
  br i1 %eh.continue.4, label %eh.resume.402937.0, label %eh.search.402937.0
eh.resume.402937.0:  ; 0x402984
  call void @ra_eh_handled()  ; 0x402984
  br label %eh.probe.402937
eh.search.402937.0:  ; 0x402984
  call void @ra_eh_unhandled(i64 4204855)  ; 0x402984
  unreachable
eh.access.402937:  ; 0x402984
  %args.p = inttoptr i64 %varargs to ptr  ; 0x402984
  %m.p.22 = getelementptr inbounds i8, ptr %args.p, i64 0  ; 0x402984
  %m.22 = load i32, ptr %m.p.22, align 1  ; 0x402984
  %diff.9 = sub i32 %m.22, ptrtoint (ptr @sec_headers to i32)  ; 0x40293a
  %sp.19 = sub i32 %rsp.phi.4, 4  ; 0x40293f
  %st.p.28 = getelementptr inbounds i8, ptr %frame.top, i64 -48  ; 0x40293f
  store i32 %diff.9, ptr %st.p.28, align 1  ; 0x40293f
  %sp.20 = sub i32 %sp.19, 4  ; 0x402940
  %st.p.29 = getelementptr inbounds i8, ptr %frame.top, i64 -52  ; 0x402940
  store i32 ptrtoint (ptr @sec_headers to i32), ptr %st.p.29, align 1  ; 0x402940
  %sp.21 = sub i32 %sp.20, 4  ; 0x402945
  %sarg0.a = add i32 %sp.21, 4
  %ret = call i32 @F_00402b30(i32 ptrtoint (ptr @sec_headers to i32), i32 %diff.9)  ; 0x402945
  %eh.pending = call i64 @ra_eh_pending()  ; 0x402945
  %eh.thrown = icmp ne i64 %eh.pending, 0  ; 0x402945
  br i1 %eh.thrown, label %eh.landing, label %eh.ok.402945
eh.landing:  ; 0x402945
  call void @ra_trap(i64 4204869, i64 8)  ; 0x402945
  unreachable
eh.ok.402945:  ; 0x402945
  %sum = add i32 %sarg0.a, 8  ; 0x40294a
  %zf.26 = icmp eq i32 %ret, 0  ; 0x40294d
  br i1 %zf.26, label %b_402988, label %b_402951
eh.probe.402951:  ; 0x402951
  %rax.phi.3 = phi i32 [ %ret, %b_402951 ], [ %cl.z.5, %eh.resume.402951.0 ]  ; 0x402951
  %rsp.phi.5 = phi i32 [ %sum, %b_402951 ], [ %sp.23, %eh.resume.402951.0 ]  ; 0x402951
  %addr.16 = add i32 %rax.phi.3, 36  ; 0x402951
  %eh.at.5 = zext i32 %addr.16 to i64  ; 0x402951
  %eh.fault.5 = call i64 @ra_eh_probe(i64 %eh.at.5, i64 4, i64 0, i64 4204881)  ; 0x402951
  %eh.faults.5 = icmp ne i64 %eh.fault.5, 0  ; 0x402951
  br i1 %eh.faults.5, label %eh.seh.402951, label %eh.access.402951
eh.seh.402951:  ; 0x402951
  %eh.node.5 = add i32 %sp, -16  ; 0x402951
  %eh.handler.frame.5 = add i32 %eh.node.5, 16  ; 0x402951
  %eh.pointers.10 = call i64 @ra_eh_pointers()  ; 0x402951
  %eh.pointers.11 = trunc i64 %eh.pointers.10 to i32  ; 0x402951
  %st.p.31 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x402951
  store i32 %eh.pointers.11, ptr %st.p.31, align 1  ; 0x402951
  %sp.22 = sub i32 %rsp.phi.5, 4  ; 0x402951
  store i32 0, ptr %st.p.28, align 1  ; 0x402951
  br label %f6_402972
eh.filtered.402951.0:  ; 0x402951
  %eh.execute.5 = icmp sgt i32 %cl.z.5, 0  ; 0x402984
  br i1 %eh.execute.5, label %eh.except.402951.0, label %eh.verdict.402951.0
f6_402972:  ; 0x402972
  %m.23 = load i32, ptr %st.p.31, align 1  ; 0x402972
  %eax.z.23 = zext i32 %m.23 to i64  ; 0x402972
  %m.p.24 = inttoptr i64 %eax.z.23 to ptr  ; 0x402975
  %m.24 = load i32, ptr %m.p.24, align 1  ; 0x402975
  %eax.z.24 = zext i32 %m.24 to i64  ; 0x402975
  %m.p.25 = inttoptr i64 %eax.z.24 to ptr  ; 0x402979
  %m.25 = load i32, ptr %m.p.25, align 1  ; 0x402979
  %zf.29 = icmp eq i32 %m.25, -1073741819  ; 0x402979
  %cl.z.5 = zext i1 %zf.29 to i32  ; 0x40297f
  %sp.23 = add i32 %sp.22, 4  ; 0x402984
  br label %eh.filtered.402951.0
eh.except.402951.0:  ; 0x402984
  call void @ra_eh_handled()  ; 0x402984
  store i32 -2, ptr %st.p.1, align 1  ; 0x402984
  br label %b_402985
eh.verdict.402951.0:  ; 0x402984
  %eh.continue.5 = icmp slt i32 %cl.z.5, 0  ; 0x402984
  br i1 %eh.continue.5, label %eh.resume.402951.0, label %eh.search.402951.0
eh.resume.402951.0:  ; 0x402984
  call void @ra_eh_handled()  ; 0x402984
  br label %eh.probe.402951
eh.search.402951.0:  ; 0x402984
  call void @ra_eh_unhandled(i64 4204881)  ; 0x402984
  unreachable
eh.access.402951:  ; 0x402984
  %m.p.26 = inttoptr i64 %eh.at.5 to ptr  ; 0x402984
  %m.26 = load i32, ptr %m.p.26, align 1  ; 0x402984
  %sf.21 = icmp slt i32 %m.26, 0  ; 0x402984
  br i1 %sf.21, label %b_402988, label %b_402957
eh.probe.402957:  ; 0x402957
  %rax.phi.4 = phi i32 [ %rax.phi.3, %b_402957 ], [ %cl.z.6, %eh.resume.402957.0 ]  ; 0x402957
  %addr.19 = add i32 %sp, -4  ; 0x402957
  %eh.at.6 = zext i32 %addr.19 to i64  ; 0x402957
  %eh.fault.6 = call i64 @ra_eh_probe(i64 %eh.at.6, i64 4, i64 1, i64 4204887)  ; 0x402957
  %eh.faults.6 = icmp ne i64 %eh.fault.6, 0  ; 0x402957
  br i1 %eh.faults.6, label %eh.seh.402957, label %eh.access.402957
eh.seh.402957:  ; 0x402957
  %eh.node.6 = add i32 %sp, -16  ; 0x402957
  %eh.handler.frame.6 = add i32 %eh.node.6, 16  ; 0x402957
  %eh.pointers.12 = call i64 @ra_eh_pointers()  ; 0x402957
  %eh.pointers.13 = trunc i64 %eh.pointers.12 to i32  ; 0x402957
  %st.p.34 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x402957
  store i32 %eh.pointers.13, ptr %st.p.34, align 1  ; 0x402957
  store i32 0, ptr %st.p.28, align 1  ; 0x402957
  br label %f7_402972
eh.filtered.402957.0:  ; 0x402957
  %eh.execute.6 = icmp sgt i32 %cl.z.6, 0  ; 0x402984
  br i1 %eh.execute.6, label %eh.except.402957.0, label %eh.verdict.402957.0
f7_402972:  ; 0x402972
  %m.27 = load i32, ptr %st.p.34, align 1  ; 0x402972
  %eax.z.26 = zext i32 %m.27 to i64  ; 0x402972
  %m.p.28 = inttoptr i64 %eax.z.26 to ptr  ; 0x402975
  %m.28 = load i32, ptr %m.p.28, align 1  ; 0x402975
  %eax.z.27 = zext i32 %m.28 to i64  ; 0x402975
  %m.p.29 = inttoptr i64 %eax.z.27 to ptr  ; 0x402979
  %m.29 = load i32, ptr %m.p.29, align 1  ; 0x402979
  %zf.33 = icmp eq i32 %m.29, -1073741819  ; 0x402979
  %cl.z.6 = zext i1 %zf.33 to i32  ; 0x40297f
  br label %eh.filtered.402957.0
eh.except.402957.0:  ; 0x402984
  call void @ra_eh_handled()  ; 0x402984
  store i32 -2, ptr %st.p.1, align 1  ; 0x402984
  br label %b_402985
eh.verdict.402957.0:  ; 0x402984
  %eh.continue.6 = icmp slt i32 %cl.z.6, 0  ; 0x402984
  br i1 %eh.continue.6, label %eh.resume.402957.0, label %eh.search.402957.0
eh.resume.402957.0:  ; 0x402984
  call void @ra_eh_handled()  ; 0x402984
  br label %eh.probe.402957
eh.search.402957.0:  ; 0x402984
  call void @ra_eh_unhandled(i64 4204887)  ; 0x402984
  unreachable
eh.access.402957:  ; 0x402984
  store i32 -2, ptr %st.p.1, align 1  ; 0x402984
  %kept.7 = and i32 %rax.phi.4, -256  ; 0x40295e
  %rax.38 = or i32 %kept.7, 1  ; 0x40295e
  %m.30 = load i32, ptr %st.p.4, align 1  ; 0x402960
  %teb.4 = call i64 @ra_teb()  ; 0x402963
  %teb.5 = trunc i64 %teb.4 to i32  ; 0x402963
  %addr.z.76 = zext i32 %teb.5 to i64  ; 0x402963
  %st.p.38 = inttoptr i64 %addr.z.76 to ptr  ; 0x402963
  store i32 %m.30, ptr %st.p.38, align 1  ; 0x402963
  ret i32 %rax.38
eh.probe.402988:  ; 0x402988
  %rax.phi.7 = phi i32 [ %rax.phi.6, %b_402988 ], [ %cl.z.7, %eh.resume.402988.0 ]  ; 0x402988
  %rsp.phi.8 = phi i32 [ %rsp.phi.7, %b_402988 ], [ %sp.33, %eh.resume.402988.0 ]  ; 0x402988
  %addr.24 = add i32 %rbp.phi.8, -4  ; 0x402988
  %eh.at.7 = zext i32 %addr.24 to i64  ; 0x402988
  %eh.fault.7 = call i64 @ra_eh_probe(i64 %eh.at.7, i64 4, i64 1, i64 4204936)  ; 0x402988
  %eh.faults.7 = icmp ne i64 %eh.fault.7, 0  ; 0x402988
  br i1 %eh.faults.7, label %eh.seh.402988, label %eh.access.402988
eh.seh.402988:  ; 0x402988
  %eh.node.7 = add i32 %rbp.phi.8, -16  ; 0x402988
  %eh.handler.frame.7 = add i32 %eh.node.7, 16  ; 0x402988
  %eh.pointers.14 = call i64 @ra_eh_pointers()  ; 0x402988
  %eh.pointers.15 = trunc i64 %eh.pointers.14 to i32  ; 0x402988
  %st.p.39 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x402988
  store i32 %eh.pointers.15, ptr %st.p.39, align 1  ; 0x402988
  %sp.32 = sub i32 %rsp.phi.8, 4  ; 0x402988
  %addr.z.85 = zext i32 %sp.32 to i64  ; 0x402988
  %st.p.40 = inttoptr i64 %addr.z.85 to ptr  ; 0x402988
  store i32 0, ptr %st.p.40, align 1  ; 0x402988
  br label %f8_402972
eh.filtered.402988.0:  ; 0x402988
  %eh.execute.7 = icmp sgt i32 %cl.z.7, 0  ; 0x402984
  br i1 %eh.execute.7, label %eh.except.402988.0, label %eh.verdict.402988.0
f8_402972:  ; 0x402972
  %m.32 = load i32, ptr %st.p.39, align 1  ; 0x402972
  %eax.z.29 = zext i32 %m.32 to i64  ; 0x402972
  %m.p.33 = inttoptr i64 %eax.z.29 to ptr  ; 0x402975
  %m.33 = load i32, ptr %m.p.33, align 1  ; 0x402975
  %eax.z.30 = zext i32 %m.33 to i64  ; 0x402975
  %m.p.34 = inttoptr i64 %eax.z.30 to ptr  ; 0x402979
  %m.34 = load i32, ptr %m.p.34, align 1  ; 0x402979
  %zf.36 = icmp eq i32 %m.34, -1073741819  ; 0x402979
  %cl.z.7 = zext i1 %zf.36 to i32  ; 0x40297f
  %sp.33 = add i32 %sp.32, 4  ; 0x402984
  br label %eh.filtered.402988.0
eh.except.402988.0:  ; 0x402984
  call void @ra_eh_handled()  ; 0x402984
  store i32 -2, ptr %st.p.1, align 1  ; 0x402984
  br label %b_402985
eh.verdict.402988.0:  ; 0x402984
  %eh.continue.7 = icmp slt i32 %cl.z.7, 0  ; 0x402984
  br i1 %eh.continue.7, label %eh.resume.402988.0, label %eh.search.402988.0
eh.resume.402988.0:  ; 0x402984
  call void @ra_eh_handled()  ; 0x402984
  br label %eh.probe.402988
eh.search.402988.0:  ; 0x402984
  call void @ra_eh_unhandled(i64 4204936)  ; 0x402984
  unreachable
eh.access.402988:  ; 0x402984
  store i32 -2, ptr %st.p.1, align 1  ; 0x402984
  %kept.9 = and i32 %rax.phi.7, -256  ; 0x40298f
  %m.35 = load i32, ptr %st.p.4, align 1  ; 0x402991
  %teb.6 = call i64 @ra_teb()  ; 0x402994
  %teb.7 = trunc i64 %teb.6 to i32  ; 0x402994
  %addr.z.93 = zext i32 %teb.7 to i64  ; 0x402994
  %st.p.43 = inttoptr i64 %addr.z.93 to ptr  ; 0x402994
  store i32 %m.35, ptr %st.p.43, align 1  ; 0x402994
  ret i32 %kept.9
}

define i64 @F_004029b0() {  ; 0x4029b0
b_4029b0:  ; 0x4029b0
  %local_m4 = alloca i32, align 4
  %esi = trunc i64 undef to i32  ; 0x4029b0
  store i32 %esi, ptr %local_m4, align 1  ; 0x4029b0
  %ret = call i64 @F_004031d0()  ; 0x4029b1
  %eh.pending = call i64 @ra_eh_pending()  ; 0x4029b1
  %eh.thrown = icmp ne i64 %eh.pending, 0  ; 0x4029b1
  br i1 %eh.thrown, label %eh.landing, label %eh.ok.4029b1
b_4029ba:  ; 0x4029ba
  %teb = call i64 @ra_teb()  ; 0x4029ba
  %teb.1 = trunc i64 %teb to i32  ; 0x4029ba
  %teb.addr = add i32 24, %teb.1  ; 0x4029ba
  %addr.z.2 = zext i32 %teb.addr to i64  ; 0x4029ba
  %m.p = inttoptr i64 %addr.z.2 to ptr  ; 0x4029ba
  %m = load i32, ptr %m.p, align 1  ; 0x4029ba
  %esi.z = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 852) to i32) to i64  ; 0x4029c0
  %addr = add i32 %m, 4  ; 0x4029c5
  %addr.z.3 = zext i32 %addr to i64  ; 0x4029c5
  %m.p.1 = inttoptr i64 %addr.z.3 to ptr  ; 0x4029c5
  %m.1 = load i32, ptr %m.p.1, align 1  ; 0x4029c5
  %cx.p = inttoptr i64 %esi.z to ptr  ; 0x4029cc
  %old.pair = cmpxchg ptr %cx.p, i32 0, i32 %m.1 seq_cst seq_cst  ; 0x4029cc
  %old = extractvalue { i32, i1 } %old.pair, 0
  %zf.3 = icmp eq i32 0, %old  ; 0x4029cc
  br i1 %zf.3, label %cx.done, label %cx.keep
b_4029d4:  ; 0x4029d4
  %rax.phi.1 = phi i64 [ %rax.phi, %cx.done ], [ %rax.phi.2, %cx.done.1 ]  ; 0x4029d4
  %eax.6 = trunc i64 %rax.phi.1 to i32  ; 0x4029d4
  %zf.7 = icmp eq i32 %m.1, %eax.6  ; 0x4029d4
  br i1 %zf.7, label %b_4029e8, label %b_4029d8
b_4029d8:  ; 0x4029d8
  %old.1.pair = cmpxchg ptr %cx.p, i32 0, i32 %m.1 seq_cst seq_cst  ; 0x4029dc
  %old.1 = extractvalue { i32, i1 } %old.1.pair, 0
  %zf.10 = icmp eq i32 0, %old.1  ; 0x4029dc
  br i1 %zf.10, label %cx.done.1, label %cx.keep.1
b_4029e4:  ; 0x4029e4
  %rax.phi.3 = phi i64 [ %ret, %eh.ok.4029b1 ], [ %rax.phi, %cx.done ], [ %rax.phi.2, %cx.done.1 ]  ; 0x4029e4
  %kept = and i64 %rax.phi.3, -256  ; 0x4029e4
  ret i64 %kept
b_4029e8:  ; 0x4029e8
  %kept.1 = and i64 %rax.phi.1, -256  ; 0x4029e8
  %rax.13 = or i64 %kept.1, 1  ; 0x4029e8
  ret i64 %rax.13
eh.landing:  ; 0x4029b1
  ret i64 %ret
eh.ok.4029b1:  ; 0x4029b1
  %eax = trunc i64 %ret to i32  ; 0x4029b6
  %zf = icmp eq i32 %eax, 0  ; 0x4029b6
  br i1 %zf, label %b_4029e4, label %b_4029ba
cx.keep:  ; 0x4029cc
  %eax.z.2 = zext i32 %old to i64  ; 0x4029cc
  br label %cx.done
cx.done:  ; 0x4029cc
  %rax.phi = phi i64 [ 0, %b_4029ba ], [ %eax.z.2, %cx.keep ]  ; 0x4029cc
  %eax.4 = trunc i64 %rax.phi to i32  ; 0x4029d0
  %zf.5 = icmp eq i32 %eax.4, 0  ; 0x4029d0
  br i1 %zf.5, label %b_4029e4, label %b_4029d4
cx.keep.1:  ; 0x4029dc
  %eax.z.4 = zext i32 %old.1 to i64  ; 0x4029dc
  br label %cx.done.1
cx.done.1:  ; 0x4029dc
  %rax.phi.2 = phi i64 [ 0, %b_4029d8 ], [ %eax.z.4, %cx.keep.1 ]  ; 0x4029dc
  %eax.8 = trunc i64 %rax.phi.2 to i32  ; 0x4029e0
  %not = icmp ne i32 %eax.8, 0  ; 0x4029e2
  br i1 %not, label %b_4029d4, label %b_4029e4
}

define void @F_004029f0(i32 %arg0) {  ; 0x4029f0
b_4029f0:  ; 0x4029f0
  %frame = alloca [124 x i8], align 16
  %frame.top = getelementptr inbounds i8, ptr %frame, i64 44
  %local_m4 = alloca i32, align 4
  %stack.arg0.p = getelementptr inbounds i8, ptr %frame.top, i64 4
  store i32 %arg0, ptr %stack.arg0.p, align 4
  %ebp = trunc i64 undef to i32  ; 0x4029f0
  store i32 %ebp, ptr %local_m4, align 1  ; 0x4029f0
  %ret = call i64 @F_004031d0()  ; 0x4029f3
  %eh.pending = call i64 @ra_eh_pending()  ; 0x4029f3
  %eh.thrown = icmp ne i64 %eh.pending, 0  ; 0x4029f3
  br i1 %eh.thrown, label %eh.landing, label %eh.ok.4029f3
b_4029fc:  ; 0x4029fc
  %m = load i8, ptr %stack.arg0.p, align 1  ; 0x4029fc
  %not = icmp ne i8 %m, 0  ; 0x402a00
  br i1 %not, label %b_402a0b, label %b_402a02
b_402a02:  ; 0x402a02
  %ecx.z = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 852) to i32) to i64  ; 0x402a04
  %at.p = inttoptr i64 %ecx.z to ptr  ; 0x402a09
  %old = atomicrmw xchg ptr %at.p, i32 0 seq_cst  ; 0x402a09
  br label %b_402a0b
b_402a0b:  ; 0x402a0b
  ret void
eh.landing:  ; 0x4029f3
  ret void
eh.ok.4029f3:  ; 0x4029f3
  %eax = trunc i64 %ret to i32  ; 0x4029f8
  %zf = icmp eq i32 %eax, 0  ; 0x4029f8
  br i1 %zf, label %b_402a0b, label %b_4029fc
}

define i8 @F_00402a10(i32 %arg0) {  ; 0x402a10
b_402a10:  ; 0x402a10
  %local_m8 = alloca i32, align 4
  %local_m4 = alloca i32, align 4
  %local_p4 = alloca i32, align 4
  store i32 %arg0, ptr %local_p4, align 4
  %ebp = trunc i64 undef to i32  ; 0x402a10
  store i32 %ebp, ptr %local_m4, align 1  ; 0x402a10
  %not = icmp ne i32 %arg0, 0  ; 0x402a17
  br i1 %not, label %b_402a20, label %b_402a19
b_402a19:  ; 0x402a19
  %addr.z.2 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 856) to i32) to i64  ; 0x402a19
  %st.p.1 = inttoptr i64 %addr.z.2 to ptr  ; 0x402a19
  store i8 1, ptr %st.p.1, align 1  ; 0x402a19
  br label %b_402a20
b_402a20:  ; 0x402a20
  %ret = call i8 @F_00402ea0()  ; 0x402a20
  %eh.pending = call i64 @ra_eh_pending()  ; 0x402a20
  %eh.thrown = icmp ne i64 %eh.pending, 0  ; 0x402a20
  br i1 %eh.thrown, label %eh.landing, label %eh.ok.402a20
b_402a2e:  ; 0x402a2e
  %ret.2 = call i8 @F_00402c70()  ; 0x402a2e
  %eh.pending.2 = call i64 @ra_eh_pending()  ; 0x402a2e
  %eh.thrown.2 = icmp ne i64 %eh.pending.2, 0  ; 0x402a2e
  br i1 %eh.thrown.2, label %eh.landing, label %eh.ok.402a2e
b_402a37:  ; 0x402a37
  store i32 0, ptr %local_m8, align 1  ; 0x402a37
  %ret.3 = call i8 @F_00402c70()  ; 0x402a39
  %eh.pending.3 = call i64 @ra_eh_pending()  ; 0x402a39
  %eh.thrown.3 = icmp ne i64 %eh.pending.3, 0  ; 0x402a39
  br i1 %eh.thrown.3, label %eh.landing, label %eh.ok.402a39
b_402a41:  ; 0x402a41
  ret i8 0
b_402a45:  ; 0x402a45
  ret i8 1
eh.landing:  ; 0x402a20
  %rax.phi.1 = phi i8 [ %ret, %b_402a20 ], [ %ret.2, %b_402a2e ], [ %ret.3, %b_402a37 ], [ %ret.1, %eh.ok.402a20 ]  ; 0x402a20
  ret i8 %rax.phi.1
eh.ok.402a20:  ; 0x402a20
  %ret.1 = call i8 @F_00402c70()  ; 0x402a25
  %eh.pending.1 = call i64 @ra_eh_pending()  ; 0x402a25
  %eh.thrown.1 = icmp ne i64 %eh.pending.1, 0  ; 0x402a25
  br i1 %eh.thrown.1, label %eh.landing, label %eh.ok.402a25
eh.ok.402a25:  ; 0x402a25
  %zf.2 = icmp eq i8 %ret.1, 0  ; 0x402a2a
  br i1 %zf.2, label %b_402a41, label %b_402a2e
eh.ok.402a2e:  ; 0x402a2e
  %not.1 = icmp ne i8 %ret.2, 0  ; 0x402a35
  br i1 %not.1, label %b_402a45, label %b_402a37
eh.ok.402a39:  ; 0x402a39
  br label %b_402a41
}

define void @F_00402a50(i32 %arg0, i32 %arg1) {  ; 0x402a50
b_402a50:  ; 0x402a50
  %frame = alloca [128 x i8], align 16
  %frame.top = getelementptr inbounds i8, ptr %frame, i64 44
  %local_mc = alloca i32, align 4
  %local_m8 = alloca i32, align 4
  %local_m4 = alloca i32, align 4
  %local_p4 = alloca i32, align 4
  store i32 %arg0, ptr %local_p4, align 4
  %stack.arg1.p = getelementptr inbounds i8, ptr %frame.top, i64 8
  store i32 %arg1, ptr %stack.arg1.p, align 4
  %ebp = trunc i64 undef to i32  ; 0x402a50
  store i32 %ebp, ptr %local_m4, align 1  ; 0x402a50
  %addr.z.1 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 856) to i32) to i64  ; 0x402a53
  %m.p = inttoptr i64 %addr.z.1 to ptr  ; 0x402a53
  %m = load i8, ptr %m.p, align 1  ; 0x402a53
  %zf = icmp eq i8 %m, 0  ; 0x402a53
  br i1 %zf, label %b_402a62, label %b_402a5c
b_402a5c:  ; 0x402a5c
  %m.1 = load i8, ptr %stack.arg1.p, align 1  ; 0x402a5c
  %not = icmp ne i8 %m.1, 0  ; 0x402a60
  br i1 %not, label %b_402a75, label %b_402a62
b_402a62:  ; 0x402a62
  %m.2 = load i32, ptr %local_p4, align 1  ; 0x402a62
  store i32 %m.2, ptr %local_m8, align 1  ; 0x402a62
  %ret = call i8 @F_00402c70()  ; 0x402a65
  %eh.pending = call i64 @ra_eh_pending()  ; 0x402a65
  %eh.thrown = icmp ne i64 %eh.pending, 0  ; 0x402a65
  br i1 %eh.thrown, label %eh.landing, label %eh.ok.402a65
b_402a75:  ; 0x402a75
  ret void
eh.landing:  ; 0x402a65
  ret void
eh.ok.402a65:  ; 0x402a65
  %m.3 = load i32, ptr %local_p4, align 1  ; 0x402a6a
  store i32 %m.3, ptr %local_mc, align 1  ; 0x402a6a
  %ret.1 = call i8 @F_00402c70()  ; 0x402a6d
  %eh.pending.1 = call i64 @ra_eh_pending()  ; 0x402a6d
  %eh.thrown.1 = icmp ne i64 %eh.pending.1, 0  ; 0x402a6d
  br i1 %eh.thrown.1, label %eh.landing, label %eh.ok.402a6d
eh.ok.402a6d:  ; 0x402a6d
  br label %b_402a75
}

define i64 @F_00402a80(i32 %arg0) {  ; 0x402a80
b_402a80:  ; 0x402a80
  %local_mc = alloca i32, align 4
  %local_m8 = alloca i32, align 4
  %local_m4 = alloca i32, align 4
  %local_p4 = alloca i32, align 4
  store i32 %arg0, ptr %local_p4, align 4
  %ebp = trunc i64 undef to i32  ; 0x402a80
  store i32 %ebp, ptr %local_m4, align 1  ; 0x402a80
  %addr.z.1 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 857) to i32) to i64  ; 0x402a83
  %m.p = inttoptr i64 %addr.z.1 to ptr  ; 0x402a83
  %m = load i8, ptr %m.p, align 1  ; 0x402a83
  %zf = icmp eq i8 %m, 0  ; 0x402a83
  br i1 %zf, label %b_402a90, label %b_402a8c
b_402a8c:  ; 0x402a8c
  %kept = and i64 undef, -256  ; 0x402a8c
  %rax.1 = or i64 %kept, 1  ; 0x402a8c
  ret i64 %rax.1
b_402a90:  ; 0x402a90
  store i32 %ebp, ptr %local_m8, align 1  ; 0x402a90
  %m.1 = load i32, ptr %local_p4, align 1  ; 0x402a91
  %zf.2 = icmp eq i32 %m.1, 0  ; 0x402a94
  br i1 %zf.2, label %b_402a9d, label %b_402a98
b_402a98:  ; 0x402a98
  %not = icmp ne i32 %m.1, 1  ; 0x402a9b
  br i1 %not, label %b_402b19, label %b_402a9d
b_402a9d:  ; 0x402a9d
  %ret = call i64 @F_004031d0()  ; 0x402a9d
  %eh.pending = call i64 @ra_eh_pending()  ; 0x402a9d
  %eh.thrown = icmp ne i64 %eh.pending, 0  ; 0x402a9d
  br i1 %eh.thrown, label %eh.landing, label %eh.ok.402a9d
b_402aa6:  ; 0x402aa6
  %not.1 = icmp ne i32 %m.1, 0  ; 0x402aa8
  br i1 %not.1, label %b_402ad1, label %b_402aaa
b_402aaa:  ; 0x402aaa
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 860) to i32), ptr %local_mc, align 1  ; 0x402aaa
  %sarg0.1 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 860) to i32) to i64
  %ret.1 = call i64 @thunk__initialize_onexit_table(i64 %sarg0.1)  ; 0x402aaf
  %eh.pending.1 = call i64 @ra_eh_pending()  ; 0x402aaf
  %eh.thrown.1 = icmp ne i64 %eh.pending.1, 0  ; 0x402aaf
  br i1 %eh.thrown.1, label %eh.landing, label %eh.ok.402aaf
b_402abb:  ; 0x402abb
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 872) to i32), ptr %local_mc, align 1  ; 0x402abb
  %sarg0.2 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 872) to i32) to i64
  %ret.2 = call i64 @thunk__initialize_onexit_table(i64 %sarg0.2)  ; 0x402ac0
  %eh.pending.2 = call i64 @ra_eh_pending()  ; 0x402ac0
  %eh.thrown.2 = icmp ne i64 %eh.pending.2, 0  ; 0x402ac0
  br i1 %eh.thrown.2, label %eh.landing, label %eh.ok.402ac0
b_402acc:  ; 0x402acc
  %rax.phi = phi i64 [ %ret.1, %eh.ok.402aaf ], [ %ret.2, %eh.ok.402ac0 ]  ; 0x402acc
  %kept.1 = and i64 %rax.phi, -256  ; 0x402acc
  ret i64 %kept.1
b_402ad1:  ; 0x402ad1
  %addr.z.14 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 860) to i32) to i64  ; 0x402ad1
  %st.p.7 = inttoptr i64 %addr.z.14 to ptr  ; 0x402ad1
  store i32 -1, ptr %st.p.7, align 1  ; 0x402ad1
  %addr.z.15 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 864) to i32) to i64  ; 0x402adb
  %st.p.8 = inttoptr i64 %addr.z.15 to ptr  ; 0x402adb
  store i32 -1, ptr %st.p.8, align 1  ; 0x402adb
  %addr.z.16 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 868) to i32) to i64  ; 0x402ae5
  %st.p.9 = inttoptr i64 %addr.z.16 to ptr  ; 0x402ae5
  store i32 -1, ptr %st.p.9, align 1  ; 0x402ae5
  %addr.z.17 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 872) to i32) to i64  ; 0x402aef
  %st.p.10 = inttoptr i64 %addr.z.17 to ptr  ; 0x402aef
  store i32 -1, ptr %st.p.10, align 1  ; 0x402aef
  %addr.z.18 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 876) to i32) to i64  ; 0x402af9
  %st.p.11 = inttoptr i64 %addr.z.18 to ptr  ; 0x402af9
  store i32 -1, ptr %st.p.11, align 1  ; 0x402af9
  %addr.z.19 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 880) to i32) to i64  ; 0x402b03
  %st.p.12 = inttoptr i64 %addr.z.19 to ptr  ; 0x402b03
  store i32 -1, ptr %st.p.12, align 1  ; 0x402b03
  br label %b_402b0d
b_402b0d:  ; 0x402b0d
  %rax.phi.2 = phi i64 [ %ret, %b_402ad1 ], [ %ret.2, %eh.ok.402ac0 ]  ; 0x402b0d
  store i8 1, ptr %m.p, align 1  ; 0x402b0d
  %kept.2 = and i64 %rax.phi.2, -256  ; 0x402b14
  %rax.11 = or i64 %kept.2, 1  ; 0x402b14
  ret i64 %rax.11
b_402b19:  ; 0x402b19
  store i32 5, ptr %local_mc, align 1  ; 0x402b19
  call void @F_00402da0(i32 5)  ; 0x402b1b
  %eh.pending.3 = call i64 @ra_eh_pending()  ; 0x402b1b
  %eh.thrown.3 = icmp ne i64 %eh.pending.3, 0  ; 0x402b1b
  br i1 %eh.thrown.3, label %eh.landing, label %eh.ok.402b1b
eh.landing:  ; 0x402a9d
  %rax.phi.3 = phi i64 [ %ret, %b_402a9d ], [ %ret.1, %b_402aaa ], [ %ret.2, %b_402abb ], [ undef, %b_402b19 ]  ; 0x402a9d
  ret i64 %rax.phi.3
eh.ok.402a9d:  ; 0x402a9d
  %eax = trunc i64 %ret to i32  ; 0x402aa2
  %zf.6 = icmp eq i32 %eax, 0  ; 0x402aa2
  br i1 %zf.6, label %b_402ad1, label %b_402aa6
eh.ok.402aaf:  ; 0x402aaf
  %eax.2 = trunc i64 %ret.1 to i32  ; 0x402ab7
  %not.2 = icmp ne i32 %eax.2, 0  ; 0x402ab9
  br i1 %not.2, label %b_402acc, label %b_402abb
eh.ok.402ac0:  ; 0x402ac0
  %eax.4 = trunc i64 %ret.2 to i32  ; 0x402ac8
  %zf.14 = icmp eq i32 %eax.4, 0  ; 0x402ac8
  br i1 %zf.14, label %b_402b0d, label %b_402acc
eh.ok.402b1b:  ; 0x402b1b
  unreachable
}

define i32 @F_00402b30(i32 %arg0, i32 %arg1) {  ; 0x402b30
b_402b30:  ; 0x402b30
  %local_mc = alloca i32, align 4
  %local_m8 = alloca i32, align 4
  %local_m4 = alloca i32, align 4
  %local_p4 = alloca i32, align 4
  %local_p8 = alloca i32, align 4
  store i32 %arg0, ptr %local_p4, align 4
  store i32 %arg1, ptr %local_p8, align 4
  %ebp = trunc i64 undef to i32  ; 0x402b30
  store i32 %ebp, ptr %local_m4, align 1  ; 0x402b30
  store i32 %ebp, ptr %local_m8, align 1  ; 0x402b36
  store i32 %ebp, ptr %local_mc, align 1  ; 0x402b37
  %addr.1 = add i32 %arg0, 60  ; 0x402b38
  %addr.z.4 = zext i32 %addr.1 to i64  ; 0x402b38
  %m.p.1 = inttoptr i64 %addr.z.4 to ptr  ; 0x402b38
  %m.1 = load i32, ptr %m.p.1, align 1  ; 0x402b38
  %sum = add i32 %m.1, %arg0  ; 0x402b3b
  %sum.s.w = zext i32 %sum to i64  ; 0x402b3b
  %sum.s = inttoptr i64 %sum.s.w to ptr  ; 0x402b3b
  %sum.f20 = getelementptr inbounds %S4, ptr %sum.s, i64 0, i32 3  ; 0x402b3d
  %m.2 = load i16, ptr %sum.f20, align 1  ; 0x402b3d
  %eax.z.1 = zext i16 %m.2 to i32  ; 0x402b3d
  %sum.1 = add i32 %eax.z.1, 24  ; 0x402b41
  %sum.2 = add i32 %sum.1, %sum  ; 0x402b44
  %sum.f6 = getelementptr inbounds %S4, ptr %sum.s, i64 0, i32 1  ; 0x402b46
  %m.3 = load i16, ptr %sum.f6, align 1  ; 0x402b46
  %ecx.z.2 = zext i16 %m.3 to i32  ; 0x402b46
  %scaled.1 = mul i32 %ecx.z.2, 40  ; 0x402b4d
  %addr.5 = add i32 %sum.2, %scaled.1  ; 0x402b4d
  %zf.3 = icmp eq i32 %sum.2, %addr.5  ; 0x402b50
  br i1 %zf.3, label %b_402b6e, label %b_402b54
b_402b54:  ; 0x402b54
  %m.4 = load i32, ptr %local_p8, align 1  ; 0x402b54
  br label %b_402b57
b_402b57:  ; 0x402b57
  %rax.phi = phi i32 [ %sum.2, %b_402b54 ], [ %sum.4, %b_402b67 ]  ; 0x402b57
  %rax.phi.s.w = zext i32 %rax.phi to i64  ; 0x402b57
  %rax.phi.s = inttoptr i64 %rax.phi.s.w to ptr  ; 0x402b57
  %rax.phi.f12 = getelementptr inbounds %S5, ptr %rax.phi.s, i64 0, i32 2  ; 0x402b57
  %m.5 = load i32, ptr %rax.phi.f12, align 1  ; 0x402b57
  %cf.4 = icmp ult i32 %m.4, %m.5  ; 0x402b5a
  br i1 %cf.4, label %b_402b67, label %b_402b5e
b_402b5e:  ; 0x402b5e
  %rax.phi.f8 = getelementptr inbounds %S5, ptr %rax.phi.s, i64 0, i32 1  ; 0x402b5e
  %m.6 = load i32, ptr %rax.phi.f8, align 1  ; 0x402b5e
  %sum.3 = add i32 %m.6, %m.5  ; 0x402b61
  %cf.7 = icmp ult i32 %m.4, %sum.3  ; 0x402b63
  br i1 %cf.7, label %b_402b70, label %b_402b67
b_402b67:  ; 0x402b67
  %sum.4 = add i32 %rax.phi, 40  ; 0x402b67
  %not = icmp ne i32 %sum.4, %addr.5  ; 0x402b6c
  br i1 %not, label %b_402b57, label %b_402b6e
b_402b6e:  ; 0x402b6e
  br label %b_402b70
b_402b70:  ; 0x402b70
  %rax.phi.2 = phi i32 [ %rax.phi, %b_402b5e ], [ 0, %b_402b6e ]  ; 0x402b70
  ret i32 %rax.phi.2
}

define i64 @F_00402b80() {  ; 0x402b80
b_402b80:  ; 0x402b80
  %xmm = alloca [16 x i128], align 16
  %frame = alloca [136 x i8], align 16
  %frame.top = getelementptr inbounds i8, ptr %frame, i64 60
  %rsp0 = ptrtoint ptr %frame.top to i64
  %ebp = trunc i64 undef to i32  ; 0x402b80
  %esp = trunc i64 %rsp0 to i32  ; 0x402b80
  %sp = sub i32 %esp, 4  ; 0x402b80
  %st.p = getelementptr inbounds i8, ptr %frame.top, i64 -4  ; 0x402b80
  store i32 %ebp, ptr %st.p, align 1  ; 0x402b80
  %addr.z.1 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 192) to i32) to i64  ; 0x402b83
  %m.p = inttoptr i64 %addr.z.1 to ptr  ; 0x402b83
  %m = load i32, ptr %m.p, align 1  ; 0x402b83
  %zf.1 = icmp eq i32 %m, -1153374642  ; 0x402b8b
  br i1 %zf.1, label %b_402ba4, label %b_402b92
b_402b92:  ; 0x402b92
  %logic = and i32 %m, -65536  ; 0x402b92
  %zf.3 = icmp eq i32 %logic, 0  ; 0x402b92
  br i1 %zf.3, label %b_402ba4, label %b_402b99
b_402b99:  ; 0x402b99
  %not = xor i32 %m, -1  ; 0x402b99
  %eax.z.1 = zext i32 %not to i64  ; 0x402b99
  %addr.z.2 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 256) to i32) to i64  ; 0x402b9b
  %st.p.1 = inttoptr i64 %addr.z.2 to ptr  ; 0x402b9b
  store i32 %not, ptr %st.p.1, align 1  ; 0x402b9b
  ret i64 %eax.z.1
b_402ba4:  ; 0x402ba4
  %addr = add i32 %sp, -12  ; 0x402ba4
  %p.xmm0 = getelementptr inbounds [16 x i128], ptr %xmm, i64 0, i64 0  ; 0x402ba7
  store i128 0, ptr %p.xmm0, align 16  ; 0x402ba7
  %st.p.2 = getelementptr inbounds i8, ptr %frame.top, i64 -28  ; 0x402baa
  store i32 %addr, ptr %st.p.2, align 1  ; 0x402baa
  %st.p.3 = getelementptr inbounds i8, ptr %frame.top, i64 -16  ; 0x402bab
  store i64 0, ptr %st.p.3, align 1  ; 0x402bab
  %sarg0 = zext i32 %addr to i64
  %ret = call i64 @GetSystemTimeAsFileTime(i64 %sarg0)  ; 0x402bb0
  %m.p.1 = getelementptr inbounds i8, ptr %frame.top, i64 -12  ; 0x402bb6
  %m.1 = load i32, ptr %m.p.1, align 1  ; 0x402bb6
  %m.2 = load i32, ptr %st.p.3, align 1  ; 0x402bb9
  %logic.1 = xor i32 %m.1, %m.2  ; 0x402bb9
  %addr.4 = add i32 %sp, -4  ; 0x402bbc
  %addr.z.9 = zext i32 %addr.4 to i64  ; 0x402bbc
  %st.p.4 = getelementptr inbounds i8, ptr %frame.top, i64 -8  ; 0x402bbc
  store i32 %logic.1, ptr %st.p.4, align 1  ; 0x402bbc
  %ret.1 = call i64 @GetCurrentThreadId()  ; 0x402bbf
  %m.3 = load i32, ptr %st.p.4, align 1  ; 0x402bc5
  %eax.7 = trunc i64 %ret.1 to i32  ; 0x402bc5
  %logic.2 = xor i32 %m.3, %eax.7  ; 0x402bc5
  store i32 %logic.2, ptr %st.p.4, align 1  ; 0x402bc5
  %ret.2 = call i64 @GetCurrentProcessId()  ; 0x402bc8
  %m.4 = load i32, ptr %st.p.4, align 1  ; 0x402bce
  %eax.8 = trunc i64 %ret.2 to i32  ; 0x402bce
  %logic.3 = xor i32 %m.4, %eax.8  ; 0x402bce
  store i32 %logic.3, ptr %st.p.4, align 1  ; 0x402bce
  %addr.9 = add i32 %sp, -20  ; 0x402bd1
  store i32 %addr.9, ptr %st.p.2, align 1  ; 0x402bd4
  %sarg0.3 = zext i32 %addr.9 to i64
  %ret.3 = call i64 @QueryPerformanceCounter(i64 %sarg0.3)  ; 0x402bd5
  %m.p.5 = getelementptr inbounds i8, ptr %frame.top, i64 -20  ; 0x402bdb
  %m.5 = load i32, ptr %m.p.5, align 1  ; 0x402bdb
  %m.p.6 = getelementptr inbounds i8, ptr %frame.top, i64 -24  ; 0x402be1
  %m.6 = load i32, ptr %m.p.6, align 1  ; 0x402be1
  %logic.4 = xor i32 %m.5, %m.6  ; 0x402be1
  %m.7 = load i32, ptr %st.p.4, align 1  ; 0x402be4
  %logic.5 = xor i32 %logic.4, %m.7  ; 0x402be4
  %logic.6 = xor i32 %logic.5, %addr.4  ; 0x402be7
  %not.1 = icmp ne i32 %logic.6, -1153374642  ; 0x402bef
  br i1 %not.1, label %b_402c08, label %b_402bf1
b_402bf1:  ; 0x402bf1
  store i32 -1153374641, ptr %m.p, align 1  ; 0x402bf6
  %addr.z.19 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 256) to i32) to i64  ; 0x402bfe
  %st.p.9 = inttoptr i64 %addr.z.19 to ptr  ; 0x402bfe
  store i32 1153374640, ptr %st.p.9, align 1  ; 0x402bfe
  ret i64 %addr.z.9
b_402c08:  ; 0x402c08
  %logic.7 = and i32 %logic.6, -65536  ; 0x402c08
  %not.3 = icmp ne i32 %logic.7, 0  ; 0x402c0e
  br i1 %not.3, label %b_402c1c, label %b_402c10
b_402c10:  ; 0x402c10
  %logic.8 = or i32 %logic.6, 18193  ; 0x402c12
  %shifted.w = shl i32 %logic.8, 16  ; 0x402c17
  %eax.z.9 = zext i32 %shifted.w to i64  ; 0x402c17
  %logic.9 = or i32 %logic.6, %shifted.w  ; 0x402c1a
  br label %b_402c1c
b_402c1c:  ; 0x402c1c
  %rax.phi = phi i64 [ %addr.z.9, %b_402c08 ], [ %eax.z.9, %b_402c10 ]  ; 0x402c1c
  %rcx.phi = phi i32 [ %logic.6, %b_402c08 ], [ %logic.9, %b_402c10 ]  ; 0x402c1c
  store i32 %rcx.phi, ptr %m.p, align 1  ; 0x402c1c
  %not.4 = xor i32 %rcx.phi, -1  ; 0x402c22
  %addr.z.23 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 256) to i32) to i64  ; 0x402c24
  %st.p.11 = inttoptr i64 %addr.z.23 to ptr  ; 0x402c24
  store i32 %not.4, ptr %st.p.11, align 1  ; 0x402c24
  ret i64 %rax.phi
}

define i64 @F_00402c30() {  ; 0x402c30
b_402c30:  ; 0x402c30
  ret i64 0
}

define i64 @F_00402c40() {  ; 0x402c40
b_402c40:  ; 0x402c40
  ret i64 1
}

define i64 @F_00402c50() {  ; 0x402c50
b_402c50:  ; 0x402c50
  ret i64 16384
}

define void @F_00402c60() {  ; 0x402c60
b_402c60:  ; 0x402c60
  %local_m4 = alloca i32, align 4
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 888) to i32), ptr %local_m4, align 1  ; 0x402c60
  %sarg0 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 888) to i32) to i64
  %ret = call i64 @ra_x86_initialize_slist_head(i64 %sarg0)  ; 0x402c65
  ret void
}

define i8 @F_00402c70() {  ; 0x402c70
b_402c70:  ; 0x402c70
  ret i8 1
}

define void @F_00402c80() {  ; 0x402c80
b_402c80:  ; 0x402c80
  %local_mc = alloca i32, align 4
  %local_m8 = alloca i32, align 4
  %local_m4 = alloca i32, align 4
  store i32 196608, ptr %local_m4, align 1  ; 0x402c80
  store i32 65536, ptr %local_m8, align 1  ; 0x402c85
  store i32 0, ptr %local_mc, align 1  ; 0x402c8a
  %ret = call i64 @thunk__controlfp_s(i64 0, i64 65536, i64 196608)  ; 0x402c8c
  %eh.pending = call i64 @ra_eh_pending()  ; 0x402c8c
  %eh.thrown = icmp ne i64 %eh.pending, 0  ; 0x402c8c
  br i1 %eh.thrown, label %eh.landing, label %eh.ok.402c8c
b_402c98:  ; 0x402c98
  ret void
b_402c99:  ; 0x402c99
  store i32 7, ptr %local_m4, align 1  ; 0x402c99
  call void @F_00402da0(i32 7)  ; 0x402c9b
  %eh.pending.1 = call i64 @ra_eh_pending()  ; 0x402c9b
  %eh.thrown.1 = icmp ne i64 %eh.pending.1, 0  ; 0x402c9b
  br i1 %eh.thrown.1, label %eh.landing, label %eh.ok.402c9b
eh.landing:  ; 0x402c8c
  ret void
eh.ok.402c8c:  ; 0x402c8c
  %eax = trunc i64 %ret to i32  ; 0x402c94
  %not = icmp ne i32 %eax, 0  ; 0x402c96
  br i1 %not, label %b_402c99, label %b_402c98
eh.ok.402c9b:  ; 0x402c9b
  unreachable
}

define void @F_00402cb0() {  ; 0x402cb0
b_402cb0:  ; 0x402cb0
  ret void
}

define i64 @F_00402cc0() {  ; 0x402cc0
b_402cc0:  ; 0x402cc0
  %ret = call i64 @F_00402ce0()  ; 0x402cc0
  %eh.pending = call i64 @ra_eh_pending()  ; 0x402cc0
  %eh.thrown = icmp ne i64 %eh.pending, 0  ; 0x402cc0
  br i1 %eh.thrown, label %eh.landing, label %eh.ok.402cc0
eh.landing:  ; 0x402cc0
  %rax.phi = phi i64 [ %ret, %b_402cc0 ], [ %ret.1, %eh.ok.402cc0 ]  ; 0x402cc0
  ret i64 %rax.phi
eh.ok.402cc0:  ; 0x402cc0
  %eax = trunc i64 %ret to i32  ; 0x402cc5
  %addr = add i32 %eax, 4  ; 0x402cc5
  %addr.z.1 = zext i32 %addr to i64  ; 0x402cc5
  %m.p = inttoptr i64 %addr.z.1 to ptr  ; 0x402cc5
  %m = load i32, ptr %m.p, align 1  ; 0x402cc5
  %addr.z.2 = zext i32 %eax to i64  ; 0x402cc8
  %m.p.1 = inttoptr i64 %addr.z.2 to ptr  ; 0x402cc8
  %m.1 = load i32, ptr %m.p.1, align 1  ; 0x402cc8
  %logic = or i32 %m.1, 36  ; 0x402cc8
  store i32 %logic, ptr %m.p.1, align 1  ; 0x402cc8
  store i32 %m, ptr %m.p, align 1  ; 0x402ccb
  %ret.1 = call i64 @F_00402cf0()  ; 0x402cce
  %eh.pending.1 = call i64 @ra_eh_pending()  ; 0x402cce
  %eh.thrown.1 = icmp ne i64 %eh.pending.1, 0  ; 0x402cce
  br i1 %eh.thrown.1, label %eh.landing, label %eh.ok.402cce
eh.ok.402cce:  ; 0x402cce
  %eax.4 = trunc i64 %ret.1 to i32  ; 0x402cd3
  %addr.2 = add i32 %eax.4, 4  ; 0x402cd3
  %addr.z.6 = zext i32 %addr.2 to i64  ; 0x402cd3
  %m.p.2 = inttoptr i64 %addr.z.6 to ptr  ; 0x402cd3
  %m.2 = load i32, ptr %m.p.2, align 1  ; 0x402cd3
  %addr.z.7 = zext i32 %eax.4 to i64  ; 0x402cd6
  %m.p.3 = inttoptr i64 %addr.z.7 to ptr  ; 0x402cd6
  %m.3 = load i32, ptr %m.p.3, align 1  ; 0x402cd6
  %logic.1 = or i32 %m.3, 2  ; 0x402cd6
  store i32 %logic.1, ptr %m.p.3, align 1  ; 0x402cd6
  store i32 %m.2, ptr %m.p.2, align 1  ; 0x402cd9
  ret i64 %ret.1
}

define i64 @F_00402ce0() {  ; 0x402ce0
b_402ce0:  ; 0x402ce0
  %eax.z = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 896) to i32) to i64  ; 0x402ce0
  ret i64 %eax.z
}

define i64 @F_00402cf0() {  ; 0x402cf0
b_402cf0:  ; 0x402cf0
  %eax.z = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 904) to i32) to i64  ; 0x402cf0
  ret i64 %eax.z
}

define i64 @F_00402d00() {  ; 0x402d00
b_402d00:  ; 0x402d00
  %addr.z = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 268) to i32) to i64  ; 0x402d02
  %m.p = inttoptr i64 %addr.z to ptr  ; 0x402d02
  %m = load i32, ptr %m.p, align 1  ; 0x402d02
  %zf.1 = icmp eq i32 %m, 0  ; 0x402d02
  %al.z = zext i1 %zf.1 to i64  ; 0x402d08
  ret i64 %al.z
}

define i32 @F_00402d10() {  ; 0x402d10
b_402d10:  ; 0x402d10
  ret i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 912) to i32)
}

define i32 @F_00402d20() {  ; 0x402d20
b_402d20:  ; 0x402d20
  ret i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 916) to i32)
}

define i64 @F_00402d30() {  ; 0x402d30
b_402d30:  ; 0x402d30
  %local_m4 = alloca i32, align 4
  store i32 0, ptr %local_m4, align 1  ; 0x402d30
  %ret = call i64 @ra_x86_get_module_handle_w(i64 0)  ; 0x402d32
  %eax = trunc i64 %ret to i32  ; 0x402d38
  %zf = icmp eq i32 %eax, 0  ; 0x402d38
  br i1 %zf, label %b_402d6f, label %b_402d3c
b_402d3c:  ; 0x402d3c
  %addr.z.1 = zext i32 %eax to i64  ; 0x402d41
  %m.p = inttoptr i64 %addr.z.1 to ptr  ; 0x402d41
  %m = load i16, ptr %m.p, align 1  ; 0x402d41
  %not = icmp ne i16 %m, 23117  ; 0x402d44
  br i1 %not, label %b_402d6f, label %b_402d46
b_402d46:  ; 0x402d46
  %addr = add i32 %eax, 60  ; 0x402d46
  %addr.z.2 = zext i32 %addr to i64  ; 0x402d46
  %m.p.1 = inttoptr i64 %addr.z.2 to ptr  ; 0x402d46
  %m.1 = load i32, ptr %m.p.1, align 1  ; 0x402d46
  %sum = add i32 %m.1, %eax  ; 0x402d49
  %sum.s.w = zext i32 %sum to i64  ; 0x402d49
  %sum.s = inttoptr i64 %sum.s.w to ptr  ; 0x402d49
  %sum.f0 = getelementptr inbounds %S6, ptr %sum.s, i64 0, i32 0  ; 0x402d4b
  %m.2 = load i32, ptr %sum.f0, align 1  ; 0x402d4b
  %not.1 = icmp ne i32 %m.2, 17744  ; 0x402d51
  br i1 %not.1, label %b_402d6f, label %b_402d53
b_402d53:  ; 0x402d53
  %sum.f24 = getelementptr inbounds %S6, ptr %sum.s, i64 0, i32 2  ; 0x402d58
  %m.3 = load i16, ptr %sum.f24, align 1  ; 0x402d58
  %not.2 = icmp ne i16 %m.3, 267  ; 0x402d5c
  br i1 %not.2, label %b_402d6f, label %b_402d5e
b_402d5e:  ; 0x402d5e
  %sum.f116 = getelementptr inbounds %S6, ptr %sum.s, i64 0, i32 4  ; 0x402d5e
  %m.4 = load i32, ptr %sum.f116, align 1  ; 0x402d5e
  %be = icmp ule i32 %m.4, 14  ; 0x402d62
  br i1 %be, label %b_402d6f, label %b_402d64
b_402d64:  ; 0x402d64
  %sum.f232 = getelementptr inbounds %S6, ptr %sum.s, i64 0, i32 6  ; 0x402d64
  %m.5 = load i32, ptr %sum.f232, align 1  ; 0x402d64
  %not.3 = icmp ne i32 %m.5, 0  ; 0x402d6b
  %al.z = zext i1 %not.3 to i64  ; 0x402d6b
  %rax.7 = or i64 256, %al.z  ; 0x402d6b
  ret i64 %rax.7
b_402d6f:  ; 0x402d6f
  %rax.phi = phi i64 [ %ret, %b_402d30 ], [ %ret, %b_402d3c ], [ %ret, %b_402d46 ], [ 267, %b_402d53 ], [ 267, %b_402d5e ]  ; 0x402d6f
  %kept.1 = and i64 %rax.phi, -256  ; 0x402d6f
  ret i64 %kept.1
}

define i64 @F_00402d80() {  ; 0x402d80
b_402d80:  ; 0x402d80
  %ret = call i64 @F_00402c30()  ; 0x402d80
  ret i64 %ret
}

define void @F_00402d90() {  ; 0x402d90
b_402d90:  ; 0x402d90
  %local_m4 = alloca i32, align 4
  store i32 ptrtoint (ptr @U_00402db0 to i32), ptr %local_m4, align 1  ; 0x402d90
  %sarg0 = zext i32 ptrtoint (ptr @U_00402db0 to i32) to i64
  %ret = call i64 @SetUnhandledExceptionFilter(i64 %sarg0)  ; 0x402d95
  ret void
}

define void @F_00402da0(i32 %arg0) {  ; 0x402da0
b_402da0:  ; 0x402da0
  %local_m4 = alloca i32, align 4
  %local_p4 = alloca i32, align 4
  store i32 %arg0, ptr %local_p4, align 4
  %ebp = trunc i64 undef to i32  ; 0x402da0
  store i32 %ebp, ptr %local_m4, align 1  ; 0x402da0
  call void @ra_trap(i64 4205990, i64 6)  ; 0x402da6
  call void @ra_trap(i64 4205990, i64 1)  ; 0x402da6
  unreachable
}

define i64 @F_00402db0(i64 %arg0) {  ; 0x402db0
b_402db0:  ; 0x402db0
  %local_mc = alloca i32, align 4
  %local_m8 = alloca i32, align 4
  %local_m4 = alloca i32, align 4
  %local_p4 = alloca i32, align 4
  %stack.arg0.lo = trunc i64 %arg0 to i32
  store i32 %stack.arg0.lo, ptr %local_p4, align 4
  %ebp = trunc i64 undef to i32  ; 0x402db0
  store i32 %ebp, ptr %local_m4, align 1  ; 0x402db0
  store i32 %ebp, ptr %local_m8, align 1  ; 0x402db3
  store i32 %ebp, ptr %local_mc, align 1  ; 0x402db4
  %edi.z = zext i32 %stack.arg0.lo to i64  ; 0x402db5
  %m.p.1 = inttoptr i64 %edi.z to ptr  ; 0x402db8
  %m.1 = load i32, ptr %m.p.1, align 1  ; 0x402db8
  %m.1.s.w = zext i32 %m.1 to i64  ; 0x402db8
  %m.1.s = inttoptr i64 %m.1.s.w to ptr  ; 0x402db8
  %m.1.f0 = getelementptr inbounds %S7, ptr %m.1.s, i64 0, i32 0  ; 0x402dba
  %m.2 = load i32, ptr %m.1.f0, align 1  ; 0x402dba
  %not = icmp ne i32 %m.2, -529697949  ; 0x402dc0
  br i1 %not, label %b_402de7, label %b_402dc2
b_402dc2:  ; 0x402dc2
  %m.1.f16 = getelementptr inbounds %S7, ptr %m.1.s, i64 0, i32 2  ; 0x402dc2
  %m.3 = load i32, ptr %m.1.f16, align 1  ; 0x402dc2
  %not.1 = icmp ne i32 %m.3, 3  ; 0x402dc6
  br i1 %not.1, label %b_402de7, label %b_402dc8
b_402dc8:  ; 0x402dc8
  %m.1.f20 = getelementptr inbounds %S7, ptr %m.1.s, i64 0, i32 3  ; 0x402dc8
  %m.4 = load i32, ptr %m.1.f20, align 1  ; 0x402dc8
  %zf.4 = icmp eq i32 %m.4, 429065504  ; 0x402dcb
  br i1 %zf.4, label %b_402def, label %b_402dd2
b_402dd2:  ; 0x402dd2
  %zf.6 = icmp eq i32 %m.4, 429065505  ; 0x402dd2
  br i1 %zf.6, label %b_402def, label %b_402dd9
b_402dd9:  ; 0x402dd9
  %zf.8 = icmp eq i32 %m.4, 429065506  ; 0x402dd9
  br i1 %zf.8, label %b_402def, label %b_402de0
b_402de0:  ; 0x402de0
  %zf.10 = icmp eq i32 %m.4, 26820608  ; 0x402de0
  br i1 %zf.10, label %b_402def, label %b_402de7
b_402de7:  ; 0x402de7
  ret i64 0
b_402def:  ; 0x402def
  %ret = call i64 @thunk___current_exception()  ; 0x402def
  %eh.pending = call i64 @ra_eh_pending()  ; 0x402def
  %eh.thrown = icmp ne i64 %eh.pending, 0  ; 0x402def
  br i1 %eh.thrown, label %eh.landing, label %eh.ok.402def
eh.landing:  ; 0x402def
  %rax.phi = phi i64 [ %ret, %b_402def ], [ %ret.1, %eh.ok.402def ], [ %ret.2, %eh.ok.402df9 ]  ; 0x402def
  ret i64 %rax.phi
eh.ok.402def:  ; 0x402def
  %eax.4 = trunc i64 %ret to i32  ; 0x402df4
  %addr.z.13 = zext i32 %eax.4 to i64  ; 0x402df4
  %st.p.4 = inttoptr i64 %addr.z.13 to ptr  ; 0x402df4
  store i32 %m.1, ptr %st.p.4, align 1  ; 0x402df4
  %addr.3 = add i32 %stack.arg0.lo, 4  ; 0x402df6
  %addr.z.14 = zext i32 %addr.3 to i64  ; 0x402df6
  %m.p.5 = inttoptr i64 %addr.z.14 to ptr  ; 0x402df6
  %m.5 = load i32, ptr %m.p.5, align 1  ; 0x402df6
  %ret.1 = call i64 @thunk___current_exception_context()  ; 0x402df9
  %eh.pending.1 = call i64 @ra_eh_pending()  ; 0x402df9
  %eh.thrown.1 = icmp ne i64 %eh.pending.1, 0  ; 0x402df9
  br i1 %eh.thrown.1, label %eh.landing, label %eh.ok.402df9
eh.ok.402df9:  ; 0x402df9
  %eax.5 = trunc i64 %ret.1 to i32  ; 0x402dfe
  %addr.z.16 = zext i32 %eax.5 to i64  ; 0x402dfe
  %st.p.6 = inttoptr i64 %addr.z.16 to ptr  ; 0x402dfe
  store i32 %m.5, ptr %st.p.6, align 1  ; 0x402dfe
  %ret.2 = call i64 @thunk_terminate()  ; 0x402e00
  %eh.pending.2 = call i64 @ra_eh_pending()  ; 0x402e00
  %eh.thrown.2 = icmp ne i64 %eh.pending.2, 0  ; 0x402e00
  br i1 %eh.thrown.2, label %eh.landing, label %eh.ok.402e00
eh.ok.402e00:  ; 0x402e00
  unreachable
}

define void @F_00402e10() {  ; 0x402e10
b_402e10:  ; 0x402e10
  %frame = alloca [120 x i8], align 16
  %frame.top = getelementptr inbounds i8, ptr %frame, i64 44
  %esi = trunc i64 undef to i32  ; 0x402e10
  %st.p = getelementptr inbounds i8, ptr %frame.top, i64 -4  ; 0x402e10
  store i32 %esi, ptr %st.p, align 1  ; 0x402e10
  br label %b_402e3c
b_402e3c:  ; 0x402e3c
  ret void
}

define i64 @F_00402e40() {  ; 0x402e40
b_402e40:  ; 0x402e40
  %frame = alloca [120 x i8], align 16
  %frame.top = getelementptr inbounds i8, ptr %frame, i64 44
  %esi = trunc i64 undef to i32  ; 0x402e40
  %st.p = getelementptr inbounds i8, ptr %frame.top, i64 -4  ; 0x402e40
  store i32 %esi, ptr %st.p, align 1  ; 0x402e40
  %esi.z = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_rdata, i64 2552) to i32) to i64  ; 0x402e41
  br label %b_402e6c
b_402e6c:  ; 0x402e6c
  ret i64 %esi.z
}

define void @F_00402e70(i64 %arg0, i64 %arg1, i64 %arg2, i64 %arg3, i64 %arg4) {  ; 0x402e70
b_402e70:  ; 0x402e70
  %local_m24 = alloca i32, align 4
  %local_m20 = alloca i32, align 4
  %local_m1c = alloca i32, align 4
  %local_m18 = alloca i32, align 4
  %local_m14 = alloca i32, align 4
  %local_m10 = alloca i32, align 4
  %local_mc = alloca i32, align 4
  %local_m8 = alloca i32, align 4
  %local_m4 = alloca i32, align 4
  %local_p4 = alloca i32, align 4
  %local_p8 = alloca i32, align 4
  %local_pc = alloca i32, align 4
  %local_p10 = alloca i32, align 4
  %stack.arg0.lo = trunc i64 %arg1 to i32
  store i32 %stack.arg0.lo, ptr %local_p4, align 4
  %stack.arg1.lo = trunc i64 %arg2 to i32
  store i32 %stack.arg1.lo, ptr %local_p8, align 4
  %stack.arg2.lo = trunc i64 %arg3 to i32
  store i32 %stack.arg2.lo, ptr %local_pc, align 4
  %stack.arg3.lo = trunc i64 %arg4 to i32
  store i32 %stack.arg3.lo, ptr %local_p10, align 4
  %ebp = trunc i64 undef to i32  ; 0x402e70
  store i32 %ebp, ptr %local_m4, align 1  ; 0x402e70
  store i32 %ebp, ptr %local_m8, align 1  ; 0x402e73
  %esi.z = zext i32 %stack.arg0.lo to i64  ; 0x402e74
  %m.p.1 = inttoptr i64 %esi.z to ptr  ; 0x402e77
  %m.1 = load i32, ptr %m.p.1, align 1  ; 0x402e77
  store i32 %m.1, ptr %local_mc, align 1  ; 0x402e77
  %a = trunc i64 %arg0 to i32
  %ret = call i32 @F_004031e0(i32 %a, i32 %m.1)  ; 0x402e79
  %eh.pending = call i64 @ra_eh_pending()  ; 0x402e79
  %eh.thrown = icmp ne i64 %eh.pending, 0  ; 0x402e79
  br i1 %eh.thrown, label %eh.landing, label %eh.ok.402e79
eh.landing:  ; 0x402e79
  ret void
eh.ok.402e79:  ; 0x402e79
  %m.2 = load i32, ptr %local_p10, align 1  ; 0x402e7e
  store i32 %m.2, ptr %local_m10, align 1  ; 0x402e7e
  store i32 %ret, ptr %m.p.1, align 1  ; 0x402e81
  %m.3 = load i32, ptr %local_pc, align 1  ; 0x402e83
  store i32 %m.3, ptr %local_m14, align 1  ; 0x402e83
  %m.4 = load i32, ptr %local_p8, align 1  ; 0x402e86
  store i32 %m.4, ptr %local_m18, align 1  ; 0x402e86
  store i32 %stack.arg0.lo, ptr %local_m1c, align 1  ; 0x402e89
  store i32 ptrtoint (ptr @U_00402570 to i32), ptr %local_m20, align 1  ; 0x402e8a
  store i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 192) to i32), ptr %local_m24, align 1  ; 0x402e8f
  %sarg0.1 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 192) to i32) to i64
  %sarg1 = zext i32 ptrtoint (ptr @U_00402570 to i32) to i64
  %sarg3 = zext i32 %m.4 to i64
  %ret.1 = call i64 @thunk__except_handler4_common(i64 %sarg0.1, i64 %sarg1, i64 %esi.z, i64 %sarg3)  ; 0x402e94
  %eh.pending.1 = call i64 @ra_eh_pending()  ; 0x402e94
  %eh.thrown.1 = icmp ne i64 %eh.pending.1, 0  ; 0x402e94
  br i1 %eh.thrown.1, label %eh.landing, label %eh.ok.402e94
eh.ok.402e94:  ; 0x402e94
  ret void
}

define i8 @F_00402ea0() {  ; 0x402ea0
b_402ea0:  ; 0x402ea0
  %local_m40 = alloca i32, align 4
  %local_m3c = alloca i32, align 4
  %local_m38 = alloca i32, align 4
  %local_m34 = alloca i32, align 4
  %local_m30 = alloca i32, align 4
  %local_m2c = alloca i32, align 4
  %local_m28 = alloca i32, align 4
  %local_m24 = alloca i32, align 4
  %local_m20 = alloca i32, align 4
  %local_m1c = alloca i32, align 4
  %local_m18 = alloca i32, align 4
  %local_m14 = alloca i32, align 4
  %local_m10 = alloca i32, align 4
  %local_mc = alloca i32, align 4
  %local_m8 = alloca i32, align 4
  %local_m4 = alloca i32, align 4
  %ebp = trunc i64 undef to i32  ; 0x402ea0
  store i32 %ebp, ptr %local_m4, align 1  ; 0x402ea0
  %addr.z.1 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 272) to i32) to i64  ; 0x402ea3
  %m.p = inttoptr i64 %addr.z.1 to ptr  ; 0x402ea3
  %m = load i32, ptr %m.p, align 1  ; 0x402ea3
  %logic = or i32 %m, 1  ; 0x402ea3
  store i32 %logic, ptr %m.p, align 1  ; 0x402ea3
  %addr.z.3 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 924) to i32) to i64  ; 0x402ead
  %st.p.2 = inttoptr i64 %addr.z.3 to ptr  ; 0x402ead
  store i32 0, ptr %st.p.2, align 1  ; 0x402ead
  store i32 10, ptr %local_m34, align 1  ; 0x402eb7
  %ret = call i64 @IsProcessorFeaturePresent(i64 10)  ; 0x402eb9
  %eax = trunc i64 %ret to i32  ; 0x402ebf
  %zf.2 = icmp eq i32 %eax, 0  ; 0x402ebf
  br i1 %zf.2, label %b_4031c5, label %b_402ec7
b_402ec7:  ; 0x402ec7
  store i32 %ebp, ptr %local_m34, align 1  ; 0x402ec7
  store i32 %ebp, ptr %local_m38, align 1  ; 0x402ec8
  store i32 %ebp, ptr %local_m3c, align 1  ; 0x402ec9
  store i32 %ebp, ptr %local_m40, align 1  ; 0x402ed1
  %cpuid = alloca [4 x i32], align 16  ; 0x402ed2
  call void @ra_cpuid(i64 0, i64 0, ptr %cpuid)  ; 0x402ed2
  %cpuid.p = getelementptr [4 x i32], ptr %cpuid, i64 0, i64 0  ; 0x402ed2
  %cpuid.v = load i32, ptr %cpuid.p, align 4  ; 0x402ed2
  %cpuid.p.1 = getelementptr [4 x i32], ptr %cpuid, i64 0, i64 1  ; 0x402ed2
  %cpuid.v.1 = load i32, ptr %cpuid.p.1, align 4  ; 0x402ed2
  %cpuid.p.2 = getelementptr [4 x i32], ptr %cpuid, i64 0, i64 2  ; 0x402ed2
  %cpuid.v.2 = load i32, ptr %cpuid.p.2, align 4  ; 0x402ed2
  %cpuid.p.3 = getelementptr [4 x i32], ptr %cpuid, i64 0, i64 3  ; 0x402ed2
  %cpuid.v.3 = load i32, ptr %cpuid.p.3, align 4  ; 0x402ed2
  %pop = load i32, ptr %local_m40, align 1  ; 0x402ed6
  store i32 %cpuid.v, ptr %local_m30, align 1  ; 0x402ed8
  store i32 %cpuid.v.1, ptr %local_m2c, align 1  ; 0x402eda
  store i32 %cpuid.v.2, ptr %local_m28, align 1  ; 0x402edd
  store i32 %cpuid.v.3, ptr %local_m24, align 1  ; 0x402ee0
  store i32 %cpuid.v, ptr %local_m8, align 1  ; 0x402ee9
  %logic.2 = xor i32 %cpuid.v.3, 1231384169  ; 0x402eec
  %logic.3 = xor i32 %cpuid.v.2, 1818588270  ; 0x402ef5
  %logic.4 = or i32 %logic.2, %logic.3  ; 0x402efa
  %logic.5 = xor i32 %cpuid.v.1, 1970169159  ; 0x402eff
  %logic.6 = or i32 %logic.4, %logic.5  ; 0x402f04
  store i32 %pop, ptr %local_m40, align 1  ; 0x402f10
  %cpuid.1 = alloca [4 x i32], align 16  ; 0x402f11
  call void @ra_cpuid(i64 1, i64 0, ptr %cpuid.1)  ; 0x402f11
  %cpuid.p.4 = getelementptr [4 x i32], ptr %cpuid.1, i64 0, i64 0  ; 0x402f11
  %cpuid.v.4 = load i32, ptr %cpuid.p.4, align 4  ; 0x402f11
  %cpuid.p.5 = getelementptr [4 x i32], ptr %cpuid.1, i64 0, i64 1  ; 0x402f11
  %cpuid.v.5 = load i32, ptr %cpuid.p.5, align 4  ; 0x402f11
  %cpuid.p.6 = getelementptr [4 x i32], ptr %cpuid.1, i64 0, i64 2  ; 0x402f11
  %cpuid.v.6 = load i32, ptr %cpuid.p.6, align 4  ; 0x402f11
  %cpuid.p.7 = getelementptr [4 x i32], ptr %cpuid.1, i64 0, i64 3  ; 0x402f11
  %cpuid.v.7 = load i32, ptr %cpuid.p.7, align 4  ; 0x402f11
  store i32 %cpuid.v.4, ptr %local_m30, align 1  ; 0x402f17
  store i32 %cpuid.v.5, ptr %local_m2c, align 1  ; 0x402f19
  store i32 %cpuid.v.6, ptr %local_m28, align 1  ; 0x402f1c
  store i32 %cpuid.v.7, ptr %local_m24, align 1  ; 0x402f1f
  %not = icmp ne i32 %logic.6, 0  ; 0x402f22
  br i1 %not, label %b_402f67, label %b_402f24
b_402f24:  ; 0x402f24
  %m.5 = load i32, ptr %local_m30, align 1  ; 0x402f24
  %logic.7 = and i32 %m.5, 268386288  ; 0x402f27
  %zf.13 = icmp eq i32 %logic.7, 67264  ; 0x402f2c
  br i1 %zf.13, label %b_402f56, label %b_402f33
b_402f33:  ; 0x402f33
  %zf.15 = icmp eq i32 %logic.7, 132704  ; 0x402f33
  br i1 %zf.15, label %b_402f56, label %b_402f3a
b_402f3a:  ; 0x402f3a
  %zf.17 = icmp eq i32 %logic.7, 132720  ; 0x402f3a
  br i1 %zf.17, label %b_402f56, label %b_402f41
b_402f41:  ; 0x402f41
  %zf.19 = icmp eq i32 %logic.7, 198224  ; 0x402f41
  br i1 %zf.19, label %b_402f56, label %b_402f48
b_402f48:  ; 0x402f48
  %zf.21 = icmp eq i32 %logic.7, 198240  ; 0x402f48
  br i1 %zf.21, label %b_402f56, label %b_402f4f
b_402f4f:  ; 0x402f4f
  %not.1 = icmp ne i32 %logic.7, 198256  ; 0x402f54
  br i1 %not.1, label %b_402f67, label %b_402f56
b_402f56:  ; 0x402f56
  %addr.z.26 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 928) to i32) to i64  ; 0x402f56
  %m.p.6 = inttoptr i64 %addr.z.26 to ptr  ; 0x402f56
  %m.6 = load i32, ptr %m.p.6, align 1  ; 0x402f56
  %logic.8 = or i32 %m.6, 1  ; 0x402f5c
  store i32 %logic.8, ptr %m.p.6, align 1  ; 0x402f5f
  br label %b_402f6d
b_402f67:  ; 0x402f67
  %addr.z.28 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 928) to i32) to i64  ; 0x402f67
  %m.p.7 = inttoptr i64 %addr.z.28 to ptr  ; 0x402f67
  %m.7 = load i32, ptr %m.p.7, align 1  ; 0x402f67
  br label %b_402f6d
b_402f6d:  ; 0x402f6d
  %rdi.phi = phi i32 [ %logic.8, %b_402f56 ], [ %m.7, %b_402f67 ]  ; 0x402f6d
  %m.8 = load i32, ptr %local_m28, align 1  ; 0x402f6d
  store i32 %m.8, ptr %local_m14, align 1  ; 0x402f74
  %m.9 = load i32, ptr %local_m8, align 1  ; 0x402f77
  store i32 0, ptr %local_m1c, align 1  ; 0x402f7b
  store i32 0, ptr %local_m10, align 1  ; 0x402f7e
  store i32 0, ptr %local_m20, align 1  ; 0x402f81
  %l = icmp slt i32 %m.9, 7  ; 0x402f84
  br i1 %l, label %b_403026, label %b_402f8a
b_402f8a:  ; 0x402f8a
  store i32 0, ptr %local_m40, align 1  ; 0x402f91
  %cpuid.2 = alloca [4 x i32], align 16  ; 0x402f92
  call void @ra_cpuid(i64 7, i64 0, ptr %cpuid.2)  ; 0x402f92
  %cpuid.p.8 = getelementptr [4 x i32], ptr %cpuid.2, i64 0, i64 0  ; 0x402f92
  %cpuid.v.8 = load i32, ptr %cpuid.p.8, align 4  ; 0x402f92
  %cpuid.p.9 = getelementptr [4 x i32], ptr %cpuid.2, i64 0, i64 1  ; 0x402f92
  %cpuid.v.9 = load i32, ptr %cpuid.p.9, align 4  ; 0x402f92
  %cpuid.p.10 = getelementptr [4 x i32], ptr %cpuid.2, i64 0, i64 2  ; 0x402f92
  %cpuid.v.10 = load i32, ptr %cpuid.p.10, align 4  ; 0x402f92
  %cpuid.p.11 = getelementptr [4 x i32], ptr %cpuid.2, i64 0, i64 3  ; 0x402f92
  %cpuid.v.11 = load i32, ptr %cpuid.p.11, align 4  ; 0x402f92
  store i32 %cpuid.v.8, ptr %local_m30, align 1  ; 0x402f9b
  store i32 %cpuid.v.9, ptr %local_m2c, align 1  ; 0x402f9d
  store i32 %cpuid.v.10, ptr %local_m28, align 1  ; 0x402fa0
  store i32 %cpuid.v.11, ptr %local_m24, align 1  ; 0x402fa3
  store i32 %cpuid.v.9, ptr %local_mc, align 1  ; 0x402fac
  store i32 %cpuid.v.11, ptr %local_m1c, align 1  ; 0x402faf
  %logic.9 = and i32 %cpuid.v.9, 512  ; 0x402fb2
  %zf.30 = icmp eq i32 %logic.9, 0  ; 0x402fb2
  br i1 %zf.30, label %b_402fc3, label %b_402fba
b_402fba:  ; 0x402fba
  %logic.10 = or i32 %rdi.phi, 2  ; 0x402fba
  %addr.z.45 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 928) to i32) to i64  ; 0x402fbd
  %st.p.30 = inttoptr i64 %addr.z.45 to ptr  ; 0x402fbd
  store i32 %logic.10, ptr %st.p.30, align 1  ; 0x402fbd
  br label %b_402fc3
b_402fc3:  ; 0x402fc3
  %m.12 = load i32, ptr %local_m30, align 1  ; 0x402fc3
  %l.1 = icmp slt i32 %m.12, 1  ; 0x402fc7
  br i1 %l.1, label %b_402ff3, label %b_402fc9
b_402fc9:  ; 0x402fc9
  store i32 %cpuid.v.9, ptr %local_m40, align 1  ; 0x402fd6
  %cpuid.3 = alloca [4 x i32], align 16  ; 0x402fd7
  call void @ra_cpuid(i64 7, i64 1, ptr %cpuid.3)  ; 0x402fd7
  %cpuid.p.12 = getelementptr [4 x i32], ptr %cpuid.3, i64 0, i64 0  ; 0x402fd7
  %cpuid.v.12 = load i32, ptr %cpuid.p.12, align 4  ; 0x402fd7
  %cpuid.p.13 = getelementptr [4 x i32], ptr %cpuid.3, i64 0, i64 1  ; 0x402fd7
  %cpuid.v.13 = load i32, ptr %cpuid.p.13, align 4  ; 0x402fd7
  %cpuid.p.14 = getelementptr [4 x i32], ptr %cpuid.3, i64 0, i64 2  ; 0x402fd7
  %cpuid.v.14 = load i32, ptr %cpuid.p.14, align 4  ; 0x402fd7
  %cpuid.p.15 = getelementptr [4 x i32], ptr %cpuid.3, i64 0, i64 3  ; 0x402fd7
  %cpuid.v.15 = load i32, ptr %cpuid.p.15, align 4  ; 0x402fd7
  %m.13 = load i32, ptr %local_mc, align 1  ; 0x402fdd
  store i32 %cpuid.v.12, ptr %local_m30, align 1  ; 0x402fe0
  store i32 %cpuid.v.13, ptr %local_m2c, align 1  ; 0x402fe2
  store i32 %cpuid.v.14, ptr %local_m28, align 1  ; 0x402fe5
  store i32 %cpuid.v.15, ptr %local_m24, align 1  ; 0x402fe8
  store i32 %cpuid.v.15, ptr %local_m10, align 1  ; 0x402fee
  br label %b_402ff5
b_402ff3:  ; 0x402ff3
  br label %b_402ff5
b_402ff5:  ; 0x402ff5
  %rbx.phi.1 = phi i32 [ %m.13, %b_402fc9 ], [ %cpuid.v.9, %b_402ff3 ]  ; 0x402ff5
  %rsi.phi = phi i32 [ %cpuid.v.15, %b_402fc9 ], [ 0, %b_402ff3 ]  ; 0x402ff5
  %m.15 = load i32, ptr %local_m8, align 1  ; 0x402ff5
  %l.2 = icmp slt i32 %m.15, 36  ; 0x402ff9
  br i1 %l.2, label %b_403023, label %b_402ffb
b_402ffb:  ; 0x402ffb
  store i32 %rbx.phi.1, ptr %local_m40, align 1  ; 0x403005
  %cpuid.4 = alloca [4 x i32], align 16  ; 0x403006
  call void @ra_cpuid(i64 36, i64 0, ptr %cpuid.4)  ; 0x403006
  %cpuid.p.16 = getelementptr [4 x i32], ptr %cpuid.4, i64 0, i64 0  ; 0x403006
  %cpuid.v.16 = load i32, ptr %cpuid.p.16, align 4  ; 0x403006
  %cpuid.p.17 = getelementptr [4 x i32], ptr %cpuid.4, i64 0, i64 1  ; 0x403006
  %cpuid.v.17 = load i32, ptr %cpuid.p.17, align 4  ; 0x403006
  %cpuid.p.18 = getelementptr [4 x i32], ptr %cpuid.4, i64 0, i64 2  ; 0x403006
  %cpuid.v.18 = load i32, ptr %cpuid.p.18, align 4  ; 0x403006
  %cpuid.p.19 = getelementptr [4 x i32], ptr %cpuid.4, i64 0, i64 3  ; 0x403006
  %cpuid.v.19 = load i32, ptr %cpuid.p.19, align 4  ; 0x403006
  %m.16 = load i32, ptr %local_mc, align 1  ; 0x40300c
  store i32 %cpuid.v.16, ptr %local_m30, align 1  ; 0x40300f
  store i32 %cpuid.v.17, ptr %local_m2c, align 1  ; 0x403011
  %m.17 = load i32, ptr %local_m10, align 1  ; 0x403014
  store i32 %cpuid.v.18, ptr %local_m28, align 1  ; 0x403017
  store i32 %cpuid.v.19, ptr %local_m24, align 1  ; 0x40301a
  store i32 %cpuid.v.17, ptr %local_m20, align 1  ; 0x403020
  br label %b_403023
b_403023:  ; 0x403023
  %rbx.phi.2 = phi i32 [ %rbx.phi.1, %b_402ff5 ], [ %m.16, %b_402ffb ]  ; 0x403023
  %rsi.phi.1 = phi i32 [ %rsi.phi, %b_402ff5 ], [ %m.17, %b_402ffb ]  ; 0x403023
  %m.19 = load i32, ptr %local_m14, align 1  ; 0x403023
  br label %b_403026
b_403026:  ; 0x403026
  %rdx.phi = phi i32 [ %m.8, %b_402f6d ], [ %m.19, %b_403023 ]  ; 0x403026
  %rbx.phi.3 = phi i32 [ 0, %b_402f6d ], [ %rbx.phi.2, %b_403023 ]  ; 0x403026
  %rsi.phi.2 = phi i32 [ 0, %b_402f6d ], [ %rsi.phi.1, %b_403023 ]  ; 0x403026
  %m.20 = load i32, ptr %m.p, align 1  ; 0x403026
  %addr.z.69 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 280) to i32) to i64  ; 0x40302b
  %m.p.21 = inttoptr i64 %addr.z.69 to ptr  ; 0x40302b
  %m.21 = load i32, ptr %m.p.21, align 1  ; 0x40302b
  %logic.11 = or i32 %m.20, 2  ; 0x403031
  %addr.z.70 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 284) to i32) to i64  ; 0x403034
  %m.p.22 = inttoptr i64 %addr.z.70 to ptr  ; 0x403034
  %m.22 = load i32, ptr %m.p.22, align 1  ; 0x403034
  %logic.12 = and i32 %m.21, -2  ; 0x40303a
  store i32 1, ptr %st.p.2, align 1  ; 0x40303d
  store i32 %logic.11, ptr %m.p, align 1  ; 0x403047
  store i32 %logic.12, ptr %m.p.21, align 1  ; 0x40304c
  store i32 %m.22, ptr %m.p.22, align 1  ; 0x403052
  %logic.13 = and i32 %rdx.phi, 1048576  ; 0x403058
  %zf.39 = icmp eq i32 %logic.13, 0  ; 0x403058
  br i1 %zf.39, label %b_4031c2, label %b_403064
b_403064:  ; 0x403064
  %logic.14 = or i32 %logic.11, 4  ; 0x403064
  store i32 2, ptr %st.p.2, align 1  ; 0x403067
  %logic.15 = and i32 %logic.12, -17  ; 0x403071
  store i32 %logic.14, ptr %m.p, align 1  ; 0x403074
  %logic.16 = and i32 %rdx.phi, 402653184  ; 0x403079
  store i32 %logic.15, ptr %m.p.21, align 1  ; 0x40307f
  store i32 %m.22, ptr %m.p.22, align 1  ; 0x403085
  %not.2 = icmp ne i32 %logic.16, 402653184  ; 0x403091
  br i1 %not.2, label %b_4031c2, label %b_403097
b_403097:  ; 0x403097
  %xgetbv = call i64 @ra_xgetbv(i64 0)  ; 0x403099
  %xcr.lo = trunc i64 %xgetbv to i32  ; 0x403099
  %xcr.hs = lshr i64 %xgetbv, 32  ; 0x403099
  %xcr.hi = trunc i64 %xcr.hs to i32  ; 0x403099
  store i32 %xcr.lo, ptr %local_m18, align 1  ; 0x40309c
  store i32 %xcr.hi, ptr %local_m14, align 1  ; 0x40309f
  %logic.17 = and i32 %xcr.lo, 6  ; 0x4030a8
  %not.3 = icmp ne i32 %logic.17, 6  ; 0x4030ae
  br i1 %not.3, label %b_4031c2, label %b_4030b4
b_4030b4:  ; 0x4030b4
  %m.25 = load i32, ptr %m.p, align 1  ; 0x4030b4
  %logic.18 = or i32 %m.25, 8  ; 0x4030b9
  store i32 3, ptr %st.p.2, align 1  ; 0x4030bc
  store i32 %logic.18, ptr %m.p, align 1  ; 0x4030c6
  %bl = trunc i32 %rbx.phi.3 to i8  ; 0x4030cb
  %logic.19 = and i8 %bl, 32  ; 0x4030cb
  %zf.51 = icmp eq i8 %logic.19, 0  ; 0x4030cb
  br i1 %zf.51, label %b_403147, label %b_4030d0
b_4030d0:  ; 0x4030d0
  %m.26 = load i32, ptr %m.p.22, align 1  ; 0x4030d0
  %logic.20 = or i32 %logic.18, 32  ; 0x4030d6
  store i32 %logic.20, ptr %m.p, align 1  ; 0x4030d9
  %logic.21 = and i32 %rbx.phi.3, -805109760  ; 0x4030de
  %m.27 = load i32, ptr %m.p.21, align 1  ; 0x4030e4
  %logic.22 = and i32 %m.27, -3  ; 0x4030e9
  store i32 5, ptr %st.p.2, align 1  ; 0x4030ec
  store i32 %logic.22, ptr %m.p.21, align 1  ; 0x4030f6
  store i32 %m.26, ptr %m.p.22, align 1  ; 0x4030fb
  %not.4 = icmp ne i32 %logic.21, -805109760  ; 0x403107
  br i1 %not.4, label %b_403152, label %b_403109
b_403109:  ; 0x403109
  %m.28 = load i32, ptr %local_m18, align 1  ; 0x403109
  %logic.23 = and i32 %m.28, 224  ; 0x40310f
  %not.5 = icmp ne i32 %logic.23, 224  ; 0x403119
  br i1 %not.5, label %b_403147, label %b_40311b
b_40311b:  ; 0x40311b
  %m.30 = load i32, ptr %m.p.21, align 1  ; 0x40311b
  %m.31 = load i32, ptr %m.p, align 1  ; 0x403120
  %logic.24 = or i32 %m.31, 64  ; 0x403120
  store i32 %logic.24, ptr %m.p, align 1  ; 0x403120
  %logic.25 = and i32 %m.30, -37  ; 0x403127
  %m.32 = load i32, ptr %m.p.22, align 1  ; 0x40312a
  store i32 6, ptr %st.p.2, align 1  ; 0x403130
  store i32 %logic.25, ptr %m.p.21, align 1  ; 0x40313a
  store i32 %m.32, ptr %m.p.22, align 1  ; 0x40313f
  br label %b_403152
b_403147:  ; 0x403147
  %m.33 = load i32, ptr %m.p.22, align 1  ; 0x403147
  %m.34 = load i32, ptr %m.p.21, align 1  ; 0x40314d
  br label %b_403152
b_403152:  ; 0x403152
  %rax.phi = phi i32 [ %logic.22, %b_4030d0 ], [ %logic.25, %b_40311b ], [ %m.34, %b_403147 ]  ; 0x403152
  %rcx.phi = phi i32 [ %m.26, %b_4030d0 ], [ %m.32, %b_40311b ], [ %m.33, %b_403147 ]  ; 0x403152
  %m.35 = load i32, ptr %local_m1c, align 1  ; 0x403152
  %logic.26 = and i32 %m.35, 8388608  ; 0x403152
  %zf.63 = icmp eq i32 %logic.26, 0  ; 0x403152
  br i1 %zf.63, label %b_40316b, label %b_40315b
b_40315b:  ; 0x40315b
  %logic.27 = and i32 %rax.phi, -16777217  ; 0x40315b
  store i32 %rcx.phi, ptr %m.p.22, align 1  ; 0x403160
  store i32 %logic.27, ptr %m.p.21, align 1  ; 0x403166
  br label %b_40316b
b_40316b:  ; 0x40316b
  %logic.28 = and i32 %rsi.phi.2, 524288  ; 0x40316b
  %zf.66 = icmp eq i32 %logic.28, 0  ; 0x40316b
  br i1 %zf.66, label %b_4031c2, label %b_403173
b_403173:  ; 0x403173
  %m.36 = load i32, ptr %local_m18, align 1  ; 0x403173
  %logic.29 = and i32 %m.36, 224  ; 0x403179
  %not.6 = icmp ne i32 %logic.29, 224  ; 0x403183
  br i1 %not.6, label %b_4031c2, label %b_403185
b_403185:  ; 0x403185
  %m.38 = load i32, ptr %local_m20, align 1  ; 0x403185
  %eax.z.38 = trunc i32 %m.38 to i8  ; 0x403185
  %m.39 = load i32, ptr %m.p.21, align 1  ; 0x403188
  %m.40 = load i32, ptr %m.p.22, align 1  ; 0x40318e
  %logic.30 = and i32 %m.39, -16777264  ; 0x403194
  %eax.z.39 = zext i8 %eax.z.38 to i32  ; 0x40319a
  %addr.z.111 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 932) to i32) to i64  ; 0x40319d
  %st.p.65 = inttoptr i64 %addr.z.111 to ptr  ; 0x40319d
  store i32 %eax.z.39, ptr %st.p.65, align 1  ; 0x40319d
  store i32 %logic.30, ptr %m.p.21, align 1  ; 0x4031a2
  store i32 %m.40, ptr %m.p.22, align 1  ; 0x4031a8
  %be = icmp ule i32 %eax.z.39, 1  ; 0x4031b1
  br i1 %be, label %b_4031c2, label %b_4031b3
b_4031b3:  ; 0x4031b3
  %logic.31 = and i32 %logic.30, -65  ; 0x4031b3
  store i32 %m.40, ptr %m.p.22, align 1  ; 0x4031b6
  store i32 %logic.31, ptr %m.p.21, align 1  ; 0x4031bc
  br label %b_4031c2
b_4031c2:  ; 0x4031c2
  br label %b_4031c5
b_4031c5:  ; 0x4031c5
  ret i8 0
}

define i64 @F_004031d0() {  ; 0x4031d0
b_4031d0:  ; 0x4031d0
  %addr.z = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 288) to i32) to i64  ; 0x4031d2
  %m.p = inttoptr i64 %addr.z to ptr  ; 0x4031d2
  %m = load i32, ptr %m.p, align 1  ; 0x4031d2
  %not = icmp ne i32 %m, 0  ; 0x4031d8
  %al.z = zext i1 %not to i64  ; 0x4031d8
  ret i64 %al.z
}

define i32 @F_004031e0(i32 %arg0, i32 %arg1) {  ; 0x4031e0
b_4031e0:  ; 0x4031e0
  %local_m8 = alloca i32, align 4
  %local_m4 = alloca i32, align 4
  %local_p4 = alloca i32, align 4
  store i32 %arg1, ptr %local_p4, align 4
  %ebp = trunc i64 undef to i32  ; 0x4031e0
  store i32 %ebp, ptr %local_m4, align 1  ; 0x4031e0
  store i32 %arg0, ptr %local_m8, align 1  ; 0x4031e3
  %addr.z.2 = zext i32 ptrtoint (ptr getelementptr (i8, ptr @sec_data, i64 924) to i32) to i64  ; 0x4031e4
  %m.p = inttoptr i64 %addr.z.2 to ptr  ; 0x4031e4
  %m = load i32, ptr %m.p, align 1  ; 0x4031e4
  %l = icmp slt i32 %m, 1  ; 0x4031ee
  br i1 %l, label %b_403252, label %b_4031f0
b_4031f0:  ; 0x4031f0
  %zf.1 = icmp eq i32 %arg1, -1073741132  ; 0x4031f0
  br i1 %zf.1, label %b_403200, label %b_4031f8
b_4031f8:  ; 0x4031f8
  %not = icmp ne i32 %arg1, -1073741131  ; 0x4031fe
  br i1 %not, label %b_403252, label %b_403200
b_403200:  ; 0x403200
  %mxcsr.p = inttoptr i64 ptrtoint (ptr @ra_mxcsr to i64) to ptr  ; 0x403200
  %mxcsr = load i32, ptr %mxcsr.p, align 4  ; 0x403200
  store i32 %mxcsr, ptr %local_m8, align 1  ; 0x403200
  %logic = xor i32 %mxcsr, 63  ; 0x403207
  %eax.z.1 = trunc i32 %logic to i8  ; 0x403207
  %logic.1 = and i8 %eax.z.1, -127  ; 0x40320a
  %zf.6 = icmp eq i8 %logic.1, 0  ; 0x40320a
  br i1 %zf.6, label %b_403258, label %b_40320e
b_40320e:  ; 0x40320e
  %logic.2 = and i32 %logic, 516  ; 0x40320e
  %not.1 = icmp ne i32 %logic.2, 0  ; 0x403213
  br i1 %not.1, label %b_40321e, label %b_403215
b_403215:  ; 0x403215
  ret i32 -1073741682
b_40321e:  ; 0x40321e
  %logic.3 = and i32 %logic, 258  ; 0x40321e
  %zf.10 = icmp eq i32 %logic.3, 0  ; 0x40321e
  br i1 %zf.10, label %b_403258, label %b_403225
b_403225:  ; 0x403225
  %logic.4 = and i32 %logic, 1032  ; 0x403225
  %not.2 = icmp ne i32 %logic.4, 0  ; 0x40322a
  br i1 %not.2, label %b_403235, label %b_40322c
b_40322c:  ; 0x40322c
  ret i32 -1073741679
b_403235:  ; 0x403235
  %logic.5 = and i32 %logic, 2064  ; 0x403235
  %not.3 = icmp ne i32 %logic.5, 0  ; 0x40323a
  br i1 %not.3, label %b_403245, label %b_40323c
b_40323c:  ; 0x40323c
  ret i32 -1073741677
b_403245:  ; 0x403245
  %logic.6 = and i32 %logic, 4128  ; 0x403245
  %zf.16 = icmp eq i32 %logic.6, 0  ; 0x403245
  %cmov = select i1 %zf.16, i32 -1073741681, i32 %arg1  ; 0x40324f
  br label %b_403252
b_403252:  ; 0x403252
  %rcx.phi.1 = phi i32 [ %arg1, %b_4031e0 ], [ %arg1, %b_4031f8 ], [ %cmov, %b_403245 ]  ; 0x403252
  ret i32 %rcx.phi.1
b_403258:  ; 0x403258
  ret i32 -1073741680
}

define i64 @thunk__CxxThrowException(i64 %arg0, i64 %arg1) {  ; 0x403261
b_403261:  ; 0x403261
  %local_p4 = alloca i32, align 4
  %local_p8 = alloca i32, align 4
  %stack.arg0.lo = trunc i64 %arg0 to i32
  store i32 %stack.arg0.lo, ptr %local_p4, align 4
  %stack.arg1.lo = trunc i64 %arg1 to i32
  store i32 %stack.arg1.lo, ptr %local_p8, align 4
  %sarg0 = zext i32 %stack.arg0.lo to i64
  %sarg1 = zext i32 %stack.arg1.lo to i64
  %ret = call i64 @_CxxThrowException(i64 %sarg0, i64 %sarg1)  ; 0x403261
  ret i64 %ret
}

define i64 @thunk___CxxFrameHandler3(i64 %arg0, i64 %arg1, i64 %arg2, i64 %arg3) {  ; 0x403267
b_403267:  ; 0x403267
  %local_p4 = alloca i32, align 4
  %local_p8 = alloca i32, align 4
  %local_pc = alloca i32, align 4
  %local_p10 = alloca i32, align 4
  %stack.arg0.lo = trunc i64 %arg0 to i32
  store i32 %stack.arg0.lo, ptr %local_p4, align 4
  %stack.arg1.lo = trunc i64 %arg1 to i32
  store i32 %stack.arg1.lo, ptr %local_p8, align 4
  %stack.arg2.lo = trunc i64 %arg2 to i32
  store i32 %stack.arg2.lo, ptr %local_pc, align 4
  %stack.arg3.lo = trunc i64 %arg3 to i32
  store i32 %stack.arg3.lo, ptr %local_p10, align 4
  %sarg0 = zext i32 %stack.arg0.lo to i64
  %sarg1 = zext i32 %stack.arg1.lo to i64
  %sarg2 = zext i32 %stack.arg2.lo to i64
  %sarg3 = zext i32 %stack.arg3.lo to i64
  %ret = call i64 @__CxxFrameHandler3(i64 %sarg0, i64 %sarg1, i64 %sarg2, i64 %sarg3)  ; 0x403267
  ret i64 %ret
}

define i64 @thunk___current_exception() {  ; 0x40326d
b_40326d:  ; 0x40326d
  %ret = call i64 @__current_exception()  ; 0x40326d
  ret i64 %ret
}

define i64 @thunk___current_exception_context() {  ; 0x403273
b_403273:  ; 0x403273
  %ret = call i64 @__current_exception_context()  ; 0x403273
  ret i64 %ret
}

define i64 @thunk___std_exception_copy(i64 %arg0, i64 %arg1, i64 %arg2, i64 %arg3, i64 %arg4, i64 %arg5, i64 %arg6, i64 %arg7, i64 %arg8) {  ; 0x403279
b_403279:  ; 0x403279
  %local_p4 = alloca i32, align 4
  %local_p8 = alloca i32, align 4
  %local_pc = alloca i32, align 4
  %local_p10 = alloca i32, align 4
  %local_p14 = alloca i32, align 4
  %local_p18 = alloca i32, align 4
  %local_p1c = alloca i32, align 4
  %local_p20 = alloca i32, align 4
  %stack.arg0.lo = trunc i64 %arg1 to i32
  store i32 %stack.arg0.lo, ptr %local_p4, align 4
  %stack.arg1.lo = trunc i64 %arg2 to i32
  store i32 %stack.arg1.lo, ptr %local_p8, align 4
  %stack.arg2.lo = trunc i64 %arg3 to i32
  store i32 %stack.arg2.lo, ptr %local_pc, align 4
  %stack.arg3.lo = trunc i64 %arg4 to i32
  store i32 %stack.arg3.lo, ptr %local_p10, align 4
  %stack.arg4.lo = trunc i64 %arg5 to i32
  store i32 %stack.arg4.lo, ptr %local_p14, align 4
  %stack.arg5.lo = trunc i64 %arg6 to i32
  store i32 %stack.arg5.lo, ptr %local_p18, align 4
  %stack.arg6.lo = trunc i64 %arg7 to i32
  store i32 %stack.arg6.lo, ptr %local_p1c, align 4
  %stack.arg7.lo = trunc i64 %arg8 to i32
  store i32 %stack.arg7.lo, ptr %local_p20, align 4
  %sarg0 = zext i32 %stack.arg0.lo to i64
  %sarg1 = zext i32 %stack.arg1.lo to i64
  %sarg2 = zext i32 %stack.arg2.lo to i64
  %sarg3 = zext i32 %stack.arg3.lo to i64
  %sarg4 = zext i32 %stack.arg4.lo to i64
  %sarg5 = zext i32 %stack.arg5.lo to i64
  %ret = call i64 @__std_exception_copy(i64 %sarg0, i64 %sarg1, i64 %sarg2, i64 %sarg3, i64 %sarg4, i64 %sarg5, i64 %arg0, i64 undef)  ; 0x403279
  ret i64 %ret
}

define i64 @thunk___std_exception_destroy(i64 %arg0, i64 %arg1, i64 %arg2, i64 %arg3, i64 %arg4, i64 %arg5, i64 %arg6, i64 %arg7, i64 %arg8) {  ; 0x40327f
b_40327f:  ; 0x40327f
  %local_p4 = alloca i32, align 4
  %local_p8 = alloca i32, align 4
  %local_pc = alloca i32, align 4
  %local_p10 = alloca i32, align 4
  %local_p14 = alloca i32, align 4
  %local_p18 = alloca i32, align 4
  %local_p1c = alloca i32, align 4
  %local_p20 = alloca i32, align 4
  %stack.arg0.lo = trunc i64 %arg1 to i32
  store i32 %stack.arg0.lo, ptr %local_p4, align 4
  %stack.arg1.lo = trunc i64 %arg2 to i32
  store i32 %stack.arg1.lo, ptr %local_p8, align 4
  %stack.arg2.lo = trunc i64 %arg3 to i32
  store i32 %stack.arg2.lo, ptr %local_pc, align 4
  %stack.arg3.lo = trunc i64 %arg4 to i32
  store i32 %stack.arg3.lo, ptr %local_p10, align 4
  %stack.arg4.lo = trunc i64 %arg5 to i32
  store i32 %stack.arg4.lo, ptr %local_p14, align 4
  %stack.arg5.lo = trunc i64 %arg6 to i32
  store i32 %stack.arg5.lo, ptr %local_p18, align 4
  %stack.arg6.lo = trunc i64 %arg7 to i32
  store i32 %stack.arg6.lo, ptr %local_p1c, align 4
  %stack.arg7.lo = trunc i64 %arg8 to i32
  store i32 %stack.arg7.lo, ptr %local_p20, align 4
  %sarg0 = zext i32 %stack.arg0.lo to i64
  %sarg1 = zext i32 %stack.arg1.lo to i64
  %sarg2 = zext i32 %stack.arg2.lo to i64
  %sarg3 = zext i32 %stack.arg3.lo to i64
  %sarg4 = zext i32 %stack.arg4.lo to i64
  %sarg5 = zext i32 %stack.arg5.lo to i64
  %ret = call i64 @__std_exception_destroy(i64 %sarg0, i64 %sarg1, i64 %sarg2, i64 %sarg3, i64 %sarg4, i64 %sarg5, i64 %arg0, i64 undef)  ; 0x40327f
  ret i64 %ret
}

define i64 @thunk__except_handler3(i64 %arg0, i64 %arg1) {  ; 0x403285
b_403285:  ; 0x403285
  %local_p4 = alloca i32, align 4
  %local_p8 = alloca i32, align 4
  %stack.arg0.lo = trunc i64 %arg0 to i32
  store i32 %stack.arg0.lo, ptr %local_p4, align 4
  %stack.arg1.lo = trunc i64 %arg1 to i32
  store i32 %stack.arg1.lo, ptr %local_p8, align 4
  %sarg0 = zext i32 %stack.arg0.lo to i64
  %sarg1 = zext i32 %stack.arg1.lo to i64
  %ret = call i64 @_except_handler3(i64 %sarg0, i64 %sarg1)  ; 0x403285
  ret i64 %ret
}

define i64 @thunk__except_handler4_common(i64 %arg0, i64 %arg1, i64 %arg2, i64 %arg3) {  ; 0x40328b
b_40328b:  ; 0x40328b
  %local_p4 = alloca i32, align 4
  %local_p8 = alloca i32, align 4
  %local_pc = alloca i32, align 4
  %local_p10 = alloca i32, align 4
  %stack.arg0.lo = trunc i64 %arg0 to i32
  store i32 %stack.arg0.lo, ptr %local_p4, align 4
  %stack.arg1.lo = trunc i64 %arg1 to i32
  store i32 %stack.arg1.lo, ptr %local_p8, align 4
  %stack.arg2.lo = trunc i64 %arg2 to i32
  store i32 %stack.arg2.lo, ptr %local_pc, align 4
  %stack.arg3.lo = trunc i64 %arg3 to i32
  store i32 %stack.arg3.lo, ptr %local_p10, align 4
  %sarg0 = zext i32 %stack.arg0.lo to i64
  %sarg1 = zext i32 %stack.arg1.lo to i64
  %sarg2 = zext i32 %stack.arg2.lo to i64
  %sarg3 = zext i32 %stack.arg3.lo to i64
  %ret = call i64 @_except_handler4_common(i64 %sarg0, i64 %sarg1, i64 %sarg2, i64 %sarg3)  ; 0x40328b
  ret i64 %ret
}

define i64 @thunk___p___argc() {  ; 0x403291
b_403291:  ; 0x403291
  %ret = call i64 @ra_x86_p_argc()  ; 0x403291
  ret i64 %ret
}

define i64 @thunk___p___argv() {  ; 0x403297
b_403297:  ; 0x403297
  %ret = call i64 @ra_x86_p_argv()  ; 0x403297
  ret i64 %ret
}

define i64 @thunk___p__commode() {  ; 0x40329d
b_40329d:  ; 0x40329d
  %ret = call i64 @ra_x86_p_commode()  ; 0x40329d
  ret i64 %ret
}

define i64 @thunk___setusermatherr(i64 %arg0) {  ; 0x4032a3
b_4032a3:  ; 0x4032a3
  %local_p4 = alloca i32, align 4
  %stack.arg0.lo = trunc i64 %arg0 to i32
  store i32 %stack.arg0.lo, ptr %local_p4, align 4
  %sarg0 = zext i32 %stack.arg0.lo to i64
  %ret = call i64 @__setusermatherr(i64 %sarg0)  ; 0x4032a3
  ret i64 %ret
}

define i64 @thunk__c_exit() {  ; 0x4032a9
b_4032a9:  ; 0x4032a9
  %ret = call i64 @_c_exit()  ; 0x4032a9
  ret i64 %ret
}

define i64 @thunk__cexit() {  ; 0x4032af
b_4032af:  ; 0x4032af
  %ret = call i64 @_cexit()  ; 0x4032af
  ret i64 %ret
}

define i64 @thunk__configthreadlocale(i64 %arg0) {  ; 0x4032b5
b_4032b5:  ; 0x4032b5
  %local_p4 = alloca i32, align 4
  %stack.arg0.lo = trunc i64 %arg0 to i32
  store i32 %stack.arg0.lo, ptr %local_p4, align 4
  %sarg0 = zext i32 %stack.arg0.lo to i64
  %ret = call i64 @_configthreadlocale(i64 %sarg0)  ; 0x4032b5
  ret i64 %ret
}

define i64 @thunk__configure_narrow_argv(i64 %arg0) {  ; 0x4032bb
b_4032bb:  ; 0x4032bb
  %local_p4 = alloca i32, align 4
  %stack.arg0.lo = trunc i64 %arg0 to i32
  store i32 %stack.arg0.lo, ptr %local_p4, align 4
  %sarg0 = zext i32 %stack.arg0.lo to i64
  %ret = call i64 @_configure_narrow_argv(i64 %sarg0)  ; 0x4032bb
  ret i64 %ret
}

define i64 @thunk__controlfp_s(i64 %arg0, i64 %arg1, i64 %arg2) {  ; 0x4032c1
b_4032c1:  ; 0x4032c1
  %local_p4 = alloca i32, align 4
  %local_p8 = alloca i32, align 4
  %local_pc = alloca i32, align 4
  %stack.arg0.lo = trunc i64 %arg0 to i32
  store i32 %stack.arg0.lo, ptr %local_p4, align 4
  %stack.arg1.lo = trunc i64 %arg1 to i32
  store i32 %stack.arg1.lo, ptr %local_p8, align 4
  %stack.arg2.lo = trunc i64 %arg2 to i32
  store i32 %stack.arg2.lo, ptr %local_pc, align 4
  %sarg0 = zext i32 %stack.arg0.lo to i64
  %sarg1 = zext i32 %stack.arg1.lo to i64
  %sarg2 = zext i32 %stack.arg2.lo to i64
  %ret = call i64 @ra_x86_controlfp_s(i64 %sarg0, i64 %sarg1, i64 %sarg2)  ; 0x4032c1
  ret i64 %ret
}

define i64 @thunk__crt_atexit(i64 %arg0) {  ; 0x4032c7
b_4032c7:  ; 0x4032c7
  %local_p4 = alloca i32, align 4
  %stack.arg0.lo = trunc i64 %arg0 to i32
  store i32 %stack.arg0.lo, ptr %local_p4, align 4
  %sarg0 = zext i32 %stack.arg0.lo to i64
  %ret = call i64 @_crt_atexit(i64 %sarg0)  ; 0x4032c7
  ret i64 %ret
}

define i64 @thunk__exit(i64 %arg0) {  ; 0x4032cd
b_4032cd:  ; 0x4032cd
  %local_p4 = alloca i32, align 4
  %stack.arg0.lo = trunc i64 %arg0 to i32
  store i32 %stack.arg0.lo, ptr %local_p4, align 4
  %sarg0 = zext i32 %stack.arg0.lo to i64
  %ret = call i64 @_exit(i64 %sarg0)  ; 0x4032cd
  ret i64 %ret
}

define i64 @thunk__get_initial_narrow_environment() {  ; 0x4032d3
b_4032d3:  ; 0x4032d3
  %ret = call i64 @ra_x86_initial_narrow_environment()  ; 0x4032d3
  ret i64 %ret
}

define i64 @thunk__initialize_narrow_environment() {  ; 0x4032d9
b_4032d9:  ; 0x4032d9
  %ret = call i64 @_initialize_narrow_environment()  ; 0x4032d9
  ret i64 %ret
}

define i64 @thunk__initialize_onexit_table(i64 %arg0) {  ; 0x4032df
b_4032df:  ; 0x4032df
  %local_p4 = alloca i32, align 4
  %stack.arg0.lo = trunc i64 %arg0 to i32
  store i32 %stack.arg0.lo, ptr %local_p4, align 4
  %sarg0 = zext i32 %stack.arg0.lo to i64
  %ret = call i64 @ra_x86_initialize_onexit_table(i64 %sarg0)  ; 0x4032df
  ret i64 %ret
}

define i64 @thunk__initterm(i64 %arg0, i64 %arg1) {  ; 0x4032e5
b_4032e5:  ; 0x4032e5
  %local_p4 = alloca i32, align 4
  %local_p8 = alloca i32, align 4
  %stack.arg0.lo = trunc i64 %arg0 to i32
  store i32 %stack.arg0.lo, ptr %local_p4, align 4
  %stack.arg1.lo = trunc i64 %arg1 to i32
  store i32 %stack.arg1.lo, ptr %local_p8, align 4
  %sarg0 = zext i32 %stack.arg0.lo to i64
  %sarg1 = zext i32 %stack.arg1.lo to i64
  %ret = call i64 @ra_x86_initterm(i64 %sarg0, i64 %sarg1)  ; 0x4032e5
  ret i64 %ret
}

define i64 @thunk__initterm_e(i64 %arg0, i64 %arg1) {  ; 0x4032eb
b_4032eb:  ; 0x4032eb
  %local_p4 = alloca i32, align 4
  %local_p8 = alloca i32, align 4
  %stack.arg0.lo = trunc i64 %arg0 to i32
  store i32 %stack.arg0.lo, ptr %local_p4, align 4
  %stack.arg1.lo = trunc i64 %arg1 to i32
  store i32 %stack.arg1.lo, ptr %local_p8, align 4
  %sarg0 = zext i32 %stack.arg0.lo to i64
  %sarg1 = zext i32 %stack.arg1.lo to i64
  %ret = call i64 @ra_x86_initterm_e(i64 %sarg0, i64 %sarg1)  ; 0x4032eb
  ret i64 %ret
}

define i64 @thunk__register_onexit_function(i64 %arg0, i64 %arg1) {  ; 0x4032f1
b_4032f1:  ; 0x4032f1
  %local_p4 = alloca i32, align 4
  %local_p8 = alloca i32, align 4
  %stack.arg0.lo = trunc i64 %arg0 to i32
  store i32 %stack.arg0.lo, ptr %local_p4, align 4
  %stack.arg1.lo = trunc i64 %arg1 to i32
  store i32 %stack.arg1.lo, ptr %local_p8, align 4
  %sarg0 = zext i32 %stack.arg0.lo to i64
  %sarg1 = zext i32 %stack.arg1.lo to i64
  %ret = call i64 @ra_x86_register_onexit_function(i64 %sarg0, i64 %sarg1)  ; 0x4032f1
  ret i64 %ret
}

define i64 @thunk__register_thread_local_exe_atexit_callback(i64 %arg0) {  ; 0x4032f7
b_4032f7:  ; 0x4032f7
  %local_p4 = alloca i32, align 4
  %stack.arg0.lo = trunc i64 %arg0 to i32
  store i32 %stack.arg0.lo, ptr %local_p4, align 4
  %sarg0 = zext i32 %stack.arg0.lo to i64
  %ret = call i64 @_register_thread_local_exe_atexit_callback(i64 %sarg0)  ; 0x4032f7
  ret i64 %ret
}

define i64 @thunk__seh_filter_exe(i64 %arg0, i64 %arg1) {  ; 0x4032fd
b_4032fd:  ; 0x4032fd
  %local_p4 = alloca i32, align 4
  %local_p8 = alloca i32, align 4
  %stack.arg0.lo = trunc i64 %arg0 to i32
  store i32 %stack.arg0.lo, ptr %local_p4, align 4
  %stack.arg1.lo = trunc i64 %arg1 to i32
  store i32 %stack.arg1.lo, ptr %local_p8, align 4
  %sarg0 = zext i32 %stack.arg0.lo to i64
  %sarg1 = zext i32 %stack.arg1.lo to i64
  %ret = call i64 @_seh_filter_exe(i64 %sarg0, i64 %sarg1)  ; 0x4032fd
  ret i64 %ret
}

define i64 @thunk__set_app_type(i64 %arg0) {  ; 0x403303
b_403303:  ; 0x403303
  %local_p4 = alloca i32, align 4
  %stack.arg0.lo = trunc i64 %arg0 to i32
  store i32 %stack.arg0.lo, ptr %local_p4, align 4
  %sarg0 = zext i32 %stack.arg0.lo to i64
  %ret = call i64 @_set_app_type(i64 %sarg0)  ; 0x403303
  ret i64 %ret
}

define i64 @thunk__set_fmode(i64 %arg0) {  ; 0x403309
b_403309:  ; 0x403309
  %local_p4 = alloca i32, align 4
  %stack.arg0.lo = trunc i64 %arg0 to i32
  store i32 %stack.arg0.lo, ptr %local_p4, align 4
  %sarg0 = zext i32 %stack.arg0.lo to i64
  %ret = call i64 @_set_fmode(i64 %sarg0)  ; 0x403309
  ret i64 %ret
}

define i64 @thunk__set_new_mode(i64 %arg0) {  ; 0x40330f
b_40330f:  ; 0x40330f
  %local_p4 = alloca i32, align 4
  %stack.arg0.lo = trunc i64 %arg0 to i32
  store i32 %stack.arg0.lo, ptr %local_p4, align 4
  %sarg0 = zext i32 %stack.arg0.lo to i64
  %ret = call i64 @_set_new_mode(i64 %sarg0)  ; 0x40330f
  ret i64 %ret
}

define i64 @thunk_exit(i64 %arg0) {  ; 0x403315
b_403315:  ; 0x403315
  %local_p4 = alloca i32, align 4
  %stack.arg0.lo = trunc i64 %arg0 to i32
  store i32 %stack.arg0.lo, ptr %local_p4, align 4
  %sarg0 = zext i32 %stack.arg0.lo to i64
  %ret = call i64 @exit(i64 %sarg0)  ; 0x403315
  ret i64 %ret
}

define i64 @thunk_free(i64 %arg0) {  ; 0x40331b
b_40331b:  ; 0x40331b
  %local_p4 = alloca i32, align 4
  %stack.arg0.lo = trunc i64 %arg0 to i32
  store i32 %stack.arg0.lo, ptr %local_p4, align 4
  %sarg0 = zext i32 %stack.arg0.lo to i64
  %ret = call i64 @free(i64 %sarg0)  ; 0x40331b
  ret i64 %ret
}

define i64 @thunk_terminate() {  ; 0x403321
b_403321:  ; 0x403321
  %ret = call i64 @terminate()  ; 0x403321
  ret i64 %ret
}

define i64 @_except_handler3(i64 %a0, i64 %a1) {
entry:
  call void @ra_unsupported(i64 0, i64 0)
  unreachable
}

define i64 @_except_handler4_common(i64 %a0, i64 %a1, i64 %a2, i64 %a3) {
entry:
  call void @ra_unsupported(i64 0, i64 0)
  unreachable
}

define i64 @U_00401000(i32 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_00401000(i32 %u0)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00401020(i32 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_00401020(i32 %u0)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00401170(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_00401170(i64 %ecx, i64 %u0)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 4, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00401190(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_00401190(i64 %ecx, i64 %u0)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 4, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_004011b0(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  call void @F_004011b0()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 0
}

define i64 @U_004011c0(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_004011c0(i64 %ecx, i64 %u0, i64 %u1, i64 %u2)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 4, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00401200(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_00401200(i64 %ecx, i64 %u0, i64 %u1, i64 %u2, i64 %u3)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 4, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00401240(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  call void @F_00401240(i64 %ecx, i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 0
}

define i64 @U_00401260(i32 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_00401260(i32 %u0)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_004013a0(i32 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_004013a0(i32 %u0)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00401470(i32 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_00401470(i32 %u0)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00401540(i32 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_00401540(i32 %u0)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_004016b0(i32 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_004016b0(i32 %u0)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_004017c0(i32 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_004017c0(i32 %u0)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_004017e0(i32 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_004017e0(i32 %u0)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_004018c0(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_004018c0()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i8 @U_004018f0(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i32 %ecx, i64 %edx) {
entry:
  call void @F_004018f0(i32 %ecx)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i8 0
}

define i64 @U_00401900(i32 %u0, i32 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_00401900(i32 %u0, i32 %u1)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00401970(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_00401970()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00401b10(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @Derived_vf1()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00401b20(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @Base_vf1()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00401b30(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @Base_vf0(i64 %ecx, i64 %u0)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 4, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00401b50(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @Problem_vf1()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00401b60(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @std_exception_vf1(i64 %ecx)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00401b70(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @std_exception_vf0(i64 %ecx, i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 4, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00401ba0(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @Problem_vf0(i64 %ecx, i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 4, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00401bd0(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_00401bd0(i64 %u0, i64 %u1, i64 %u2, i64 %u3)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00401be0(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_00401be0(i64 %u0, i64 %u1, i64 %u2, i64 %u3)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00401bf0(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_00401bf0(i64 %u0, i64 %u1, i64 %u2, i64 %u3)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00401c00(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_00401c00(i64 %u0, i64 %u1, i64 %u2, i64 %u3)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00401c10(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_00401c10(i64 %u0, i64 %u1, i64 %u2, i64 %u3)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00401c20(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_00401c20(i64 %u0, i64 %u1, i64 %u2, i64 %u3)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00401c30(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_00401c30()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00401c40(i32 %u0, i32 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_00401c40(i32 %u0, i32 %u1)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00402070(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i32 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_00402070(i32 %ecx)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_004021a0(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_004021a0()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00402530(i32 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_00402530(i32 %u0)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00402540(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @type_info_vf0(i64 %ecx, i64 %u0)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 4, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00402570(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  call void @F_00402570(i64 %ecx)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 0
}

define i64 @U_00402580(i32 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_00402580(i32 %u0)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i8 @U_00402590(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  call void @F_00402590()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i8 0
}

define i64 @U_004025a0(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  call void @F_004025a0()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 0
}

define i64 @U_00402650(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_00402650()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00402660(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  call void @F_00402660()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 0
}

define i64 @U_00402680(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_00402680()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00402870(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @entry()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i8 @U_00402880(i32 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  call void @F_00402880(i32 %u0)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i8 0
}

define i32 @U_004028d0(i32 %u0, i32 %u1, i32 %u2, i32 %u3, i32 %u4, i32 %u5, i64 %ecx, i64 %edx) {
entry:
  %args.p = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr
  %u6 = load i64, ptr %args.p, align 8
  %args.p.1 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr
  %u7 = load i64, ptr %args.p.1, align 8
  %varargs = alloca [8 x i32], align 16
  %slot = getelementptr i32, ptr %varargs, i64 0
  store i32 %u0, ptr %slot, align 4
  %slot.1 = getelementptr i32, ptr %varargs, i64 1
  store i32 %u1, ptr %slot.1, align 4
  %slot.2 = getelementptr i32, ptr %varargs, i64 2
  store i32 %u2, ptr %slot.2, align 4
  %slot.3 = getelementptr i32, ptr %varargs, i64 3
  store i32 %u3, ptr %slot.3, align 4
  %slot.4 = getelementptr i32, ptr %varargs, i64 4
  store i32 %u4, ptr %slot.4, align 4
  %slot.5 = getelementptr i32, ptr %varargs, i64 5
  store i32 %u5, ptr %slot.5, align 4
  %slot.6 = getelementptr i32, ptr %varargs, i64 6
  %lo.6 = trunc i64 %u6 to i32
  store i32 %lo.6, ptr %slot.6, align 4
  %slot.7 = getelementptr i32, ptr %varargs, i64 7
  %lo.7 = trunc i64 %u7 to i32
  store i32 %lo.7, ptr %slot.7, align 4
  %varargs.i = ptrtoint ptr %varargs to i64
  %ret = call i32 @F_004028d0(i64 %varargs.i)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i32 %ret
}

define i64 @U_004029b0(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_004029b0()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i8 @U_004029f0(i32 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  call void @F_004029f0(i32 %u0)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i8 0
}

define i8 @U_00402a10(i32 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i8 @F_00402a10(i32 %u0)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i8 %ret
}

define i8 @U_00402a50(i32 %u0, i32 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  call void @F_00402a50(i32 %u0, i32 %u1)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i8 0
}

define i64 @U_00402a80(i32 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_00402a80(i32 %u0)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i32 @U_00402b30(i32 %u0, i32 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i32 @F_00402b30(i32 %u0, i32 %u1)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i32 %ret
}

define i64 @U_00402b80(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_00402b80()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00402c30(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_00402c30()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00402c40(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_00402c40()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00402c50(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_00402c50()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i8 @U_00402c60(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  call void @F_00402c60()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i8 0
}

define i8 @U_00402c70(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i8 @F_00402c70()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i8 %ret
}

define i8 @U_00402c80(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  call void @F_00402c80()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i8 0
}

define i64 @U_00402cb0(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  call void @F_00402cb0()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 0
}

define i64 @U_00402cc0(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_00402cc0()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00402ce0(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_00402ce0()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00402cf0(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_00402cf0()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00402d00(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_00402d00()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i32 @U_00402d10(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i32 @F_00402d10()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i32 %ret
}

define i32 @U_00402d20(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i32 @F_00402d20()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i32 %ret
}

define i64 @U_00402d30(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_00402d30()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00402d80(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_00402d80()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i8 @U_00402d90(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  call void @F_00402d90()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i8 0
}

define i8 @U_00402da0(i32 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  call void @F_00402da0(i32 %u0)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i8 0
}

define i64 @U_00402db0(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_00402db0(i64 %u0)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 4, ptr %pops.p, align 8
  ret i64 %ret
}

define i8 @U_00402e10(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  call void @F_00402e10()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i8 0
}

define i64 @U_00402e40(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_00402e40()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00402e70(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  call void @F_00402e70(i64 %ecx, i64 %u0, i64 %u1, i64 %u2, i64 %u3)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 0
}

define i8 @U_00402ea0(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i8 @F_00402ea0()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i8 %ret
}

define i64 @U_004031d0(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @F_004031d0()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i32 @U_004031e0(i32 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i32 %ecx, i64 %edx) {
entry:
  %ret = call i32 @F_004031e0(i32 %ecx, i32 %u0)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i32 %ret
}

define i64 @U_00403261(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @thunk__CxxThrowException(i64 %u0, i64 %u1)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00403267(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @thunk___CxxFrameHandler3(i64 %u0, i64 %u1, i64 %u2, i64 %u3)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_0040326d(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @thunk___current_exception()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00403273(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @thunk___current_exception_context()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00403279(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %args.p = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr
  %u6 = load i64, ptr %args.p, align 8
  %args.p.1 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr
  %u7 = load i64, ptr %args.p.1, align 8
  %ret = call i64 @thunk___std_exception_copy(i64 %ecx, i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %u6, i64 %u7)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_0040327f(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %args.p = inttoptr i64 ptrtoint (ptr @ra_args to i64) to ptr
  %u6 = load i64, ptr %args.p, align 8
  %args.p.1 = inttoptr i64 ptrtoint (ptr getelementptr (i8, ptr @ra_args, i64 8) to i64) to ptr
  %u7 = load i64, ptr %args.p.1, align 8
  %ret = call i64 @thunk___std_exception_destroy(i64 %ecx, i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %u6, i64 %u7)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00403285(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @thunk__except_handler3(i64 %u0, i64 %u1)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_0040328b(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @thunk__except_handler4_common(i64 %u0, i64 %u1, i64 %u2, i64 %u3)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00403291(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @thunk___p___argc()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00403297(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @thunk___p___argv()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_0040329d(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @thunk___p__commode()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_004032a3(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @thunk___setusermatherr(i64 %u0)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_004032a9(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @thunk__c_exit()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_004032af(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @thunk__cexit()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_004032b5(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @thunk__configthreadlocale(i64 %u0)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_004032bb(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @thunk__configure_narrow_argv(i64 %u0)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_004032c1(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @thunk__controlfp_s(i64 %u0, i64 %u1, i64 %u2)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_004032c7(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @thunk__crt_atexit(i64 %u0)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_004032cd(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @thunk__exit(i64 %u0)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_004032d3(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @thunk__get_initial_narrow_environment()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_004032d9(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @thunk__initialize_narrow_environment()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_004032df(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @thunk__initialize_onexit_table(i64 %u0)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_004032e5(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @thunk__initterm(i64 %u0, i64 %u1)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_004032eb(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @thunk__initterm_e(i64 %u0, i64 %u1)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_004032f1(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @thunk__register_onexit_function(i64 %u0, i64 %u1)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_004032f7(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @thunk__register_thread_local_exe_atexit_callback(i64 %u0)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_004032fd(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @thunk__seh_filter_exe(i64 %u0, i64 %u1)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00403303(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @thunk__set_app_type(i64 %u0)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00403309(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @thunk__set_fmode(i64 %u0)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_0040330f(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @thunk__set_new_mode(i64 %u0)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00403315(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @thunk_exit(i64 %u0)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_0040331b(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @thunk_free(i64 %u0)
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}

define i64 @U_00403321(i64 %u0, i64 %u1, i64 %u2, i64 %u3, i64 %u4, i64 %u5, i64 %ecx, i64 %edx) {
entry:
  %ret = call i64 @thunk_terminate()
  %pops.p = inttoptr i64 ptrtoint (ptr @ra_pops to i64) to ptr
  store i64 0, ptr %pops.p, align 8
  ret i64 %ret
}
