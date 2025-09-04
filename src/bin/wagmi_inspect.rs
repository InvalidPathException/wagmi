use clap::Parser;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use wagmi::{Module, Instance, Imports, ExportValue, ValType};

#[derive(Parser, Debug)]
#[command(name = "wagmi-inspect")]
#[command(about = "Inspect WebAssembly modules to understand their structure")]
#[command(long_about = "
WAGMI Inspect - WebAssembly Module Inspector

This tool analyzes WebAssembly modules and displays detailed information about their
structure, including imports, exports, functions, memory, tables, and globals.

Examples:
  # Basic inspection
  wagmi-inspect module.wasm
  
  # Show only exports
  wagmi-inspect module.wasm --exports-only
  
  # Show only imports
  wagmi-inspect module.wasm --imports-only
  
  # Verbose output with internal details
  wagmi-inspect module.wasm --verbose
")]
struct Args {
    /// Path to the WebAssembly module file
    wasm_file: PathBuf,
    
    /// Show only exports
    #[arg(long)]
    exports_only: bool,
    
    /// Show only imports
    #[arg(long)]
    imports_only: bool,
    
    /// Show verbose output with internal details
    #[arg(short, long)]
    verbose: bool,
}

fn format_type(val_type: &ValType) -> &'static str {
    match val_type {
        ValType::I32 => "i32",
        ValType::I64 => "i64",
        ValType::F32 => "f32",
        ValType::F64 => "f64",
        ValType::Any => "any",
    }
}

fn format_signature(params: &[ValType], result: Option<ValType>) -> String {
    let params_str = params.iter()
        .map(format_type)
        .collect::<Vec<_>>()
        .join(", ");
    
    match result {
        Some(r) => format!("({}) -> {}", params_str, format_type(&r)),
        None => format!("({})", params_str),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    
    let bytes = fs::read(&args.wasm_file)
        .map_err(|e| format!("Failed to read WASM file: {}", e))?;
    
    println!("Module: {}", args.wasm_file.display());
    println!("Size: {} bytes", bytes.len());
    println!();
    
    let module = Module::compile(bytes)
        .map_err(|e| format!("Failed to compile module: {:?}", e))?;
    
    let module = Rc::new(module);
    
    if !args.exports_only {
        if !module.imports.is_empty() {
            println!("Imports:");
            for (module_name, imports) in &module.imports {
                for (field_name, import_type) in imports {
                    let type_str = match import_type {
                        wagmi::module::ExternType::Func => "function",
                        wagmi::module::ExternType::Table => "table",
                        wagmi::module::ExternType::Mem => "memory",
                        wagmi::module::ExternType::Global => "global",
                    };
                    println!("  {}.{} ({})", module_name, field_name, type_str);
                }
            }
            println!();
        } else if !args.imports_only {
            println!("Imports: none");
            println!();
        }
    }
    
    if args.imports_only {
        return Ok(());
    }
    
    let imports = Imports::new();
    let instance = match Instance::instantiate(module.clone(), &imports) {
        Ok(inst) => inst,
        Err(e) => {
            if !args.exports_only {
                eprintln!("Note: Module instantiation failed (likely due to missing imports): {:?}", e);
                eprintln!("Showing available compile-time information only.\n");
            }
            
            if !module.exports.is_empty() {
                println!("Exports (from module metadata):");
                for (name, export) in &module.exports {
                    let type_str = match export.extern_type {
                        wagmi::module::ExternType::Func => {
                            let func_idx = export.idx as usize;
                            if func_idx < module.functions.len() {
                                let func = &module.functions[func_idx];
                                format!("function {}", format_signature(&func.ty.params, func.ty.result))
                            } else {
                                "function".to_string()
                            }
                        }
                        wagmi::module::ExternType::Table => "table".to_string(),
                        wagmi::module::ExternType::Mem => "memory".to_string(),
                        wagmi::module::ExternType::Global => "global".to_string(),
                    };
                    println!("  {} ({})", name, type_str);
                }
            } else {
                println!("Exports: none");
            }
            return Ok(());
        }
    };
    
    if !instance.exports.is_empty() {
        println!("Exports:");
        let mut exports: Vec<_> = instance.exports.iter().collect();
        exports.sort_by_key(|(name, _)| name.as_str());
        
        for (name, export) in exports {
            match export {
                ExportValue::Function(func) => {
                    println!("  {} : function (params: {}, result: {})", name, func.ty.n_params(), if func.ty.has_result() { "yes" } else { "no" });
                }
                ExportValue::Table(table) => {
                    let t = table.borrow();
                    println!("  {} : table [size: {}, max: {}]", name, t.size(), t.max());
                }
                ExportValue::Memory(mem) => {
                    let m = mem.borrow();
                    println!("  {} : memory [pages: {}, max: {}]", name, m.size(), m.max());
                }
                ExportValue::Global(global) => {
                    let g = global.borrow();
                    let mutability = if g.mutable { "mut " } else { "" };
                    println!("  {} : global {}{}", name, mutability, format_type(&g.ty));
                }
            }
        }
        println!();
    } else if !args.imports_only {
        println!("Exports: none");
        println!();
    }
    
    if args.verbose && !args.exports_only && !args.imports_only {
        println!("Module details:");
        println!("  Functions: {} total", module.functions.len());
        
        let imported_funcs = module.functions.iter().filter(|f| f.import.is_some()).count();
        let defined_funcs = module.functions.len() - imported_funcs;
        if imported_funcs > 0 {
            println!("    - {} imported", imported_funcs);
        }
        if defined_funcs > 0 {
            println!("    - {} defined", defined_funcs);
        }
        
        if module.memory.is_some() {
            let mem = module.memory.as_ref().unwrap();
            println!("  Memory: {} pages (min), {} pages (max)", mem.min, mem.max);
        }
        
        if module.table.is_some() {
            let table = module.table.as_ref().unwrap();
            println!("  Table: {} elements (min), {} elements (max)", table.min, table.max);
        }
        
        if !module.globals.is_empty() {
            println!("  Globals: {}", module.globals.len());
        }
        
        if module.start != u32::MAX {
            println!("  Start function: index {}", module.start);
        }
        
        if module.n_data > 0 {
            println!("  Data segments: {}", module.n_data);
        }
        
        println!("  Type signatures: {}", module.types.len());
        if args.verbose && !module.types.is_empty() {
            for (i, sig) in module.types.iter().enumerate() {
                println!("    [{}] {}", i, format_signature(&sig.params, sig.result));
            }
        }
    }
    
    Ok(())
}