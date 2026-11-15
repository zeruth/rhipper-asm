; `sub rsp, 56; mov [rsp+32], rcx; call F_401020; add rsp, 56; ret` beside `F_401020: mov rax,
; [rsp+40]; ret`: the outgoing stack argument the caller writes in its frame is the argument the
; raised callee takes, and the callee reads its own from its slot.
; pass: frame
; call: @F_401000(rcx 7)
; call: @F_401000(rcx -3)

%State = type { [16 x i64], i64, [8 x i8], [32 x i128] }

define void @F_401000(ptr %state) {
b_401000:
  %p.rsp = getelementptr %State, ptr %state, i32 0, i32 0, i32 4
  %rsp = load i64, ptr %p.rsp, align 8
  %rsp.1 = sub i64 %rsp, 56
  store i64 %rsp.1, ptr %p.rsp, align 8
  %p.rcx = getelementptr %State, ptr %state, i32 0, i32 0, i32 1
  %rcx = load i64, ptr %p.rcx, align 8
  %rsp.2 = load i64, ptr %p.rsp, align 8
  %addr = add i64 %rsp.2, 32
  %out.p = inttoptr i64 %addr to ptr
  store i64 %rcx, ptr %out.p, align 1
  %rsp.3 = load i64, ptr %p.rsp, align 8
  %rsp.4 = sub i64 %rsp.3, 8
  store i64 %rsp.4, ptr %p.rsp, align 8
  %push.p = inttoptr i64 %rsp.4 to ptr
  store i64 4198420, ptr %push.p, align 1
  call void @F_401020(ptr %state)
  %rsp.5 = load i64, ptr %p.rsp, align 8
  %rsp.6 = add i64 %rsp.5, 56
  store i64 %rsp.6, ptr %p.rsp, align 8
  %rsp.7 = load i64, ptr %p.rsp, align 8
  %ret.addr.p = inttoptr i64 %rsp.7 to ptr
  %ret.addr = load i64, ptr %ret.addr.p, align 1
  %sp = add i64 %rsp.7, 8
  store i64 %sp, ptr %p.rsp, align 8
  ret void
}

define void @F_401020(ptr %state) {
b_401020:
  %p.rsp = getelementptr %State, ptr %state, i32 0, i32 0, i32 4
  %rsp = load i64, ptr %p.rsp, align 8
  %addr = add i64 %rsp, 40
  %arg.p = inttoptr i64 %addr to ptr
  %arg = load i64, ptr %arg.p, align 1
  %p.rax = getelementptr %State, ptr %state, i32 0, i32 0, i32 0
  store i64 %arg, ptr %p.rax, align 8
  %ret.addr.p = inttoptr i64 %rsp to ptr
  %ret.addr = load i64, ptr %ret.addr.p, align 1
  %sp = add i64 %rsp, 8
  store i64 %sp, ptr %p.rsp, align 8
  ret void
}
