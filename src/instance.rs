use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use crate::error::*;
use crate::leb128::{read_leb128, read_sleb128};
use crate::Module;
use crate::module::ExternType;
use crate::signature::*;
use crate::wasm_memory::WasmMemory;

#[derive(Copy, Clone, Default)]
pub struct WasmValue(pub u64);

impl WasmValue {
    #[inline] pub fn from_i32(v: i32) -> Self { Self(v as u32 as u64) }
    #[inline] pub fn as_i32(self) -> i32 { self.0 as u32 as i32 }
    #[inline] pub fn from_u32(v: u32) -> Self { Self(v as u64) }
    #[inline] pub fn as_u32(self) -> u32 { self.0 as u32 }
    #[inline] pub fn from_i64(v: i64) -> Self { Self(v as u64) }
    #[inline] pub fn as_i64(self) -> i64 { self.0 as i64 }
    #[inline] pub fn from_u64(v: u64) -> Self { Self(v) }
    #[inline] pub fn as_u64(self) -> u64 { self.0 }
    #[inline] pub fn from_f32_bits(bits: u32) -> Self { Self(bits as u64) }
    #[inline] pub fn as_f32_bits(self) -> u32 { self.0 as u32 }
    #[inline] pub fn from_f64_bits(bits: u64) -> Self { Self(bits) }
    #[inline] pub fn as_f64_bits(self) -> u64 { self.0 }
    #[inline] pub fn from_f32(v: f32) -> Self { Self::from_f32_bits(v.to_bits()) }
    #[inline] pub fn as_f32(self) -> f32 { f32::from_bits(self.as_f32_bits()) }
    #[inline] pub fn from_f64(v: f64) -> Self { Self::from_f64_bits(v.to_bits()) }
    #[inline] pub fn as_f64(self) -> f64 { f64::from_bits(self.as_f64_bits()) }
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub struct RuntimeType(u64);

macro_rules! set_type_bit {
    ($bits:expr, $t:expr) => {
        match $t {
            ValType::I32 => $bits |= RuntimeType::HAS_I32,
            ValType::I64 => $bits |= RuntimeType::HAS_I64,
            ValType::F32 => $bits |= RuntimeType::HAS_F32,
            ValType::F64 => $bits |= RuntimeType::HAS_F64,
            _ => unreachable!()
        }
    };
}

impl RuntimeType {
    const HAS_RESULT: u64 = 1 << 32;
    const HAS_I32: u64 = 1 << 33; const HAS_I64: u64 = 1 << 34;
    const HAS_F32: u64 = 1 << 35; const HAS_F64: u64 = 1 << 36;
    #[inline] pub fn n_params(&self) -> u32 { self.0 as u32 }
    #[inline] pub fn has_result(&self) -> bool { (self.0 & Self::HAS_RESULT) != 0 }
    #[inline] pub fn has_i32(&self) -> bool { (self.0 & Self::HAS_I32) != 0 }
    #[inline] pub fn has_i64(&self) -> bool { (self.0 & Self::HAS_I64) != 0 }
    #[inline] pub fn has_f32(&self) -> bool { (self.0 & Self::HAS_F32) != 0 }
    #[inline] pub fn has_f64(&self) -> bool { (self.0 & Self::HAS_F64) != 0 }

    pub fn from_signature(sig: &Signature) -> Self {
        let mut bits: u64 = sig.params.len() as u64;
        if sig.result.is_some() { bits |= Self::HAS_RESULT; }
        for &param in &sig.params { set_type_bit!(bits, param); }
        if let Some(res) = sig.result { set_type_bit!(bits, res); }
        RuntimeType(bits)
    }
}

#[derive(Debug)]
struct FuncRef {
    handle: u64,
}

impl FuncRef {
    const NULL: Self = Self { handle: 0 };

    fn new(owner_id: u32, func_idx: u32) -> Self {
        if owner_id == 0 || func_idx == u32::MAX {
            return Self::NULL;
        }
        // Try to increment refcount, but don't fail if thread local is gone
        let _ = INSTANCE_MANAGER.try_with(|mgr| {
            mgr.borrow_mut().inc_ref(owner_id);
        });
        Self {
            handle: ((owner_id as u64) << 32) | ((func_idx as u64) + 1)
        }
    }

