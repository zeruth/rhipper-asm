; A `switch` whose cases each return, and two cases that share a block.

define i32 @dispatch(i32 %k) {
entry:
  switch i32 %k, label %other [
    i32 0, label %zero
    i32 1, label %small
    i32 2, label %small
    i32 7, label %seven
  ]
zero:
  ret i32 100
small:
  ret i32 200
seven:
  ret i32 700
other:
  %r = add i32 %k, 1
  ret i32 %r
}
