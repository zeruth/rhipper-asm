uint64_t sum_loop(uint64_t n) { RA_ENTER("sum_loop");
    uint64_t i, sum;
    i = 0ull;
    sum = 0ull;
    if (n == 0ull) return 0ull;
    do {
        sum = sum + i;
        i = i + 1ull;
    } while (i < n);
    return sum;
}
