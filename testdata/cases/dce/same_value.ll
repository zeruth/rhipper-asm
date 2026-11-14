; A join whose sides bring the same value: the phi is that value, and its use reads it directly.
; pass: dce
; call: @rejoin(i32 4, i32 3)
; call: @rejoin(i32 4, i32 -3)
; call: @rejoin(i32 -1, i32 0)

define i32 @rejoin(i32 %a, i32 %b) {
entry:
  %v = add i32 %a, 1
  %c = icmp slt i32 %b, 0
  br i1 %c, label %left, label %right
left:
  br label %join
right:
  br label %join
join:
  %p = phi i32 [ %v, %left ], [ %v, %right ]
  %r = mul i32 %p, %b
  ret i32 %r
}

; after:
define i32 @rejoin(i32 %a, i32 %b) {
entry:
  %v = add i32 %a, 1
  %c = icmp slt i32 %b, 0
  br i1 %c, label %left, label %right
left:
  br label %join
right:
  br label %join
join:
  %r = mul i32 %v, %b
  ret i32 %r
}
