pub mod go;
pub mod python;

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
    } else if prefix.contains("bsd 3-clause")
        || prefix.contains("redistribution and use in source and binary")
    {
        Some("BSD-3-Clause".into())
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
    fn test_classify_bsd3_via_redistribution_phrase() {
        // A license with the redistribution phrase but no explicit "bsd N-clause" header
        // defaults to BSD-3-Clause.
        let bsd3 = "Copyright (c) 2024\n\nRedistribution and use in source and binary forms...";
        assert_eq!(classify_license(bsd3), Some("BSD-3-Clause".to_string()));
    }

    #[test]
    fn test_license_filenames_contains_common() {
        assert!(LICENSE_FILENAMES.contains(&"LICENSE"));
        assert!(LICENSE_FILENAMES.contains(&"LICENSE.md"));
        assert!(LICENSE_FILENAMES.contains(&"COPYING"));
    }
}
