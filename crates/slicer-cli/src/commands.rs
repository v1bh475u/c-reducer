//! CLI command implementations.

use anyhow::{Context, Result};
use clap::Args;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{info, warn};

use slicer_core::config::{PipelineConfig, Policy};
use slicer_core::pipeline::Pipeline;
use slicer_parser::CParser;
use slicer_passes::all_passes;
use slicer_validator::{CompilerConfig, Oracle, OracleConfig, TimeoutConfig};

/// Arguments for the reduce command.
#[derive(Args)]
pub struct ReduceArgs {
    /// Input C source file
    #[arg(short, long)]
    pub input: PathBuf,

    /// Output file (default: input.reduced.c)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Configuration file (TOML format)
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    /// Reduction policy (aggressive, conservative)
    #[arg(short, long, default_value = "aggressive")]
    pub policy: String,

    /// Maximum iterations
    #[arg(long, default_value = "100")]
    pub max_iterations: u32,

    /// Timeout for each validation (seconds)
    #[arg(long, default_value = "5")]
    pub timeout: u64,

    /// Total timeout for the entire reduction (seconds, 0 = no limit)
    #[arg(long, default_value = "60")]
    pub total_timeout: u64,

    /// Disable coverage checking
    #[arg(long)]
    pub no_coverage: bool,

    /// Passes to enable (comma-separated, or 'all')
    #[arg(long, default_value = "all")]
    pub passes: String,

    /// Additional include paths for the C compiler (can be specified multiple times)
    #[arg(short = 'I', long = "include")]
    pub include_paths: Vec<PathBuf>,

    /// Additional compiler flags
    #[arg(long = "cflags")]
    pub cflags: Option<String>,
}

/// Arguments for the validate command.
#[derive(Args)]
pub struct ValidateArgs {
    /// Original C source file
    #[arg(short, long)]
    pub original: PathBuf,

    /// Reduced C source file
    #[arg(short, long)]
    pub reduced: PathBuf,

    /// Check coverage preservation
    #[arg(long)]
    pub check_coverage: bool,
}

/// Arguments for the parse command.
#[derive(Args)]
pub struct ParseArgs {
    /// Input C source file
    #[arg(short, long)]
    pub input: PathBuf,

    /// Show AST summary
    #[arg(long)]
    pub ast: bool,

    /// Show functions
    #[arg(long)]
    pub functions: bool,

    /// Show includes
    #[arg(long)]
    pub includes: bool,
}

/// Arguments for the init command.
#[derive(Args)]
pub struct InitArgs {
    /// Output configuration file
    #[arg(short, long, default_value = "slicer.toml")]
    pub output: PathBuf,
}

