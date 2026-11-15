; `mov [rsp+8], rcx; mov [rsp+16], edx; mov rax, [rsp+8]; add eax, [rsp+16]; ret`: the home
; slots above the return address take the register arguments, a qword and a dword; they are the
; function's to use and no stack argument.
; pass: stack
; call: @F_401000(rcx 7, rdx 3)
; call: @F_401000(rcx 0xffffffff, rdx 0x100000001)

%State = type { [16 x i64], i64, [8 x i8], [32 x i128] }

define void @F_401000(ptr %state) {
b_401000:
  %p.rsp = getelementptr %State, ptr %state, i32 0, i32 0, i32 4
  %rsp = load i64, ptr %p.rsp, align 8
  %p.rcx = getelementptr %State, ptr %state, i32 0, i32 0, i32 1
  %rcx = load i64, ptr %p.rcx, align 8
  %addr = add i64 %rsp, 8
  %home1.p = inttoptr i64 %addr to ptr
  store i64 %rcx, ptr %home1.p, align 1
  %p.rdx = getelementptr %State, ptr %state, i32 0, i32 0, i32 2
  %rdx = load i64, ptr %p.rdx, align 8
  %edx = trunc i64 %rdx to i32
  %addr.1 = add i64 %rsp, 16
  %home2.p = inttoptr i64 %addr.1 to ptr
  store i32 %edx, ptr %home2.p, align 1
  %home1 = load i64, ptr %home1.p, align 1
  %eax = trunc i64 %home1 to i32
  %home2 = load i32, ptr %home2.p, align 1
  %sum = add i32 %eax, %home2
  %rax = zext i32 %sum to i64
  %p.rax = getelementptr %State, ptr %state, i32 0, i32 0, i32 0
  store i64 %rax, ptr %p.rax, align 8
  %ret.addr.p = inttoptr i64 %rsp to ptr
  %ret.addr = load i64, ptr %ret.addr.p, align 1
  %sp = add i64 %rsp, 8
  store i64 %sp, ptr %p.rsp, align 8
  ret void
}
