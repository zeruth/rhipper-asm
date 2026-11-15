; `addsd xmm0, xmm1; ret`: two doubles in, the double left in `xmm0` out, which on Win64 is the
; result over `rax`.
; pass: abi
; call: @F_401000(xmm0 0x3ff8000000000000, xmm1 0x4002000000000000)
; call: @F_401000(xmm0 0xc01f000000000000, xmm1 0x4004000000000000, rax 9)
; signature: @F_401000 double (double, double)

%State = type { [16 x i64], i64, [8 x i8], [32 x i128] }

define void @F_401000(ptr %state) {
b_401000:
  %p.xmm0 = getelementptr %State, ptr %state, i32 0, i32 3, i32 0
  %xmm0 = load double, ptr %p.xmm0, align 8
  %p.xmm1 = getelementptr %State, ptr %state, i32 0, i32 3, i32 1
  %xmm1 = load double, ptr %p.xmm1, align 8
  %sum = fadd double %xmm0, %xmm1
  store double %sum, ptr %p.xmm0, align 8
  %p.rsp = getelementptr %State, ptr %state, i32 0, i32 0, i32 4
  %rsp = load i64, ptr %p.rsp, align 8
  %ret.addr.p = inttoptr i64 %rsp to ptr
  %ret.addr = load i64, ptr %ret.addr.p, align 1
  %sp = add i64 %rsp, 8
  store i64 %sp, ptr %p.rsp, align 8
  ret void
}
