; `cmp; jl`: the sign and the overflow of the difference, combined, are one signed comparison of
; the operands, and the flag arithmetic goes with the cleanup. The overflow is in the calls.
; pass: conditions
; call: @less(i32 1, i32 2)
; call: @less(i32 2, i32 1)
; call: @less(i32 5, i32 5)
; call: @less(i32 -2147483648, i32 1)
; call: @less(i32 2147483647, i32 -1)

define i32 @less(i32 %a, i32 %b) {
entry:
  %d = sub i32 %a, %b
  %sf = icmp slt i32 %d, 0
  %x1 = xor i32 %a, %b
  %x2 = xor i32 %a, %d
  %ov = and i32 %x1, %x2
  %of = icmp slt i32 %ov, 0
  %l = xor i1 %sf, %of
  br i1 %l, label %yes, label %no
yes:
  ret i32 1
no:
  ret i32 0
}

; after:
define i32 @less(i32 %a, i32 %b) {
entry:
  %l = icmp slt i32 %a, %b
  br i1 %l, label %yes, label %no
yes:
  ret i32 1
no:
  ret i32 0
}
