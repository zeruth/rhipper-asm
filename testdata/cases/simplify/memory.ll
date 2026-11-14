; The effects stay where they were: a sum of multiples of one value becomes a multiply and a
; chain of offsets one offset, and the stores, the global and the host's call see the same values.
; pass: simplify
; call: @fill(ptr @buf, i32 3)
; call: @fill(ptr @buf, i32 -1)
; call: @fill(ptr @buf, i32 0)

@buf = global [4 x i32] zeroinitializer, align 4
@total = global i32 100, align 4

declare i32 @report(i32)

define i32 @fill(ptr %out, i32 %n) {
entry:
  %twice = mul i32 %n, 2
  %three = add i32 %twice, %n
  %kept = or i32 %three, 0
  store i32 %kept, ptr %out, align 4
  %base = ptrtoint ptr %out to i64
  %past = add i64 %base, 12
  %at = sub i64 %past, 8
  %slot = inttoptr i64 %at to ptr
  %old = load i32, ptr @total, align 4
  %new = add i32 %old, %twice
  store i32 %new, ptr %slot, align 4
  store i32 %new, ptr @total, align 4
  %seen = call i32 @report(i32 %new)
  %r = xor i32 %seen, 0
  ret i32 %r
}
