//! Simulation code takes transcendental functions from the `libm` crate,
//! never from the float methods of the standard library (ADR-0003). Those
//! call the platform's C maths library (glibc, musl or the Windows CRT),
//! whose results differ in the last bit, so the same scenario and seed would
//! give different output on different builds. `sqrt` is correctly rounded
//! everywhere and `powi` comes from Rust's own compiler builtins, so both
//! are allowed.
//!
//! Scans `src/` of every crate whose code decides what the simulation does or
//! writes (marg-core, marg-net, marg-params, marg-io), outside comments and
//! outside `#[cfg(test)]` items.

use std::path::{Path, PathBuf};

/// Float methods that go to the platform's maths library.
const PLATFORM: [&str; 26] = [
    "sin", "cos", "tan", "sin_cos", "asin", "acos", "atan", "atan2", "sinh", "cosh", "tanh",
    "asinh", "acosh", "atanh", "hypot", "exp", "exp2", "exp_m1", "ln", "ln_1p", "log", "log2",
    "log10", "powf", "cbrt", "mul_add",
];

const CRATES: [&str; 4] = ["marg-core", "marg-net", "marg-params", "marg-io"];

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// The platform maths calls on one line of code (comments already removed):
/// method calls such as `x.hypot(y)` and paths such as `f64::sin`.
fn platform_calls(code: &str) -> Vec<&'static str> {
    let bytes = code.as_bytes();
    let mut out = Vec::new();
    for name in PLATFORM {
        let method = format!(".{name}(");
        if code.contains(&method) {
            out.push(name);
            continue;
        }
        for prefix in ["f64::", "f32::"] {
            let path = format!("{prefix}{name}");
            let found = code.match_indices(&path).any(|(i, _)| {
                let end = i + path.len();
                let before = i == 0 || !is_ident(bytes[i - 1]);
                let after = end == bytes.len() || !is_ident(bytes[end]);
                before && after
            });
            if found {
                out.push(name);
                break;
            }
        }
    }
    out
}

/// A line without its `//` comment and with string and character literals
/// blanked, so that their contents neither match nor count as braces.
fn code_of(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    let mut in_str = false;
    while let Some(c) = chars.next() {
        if in_str {
            match c {
                '\\' => {
                    chars.next();
                }
                '"' => {
                    in_str = false;
                    out.push('"');
                }
                _ => {}
            }
            continue;
        }
        match c {
            '"' => {
                in_str = true;
                out.push('"');
            }
            '/' if chars.peek() == Some(&'/') => break,
            '\'' => {
                // A character literal (`'{'`, `'\''`) is blanked; a lifetime
                // (`'a`) is kept.
                let rest: String = chars.clone().take(12).collect();
                let len = if rest.starts_with('\\') {
                    rest[2..].find('\'').map(|k| k + 3)
                } else {
                    rest.char_indices()
                        .nth(1)
                        .filter(|&(_, q)| q == '\'')
                        .map(|_| 2)
                };
                match len {
                    Some(n) => {
                        for _ in 0..n {
                            chars.next();
                        }
                        out.push_str("' '");
                    }
                    None => out.push('\''),
                }
            }
            _ => out.push(c),
        }
    }
    out
}

/// `(line number, code)` for each line of a file that is not test code.
/// A `#[cfg(test)]` item (a module, an `impl`, a function, a `use`) is
/// skipped to its closing brace or semicolon.
fn non_test_lines(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut skipping = false;
    let mut depth = 0i64;
    let mut opened = false;
    for (n, line) in text.lines().enumerate() {
        let code = code_of(line);
        if !skipping && code.trim() == "#[cfg(test)]" {
            skipping = true;
            depth = 0;
            opened = false;
            continue;
        }
        if skipping {
            for c in code.chars() {
                match c {
                    '{' => {
                        depth += 1;
                        opened = true;
                    }
                    '}' => depth -= 1,
                    _ => {}
                }
            }
            let ends_item = if opened {
                depth == 0
            } else {
                code.trim_end().ends_with(';')
            };
            if ends_item {
                skipping = false;
            }
            continue;
        }
        out.push((n + 1, code));
    }
    out
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn simulation_code_takes_transcendentals_from_libm() {
    let crates_dir = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut bad = Vec::new();
    let mut scanned = 0;
    for krate in CRATES {
        let mut files = Vec::new();
        rust_files(&crates_dir.join(krate).join("src"), &mut files);
        for path in files {
            scanned += 1;
            let text = std::fs::read_to_string(&path).unwrap();
            for (n, code) in non_test_lines(&text) {
                for name in platform_calls(&code) {
                    let rel = path.strip_prefix(crates_dir).unwrap();
                    bad.push(format!("crates/{}:{n}: {name}", rel.display()));
                }
            }
        }
    }
    assert!(
        scanned > 20,
        "scanned only {scanned} files: wrong directory?"
    );
    assert!(
        bad.is_empty(),
        "platform maths in simulation code (use the libm function instead, ADR-0003):\n{}",
        bad.join("\n")
    );
}

#[test]
fn scanner_finds_platform_calls() {
    assert_eq!(platform_calls("let d = dx.hypot(dy);"), vec!["hypot"]);
    assert_eq!(
        platform_calls("(0.5 * h).tan() / h.sin()"),
        vec!["sin", "tan"]
    );
    assert_eq!(platform_calls("xs.iter().map(f64::exp)"), vec!["exp"]);
    assert_eq!(platform_calls("let r = x.powf(2.5);"), vec!["powf"]);
    assert!(platform_calls("libm::hypot(dx, dy) + libm::sin(a)").is_empty());
    assert!(platform_calls("x.sqrt() + y.powi(2) + v.expect(\"z\")").is_empty());
    assert!(platform_calls("fold(0.0, f64::max); f64::sinh_like").is_empty());
}

#[test]
fn scanner_skips_comments_strings_and_test_items() {
    assert_eq!(code_of("let a = b; // x.sin()"), "let a = b; ");
    assert_eq!(
        code_of("f(\"x.cos() // {\", '{', 'a')"),
        "f(\"\", ' ', ' ')"
    );
    assert_eq!(code_of("fn f<'a>(x: &'a str)"), "fn f<'a>(x: &'a str)");
    assert_eq!(
        code_of(r"g('\'', '\n', '\u{7d}') { x.sin()"),
        "g(' ', ' ', ' ') { x.sin()"
    );
    let text = "fn a() {}\n#[cfg(test)]\nuse std::f64;\n#[cfg(test)]\nimpl X {\n    fn t() { '}'; }\n}\nfn b() {}\n#[cfg(test)]\nmod tests {\n    fn c() {}\n}\n";
    let kept: Vec<usize> = non_test_lines(text).into_iter().map(|(n, _)| n).collect();
    assert_eq!(kept, vec![1, 8]);
}
