; A counted loop with an accumulator: two phis carried around the back edge.
; call: @sum_loop(i64 0) = 0
; call: @sum_loop(i64 1) = 0
; call: @sum_loop(i64 5) = 10

define i64 @sum_loop(i64 %n) {
entry:
  %empty = icmp eq i64 %n, 0
  br i1 %empty, label %done, label %loop
loop:
  %i = phi i64 [ 0, %entry ], [ %next, %loop ]
  %sum = phi i64 [ 0, %entry ], [ %added, %loop ]
  %added = add i64 %sum, %i
  %next = add i64 %i, 1
  %more = icmp ult i64 %next, %n
  br i1 %more, label %loop, label %done
done:
  %r = phi i64 [ 0, %entry ], [ %added, %loop ]
  ret i64 %r
}