    fn from_raw(handle: u64) -> Self {
        if handle != 0 {
            let owner_id = (handle >> 32) as u32;
            // Try to increment refcount, but don't fail if thread local is gone
            let _ = INSTANCE_MANAGER.try_with(|mgr| {
                mgr.borrow_mut().inc_ref(owner_id);
            });
        }
        Self { handle }
    }

    fn as_raw(&self) -> u64 { self.handle }
    fn owner_id(&self) -> u32 { (self.handle >> 32) as u32 }
}

impl Clone for FuncRef {
    fn clone(&self) -> Self {
        if self.handle != 0 {
            // Use try_with to avoid panicking if thread local is destroyed
            let _ = INSTANCE_MANAGER.try_with(|mgr| {
                mgr.borrow_mut().inc_ref(self.owner_id());
            });
        }
        Self { handle: self.handle }
    }
}

impl Drop for FuncRef {
    fn drop(&mut self) {
        if self.handle != 0 {
            // Use try_with to avoid panicking if thread local is destroyed
            let _ = INSTANCE_MANAGER.try_with(|mgr| {
                mgr.borrow_mut().dec_ref(self.owner_id());
            });
        }
    }
}

impl Default for FuncRef {
    fn default() -> Self { Self::NULL }
}

/// Manages instance registry and reference counting
struct InstanceManager {
    registry: HashMap<u32, Weak<Instance>>,
    refcounts: HashMap<u32, usize>,
    next_id: u32,
    /// Instances that failed to instantiate but have live funcref references
    /// These are kept alive until their refcount drops to zero
    zombie_instances: HashMap<u32, Rc<Instance>>,
}

impl InstanceManager {
    fn new() -> Self {
        Self {
            registry: HashMap::new(),
            refcounts: HashMap::new(),
            next_id: 1,
            zombie_instances: HashMap::new(),
        }
    }

    fn with<R>(f: impl FnOnce(&mut InstanceManager) -> R) -> R {
        INSTANCE_MANAGER.with(|mgr| f(&mut mgr.borrow_mut()))
    }

    fn allocate_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn register_instance(&mut self, inst: &Rc<Instance>) {
        self.registry.insert(inst.id, Rc::downgrade(inst));
    }

    fn get_instance(&self, id: u32) -> Option<Rc<Instance>> {
        // First check if it's a zombie instance (failed instantiation but has live refs)
        if let Some(zombie) = self.zombie_instances.get(&id) {
            return Some(zombie.clone());
        }
        // Otherwise check the normal registry
        self.registry.get(&id).and_then(|w| w.upgrade())
    }

    fn inc_ref(&mut self, owner_id: u32) {
        *self.refcounts.entry(owner_id).or_insert(0) += 1;
    }

    fn dec_ref(&mut self, owner_id: u32) {
        if let Some(count) = self.refcounts.get_mut(&owner_id) {
            if *count > 0 {
                *count -= 1;
                // If refcount drops to zero, remove any zombie instance
                if *count == 0 {
                    self.zombie_instances.remove(&owner_id);
                }
            }
        }
    }

    fn has_refs(&self, owner_id: u32) -> bool {
        self.refcounts.get(&owner_id).copied().unwrap_or(0) > 0
    }

