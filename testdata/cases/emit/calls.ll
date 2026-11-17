; Calls of a declaration: one whose result is read, one whose result is dropped.

declare i64 @report(i64, i64)

define i64 @calls(i64 %x) {
entry:
  %first = call i64 @report(i64 %x, i64 1)
  %ignored = call i64 @report(i64 %first, i64 2)
  %r = add i64 %first, %x
  ret i64 %r
}
