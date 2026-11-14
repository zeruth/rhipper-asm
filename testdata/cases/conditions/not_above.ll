; `cmp; jbe`: the carry, already a comparison, and the zero of the difference are one `ule`; the
; difference stays, since a side of the branch returns it.
; pass: conditions
; call: @distance(i32 3, i32 5)
; call: @distance(i32 5, i32 3)
; call: @distance(i32 4, i32 4)
; call: @distance(i32 0, i32 -1)
; call: @distance(i32 -1, i32 0)

define i32 @distance(i32 %a, i32 %b) {
entry:
  %d = sub i32 %a, %b
  %cf = icmp ult i32 %a, %b
  %zf = icmp eq i32 %d, 0
  %be = or i1 %cf, %zf
  br i1 %be, label %low, label %high
low:
  ret i32 %d
high:
  %neg = sub i32 0, %d
  ret i32 %neg
}

; after:
define i32 @distance(i32 %a, i32 %b) {
entry:
  %d = sub i32 %a, %b
  %be = icmp ule i32 %a, %b
  br i1 %be, label %low, label %high
low:
  ret i32 %d
high:
  %neg = sub i32 0, %d
  ret i32 %neg
}
