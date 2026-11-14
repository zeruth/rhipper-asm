; A register written and written again before a read: the first value and its narrowing reach
; nothing and go.
; pass: dce
; call: @overwrite(i64 0)
; call: @overwrite(i64 7)
; call: @overwrite(i64 -1)

define i64 @overwrite(i64 %rcx) {
entry:
  %first = add i64 %rcx, 1
  %low = trunc i64 %first to i32
  %second = mul i64 %rcx, 3
  ret i64 %second
}

; after:
define i64 @overwrite(i64 %rcx) {
entry:
  %second = mul i64 %rcx, 3
  ret i64 %second
}
