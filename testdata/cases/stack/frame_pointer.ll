; `push rbp; mov rbp, rsp; sub rsp, 16; mov [rbp-8], rcx; mov rax, [rbp-8]; add rax, [rbp+48];
; mov rsp, rbp; pop rbp; ret`: the frame and the stack argument reached through the frame
; pointer, which holds an offset of the entry stack pointer as the stack pointer does.
; pass: stack
; call: @F_401000(rcx 7, rbp 0x99, [40] 5)
; call: @F_401000(rcx -1, rbp 0, [40] -1)

%State = type { [16 x i64], i64, [8 x i8], [32 x i128] }

define void @F_401000(ptr %state) {
b_401000:
  %p.rbp = getelementptr %State, ptr %state, i32 0, i32 0, i32 5
  %rbp = load i64, ptr %p.rbp, align 8
  %p.rsp = getelementptr %State, ptr %state, i32 0, i32 0, i32 4
  %rsp = load i64, ptr %p.rsp, align 8
  %rsp.1 = sub i64 %rsp, 8
  store i64 %rsp.1, ptr %p.rsp, align 8
  %push.p = inttoptr i64 %rsp.1 to ptr
  store i64 %rbp, ptr %push.p, align 1
  %rsp.2 = load i64, ptr %p.rsp, align 8
  store i64 %rsp.2, ptr %p.rbp, align 8
  %rsp.3 = load i64, ptr %p.rsp, align 8
  %rsp.4 = sub i64 %rsp.3, 16
  store i64 %rsp.4, ptr %p.rsp, align 8
  %p.rcx = getelementptr %State, ptr %state, i32 0, i32 0, i32 1
  %rcx = load i64, ptr %p.rcx, align 8
  %rbp.1 = load i64, ptr %p.rbp, align 8
  %addr = add i64 %rbp.1, -8
  %local.p = inttoptr i64 %addr to ptr
  store i64 %rcx, ptr %local.p, align 1
  %rbp.2 = load i64, ptr %p.rbp, align 8
  %addr.1 = add i64 %rbp.2, -8
  %local.p.1 = inttoptr i64 %addr.1 to ptr
  %local = load i64, ptr %local.p.1, align 1
  %addr.2 = add i64 %rbp.2, 48
  %arg.p = inttoptr i64 %addr.2 to ptr
  %arg = load i64, ptr %arg.p, align 1
  %sum = add i64 %local, %arg
  %p.rax = getelementptr %State, ptr %state, i32 0, i32 0, i32 0
  store i64 %sum, ptr %p.rax, align 8
  %rbp.3 = load i64, ptr %p.rbp, align 8
  store i64 %rbp.3, ptr %p.rsp, align 8
  %rsp.5 = load i64, ptr %p.rsp, align 8
  %pop.p = inttoptr i64 %rsp.5 to ptr
  %pop = load i64, ptr %pop.p, align 1
  %rsp.6 = add i64 %rsp.5, 8
  store i64 %rsp.6, ptr %p.rsp, align 8
  store i64 %pop, ptr %p.rbp, align 8
  %rsp.7 = load i64, ptr %p.rsp, align 8
  %ret.addr.p = inttoptr i64 %rsp.7 to ptr
  %ret.addr = load i64, ptr %ret.addr.p, align 1
  %sp = add i64 %rsp.7, 8
  store i64 %sp, ptr %p.rsp, align 8
  ret void
}
