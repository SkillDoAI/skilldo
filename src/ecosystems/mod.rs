pub mod go;
pub mod python;

/// Common license file names found at the root of open-source repositories.
pub(crate) const LICENSE_FILENAMES: &[&str] =
    &["LICENSE", "LICENSE.md", "LICENSE.txt", "LICENCE", "COPYING"];

/// Detect whether a setup.py value is an indirect reference (dict access,
/// about/meta variable lookup) rather than a literal version string.
#[allow(dead_code)] // shared utility; consumers pending
pub(crate) fn is_setup_py_indirect(value: &str) -> bool {
    value.starts_with('[') || value.contains("about") || value.contains("meta")
}

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
    } else if prefix.contains("bsd 3-clause")
        || prefix.contains("redistribution and use in source and binary")
    {
        Some("BSD-3-Clause".into())
    } else if prefix.contains("bsd 2-clause") {
        Some("BSD-2-Clause".into())
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
    fn test_is_setup_py_indirect_dict_access() {
        assert!(is_setup_py_indirect(r#"about["__title__"]"#));
    }

    #[test]
    fn test_is_setup_py_indirect_meta_access() {
        assert!(is_setup_py_indirect("meta['version']"));
    }

    #[test]
    fn test_is_setup_py_indirect_bracket() {
        assert!(is_setup_py_indirect("[metadata]"));
    }

    #[test]
    fn test_is_setup_py_indirect_normal_string() {
        assert!(!is_setup_py_indirect("1.2.3"));
    }

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
    fn test_license_filenames_contains_common() {
        assert!(LICENSE_FILENAMES.contains(&"LICENSE"));
        assert!(LICENSE_FILENAMES.contains(&"LICENSE.md"));
        assert!(LICENSE_FILENAMES.contains(&"COPYING"));
    }
}