    fn add_zombie(&mut self, inst: Rc<Instance>) {
        if self.has_refs(inst.id) {
            self.zombie_instances.insert(inst.id, inst);
        }
    }
}

thread_local! {
    static INSTANCE_MANAGER: RefCell<InstanceManager> = RefCell::new(InstanceManager::new());
}

pub struct WasmTable {
    elements: Vec<FuncRef>,  // Changed to FuncRef for automatic refcounting
    pub current: u32,
    pub maximum: u32,
}

impl WasmTable {
    pub fn new(initial: u32, maximum: u32) -> Self {
        let mut elements = Vec::new();
        elements.resize(initial as usize, FuncRef::default());
        Self { elements, current: initial, maximum }
    }
    pub fn size(&self) -> u32 { self.current }
    pub fn max(&self) -> u32 { self.maximum }
    pub fn grow(&mut self, delta: u32, value: WasmValue) -> u32 {
        if delta == 0 { return self.current; }
        if delta > self.maximum.saturating_sub(self.current) { return u32::MAX; }
        let new_current = self.current + delta;
        let func_ref = FuncRef::from_raw(value.as_u64());
        self.elements.resize(new_current as usize, func_ref);
        let old = self.current;
        self.current = new_current;
        old
    }
    pub fn get(&self, idx: u32) -> Result<WasmValue, &'static str> {
        let i = idx as usize;
        if i >= self.elements.len() { return Err(OOB_TABLE_ACCESS); }
        Ok(WasmValue::from_u64(self.elements[i].as_raw()))
    }
    pub fn set(&mut self, idx: u32, value: WasmValue) -> Result<(), &'static str> {
        let i = idx as usize;
        if i >= self.elements.len() { return Err(OOB_TABLE_ACCESS); }
        // FuncRef handles refcounting automatically via Drop/Clone
        self.elements[i] = FuncRef::from_raw(value.as_u64());
        Ok(())
    }
}

pub struct WasmGlobal {
    pub ty: ValType,
    pub mutable: bool,
    pub value: WasmValue,
}

// --------------- Imports/Exports and Functions ---------------

#[derive(Clone)]
pub struct FunctionInfo {
    pub ty: RuntimeType,
    pub wasm_fn: Option<usize>,
    pub locals_count: usize,
    pub host: Option<Rc<dyn Fn(&mut [WasmValue])>>,
    // If present, this function represents an external/cross-instance
    // function owned by another instance. The owner is referenced weakly
    // to avoid cycles. When invoked, execution should occur in the owning
    // instance context using the stored index.
    pub owner: Option<Weak<Instance>>,
    pub owner_index: Option<usize>,
}

#[derive(Clone)]
pub enum ExportValue {
    Function(FunctionInfo),
    Table(Rc<RefCell<WasmTable>>),
    Memory(Rc<RefCell<WasmMemory>>),
    Global(Rc<RefCell<WasmGlobal>>),
}

pub type Exports = HashMap<String, ExportValue>;
pub type ModuleImports = HashMap<String, ExportValue>;
pub type Imports = HashMap<String, ModuleImports>;

struct ControlFrame {
    stack_len: usize,
    dest_pc: usize,
    arity: u32,
    has_result: bool,
}

#[derive(Default)]
pub struct Instance {
    pub id: u32,
    pub module: Rc<Module>,
    pub memory: Option<Rc<RefCell<WasmMemory>>>,
    pub table: Option<Rc<RefCell<WasmTable>>>,
    pub globals: Vec<Rc<RefCell<WasmGlobal>>>,
    pub functions: Vec<FunctionInfo>,
    pub exports: Exports,
}

impl Instance {
    pub fn new(module: Rc<Module>) -> Self {
        // Other than the validated module, everything starts empty
        Self { module, ..Default::default() }
    }

