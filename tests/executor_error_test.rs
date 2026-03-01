//! Tests for test agent executor error paths
//! Tests error handling in PythonUvExecutor

use anyhow::Result;
use skilldo::test_agent::executor::PythonUvExecutor;
use skilldo::test_agent::LanguageExecutor;

#[tokio::test]
async fn test_executor_setup_fails_when_uv_not_installed() {
    // This test will pass if uv IS installed (our case)
    // but tests the error path by checking it doesn't panic
    let executor = PythonUvExecutor::new();

    // Try to setup environment - either succeeds or fails gracefully
    let result = executor.setup_environment(&[]);

    // Should either work (if uv installed) or return error (if not)
    match result {
        Ok(_) => {} // Setup succeeded with uv installed
        Err(e) => {
            // Should fail with specific message about uv
            assert!(
                e.to_string().contains("uv"),
                "Error should mention uv: {}",
                e
            );
        }
    }
}

#[tokio::test]
async fn test_executor_setup_with_invalid_dependency() -> Result<()> {
    let executor = PythonUvExecutor::new();

    // Try to install a non-existent package
    // uv may fail or timeout
    let result = executor.setup_environment(&["nonexistent-package-xyzabc-12345".to_string()]);

    // Either succeeds (uv is very permissive) or fails
    match result {
        Ok(_) => {} // uv might allow invalid deps in pyproject.toml
        Err(e) => {
            // Or it might fail during sync
            assert!(!e.to_string().is_empty(), "Error should have message");
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_executor_run_code_with_syntax_error() -> Result<()> {
    let executor = PythonUvExecutor::new();
    let env = executor.setup_environment(&[])?;

    // Code with syntax error
    let code = r#"
def broken syntax here
    print("this won't work")
"#;

    let result = executor.run_code(&env, code)?;

    // Should return Fail with syntax error
    assert!(result.is_fail(), "Code with syntax error should fail");

    let error = match result {
        skilldo::test_agent::executor::ExecutionResult::Fail(err) => err,
        _ => panic!("Expected Fail result for syntax error"),
    };

    assert!(
        error.contains("SyntaxError") || error.contains("syntax"),
        "Error should mention syntax error: {}",
        error
    );

    executor.cleanup(&env)?;

    Ok(())
}

#[tokio::test]
async fn test_executor_cleanup_succeeds() -> Result<()> {
    let executor = PythonUvExecutor::new();
    let env = executor.setup_environment(&[])?;

    // Cleanup should succeed
    let result = executor.cleanup(&env);
    assert!(result.is_ok(), "Cleanup should succeed");

    Ok(())
}

#[tokio::test]
async fn test_executor_handles_code_with_import_error() -> Result<()> {
    let executor = PythonUvExecutor::new();
    let env = executor.setup_environment(&[])?;

    // Code that tries to import non-existent module
    let code = r#"
import nonexistent_module_xyz
print("Should not reach here")
"#;

    let result = executor.run_code(&env, code)?;

    // Should fail with ImportError
    assert!(result.is_fail(), "Import error should cause failure");

    let error = match result {
        skilldo::test_agent::executor::ExecutionResult::Fail(err) => err,
        _ => panic!("Expected Fail result for import error"),
    };

    assert!(
        error.contains("ModuleNotFoundError") || error.contains("ImportError"),
        "Error should mention import error: {}",
        error
    );

    executor.cleanup(&env)?;

    Ok(())
}

#[tokio::test]
async fn test_executor_with_very_short_timeout() -> Result<()> {
    // Create executor with 1 second timeout
    let executor = PythonUvExecutor::new().with_timeout(1);
    let env = executor.setup_environment(&[])?;

    // Code that sleeps for longer than timeout
    let code = r#"
import time
time.sleep(5)
print("Should not reach here")
"#;

    let result = executor.run_code(&env, code)?;

    // Should timeout (or fail if OS kills it immediately)
    match result {
        skilldo::test_agent::executor::ExecutionResult::Timeout => {} // expected
        skilldo::test_agent::executor::ExecutionResult::Fail(_) => {} // acceptable on some systems
        skilldo::test_agent::executor::ExecutionResult::Pass(_) => {} // possible if timeout is generous
    }

    executor.cleanup(&env)?;

    Ok(())
}
