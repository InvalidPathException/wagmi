use std::{collections::HashMap, fs, path::Path, process::Command, rc::{Rc}, cell::RefCell, env};
use serde::Deserialize;
use wagmi::{Module, Instance, Imports, ExportValue, WasmValue, WasmGlobal, WasmTable, WasmMemory, RuntimeFunction, RuntimeType, ValType, Error, Signature};

#[derive(Deserialize, Clone)]
struct ValueJSON { 
    r#type: String, 
    value: String 
}

#[derive(Deserialize, Clone)]
#[serde(tag = "type")]
enum Act {
    #[serde(rename = "get")]
    Get { module: Option<String>, field: String },
    #[serde(rename = "invoke")]
    Invoke { module: Option<String>, field: String, args: Vec<ValueJSON> },
}

#[allow(dead_code)]
#[derive(Deserialize, Clone)]
#[serde(tag = "type")]
enum TestCmd {
    #[serde(rename = "module")]
    Module { line: i32, name: Option<String>, filename: String },
    #[serde(rename = "register")]
    Register { line: i32, name: Option<String>, r#as: String },
    #[serde(rename = "action")]
    Action { line: i32, action: Act, expected: Option<Vec<ValueJSON>> },
    #[serde(rename = "assert_return")]
    AssertReturn { line: i32, action: Act, expected: Vec<ValueJSON> },
    #[serde(rename = "assert_trap")]
    AssertTrap { line: i32, action: Act, text: String },
    #[serde(rename = "assert_exhaustion")]
    AssertExhaustion { line: i32, action: Act, text: String },
    #[serde(rename = "assert_malformed")]
    AssertMalformed { line: i32, filename: String, text: String, module_type: String },
    #[serde(rename = "assert_invalid")]
    AssertInvalid { line: i32, filename: String, text: String, module_type: String },
    #[serde(rename = "assert_unlinkable")]
    AssertUnlinkable { line: i32, filename: String, text: String, module_type: String },
    #[serde(rename = "assert_uninstantiable")]
    AssertUninstantiable { line: i32, filename: String, text: String, module_type: String },
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct WastJSON { 
    source_filename: String,
    commands: Vec<TestCmd> 
}

fn to_wasm_values(values: &[ValueJSON]) -> Vec<WasmValue> {
    values.iter().map(|v| match v.r#type.as_str() {
        "i32" => WasmValue::from_u32(v.value.parse().unwrap()),
        "i64" => WasmValue::from_u64(v.value.parse().unwrap()),
        "f32" => {
            let bits = if v.value.starts_with("nan:") { 
                0x7fc0_0000u32 
            } else { 
                v.value.parse().unwrap() 
            };
            WasmValue::from_f32_bits(bits)
        }
        "f64" => {
            let bits = if v.value.starts_with("nan:") { 
                0x7ff8_0000_0000_0000u64 
            } else { 
                v.value.parse().unwrap() 
            };
            WasmValue::from_f64_bits(bits)
        }
        _ => panic!("unknown value type: {}", v.r#type)
    }).collect()
}

fn externalized_exports_for(inst: &Rc<Instance>) -> HashMap<String, ExportValue> {
    let mut out = inst.exports.clone();
    let weak = Rc::downgrade(inst);
    for (name, ex) in &inst.module.exports {
        if matches!(ex.extern_type, wagmi::module::ExternType::Func) {
            let fi = ex.idx as usize;
            let src = inst.functions[fi].clone();
            if src.host.is_some() {
                out.insert(name.clone(), ExportValue::Function(src));
            } else {
                // For wasm-backed exports, expose an owner handle that
                // delegates execution into the owning instance
                let ty = src.ty;
                out.insert(name.clone(), ExportValue::Function(RuntimeFunction {
                    ty,
                    pc_start: None,
                    locals_count: 0,
                    host: None,
                    owner: Some(weak.clone()),
                    owner_idx: Some(fi),
                }));
            }
        }
    }
    out
}

fn spectest_exports() -> HashMap<String, ExportValue> {
    
    let mut exports = HashMap::new();

    exports.insert("global_i32".into(), ExportValue::Global(Rc::new(RefCell::new(
        WasmGlobal { ty: ValType::I32, mutable: false, value: WasmValue::from_u32(666) }
    ))));
    exports.insert("global_i64".into(), ExportValue::Global(Rc::new(RefCell::new(
        WasmGlobal { ty: ValType::I64, mutable: false, value: WasmValue::from_u64(666) }
    ))));
    exports.insert("global_f32".into(), ExportValue::Global(Rc::new(RefCell::new(
        WasmGlobal { ty: ValType::F32, mutable: false, value: WasmValue::from_f32(666.6) }
    ))));
    exports.insert("global_f64".into(), ExportValue::Global(Rc::new(RefCell::new(
        WasmGlobal { ty: ValType::F64, mutable: false, value: WasmValue::from_f64(666.6) }
    ))));

    exports.insert("table".into(), ExportValue::Table(Rc::new(RefCell::new(
        WasmTable::new(10, 20)
    ))));
    exports.insert("memory".into(), ExportValue::Memory(Rc::new(RefCell::new(
        WasmMemory::new(1, 2)
    ))));
    
    // Print functions (no-ops for testing)
    let make_fn = |sig: Signature| {
        let ty = RuntimeType::from_signature(&sig);
        ExportValue::Function(RuntimeFunction {
            ty, 
            pc_start: None,
            locals_count: 0, 
            host: Some(Rc::new(|_| {})), 
            owner: None, 
            owner_idx: None
        })
    };
    
    exports.insert("print".into(), make_fn(Signature { params: vec![], result: None }));
    exports.insert("print_i32".into(), make_fn(Signature { params: vec![ValType::I32], result: None }));
    exports.insert("print_i64".into(), make_fn(Signature { params: vec![ValType::I64], result: None }));
    exports.insert("print_f32".into(), make_fn(Signature { params: vec![ValType::F32], result: None }));
    exports.insert("print_f64".into(), make_fn(Signature { params: vec![ValType::F64], result: None }));
    exports.insert("print_i32_f32".into(), make_fn(Signature { params: vec![ValType::I32, ValType::F32], result: None }));
    exports.insert("print_f64_f64".into(), make_fn(Signature { params: vec![ValType::F64, ValType::F64], result: None }));
    
    exports
}

fn exec_action(instances: &HashMap<String, Rc<Instance>>, action: &Act) -> Result<Vec<WasmValue>, Error> {
    let (module_name, field, args) = match action {
        Act::Get { module, field } => (module.as_deref().unwrap_or("default"), field.as_str(), vec![]),
        Act::Invoke { module, field, args } => (module.as_deref().unwrap_or("default"), field.as_str(), to_wasm_values(args)),
    };
    
    let inst = instances.get(module_name).ok_or(Error::Trap("module not found"))?;
    let export = inst.exports.get(field).ok_or(Error::Trap("export not found"))?;
    
    match action {
        Act::Get { .. } => match export {
            ExportValue::Global(g) => Ok(vec![g.borrow().value]),
            _ => Err(Error::Trap("not a global"))
        },
        Act::Invoke { .. } => match export {
            ExportValue::Function(f) => inst.invoke(f, &args),
            _ => Err(Error::Trap("not a function"))
        }
    }
}

fn check_results(results: &[WasmValue], expected: &[ValueJSON]) -> Result<(), String> {
    let exp_values = to_wasm_values(expected);
    
    if results.len() != exp_values.len() {
        return Err(format!("result count mismatch: expected {}, got {}", exp_values.len(), results.len()));
    }
    
    for (i, ((result, exp_val), exp_json)) in results.iter().zip(&exp_values).zip(expected).enumerate() {
        // Exact bit match first
        if result.as_u64() == exp_val.as_u64() {
            continue;
        }
        
        // Check if both are NaN for float types
        let both_nan = match exp_json.r#type.as_str() {
            "f32" => {
                let r = f32::from_bits(result.as_f32_bits());
                let e = f32::from_bits(exp_val.as_f32_bits());
                r.is_nan() && e.is_nan()
            }
            "f64" => {
                let r = f64::from_bits(result.as_f64_bits());
                let e = f64::from_bits(exp_val.as_f64_bits());
                r.is_nan() && e.is_nan()
            }
            _ => false
        };
        
        if !both_nan {
            return Err(format!("result[{}] mismatch", i));
        }
    }
    
    Ok(())
}

fn run_test_file(json_path: &Path, wast_name: &str) -> Result<(u32, u32, u32), String> {
    let json_text = fs::read_to_string(json_path)
        .map_err(|e| format!("failed to read json: {}", e))?;
    let wast: WastJSON = serde_json::from_str(&json_text)
        .map_err(|e| format!("failed to parse json: {}", e))?;
    
    let mut instances: HashMap<String, Rc<Instance>> = HashMap::new();
    // Keep strong references to registered instances so cross-module imports remain valid
    let mut keepalive: Vec<Rc<Instance>> = Vec::new();
    let mut imports: Imports = HashMap::new();
    imports.insert("spectest".to_string(), spectest_exports());
    
    let base_dir = json_path.parent().unwrap();
    let mut passes = 0u32;
    let mut message_mismatches = 0u32;
    let mut failures = 0u32;
    
    for cmd in &wast.commands {
        let result = match cmd {
            TestCmd::Module { name, filename, .. } => {
                let wasm_path = base_dir.join(filename);
                let bytes = fs::read(&wasm_path)
                    .map_err(|e| format!("failed to read wasm: {}", e))?;
                let module = Module::compile(bytes)
                    .map_err(|e| format!("compile failed: {}", e))?;
                let module_rc = Rc::new(module);
                let inst = Instance::instantiate(module_rc, &imports)
                    .map_err(|e| format!("instantiate failed: {}", e))?;
                
                let inst_rc = Rc::new(inst);
                // Re-register with the new Rc wrapper
                Instance::register_external_instance(&inst_rc);
                
                if let Some(n) = name {
                    instances.insert(n.clone(), inst_rc.clone());
                }
                instances.insert("default".to_string(), inst_rc);
                Ok(())
            }
            
            TestCmd::Register { name, r#as, .. } => {
                let key = name.as_deref().unwrap_or("default");
                let inst = instances.get(key)
                    .ok_or_else(|| format!("module '{}' not found", key))?;
                let ex = externalized_exports_for(inst);
                imports.insert(r#as.clone(), ex);
                // Keep the instance alive for the duration of the run
                // to ensure Weak owners can be upgraded
                keepalive.push(inst.clone());
                Ok(())
            }
            
            TestCmd::Action { action, .. } => {
                exec_action(&instances, action).map(|_| ()).map_err(|e| e.to_string())
            }
            
            TestCmd::AssertReturn { action, expected, .. } => {
                exec_action(&instances, action)
                    .map_err(|e| e.to_string())
                    .and_then(|results| check_results(&results, expected))
            }
            
            TestCmd::AssertTrap { action, text, .. } => {
                match exec_action(&instances, action) {
                    Err(Error::Trap(msg)) => {
                        if msg == text || msg.starts_with(text) {
                            Ok(())  // Exact match or starts with expected
                        } else if text.starts_with(&msg) || 
                                  (text.starts_with("uninitialized") && msg == "unreachable") ||
                                  (text.starts_with("undefined") && msg == "unreachable") {
                            Err(format!("message mismatch: expected '{}', got '{}'", text, msg))
                        } else {
                            Err(format!("message mismatch: expected '{}', got '{}'", text, msg))
                        }
                    }
                    Err(_) => Err(format!("wrong error type, expected trap: '{}'", text)),
                    Ok(_) => Err(format!("expected trap: '{}'", text))
                }
            }
            
            TestCmd::AssertExhaustion { action, .. } => {
                match exec_action(&instances, action) {
                    Err(Error::Trap(msg)) if msg == "call stack exhausted" => Ok(()),
                    _ => Err("expected exhaustion".into())
                }
            }
            
            TestCmd::AssertMalformed { filename, text, module_type, .. } => {
                if module_type != "binary" {
                    Ok(()) // Skip non-binary tests
                } else {
                    let wasm_path = base_dir.join(filename);
                    match fs::read(&wasm_path).ok().and_then(|b| Module::compile(b).err()) {
                        Some(Error::Malformed(msg)) => {
                            if msg == text || msg.starts_with(text) {
                                Ok(())  // Exact match or starts with expected
                            } else {
                                Err(format!("message mismatch: expected '{}', got '{}'", text, msg))
                            }
                        }
                        _ => Err(format!("expected malformed: '{}'", text))
                    }
                }
            }
            
            TestCmd::AssertInvalid { filename, text, .. } => {
                let wasm_path = base_dir.join(filename);
                let bytes = fs::read(&wasm_path).map_err(|e| format!("read failed: {}", e))?;
                match Module::compile(bytes) {
                    Err(Error::Validation(msg)) => {
                        if msg == text {
                            Ok(())  // Exact match
                        } else {
                            Err(format!("message mismatch: expected '{}', got '{}'", text, msg))
                        }
                    }
                    Ok(m) => match Instance::instantiate(Rc::new(m), &imports) {
                        Err(Error::Validation(msg)) => {
                            if msg == text {
                                Ok(())  // Exact match
                            } else {
                                Err(format!("message mismatch: expected '{}', got '{}'", text, msg))
                            }
                        }
                        _ => Err(format!("expected validation error: '{}'", text))
                    },
                    _ => Err(format!("expected validation error: '{}'", text))
                }
            }
            
            TestCmd::AssertUnlinkable { filename, text, .. } => {
                let wasm_path = base_dir.join(filename);
                match fs::read(&wasm_path).ok().and_then(|b| Module::compile(b).ok()) {
                    Some(m) => match Instance::instantiate(Rc::new(m), &imports) {
                        Err(Error::Link(msg)) => {
                            if msg == text {
                                Ok(())  // Exact match
                            } else {
                                Err(format!("message mismatch: expected '{}', got '{}'", text, msg))
                            }
                        }
                        _ => Err(format!("expected unlinkable: '{}'", text))
                    },
                    _ => Err("failed to compile".into())
                }
            }
            
            TestCmd::AssertUninstantiable { filename, text, .. } => {
                let wasm_path = base_dir.join(filename);
                match fs::read(&wasm_path).ok().and_then(|b| Module::compile(b).ok()) {
                    Some(m) => match Instance::instantiate(Rc::new(m), &imports) {
                        Err(Error::Uninstantiable(msg)) => {
                            if msg == text {
                                Ok(())  // Exact match
                            } else {
                                Err(format!("message mismatch: expected '{}', got '{}'", text, msg))
                            }
                        }
                        _ => Err(format!("expected uninstantiable: '{}'", text))
                    },
                    _ => Err("failed to compile".into())
                }
            }
        };
        
        match result {
            Ok(()) => passes += 1,
            Err(e) if e.starts_with("message mismatch") => {
                message_mismatches += 1;
                eprintln!("[{}:{}] {}", wast_name,
                    match cmd {
                        TestCmd::Module { line, .. } => line,
                        TestCmd::Register { line, .. } => line,
                        TestCmd::Action { line, .. } => line,
                        TestCmd::AssertReturn { line, .. } => line,
                        TestCmd::AssertTrap { line, .. } => line,
                        TestCmd::AssertExhaustion { line, .. } => line,
                        TestCmd::AssertMalformed { line, .. } => line,
                        TestCmd::AssertInvalid { line, .. } => line,
                        TestCmd::AssertUnlinkable { line, .. } => line,
                        TestCmd::AssertUninstantiable { line, .. } => line,
                    }, e);
            }
            Err(e) => {
                eprintln!("[{}:{}] {}: {}", 
                    wast_name,
                    match cmd {
                        TestCmd::Module { line, .. } => line,
                        TestCmd::Register { line, .. } => line,
                        TestCmd::Action { line, .. } => line,
                        TestCmd::AssertReturn { line, .. } => line,
                        TestCmd::AssertTrap { line, .. } => line,
                        TestCmd::AssertExhaustion { line, .. } => line,
                        TestCmd::AssertMalformed { line, .. } => line,
                        TestCmd::AssertInvalid { line, .. } => line,
                        TestCmd::AssertUnlinkable { line, .. } => line,
                        TestCmd::AssertUninstantiable { line, .. } => line,
                    },
                    match cmd {
                        TestCmd::Module { .. } => "module",
                        TestCmd::Register { .. } => "register",
                        TestCmd::Action { .. } => "action",
                        TestCmd::AssertReturn { .. } => "assert_return",
                        TestCmd::AssertTrap { .. } => "assert_trap",
                        TestCmd::AssertExhaustion { .. } => "assert_exhaustion",
                        TestCmd::AssertMalformed { .. } => "assert_malformed",
                        TestCmd::AssertInvalid { .. } => "assert_invalid",
                        TestCmd::AssertUnlinkable { .. } => "assert_unlinkable",
                        TestCmd::AssertUninstantiable { .. } => "assert_uninstantiable",
                    },
                    e
                );
                failures += 1;
            }
        }
    }
    
    Ok((passes, message_mismatches, failures))
}

#[test]
fn run_spec_tests() {
    let filter = env::var("SPEC_FILTER").ok();
    let test_dir = Path::new("tests/core");
    let wast2json = if cfg!(target_os = "macos") {
        Path::new("tools/osx/wast2json")
    } else if cfg!(target_os = "linux") {
        Path::new("tools/linux/wast2json")
    } else {
        panic!("Unsupported OS for wast2json")
    };
    let tmp_dir = Path::new("tmp/spec-json");
    
    // Create tmp directory if it doesn't exist
    fs::create_dir_all(tmp_dir).expect("failed to create tmp directory");
    
    let mut total_passes = 0u32;
    let mut total_mismatches = 0u32;
    let mut total_failures = 0u32;
    
    for entry in fs::read_dir(test_dir).expect("failed to read test directory") {
        let entry = entry.expect("failed to read entry");
        let path = entry.path();
        
        if path.extension().and_then(|s| s.to_str()) != Some("wast") {
            continue;
        }
        
        let stem = path.file_stem().unwrap().to_str().unwrap();
        
        // Apply filter if set
        if let Some(ref f) = filter {
            if !stem.contains(f) {
                continue;
            }
        }
        
        // Create subdirectory for this test's outputs
        let test_out_dir = tmp_dir.join(stem);
        fs::create_dir_all(&test_out_dir).expect("failed to create test output directory");
        
        // Convert wast to json
        let json_path = test_out_dir.join(format!("{}.json", stem));
        let output = Command::new(wast2json)
            .arg(&path)
            .arg("-o")
            .arg(&json_path)
            .output()
            .expect("failed to run wast2json");
        
        if !output.status.success() {
            eprintln!("wast2json failed for {}: {}", stem, String::from_utf8_lossy(&output.stderr));
            continue;
        }
        
        // Run tests
        println!("Running {}", stem);
        match run_test_file(&json_path, stem) {
            Ok((passes, message_mismatches, failures)) => {
                total_passes += passes;
                total_mismatches += message_mismatches;
                total_failures += failures;
                println!("  {} passed, {} had error message mismatch, {} failed", passes, message_mismatches, failures);
            }
            Err(e) => {
                eprintln!("  Error: {}", e);
                total_failures += 1;
            }
        }
    }
    
    println!("\nTotal: {} passed, {} had error message mismatch, {} failed", total_passes, total_mismatches, total_failures);
    
    if total_failures > 0 || total_mismatches > 0 {
        panic!("{} tests failed and {} tests had error message mismatch", total_failures, total_mismatches);
    }
}