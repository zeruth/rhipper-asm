; A register carried around a loop and never read after it: the phi and the add feed each other
; and nothing else, so the cycle goes whole while the counter stays.
; pass: dce
; call: @count(i64 0)
; call: @count(i64 1)
; call: @count(i64 10)

define i64 @count(i64 %n) {
entry:
  br label %loop
loop:
  %i = phi i64 [ 0, %entry ], [ %next, %loop ]
  %dead = phi i64 [ 7, %entry ], [ %bump, %loop ]
  %bump = add i64 %dead, 2
  %next = add i64 %i, 1
  %more = icmp ult i64 %next, %n
  br i1 %more, label %loop, label %done
done:
  ret i64 %next
}

; after:
define i64 @count(i64 %n) {
entry:
  br label %loop
loop:
  %i = phi i64 [ 0, %entry ], [ %next, %loop ]
  %next = add i64 %i, 1
  %more = icmp ult i64 %next, %n
  br i1 %more, label %loop, label %done
done:
  ret i64 %next
}
