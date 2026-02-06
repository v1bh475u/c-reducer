//! Pipeline configuration.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Reduction policy determines the aggressiveness of code reduction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Policy {
    /// Maximum reduction
    #[default]
    Aggressive,
    /// More cautious reduction
    Conservative,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilerConfig {
    #[serde(default = "default_compiler")]
    pub name: String,
    #[serde(default)]
    pub flags: Vec<String>,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_compiler() -> String {
    "gcc".to_string()
}

fn default_timeout() -> u64 {
    10
}

impl Default for CompilerConfig {
    fn default() -> Self {
        Self {
            name: default_compiler(),
            flags: vec!["-O0".to_string(), "-w".to_string()],
            timeout_secs: default_timeout(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_coverage_timeout")]
    pub timeout_secs: u64,
}

fn default_true() -> bool {
    true
}

fn default_coverage_timeout() -> u64 {
    30
}

impl Default for CoverageConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            timeout_secs: 30,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    #[serde(default)]
    pub policy: Policy,
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,
    #[serde(default)]
    pub convergence_threshold: u32,
    #[serde(default)]
    pub total_timeout_secs: u64,
    #[serde(default)]
    pub compiler: CompilerConfig,
    #[serde(default)]
    pub coverage: CoverageConfig,
    #[serde(default)]
    pub enabled_passes: Option<Vec<String>>,
    #[serde(default)]
    pub disabled_passes: Vec<String>,
    #[serde(default)]
    pub verbose: bool,
}

fn default_max_iterations() -> u32 {
    20
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            policy: Policy::Aggressive,
            max_iterations: 20,
            convergence_threshold: 0,
            total_timeout_secs: 0, // 0 = no timeout
            compiler: CompilerConfig::default(),
            coverage: CoverageConfig::default(),
            enabled_passes: None,
            disabled_passes: Vec::new(),
            verbose: false,
        }
    }
}

impl PipelineConfig {
    pub fn builder() -> PipelineConfigBuilder {
        PipelineConfigBuilder::default()
    }

    /// Load configuration from a TOML file.
    pub fn from_file(path: &PathBuf) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Self = toml::from_str(&content)?;
        Ok(config)
    }

    /// Save configuration to a TOML file.
    pub fn to_file(&self, path: &PathBuf) -> anyhow::Result<()> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Check if a pass is enabled.
    pub fn is_pass_enabled(&self, name: &str) -> bool {
        if self.disabled_passes.iter().any(|p| p == name) {
            return false;
        }
        match &self.enabled_passes {
            Some(enabled) => enabled.iter().any(|p| p == name),
            None => true,
        }
    }
}

#[derive(Debug, Default)]
pub struct PipelineConfigBuilder {
    config: PipelineConfig,
}

impl PipelineConfigBuilder {
    pub fn policy(mut self, policy: Policy) -> Self {
        self.config.policy = policy;
        self
    }

    pub fn max_iterations(mut self, n: u32) -> Self {
        self.config.max_iterations = n;
        self
    }

    pub fn compiler(mut self, compiler: CompilerConfig) -> Self {
        self.config.compiler = compiler;
        self
    }

    pub fn disabled_passes(mut self, passes: Vec<String>) -> Self {
        self.config.disabled_passes = passes;
        self
    }

    pub fn total_timeout_secs(mut self, secs: u64) -> Self {
        self.config.total_timeout_secs = secs;
        self
    }

    pub fn build(self) -> PipelineConfig {
        self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = PipelineConfig::default();
        assert_eq!(config.policy, Policy::Aggressive);
        assert_eq!(config.max_iterations, 20);
        assert!(config.is_pass_enabled("dead_code"));
    }

    #[test]
    fn test_builder() {
        let config = PipelineConfig::builder()
            .policy(Policy::Conservative)
            .max_iterations(10)
            .build();

        assert_eq!(config.policy, Policy::Conservative);
        assert_eq!(config.max_iterations, 10);
    }

    #[test]
    fn test_pass_filtering() {
        let config = PipelineConfig::builder()
            .disabled_passes(vec!["delta_debug".to_string()])
            .build();

        assert!(config.is_pass_enabled("dead_code"));
        assert!(!config.is_pass_enabled("delta_debug"));
    }
}
