//! The device a microphone is heard through, as it's opened.

/// An input device, and what it gives its audio as, before it's converted to the rate its caller asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputDevice {
    /// What the system calls it, such as `MacBook Pro Microphone`, if it says.
    pub name: Option<String>,
    pub sample_rate: u32,
    pub channels: u16,
}
