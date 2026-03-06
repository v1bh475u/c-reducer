use std::io::{Read, Write};
use std::process::{Command, Stdio};

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub line: usize,
    pub code: String,
    #[allow(dead_code)]
    pub message: String,
}

fn make_lsp_message(body: &serde_json::Value) -> Option<Vec<u8>> {
    let content = serde_json::to_string(body).ok()?;
    Some(format!("Content-Length: {}\r\n\r\n{}", content.len(), content).into_bytes())
}

pub fn get_diagnostics(source: &str, extra_flags: &[&str]) -> Vec<Diagnostic> {
    let temp_dir = match tempfile::TempDir::new() {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };

    let source_path = temp_dir.path().join("input.c");
    if std::fs::write(&source_path, source).is_err() {
        return Vec::new();
    }

    let mut compile_cmd = "cc".to_string();
    for flag in extra_flags {
        compile_cmd.push(' ');
        compile_cmd.push_str(flag);
    }
    compile_cmd.push_str(&format!(" -c {}", source_path.display()));

    let compile_commands = serde_json::json!([{
        "directory": temp_dir.path().to_string_lossy(),
        "file": source_path.to_string_lossy(),
        "command": compile_cmd
    }]);

    let compile_commands_json = match serde_json::to_string(&compile_commands) {
        Ok(json) => json,
        Err(_) => return Vec::new(),
    };

    let cdb_path = temp_dir.path().join("compile_commands.json");
    if std::fs::write(&cdb_path, compile_commands_json).is_err() {
        return Vec::new();
    }

    let compile_commands_dir = temp_dir.path().to_string_lossy().to_string();
    let uri = format!("file://{}", source_path.display());

    let mut child = match Command::new("clangd")
        .arg("--log=error")
        .arg(format!("--compile-commands-dir={}", compile_commands_dir))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let stdin = match child.stdin.as_mut() {
        Some(s) => s,
        None => return Vec::new(),
    };

    let init = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "processId": std::process::id(),
            "rootUri": format!("file://{}", temp_dir.path().display()),
            "capabilities": {
                "textDocument": {
                    "publishDiagnostics": {"relatedInformation": true}
                }
            }
        }
    });

    let initialized = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "initialized",
        "params": {}
    });

    let did_open = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": uri,
                "languageId": "c",
                "version": 1,
                "text": source
            }
        }
    });

    if let Some(msg) = make_lsp_message(&init) {
        let _ = stdin.write_all(&msg);
    }
    if let Some(msg) = make_lsp_message(&initialized) {
        let _ = stdin.write_all(&msg);
    }
    if let Some(msg) = make_lsp_message(&did_open) {
        let _ = stdin.write_all(&msg);
    }
    let _ = stdin.flush();

    std::thread::sleep(std::time::Duration::from_secs(3));

    let shutdown = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "shutdown",
        "params": null
    });
    let exit = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "exit",
        "params": null
    });

    if let Some(msg) = make_lsp_message(&shutdown) {
        let _ = stdin.write_all(&msg);
    }
    if let Some(msg) = make_lsp_message(&exit) {
        let _ = stdin.write_all(&msg);
    }
    let _ = stdin.flush();

    let stdin_handle = child.stdin.take();
    drop(stdin_handle);

    let mut stdout_data = Vec::new();
    if let Some(mut stdout) = child.stdout.take() {
        let _ = stdout.read_to_end(&mut stdout_data);
    }
    let _ = child.wait();

    parse_diagnostics(&stdout_data)
}

fn parse_diagnostics(stdout_data: &[u8]) -> Vec<Diagnostic> {
    let stdout_str = String::from_utf8_lossy(stdout_data);
    let mut diagnostics = Vec::new();

    for part in stdout_str.split("Content-Length: ").skip(1) {
        if let Some(header_end) = part.find("\r\n\r\n") {
            let body = &part[header_end + 4..];
            if let Ok(msg) = serde_json::from_str::<serde_json::Value>(body) {
                if msg.get("method").and_then(|m| m.as_str())
                    == Some("textDocument/publishDiagnostics")
                {
                    if let Some(diags) = msg["params"]["diagnostics"].as_array() {
                        for d in diags {
                            let code = d
                                .get("code")
                                .and_then(|c| c.as_str())
                                .unwrap_or("")
                                .to_string();
                            let message = d
                                .get("message")
                                .and_then(|m| m.as_str())
                                .unwrap_or("")
                                .to_string();
                            if let Some(line) = d["range"]["start"]["line"].as_u64() {
                                diagnostics.push(Diagnostic {
                                    line: line as usize,
                                    code,
                                    message,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    diagnostics
}

pub fn lines_with_code(
    source: &str,
    code: &str,
    extra_flags: &[&str],
) -> std::collections::HashSet<usize> {
    get_diagnostics(source, extra_flags)
        .into_iter()
        .filter(|d| d.code == code)
        .map(|d| d.line)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_diagnostics_unused_include() {
        let source = "#include <stdio.h>\n#include <stdlib.h>\n\nint main() {\n    printf(\"hi\");\n    return 0;\n}\n";
        let diags = get_diagnostics(source, &[]);
        let unused: Vec<_> = diags
            .iter()
            .filter(|d| d.code == "unused-includes")
            .collect();
        assert!(!unused.is_empty(), "should detect unused stdlib.h");
        assert!(
            unused.iter().all(|d| d.line != 0),
            "stdio.h should not be flagged"
        );
    }

    #[test]
    fn test_get_diagnostics_unused_variable() {
        let source = "int main() {\n    int x = 1;\n    int y = 2;\n    return x;\n}\n";
        let diags = get_diagnostics(source, &["-Wall"]);
        let unused: Vec<_> = diags
            .iter()
            .filter(|d| d.code == "-Wunused-variable")
            .collect();
        assert!(
            !unused.is_empty(),
            "should detect unused variable y, got: {:?}",
            diags
                .iter()
                .map(|d| format!("{}:{}", d.code, d.message))
                .collect::<Vec<_>>()
        );
    }
}
