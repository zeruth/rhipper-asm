; `cmp; ja` at the bottom of a loop: `not (cf | zf)` is one `ugt` of the counter and the bound,
; and the loop's phis are no condition.
; pass: conditions
; call: @sum_to(i64 0)
; call: @sum_to(i64 1)
; call: @sum_to(i64 4)
; call: @sum_to(i64 100)

define i64 @sum_to(i64 %n) {
entry:
  br label %loop
loop:
  %i = phi i64 [ 0, %entry ], [ %next, %loop ]
  %acc = phi i64 [ 0, %entry ], [ %sum, %loop ]
  %sum = add i64 %acc, %i
  %next = add i64 %i, 1
  %d = sub i64 %next, %n
  %cf = icmp ult i64 %next, %n
  %zf = icmp eq i64 %d, 0
  %be = or i1 %cf, %zf
  %above = xor i1 %be, true
  br i1 %above, label %done, label %loop
done:
  ret i64 %sum
}

; after:
define i64 @sum_to(i64 %n) {
entry:
  br label %loop
loop:
  %i = phi i64 [ 0, %entry ], [ %next, %loop ]
  %acc = phi i64 [ 0, %entry ], [ %sum, %loop ]
  %sum = add i64 %acc, %i
  %next = add i64 %i, 1
  %above = icmp ugt i64 %next, %n
  br i1 %above, label %done, label %loop
done:
  ret i64 %sum
}
