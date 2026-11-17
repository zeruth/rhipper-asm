uint64_t counter(uint64_t step) { RA_ENTER("counter");
    uint64_t old = RA_U64((uint8_t *)&count);
    RA_U64((uint8_t *)&count) = old + step;
    return old;
}
