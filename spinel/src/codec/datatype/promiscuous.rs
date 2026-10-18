use core::fmt;

/// MAC promiscuous mode of a radio.
///
/// Carried by [`Property::MacPromiscuousMode`](crate::Property) as a Spinel
/// `C` (unsigned 8-bit integer).
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum PromiscuousMode {
    /// Normal MAC filtering is in place.
    Off,

    /// The device passes up all MAC frames that match the network.
    Network,

    /// The device passes up all decoded MAC frames.
    Full,
}

impl PromiscuousMode {
    const PROMISCUOUS_OFF: u8 = 0;
    const PROMISCUOUS_NETWORK: u8 = 1;
    const PROMISCUOUS_FULL: u8 = 2;
}

impl fmt::Display for PromiscuousMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PromiscuousMode::Off => write!(f, "Off"),
            PromiscuousMode::Network => write!(f, "Network"),
            PromiscuousMode::Full => write!(f, "Full"),
        }
    }
}

impl TryFrom<u8> for PromiscuousMode {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            Self::PROMISCUOUS_OFF => Ok(Self::Off),
            Self::PROMISCUOUS_NETWORK => Ok(Self::Network),
            Self::PROMISCUOUS_FULL => Ok(Self::Full),
            _ => Err(()),
        }
    }
}

impl From<PromiscuousMode> for u8 {
    fn from(mode: PromiscuousMode) -> Self {
        match mode {
            PromiscuousMode::Off => PromiscuousMode::PROMISCUOUS_OFF,
            PromiscuousMode::Network => PromiscuousMode::PROMISCUOUS_NETWORK,
            PromiscuousMode::Full => PromiscuousMode::PROMISCUOUS_FULL,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_round_trip() {
        for (mode, value) in [
            (PromiscuousMode::Off, 0),
            (PromiscuousMode::Network, 1),
            (PromiscuousMode::Full, 2),
        ] {
            assert_eq!(u8::from(mode), value);
            assert_eq!(PromiscuousMode::try_from(value), Ok(mode));
        }
    }

    #[test]
    fn invalid_value_errors() {
        assert_eq!(PromiscuousMode::try_from(3), Err(()));
        assert_eq!(PromiscuousMode::try_from(0xFF), Err(()));
    }
}
