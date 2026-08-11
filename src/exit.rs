#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Code {
    Conflict,
    Error,
    Findings,
    Locked,
    Misuse,
    Ok,
}

impl Code {
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::Conflict | Self::Error => 1,
            Self::Findings => 4,
            Self::Locked => 3,
            Self::Misuse => 2,
            Self::Ok => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Code;

    #[test]
    fn codes_are_the_documented_interface() {
        assert_eq!(Code::Ok.as_u8(), 0);
        assert_eq!(Code::Error.as_u8(), 1);
        assert_eq!(Code::Misuse.as_u8(), 2);
        assert_eq!(Code::Conflict.as_u8(), 1);
        assert_eq!(Code::Locked.as_u8(), 3);
        assert_eq!(Code::Findings.as_u8(), 4);
    }
}
