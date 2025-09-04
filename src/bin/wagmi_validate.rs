use clap::Parser;
use std::fs;
use std::path::PathBuf;
use wagmi::{Module, Validator};

#[derive(Parser, Debug)]
#[command(name = "wagmi-validate")]
#[command(about = "Validate WebAssembly modules for correctness")]
#[command(long_about = "
WAGMI Validate - WebAssembly Module Validator

This tool validates WebAssembly modules according to the WebAssembly 1.0 specification.
It checks for structural validity, type correctness, and other validation rules without
executing the module.

Examples:
  # Validate a single module
  wagmi-validate module.wasm
  
  # Validate multiple modules
  wagmi-validate module1.wasm module2.wasm module3.wasm
  
  # Validate with verbose output
  wagmi-validate module.wasm --verbose
  
  # Quiet mode (only show errors)
  wagmi-validate module.wasm --quiet
")]
struct Args {
    /// Path(s) to WebAssembly module file(s)
    wasm_files: Vec<PathBuf>,
    
    /// Show verbose validation details
    #[arg(short, long)]
    verbose: bool,
    
    /// Quiet mode - only show errors
    #[arg(short, long)]
    quiet: bool,
}

fn validate_file(path: &PathBuf, verbose: bool, quiet: bool) -> Result<(), Box<dyn std::error::Error>> {
    if verbose {
        println!("Validating: {}", path.display());
    }
    
    let bytes = fs::read(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    
    if verbose {
        println!("  Size: {} bytes", bytes.len());
    }
    
    match Module::compile(bytes) {
        Ok(mut module) => {
            if verbose {
                println!("  Module compiled successfully");
                println!("  Functions: {}", module.functions.len());
                println!("  Exports: {}", module.exports.len());
                if !module.imports.is_empty() {
                    let import_count: usize = module.imports.values()
                        .map(|m| m.len())
                        .sum();
                    println!("  Imports: {}", import_count);
                }
            }
            
            let func_count = module.functions.len();
            let mut imported_idxs = Vec::new();
            for (idx, func) in module.functions.iter().enumerate() {
                if func.import.is_some() {
                    imported_idxs.push(idx);
                }
            }
            
            let mut validator = Validator::new(&mut module);
            
            for idx in 0..func_count {
                if imported_idxs.contains(&idx) {
                    continue;
                }
                
                if verbose {
                    println!("  Validating function {}", idx);
                }
                
                if let Err(e) = validator.validate_function(idx) {
                    return Err(format!("Validation failed for function {}: {:?}", idx, e).into());
                }
            }
            
            if !quiet {
                println!("VALID: {}", path.display());
            }
            Ok(())
        }
        Err(e) => {
            Err(format!("INVALID: {} - {:?}", path.display(), e).into())
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    
    if args.wasm_files.is_empty() {
        eprintln!("Error: No WebAssembly files specified");
        eprintln!("Usage: wagmi-validate <WASM_FILES>...");
        std::process::exit(1);
    }
    
    let mut all_valid = true;
    let mut errors = Vec::new();
    
    for path in &args.wasm_files {
        if !path.exists() {
            eprintln!("ERROR: {} - File not found", path.display());
            all_valid = false;
            continue;
        }
        
        match validate_file(path, args.verbose, args.quiet) {
            Ok(()) => {
                if args.verbose && args.wasm_files.len() > 1 {
                    println!();
                }
            }
            Err(e) => {
                errors.push(e.to_string());
                all_valid = false;
            }
        }
    }
    
    if !errors.is_empty() {
        eprintln!("\nValidation errors:");
        for error in &errors {
            eprintln!("{}", error);
        }
    }
    
    if args.wasm_files.len() > 1 && !args.quiet {
        println!("\nSummary:");
        let valid_count = args.wasm_files.len() - errors.len();
        println!("  Valid: {}/{}", valid_count, args.wasm_files.len());
        if !all_valid {
            println!("  Invalid: {}/{}", errors.len(), args.wasm_files.len());
        }
    }
    
    if all_valid {
        Ok(())
    } else {
        std::process::exit(1);
    }
}