; `imul ecx, edx`: a multiply of two registers whose low half alone is read is a 32-bit multiply
; of their low halves, and the narrowing after it goes.
; pass: widths
; call: @product(i64 3, i64 5)
; call: @product(i64 0x100000002, i64 0x7fffffff)
; call: @product(i64 -1, i64 -1)
; call: @product(i64 0x123456789, i64 0)

define i64 @product(i64 %rcx, i64 %rdx) {
entry:
  %m = mul i64 %rcx, %rdx
  %low = trunc i64 %m to i32
  %r = zext i32 %low to i64
  ret i64 %r
}

; after:
define i64 @product(i64 %rcx, i64 %rdx) {
entry:
  %n = trunc i64 %rdx to i32
  %n.1 = trunc i64 %rcx to i32
  %m = mul i32 %n.1, %n
  %r = zext i32 %m to i64
  ret i64 %r
}
