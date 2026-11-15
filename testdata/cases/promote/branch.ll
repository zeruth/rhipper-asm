; `cmp rcx, rdx; jl a; mov rax, rdx; jmp e; a: mov rax, rcx; e: ret`, the signed minimum: the
; sign and the overflow flags go through the state between the compare and the branch, and the
; result arrives at the `ret` from either side. The overflow is in the calls.
; pass: promote
; call: @F_401000(rcx 3, rdx 8)
; call: @F_401000(rcx 8, rdx 3)
; call: @F_401000(rcx -5, rdx 2)
; call: @F_401000(rcx 0x8000000000000000, rdx 1)
; call: @F_401000(rcx 0x7fffffffffffffff, rdx -1)
; signature: @F_401000 i64 (i64, i64)

%State = type { [16 x i64], i64, [8 x i8], [32 x i128] }

define void @F_401000(ptr %state) {
b_401000:
  %p.rcx = getelementptr %State, ptr %state, i32 0, i32 0, i32 1
  %rcx = load i64, ptr %p.rcx, align 8
  %p.rdx = getelementptr %State, ptr %state, i32 0, i32 0, i32 2
  %rdx = load i64, ptr %p.rdx, align 8
  %d = sub i64 %rcx, %rdx
  %sf = icmp slt i64 %d, 0
  %sf.b = zext i1 %sf to i8
  %p.sf = getelementptr %State, ptr %state, i32 0, i32 2, i32 4
  store i8 %sf.b, ptr %p.sf, align 1
  %x1 = xor i64 %rcx, %rdx
  %x2 = xor i64 %rcx, %d
  %ov = and i64 %x1, %x2
  %of = icmp slt i64 %ov, 0
  %of.b = zext i1 %of to i8
  %p.of = getelementptr %State, ptr %state, i32 0, i32 2, i32 6
  store i8 %of.b, ptr %p.of, align 1
  %sf.b.1 = load i8, ptr %p.sf, align 1
  %sf.1 = trunc i8 %sf.b.1 to i1
  %of.b.1 = load i8, ptr %p.of, align 1
  %of.1 = trunc i8 %of.b.1 to i1
  %l = xor i1 %sf.1, %of.1
  br i1 %l, label %b_401010, label %b_401008
b_401008:
  %p.rdx.1 = getelementptr %State, ptr %state, i32 0, i32 0, i32 2
  %rdx.1 = load i64, ptr %p.rdx.1, align 8
  %p.rax = getelementptr %State, ptr %state, i32 0, i32 0, i32 0
  store i64 %rdx.1, ptr %p.rax, align 8
  br label %b_401013
b_401010:
  %p.rcx.1 = getelementptr %State, ptr %state, i32 0, i32 0, i32 1
  %rcx.1 = load i64, ptr %p.rcx.1, align 8
  %p.rax.1 = getelementptr %State, ptr %state, i32 0, i32 0, i32 0
  store i64 %rcx.1, ptr %p.rax.1, align 8
  br label %b_401013
b_401013:
  %p.rsp = getelementptr %State, ptr %state, i32 0, i32 0, i32 4
  %rsp = load i64, ptr %p.rsp, align 8
  %ret.addr.p = inttoptr i64 %rsp to ptr
  %ret.addr = load i64, ptr %ret.addr.p, align 1
  %sp = add i64 %rsp, 8
  store i64 %sp, ptr %p.rsp, align 8
  ret void
}
