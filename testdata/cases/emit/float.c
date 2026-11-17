double blend(double a, double b, uint32_t n) { RA_ENTER("blend");
    double scaled = ((a + b) * b) / (double)(int32_t)n;
    return (scaled < a) ? scaled : a;
}
