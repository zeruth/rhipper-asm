; `cmp ecx, edx; cmovle`: the low halves of two registers compared, and `l | zf` read back as
; one `sle`, which picks the register.
; pass: conditions
; call: @smaller(i64 3, i64 9)
; call: @smaller(i64 9, i64 3)
; call: @smaller(i64 7, i64 7)
; call: @smaller(i64 0x180000000, i64 0x7fffffff)
; call: @smaller(i64 -1, i64 0x100000000)

define i64 @smaller(i64 %rcx, i64 %rdx) {
entry:
  %a = trunc i64 %rcx to i32
  %b = trunc i64 %rdx to i32
  %d = sub i32 %a, %b
  %zf = icmp eq i32 %d, 0
  %sf = icmp slt i32 %d, 0
  %x1 = xor i32 %a, %b
  %x2 = xor i32 %a, %d
  %ov = and i32 %x1, %x2
  %of = icmp slt i32 %ov, 0
  %l = xor i1 %sf, %of
  %le = or i1 %l, %zf
  %r = select i1 %le, i64 %rcx, i64 %rdx
  ret i64 %r
}

; after:
define i64 @smaller(i64 %rcx, i64 %rdx) {
entry:
  %a = trunc i64 %rcx to i32
  %b = trunc i64 %rdx to i32
  %le = icmp sle i32 %a, %b
  %r = select i1 %le, i64 %rcx, i64 %rdx
  ret i64 %r
}
