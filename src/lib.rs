//! RustNeverSleeps - Ollama Performance Benchmarking Suite
//!
//! This library provides types and utilities for benchmarking Ollama models,
//! specifically custom CUAD Atticus fine-tuned models for contract analysis.

#[cfg(test)]
mod tests {
    // Basic functionality tests for the ollama-ui library

    #[test]
    fn test_basic_functionality() {
        // Basic test to ensure the project compiles
        assert_eq!(2 + 2, 4);
    }

    #[test]
    fn test_model_types() {
        // Test that we can create model type representations
        let q4_model = "qwen-contract:latest";
        let q5_model = "qwen-contract-q5:latest";

        assert!(q4_model.contains("contract"));
        assert!(q5_model.contains("contract"));
        assert!(q5_model.contains("q5"));
    }

    #[test]
    fn test_performance_tracking() {
        // Test performance measurement concepts
        use std::time::Instant;

        let start = Instant::now();
        std::thread::sleep(std::time::Duration::from_millis(1));
        let duration = start.elapsed();

        assert!(duration.as_millis() >= 1);
    }
}
