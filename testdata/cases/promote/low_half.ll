; `mov eax, ecx; add eax, edx; ret`: the 32-bit operation as the lift writes it, the registers
; read whole and cut, the result widened with zeros, which the 64-bit result keeps: the high
; halves of the arguments do not reach it.
; pass: promote
; call: @F_401000(rcx 7, rdx 9)
; call: @F_401000(rcx 0xffffffff00000001, rdx 0xffffffff)
; call: @F_401000(rcx 0x123456789, rdx 0x987654321)
; signature: @F_401000 i64 (i64, i64)

%State = type { [16 x i64], i64, [8 x i8], [32 x i128] }

define void @F_401000(ptr %state) {
b_401000:
  %p.rcx = getelementptr %State, ptr %state, i32 0, i32 0, i32 1
  %rcx = load i64, ptr %p.rcx, align 8
  %ecx = trunc i64 %rcx to i32
  %eax.z = zext i32 %ecx to i64
  %p.rax = getelementptr %State, ptr %state, i32 0, i32 0, i32 0
  store i64 %eax.z, ptr %p.rax, align 8
  %rax = load i64, ptr %p.rax, align 8
  %eax = trunc i64 %rax to i32
  %p.rdx = getelementptr %State, ptr %state, i32 0, i32 0, i32 2
  %rdx = load i64, ptr %p.rdx, align 8
  %edx = trunc i64 %rdx to i32
  %sum = add i32 %eax, %edx
  %eax.z.1 = zext i32 %sum to i64
  store i64 %eax.z.1, ptr %p.rax, align 8
  %p.rsp = getelementptr %State, ptr %state, i32 0, i32 0, i32 4
  %rsp = load i64, ptr %p.rsp, align 8
  %ret.addr.p = inttoptr i64 %rsp to ptr
  %ret.addr = load i64, ptr %ret.addr.p, align 1
  %sp = add i64 %rsp, 8
  store i64 %sp, ptr %p.rsp, align 8
  ret void
}
