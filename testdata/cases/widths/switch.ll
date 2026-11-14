; A switch on a register widened from 32 bits, with cases that fit, runs on the 32-bit value.
; pass: widths
; call: @classify(i32 1)
; call: @classify(i32 2)
; call: @classify(i32 3)
; call: @classify(i32 -1)

define i32 @classify(i32 %x) {
entry:
  %w = zext i32 %x to i64
  switch i64 %w, label %other [ i64 1, label %one i64 2, label %two ]
one:
  ret i32 10
two:
  ret i32 20
other:
  ret i32 0
}

; after:
define i32 @classify(i32 %x) {
entry:
  switch i32 %x, label %other [ i32 1, label %one i32 2, label %two ]
one:
  ret i32 10
two:
  ret i32 20
other:
  ret i32 0
}
