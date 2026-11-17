uint32_t early_return(uint32_t x, uint8_t * p) { RA_ENTER("early_return");
    if (p == (uint8_t *)0) return (uint32_t)-1;
    uint32_t v = RA_U32(p);
    if (v == 0u) return 0u;
    if (v > x) return 1u;
    return x - v;
}
