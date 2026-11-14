; What a function does stays though no value reads it: the store, and the call whose result is
; dropped with the argument it was given. A load nothing reads and the arithmetic on it go.
; pass: dce
; call: @effects(ptr @cell, i64 5)
; call: @effects(ptr @seen, i64 -1)

@seen = global i64 100, align 8
@cell = global i64 41, align 8

declare i64 @log(i64)

define i64 @effects(ptr %p, i64 %x) {
entry:
  %old = load i64, ptr %p, align 8
  %unused = load i64, ptr @seen, align 8
  %twice = add i64 %x, %x
  %ignored = call i64 @log(i64 %twice)
  %junk = xor i64 %unused, %ignored
  store i64 %x, ptr @seen, align 8
  ret i64 %old
}

; after:
define i64 @effects(ptr %p, i64 %x) {
entry:
  %old = load i64, ptr %p, align 8
  %twice = add i64 %x, %x
  %ignored = call i64 @log(i64 %twice)
  store i64 %x, ptr @seen, align 8
  ret i64 %old
}
