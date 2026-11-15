; `mov rax, [rsp+48]; ret`: a read of the second stack argument makes the stack parameters run
; from the first to it, and no register is read: two parameters, both on the stack.
; pass: abi
; call: @F_401000([48] 5)
; call: @F_401000([40] 1, [48] -7)
; signature: @F_401000 i64 (i64, i64)

%State = type { [16 x i64], i64, [8 x i8], [32 x i128] }

define void @F_401000(ptr %state) {
b_401000:
  %p.rsp = getelementptr %State, ptr %state, i32 0, i32 0, i32 4
  %rsp = load i64, ptr %p.rsp, align 8
  %addr = add i64 %rsp, 48
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
