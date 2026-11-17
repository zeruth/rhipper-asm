uint64_t absolute(uint64_t x) { RA_ENTER("absolute");
    if ((int64_t)x < 0ll) return 0ull - x;
    return x;
}
