// The open assumptions the checked build watches, by the number `ra_assume` names each by.
const struct ra_assumption ra_assumptions[] = {
    {"F_00001000", "callee-saved", 0x1004ull},
    {"F_00001100", "table-index", 0x1108ull},
    {"F_00001200", "balanced-stack", 0x1208ull},
};
