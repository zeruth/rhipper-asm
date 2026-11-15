; `mov [out], rcx; ret`: a function that writes no result register returns nothing, and what it
; does is the global it leaves.
; pass: abi
; call: @F_401000(rcx 0x1122334455667788)
; call: @F_401000(rcx 0, rax 5)
; signature: @F_401000 void (i64)

%State = type { [16 x i64], i64, [8 x i8], [32 x i128] }

@out = global i64 0

define void @F_401000(ptr %state) {
b_401000:
  %p.rcx = getelementptr %State, ptr %state, i32 0, i32 0, i32 1
  %rcx = load i64, ptr %p.rcx, align 8
  store i64 %rcx, ptr @out, align 8
  %p.rsp = getelementptr %State, ptr %state, i32 0, i32 0, i32 4
  %rsp = load i64, ptr %p.rsp, align 8
  %ret.addr.p = inttoptr i64 %rsp to ptr
  %ret.addr = load i64, ptr %ret.addr.p, align 1
  %sp = add i64 %rsp, 8
  store i64 %sp, ptr %p.rsp, align 8
  ret void
}
