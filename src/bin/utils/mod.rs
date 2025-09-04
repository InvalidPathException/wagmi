use std::process::Command;
use std::path::{Path, PathBuf};
use std::fs;
use std::env;

/// Compiles a WAT file to WASM using wat2wasm
pub fn compile_wat(wat_path: &Path) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let temp_dir = env::temp_dir();
    let stem = wat_path.file_stem()
        .ok_or("Invalid WAT file path")?
        .to_string_lossy();
    let wasm_path = temp_dir.join(format!("{}.wasm", stem));
    
    // Determine the correct wat2wasm binary based on OS
    let wat2wasm = if cfg!(target_os = "macos") {
        "tools/osx/wat2wasm"
    } else if cfg!(target_os = "linux") {
        "tools/linux/wat2wasm"
    } else {
        return Err("Unsupported OS for wat2wasm".into());
    };
    
    // Run wat2wasm
    let output = Command::new(wat2wasm)
        .arg(wat_path)
        .arg("-o")
        .arg(&wasm_path)
        .output()
        .map_err(|e| format!("Failed to run wat2wasm: {}", e))?;
    
    if !output.status.success() {
        return Err(format!(
            "wat2wasm compilation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ).into());
    }
    
    let wasm_bytes = fs::read(&wasm_path)?;
    let _ = fs::remove_file(&wasm_path);
    
    Ok(wasm_bytes)
}

/// Loads and compiles a resource WAT file
pub fn load_resource_module(name: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/bin/resources")
        .join(format!("{}.wat", name));
    compile_wat(&path)
}