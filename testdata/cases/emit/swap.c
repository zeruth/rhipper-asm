void swap(uint8_t * a, uint8_t * b) { RA_ENTER("swap");
    uint32_t x = RA_U32(a);
    RA_U32(a) = RA_U32(b);
    RA_U32(b) = x;
}
