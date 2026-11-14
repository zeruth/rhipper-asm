; The identities the lift writes to keep a value's type or to make a constant: each is its
; operand, and the uses follow the chain to it.
; pass: simplify
; call: @f(i32 7, i32 9)
; call: @f(i32 -1, i32 0)
; call: @f(i32 2147483647, i32 1)

define i32 @f(i32 %a, i32 %b) {
entry:
  %a0 = or i32 %a, 0
  %a1 = and i32 %a0, -1
  %b0 = add i32 %b, 0
  %b1 = xor i32 %b0, 0
  %both = and i32 %a1, %a1
  %sum = add i32 %both, %b1
  %none = sub i32 %sum, %sum
  %out = or i32 %sum, %none
  ret i32 %out
}

; after:
define i32 @f(i32 %a, i32 %b) {
entry:
  %sum = add i32 %a, %b
  %out = or i32 %sum, 0
  ret i32 %out
}
