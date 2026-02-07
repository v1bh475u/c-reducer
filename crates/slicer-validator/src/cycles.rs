use std::path::Path;
use std::process::Command;
use regex::Regex;


pub fn measure_cycles(binary_path: &Path) -> Option<u64> {
    let output = Command::new("perf")
        .args(["stat", "-e", "cpu-cycles", "-x", ","])
        .arg(binary_path)
        .output()
        .ok()?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    let re = Regex::new(r"(\d+),,cpu-cycles").ok()?;
    re.captures(&stderr)
        .and_then(|caps| caps[1].parse::<u64>().ok())
}

pub fn pad_for_cycles(
    reduced_source: &str,
    original_cycles: u64,
    reduced_cycles: u64,
    tolerance_pct: f64,
) -> Option<String> {
    let delta_pct = ((reduced_cycles as f64 - original_cycles as f64) / original_cycles as f64) * 100.0;

    if delta_pct.abs() <= tolerance_pct {
        return None;
    }

    let ratio = if original_cycles > reduced_cycles {
        (original_cycles as f64 / reduced_cycles as f64) - 1.0
    } else {
        return None;
    };

    let loop_iters = (ratio * 10000.0) as u64;
    if loop_iters == 0 {
        return None;
    }

    let padding = format!(
        "\n    {{ volatile int _pad = 0; for (long _i = 0; _i < {}; _i++) _pad += (int)_i; }}\n",
        loop_iters
    );

    let mut result = reduced_source.to_string();
    if let Some(main_pos) = result.find("int main(") {
        if let Some(brace_pos) = result[main_pos..].find('{') {
            let insert_pos = main_pos + brace_pos + 1;
            result.insert_str(insert_pos, &padding);
            return Some(result);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pad_for_cycles_no_change_needed() {
        let result = pad_for_cycles("int main() { return 0; }", 1000, 980, 5.0);
        assert!(result.is_none());
    }

    #[test]
    fn test_pad_for_cycles_inserts_loop() {
        let source = "int main() {\n    return 0;\n}\n";
        let result = pad_for_cycles(source, 10000, 5000, 5.0);
        assert!(result.is_some());
        let padded = result.unwrap();
        assert!(padded.contains("volatile int _pad"));
        assert!(padded.contains("return 0;"));
    }

    #[test]
    fn test_pad_for_cycles_reduced_more_than_original() {
        let result = pad_for_cycles("int main() { return 0; }", 1000, 2000, 5.0);
        assert!(result.is_none());
    }
}
