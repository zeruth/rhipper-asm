; `mov rax, [rsp+48]; add rax, [rsp+40]; ret`: two stack arguments, which the raised function
; takes as parameters and the frame pass reads from their slots above the return address.
; pass: frame
; call: @F_401000([40] 9, [48] 5)
; call: @F_401000([40] -1, [48] 0x100000000)

%State = type { [16 x i64], i64, [8 x i8], [32 x i128] }

define void @F_401000(ptr %state) {
b_401000:
  %p.rsp = getelementptr %State, ptr %state, i32 0, i32 0, i32 4
  %rsp = load i64, ptr %p.rsp, align 8
  %addr = add i64 %rsp, 48
  %second.p = inttoptr i64 %addr to ptr
  %second = load i64, ptr %second.p, align 1
  %addr.1 = add i64 %rsp, 40
  %first.p = inttoptr i64 %addr.1 to ptr
  %first = load i64, ptr %first.p, align 1
  %sum = add i64 %second, %first
  %p.rax = getelementptr %State, ptr %state, i32 0, i32 0, i32 0
  store i64 %sum, ptr %p.rax, align 8
  %ret.addr.p = inttoptr i64 %rsp to ptr
  %ret.addr = load i64, ptr %ret.addr.p, align 1
  %sp = add i64 %rsp, 8
  store i64 %sp, ptr %p.rsp, align 8
  ret void
}
