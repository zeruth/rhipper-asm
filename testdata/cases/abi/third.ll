; `lea rax, [r8+r8]; ret`: a Win64 parameter is its position, so a function that reads only the
; third register takes three; the first two are passed and unread.
; pass: abi
; call: @F_401000(r8 21)
; call: @F_401000(rcx 1, rdx 2, r8 -4)
; signature: @F_401000 i64 (i64, i64, i64)

%State = type { [16 x i64], i64, [8 x i8], [32 x i128] }

define void @F_401000(ptr %state) {
b_401000:
  %p.r8 = getelementptr %State, ptr %state, i32 0, i32 0, i32 8
  %r8 = load i64, ptr %p.r8, align 8
  %twice = add i64 %r8, %r8
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
