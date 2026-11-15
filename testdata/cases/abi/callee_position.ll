; `lea rdx, [rcx+1]; call F_401020; inc rax; ret`, the callee `lea rax, [rdx+rdx]; ret`: the
; callee reads its second position only and takes two, and the caller's read of `rax` after the
; call makes the callee return it.
; pass: abi
; call: @F_401000(rcx 4)
; call: @F_401000(rcx -1, rdx 50)
; signature: @F_401000 i64 (i64)
; signature: @F_401020 i64 (i64, i64)

%State = type { [16 x i64], i64, [8 x i8], [32 x i128] }

define void @F_401000(ptr %state) {
b_401000:
  %p.rcx = getelementptr %State, ptr %state, i32 0, i32 0, i32 1
  %rcx = load i64, ptr %p.rcx, align 8
  %next = add i64 %rcx, 1
  %p.rdx = getelementptr %State, ptr %state, i32 0, i32 0, i32 2
  store i64 %next, ptr %p.rdx, align 8
  %p.rsp = getelementptr %State, ptr %state, i32 0, i32 0, i32 4
  %rsp = load i64, ptr %p.rsp, align 8
  %sp = sub i64 %rsp, 8
  %st.p = inttoptr i64 %sp to ptr
  store i64 4198409, ptr %st.p, align 1
  store i64 %sp, ptr %p.rsp, align 8
  call void @F_401020(ptr %state) ; 0x401004
  br label %b_401009
b_401009:
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
  %p.rdx = getelementptr %State, ptr %state, i32 0, i32 0, i32 2
  %rdx = load i64, ptr %p.rdx, align 8
  %twice = add i64 %rdx, %rdx
  %p.rax = getelementptr %State, ptr %state, i32 0, i32 0, i32 0
  store i64 %twice, ptr %p.rax, align 8
  %p.rsp = getelementptr %State, ptr %state, i32 0, i32 0, i32 4
  %rsp = load i64, ptr %p.rsp, align 8
  %ret.addr.p = inttoptr i64 %rsp to ptr
  %ret.addr = load i64, ptr %ret.addr.p, align 1
  %sp = add i64 %rsp, 8
  store i64 %sp, ptr %p.rsp, align 8
  ret void
}
