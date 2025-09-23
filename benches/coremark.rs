use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use wagmi::{ExportValue, Imports, Instance, Module, RuntimeFunction, ValType, WasmValue};

fn vt_name(v: ValType) -> &'static str {
    match v {
        ValType::I32 => "i32",
        ValType::I64 => "i64",
        ValType::F32 => "f32",
        ValType::F64 => "f64",
        ValType::Any => "any",
    }
}

fn clock_ms_i64() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Clock may have gone backwards")
        .as_millis() as i64
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let coremark_bytes = include_bytes!("coremark-minimal.wasm");

    let module = Module::compile(coremark_bytes.to_vec())
        .map_err(|e| format!("Failed to compile coremark module: {:?}", e))?;

    {
        println!("Imports:");
        for (mod_name, fields) in &module.imports {
            for (field, et) in fields {
                let kind = match et { wagmi::module::ExternType::Func => "func", wagmi::module::ExternType::Table => "table", wagmi::module::ExternType::Mem => "mem", wagmi::module::ExternType::Global => "global" };
                println!("  {}::{} ({})", mod_name, field, kind);
            }
        }
        
        for (idx, func) in module.functions.iter().enumerate() {
            if let Some(import) = &func.import {
                let params: Vec<&str> = func.ty.params.iter().copied().map(vt_name).collect();
                let res = func.ty.result.map(vt_name);
                println!(
                    "  import func #{} {}::{} (params=[{}], result={})",
                    idx,
                    import.module,
                    import.field,
                    params.join(", "),
                    res.unwrap_or("void")
                );
            }
        }
        if let Some(mem) = &module.memory {
            if let Some(import) = &mem.import { println!("  imports memory {}::{} min={} max={}", import.module, import.field, mem.min, mem.max); }
        }
        if let Some(table) = &module.table {
            if let Some(import) = &table.import { println!("  imports table {}::{} min={} max={}", import.module, import.field, table.min, table.max); }
        }
    }
    let module = Rc::new(module);

    let clock_fn = RuntimeFunction::new_host(
        vec![],
        Some(ValType::I64),
        move |_args| Some(WasmValue::from_i64(clock_ms_i64())),
    );
    let mut imports: Imports = Imports::new();
    let mut env_mod: HashMap<String, ExportValue> = HashMap::new();
    env_mod.insert("clock_ms".to_string(), ExportValue::Function(clock_fn));
    imports.insert("env".to_string(), env_mod);

    let instance = Instance::instantiate(module, &imports)
        .map_err(|e| format!("Failed to instantiate coremark module: {:?}", e))?;

    let run_fn = match instance.exports.get("run") {
        Some(ExportValue::Function(f)) => f,
        _ => return Err("Export 'run' not found or not a function".into()),
    };

    println!("Running Coremark minimal with WAGMI... (this may take a while)");
    let t0 = Instant::now();
    let results = instance
        .invoke(run_fn, &[])
        .map_err(|e| format!("Execution failed: {:?}", e))?;
    let elapsed = t0.elapsed();

    let score = results[0].as_f32();
    println!("Result: {} (elapsed {:.3}s)", score, elapsed.as_secs_f64());

    Ok(())
}


