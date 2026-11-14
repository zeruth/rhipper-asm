; A register a loop never writes: its phi brings the entry's value or its own, so it is the entry's
; value, which the exit reads directly.
; pass: dce
; call: @spin(i64 5, i64 0)
; call: @spin(i64 5, i64 3)
; call: @spin(i64 -1, i64 2)

define i64 @spin(i64 %x, i64 %n) {
entry:
  br label %loop
loop:
  %i = phi i64 [ 0, %entry ], [ %next, %loop ]
  %keep = phi i64 [ %x, %entry ], [ %keep, %loop ]
  %next = add i64 %i, 1
  %more = icmp ult i64 %next, %n
  br i1 %more, label %loop, label %done
done:
  %r = add i64 %keep, %i
  ret i64 %r
}

; after:
define i64 @spin(i64 %x, i64 %n) {
entry:
  br label %loop
loop:
  %i = phi i64 [ 0, %entry ], [ %next, %loop ]
  %next = add i64 %i, 1
  %more = icmp ult i64 %next, %n
  br i1 %more, label %loop, label %done
done:
  %r = add i64 %x, %i
  ret i64 %r
}
