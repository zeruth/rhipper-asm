; Two constants fold, a narrowing of a widening is the value widened, a value compared with
; itself is known, and both sides of the branch still reach the phi.
; pass: simplify
; call: @f(i64 5)
; call: @f(i64 -3)
; call: @f(i64 0)
; call: @f(i64 0x1ffffffff)

define i64 @f(i64 %x) {
entry:
  %k = mul i32 6, 7
  %wide = zext i32 %k to i64
  %low = trunc i64 %x to i32
  %back = zext i32 %low to i64
  %again = trunc i64 %back to i32
  %same = icmp eq i32 %again, %again
  %pick = select i1 %same, i64 %wide, i64 %x
  %neg = icmp slt i32 %again, 0
  br i1 %neg, label %minus, label %plus
minus:
  %m = sub i64 %pick, %back
  br label %done
plus:
  %p = add i64 %pick, %back
  br label %done
done:
  %r = phi i64 [ %m, %minus ], [ %p, %plus ]
  ret i64 %r
}
