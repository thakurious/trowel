//! DigUpKit port.

pub mod crawler;
pub mod database;
pub mod embedder;
pub mod estimator;
pub mod extract;
pub mod folder_selection;
pub mod git_files;
pub mod index_store;
pub mod indexer;
pub mod model;
pub mod model_files;
pub mod prompts;
pub mod rules;
pub mod search_text;
pub mod searcher;
pub mod vectors;

pub use crawler::{CrawlReport, Crawler};
pub use database::{Database, DbError, SqlValue};
pub use embedder::{Embedder, Embedding, FakeEmbedder, SharedEmbedder, WorkerInfo};
pub use estimator::{CodeIndexSize, Costs, Estimate, Estimator};
pub use extract::{
    AudioExtractor, CodeExtractor, CodeLookup, DocExtractor, ExtractError, Extractor,
    ImageExtractor, MediaInfo, Ocr, PdfExtractor, VideoExtractor,
};
pub use folder_selection::{FolderSelection, Mark};
pub use index_store::{FileRecord, IndexCounts, IndexStore};
pub use indexer::{IndexEvent, Indexer, IndexerError, Step, SyncSummary};
pub use model::{Candidate, CodeQuery, FileKind, IndexOptions, Modality, SegmentKind};
pub use model_files::ModelFiles;
pub use prompts::Prompts;
pub use rules::{Classification, Rules};
pub use search_text::{Landmark, PageTextProblem, SearchText};
pub use searcher::{SearchHit, Searcher};
pub use vectors::{VectorStorage, Vectors, DIMENSION};
