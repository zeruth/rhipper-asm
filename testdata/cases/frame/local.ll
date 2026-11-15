; `sub rsp, 40; mov [rsp+32], rcx; mov rax, [rsp+32]; add rsp, 40; ret`: a local 8 bytes under
; the return address, which the frame pass reads off the frame the raised function allocates.
; pass: frame
; call: @F_401000(rcx 7)
; call: @F_401000(rcx -1)

%State = type { [16 x i64], i64, [8 x i8], [32 x i128] }

define void @F_401000(ptr %state) {
b_401000:
  %p.rsp = getelementptr %State, ptr %state, i32 0, i32 0, i32 4
  %rsp = load i64, ptr %p.rsp, align 8
  %rsp.1 = sub i64 %rsp, 40
  store i64 %rsp.1, ptr %p.rsp, align 8
  %p.rcx = getelementptr %State, ptr %state, i32 0, i32 0, i32 1
  %rcx = load i64, ptr %p.rcx, align 8
  %rsp.2 = load i64, ptr %p.rsp, align 8
  %addr = add i64 %rsp.2, 32
  %st.p = inttoptr i64 %addr to ptr
  store i64 %rcx, ptr %st.p, align 1
  %rsp.3 = load i64, ptr %p.rsp, align 8
  %addr.1 = add i64 %rsp.3, 32
  %local.p = inttoptr i64 %addr.1 to ptr
  %local = load i64, ptr %local.p, align 1
  %p.rax = getelementptr %State, ptr %state, i32 0, i32 0, i32 0
  store i64 %local, ptr %p.rax, align 8
  %rsp.4 = load i64, ptr %p.rsp, align 8
  %rsp.5 = add i64 %rsp.4, 40
  store i64 %rsp.5, ptr %p.rsp, align 8
  %ret.addr.p = inttoptr i64 %rsp.5 to ptr
  %ret.addr = load i64, ptr %ret.addr.p, align 1
  %sp = add i64 %rsp.5, 8
  store i64 %sp, ptr %p.rsp, align 8
  ret void
}
