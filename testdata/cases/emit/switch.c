uint32_t dispatch(uint32_t k) { RA_ENTER("dispatch");
    enum dispatch_case { DISPATCH_0 = 0, DISPATCH_1 = 1, DISPATCH_2 = 2, DISPATCH_7 = 7 };
    switch ((enum dispatch_case)k) {
    case DISPATCH_0:
        return 100u;
    case DISPATCH_1:
    case DISPATCH_2:
        return 200u;
    case DISPATCH_7:
        return 700u;
    default:
        return k + 1u;
    }
}
