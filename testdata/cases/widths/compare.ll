; An unsigned comparison of two values widened from 32 bits compares the 32-bit values, and the
; widenings go.
; pass: widths
; call: @below(i32 1, i32 2)
; call: @below(i32 2, i32 1)
; call: @below(i32 -1, i32 0)
; call: @below(i32 0, i32 -1)

define i32 @below(i32 %a, i32 %b) {
entry:
  %x = zext i32 %a to i64
  %y = zext i32 %b to i64
  %c = icmp ult i64 %x, %y
  %r = select i1 %c, i32 1, i32 0
  ret i32 %r
}

; after:
define i32 @below(i32 %a, i32 %b) {
entry:
  %c = icmp ult i32 %a, %b
  %r = select i1 %c, i32 1, i32 0
  ret i32 %r
}
