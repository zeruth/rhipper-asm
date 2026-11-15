; `cvttsd2si rax, xmm1; ret`: a double read from the second vector register is the second
; parameter, at its position, and the unread first position is an integer.
; pass: abi
; call: @F_401000(xmm1 0x4004000000000000)
; call: @F_401000(rcx 3, xmm1 0xc01f000000000000)
; signature: @F_401000 i64 (i64, double)

%State = type { [16 x i64], i64, [8 x i8], [32 x i128] }

define void @F_401000(ptr %state) {
b_401000:
  %p.xmm1 = getelementptr %State, ptr %state, i32 0, i32 3, i32 1
  %xmm1 = load double, ptr %p.xmm1, align 8
  %int = fptosi double %xmm1 to i64
  %p.rax = getelementptr %State, ptr %state, i32 0, i32 0, i32 0
  store i64 %int, ptr %p.rax, align 8
  %p.rsp = getelementptr %State, ptr %state, i32 0, i32 0, i32 4
  %rsp = load i64, ptr %p.rsp, align 8
  %ret.addr.p = inttoptr i64 %rsp to ptr
  %ret.addr = load i64, ptr %ret.addr.p, align 1
  %sp = add i64 %rsp, 8
  store i64 %sp, ptr %p.rsp, align 8
  ret void
}
