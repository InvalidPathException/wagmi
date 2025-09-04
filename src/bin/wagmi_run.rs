use clap::Parser;
use std::fs;
use std::path::PathBuf;
use wagmi::{Module, Instance, Imports, WasmValue, ExportValue};

mod utils;
use utils::compile_wat;

#[derive(Parser, Debug)]
#[command(name = "wagmi-run")]
#[command(about = "Execute WebAssembly modules using WAGMI interpreter")]
#[command(long_about = "
WAGMI Run - WebAssembly General & Minimal Interpreter Runner

This tool allows you to execute WebAssembly modules directly from the command line.
You can invoke specific exported functions with arguments and see the results.

Examples:
  # Run the default _start function (if exists)
  wagmi-run module.wasm
  
  # Invoke a specific function with no arguments
  wagmi-run module.wasm --invoke main
  
  # Invoke a function with arguments (i32, f32, i64, f64 supported)
  wagmi-run module.wasm --invoke add --args 10:i32 20:i32
  
  # Invoke a function with floating point arguments
  wagmi-run module.wasm --invoke calculate --args 3.14:f32 2.718:f64
  
  # Enable debug output
  wagmi-run module.wasm --invoke factorial --args 5:i32 --debug
")]
struct Args {
    /// Path to the WebAssembly module file
    wasm_file: PathBuf,
    
    /// Function to invoke (defaults to _start if available)
    #[arg(short, long)]
    invoke: Option<String>,
    
    /// Arguments to pass to the function (format: value:type, e.g., 42:i32, 3.14:f32)
    #[arg(short, long, value_delimiter = ' ', num_args = 0..)]
    args: Vec<String>,
    
    /// Enable debug output
    #[arg(short, long)]
    debug: bool,
    
    /// List all exports instead of running
    #[arg(short, long)]
    list_exports: bool,
}

fn parse_value(arg: &str) -> Result<WasmValue, String> {
    let parts: Vec<&str> = arg.split(':').collect();
    if parts.len() != 2 {
        return Err(format!("Invalid argument format '{}'. Expected format: value:type (e.g., 42:i32)", arg));
    }
    
    let value_str = parts[0];
    let type_str = parts[1];
    
    match type_str {
        "i32" => {
            let val = value_str.parse::<i32>()
                .map_err(|_| format!("Failed to parse '{}' as i32", value_str))?;
            Ok(WasmValue::from_i32(val))
        }
        "i64" => {
            let val = value_str.parse::<i64>()
                .map_err(|_| format!("Failed to parse '{}' as i64", value_str))?;
            Ok(WasmValue::from_i64(val))
        }
        "f32" => {
            let val = value_str.parse::<f32>()
                .map_err(|_| format!("Failed to parse '{}' as f32", value_str))?;
            Ok(WasmValue::from_f32(val))
        }
        "f64" => {
            let val = value_str.parse::<f64>()
                .map_err(|_| format!("Failed to parse '{}' as f64", value_str))?;
            Ok(WasmValue::from_f64(val))
        }
        _ => Err(format!("Unknown type '{}'. Supported types: i32, i64, f32, f64", type_str))
    }
}

fn format_value(val: &WasmValue, _hint: Option<&str>) -> String {
    let i32_val = val.as_i32();
    let i64_val = val.as_i64();
    
    if i64_val == i32_val as i64 {
        format!("{} (i32)", i32_val)
    } else {
        format!("{} (i64)", i64_val)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    
    if args.debug {
        eprintln!("Loading module from: {:?}", args.wasm_file);
    }
    
    // Check if it's a WAT file or WASM file
    let bytes = if args.wasm_file.extension().and_then(|s| s.to_str()) == Some("wat") {
        if args.debug {
            eprintln!("Detected WAT file, compiling to WASM...");
        }
        compile_wat(&args.wasm_file)
            .map_err(|e| format!("Failed to compile WAT file: {}", e))?
    } else {
        fs::read(&args.wasm_file)
            .map_err(|e| format!("Failed to read WASM file: {}", e))?
    };
    
    if args.debug {
        eprintln!("Module size: {} bytes", bytes.len());
    }
    
    let module = Module::compile(bytes)
        .map_err(|e| format!("Failed to compile module: {:?}", e))?;
    
    let module = std::rc::Rc::new(module);
    
    let imports = Imports::new();
    let instance = Instance::instantiate(module.clone(), &imports)
        .map_err(|e| format!("Failed to instantiate module: {:?}", e))?;
    
    if args.list_exports {
        println!("Exported functions:");
        for (name, export) in &instance.exports {
            if let ExportValue::Function(func) = export {
                print!("  {} (", name);
                let n_params = func.ty.n_params();
                for i in 0..n_params {
                    if i > 0 { print!(", "); }
                    print!("param{}", i);
                }
                print!(")");
                if func.ty.has_result() {
                    print!(" -> result");
                }
                println!();
            }
        }
        return Ok(());
    }
    
    let func_name = args.invoke.as_deref().unwrap_or("_start");
    
    if args.debug {
        eprintln!("Looking for function: {}", func_name);
    }
    
    let export = instance.exports.get(func_name)
        .ok_or_else(|| format!("Function '{}' not found in exports", func_name))?;
    
    let func = match export {
        ExportValue::Function(f) => f,
        _ => return Err(format!("Export '{}' is not a function", func_name).into()),
    };
    
    let mut wasm_args = Vec::new();
    for arg_str in &args.args {
        wasm_args.push(parse_value(arg_str)?);
    }
    
    if wasm_args.len() != func.ty.n_params() as usize {
        return Err(format!(
            "Function '{}' expects {} arguments, but {} provided",
            func_name,
            func.ty.n_params(),
            wasm_args.len()
        ).into());
    }
    
    if args.debug {
        eprintln!("Invoking function with {} arguments", wasm_args.len());
    }
    
    let results = instance.invoke(func, &wasm_args)
        .map_err(|e| format!("Execution failed: {:?}", e))?;
    
    if results.is_empty() {
        if args.debug {
            eprintln!("Function completed successfully (no return value)");
        }
    } else {
        println!("Result:");
        for (i, result) in results.iter().enumerate() {
            println!("  [{}] {}", i, format_value(result, None));
        }
    }
    
    Ok(())
}