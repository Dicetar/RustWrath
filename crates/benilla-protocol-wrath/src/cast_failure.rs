//! Cast-failure reason translation.
//!
//! 3.3.5 `SpellCastResult` (`SharedDefines.h:935-1127`) has the same failure names as 1.12 but
//! inserts and appends values, so the numeric byte cannot cross the adapter. Map by meaning to
//! 1.12's `CAST_FAIL_KEYS` order (`benilla-app/src/ui_action/cast_fail.rs`); `None` means 1.12
//! has no corresponding message and the caller must report it explicitly rather than guess.

/// A 3.3.5 reason with a 1.12 meaning, or an unsupported original value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    Vanilla(u8),
    Unsupported(u8),
}

/// 3.3.5 reason byte -> 1.12 reason byte, by `SPELL_FAILED_*` name. Semantic aliases are the
/// only entries not copied from the same name: full mana -> full power, incorrect area -> not
/// here, ranged-weapon missing -> weapon missing, not-on-shapeshift -> not-shapeshift,
/// partial/no playtime -> playtime, and cannot-right-now -> try-again.
const VANILLA_CAST_FAILURE: [Option<u8>; 188] = [
    None, Some(0), Some(1), Some(2), Some(2), Some(3), Some(4), Some(5),
    Some(6), Some(7), Some(8), Some(9), Some(10), Some(11), Some(12), None,
    None, Some(13), Some(14), Some(15), Some(16), Some(17), Some(18), Some(19),
    Some(20), Some(21), Some(22), Some(23), Some(24), Some(25), Some(26), Some(27),
    Some(28), Some(29), Some(30), Some(31), Some(32), Some(33), Some(34), Some(53),
    Some(35), Some(36), Some(37), Some(38), Some(39), Some(40), Some(41), Some(42),
    Some(43), Some(44), Some(45), Some(46), Some(47), Some(48), Some(49), None,
    Some(50), Some(51), Some(52), None, Some(53), Some(54), Some(55), Some(56),
    Some(57), Some(58), Some(59), Some(60), Some(61), Some(62), Some(63), Some(64),
    Some(65), Some(66), None, Some(67), Some(68), Some(69), Some(70), Some(71),
    Some(72), Some(73), Some(74), Some(75), Some(76), Some(77), Some(78), Some(79),
    Some(80), Some(81), Some(82), Some(83), Some(84), Some(85), Some(86), Some(87),
    Some(88), Some(89), Some(90), Some(91), Some(92), Some(93), Some(94), Some(95),
    Some(96), Some(97), Some(98), Some(99), Some(100), Some(101), Some(102), Some(103),
    Some(104), Some(105), Some(106), Some(107), Some(108), Some(109), None, Some(110),
    Some(111), Some(112), Some(113), Some(114), Some(115), Some(115), Some(116), Some(117),
    Some(118), Some(119), None, Some(120), Some(122), Some(123), Some(124), Some(125),
    Some(126), Some(127), Some(128), Some(129), Some(133), Some(134), Some(135), Some(136),
    Some(138), Some(139), Some(140), Some(141), Some(142), Some(143), Some(144), None,
    Some(61), None, None, None, None, None, None, None,
    None, None, None, None, Some(142), Some(142), None, None,
    None, None, None, None, None, Some(122), None, None,
    None, None, None, None, None, None, None, None,
    None, None, None, Some(145),
];

/// Map a wire reason. `0` is 3.3.5's success sentinel, never a failure, and is unsupported here.
pub fn reason(wrath: u8) -> Reason {
    match VANILLA_CAST_FAILURE.get(usize::from(wrath)) {
        Some(Some(vanilla)) => Reason::Vanilla(*vanilla),
        _ => Reason::Unsupported(wrath),
    }
}
