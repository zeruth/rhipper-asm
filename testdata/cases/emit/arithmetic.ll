; Straight-line arithmetic on two registers: one expression per value the return does not fold.
; call: @arithmetic(i64 5, i64 3) = 56
; call: @arithmetic(i64 2, i64 7) = -144

define i64 @arithmetic(i64 %a, i64 %b) {
entry:
  %sum = add i64 %a, %b
  %difference = sub i64 %a, %b
  %product = mul i64 %sum, %difference
  %scaled = mul i64 %product, 3
  %r = xor i64 %scaled, %sum
  ret i64 %r
}
