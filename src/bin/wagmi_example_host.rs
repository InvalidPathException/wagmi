use std::rc::Rc;
use std::cell::RefCell;
use std::collections::HashMap;
use wagmi::{Module, Instance, Imports, ExportValue, WasmValue, RuntimeFunction, ValType};

mod utils;
use utils::load_resource_module;

struct HostState {
    call_count: RefCell<u32>,
    counter: RefCell<i32>,
    call_sequence: RefCell<Vec<String>>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let host_state = Rc::new(HostState {
        call_count: RefCell::new(0),
        counter: RefCell::new(0),
        call_sequence: RefCell::new(Vec::new()),
    });
    
    let state_clone = host_state.clone();
    let print_fn = RuntimeFunction::new_host(
        vec![ValType::I32],
        None,
        move |args| {
            let value = args[0].as_i32();
            println!("  [Host:print] {}", value);
            state_clone.call_sequence.borrow_mut().push(format!("print({})", value));
            *state_clone.call_count.borrow_mut() += 1;
            None
        }
    );
    
    let state_clone = host_state.clone();
    let random_fn = RuntimeFunction::new_host(
        vec![],
        Some(ValType::I32),
        move |_args| {
            use std::time::{SystemTime, UNIX_EPOCH};
            let seed = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .subsec_nanos() as i32;
            let random = ((seed ^ 0x5DEECE66Di64 as i32) % 50).abs();
            println!("  [Host:random] → {}", random);
            state_clone.call_sequence.borrow_mut().push(format!("random() -> {}", random));
            *state_clone.call_count.borrow_mut() += 1;
            Some(WasmValue::from_i32(random))
        }
    );
    
    let state_clone = host_state.clone();
    let add_fn = RuntimeFunction::new_host(
        vec![ValType::I32, ValType::I32],
        Some(ValType::I32),
        move |args| {
            let a = args[0].as_i32();
            let b = args[1].as_i32();
            let result = a + b;
            println!("  [Host:add] {} + {} = {}", a, b, result);
            state_clone.call_sequence.borrow_mut().push(format!("add({}, {}) -> {}", a, b, result));
            *state_clone.call_count.borrow_mut() += 1;
            Some(WasmValue::from_i32(result))
        }
    );
    
    let state_clone = host_state.clone();
    let mul_fn = RuntimeFunction::new_host(
        vec![ValType::I32, ValType::I32],
        Some(ValType::I32),
        move |args| {
            let a = args[0].as_i32();
            let b = args[1].as_i32();
            let result = a * b;
            println!("  [Host:mul] {} * {} = {}", a, b, result);
            state_clone.call_sequence.borrow_mut().push(format!("mul({}, {}) -> {}", a, b, result));
            *state_clone.call_count.borrow_mut() += 1;
            Some(WasmValue::from_i32(result))
        }
    );
    
    let state_clone = host_state.clone();
    let counter_inc_fn = RuntimeFunction::new_host(
        vec![],
        Some(ValType::I32),
        move |_args| {
            let mut counter = state_clone.counter.borrow_mut();
            *counter += 1;
            let value = *counter;
            println!("  [Host:counter++] → {}", value);
            state_clone.call_sequence.borrow_mut().push(format!("counter++ -> {}", value));
            *state_clone.call_count.borrow_mut() += 1;
            Some(WasmValue::from_i32(value))
        }
    );
    
    let state_clone = host_state.clone();
    let counter_get_fn = RuntimeFunction::new_host(
        vec![],
        Some(ValType::I32),
        move |_args| {
            let value = *state_clone.counter.borrow();
            println!("  [Host:counter] → {}", value);
            state_clone.call_sequence.borrow_mut().push(format!("counter -> {}", value));
            *state_clone.call_count.borrow_mut() += 1;
            Some(WasmValue::from_i32(value))
        }
    );
    
    let mut imports = Imports::new();
    let mut host_module = HashMap::new();
    host_module.insert("print".to_string(), ExportValue::Function(print_fn));
    host_module.insert("random".to_string(), ExportValue::Function(random_fn));
    host_module.insert("add".to_string(), ExportValue::Function(add_fn));
    host_module.insert("mul".to_string(), ExportValue::Function(mul_fn));
    host_module.insert("counter_inc".to_string(), ExportValue::Function(counter_inc_fn));
    host_module.insert("counter_get".to_string(), ExportValue::Function(counter_get_fn));
    imports.insert("host".to_string(), host_module);
    
    let wasm_bytes = load_resource_module("host_imports")?;
    let module = Module::compile(wasm_bytes)?;
    let module = Rc::new(module);
    let instance = Instance::instantiate(module, &imports)?;
    
    if let Some(ExportValue::Function(main_func)) = instance.exports.get("main") {
        println!("Calling main():");
        let results = instance.invoke(main_func, &[])?;
        println!("→ returned: {}\n", results[0].as_i32());
    }
    
    if let Some(ExportValue::Function(func)) = instance.exports.get("sequence") {
        println!("Calling sequence():");
        let results = instance.invoke(func, &[])?;
        println!("→ returned: {}\n", results[0].as_i32());
    }
    
    if let Some(ExportValue::Function(func)) = instance.exports.get("nested_calls") {
        println!("Calling nested_calls():");
        let results = instance.invoke(func, &[])?;
        println!("→ returned: {}\n", results[0].as_i32());
    }
    
    if let Some(ExportValue::Function(func)) = instance.exports.get("stateful") {
        println!("Calling stateful():");
        let results = instance.invoke(func, &[])?;
        println!("→ returned: {}\n", results[0].as_i32());
    }
    
    println!("=== Summary ===");
    println!("Total host calls: {}", *host_state.call_count.borrow());
    println!("Final counter: {}", *host_state.counter.borrow());
    
    if !host_state.call_sequence.borrow().is_empty() {
        println!("\nCall sequence:");
        for (i, call) in host_state.call_sequence.borrow().iter().enumerate() {
            println!("  {}. {}", i + 1, call);
        }
    }
    
    Ok(())
}