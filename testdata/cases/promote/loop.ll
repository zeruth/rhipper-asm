; A loop over the state, `xor eax, eax; l: test rcx, rcx; jz e; add rax, rcx; dec rcx; jmp l;
; e: ret`: the registers live across the back edge and the zero flag between its store and the
; branch become values, joined at the head of the loop.
; pass: promote
; call: @F_401000(rcx 4)
; call: @F_401000(rcx 0)
; call: @F_401000(rcx 100)
; signature: @F_401000 i64 (i64)

%State = type { [16 x i64], i64, [8 x i8], [32 x i128] }

define void @F_401000(ptr %state) {
b_401000:
  %p.rax = getelementptr %State, ptr %state, i32 0, i32 0, i32 0
  store i64 0, ptr %p.rax, align 8
  br label %b_401002
b_401002:
  %p.rcx = getelementptr %State, ptr %state, i32 0, i32 0, i32 1
  %rcx = load i64, ptr %p.rcx, align 8
  %zf = icmp eq i64 %rcx, 0
  %zf.b = zext i1 %zf to i8
  %p.zf = getelementptr %State, ptr %state, i32 0, i32 2, i32 3
  store i8 %zf.b, ptr %p.zf, align 1
  %zf.b.1 = load i8, ptr %p.zf, align 1
  %zf.1 = trunc i8 %zf.b.1 to i1
  br i1 %zf.1, label %b_401010, label %b_401007
b_401007:
  %p.rax.1 = getelementptr %State, ptr %state, i32 0, i32 0, i32 0
  %rax = load i64, ptr %p.rax.1, align 8
  %p.rcx.1 = getelementptr %State, ptr %state, i32 0, i32 0, i32 1
  %rcx.1 = load i64, ptr %p.rcx.1, align 8
  %rax.1 = add i64 %rax, %rcx.1
  store i64 %rax.1, ptr %p.rax.1, align 8
  %rcx.2 = sub i64 %rcx.1, 1
  store i64 %rcx.2, ptr %p.rcx.1, align 8
  br label %b_401002
b_401010:
  %p.rsp = getelementptr %State, ptr %state, i32 0, i32 0, i32 4
  %rsp = load i64, ptr %p.rsp, align 8
  %ret.addr.p = inttoptr i64 %rsp to ptr
  %ret.addr = load i64, ptr %ret.addr.p, align 1
  %sp = add i64 %rsp, 8
  store i64 %sp, ptr %p.rsp, align 8
  ret void
}
