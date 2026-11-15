; `push rbx; mov rbx, rcx; mov rcx, rdx; call F_401020; add rax, rbx; pop rbx; ret`: a value
; kept across a call in a register the callee saves, which the function itself saves on the stack
; and restores, so its caller finds `rbx` as it was; the callee writes `rcx`, which the caller
; does not read after.
; pass: promote
; call: @F_401000(rcx 5, rdx 7, rbx 0x55)
; call: @F_401000(rcx -1, rdx 2, rbx -1)
; signature: @F_401000 i64 (i64, i64)
; signature: @F_401020 i64 (i64)

%State = type { [16 x i64], i64, [8 x i8], [32 x i128] }

define void @F_401000(ptr %state) {
b_401000:
  %p.rbx = getelementptr %State, ptr %state, i32 0, i32 0, i32 3
  %rbx = load i64, ptr %p.rbx, align 8
  %p.rsp = getelementptr %State, ptr %state, i32 0, i32 0, i32 4
  %rsp = load i64, ptr %p.rsp, align 8
  %sp = sub i64 %rsp, 8
  %st.p = inttoptr i64 %sp to ptr
  store i64 %rbx, ptr %st.p, align 1
  store i64 %sp, ptr %p.rsp, align 8
  %p.rcx = getelementptr %State, ptr %state, i32 0, i32 0, i32 1
  %rcx = load i64, ptr %p.rcx, align 8
  store i64 %rcx, ptr %p.rbx, align 8
  %p.rdx = getelementptr %State, ptr %state, i32 0, i32 0, i32 2
  %rdx = load i64, ptr %p.rdx, align 8
  store i64 %rdx, ptr %p.rcx, align 8
  %rsp.1 = load i64, ptr %p.rsp, align 8
  %sp.1 = sub i64 %rsp.1, 8
  %st.p.1 = inttoptr i64 %sp.1 to ptr
  store i64 4198410, ptr %st.p.1, align 1
  store i64 %sp.1, ptr %p.rsp, align 8
  call void @F_401020(ptr %state) ; 0x401005
  br label %b_40100a
b_40100a:
  %p.rax = getelementptr %State, ptr %state, i32 0, i32 0, i32 0
  %rax = load i64, ptr %p.rax, align 8
  %rbx.1 = load i64, ptr %p.rbx, align 8
  %rax.1 = add i64 %rax, %rbx.1
  store i64 %rax.1, ptr %p.rax, align 8
  %rsp.2 = load i64, ptr %p.rsp, align 8
  %rbx.p = inttoptr i64 %rsp.2 to ptr
  %rbx.2 = load i64, ptr %rbx.p, align 1
  %sp.2 = add i64 %rsp.2, 8
  store i64 %rbx.2, ptr %p.rbx, align 8
  store i64 %sp.2, ptr %p.rsp, align 8
  %ret.addr.p = inttoptr i64 %sp.2 to ptr
  %ret.addr = load i64, ptr %ret.addr.p, align 1
  %sp.3 = add i64 %sp.2, 8
  store i64 %sp.3, ptr %p.rsp, align 8
  ret void
}

define void @F_401020(ptr %state) {
b_401020:
  %p.rcx = getelementptr %State, ptr %state, i32 0, i32 0, i32 1
  %rcx = load i64, ptr %p.rcx, align 8
  %triple = mul i64 %rcx, 3
  %p.rax = getelementptr %State, ptr %state, i32 0, i32 0, i32 0
  store i64 %triple, ptr %p.rax, align 8
  store i64 0, ptr %p.rcx, align 8
  %p.rsp = getelementptr %State, ptr %state, i32 0, i32 0, i32 4
  %rsp = load i64, ptr %p.rsp, align 8
  %ret.addr.p = inttoptr i64 %rsp to ptr
  %ret.addr = load i64, ptr %ret.addr.p, align 1
  %sp = add i64 %rsp, 8
  store i64 %sp, ptr %p.rsp, align 8
  ret void
}
