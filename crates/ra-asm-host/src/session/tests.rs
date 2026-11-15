use super::*;

fn sample() -> Session {
    let long: Vec<u8> = (0..150u8).collect();
    Session {
        events: vec![
            Event {
                name: "GetStdHandle".into(),
                args: vec![Value::Int(0xffff_fff5)],
                answer: Answer::Value(Value::Int(0x54)),
                writes: vec![],
            },
            Event {
                name: "ra_x86_pow_sse2".into(),
                args: vec![Value::F64(2.0f64.to_bits()), Value::F32(f32::NAN.to_bits())],
                answer: Answer::Value(Value::Aggregate(vec![Value::Vector(vec![]), Value::Vector(vec![Value::Int(1), Value::F32(0)])])),
                writes: vec![(0x1000, vec![1, 2, 3]), (0x2000, long)],
            },
            Event { name: "??2@YAPAXI@Z".into(), args: vec![], answer: Answer::Fault("a = b ! c\\d\nnext\r".into()), writes: vec![] },
            Event { name: "ExitProcess".into(), args: vec![Value::Int(0)], answer: Answer::Exit(-3), writes: vec![(0x10, vec![0xff])] },
            Event { name: "Sleep".into(), args: vec![Value::Int(u128::MAX)], answer: Answer::Limit(7), writes: vec![] },
        ],
    }
}

#[test]
fn a_session_reads_back_in_its_format() {
    let session = sample();
    let text = session.write();
    assert!(text.starts_with("ra-session 1\ncall GetStdHandle 0xfffffff5 = 0x54\n"), "{text}");
    assert!(text.contains("call ExitProcess 0x0 ! exit -3\n  write 0x10 ff\n"), "{text}");
    let read = Session::parse(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
    assert_eq!(read, session);
    assert_eq!(read.write(), text);
    assert_eq!(Session::parse(&text.replace('\n', "\r\n")).as_ref(), Ok(&session));
}

#[test]
fn a_line_that_does_not_read_is_named_in_the_format() {
    let error = |text: &str| Session::parse(text).unwrap_err();
    assert_eq!(error("ra-session 2\n").line, 1);
    assert_eq!(error("ra-session 1\n  write 0x10 00\n").line, 2);
    assert_eq!(error("ra-session 1\ncall f 0x1\n").line, 2);
    assert_eq!(error("ra-session 1\ncall f 0x1 = 0x\n").line, 2);
    assert_eq!(error("ra-session 1\ncall f = 0x0\n  write 0x10 0\n").line, 3);
    assert_eq!(error("ra-session 1\ncall f ! fault a\\q\n").line, 2);
}
