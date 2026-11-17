uint64_t arithmetic(uint64_t a, uint64_t b) { RA_ENTER("arithmetic");
    uint64_t sum = a + b;
    return ((sum * (a - b)) * 3ull) ^ sum;
}
