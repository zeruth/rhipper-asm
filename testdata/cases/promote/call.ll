; `add rcx, rcx; call F_401020; inc rax; ret`, the callee `imul rax, rcx, 3; ret`: the call
; passes the register its callee reads and takes the result its caller reads after it, and the
; push of the return address goes with the state.
; pass: promote
; call: @F_401000(rcx 5)
; call: @F_401000(rcx -3)
; signature: @F_401000 i64 (i64)
; signature: @F_401020 i64 (i64)

%State = type { [16 x i64], i64, [8 x i8], [32 x i128] }

define void @F_401000(ptr %state) {
b_401000:
  %p.rcx = getelementptr %State, ptr %state, i32 0, i32 0, i32 1
  %rcx = load i64, ptr %p.rcx, align 8
  %rcx.1 = add i64 %rcx, %rcx
  store i64 %rcx.1, ptr %p.rcx, align 8
  %p.rsp = getelementptr %State, ptr %state, i32 0, i32 0, i32 4
  %rsp = load i64, ptr %p.rsp, align 8
  %sp = sub i64 %rsp, 8
  %st.p = inttoptr i64 %sp to ptr
  store i64 4198408, ptr %st.p, align 1
  store i64 %sp, ptr %p.rsp, align 8
  call void @F_401020(ptr %state) ; 0x401003
  br label %b_401008
b_401008:
  %p.rax = getelementptr %State, ptr %state, i32 0, i32 0, i32 0
  %rax = load i64, ptr %p.rax, align 8
  %rax.1 = add i64 %rax, 1
  store i64 %rax.1, ptr %p.rax, align 8
  %rsp.1 = load i64, ptr %p.rsp, align 8
  %ret.addr.p = inttoptr i64 %rsp.1 to ptr
  %ret.addr = load i64, ptr %ret.addr.p, align 1
  %sp.1 = add i64 %rsp.1, 8
  store i64 %sp.1, ptr %p.rsp, align 8
  ret void
}

define void @F_401020(ptr %state) {
b_401020:
  %p.rcx = getelementptr %State, ptr %state, i32 0, i32 0, i32 1
  %rcx = load i64, ptr %p.rcx, align 8
  %triple = mul i64 %rcx, 3
  %p.rax = getelementptr %State, ptr %state, i32 0, i32 0, i32 0
  store i64 %triple, ptr %p.rax, align 8
  %p.rsp = getelementptr %State, ptr %state, i32 0, i32 0, i32 4
  %rsp = load i64, ptr %p.rsp, align 8
  %ret.addr.p = inttoptr i64 %rsp to ptr
  %ret.addr = load i64, ptr %ret.addr.p, align 1
  %sp = add i64 %rsp, 8
  store i64 %sp, ptr %p.rsp, align 8
  ret void
}
