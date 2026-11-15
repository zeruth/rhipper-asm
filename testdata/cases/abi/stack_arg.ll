; `mov rax, [rsp+40]; add rax, rcx; ret`: the qword past the return address and the four home
; slots is the first stack argument, a parameter after the register one.
; pass: abi
; call: @F_401000(rcx 2, [40] 9)
; call: @F_401000(rcx -1, [40] 0x100000000)
; signature: @F_401000 i64 (i64, i64)

%State = type { [16 x i64], i64, [8 x i8], [32 x i128] }

define void @F_401000(ptr %state) {
b_401000:
  %p.rsp = getelementptr %State, ptr %state, i32 0, i32 0, i32 4
  %rsp = load i64, ptr %p.rsp, align 8
  %addr = add i64 %rsp, 40
  %arg.p = inttoptr i64 %addr to ptr
  %arg = load i64, ptr %arg.p, align 1
  %p.rcx = getelementptr %State, ptr %state, i32 0, i32 0, i32 1
  %rcx = load i64, ptr %p.rcx, align 8
  %sum = add i64 %arg, %rcx
  %p.rax = getelementptr %State, ptr %state, i32 0, i32 0, i32 0
  store i64 %sum, ptr %p.rax, align 8
  %ret.addr.p = inttoptr i64 %rsp to ptr
  %ret.addr = load i64, ptr %ret.addr.p, align 1
  %sp = add i64 %rsp, 8
  store i64 %sp, ptr %p.rsp, align 8
  ret void
}
