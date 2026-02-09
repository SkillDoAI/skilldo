use skilldo::ecosystems::python::PythonHandler;
use std::path::Path;

#[test]
fn test_python_file_discovery() {
    // Try e2e path first, fall back to /tmp/requests
    let candidates = ["/tmp/skilldo-e2e/requests", "/tmp/requests"];
    let requests_path = candidates
        .iter()
        .map(Path::new)
        .find(|p| p.join("src").exists() || p.join("requests").exists());

    let requests_path = match requests_path {
        Some(p) => p,
        None => {
            println!("Skipping test - no requests repo found");
            return;
        }
    };

    let handler = PythonHandler::new(requests_path);

    // Should find source files
    let source_files = handler
        .find_source_files()
        .expect("Should find source files");
    assert!(!source_files.is_empty(), "Should have source files");
    println!("Found {} source files", source_files.len());

    // Test files may not exist in shallow clones — only assert if found
    match handler.find_test_files() {
        Ok(test_files) if !test_files.is_empty() => {
            println!("Found {} test files", test_files.len());
        }
        _ => {
            println!("No test files found (shallow clone?) - skipping test file assertions");
        }
    }

    // Should find docs
    let docs = handler.find_docs().expect("Should find docs");
    assert!(!docs.is_empty(), "Should have docs");
    println!("Found {} doc files", docs.len());

    // Should find changelog
    let changelog = handler.find_changelog();
    assert!(changelog.is_some(), "Should have changelog");
    println!("Found changelog: {:?}", changelog.unwrap());

    // Should get version
    let version = handler.get_version().expect("Should get version");
    println!("Version: {}", version);
}