    pub fn instantiate(module: Rc<Module>, imports: &Imports) -> Result<Self, Error> {
        // Build the instance inside a Rc so we can register a Weak handle
        // for cross-instance func_ref dispatch even if instantiation ultimately fails.
        let mut inst_rc = Rc::new(Instance::new(module.clone()));
        {
            // Configure the instance while we hold the only strong Rc
            let inst = Rc::get_mut(&mut inst_rc).expect("sole owner expected");
            inst.id = InstanceManager::with(|mgr| mgr.allocate_id());

            // Memory
            if let Some(memory) = &module.memory {
                if let Some(import_ref) = memory.import.clone() {
                    let module = import_ref.module;
                    let field = import_ref.field;
                    let imported = imports.get(&module).and_then(|m| m.get(&field)).ok_or(Error::Link(UNKNOWN_IMPORT))?;
                    match imported {
                        ExportValue::Memory(mem) => {
                            let m = mem.borrow();
                            if m.size() < memory.min || m.max() > memory.max { return Err(Error::Link(INCOMPATIBLE_IMPORT)); }
                            drop(m);
                            inst.memory = Some(mem.clone());
                        }
                        _ => return Err(Error::Link(INCOMPATIBLE_IMPORT)),
                    }
                } else {
                    inst.memory = Some(Rc::new(RefCell::new(WasmMemory::new(memory.min, memory.max))));
                }
            }

            // Tables
            if let Some(table) = &module.table {
                if let Some(import_ref) = table.import.clone() {
                    let module = import_ref.module;
                    let field = import_ref.field;
                    let imported = imports.get(&module).and_then(|m| m.get(&field)).ok_or(Error::Link(UNKNOWN_IMPORT))?;
                    match imported {
                        ExportValue::Table(tab) => {
                            let tb = tab.borrow();
                            if tb.size() < table.min || tb.max() > table.max { return Err(Error::Link(INCOMPATIBLE_IMPORT)); }
                            drop(tb);
                            inst.table = Some(tab.clone());
                        }
                        _ => return Err(Error::Link(INCOMPATIBLE_IMPORT)),
                    }
                } else {
                    inst.table = Some(Rc::new(RefCell::new(WasmTable::new(table.min, table.max))));
                }
            }

            // Functions
            inst.functions.reserve(module.functions.len());
            for function in &module.functions {
                if let Some(import_ref) = function.import.clone() {
                    let module = import_ref.module;
                    let field = import_ref.field;
                    let imported = imports.get(&module).and_then(|m| m.get(&field)).ok_or(Error::Link(UNKNOWN_IMPORT))?;
                    let ty = RuntimeType::from_signature(&function.ty);
                    match imported {
                        ExportValue::Function(f) => {
                            if f.ty != ty { return Err(Error::Link(INCOMPATIBLE_IMPORT)); }
                            inst.functions.push(f.clone());
                        }
                        _ => return Err(Error::Link(INCOMPATIBLE_IMPORT)),
                    }
                } else {
                    let locals_count = function.locals.len().saturating_sub(function.ty.params.len());
                    inst.functions.push(FunctionInfo { ty: RuntimeType::from_signature(&function.ty), wasm_fn: Some(function.body.start), locals_count, host: None, owner: None, owner_index: None });
                }
            }

            // Globals
            inst.globals.reserve(module.globals.len());
            for g in &module.globals {
                if let Some(import_ref) = g.import.clone() {
                    let module = import_ref.module;
                    let field = import_ref.field;
                    let imported = imports.get(&module).and_then(|m| m.get(&field)).ok_or(Error::Link(UNKNOWN_IMPORT))?;
                    match imported {
                        ExportValue::Global(gl) => {
                            let gb = gl.borrow();
                            if gb.ty != g.ty || gb.mutable != g.is_mutable { return Err(Error::Link(INCOMPATIBLE_IMPORT)); }
                            drop(gb);
                            inst.globals.push(gl.clone());
                        }
                        _ => return Err(Error::Link(INCOMPATIBLE_IMPORT)),
                    }
                } else {
                    // evaluate constant initializer
                    let mut cpc = g.initializer_offset;
                    let val = Instance::eval_const(&module, &mut cpc, &inst.globals)?;
                    inst.globals.push(Rc::new(RefCell::new(WasmGlobal { ty: g.ty, mutable: g.is_mutable, value: val })));
                }
            }

            let mut collected_elements: Option<Vec<(u32, Vec<u32>)>> = None;
            if module.element_count > 0 {
                if inst.table.is_none() { return Err(Error::Link(UNKNOWN_TABLE)); }
                let bytes = &module.bytes;
                let mut it = module.element_start;
                let n_segments: u32 = module.element_count;
                let mut collected: Vec<(u32, Vec<u32>)> = Vec::with_capacity(n_segments as usize);
                for _ in 0..n_segments {
                    let flags: u32 = read_leb128(bytes, &mut it)?;
                    if flags != 0 { return Err(Error::Malformed(INVALID_VALUE_TYPE)); }
                    let offset = Instance::eval_const(&module, &mut it, &inst.globals)?.as_u32();
                    let n: u32 = read_leb128(bytes, &mut it)?;
                    {
                        let table_rc = inst.table.as_ref().ok_or(Error::Link(UNKNOWN_TABLE))?;
                        let table_borrow = table_rc.borrow();
                        if (offset as u64) + (n as u64) > table_borrow.size() as u64 {
                            return Err(Error::Link(ELEM_SEG_DNF));
                        }
                    }
                    let mut indices: Vec<u32> = Vec::with_capacity(n as usize);
                    for _ in 0..n {
                        let fn_index: u32 = read_leb128(bytes, &mut it)?;
                        indices.push(fn_index);
                    }
                    collected.push((offset, indices));
                }
                collected_elements = Some(collected);
            }

            // Validate and collect data segments (no writes yet)
            let mut pending_data: Option<Vec<(u32, Vec<u8>)>> = None;
            if let Some(mem) = &inst.memory {
                struct PendingData { offset: u32, bytes: Vec<u8> }
                let mut pending: Vec<PendingData> = Vec::new();
                for seg in &module.data_segments {
                    let mut ip = seg.initializer_offset;
                    let offset = Instance::eval_const(&module, &mut ip, &inst.globals)?.as_u32();
                    let bytes_vec = module.bytes[seg.data_range.clone()].to_vec();
                    let m = mem.borrow();
                    let end = (offset as usize).saturating_add(bytes_vec.len());
                    if end > (m.size() as usize) * (WasmMemory::PAGE_SIZE as usize) {
                        return Err(Error::Link(DATA_SEG_DNF));
                    }
                    drop(m);
                    pending.push(PendingData { offset, bytes: bytes_vec });
                }
                if !pending.is_empty() {
                    pending_data = Some(pending.into_iter().map(|p| (p.offset, p.bytes)).collect());
                }
            }

            // Apply element segments now that data segments have been validated
            if let Some(collected) = collected_elements {
                let table_rc = inst.table.as_ref().ok_or(Error::Link(UNKNOWN_TABLE))?.clone();
                #[cfg(feature = "wasm_debug")]
                {
                    let sz = table_rc.borrow().size();
                    crate::debug_println!("[elem] table size={} segments={} ", sz, collected.len());
                }
                for (offset, indices) in collected.iter() {
                    for (j, idx_fn) in indices.iter().enumerate() {
                        let fn_index = *idx_fn as usize;
                        let f = inst.functions[fn_index].clone();
                        let (owner_id, owner_fn_index) = if let (Some(weak_owner), Some(owner_idx)) = (f.owner.clone(), f.owner_index) {
                            if let Some(owner_rc) = weak_owner.upgrade() { (owner_rc.id, owner_idx as u32) } else { (inst.id, fn_index as u32) }
                        } else {
                            (inst.id, fn_index as u32)
                        };
                        let func_ref = FuncRef::new(owner_id, owner_fn_index);
                        let func_ref_value = WasmValue::from_u64(func_ref.as_raw());
                        if table_rc.borrow_mut().set(*offset + (j as u32), func_ref_value).is_err() {
                            return Err(Error::Link(ELEM_SEG_DNF));
                        }
                    }
                }
            }

            // Apply data segments (writes), after elements
            if let (Some(mem), Some(pending)) = (&inst.memory, pending_data) {
                if !pending.is_empty() {
                    let mut m = mem.borrow_mut();
                    for (offset, bytes_vec) in pending.into_iter() {
                        for (i, b) in bytes_vec.iter().enumerate() {
                            m.store_u8(offset + i as u32, 0, *b).map_err(Error::Trap)?;
                        }
                    }
                }
            }

            // Exports
            for (name, ex) in &module.exports {
                match ex.extern_type {
                    ExternType::Func => { inst.exports.insert(name.clone(), ExportValue::Function(inst.functions[ex.idx as usize].clone())); }
                    ExternType::Table => {
                        if let Some(table) = &inst.table {
                            inst.exports.insert(name.clone(), ExportValue::Table(table.clone()));
                        }
                    }
                    ExternType::Mem => { if let Some(mem) = &inst.memory { inst.exports.insert(name.clone(), ExportValue::Memory(mem.clone())); } }
                    ExternType::Global => { inst.exports.insert(name.clone(), ExportValue::Global(inst.globals[ex.idx as usize].clone())); }
                }
            }
        }

        // Register a weak reference before potential start execution so that
        // even if start traps, func_refs already stored in tables can resolve
        // the owning instance via the registry
        InstanceManager::with(|mgr| mgr.register_instance(&inst_rc));

        // Start
        if module.start != u32::MAX {
            let fi = module.start as usize;
            let func_info = &inst_rc.functions[fi];
            if func_info.ty.n_params() != 0 || func_info.ty.has_result() { return Err(Error::Validation(START_FUNC)); }
            let mut stack = Vec::with_capacity(64);
            let mut return_pc = 0usize;
            let mut control: Vec<ControlFrame> = Vec::new();
            let mut bases: Vec<usize> = Vec::new();
            let mut ctrl_bases = vec![];
            match inst_rc.call_function_index(fi, &mut return_pc, &mut stack, &mut control, &mut bases, &mut ctrl_bases) {
                Ok(()) => {}
                Err(Error::Trap(msg)) => {
                    // If there are live func_ref references to this instance,
                    // keep it alive as a zombie until all references are dropped
                    InstanceManager::with(|mgr| mgr.add_zombie(inst_rc));
                    return Err(Error::Uninstantiable(msg));
                }
                Err(e) => { return Err(e); }
            }
        }

        // Success: unwrap Rc to return by value
        match Rc::try_unwrap(inst_rc) {
            Ok(inst) => Ok(inst),
            Err(_) => unreachable!("unexpected extra strong refs while instantiating"),
        }
    }

