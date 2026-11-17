; A signed comparison and a select: the conditional expression, with the signed casts.
; call: @minimum(i32 3, i32 9) = 3
; call: @minimum(i32 -4, i32 2) = -4
; call: @minimum(i32 6, i32 -1) = -1

define i32 @minimum(i32 %a, i32 %b) {
entry:
  %less = icmp slt i32 %a, %b
  %r = select i1 %less, i32 %a, i32 %b
  ret i32 %r
}
