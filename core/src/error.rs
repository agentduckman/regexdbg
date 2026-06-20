#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileError {
    pub message: String,
    /// Byte offset into the pattern where compilation failed.
    pub offset: usize,
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (at offset {})", self.message, self.offset)
    }
}
