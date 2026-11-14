; The `lea` shapes: `x + x * 4` is `x * 5` and `(x * 4) * 2` is `x * 8`; a chain of offsets on one
; value is one offset, and two widenings are one. A sum of two different values stays.
; pass: simplify
; call: @scaled(i64 3, i16 7)
; call: @scaled(i64 0, i16 0)
; call: @scaled(i64 -1, i16 -1)
; call: @scaled(i64 0x4000000000000000, i16 0x8000)

define i64 @scaled(i64 %x, i16 %y) {
entry:
  %four = mul i64 %x, 4
  %five = add i64 %x, %four
  %eight = mul i64 %four, 2
  %sum = add i64 %five, %eight
  %low = sub i64 %sum, 56
  %back = add i64 %low, 52
  %yw = zext i16 %y to i32
  %yww = zext i32 %yw to i64
  %out = add i64 %back, %yww
  ret i64 %out
}

; after:
define i64 @scaled(i64 %x, i16 %y) {
entry:
  %four = mul i64 %x, 4
  %five = mul i64 %x, 5
  %eight = mul i64 %x, 8
  %sum = add i64 %five, %eight
  %low = sub i64 %sum, 56
  %back = sub i64 %sum, 4
  %yw = zext i16 %y to i32
  %yww = zext i16 %y to i64
  %out = add i64 %back, %yww
  ret i64 %out
}
