; Double arithmetic, an ordered comparison and a conversion from an integer.

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
