//! Reduction context - shared state for passes.

use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct CoverageData {
    pub line_hits: HashMap<u32, u64>,
    pub function_hits: HashMap<String, u64>,
}

impl CoverageData {
    pub fn dead_lines(&self) -> Vec<u32> {
        self.line_hits
            .iter()
            .filter(|(_, &hits)| hits == 0)
            .map(|(&line, _)| line)
            .collect()
    }

    pub fn dead_functions(&self) -> Vec<&str> {
        self.function_hits
            .iter()
            .filter(|(_, &hits)| hits == 0)
            .map(|(name, _)| name.as_str())
            .collect()
    }

    pub fn is_line_executed(&self, line: u32) -> bool {
        self.line_hits.get(&line).copied().unwrap_or(0) > 0
    }

    pub fn is_function_executed(&self, name: &str) -> bool {
        self.function_hits.get(name).copied().unwrap_or(0) > 0
    }
}

/// Trait for validation oracles.
///
/// An oracle determines whether a reduced program is valid by checking
/// that it compiles and produces the expected behavior.
pub trait ValidationOracle: Send + Sync {
    /// Check if a source program is valid (compiles and produces expected output).
    fn is_valid(&mut self, source: &str) -> bool;
}

/// A simple oracle that uses a validation function.
///
/// This allows wrapping any function that can validate source code.
pub struct FnOracle<F>
where
    F: FnMut(&str) -> bool + Send + Sync,
{
    validate_fn: F,
}

impl<F> FnOracle<F>
where
    F: FnMut(&str) -> bool + Send + Sync,
{
    pub fn new(validate_fn: F) -> Self {
        Self { validate_fn }
    }
}

impl<F> ValidationOracle for FnOracle<F>
where
    F: FnMut(&str) -> bool + Send + Sync,
{
    fn is_valid(&mut self, source: &str) -> bool {
        (self.validate_fn)(source)
    }
}

/// A simple oracle that always returns true.
///
/// Useful for testing or when validation is handled externally.
pub struct AlwaysValidOracle;

impl ValidationOracle for AlwaysValidOracle {
    fn is_valid(&mut self, _source: &str) -> bool {
        true
    }
}

/// Context passed to reduction passes.
pub struct Context {
    oracle: Box<dyn ValidationOracle>,
    pub coverage: Option<CoverageData>,
    pub iteration: u32,
    pub validation_cache: HashMap<u64, bool>,
}

impl Context {
    pub fn new(oracle: Box<dyn ValidationOracle>) -> Self {
        Self {
            oracle,
            coverage: None,
            iteration: 0,
            validation_cache: HashMap::new(),
        }
    }

    pub fn with_validator<F>(validate_fn: F) -> Self
    where
        F: FnMut(&str) -> bool + Send + Sync + 'static,
    {
        Self::new(Box::new(FnOracle::new(validate_fn)))
    }

    pub fn with_coverage(mut self, coverage: CoverageData) -> Self {
        self.coverage = Some(coverage);
        self
    }

    pub fn is_valid(&mut self, source: &str) -> bool {
        let hash = self.hash_source(source);

        // Check cache
        if let Some(&valid) = self.validation_cache.get(&hash) {
            return valid;
        }

        // Validate
        let valid = self.oracle.is_valid(source);

        // Cache result
        self.validation_cache.insert(hash, valid);

        valid
    }

    fn hash_source(&self, source: &str) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        source.hash(&mut hasher);
        hasher.finish()
    }
}
