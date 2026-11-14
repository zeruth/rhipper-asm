; A function only the module calls returns a byte its caller reads as a byte: the parameter and the
; result are bytes, the mask is the byte itself, and the caller narrows its argument once.
; pass: widths
; call: @outer(i32 0x1ff)
; call: @outer(i32 7)
; call: @outer(i32 -1)

define i32 @outer(i32 %x) {
entry:
  %wx = zext i32 %x to i64
  %r = call i64 @low_byte(i64 %wx)
  %b = trunc i64 %r to i8
  %out = zext i8 %b to i32
  ret i32 %out
}

define i64 @low_byte(i64 %v) {
entry:
  %m = and i64 %v, 255
  ret i64 %m
}

; after:
define i32 @outer(i32 %x) {
entry:
  %wx = trunc i32 %x to i8
  %r = call i8 @low_byte(i8 %wx)
  %out = zext i8 %r to i32
  ret i32 %out
}

define i8 @low_byte(i8 %v) {
entry:
  ret i8 %v
}
