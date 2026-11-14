; `cmp; jne` lifts as the zero flag negated, which is one `ne` of the operands; a `test eax, eax`
; is a direct comparison with zero, which stays, and the cleanup reads its `and` as the register.
; pass: conditions
; call: @pick(i32 3, i32 3)
; call: @pick(i32 3, i32 5)
; call: @pick(i32 0, i32 5)
; call: @pick(i32 -7, i32 -7)
; call: @pick(i32 -7, i32 9)

define i32 @pick(i32 %a, i32 %b) {
entry:
  %d = sub i32 %a, %b
  %zf = icmp eq i32 %d, 0
  %nz = xor i1 %zf, true
  br i1 %nz, label %differ, label %same
differ:
  %t = and i32 %a, %a
  %tz = icmp eq i32 %t, 0
  %r = select i1 %tz, i32 -1, i32 %d
  ret i32 %r
same:
  ret i32 0
}

; after:
define i32 @pick(i32 %a, i32 %b) {
entry:
  %d = sub i32 %a, %b
  %nz = icmp ne i32 %a, %b
  br i1 %nz, label %differ, label %same
differ:
  %tz = icmp eq i32 %a, 0
  %r = select i1 %tz, i32 -1, i32 %d
  ret i32 %r
same:
  ret i32 0
}
