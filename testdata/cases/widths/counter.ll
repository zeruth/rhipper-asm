; A 32-bit loop counter kept in a 64-bit register: the phi runs at 32 bits, its narrowing goes, and
; the value the loop carries back is the 32-bit sum itself.
; pass: widths
; call: @count(i64 0)
; call: @count(i64 5)
; call: @count(i64 0x100000003)

define i64 @count(i64 %n) {
entry:
  br label %loop
loop:
  %i = phi i64 [ 0, %entry ], [ %next, %loop ]
  %low = trunc i64 %i to i32
  %inc = add i32 %low, 1
  %next = zext i32 %inc to i64
  %limit = trunc i64 %n to i32
  %more = icmp ult i32 %inc, %limit
  br i1 %more, label %loop, label %done
done:
  ret i64 %next
}

; after:
define i64 @count(i64 %n) {
entry:
  br label %loop
loop:
  %i = phi i32 [ 0, %entry ], [ %inc, %loop ]
  %inc = add i32 %i, 1
  %next = zext i32 %inc to i64
  %limit = trunc i64 %n to i32
  %more = icmp ult i32 %inc, %limit
  br i1 %more, label %loop, label %done
done:
  ret i64 %next
}
