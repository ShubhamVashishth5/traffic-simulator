//! Model code must not contain behavioural constants (CLAUDE.md rule 1):
//! every number comes from the register. Allowed: 0, 1, 2, 0.5 and 4 used as
//! arithmetic, test modules, and lines marked `allow-literal` with a reason.

use std::path::Path;

const ALLOWED: [&str; 8] = ["0.0", "1.0", "2.0", "0.5", "4.0", "0.", "1.", "2."];

fn float_literals(line: &str) -> Vec<String> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        let prev_ident = i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
        if c.is_ascii_digit() && !prev_ident {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'_') {
                i += 1;
            }
            let mut is_float = false;
            if i + 1 < bytes.len() && bytes[i] == b'.' && bytes[i + 1].is_ascii_digit() {
                is_float = true;
                i += 1;
                while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'_') {
                    i += 1;
                }
            }
            if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
                is_float = true;
            }
            if is_float {
                out.push(line[start..i].to_string());
            }
        } else {
            i += 1;
        }
    }
    out
}

#[test]
fn model_code_has_no_magic_numbers() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/model");
    let mut bad = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        let body = text.split("#[cfg(test)]").next().unwrap();
        for (n, line) in body.lines().enumerate() {
            let code = line.split("//").next().unwrap();
            if line.contains("allow-literal") {
                continue;
            }
            for lit in float_literals(code) {
                if !ALLOWED.contains(&lit.as_str()) {
                    bad.push(format!(
                        "{}:{}: {lit}",
                        path.file_name().unwrap().to_string_lossy(),
                        n + 1
                    ));
                }
            }
        }
    }
    assert!(
        bad.is_empty(),
        "float literals in model code (move them to the register): {bad:?}"
    );
}

#[test]
fn scanner_finds_literals() {
    assert_eq!(
        float_literals("let x = 0.615 * v + 1.5e-3;"),
        vec!["0.615", "1.5"]
    );
    assert!(float_literals("let t2 = a[3]; x1.0").is_empty());
    assert_eq!(float_literals("y = 2.0 * z"), vec!["2.0"]);
}
