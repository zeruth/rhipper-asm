//! The controller both PlayStations take: its buttons and sticks as the host reads them, and the
//! byte protocol the console's serial port speaks with it. The first console drives the protocol
//! from its own registers; the second's pad library asks for the same report in one piece.

/// The buttons, a bit each, set while pressed: the order of the two button bytes of the report
/// (which carries them inverted).
pub const SELECT: u16 = 1 << 0;
pub const L3: u16 = 1 << 1;
pub const R3: u16 = 1 << 2;
pub const START: u16 = 1 << 3;
pub const UP: u16 = 1 << 4;
pub const RIGHT: u16 = 1 << 5;
pub const DOWN: u16 = 1 << 6;
pub const LEFT: u16 = 1 << 7;
pub const L2: u16 = 1 << 8;
pub const R2: u16 = 1 << 9;
pub const L1: u16 = 1 << 10;
pub const R1: u16 = 1 << 11;
pub const TRIANGLE: u16 = 1 << 12;
pub const CIRCLE: u16 = 1 << 13;
pub const CROSS: u16 = 1 << 14;
pub const SQUARE: u16 = 1 << 15;

/// The buttons by the names an input script writes them with.
pub const NAMES: [(&str, u16); 16] = [
    ("select", SELECT),
    ("l3", L3),
    ("r3", R3),
    ("start", START),
    ("up", UP),
    ("right", RIGHT),
    ("down", DOWN),
    ("left", LEFT),
    ("l2", L2),
    ("r2", R2),
    ("l1", L1),
    ("r1", R1),
    ("triangle", TRIANGLE),
    ("circle", CIRCLE),
    ("cross", CROSS),
    ("square", SQUARE),
];

/// A controller at one moment: the buttons pressed and the two sticks, `0x80` at rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct State {
    pub buttons: u16,
    /// The right stick's x and y, then the left's, as the analog report orders them.
    pub sticks: [u8; 4],
}

impl Default for State {
    fn default() -> Self {
        Self { buttons: 0, sticks: [0x80; 4] }
    }
}

impl State {
    /// The buttons of a script's `up+cross` form; `None` for a name that is no button.
    pub fn parse(text: &str) -> Option<Self> {
        let mut buttons = 0;
        for name in text.split('+').map(str::trim).filter(|n| !n.is_empty() && *n != "none") {
            buttons |= NAMES.iter().find(|(n, _)| n.eq_ignore_ascii_case(name))?.1;
        }
        Some(Self { buttons, ..Self::default() })
    }

    /// The bytes a poll answers after its header: the two button bytes, a clear bit for a pressed
    /// button, and the four stick bytes of the analog form.
    pub fn report(&self, analog: bool) -> Vec<u8> {
        let mut out = (!self.buttons).to_le_bytes().to_vec();
        if analog {
            out.extend_from_slice(&self.sticks);
        }
        out
    }
}

/// The identifier of the digital pad, and of the analog one with its sticks on.
pub const DIGITAL: u8 = 0x41;
pub const ANALOG: u8 = 0x73;

/// One port's end of the serial protocol: the console sends a byte, the device answers with the
/// byte it had ready and acknowledges while it has more. A transfer is `01` (the pad's address),
/// `42` (poll), then a byte per answer: the identifier, `5a`, and the report.
#[derive(Debug, Default)]
pub struct Port {
    /// The bytes still to answer in this transfer.
    answer: std::collections::VecDeque<u8>,
    /// How far into the transfer the console is: 0 before the address, 1 before the command.
    step: u8,
}

impl Port {
    /// Ends the transfer: the console dropped the port's select line.
    pub fn deselect(&mut self) {
        self.answer.clear();
        self.step = 0;
    }

    /// The device's answer to `sent` and whether it acknowledges; `pad` is what the port holds.
    /// The memory card's address (`81`) and an empty port answer nothing.
    pub fn exchange(&mut self, pad: Option<State>, sent: u8) -> (u8, bool) {
        let Some(pad) = pad else { return (0xff, false) };
        match self.step {
            0 if sent == 0x01 => {
                self.step = 1;
                (0xff, true)
            }
            0 => (0xff, false),
            1 => {
                self.step = 2;
                if sent != 0x42 {
                    // The configuration commands of the analog pad: a digital pad answers none.
                    self.step = 0;
                    return (DIGITAL, false);
                }
                self.answer = std::iter::once(0x5a).chain(pad.report(false)).collect();
                (DIGITAL, true)
            }
            _ => {
                let byte = self.answer.pop_front().unwrap_or(0xff);
                let more = !self.answer.is_empty();
                if !more {
                    self.step = 0;
                }
                (byte, more)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_poll_answers_the_identifier_and_the_buttons_inverted() {
        let pad = State { buttons: CROSS | START, ..State::default() };
        let mut port = Port::default();
        let sent = [0x01, 0x42, 0x00, 0x00, 0x00];
        let got: Vec<(u8, bool)> = sent.iter().map(|&b| port.exchange(Some(pad), b)).collect();
        assert_eq!(got, [(0xff, true), (0x41, true), (0x5a, true), (0xf7, true), (0xbf, false)]);
        // The next transfer starts over.
        assert_eq!(port.exchange(Some(pad), 0x01), (0xff, true));
    }

    #[test]
    fn an_empty_port_and_a_memory_card_address_answer_nothing() {
        let mut port = Port::default();
        assert_eq!(port.exchange(None, 0x01), (0xff, false));
        assert_eq!(port.exchange(Some(State::default()), 0x81), (0xff, false));
    }

    #[test]
    fn a_script_names_buttons_joined_by_plus() {
        assert_eq!(State::parse("up+Cross").map(|s| s.buttons), Some(UP | CROSS));
        assert_eq!(State::parse("none").map(|s| s.buttons), Some(0));
        assert_eq!(State::parse("jump"), None);
    }
}
