use std::rc::Rc;
use std::cell::RefCell;
use std::collections::HashMap;
use wagmi::{Module, Instance, Imports, ExportValue, WasmValue, RuntimeFunction, RuntimeType, Signature, ValType};

mod utils;
use utils::load_resource_module;

struct HostState {
    call_count: RefCell<u32>,
    last_printed: RefCell<Option<i32>>,
}

fn create_host_print(state: Rc<HostState>) -> RuntimeFunction {
    let signature = Signature {
        params: vec![ValType::I32],
        result: None,
    };
    
    let host_fn = Rc::new(move |args: &mut [WasmValue]| {
        let value = args[0].as_i32();
        println!("  [Host Print] Value: {}", value);
        *state.last_printed.borrow_mut() = Some(value);
        *state.call_count.borrow_mut() += 1;
    });
    
    RuntimeFunction {
        ty: RuntimeType::from_signature(&signature),
        pc_start: None,
        locals_count: 0,
        owner: None,
        owner_idx: None,
        host: Some(host_fn),
    }
}

fn create_host_random() -> RuntimeFunction {
    let signature = Signature {
        params: vec![],
        result: Some(ValType::I32),
    };
    
    let host_fn = Rc::new(move |args: &mut [WasmValue]| {
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .subsec_nanos() as i32;
        let random = (seed ^ 0x5DEECE66Di64 as i32) % 100;
        println!("  [Host Random] Generated: {}", random);
        args[0] = WasmValue::from_i32(random);
    });
    
    RuntimeFunction {
        ty: RuntimeType::from_signature(&signature),
        pc_start: None,
        locals_count: 0,
        owner: None,
        owner_idx: None,
        host: Some(host_fn),
    }
}

fn create_host_add() -> RuntimeFunction {
    let signature = Signature {
        params: vec![ValType::I32, ValType::I32],
        result: Some(ValType::I32),
    };
    
    let host_fn = Rc::new(move |args: &mut [WasmValue]| {
        let a = args[0].as_i32();
        let b = args[1].as_i32();
        let result = a + b;
        println!("  [Host Add] {} + {} = {}", a, b, result);
        args[0] = WasmValue::from_i32(result);
    });
    
    RuntimeFunction {
        ty: RuntimeType::from_signature(&signature),
        pc_start: None,
        locals_count: 0,
        owner: None,
        owner_idx: None,
        host: Some(host_fn),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let host_state = Rc::new(HostState {
        call_count: RefCell::new(0),
        last_printed: RefCell::new(None),
    });
    
    let print_fn = create_host_print(host_state.clone());
    let random_fn = create_host_random();
    let add_fn = create_host_add();
    
    let mut imports = Imports::new();
    let mut host_module = HashMap::new();
    host_module.insert("print".to_string(), ExportValue::Function(print_fn));
    host_module.insert("random".to_string(), ExportValue::Function(random_fn));
    host_module.insert("add".to_string(), ExportValue::Function(add_fn));
    imports.insert("host".to_string(), host_module);
    
    let wasm_bytes = load_resource_module("host_imports")?;
    let module = Module::compile(wasm_bytes)?;
    let module = Rc::new(module);
    let instance = Instance::instantiate(module, &imports)?;
    
    
    if let Some(ExportValue::Function(main_func)) = instance.exports.get("main") {
        let results = instance.invoke(main_func, &[])?;
        println!("main() returned: {}", results[0].as_i32());
    }
    
    if let Some(ExportValue::Function(print_random)) = instance.exports.get("print_random") {
        let results = instance.invoke(print_random, &[])?;
        println!("print_random() returned: {}", results[0].as_i32());
        
        let results = instance.invoke(print_random, &[])?;
        println!("print_random() returned: {}", results[0].as_i32());
    }
    
    if let Some(ExportValue::Function(random_calc)) = instance.exports.get("random_calculation") {
        let results = instance.invoke(random_calc, &[])?;
        println!("random_calculation() returned: {}", results[0].as_i32());
    }
    
    
    println!("Total host.print() calls: {}", *host_state.call_count.borrow());
    if let Some(last) = *host_state.last_printed.borrow() {
        println!("Last printed value: {}", last);
    }
    
    Ok(())
}