    fn eval_const(
        module: &Module,
        pc: &mut usize,
        globals: &[Rc<RefCell<WasmGlobal>>]
    ) -> Result<WasmValue, Error> {
        let bytes = &module.bytes;
        let mut stack: Vec<WasmValue> = Vec::new();
        loop {
            let op = bytes[*pc]; *pc += 1;
            match op {
                0x41 => { let v: i32 = read_sleb128(bytes, pc)?; stack.push(WasmValue::from_i32(v)); }
                0x42 => { let v: i64 = read_sleb128(bytes, pc)?; stack.push(WasmValue::from_i64(v)); }
                0x43 => { let bits = u32::from_le_bytes(bytes[*pc..*pc+4].try_into().unwrap()); *pc += 4; stack.push(WasmValue::from_f32_bits(bits)); }
                0x44 => { let bits = u64::from_le_bytes(bytes[*pc..*pc+8].try_into().unwrap()); *pc += 8; stack.push(WasmValue::from_f64_bits(bits)); }
                0x23 => { let gi: u32 = read_leb128(bytes, pc)?; let g = gi as usize; if g >= globals.len() { return Err(Error::Validation(UNKNOWN_GLOBAL)); } stack.push(globals[g].borrow().value); }
                0x6a => { let b = stack.pop().unwrap().as_u32(); let a = stack.pop().unwrap().as_u32(); stack.push(WasmValue::from_u32(a.wrapping_add(b))); }
                0x6b => { let b = stack.pop().unwrap().as_u32(); let a = stack.pop().unwrap().as_u32(); stack.push(WasmValue::from_u32(a.wrapping_sub(b))); }
                0x6c => { let b = stack.pop().unwrap().as_u32(); let a = stack.pop().unwrap().as_u32(); stack.push(WasmValue::from_u32(a.wrapping_mul(b))); }
                0x7c => { let b = stack.pop().unwrap().as_u64(); let a = stack.pop().unwrap().as_u64(); stack.push(WasmValue::from_u64(a.wrapping_add(b))); }
                0x7d => { let b = stack.pop().unwrap().as_u64(); let a = stack.pop().unwrap().as_u64(); stack.push(WasmValue::from_u64(a.wrapping_sub(b))); }
                0x7e => { let b = stack.pop().unwrap().as_u64(); let a = stack.pop().unwrap().as_u64(); stack.push(WasmValue::from_u64(a.wrapping_mul(b))); }
                0x0b => break,
                _ => return Err(Error::Validation(CONST_EXP_REQUIRED)),
            }
        }
        Ok(stack.pop().unwrap())
    }

