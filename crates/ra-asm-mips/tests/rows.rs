//! The machines of rows the MIPS processors are, checked as `ra-asm-machine` checks a table and
//! run through the interpreter over their state.

mod r5900_spec {
    use ra_asm_ir::interp::{Host, Memory, Region, Stop, Value, Vm};
    use ra_asm_ir::{FnType, Module};
    use ra_asm_machine::check;
    use ra_asm_mips::Mips;
    use ra_asm_mips::r5900::spec::{ACC, BITS, F0, HI, HI1, LO, LO1, REGISTERS, SA, SPEC, machine};

    /// Where the state lies in the interpreter's memory.
    const STATE: u64 = 0x1_0000;
    const ADDRESS: u64 = 0x1000;

    /// The runtime of one instruction's function, which calls nothing.
    struct Refuses;

    impl Host for Refuses {
        fn call(&mut self, _: &mut Vm<'_>, name: &str, _: &FnType, _: &[Value]) -> Result<Value, Stop> {
            Err(Stop::fault(format!("a call to {name}")))
        }
    }

    fn addu(rd: u32, rs: u32, rt: u32) -> u32 {
        (rs << 21) | (rt << 16) | (rd << 11) | 0x21
    }

    /// The registers after `word` runs over `before`, every slot of the file.
    fn run(word: u32, before: &[u128]) -> Vec<u128> {
        let function = machine().lift(&SPEC, word, ADDRESS).expect("the word decodes");
        let name = function.name.clone();
        let mut module = Module::x86_64_windows("r5900");
        SPEC.declare(&mut module);
        module.functions.push(function);
        let mut memory = Memory::new();
        memory.add_region(Region::owned(STATE, SPEC.size(), "state"));
        for (n, value) in before.iter().enumerate() {
            memory.write(STATE + SPEC.register_offset(n as u32), &value.to_le_bytes()).expect("the state maps");
        }
        let mut vm = Vm::new(&module, memory);
        vm.call_named(&mut Refuses, &name, vec![Value::Int(u128::from(STATE))]).expect("the instruction runs");
        (0..REGISTERS)
            .map(|n| {
                let bytes = vm.memory.read_vec(STATE + SPEC.register_offset(n), 16).expect("the state reads back");
                u128::from_le_bytes(bytes.try_into().expect("a slot is 16 bytes"))
            })
            .collect()
    }

    /// A file of distinct values in both halves of every slot but `$zero`.
    fn file() -> Vec<u128> {
        (0..REGISTERS).map(|n| if n == 0 { 0 } else { (u128::from(n) << 96) | (u128::from(n) << 64) | (u128::from(n) * 0x0101) }).collect()
    }

    #[test]
    fn the_table_passes_the_check() {
        for seed in [1, 7, 42] {
            if let Err(failures) = check(machine(), &SPEC, seed) {
                let text: Vec<String> = failures.iter().map(ToString::to_string).collect();
                panic!("seed {seed}: {}", text.join("; "));
            }
        }
    }

    #[test]
    fn addu_writes_the_low_64_bits_sign_extended_and_leaves_the_high_64() {
        let mut before = file();
        before[4] = 0xaaaa_bbbb_cccc_dddd_dead_beef_7fff_ffff;
        before[5] = 0x5555_6666_7777_8888_0000_0000_0000_0001;
        before[6] = 0x1234_5678_9abc_def0_1111_2222_3333_4444;
        let after = run(addu(6, 4, 5), &before);
        assert_eq!(after[6], 0x1234_5678_9abc_def0_ffff_ffff_8000_0000, "{:#x}", after[6]);
        let mut expected = before.clone();
        expected[6] = after[6];
        assert_eq!(after, expected, "only $6 changes");
    }

    #[test]
    fn addu_of_a_positive_sum_clears_the_low_half_above_it() {
        let mut before = file();
        before[7] = 0xffff_ffff_ffff_ffff_0000_0000_0000_0002;
        before[8] = 0x0000_0000_0000_0000_ffff_ffff_0000_0003;
        before[9] = u128::MAX;
        let after = run(addu(9, 7, 8), &before);
        assert_eq!(after[9], 0xffff_ffff_ffff_ffff_0000_0000_0000_0005, "{:#x}", after[9]);
    }

    #[test]
    fn addu_to_zero_changes_nothing() {
        let before = file();
        assert_eq!(run(addu(0, 4, 5), &before), before);
    }

    #[test]
    fn the_state_has_hi1_lo1_sa_and_the_fpu_with_its_accumulator() {
        let registers = machine().registers;
        assert_eq!(registers.len(), REGISTERS as usize);
        let named = |n: u32| registers[n as usize].name;
        assert_eq!([named(HI), named(LO), named(HI1), named(LO1), named(SA)], ["hi", "lo", "hi1", "lo1", "sa"]);
        assert_eq!([named(F0), named(F0 + 31), named(ACC)], ["$f0", "$f31", "acc"]);
        assert_eq!((named(0), named(29), named(31)), ("$zero", "$sp", "$ra"));
        assert_eq!(registers[29].bits, BITS);
        assert_eq!(SPEC.register_offset(HI1), 34 * 16);
        assert_eq!(SPEC.pc_offset(), u64::from(REGISTERS) * 16);
        assert_eq!(SPEC.size(), u64::from(REGISTERS) * 16 + 16);
    }

    #[test]
    fn the_rows_are_the_r5900_decoders_in_their_places() {
        let ours = machine().table.rows();
        let decoder = Mips::r5900_machine().table.rows();
        assert_eq!(ours.len(), decoder.len());
        for (row, theirs) in ours.iter().zip(decoder) {
            assert_eq!(
                (row.name, row.mask, row.bits, row.fields, row.flow),
                (theirs.name, theirs.mask, theirs.bits, theirs.fields, theirs.flow)
            );
        }
        let lifted: Vec<&str> = ours.iter().filter(|row| row.sem.is_some()).map(|row| row.name).collect();
        assert_eq!(lifted, ["addu"]);
        let instr = machine().instr(addu(6, 4, 5), ADDRESS).expect("addu decodes");
        assert_eq!(instr.text, "addu\t$6, $4, $5");
    }
}
