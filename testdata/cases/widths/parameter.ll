; A function only the module calls reads the low byte of its parameter: the parameter is a byte,
; and the caller passes its register narrowed. The result stays, since the caller reads it whole.
; pass: widths
; call: @caller(i64 0x1234)
; call: @caller(i64 -1)
; call: @caller(i64 0)

define i64 @caller(i64 %x) {
entry:
  %r = call i64 @inner(i64 %x)
  ret i64 %r
}

define i64 @inner(i64 %v) {
entry:
  %low = trunc i64 %v to i8
  %w = zext i8 %low to i64
  ret i64 %w
}

; after:
define i64 @caller(i64 %x) {
entry:
  %a = trunc i64 %x to i8
  %r = call i64 @inner(i8 %a)
  ret i64 %r
}

define i64 @inner(i8 %v) {
entry:
  %w = zext i8 %v to i64
  ret i64 %w
}
