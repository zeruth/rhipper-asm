; A value narrowed and widened again, by zero and by its sign.
; call: @widths(i64 511) = 766
; call: @widths(i64 32769) = -32766
; call: @widths(i64 74565) = 9098

define i64 @widths(i64 %x) {
entry:
  %byte = trunc i64 %x to i8
  %word = trunc i64 %x to i16
  %unsigned = zext i8 %byte to i64
  %signed = sext i16 %word to i64
  %r = add i64 %unsigned, %signed
  ret i64 %r
}
