; A chain of tests that each return early, and the fall-through return after them.

define i32 @early_return(i32 %x, ptr %p) {
entry:
  %null = icmp eq ptr %p, null
  br i1 %null, label %nothing, label %read
nothing:
  ret i32 -1
read:
  %v = load i32, ptr %p, align 4
  %zero = icmp eq i32 %v, 0
  br i1 %zero, label %empty, label %compare
empty:
  ret i32 0
compare:
  %above = icmp ugt i32 %v, %x
  br i1 %above, label %big, label %rest
big:
  ret i32 1
rest:
  %r = sub i32 %x, %v
  ret i32 %r
}