    #[inline]
    fn setup_wasm_function_call(
        func_info: &FunctionInfo,
        stack: &mut Vec<WasmValue>,
        control: &mut Vec<ControlFrame>,
        fn_bases: &mut Vec<usize>,
        ctrl_bases: &mut Vec<usize>,
        return_dest: usize
    ) -> Result<usize, Error> {
        let n_params = func_info.ty.n_params() as usize;
        let has_result = func_info.ty.has_result();
        let locals_start = stack.len() - n_params;

        // Allocate space for local variables
        for _ in 0..func_info.locals_count {
            stack.push(WasmValue::default());
        }

        // Push return target
        control.push(ControlFrame {
            stack_len: locals_start,
            dest_pc: return_dest,
            arity: if has_result { 1 } else { 0 },
            has_result,
        });

        const MAX_CONTROL_DEPTH: usize = 1000;
        if control.len() > MAX_CONTROL_DEPTH {
            return Err(Error::Trap(STACK_EXHAUSTED));
        }

        // Track function frame bases
        fn_bases.push(locals_start);
        ctrl_bases.push(control.len() - 1);

        // Return the function's start PC
        Ok(func_info.wasm_fn.unwrap())
    }


    #[inline]
    fn call_host_function(
        host: &dyn Fn(&mut [WasmValue]),
        ty: RuntimeType,
        stack: &mut Vec<WasmValue>,
        params_start: usize,
    ) {
        let n_params = ty.n_params() as usize;
        let has_result = ty.has_result();

        // Buffer size: need space for params, or at least 1 for result-only functions
        let buffer_size = n_params.max(if has_result && n_params == 0 { 1 } else { 0 });

        const STACK_THRESHOLD: usize = 8;
        let mut small_buffer;
        let mut large_buffer;

        let buffer = if buffer_size <= STACK_THRESHOLD {
            small_buffer = [WasmValue::default(); STACK_THRESHOLD];
            &mut small_buffer[..buffer_size]
        } else {
            large_buffer = vec![WasmValue::default(); buffer_size];
            &mut large_buffer[..]
        };

        if n_params > 0 {
            buffer[..n_params].copy_from_slice(&stack[params_start..params_start + n_params]);
        }

        host(buffer);
        stack.truncate(params_start);
        if has_result {
            stack.push(buffer[0]);
        }
    }

