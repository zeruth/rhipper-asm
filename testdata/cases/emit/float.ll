; Double arithmetic, an ordered comparison and a conversion from an integer.
; call: @blend(double 4.0, double 1.0, i32 10) = 0.5
; call: @blend(double 1.0, double 3.0, i32 8) = 1.0
; call: @blend(double 2.0, double 2.0, i32 -2) = -4.0

define double @blend(double %a, double %b, i32 %n) {
entry:
  %sum = fadd double %a, %b
  %product = fmul double %sum, %b
  %wide = sitofp i32 %n to double
  %scaled = fdiv double %product, %wide
  %less = fcmp olt double %scaled, %a
  %r = select i1 %less, double %scaled, double %a
  ret double %r
}
