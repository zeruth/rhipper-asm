; `cdq`: the sign spread over the high half, shifted up and joined to the value, is one `sext`; the
; pieces stay for the dead code to take.
; pass: simplify
; call: @widen(i32 0)
; call: @widen(i32 5)
; call: @widen(i32 -1)
; call: @widen(i32 -2147483648)
; call: @widen(i32 2147483647)

define i64 @widen(i32 %x) {
entry:
  %sign = ashr i32 %x, 31
  %hi = zext i32 %sign to i64
  %high = shl i64 %hi, 32
  %lo = zext i32 %x to i64
  %v = or i64 %high, %lo
  ret i64 %v
}

; after:
define i64 @widen(i32 %x) {
entry:
  %sign = ashr i32 %x, 31
  %hi = zext i32 %sign to i64
  %high = shl i64 %hi, 32
  %lo = zext i32 %x to i64
  %v = sext i32 %x to i64
  ret i64 %v
}