/// Execute the reduce command.
pub fn reduce(args: ReduceArgs, format: &str) -> Result<()> {
    let start = Instant::now();

    // Read input file
    let source = std::fs::read_to_string(&args.input)
        .with_context(|| format!("Failed to read input file: {:?}", args.input))?;

    info!("Input file: {:?} ({} bytes)", args.input, source.len());

    // Load or create config
    let config = if let Some(config_path) = &args.config {
        let config_str = std::fs::read_to_string(config_path)
            .with_context(|| format!("Failed to read config file: {:?}", config_path))?;
        toml::from_str(&config_str)?
    } else {
        let policy = match args.policy.to_lowercase().as_str() {
            "aggressive" => Policy::Aggressive,
            "conservative" => Policy::Conservative,
            _ => {
                warn!("Unknown policy '{}', using aggressive", args.policy);
                Policy::Aggressive
            }
        };

        PipelineConfig::builder()
            .policy(policy)
            .max_iterations(args.max_iterations)
            .total_timeout_secs(args.total_timeout)
            .build()
    };

    // Create pipeline with passes
    let mut pipeline = Pipeline::new(config)?;

    let passes = all_passes();
    for pass in passes {
        // Filter passes if specified
        if args.passes != "all" {
            let enabled: Vec<&str> = args.passes.split(',').collect();
            if !enabled.contains(&pass.name()) {
                continue;
            }
        }
        pipeline.register_pass(pass);
    }

    info!("Enabled passes: {:?}", pipeline.enabled_passes());

    // Build compiler configuration with include paths
    let mut compiler_flags = CompilerConfig::default().flags;
    if let Some(cflags) = &args.cflags {
        compiler_flags.extend(cflags.split_whitespace().map(String::from));
    }
    let compiler_config = CompilerConfig {
        include_paths: args.include_paths.clone(),
        flags: compiler_flags,
        ..Default::default()
    };

    // Create the real validation oracle
    let oracle_config = OracleConfig {
        compiler: compiler_config,
        timeout: TimeoutConfig {
            execution_timeout: Duration::from_secs(args.timeout),
            total_timeout: Duration::from_secs(args.timeout * 10),
        },
        check_coverage: !args.no_coverage,
        ..Default::default()
    };

    let mut oracle =
        Oracle::new(oracle_config).with_context(|| "Failed to create validation oracle")?;

    // Initialize oracle with the original program
    oracle
        .initialize(&source)
        .with_context(|| "Failed to initialize oracle with original program")?;

    info!(
        "Oracle initialized. Expected exit code: {:?}",
        oracle.expected_exit_code()
    );

    // Extract coverage data from the oracle for use by coverage-based passes
    let coverage_data = if !args.no_coverage {
        oracle.original_coverage().map(|report| {
            let mut line_hits = std::collections::HashMap::new();
            for (&line, cov) in &report.lines {
                match cov {
                    slicer_validator::LineCoverage::Executed(n) => {
                        line_hits.insert(line as u32, *n);
                    }
                    slicer_validator::LineCoverage::NotExecuted => {
                        line_hits.insert(line as u32, 0);
                    }
                    slicer_validator::LineCoverage::NonExecutable => {}
                }
            }
            info!(
                "Coverage data: {} executable lines ({} executed)",
                line_hits.len(),
                line_hits.values().filter(|&&h| h > 0).count()
            );
            slicer_core::context::CoverageData {
                line_hits,
                function_hits: std::collections::HashMap::new(),
            }
        })
    } else {
        None
    };

    // Wrap oracle in Arc<Mutex> so it can be shared with the validation closure
    let oracle = Arc::new(Mutex::new(oracle));

    // Create context with the real validator
    let oracle_clone = Arc::clone(&oracle);
    let mut ctx = slicer_core::Context::with_validator(move |source: &str| {
        let mut oracle = oracle_clone.lock().unwrap();
        oracle.validate(source).unwrap_or(false)
    });

    // Inject coverage data so coverage-based passes (e.g. dead_code) can use it
    if let Some(cov) = coverage_data {
        ctx = ctx.with_coverage(cov);
    }

    // Run the pipeline
    let result = pipeline.reduce(&source, &mut ctx)?;

    let elapsed = start.elapsed();

    // Print oracle stats
    {
        let oracle = oracle.lock().unwrap();
        let stats = oracle.stats();
        info!(
            "Oracle stats: {} total, {} successful, {} compile failures, {} timeouts",
            stats.total_validations, stats.successful, stats.compile_failures, stats.timeouts
        );
    }

    // Write output
    let output_path = args.output.unwrap_or_else(|| {
        let stem = args.input.file_stem().unwrap().to_string_lossy();
        args.input.with_file_name(format!("{}.reduced.c", stem))
    });

    std::fs::write(&output_path, &result.final_source)?;

    // Print results
    if format == "json" {
        let json = serde_json::json!({
            "input_file": args.input,
            "output_file": output_path,
            "original_lines": result.original_lines,
            "final_lines": result.final_lines,
            "reduction_percent": result.reduction_percent,
            "iterations": result.iterations,
            "converged": result.converged,
            "duration_secs": elapsed.as_secs_f64(),
        });
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else {
        println!("Reduction complete!");
        println!(
            "  Input:  {:?} ({} lines)",
            args.input, result.original_lines
        );
        println!("  Output: {:?} ({} lines)", output_path, result.final_lines);
        println!("  Reduction: {:.1}%", result.reduction_percent);
        println!("  Iterations: {}", result.iterations);
        println!("  Converged: {}", result.converged);
        println!("  Time: {:.2}s", elapsed.as_secs_f64());
    }

    Ok(())
}

/// Execute the validate command.
pub fn validate(args: ValidateArgs) -> Result<()> {
    let original = std::fs::read_to_string(&args.original)
        .with_context(|| format!("Failed to read original file: {:?}", args.original))?;

    let reduced = std::fs::read_to_string(&args.reduced)
        .with_context(|| format!("Failed to read reduced file: {:?}", args.reduced))?;

    // Create a real oracle for validation
    let oracle_config = OracleConfig::default();
    let mut oracle =
        Oracle::new(oracle_config).with_context(|| "Failed to create validation oracle")?;

    // Initialize oracle with the original program
    oracle
        .initialize(&original)
        .with_context(|| "Failed to initialize oracle with original program")?;

    println!(
        "Original program: exit code {:?}",
        oracle.expected_exit_code()
    );

    let is_valid = oracle
        .validate(&reduced)
        .with_context(|| "Failed to validate reduced program")?;

    if is_valid {
        println!("✓ Reduced program is a valid reduction");
        println!("  - Compiles successfully");
        println!("  - Produces same behavior as original");

        // Parse both to compare
        let parser = CParser::default();

        if let (Ok(orig_unit), Ok(red_unit)) = (parser.parse(&original), parser.parse(&reduced)) {
            let orig_funcs = orig_unit.functions();
            let red_funcs = red_unit.functions();

            println!("  Original: {} functions", orig_funcs.len());
            println!("  Reduced:  {} functions", red_funcs.len());
        }

        Ok(())
    } else {
        println!("✗ Reduced program is NOT a valid reduction");
        println!("  - The reduced program does not preserve the original behavior");
        std::process::exit(1);
    }
}

/// Execute the parse command.
pub fn parse(args: ParseArgs) -> Result<()> {
    let source = std::fs::read_to_string(&args.input)
        .with_context(|| format!("Failed to read input file: {:?}", args.input))?;

    let parser = CParser::default();
    let unit = parser.parse(&source)?;

    println!("File: {:?}", args.input);
    println!(
        "Size: {} bytes, {} lines",
        source.len(),
        source.lines().count()
    );
    println!("Has errors: {}", unit.has_errors());

    if unit.has_errors() {
        println!();
        println!("Diagnostics:");
        for diag in unit.diagnostics() {
            println!("  {}", diag);
        }
    }

    if args.functions || (!args.ast && !args.includes) {
        println!();
        println!("Functions:");
        for func in unit.functions() {
            println!(
                "  {} ({}) at {}..{}",
                func.name, func.return_type.name, func.range.start, func.range.end
            );

            if !func.parameters.is_empty() {
                print!("    params: ");
                for (i, param) in func.parameters.iter().enumerate() {
                    if i > 0 {
                        print!(", ");
                    }
                    print!("{}: {}", param.name, param.type_info.name);
                }
                println!();
            }
        }
    }

    if args.includes {
        println!();
        println!("Includes:");
        for include_range in unit.includes() {
            if let Some(text) = include_range.extract(&source) {
                println!("  {}", text.trim());
            }
        }
    }

    if args.ast {
        println!();
        println!("Declarations:");
        for decl in unit.declarations() {
            println!(
                "  {} ({:?}) at {}..{}",
                decl.name, decl.kind, decl.range.start, decl.range.end
            );
        }

        println!();
        println!("Function calls:");
        for call in unit.function_calls() {
            println!("  {}", call);
        }

        println!();
        println!("Used identifiers:");
        let mut ids: Vec<_> = unit.used_identifiers().iter().collect();
        ids.sort();
        for id in ids.iter().take(20) {
            println!("  {}", id);
        }
        if ids.len() > 20 {
            println!("  ... and {} more", ids.len() - 20);
        }
    }

    Ok(())
}

/// Execute the init command.
pub fn init(args: InitArgs) -> Result<()> {
    let config = PipelineConfig::default();
    let toml = toml::to_string_pretty(&config)?;

    std::fs::write(&args.output, toml)?;
    println!("Created configuration file: {:?}", args.output);

    Ok(())
}

/// List available passes.
pub fn list_passes() -> Result<()> {
    let passes = all_passes();

    println!("Available reduction passes:");
    println!();

    for pass in &passes {
        println!("  {:20} (priority: {})", pass.name(), pass.priority());
    }

    println!();
    println!("Passes run in order of priority (higher first).");
    println!("Use --passes=pass1,pass2 to enable specific passes.");

    Ok(())
}
