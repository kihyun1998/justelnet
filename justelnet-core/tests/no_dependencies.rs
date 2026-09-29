//! The Core builds with no dependencies: its manifest declares no normal or
//! build dependency, for any target.

/// Whether a table header (without brackets or comment) declares normal or
/// build dependencies: `dependencies`, `build-dependencies`, a
/// `target.<cfg>.` form of either, or a `.<name>` table under any of them.
fn is_dependency_table(header: &str) -> bool {
    let segments: Vec<&str> = header.split('.').map(str::trim).collect();
    segments
        .iter()
        .any(|s| *s == "dependencies" || *s == "build-dependencies")
}

/// The lines of `manifest` that declare a normal or build dependency.
fn dependencies(manifest: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut in_dependencies = false;
    for raw in manifest.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if let Some(header) = line.strip_prefix('[') {
            let header = header.trim_start_matches('[').trim_end_matches(']');
            // A table of its own for one dependency: `dependencies.<name>`,
            // or `target.<cfg>.dependencies.<name>`.
            let segments = header.split('.').count();
            let named = if header.starts_with("target.") {
                segments > 3
            } else {
                segments > 1
            };
            in_dependencies = is_dependency_table(header);
            if in_dependencies && named {
                found.push(line);
            }
        } else if in_dependencies && !line.is_empty() {
            found.push(line);
        }
    }
    found
}

#[test]
fn manifest_lists_no_dependencies() {
    assert_eq!(
        dependencies(include_str!("../Cargo.toml")),
        Vec::<&str>::new()
    );
}

#[test]
fn every_dependency_form_is_found() {
    let manifest = r#"
[package]
name = "x"

[dependencies] # none yet
foo = "1"

[dependencies.bar]
version = "2"

[build-dependencies]
cc = "1"

[target.'cfg(unix)'.dependencies]
libc = "0.2"

[target.'cfg(unix)'.dependencies.nix]
version = "0.29"

[dev-dependencies]
proptest = "1"

[target.'cfg(unix)'.dev-dependencies]
tempfile = "3"
"#;
    assert_eq!(
        dependencies(manifest),
        [
            "foo = \"1\"",
            "[dependencies.bar]",
            "version = \"2\"",
            "cc = \"1\"",
            "libc = \"0.2\"",
            "[target.'cfg(unix)'.dependencies.nix]",
            "version = \"0.29\"",
        ]
    );
}