    fn call_function_index(
        &self,
        idx: usize,
        return_pc: &mut usize,
        stack: &mut Vec<WasmValue>,
        control: &mut Vec<ControlFrame>,
        fn_bases: &mut Vec<usize>,
        ctrl_bases: &mut Vec<usize>
    ) -> Result<(), Error> {
        const MAX_CALL_DEPTH: usize = 1000;
        if fn_bases.len() >= MAX_CALL_DEPTH {
            return Err(Error::Trap(STACK_EXHAUSTED));
        }
        let fi = &self.functions[idx];
        if fi.wasm_fn.is_some() {
            let pc_start = Self::setup_wasm_function_call(fi, stack, control, fn_bases, ctrl_bases, *return_pc)?;
            self.interpret(pc_start, stack, control, fn_bases, ctrl_bases)?;
        } else if let Some(host) = &fi.host {
            let params_start = stack.len() - fi.ty.n_params() as usize;
            Self::call_host_function(host.as_ref(), fi.ty, stack, params_start);
        } else {
            return Err(Error::Trap(FUNC_NO_IMPL));
        }
        Ok(())
    }

    fn interpret(
        &self,
        mut pc: usize,
        stack: &mut Vec<WasmValue>,
        control: &mut Vec<ControlFrame>,
        fn_bases: &mut Vec<usize>,
        ctrl_bases: &mut Vec<usize>
    ) -> Result<(), Error> {
        Ok(())
    }
}