; `push rbx; mov rbx, rcx; lea rax, [rbx+1]; pop rbx; ret`: the save of a callee-saved register
; in the frame, which the raised function keeps, and the register as the caller sees it after.
; pass: frame
; call: @F_401000(rcx 7, rbx 0x1234)
; call: @F_401000(rcx -1, rbx -2)

%State = type { [16 x i64], i64, [8 x i8], [32 x i128] }

define void @F_401000(ptr %state) {
b_401000:
  %p.rbx = getelementptr %State, ptr %state, i32 0, i32 0, i32 3
  %rbx = load i64, ptr %p.rbx, align 8
  %p.rsp = getelementptr %State, ptr %state, i32 0, i32 0, i32 4
  %rsp = load i64, ptr %p.rsp, align 8
  %rsp.1 = sub i64 %rsp, 8
  store i64 %rsp.1, ptr %p.rsp, align 8
  %push.p = inttoptr i64 %rsp.1 to ptr
  store i64 %rbx, ptr %push.p, align 1
  %p.rcx = getelementptr %State, ptr %state, i32 0, i32 0, i32 1
  %rcx = load i64, ptr %p.rcx, align 8
  store i64 %rcx, ptr %p.rbx, align 8
  %rbx.1 = load i64, ptr %p.rbx, align 8
  %lea = add i64 %rbx.1, 1
  %p.rax = getelementptr %State, ptr %state, i32 0, i32 0, i32 0
  store i64 %lea, ptr %p.rax, align 8
  %rsp.2 = load i64, ptr %p.rsp, align 8
  %pop.p = inttoptr i64 %rsp.2 to ptr
  %pop = load i64, ptr %pop.p, align 1
  %rsp.3 = add i64 %rsp.2, 8
  store i64 %rsp.3, ptr %p.rsp, align 8
  store i64 %pop, ptr %p.rbx, align 8
  %rsp.4 = load i64, ptr %p.rsp, align 8
  %ret.addr.p = inttoptr i64 %rsp.4 to ptr
  %ret.addr = load i64, ptr %ret.addr.p, align 1
  %sp = add i64 %rsp.4, 8
  store i64 %sp, ptr %p.rsp, align 8
  ret void
}
