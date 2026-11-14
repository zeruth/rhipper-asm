; A division by ten through its magic multiply: the widened multiply and the shift are `x / 10`.
; The remainder beside it waits for the next round, which sees the division.
; pass: simplify
; call: @tenths(i32 0)
; call: @tenths(i32 9)
; call: @tenths(i32 10)
; call: @tenths(i32 12345)
; call: @tenths(i32 -1)
; call: @tenths(i32 0x80000000)

define i32 @tenths(i32 %x) {
entry:
  %w = zext i32 %x to i64
  %m = mul i64 %w, 3435973837
  %s = lshr i64 %m, 35
  %q = trunc i64 %s to i32
  %p = mul i32 %q, 10
  %r = sub i32 %x, %p
  %out = add i32 %q, %r
  ret i32 %out
}

; after:
define i32 @tenths(i32 %x) {
entry:
  %w = zext i32 %x to i64
  %m = mul i64 %w, 3435973837
  %s = lshr i64 %m, 35
  %q = udiv i32 %x, 10
  %p = mul i32 %q, 10
  %r = sub i32 %x, %p
  %out = add i32 %q, %r
  ret i32 %out
}
