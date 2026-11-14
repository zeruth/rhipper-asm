; `cmp rax, rdx; jg` on 64 bits: `not (l | zf)` is the inverse of `sle`, one `sgt`, and nothing
; of the flags is left.
; pass: conditions
; call: @larger(i64 2, i64 1)
; call: @larger(i64 1, i64 2)
; call: @larger(i64 4, i64 4)
; call: @larger(i64 -9223372036854775808, i64 1)
; call: @larger(i64 9223372036854775807, i64 -1)

define i64 @larger(i64 %a, i64 %b) {
entry:
  %d = sub i64 %a, %b
  %zf = icmp eq i64 %d, 0
  %sf = icmp slt i64 %d, 0
  %x1 = xor i64 %a, %b
  %x2 = xor i64 %a, %d
  %ov = and i64 %x1, %x2
  %of = icmp slt i64 %ov, 0
  %l = xor i1 %sf, %of
  %le = or i1 %l, %zf
  %g = xor i1 %le, true
  br i1 %g, label %first, label %second
first:
  ret i64 %a
second:
  ret i64 %b
}

; after:
define i64 @larger(i64 %a, i64 %b) {
entry:
  %g = icmp sgt i64 %a, %b
  br i1 %g, label %first, label %second
first:
  ret i64 %a
second:
  ret i64 %b
}
