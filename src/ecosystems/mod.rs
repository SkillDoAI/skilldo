pub mod go;
pub mod java;
pub mod javascript;
pub mod python;
pub mod rust;

use std::path::{Path, PathBuf};

/// Walk a directory tree respecting `.gitignore`, returning files matching
/// the given extensions. Uses the `ignore` crate (same walker as ripgrep)
/// so `.gitignore`, `.git/info/exclude`, and global gitignore are all honoured.
///
/// - `root`: directory to walk
/// - `extensions`: file extensions to include (without dot), e.g. `["rs", "toml"]`
/// - `extra_skip`: additional directory names to skip beyond gitignore
/// - `max_depth`: maximum recursion depth (None for unlimited)
pub(crate) fn walk_files(
    root: &Path,
    extensions: &[&str],
    extra_skip: &[&str],
    max_depth: Option<usize>,
) -> Vec<PathBuf> {
    let mut builder = ignore::WalkBuilder::new(root);
    builder
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .follow_links(false)
        .sort_by_file_path(|a, b| a.cmp(b));

    if let Some(depth) = max_depth {
        builder.max_depth(Some(depth));
    }

    if !extra_skip.is_empty() {
        let mut overrides = ignore::overrides::OverrideBuilder::new(root);
        for dir in extra_skip {
            if let Err(e) = overrides.add(&format!("!{dir}/")) {
                tracing::warn!("walk_files: invalid skip pattern '!{dir}/': {e}");
            }
        }
        if let Ok(ov) = overrides.build() {
            builder.overrides(ov);
        }
    }

    let mut files = Vec::new();
    for entry in builder.build().flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if extensions.is_empty() {
            files.push(path.to_path_buf());
            continue;
        }
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if extensions.iter().any(|e| ext.eq_ignore_ascii_case(e)) {
                files.push(path.to_path_buf());
            }
        }
    }
    files
}

/// Common license file names found at the root of open-source repositories.
pub(crate) const LICENSE_FILENAMES: &[&str] =
    &["LICENSE", "LICENSE.md", "LICENSE.txt", "LICENCE", "COPYING"];

