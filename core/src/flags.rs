#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Flags {
    pub caseless:  bool,
    pub multiline: bool,
    pub dotall:    bool,
    pub extended:  bool,
    pub ungreedy:  bool,
    /// Enables UTF-8 + UCP semantics. OFF by default — input is raw bytes.
    pub utf_ucp:   bool,
}
