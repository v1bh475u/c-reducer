use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

use slicer_core::pipeline::Pipeline;
use slicer_passes::all_passes;
use slicer_validator::cycles::measure_cycles;
use slicer_validator::{CompilerConfig, LineCoverage, Oracle, OracleConfig, TimeoutConfig};

#[derive(Parser)]
#[command(name = "slicer", about = "Reduce C programs while preserving behavior")]
struct Cli {
    #[arg(short, long)]
    input: PathBuf,

    #[arg(short, long)]
    output: Option<PathBuf>,

    #[arg(long, default_value_t = 100)]
    iterations: u32,

    #[arg(short, long, default_value_t = 5)]
    timeout: u64,

    #[arg(long, default_value_t = 0)]
    total_timeout: u64,

    #[arg(long)]
    no_coverage: bool,

    #[arg(short = 'f', long = "flag")]
    flags: Vec<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let source = std::fs::read_to_string(&cli.input)?;
    let original_size = source.len();

    let mut compiler_config = CompilerConfig::default();
    for flag in &cli.flags {
        compiler_config = compiler_config.with_flag(flag);
    }

    let oracle_config = OracleConfig {
        compiler: compiler_config,
        timeout: TimeoutConfig::new(cli.timeout),
        check_coverage: !cli.no_coverage,
        ..Default::default()
    };

    let mut oracle = Oracle::new(oracle_config)?;
    oracle.initialize(&source)?;

    let coverage_data = oracle.original_coverage().map(|report| {
        let mut line_hits = std::collections::HashMap::new();
        for (&line, cov) in &report.lines {
            if let LineCoverage::Executed(count) = cov {
                line_hits.insert(line as u32, *count);
            }
        }
        slicer_core::CoverageData {
            line_hits,
            function_hits: std::collections::HashMap::new(),
        }
    });

    let passes = all_passes();
    let pipeline = Pipeline::new(passes, cli.iterations).with_total_timeout(cli.total_timeout);
    let reduced = pipeline.reduce(&source, coverage_data.as_ref(), &mut |candidate| {
        oracle.validate(candidate).unwrap_or(false)
    });

    let output_path = match &cli.output {
        Some(p) => p.clone(),
        None => cli.input.with_extension("reduced.c"),
    };
    std::fs::write(&output_path, &reduced)?;
    let reduced_size = reduced.len();

    let original_cycles = measure_cycles(&cli.input);
    let reduced_cycles = measure_cycles(&output_path);

    println!("Input:    {}", cli.input.display());
    println!("Output:   {}", output_path.display());
    println!(
        "Size:     {} -> {} bytes ({:.1}% reduction)",
        original_size,
        reduced_size,
        (1.0 - reduced_size as f64 / original_size as f64) * 100.0
    );
    if let (Some(orig), Some(red)) = (original_cycles, reduced_cycles) {
        println!(
            "Cycles:   {} -> {} ({:.1}% reduction)",
            orig,
            red,
            (1.0 - red as f64 / orig as f64) * 100.0
        );
    }

    Ok(())
}
