; A signed comparison and a select: the conditional expression, with the signed casts.

define i32 @minimum(i32 %a, i32 %b) {
entry:
  %less = icmp slt i32 %a, %b
  %r = select i1 %less, i32 %a, i32 %b
  ret i32 %r
}
