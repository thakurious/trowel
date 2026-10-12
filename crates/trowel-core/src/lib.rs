//! DigUpKit port. Phase 0 only carries prompts and model-file locations used by
//! the vector-parity harness.

pub mod model_files;
pub mod prompts;

pub use model_files::ModelFiles;
pub use prompts::Prompts;