/// Classify a license file by its content (first few hundred chars).
pub(crate) fn classify_license(content: &str) -> Option<String> {
    // Only lowercase the prefix we actually inspect (avoid full-file allocation)
    let byte_end = content.len().min(600);
    let mut end = byte_end;
    while end > 0 && !content.is_char_boundary(end) {
        end -= 1;
    }
    let prefix = content[..end].to_lowercase();

    if prefix.contains("mit license")
        || prefix.contains("permission is hereby granted, free of charge")
    {
        Some("MIT".into())
    } else if prefix.contains("apache license") && prefix.contains("version 2") {
        Some("Apache-2.0".into())
    } else if prefix.contains("bsd 2-clause") {
        Some("BSD-2-Clause".into())
    } else if prefix.contains("bsd 3-clause") {
        Some("BSD-3-Clause".into())
    } else if prefix.contains("redistribution and use in source and binary") {
        // Distinguish BSD-3 from BSD-2 by the non-endorsement clause
        if prefix.contains("neither the name of")
            || prefix.contains("may be used to endorse or promote")
        {
            Some("BSD-3-Clause".into())
        } else {
            Some("BSD-2-Clause".into())
        }
    } else if prefix.contains("mozilla public license") {
        Some("MPL-2.0".into())
    } else if prefix.contains("gnu general public license") {
        if prefix.contains("version 3") {
            Some("GPL-3.0".into())
        } else {
            Some("GPL-2.0".into())
        }
    } else if prefix.contains("the unlicense") || prefix.contains("unlicense") {
        Some("Unlicense".into())
    } else if prefix.contains("isc license") {
        Some("ISC".into())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_license_mit() {
        assert_eq!(
            classify_license("MIT License\n\nCopyright (c) 2024"),
            Some("MIT".to_string())
        );
    }

    #[test]
    fn test_classify_license_apache() {
        assert_eq!(
            classify_license("Apache License\nVersion 2.0"),
            Some("Apache-2.0".to_string())
        );
    }

    #[test]
    fn test_classify_license_unknown() {
        assert_eq!(
            classify_license("Some random text that is not a license"),
            None
        );
    }

    #[test]
    fn test_classify_bsd2_with_redistribution_phrase() {
        // BSD-2-Clause files also contain "redistribution and use in source and binary"
        // — must not be misclassified as BSD-3-Clause.
        let bsd2 = "BSD 2-Clause License\n\nRedistribution and use in source and binary forms, with or without modification, are permitted...";
        assert_eq!(classify_license(bsd2), Some("BSD-2-Clause".to_string()));
    }

    #[test]
    fn test_classify_bsd3_via_non_endorsement_clause() {
        // BSD-3-Clause detected by non-endorsement clause, not just header
        let bsd3 = "Copyright (c) 2024\n\nRedistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met:\n\n1. Redistributions of source code...\n2. Redistributions in binary form...\n3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products...";
        assert_eq!(classify_license(bsd3), Some("BSD-3-Clause".to_string()));
    }

    #[test]
    fn test_classify_bsd2_via_redistribution_without_endorsement() {
        // Redistribution phrase without non-endorsement clause → BSD-2-Clause
        let bsd2 = "Copyright (c) 2024\n\nRedistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met:\n\n1. Redistributions of source code...\n2. Redistributions in binary form...";
        assert_eq!(classify_license(bsd2), Some("BSD-2-Clause".to_string()));
    }

    #[test]
    fn test_classify_license_multibyte_boundary() {
        // 598 ASCII bytes + a 3-byte UTF-8 char (€) = 601 bytes total.
        // The 600-byte cutoff lands inside the multi-byte char, triggering
        // the char boundary adjustment loop (lines 13-15).
        let mut content = "MIT License ".repeat(49); // 49 * 12 = 588 bytes
        content.push_str("1234567890"); // 598 bytes
        content.push('€'); // 3-byte UTF-8 char → total 601
        assert_eq!(classify_license(&content), Some("MIT".to_string()));
    }

    #[test]
    fn test_license_filenames_contains_common() {
        assert!(LICENSE_FILENAMES.contains(&"LICENSE"));
        assert!(LICENSE_FILENAMES.contains(&"LICENSE.md"));
        assert!(LICENSE_FILENAMES.contains(&"COPYING"));
    }

    #[test]
    fn walk_files_respects_gitignore() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Init a git repo so .gitignore is honoured
        std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(root)
            .output()
            .unwrap();

        // Create .gitignore that excludes internal/
        std::fs::write(root.join(".gitignore"), "internal/\n").unwrap();

        // Create docs: one visible, one gitignored
        let docs = root.join("docs");
        std::fs::create_dir_all(docs.join("internal")).unwrap();
        std::fs::write(docs.join("guide.md"), "# Guide\n").unwrap();
        std::fs::write(docs.join("internal").join("design.md"), "# Design\n").unwrap();

        let files = walk_files(&docs, &["md"], &[], None);
        let names: Vec<&str> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();

        assert!(names.contains(&"guide.md"), "visible doc should be found");
        assert!(
            !names.contains(&"design.md"),
            "gitignored doc should be excluded: {names:?}"
        );
    }

    #[test]
    fn walk_files_filters_by_extension() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        std::fs::write(root.join("lib.rs"), "fn main() {}").unwrap();
        std::fs::write(root.join("helper.rs"), "fn help() {}").unwrap();
        std::fs::write(root.join("script.py"), "print('hi')").unwrap();
        std::fs::write(root.join("readme.md"), "# Readme").unwrap();

        let files = walk_files(root, &["rs"], &[], None);
        let names: Vec<&str> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();

        assert_eq!(names.len(), 2, "should find exactly 2 .rs files: {names:?}");
        assert!(names.contains(&"lib.rs"));
        assert!(names.contains(&"helper.rs"));
        assert!(!names.contains(&"script.py"));
        assert!(!names.contains(&"readme.md"));
    }

    #[test]
    fn walk_files_max_depth_limits_recursion() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // depth 1 (relative to root): root/top.txt
        std::fs::write(root.join("top.txt"), "top").unwrap();
        // depth 2: root/sub/mid.txt
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("sub").join("mid.txt"), "mid").unwrap();
        // depth 3: root/sub/deep/bottom.txt
        std::fs::create_dir_all(root.join("sub").join("deep")).unwrap();
        std::fs::write(root.join("sub").join("deep").join("bottom.txt"), "bot").unwrap();

        // max_depth=1 means only the root directory itself (no subdirs)
        let files_d1 = walk_files(root, &["txt"], &[], Some(1));
        let names_d1: Vec<&str> = files_d1
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();
        assert!(names_d1.contains(&"top.txt"), "depth-1 file should appear");
        assert!(
            !names_d1.contains(&"mid.txt"),
            "depth-2 file should be excluded at max_depth=1"
        );

        // max_depth=2 includes root + one level of subdirs
        let files_d2 = walk_files(root, &["txt"], &[], Some(2));
        let names_d2: Vec<&str> = files_d2
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();
        assert!(names_d2.contains(&"top.txt"));
        assert!(names_d2.contains(&"mid.txt"));
        assert!(
            !names_d2.contains(&"bottom.txt"),
            "depth-3 file should be excluded at max_depth=2"
        );
    }

    #[test]
    fn walk_files_empty_directory_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let files = walk_files(root, &["rs"], &[], None);
        assert!(
            files.is_empty(),
            "empty directory should return no files: {files:?}"
        );
    }

    #[test]
    fn walk_files_skips_extra_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        std::fs::create_dir_all(root.join("vendor")).unwrap();
        std::fs::write(root.join("good.md"), "# Good\n").unwrap();
        std::fs::write(root.join("vendor").join("vendored.md"), "# Vendored\n").unwrap();

        let files = walk_files(root, &["md"], &["vendor"], None);
        let names: Vec<&str> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();

        assert!(names.contains(&"good.md"));
        assert!(
            !names.contains(&"vendored.md"),
            "extra_skip should exclude vendor/: {names:?}"
        );
    }
}
