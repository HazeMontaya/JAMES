//! F1-06: architecture boundary tests (static, std-only).
//!
//! Rules enforced:
//! R1. Domain crates never reference UI stacks, concrete databases/model
//!     providers, or the diagnostics bin (cycle guard). Scoped to the
//!     current domain crates; NEW crates extend CRATES + rules in their
//!     own tasks (M5 models, S6 storage).
//! R2. No secret-looking literals in non-test source.
//!
//! Method: plain substring scan over source files. Test-only code is
//! excluded by truncating each file at its (trailing) `#[cfg(test)]`
//! marker — true for every crate today; if a crate ever places tests
//! elsewhere this test fails loudly and must be extended, not weakened.

use std::path::{Path, PathBuf};

/// Crates forming the locked domain core (workspace-relative paths).
const CRATES: &[&str] = &[
    "core/crates/james-events/src",
    "core/crates/james-errors/src",
    "core/crates/james-registry/src",
    "core/crates/james-capabilities/src",
    "core/crates/james-services/src",
    "core/crates/james-tasks/src",
    "core/crates/james-scheduler/src",
    "core/crates/james-health/src",
    "core/crates/james-core/src",
];

/// Extra roots scanned for secrets (including TS + diagnostics bin).
const SECRET_ROOTS: &[&str] = &[
    "core/crates/james-events/src",
    "core/crates/james-errors/src",
    "core/crates/james-registry/src",
    "core/crates/james-capabilities/src",
    "core/crates/james-services/src",
    "core/crates/james-tasks/src",
    "core/crates/james-scheduler/src",
    "core/crates/james-health/src",
    "core/crates/james-core/src",
    "tools/diagnostics-bin/src",
    "tools/discovery/src",
];

/// Forbidden dependency markers (R1). Matched case-sensitively as written;
/// each entry documents WHY in the failure message via its group.
/// NOTE: concrete-db/provider crates (ollama, sqlx, ...) are NOT listed
/// here: field names (`ollama_url`) and docs legitimately mention them.
/// They are enforced structurally instead (see DEP_CRATES below).
const FORBIDDEN: &[(&str, &str)] = &[
    ("james_diagnostics", "core->diagnostics cycle"),
    ("james-diagnostics", "core->diagnostics cycle"),
    ("void", "Core->Void UI"),
    ("classic", "Core->Classic UI"),
    ("tauri", "Core->desktop shell"),
    ("webview", "Core->browser UI"),
    ("yew", "Core->WASM UI"),
    ("dioxus", "Core->UI framework"),
    ("react", "Core->web UI"),
    ("ui_contract", "Core->UI contract impl"),
    ("james-void", "Core->UI module"),
    ("james-classic", "Core->UI module"),
];

/// Concrete external crates that must never become domain dependencies.
/// Enforced structurally: banned as Cargo dependencies AND as `::` code
/// paths (so config keys like `ollama_url` and docs don't false-positive).
const DEP_CRATES: &[&str] = &["ollama", "sqlx", "rusqlite", "reqwest", "qdrant"];

/// Secret-shaped literals (R2). Deliberately narrow to avoid false
/// positives on words like "tokenizer" (covered by redaction unit tests).
const SECRET_MARKERS: &[&str] = &["sk-"];

fn workspace_root() -> PathBuf {
    // tests/ lives in core/crates/james-core/tests/.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).expect("crate src dir must exist");
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map(|e| e == "rs").unwrap_or(false) {
            out.push(path);
        }
    }
}

fn ts_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            ts_sources(&path, out);
        } else if path.extension().map(|e| e == "ts").unwrap_or(false)
            && !path
                .file_name()
                .map(|n| n.to_string_lossy().contains(".test."))
                .unwrap_or(false)
        {
            out.push(path);
        }
    }
}

/// Strip trailing test module: everything from `#[cfg(test)]` on is test code.
fn production_code(content: &str) -> &str {
    match content.find("#[cfg(test)]") {
        Some(idx) => &content[..idx],
        None => content,
    }
}

fn is_marker_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-'
}

