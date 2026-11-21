//! The operand stack against the compiler: for every method of the fixture class, in every
//! release, the walk reaches every instruction with one stack, and the deepest the stack gets
//! is the `max_stack` javac wrote.

use std::path::PathBuf;

use ra_asm_jvm::ClassFile;
use ra_asm_jvm::flow::{Kind, flow, method_kinds};

#[test]
fn the_stack_is_what_the_compiler_says() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/java");
    let mut methods = 0;
    for release in 8..=25 {
        let file = ClassFile::parse(&std::fs::read(fixtures.join(format!("{release}/Opcodes.class"))).unwrap()).unwrap();
        for method in &file.methods {
            let name = file.utf8(method.name).unwrap();
            let Some(code) = file.code(method).unwrap() else { continue };
            let walked = flow(&file, &code).unwrap_or_else(|e| panic!("release {release}, {name}: {e}"));
            assert_eq!(walked.max_stack, code.max_stack, "release {release}, {name}: the stack's depth");
            // javac writes no code control cannot reach.
            assert!(walked.stacks.iter().all(Option::is_some), "release {release}, {name}: an instruction nothing reaches");
            // A return finds what the method returns, alone.
            let (_, returned) = method_kinds(file.utf8(method.descriptor).unwrap()).unwrap();
            for (instruction, stack) in walked.instructions.iter().zip(&walked.stacks) {
                if (0xac..=0xb1).contains(&instruction.opcode) {
                    let expected: Vec<Kind> = returned.into_iter().collect();
                    assert_eq!(stack.as_deref(), Some(expected.as_slice()), "release {release}, {name}: the stack at its return");
                }
            }
            methods += 1;
        }
    }
    assert!(methods >= 18 * 15, "{methods} methods walked");
}
