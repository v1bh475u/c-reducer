use slicer_core::context::CoverageData;
use slicer_core::pass::{Candidate, ReductionPass};
use slicer_parser::CParser;

use crate::clangd;

#[derive(Debug, Default)]
pub struct ArgumentRemovalPass;

impl ReductionPass for ArgumentRemovalPass {
    fn apply(&self, source: &str, _coverage: Option<&CoverageData>) -> Vec<Candidate> {
        let diags = clangd::get_diagnostics(source, &["-Wunused-parameter"]);

        let unused_param_names: Vec<String> = diags
            .iter()
            .filter(|d| d.code == "-Wunused-parameter")
            .filter_map(|d| extract_param_name(&d.message))
            .collect();

        if unused_param_names.is_empty() {
            return Vec::new();
        }

        let parser = CParser::default();
        let unit = match parser.parse(source) {
            Ok(u) => u,
            Err(_) => return Vec::new(),
        };

        let mut candidates = Vec::new();

        for func in unit.functions() {
            if func.name == "main" || !func.is_definition || func.parameters.is_empty() {
                continue;
            }

            for (param_idx, param) in func.parameters.iter().enumerate() {
                if param.name.is_empty() {
                    continue;
                }

                if unused_param_names.is_empty() {
                    let func_text = match func.range.extract(source) {
                        Some(t) => t,
                        None => continue,
                    };
                    if identifier_occurs(func_text, &param.name) {
                        continue;
                    }
                } else if !unused_param_names.contains(&param.name) {
                    continue;
                }

                let func_text = match func.range.extract(source) {
                    Some(t) => t,
                    None => continue,
                };

                let paren_open = match func_text.find('(') {
                    Some(p) => p,
                    None => continue,
                };
                let paren_close = match find_matching_paren(func_text, paren_open) {
                    Some(p) => p,
                    None => continue,
                };

                let param_text = &func_text[paren_open + 1..paren_close];
                let params: Vec<&str> = split_params(param_text);

                if param_idx >= params.len() {
                    continue;
                }

                let mut new_params: Vec<&str> = params.clone();
                new_params.remove(param_idx);
                let new_param_str = new_params.join(", ");

                let new_func_text = format!(
                    "{}({}){}",
                    &func_text[..paren_open],
                    new_param_str,
                    &func_text[paren_close + 1..]
                );

                let mut new_source = String::with_capacity(source.len());
                new_source.push_str(&source[..func.range.start]);
                new_source.push_str(&new_func_text);
                let mut rest = &source[func.range.end..];

                let call_pattern = format!("{}(", func.name);
                let mut rewritten_rest = String::new();
                while let Some(call_pos) = rest.find(&call_pattern) {
                    rewritten_rest.push_str(&rest[..call_pos]);
                    let call_start = call_pos + func.name.len();

                    if call_pos > 0 {
                        let prev_byte = rest.as_bytes()[call_pos - 1];
                        if prev_byte.is_ascii_alphanumeric() || prev_byte == b'_' {
                            rewritten_rest.push_str(&rest[..call_start + 1]);
                            rest = &rest[call_start + 1..];
                            continue;
                        }
                    }

                    let paren_start = call_start;
                    if let Some(close) = find_matching_paren(rest, paren_start) {
                        let args_text = &rest[paren_start + 1..close];
                        let args: Vec<&str> = split_params(args_text);

                        if param_idx < args.len() {
                            let mut new_args: Vec<&str> = args;
                            new_args.remove(param_idx);
                            rewritten_rest.push_str(&func.name);
                            rewritten_rest.push('(');
                            rewritten_rest.push_str(&new_args.join(", "));
                            rewritten_rest.push(')');
                            rest = &rest[close + 1..];
                        } else {
                            rewritten_rest.push_str(&rest[..close + 1]);
                            rest = &rest[close + 1..];
                        }
                    } else {
                        rewritten_rest.push_str(&rest[..call_start + 1]);
                        rest = &rest[call_start + 1..];
                    }
                }
                rewritten_rest.push_str(rest);

                let full_new = format!("{}{}", new_source, rewritten_rest);
                candidates.push(Candidate::new(0..source.len(), full_new));
            }
        }

        candidates
    }
}

