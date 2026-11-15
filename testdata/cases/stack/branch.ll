; `test rcx, rcx; jz e; push rcx; pop rax; e: ret`: one arm pushes and pops, the other leaves the
; stack alone, and the two reach the `ret` at the same depth.
; pass: stack
; call: @F_401000(rcx 0, rax 5)
; call: @F_401000(rcx 9, rax 5)

%State = type { [16 x i64], i64, [8 x i8], [32 x i128] }

define void @F_401000(ptr %state) {
b_401000:
  %p.rcx = getelementptr %State, ptr %state, i32 0, i32 0, i32 1
  %rcx = load i64, ptr %p.rcx, align 8
  %zf = icmp eq i64 %rcx, 0
  %zf.b = zext i1 %zf to i8
  %p.zf = getelementptr %State, ptr %state, i32 0, i32 2, i32 3
  store i8 %zf.b, ptr %p.zf, align 1
  %zf.b.1 = load i8, ptr %p.zf, align 1
  %zf.1 = trunc i8 %zf.b.1 to i1
  br i1 %zf.1, label %b_401007, label %b_401005
b_401005:
  %p.rsp = getelementptr %State, ptr %state, i32 0, i32 0, i32 4
  %rsp = load i64, ptr %p.rsp, align 8
  %rsp.1 = sub i64 %rsp, 8
  store i64 %rsp.1, ptr %p.rsp, align 8
  %rcx.1 = load i64, ptr %p.rcx, align 8
  %push.p = inttoptr i64 %rsp.1 to ptr
  store i64 %rcx.1, ptr %push.p, align 1
  %rsp.2 = load i64, ptr %p.rsp, align 8
  %pop.p = inttoptr i64 %rsp.2 to ptr
  %pop = load i64, ptr %pop.p, align 1
  %rsp.3 = add i64 %rsp.2, 8
  store i64 %rsp.3, ptr %p.rsp, align 8
  %p.rax = getelementptr %State, ptr %state, i32 0, i32 0, i32 0
  store i64 %pop, ptr %p.rax, align 8
  br label %b_401007
b_401007:
  %p.rsp.1 = getelementptr %State, ptr %state, i32 0, i32 0, i32 4
  %rsp.4 = load i64, ptr %p.rsp.1, align 8
  %ret.addr.p = inttoptr i64 %rsp.4 to ptr
  %ret.addr = load i64, ptr %ret.addr.p, align 1
  %sp = add i64 %rsp.4, 8
  store i64 %sp, ptr %p.rsp.1, align 8
  ret void
}
