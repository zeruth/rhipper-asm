uint64_t calls(uint64_t x) { RA_ENTER("calls");
    uint64_t first = report(x, 1ull);
    report(first, 2ull);
    return first + x;
}
