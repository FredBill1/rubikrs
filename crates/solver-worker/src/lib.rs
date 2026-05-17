#![forbid(unsafe_code)]

//! Future home of the GitHub Pages-compatible solver worker pool.
//!
//! Each worker will run as its own single-threaded wasm module and communicate
//! with the Bevy runtime through structured-clone messages. This crate exists
//! early so the workspace encodes that constraint before the actual solver
//! protocol is implemented.

pub const WORKER_MODEL: &str = "single-threaded wasm worker pool";

#[cfg(test)]
mod tests {
    use super::WORKER_MODEL;

    #[test]
    fn documents_the_expected_worker_model() {
        assert!(WORKER_MODEL.contains("worker pool"));
        assert!(!WORKER_MODEL.contains("SharedArrayBuffer"));
    }
}
