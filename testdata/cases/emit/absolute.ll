; A diamond whose join takes a phi: an `if` that assigns the joined value on one side.

define i64 @absolute(i64 %x) {
entry:
  %negative = icmp slt i64 %x, 0
  br i1 %negative, label %flip, label %done
flip:
  %negated = sub i64 0, %x
  br label %done
done:
  %r = phi i64 [ %negated, %flip ], [ %x, %entry ]
  ret i64 %r
}
