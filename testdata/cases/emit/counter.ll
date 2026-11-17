; A global read, bumped and written back: the access of a global by name.
; call: @counter(i64 5) = 0
; call: @counter(i64 2) = 5
; call: @counter(i64 1) = 7

@count = global i64 0, align 8

define i64 @counter(i64 %step) {
entry:
  %old = load i64, ptr @count, align 8
  %new = add i64 %old, %step
  store i64 %new, ptr @count, align 8
  ret i64 %old
}
