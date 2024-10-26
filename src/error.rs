use std::fmt;

#[derive(Debug)]
pub enum AtomAllocError {
    OutOfMemory,
    InvalidSize {
        requested: usize,
        max_allowed: usize,
    },
    InvalidAlignment {
        requested: usize,
        supported: usize,
    },
    ManagerError {
        message: String,
    },
    BlockError(BlockError),
}

#[derive(Debug)]
pub enum BlockError {
    OutOfBounds {
        offset: usize,
        len: usize,
        size: usize,
    },
    NotInitialized,
    InUse,
    InvalidGeneration {
        block: u64,
        expected: u64,
    },
}

impl fmt::Display for AtomAllocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfMemory => write!(f, "Out of memory"),
            Self::InvalidSize {
                requested,
                max_allowed,
            } => {
                write!(
                    f,
                    "Invalid size requested: {} (max: {})",
                    requested, max_allowed
                )
            }
            Self::InvalidAlignment {
                requested,
                supported,
            } => {
                write!(
                    f,
                    "Invalid alignment: {} (supported: {})",
                    requested, supported
                )
            }
            Self::ManagerError { message } => {
                write!(f, "Manager error: {}", message)
            }
            Self::BlockError(e) => write!(f, "Block error: {}", e),
        }
    }
}

impl fmt::Display for BlockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfBounds { offset, len, size } => {
                write!(
                    f,
                    "Out of bounds access: offset {} len {} exceeds size {}",
                    offset, len, size
                )
            }
            Self::NotInitialized => write!(f, "Block not initialized"),
            Self::InUse => write!(f, "Block already in use"),
            Self::InvalidGeneration { block, expected } => {
                write!(
                    f,
                    "Invalid block generation: {} (expected {})",
                    block, expected
                )
            }
        }
    }
}

impl std::error::Error for AtomAllocError {}
impl std::error::Error for BlockError {}

impl From<BlockError> for AtomAllocError {
    fn from(error: BlockError) -> Self {
        AtomAllocError::BlockError(error)
    }
}