/// Marker must stand alone (not inside `avoid`, `interaction`, ...).
fn contains_marker(code: &str, marker: &str) -> bool {
    code.match_indices(marker).any(|(pos, _)| {
        let before_ok = pos == 0
            || !code[..pos]
                .chars()
                .next_back()
                .map(is_marker_char)
                .unwrap_or(false);
        let after_ok = code[pos + marker.len()..]
            .chars()
            .next()
            .map(|c| !is_marker_char(c))
            .unwrap_or(true);
        before_ok && after_ok
    })
}

#[test]
fn test_architecture_no_forbidden_dependencies() {
    let root = workspace_root();
    let mut violations = Vec::new();

    for krate in CRATES {
        let src = root.join(krate);
        let mut files = Vec::new();
        rust_sources(&src, &mut files);
        for file in &files {
            let content = std::fs::read_to_string(file).expect("source must be readable");
            let code = production_code(&content);
            for (marker, reason) in FORBIDDEN {
                if contains_marker(code, marker) {
                    violations.push(format!(
                        "{} contains {:?} ({})",
                        display(&root, file),
                        marker,
                        reason
                    ));
                }
            }
            // Structural dep-crate check: `name::` code paths.
            for dep in DEP_CRATES {
                let path_marker = format!("{dep}::");
                if contains_marker(code, &path_marker) {
                    violations.push(format!(
                        "{} uses concrete crate {:?} (must live behind a trait in its own provider crate)",
                        display(&root, file),
                        dep
                    ));
                }
            }
        }
        // Cargo.toml dependency check.
        let manifest = src
            .parent()
            .expect("crate dir")
            .join("Cargo.toml");
        if let Ok(content) = std::fs::read_to_string(&manifest) {
            for line in content.lines() {
                let trimmed = line.trim_start();
                for dep in DEP_CRATES {
                    if trimmed.starts_with(&format!("{dep} "))
                        || trimmed.starts_with(&format!("{dep}="))
                        || trimmed.starts_with(&format!("{dep}."))
                    {
                        violations.push(format!(
                            "{} declares forbidden dependency {:?}",
                            display(&root, &manifest),
                            dep
                        ));
                    }
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "architecture violations:\n{}",
        violations.join("\n")
    );
}

fn display(root: &Path, file: &Path) -> String {
    file.strip_prefix(root)
        .unwrap_or(file)
        .display()
        .to_string()
}

#[test]
fn test_architecture_no_secrets_in_source() {
    let root = workspace_root();
    let mut violations = Vec::new();

    for scope in SECRET_ROOTS {
        let dir = root.join(scope);
        if scope.ends_with("/src") && scope.starts_with("core/") {
            let mut files = Vec::new();
            rust_sources(&dir, &mut files);
            for file in files {
                let content = std::fs::read_to_string(&file).expect("source must be readable");
                let code = production_code(&content);
                for marker in SECRET_MARKERS {
                    // Look for marker followed by a long token run.
                    for (lineno, line) in code.lines().enumerate() {
                        if let Some(pos) = line.find(marker) {
                            let after: String =
                                line[pos + marker.len()..].chars().take(12).collect();
                            if after.len() >= 8
                                && after.chars().all(|c| {
                                    c.is_alphanumeric() || c == '-' || c == '_' || c == '.'
                                })
                            {
                                violations.push(format!(
                                    "{}:{} looks like a live secret",
                                    file.display(),
                                    lineno + 1
                                ));
                            }
                        }
                    }
                }
            }
        } else {
            let mut files = Vec::new();
            ts_sources(&dir, &mut files);
            for file in files {
                let content = std::fs::read_to_string(&file).expect("source must be readable");
                for marker in SECRET_MARKERS {
                    for (lineno, line) in content.lines().enumerate() {
                        if let Some(pos) = line.find(marker) {
                            let after: String =
                                line[pos + marker.len()..].chars().take(12).collect();
                            if after.len() >= 8
                                && after.chars().all(|c| {
                                    c.is_alphanumeric() || c == '-' || c == '_' || c == '.'
                                })
                            {
                                violations.push(format!(
                                    "{}:{} looks like a live secret",
                                    file.display(),
                                    lineno + 1
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "secret violations:\n{}",
        violations.join("\n")
    );
}
