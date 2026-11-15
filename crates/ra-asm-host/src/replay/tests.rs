use super::*;

fn replay() -> Replay {
    let call = |name: &str, args: &[u128], answer: Answer| Event {
        name: name.into(),
        args: args.iter().map(|&n| Value::Int(n)).collect(),
        answer,
        writes: vec![],
    };
    let events = vec![
        call("GetStdHandle", &[0xffff_fff5], Answer::Value(Value::Int(0x54))),
        call("WriteFile", &[0x54, 0x1000, 4, 0x2000, 0], Answer::Value(Value::Int(1))),
        call("ExitProcess", &[0], Answer::Exit(0)),
    ];
    Replay { events, next: 0, open: Vec::new(), difference: None }
}

fn ints(args: &[u128]) -> Vec<interp::Value> {
    args.iter().map(|&n| interp::Value::Int(n)).collect()
}

/// The call of `name` from `@F_401000`, as the machine reports it to the replay.
fn from_main(replay: &mut Replay, name: &str, args: &[u128]) -> Result<usize, Stop> {
    replay.enter("F_401000", 0);
    replay.enter(name, 1);
    let index = replay.expect(name, &ints(args));
    replay.leave(name, 1);
    replay.leave("F_401000", 0);
    index
}

#[test]
fn a_replay_answers_each_event_in_its_order() {
    let mut replay = replay();
    let mut memory = Memory::new();
    let index = from_main(&mut replay, "GetStdHandle", &[0xffff_fff5]).expect("the first event");
    assert_eq!(replay.answer(&mut memory, index), Ok(interp::Value::Int(0x54)));
    let index = from_main(&mut replay, "WriteFile", &[0x54, 0x1000, 4, 0x2000, 0]).expect("the second event");
    assert_eq!(replay.answer(&mut memory, index), Ok(interp::Value::Int(1)));
    let index = from_main(&mut replay, "ExitProcess", &[0]).expect("the third event");
    assert_eq!(replay.answer(&mut memory, index), Err(Stop::Exit(0)));
    assert_eq!((replay.next, replay.difference), (3, None));
}

#[test]
fn a_replay_names_the_event_a_changed_argument_differs_at() {
    let mut replay = replay();
    from_main(&mut replay, "GetStdHandle", &[0xffff_fff5]).expect("the first event");
    let stop = from_main(&mut replay, "WriteFile", &[0x54, 0x1000, 5, 0x2000, 0]).expect_err("a changed length");
    let difference = replay.difference.clone().expect("the difference is kept");
    assert_eq!(difference.event, 2);
    assert_eq!(difference.function.as_deref(), Some("F_401000"));
    assert_eq!(
        difference.to_string(),
        "event 2: the call WriteFile 0x54 0x1000 0x5 0x2000 0x0 by @F_401000, where the session has WriteFile 0x54 0x1000 0x4 0x2000 0x0"
    );
    assert_eq!(stop, Stop::fault(format!("the replay differs at {difference}")));
    // The run stops there: no later call is the session's either.
    assert!(from_main(&mut replay, "ExitProcess", &[0]).is_err());
    assert_eq!(replay.difference.map(|d| d.event), Some(2));
}

#[test]
fn a_replay_names_a_call_past_the_last_event() {
    let mut replay = replay();
    replay.next = 3;
    from_main(&mut replay, "Sleep", &[1]).expect_err("no event left");
    let difference = replay.difference.expect("the difference is kept");
    assert_eq!((difference.event, difference.expected), (4, None));
}
