; The flags a `cmp` computes that no branch reads, once the condition is a direct comparison: the
; whole formula goes and the comparison and the difference it no longer uses with it.
; pass: dce
; call: @minimum(i32 1, i32 2)
; call: @minimum(i32 2, i32 1)
; call: @minimum(i32 -2147483648, i32 1)

define i32 @minimum(i32 %a, i32 %b) {
entry:
  %d = sub i32 %a, %b
  %zf = icmp eq i32 %d, 0
  %sf = icmp slt i32 %d, 0
  %cf = icmp ult i32 %a, %b
  %x1 = xor i32 %a, %b
  %x2 = xor i32 %a, %d
  %ov = and i32 %x1, %x2
  %of = icmp slt i32 %ov, 0
  %l = icmp slt i32 %a, %b
  %r = select i1 %l, i32 %a, i32 %b
  ret i32 %r
}

; after:
define i32 @minimum(i32 %a, i32 %b) {
entry:
  %l = icmp slt i32 %a, %b
  %r = select i1 %l, i32 %a, i32 %b
  ret i32 %r
}
