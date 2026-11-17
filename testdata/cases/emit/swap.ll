; Two loads and two stores through pointer arguments, and no result.

define void @swap(ptr %a, ptr %b) {
entry:
  %x = load i32, ptr %a, align 4
  %y = load i32, ptr %b, align 4
  store i32 %y, ptr %a, align 4
  store i32 %x, ptr %b, align 4
  ret void
}
