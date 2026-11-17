; `sub rsp, 40; mov [rsp+32], rcx; lea rcx, [rsp+32]; call F_401020; mov rax, [rsp+32]; add rsp,
; 40; ret` beside `F_401020: add qword [rcx], 9; ret`: a local whose address the callee writes
; through, with the return address right above it. It is a variable of its own whose address the
; call takes (`long v = x; bump(&v); return v;`), not the bottom of an array the callee may run up.
; pass: frame
; call: @F_401000(rcx 7)
; call: @F_401000(rcx -3)

%State = type { [16 x i64], i64, [8 x i8], [32 x i128] }

define void @F_401000(ptr %state) {
b_401000:
  %p.rsp = getelementptr %State, ptr %state, i32 0, i32 0, i32 4
  %rsp = load i64, ptr %p.rsp, align 8
  %rsp.1 = sub i64 %rsp, 40
  store i64 %rsp.1, ptr %p.rsp, align 8
  %p.rcx = getelementptr %State, ptr %state, i32 0, i32 0, i32 1
  %rcx = load i64, ptr %p.rcx, align 8
  %addr = add i64 %rsp.1, 32
  %local.p = inttoptr i64 %addr to ptr
  store i64 %rcx, ptr %local.p, align 1
  store i64 %addr, ptr %p.rcx, align 8
  %rsp.2 = sub i64 %rsp.1, 8
  store i64 %rsp.2, ptr %p.rsp, align 8
  %push.p = inttoptr i64 %rsp.2 to ptr
  store i64 4198420, ptr %push.p, align 1
  call void @F_401020(ptr %state)
  %rsp.3 = load i64, ptr %p.rsp, align 8
  %addr.1 = add i64 %rsp.3, 32
  %back.p = inttoptr i64 %addr.1 to ptr
  %back = load i64, ptr %back.p, align 1
  %p.rax = getelementptr %State, ptr %state, i32 0, i32 0, i32 0
  store i64 %back, ptr %p.rax, align 8
  %rsp.4 = add i64 %rsp.3, 40
  store i64 %rsp.4, ptr %p.rsp, align 8
  %ret.addr.p = inttoptr i64 %rsp.4 to ptr
  %ret.addr = load i64, ptr %ret.addr.p, align 1
  %sp = add i64 %rsp.4, 8
  store i64 %sp, ptr %p.rsp, align 8
  ret void
}

define void @F_401020(ptr %state) {
b_401020:
  %p.rcx = getelementptr %State, ptr %state, i32 0, i32 0, i32 1
  %rcx = load i64, ptr %p.rcx, align 8
  %v.p = inttoptr i64 %rcx to ptr
  %v = load i64, ptr %v.p, align 1
  %v.1 = add i64 %v, 9
  store i64 %v.1, ptr %v.p, align 1
  %p.rsp = getelementptr %State, ptr %state, i32 0, i32 0, i32 4
  %rsp = load i64, ptr %p.rsp, align 8
  %ret.addr.p = inttoptr i64 %rsp to ptr
  %ret.addr = load i64, ptr %ret.addr.p, align 1
  %sp = add i64 %rsp, 8
  store i64 %sp, ptr %p.rsp, align 8
  ret void
}
