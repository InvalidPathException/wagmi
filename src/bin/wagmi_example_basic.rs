use std::rc::Rc;
use wagmi::{Module, Instance, Imports, WasmValue};

mod utils;
use utils::load_resource_module;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arithmetic_bytes = load_resource_module("arithmetic")?;
    let arithmetic_module = Module::compile(arithmetic_bytes)?;
    let arithmetic_module = Rc::new(arithmetic_module);
    let imports = Imports::new();
    let arithmetic_instance = Instance::instantiate(arithmetic_module, &imports)?;
    
    
    if let Some(wagmi::ExportValue::Function(add)) = arithmetic_instance.exports.get("add") {
        let result = arithmetic_instance.invoke(add, &[WasmValue::from_i32(10), WasmValue::from_i32(32)])?;
        println!("add(10, 32) = {}", result[0].as_i32());
    }
    
    if let Some(wagmi::ExportValue::Function(sub)) = arithmetic_instance.exports.get("subtract") {
        let result = arithmetic_instance.invoke(sub, &[WasmValue::from_i32(100), WasmValue::from_i32(58)])?;
        println!("subtract(100, 58) = {}", result[0].as_i32());
    }
    
    if let Some(wagmi::ExportValue::Function(mul)) = arithmetic_instance.exports.get("multiply") {
        let result = arithmetic_instance.invoke(mul, &[WasmValue::from_i32(6), WasmValue::from_i32(7)])?;
        println!("multiply(6, 7) = {}", result[0].as_i32());
    }
    
    if let Some(wagmi::ExportValue::Function(div)) = arithmetic_instance.exports.get("divide") {
        let result = arithmetic_instance.invoke(div, &[WasmValue::from_i32(84), WasmValue::from_i32(2)])?;
        println!("divide(84, 2) = {}", result[0].as_i32());
    }
    
    if let Some(wagmi::ExportValue::Function(modulo)) = arithmetic_instance.exports.get("modulo") {
        let result = arithmetic_instance.invoke(modulo, &[WasmValue::from_i32(10), WasmValue::from_i32(3)])?;
        println!("modulo(10, 3) = {}", result[0].as_i32());
    }
    
    
    let factorial_bytes = load_resource_module("factorial")?;
    let factorial_module = Module::compile(factorial_bytes)?;
    let factorial_module = Rc::new(factorial_module);
    let factorial_instance = Instance::instantiate(factorial_module, &imports)?;
    
    if let Some(wagmi::ExportValue::Function(factorial)) = factorial_instance.exports.get("factorial") {
        for n in [0, 1, 5, 10] {
            let result = factorial_instance.invoke(factorial, &[WasmValue::from_i32(n)])?;
            println!("factorial({}) = {}", n, result[0].as_i32());
        }
    }
    
    
    let control_bytes = load_resource_module("control_flow")?;
    let control_module = Module::compile(control_bytes)?;
    let control_module = Rc::new(control_module);
    let control_instance = Instance::instantiate(control_module, &imports)?;
    
    
    if let Some(wagmi::ExportValue::Function(fib)) = control_instance.exports.get("fibonacci") {
        for n in [0, 1, 2, 5, 10] {
            let result = control_instance.invoke(fib, &[WasmValue::from_i32(n)])?;
            println!("fibonacci({}) = {}", n, result[0].as_i32());
        }
    }
    
    if let Some(wagmi::ExportValue::Function(max)) = control_instance.exports.get("max") {
        let result = control_instance.invoke(max, &[WasmValue::from_i32(42), WasmValue::from_i32(17)])?;
        println!("max(42, 17) = {}", result[0].as_i32());
    }
    
    if let Some(wagmi::ExportValue::Function(min)) = control_instance.exports.get("min") {
        let result = control_instance.invoke(min, &[WasmValue::from_i32(42), WasmValue::from_i32(17)])?;
        println!("min(42, 17) = {}", result[0].as_i32());
    }
    
    if let Some(wagmi::ExportValue::Function(abs)) = control_instance.exports.get("abs") {
        let result = control_instance.invoke(abs, &[WasmValue::from_i32(-42)])?;
        println!("abs(-42) = {}", result[0].as_i32());
    }
    
    if let Some(wagmi::ExportValue::Function(sign)) = control_instance.exports.get("sign") {
        for n in [-42, 0, 42] {
            let result = control_instance.invoke(sign, &[WasmValue::from_i32(n)])?;
            println!("sign({}) = {}", n, result[0].as_i32());
        }
    }
    
    
    let memory_bytes = load_resource_module("memory_ops")?;
    let memory_module = Module::compile(memory_bytes)?;
    let memory_module = Rc::new(memory_module);
    let memory_instance = Instance::instantiate(memory_module, &imports)?;
    
    
    if let (Some(wagmi::ExportValue::Function(store)), Some(wagmi::ExportValue::Function(load))) = 
        (memory_instance.exports.get("store_i32"), memory_instance.exports.get("load_i32")) {
        
        memory_instance.invoke(store, &[WasmValue::from_i32(0), WasmValue::from_i32(42)])?;
        println!("Stored 42 at offset 0");
        
        let result = memory_instance.invoke(load, &[WasmValue::from_i32(0)])?;
        println!("Loaded from offset 0: {}", result[0].as_i32());
    }
    
    if let Some(wagmi::ExportValue::Function(memset)) = memory_instance.exports.get("memset") {
        memory_instance.invoke(memset, &[WasmValue::from_i32(100), WasmValue::from_i32(0xFF), WasmValue::from_i32(10)])?;
        println!("Filled 10 bytes at offset 100 with 0xFF");
    }
    
    Ok(())
}