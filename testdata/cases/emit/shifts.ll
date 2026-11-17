; The shifts and divisions whose signedness C spells by a cast of the operand.

define i32 @shifts(i32 %a, i32 %b) {
entry:
  %left = shl i32 %a, 3
  %arithmetic = ashr i32 %a, 2
  %logical = lshr i32 %b, 4
  %quotient = sdiv i32 %left, %b
  %remainder = urem i32 %logical, 10
  %t = add i32 %quotient, %remainder
  %r = or i32 %t, %arithmetic
  ret i32 %r
}
