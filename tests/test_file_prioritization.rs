// Test file prioritization for large codebases
use skilldo::ecosystems::python::PythonHandler;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_prioritization_init_files_first() {
    let temp = TempDir::new().unwrap();

    // Create a hybrid codebase structure like PyTorch
    let torch_dir = temp.path().join("torch");
    fs::create_dir_all(&torch_dir).unwrap();
    fs::write(torch_dir.join("__init__.py"), "# Main API").unwrap();

    let nn_dir = torch_dir.join("nn");
    fs::create_dir_all(&nn_dir).unwrap();
    fs::write(nn_dir.join("__init__.py"), "# NN API").unwrap();
    fs::write(nn_dir.join("modules.py"), "# NN modules").unwrap();

    let internal_dir = torch_dir.join("_internal");
    fs::create_dir_all(&internal_dir).unwrap();
    fs::write(internal_dir.join("_utils.py"), "# Internal utils").unwrap();

    let handler = PythonHandler::new(temp.path());
    let files = handler.find_source_files().unwrap();

    // Debug: print all files and their order
    for (i, file) in files.iter().enumerate() {
        println!("{}: {}", i, file.display());
    }

    // Check that __init__.py files come first
    assert!(files[0].ends_with("torch/__init__.py") || files[0].ends_with("torch\\__init__.py"));
    assert!(files[1].ends_with("nn/__init__.py") || files[1].ends_with("nn\\__init__.py"));

    // Check that _internal files come last
    let last_file = files.last().unwrap();
    assert!(last_file.to_str().unwrap().contains("_internal"));
}

#[test]
fn test_prioritization_public_before_private() {
    let temp = TempDir::new().unwrap();
    let pkg_dir = temp.path().join("mypackage");
    fs::create_dir_all(&pkg_dir).unwrap();

    fs::write(pkg_dir.join("__init__.py"), "# Init").unwrap();
    fs::write(pkg_dir.join("public_api.py"), "# Public").unwrap();
    fs::write(pkg_dir.join("_private.py"), "# Private").unwrap();
    fs::write(pkg_dir.join("_internal.py"), "# Internal").unwrap();

    let handler = PythonHandler::new(temp.path());
    let files = handler.find_source_files().unwrap();

    // Check order: __init__.py -> public_api.py -> private files
    assert!(files[0].ends_with("__init__.py"));
    assert!(files[1].ends_with("public_api.py"));

    // Private files should be last
    let private_files: Vec<_> = files
        .iter()
        .filter(|f| {
            let name = f.file_name().unwrap().to_str().unwrap();
            name.starts_with('_') && name != "__init__.py"
        })
        .collect();

    assert_eq!(private_files.len(), 2);
    assert!(
        files
            .iter()
            .position(|f| f.ends_with("_private.py"))
            .unwrap()
            > 1
    );
}

#[test]
fn test_prioritization_skips_test_directories() {
    let temp = TempDir::new().unwrap();
    let pkg_dir = temp.path().join("mypackage");
    fs::create_dir_all(&pkg_dir).unwrap();

    fs::write(pkg_dir.join("__init__.py"), "# Init").unwrap();
    fs::write(pkg_dir.join("api.py"), "# API").unwrap();

    let tests_dir = pkg_dir.join("tests");
    fs::create_dir_all(&tests_dir).unwrap();
    fs::write(tests_dir.join("test_api.py"), "# Test").unwrap();

    let tools_dir = pkg_dir.join("tools");
    fs::create_dir_all(&tools_dir).unwrap();
    fs::write(tools_dir.join("benchmark.py"), "# Bench").unwrap();

    let handler = PythonHandler::new(temp.path());
    let files = handler.find_source_files().unwrap();

    // API files should come before test/tool files
    let api_pos = files.iter().position(|f| f.ends_with("api.py")).unwrap();
    let test_pos = files
        .iter()
        .position(|f| f.to_str().unwrap().contains("tests"))
        .unwrap();
    let tool_pos = files
        .iter()
        .position(|f| f.to_str().unwrap().contains("tools"))
        .unwrap();

    assert!(api_pos < test_pos);
    assert!(api_pos < tool_pos);
}

#[test]
fn test_prioritization_pytorch_structure() {
    let temp = TempDir::new().unwrap();

    // Simulate PyTorch structure
    let torch_dir = temp.path().join("torch");
    fs::create_dir_all(&torch_dir).unwrap();
    fs::write(torch_dir.join("__init__.py"), "# Torch main").unwrap();

    // Public API modules
    let nn_dir = torch_dir.join("nn");
    fs::create_dir_all(&nn_dir).unwrap();
    fs::write(nn_dir.join("__init__.py"), "# NN").unwrap();
    fs::write(nn_dir.join("functional.py"), "# F").unwrap();

    let optim_dir = torch_dir.join("optim");
    fs::create_dir_all(&optim_dir).unwrap();
    fs::write(optim_dir.join("__init__.py"), "# Optim").unwrap();

    // Internal implementation
    let internal_dir = torch_dir.join("_internal");
    fs::create_dir_all(&internal_dir).unwrap();
    fs::write(internal_dir.join("_utils.py"), "# Utils").unwrap();

    // Testing utilities
    let testing_dir = torch_dir.join("testing");
    fs::create_dir_all(&testing_dir).unwrap();
    fs::write(testing_dir.join("_core.py"), "# Test utils").unwrap();

    let handler = PythonHandler::new(temp.path());
    let files = handler.find_source_files().unwrap();

    // First file should be torch/__init__.py
    assert!(
        files[0].to_str().unwrap().ends_with("torch/__init__.py")
            || files[0].to_str().unwrap().ends_with("torch\\__init__.py")
    );

    // Public subpackage __init__.py files should be early
    let early_files: Vec<_> = files.iter().take(4).collect();
    assert!(early_files
        .iter()
        .any(|f| f.to_str().unwrap().contains("nn") && f.ends_with("__init__.py")));
    assert!(early_files
        .iter()
        .any(|f| f.to_str().unwrap().contains("optim") && f.ends_with("__init__.py")));

    // Internal and testing files should be last
    let last_files: Vec<_> = files.iter().rev().take(2).collect();
    assert!(last_files
        .iter()
        .any(|f| f.to_str().unwrap().contains("_internal")
            || f.to_str().unwrap().contains("testing")));
}

#[test]
fn test_collector_respects_priority_with_char_limit() {
    let temp = TempDir::new().unwrap();
    let pkg_dir = temp.path().join("mypackage");
    fs::create_dir_all(&pkg_dir).unwrap();

    // Create files with known sizes
    fs::write(pkg_dir.join("__init__.py"), "x".repeat(5000)).unwrap(); // 5K
    fs::write(pkg_dir.join("api.py"), "y".repeat(8000)).unwrap(); // 8K
    fs::write(pkg_dir.join("_impl.py"), "z".repeat(10000)).unwrap(); // 10K

    let handler = PythonHandler::new(temp.path());
    let files = handler.find_source_files().unwrap();

    // With 15K limit, should get __init__.py (5K) + api.py (8K) = 13K
    // Should NOT get _impl.py (would exceed limit)
    assert_eq!(files.len(), 3);
    assert!(files[0].ends_with("__init__.py"));
    assert!(files[1].ends_with("api.py"));
    assert!(files[2].ends_with("_impl.py")); // Listed but will be truncated by collector
}