fn extract_param_name(message: &str) -> Option<String> {
    let start = message.find('\'')?;
    let end = message[start + 1..].find('\'')?;
    Some(message[start + 1..start + 1 + end].to_string())
}

fn find_matching_paren(text: &str, open_pos: usize) -> Option<usize> {
    let mut depth = 0;
    for (i, b) in text[open_pos..].bytes().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open_pos + i);
                }
            },
            _ => {},
        }
    }
    None
}

fn split_params(text: &str) -> Vec<&str> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed == "void" {
        return Vec::new();
    }

    let mut parts = Vec::new();
    let mut depth = 0;
    let mut start = 0;

    for (i, b) in trimmed.bytes().enumerate() {
        match b {
            b'(' | b'[' => depth += 1,
            b')' | b']' => depth -= 1,
            b',' if depth == 0 => {
                parts.push(trimmed[start..i].trim());
                start = i + 1;
            },
            _ => {},
        }
    }
    parts.push(trimmed[start..].trim());
    parts
}

fn identifier_occurs(text: &str, ident: &str) -> bool {
    let bytes = text.as_bytes();
    let name = ident.as_bytes();
    if name.is_empty() {
        return false;
    }
    let mut i = 0;
    while i + name.len() <= bytes.len() {
        if &bytes[i..i + name.len()] == name {
            let before = if i == 0 { b' ' } else { bytes[i - 1] };
            let after = if i + name.len() >= bytes.len() {
                b' '
            } else {
                bytes[i + name.len()]
            };
            let before_ok = !(before.is_ascii_alphanumeric() || before == b'_');
            let after_ok = !(after.is_ascii_alphanumeric() || after == b'_');
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_unused_param() {
        let source =
            "int foo(int a, int b) {\n    return a;\n}\n\nint main() {\n    return foo(1, 2);\n}\n";
        let pass = ArgumentRemovalPass;
        let candidates = pass.apply(source, None);
        let has_b_removal = candidates
            .iter()
            .any(|c| !c.replacement.contains("int b") && c.replacement.contains("int a"));
        assert!(
            has_b_removal,
            "should produce candidate removing unused param b"
        );
    }

    #[test]
    fn test_call_site_updated() {
        let source =
            "int foo(int a, int b) {\n    return a;\n}\n\nint main() {\n    return foo(1, 2);\n}\n";
        let pass = ArgumentRemovalPass;
        let candidates = pass.apply(source, None);
        let updated = candidates.iter().find(|c| !c.replacement.contains("int b"));
        assert!(updated.is_some());
        let new_source = &updated.unwrap().replacement;
        assert!(
            new_source.contains("foo(1)"),
            "call site should be updated to foo(1), got: {}",
            new_source
        );
    }

    #[test]
    fn test_skip_all_used_params() {
        let source = "int add(int a, int b) {\n    return a + b;\n}\n\nint main() {\n    return add(1, 2);\n}\n";
        let pass = ArgumentRemovalPass;
        let candidates = pass.apply(source, None);
        assert!(candidates.is_empty(), "should not remove used parameters");
    }

    #[test]
    fn test_skip_main() {
        let source = "int main(int argc, char **argv) {\n    return 0;\n}\n";
        let pass = ArgumentRemovalPass;
        let candidates = pass.apply(source, None);
        assert!(candidates.is_empty(), "should never modify main");
    }

    #[test]
    fn test_split_params() {
        assert_eq!(split_params("int a, int b"), vec!["int a", "int b"]);
        assert_eq!(split_params("int a"), vec!["int a"]);
        assert!(split_params("").is_empty());
        assert!(split_params("void").is_empty());
        assert_eq!(
            split_params("int (*fp)(int, int), int b"),
            vec!["int (*fp)(int, int)", "int b"]
        );
    }

    #[test]
    fn test_find_matching_paren() {
        assert_eq!(find_matching_paren("(a, b)", 0), Some(5));
        assert_eq!(find_matching_paren("(a, (b, c))", 0), Some(10));
        assert_eq!(find_matching_paren("x(y)", 1), Some(3));
    }
}
