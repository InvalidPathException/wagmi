use crate::error::*;
use crate::leb128::{read_leb128, read_sleb128};
use crate::module::ExternType;
use crate::signature::{Signature, ValType, RuntimeSignature};
use crate::wasm_memory::WasmMemory;
use crate::Module;
use paste::paste;
use std::cell::{RefCell, Cell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

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
        // FuncRef handles ref-counting automatically via Drop/Clone
        self.elements[i] = FuncRef::from_raw(value.as_u64());
        Ok(())
    }
}

pub struct WasmGlobal {
    pub ty: ValType,
    pub mutable: bool,
    pub value: Cell<WasmValue>,
}

// --------------- Imports/Exports and Functions ---------------

#[derive(Clone)]
pub enum RuntimeFunction {
    OwnedWasm {
        runtime_sig: RuntimeSignature,
        pc_start: usize,
        locals_count: usize,
    },
    ImportedWasm {
        runtime_sig: RuntimeSignature,
        owner: Weak<Instance>,
        function_index: usize,
    },
    Host {
        callback: Rc<dyn Fn(&[WasmValue]) -> Option<WasmValue>>,
        runtime_sig: RuntimeSignature,
    }
}

impl RuntimeFunction {
    pub fn signature(&self) -> RuntimeSignature {
        match self {
            RuntimeFunction::OwnedWasm { runtime_sig, .. } => *runtime_sig,
            RuntimeFunction::ImportedWasm { runtime_sig, .. } => *runtime_sig,
            RuntimeFunction::Host { runtime_sig, .. } => *runtime_sig,
        }
    }
    
    pub fn param_count(&self) -> usize {
        self.signature().n_params() as usize
    }

    pub fn new_host(
        params: Vec<ValType>,
        result: Option<ValType>,
        callback: impl Fn(&[WasmValue]) -> Option<WasmValue> + 'static,
    ) -> Self {
        RuntimeFunction::Host {
            callback: Rc::new(callback),
            runtime_sig: RuntimeSignature::from_signature(&Signature { params, result }),
        }
    }
}

#[derive(Clone)]
pub enum ExportValue {
    Function(RuntimeFunction),
    Table(Rc<RefCell<WasmTable>>),
    Memory(Rc<RefCell<WasmMemory>>),
    Global(Rc<WasmGlobal>),
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
    pub globals: Vec<Rc<WasmGlobal>>,
    pub functions: Vec<RuntimeFunction>,
    pub exports: Exports,
}

impl Instance {
    pub fn new(module: Rc<Module>) -> Self {
        // Other than the validated module, everything starts empty
        Self { module, ..Default::default() }
    }

    /// Register or re-register an instance, used for testing when wrapping in a new Rc
    pub fn register_external_instance(inst: &Rc<Instance>) {
        // This updates the registry entry even if the instance was already registered
        InstanceManager::with(|mgr| mgr.register_instance(inst));
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
                    let imported = imports.get(&import_ref.module).and_then(|m| m.get(&import_ref.field)).ok_or(Error::link(UNKNOWN_IMPORT))?;
                    match imported {
                        ExportValue::Memory(mem) => {
                            let m = mem.borrow();
                            if m.size() < memory.min || m.max() > memory.max { return Err(Error::link(INCOMPATIBLE_IMPORT)); }
                            drop(m);
                            inst.memory = Some(mem.clone());
                        }
                        _ => return Err(Error::link(INCOMPATIBLE_IMPORT)),
                    }
                } else {
                    inst.memory = Some(Rc::new(RefCell::new(WasmMemory::new(memory.min, memory.max))));
                }
            }

            // Tables
            if let Some(table) = &module.table {
                if let Some(import_ref) = table.import.clone() {
                    let imported = imports.get(&import_ref.module).and_then(|m| m.get(&import_ref.field)).ok_or(Error::link(UNKNOWN_IMPORT))?;
                    match imported {
                        ExportValue::Table(tab) => {
                            let tb = tab.borrow();
                            if tb.size() < table.min || tb.max() > table.max { return Err(Error::link(INCOMPATIBLE_IMPORT)); }
                            drop(tb);
                            inst.table = Some(tab.clone());
                        }
                        _ => return Err(Error::link(INCOMPATIBLE_IMPORT)),
                    }
                } else {
                    inst.table = Some(Rc::new(RefCell::new(WasmTable::new(table.min, table.max))));
                }
            }

            // Functions
            inst.functions.reserve(module.functions.len());
            for function in &module.functions {
                if let Some(import_ref) = function.import.clone() {
                    let imported = imports.get(&import_ref.module).and_then(|m| m.get(&import_ref.field)).ok_or(Error::link(UNKNOWN_IMPORT))?;
                    let runtime_sig = RuntimeSignature::from_signature(&function.ty);
                    match imported {
                        ExportValue::Function(f) => {
                            if f.signature() != runtime_sig { return Err(Error::link(INCOMPATIBLE_IMPORT)); }
                            inst.functions.push(f.clone());
                        }
                        _ => return Err(Error::link(INCOMPATIBLE_IMPORT)),
                    }
                } else {
                    let locals_count = function.locals.len().saturating_sub(function.ty.params.len());
                    inst.functions.push(RuntimeFunction::OwnedWasm {
                        runtime_sig: RuntimeSignature::from_signature(&function.ty),
                        pc_start: function.body.start, 
                        locals_count,
                    });
                }
            }

            // Globals
            inst.globals.reserve(module.globals.len());
            for g in &module.globals {
                if let Some(import_ref) = g.import.clone() {
                    let imported = imports.get(&import_ref.module).and_then(|m| m.get(&import_ref.field)).ok_or(Error::link(UNKNOWN_IMPORT))?;
                    match imported {
                        ExportValue::Global(gl) => {
                            let gb = gl.as_ref();
                            if gb.ty != g.ty || gb.mutable != g.is_mutable { return Err(Error::link(INCOMPATIBLE_IMPORT)); }
                            inst.globals.push(gl.clone());
                        }
                        _ => return Err(Error::link(INCOMPATIBLE_IMPORT)),
                    }
                } else {
                    // evaluate constant initializer
                    let mut cpc = g.initializer_offset;
                    let val = Instance::eval_const(&module, &mut cpc, &inst.globals)?;
                    inst.globals.push(Rc::new(WasmGlobal { ty: g.ty, mutable: g.is_mutable, value: Cell::new(val) }));
                }
            }

            let mut collected_elements: Option<Vec<(u32, Vec<u32>)>> = None;
            if module.element_count > 0 {
                if inst.table.is_none() { return Err(Error::link(UNKNOWN_TABLE)); }
                let bytes = &module.bytes;
                let mut it = module.element_start;
                let n_segments: u32 = module.element_count;
                let mut collected: Vec<(u32, Vec<u32>)> = Vec::with_capacity(n_segments as usize);
                for _ in 0..n_segments {
                    let flags: u32 = read_leb128(bytes, &mut it)?;
                    if flags != 0 { return Err(Error::malformed(INVALID_VALUE_TYPE)); }
                    let offset = Instance::eval_const(&module, &mut it, &inst.globals)?.as_u32();
                    let n: u32 = read_leb128(bytes, &mut it)?;
                    {
                        let table_rc = inst.table.as_ref().ok_or(Error::link(UNKNOWN_TABLE))?;
                        let table_borrow = table_rc.borrow();
                        if (offset as u64) + (n as u64) > table_borrow.size() as u64 {
                            return Err(Error::link(ELEM_SEG_DNF));
                        }
                    }
                    let mut indices: Vec<u32> = Vec::with_capacity(n as usize);
                    for _ in 0..n {
                        let func_idx: u32 = read_leb128(bytes, &mut it)?;
                        indices.push(func_idx);
                    }
                    collected.push((offset, indices));
                }
                collected_elements = Some(collected);
            }

            // Validate and collect data segments (no writes yet)
            let mut pending_data: Option<Vec<(u32, Vec<u8>)>> = None;
            if let Some(mem) = &inst.memory {
                let mut pending: Vec<(u32, Vec<u8>)> = Vec::new();
                for seg in &module.data_segments {
                    let mut ip = seg.initializer_offset;
                    let offset = Instance::eval_const(&module, &mut ip, &inst.globals)?.as_u32();
                    let bytes_vec = module.bytes[seg.data_range.clone()].to_vec();
                    let m = mem.borrow();
                    let end = (offset as usize).saturating_add(bytes_vec.len());
                    if end > (m.size() as usize) * (WasmMemory::PAGE_SIZE as usize) {
                        return Err(Error::link(DATA_SEG_DNF));
                    }
                    drop(m);
                    pending.push((offset, bytes_vec));
                }
                if !pending.is_empty() {
                    pending_data = Some(pending);
                }
            }

            // Apply element segments now that data segments have been validated
            if let Some(collected) = collected_elements {
                let table_rc = inst.table.as_ref().ok_or(Error::link(UNKNOWN_TABLE))?.clone();
                for (offset, indices) in collected.iter() {
                    for (j, idx) in indices.iter().enumerate() {
                        let func_idx = *idx as usize;
                        let f = inst.functions[func_idx].clone();
                        let (owner_id, owner_func_idx) = match &f {
                            RuntimeFunction::ImportedWasm { owner, function_index, .. } => {
                                if let Some(owner_rc) = owner.upgrade() { (owner_rc.id, *function_index as u32) } else { (inst.id, func_idx as u32) }
                            }
                            RuntimeFunction::OwnedWasm { .. } | RuntimeFunction::Host { .. } => (inst.id, func_idx as u32),
                        };
                        let func_ref = FuncRef::new(owner_id, owner_func_idx);
                        let func_ref_value = WasmValue::from_u64(func_ref.as_raw());
                        if table_rc.borrow_mut().set(*offset + (j as u32), func_ref_value).is_err() {
                            return Err(Error::link(ELEM_SEG_DNF));
                        }
                    }
                }
            }

            // Apply data segments (writes), after elements
            if let (Some(mem), Some(pending)) = (&inst.memory, pending_data) {
                if !pending.is_empty() {
                    let mut m = mem.borrow_mut();
                    for (offset, bytes_vec) in pending.into_iter() {
                        m.write_bytes(offset, &bytes_vec).map_err(Error::trap)?;
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
            let function = &inst_rc.functions[fi];
            if function.signature().n_params() != 0 || function.signature().has_result() { return Err(Error::validation(START_FUNC)); }
            let mut stack = Vec::with_capacity(64);
            let mut return_pc = 0usize;
            let mut control: Vec<ControlFrame> = Vec::new();
            let mut bases: Vec<usize> = Vec::new();
            let mut ctrl_bases = vec![];
            match inst_rc.call_function_idx(fi, &mut return_pc, &mut stack, &mut control, &mut bases, &mut ctrl_bases) {
                Ok(()) => {}
                Err(Error::Trap(msg)) => {
                    // If there are live func_ref references to this instance,
                    // keep it alive as a zombie until all references are dropped
                    InstanceManager::with(|mgr| mgr.add_zombie(inst_rc));
                    return Err(Error::uninstantiable(msg));
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
        globals: &[Rc<WasmGlobal>]
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
                0x23 => { let gi: u32 = read_leb128(bytes, pc)?; let g = gi as usize; if g >= globals.len() { return Err(Error::validation(UNKNOWN_GLOBAL)); } stack.push(globals[g].value.get()); }
                0x6a => { let b = stack.pop().unwrap().as_u32(); let a = stack.pop().unwrap().as_u32(); stack.push(WasmValue::from_u32(a.wrapping_add(b))); }
                0x6b => { let b = stack.pop().unwrap().as_u32(); let a = stack.pop().unwrap().as_u32(); stack.push(WasmValue::from_u32(a.wrapping_sub(b))); }
                0x6c => { let b = stack.pop().unwrap().as_u32(); let a = stack.pop().unwrap().as_u32(); stack.push(WasmValue::from_u32(a.wrapping_mul(b))); }
                0x7c => { let b = stack.pop().unwrap().as_u64(); let a = stack.pop().unwrap().as_u64(); stack.push(WasmValue::from_u64(a.wrapping_add(b))); }
                0x7d => { let b = stack.pop().unwrap().as_u64(); let a = stack.pop().unwrap().as_u64(); stack.push(WasmValue::from_u64(a.wrapping_sub(b))); }
                0x7e => { let b = stack.pop().unwrap().as_u64(); let a = stack.pop().unwrap().as_u64(); stack.push(WasmValue::from_u64(a.wrapping_mul(b))); }
                0x0b => break,
                _ => return Err(Error::validation(CONST_EXP_REQUIRED)),
            }
        }
        Ok(stack.pop().unwrap())
    }

    #[inline]
    fn setup_wasm_function_call(
        runtime_sig: RuntimeSignature,
        pc_start: usize,
        locals_count: usize,
        stack: &mut Vec<WasmValue>,
        control: &mut Vec<ControlFrame>,
        func_bases: &mut Vec<usize>,
        ctrl_bases: &mut Vec<usize>,
        return_dest: usize
    ) -> Result<usize, Error> {
        let n_params = runtime_sig.n_params() as usize;
        let has_result = runtime_sig.has_result();
        let locals_start = stack.len() - n_params;

        // Allocate space for local variables
        stack.resize(stack.len() + locals_count, WasmValue::default());

        // Push return target
        control.push(ControlFrame {
            stack_len: locals_start,
            dest_pc: return_dest,
            arity: if has_result { 1 } else { 0 },
            has_result,
        });

        const MAX_CONTROL_DEPTH: usize = 1000;
        if control.len() > MAX_CONTROL_DEPTH {
            return Err(Error::trap(STACK_EXHAUSTED));
        }

        // Track function frame bases
        func_bases.push(locals_start);
        ctrl_bases.push(control.len() - 1);

        // Return the function's start PC
        Ok(pc_start)
    }


    #[inline(always)]
    fn call_function_idx(
        &self,
        idx: usize,
        return_pc: &mut usize,
        stack: &mut Vec<WasmValue>,
        control: &mut Vec<ControlFrame>,
        func_bases: &mut Vec<usize>,
        ctrl_bases: &mut Vec<usize>
    ) -> Result<(), Error> {
        const MAX_CALL_DEPTH: usize = 1000;
        if func_bases.len() >= MAX_CALL_DEPTH {
            return Err(Error::trap(STACK_EXHAUSTED));
        }
        let fi = &self.functions[idx];
        match fi {
            RuntimeFunction::OwnedWasm { runtime_sig, pc_start, locals_count } => {
                let pc = Self::setup_wasm_function_call(*runtime_sig, *pc_start, *locals_count, stack, control, func_bases, ctrl_bases, *return_pc)?;
                self.interpret(pc, stack, control, func_bases, ctrl_bases)?;
            }
            RuntimeFunction::ImportedWasm { owner, function_index, .. } => {
                if let Some(owner_rc) = owner.upgrade() {
                    owner_rc.call_function_idx(*function_index, return_pc, stack, control, func_bases, ctrl_bases)?;
                } else {
                    return Err(Error::trap(FUNC_NO_IMPL));
                }
            }
            RuntimeFunction::Host { callback, runtime_sig } => {
                let param_count = runtime_sig.n_params() as usize;
                let params_start = stack.len() - param_count;
                if let Some(result) = callback(&stack[params_start..]) {
                    stack.truncate(params_start);
                    stack.push(result);
                } else {
                    stack.truncate(params_start);
                }
            }
        }
        Ok(())
    }

    fn interpret(
        &self,
        mut pc: usize,
        stack: &mut Vec<WasmValue>,
        control: &mut Vec<ControlFrame>,
        func_bases: &mut Vec<usize>,
        ctrl_bases: &mut Vec<usize>
    ) -> Result<(), Error> {
        let bytes = &self.module.bytes;
        let mem = self.memory.as_ref();
        let tab = self.table.as_ref();

        macro_rules! next_op { () => {{ let byte = bytes[pc]; pc += 1; byte }} }
        macro_rules! pop_val { () => {{
            match stack.pop() { Some(v) => v, None => return Err(Error::trap(STACK_UNDERFLOW)) }
        }} }
        macro_rules! binary {
            ($type:ident, $op:tt) => {{
                paste! {
                    let b = pop_val!().[<as_ $type>]();
                    let a = pop_val!().[<as_ $type>]();
                    stack.push(WasmValue::[<from_ $type>](a $op b));
                }
            }};
            ($type:ident, .$method:ident) => {{
                paste! {
                    let b = pop_val!().[<as_ $type>]();
                    let a = pop_val!().[<as_ $type>]();
                    stack.push(WasmValue::[<from_ $type>](a.$method(b)));
                }
            }};
        }
        macro_rules! compare {
            ($type:ident, $op:tt) => {{
                paste! {
                    let b = pop_val!().[<as_ $type>]();
                    let a = pop_val!().[<as_ $type>]();
                    stack.push(WasmValue::from_u32((a $op b) as u32));
                }
            }};
        }
        macro_rules! shift {
            (u32, $op:tt) => {{
                let b = pop_val!().as_u32() % 32;
                let a = pop_val!().as_u32();
                stack.push(WasmValue::from_u32(a $op b));
            }};
            (u64, $op:tt) => {{
                let b = pop_val!().as_u64() % 64;
                let a = pop_val!().as_u64();
                stack.push(WasmValue::from_u64(a $op b));
            }};
        }
        macro_rules! rotate {
            (u32, $dir:ident) => {{
                let b = pop_val!().as_u32();
                let a = pop_val!().as_u32();
                paste! {
                    stack.push(WasmValue::from_u32(a.[<rotate_ $dir>](b % 32)));
                }
            }};
            (u64, $dir:ident) => {{
                let b = pop_val!().as_u64();
                let a = pop_val!().as_u64();
                paste! {
                    stack.push(WasmValue::from_u64(a.[<rotate_ $dir>]((b % 64) as u32)));
                }
            }};
        }
        macro_rules! unary {
            ($type:ident, $f:expr) => {{
                paste! {
                    let a = pop_val!().[<as_ $type>]();
                    stack.push(WasmValue::[<from_ $type>]($f(a)));
                }
            }};
        }
        macro_rules! minmax {
            ($type:ident, min) => {{ minmax!(@impl $type, min, true) }};
            ($type:ident, max) => {{ minmax!(@impl $type, max, false) }};
            (@impl $type:ident, $op:ident, $want_negative:literal) => {{
                paste! {
                    let b = pop_val!().[<as_ $type>]();
                    let a = pop_val!().[<as_ $type>]();

                    let result = if a.is_nan() {
                        a
                    } else if b.is_nan() {
                        b
                    } else if a == b && a == 0.0 {
                        const SIGN_BIT_SHIFT: usize = std::mem::size_of::<$type>() * 8 - 1;
                        let a_has_sign = a.to_bits() >> SIGN_BIT_SHIFT != 0;
                        if a_has_sign == $want_negative { a } else { b }
                    } else {
                        a.$op(b)
                    };

                    stack.push(WasmValue::[<from_ $type>](result));
                }
            }};
        }
        macro_rules! shr_s {
            ($int_type:ident, $uint_type:ident, $bits:literal) => {{
                paste! {
                    let b = pop_val!().[<as_ $uint_type>]() % $bits;
                    let a = pop_val!().[<as_ $int_type>]();
                    stack.push(WasmValue::[<from_ $int_type>](a >> b));
                }
            }};
        }
        macro_rules! copysign {
            ($type:ident) => {{
                paste! {
                    let b = pop_val!().[<as_ $type>]();
                    let a = pop_val!().[<as_ $type>]();
                    stack.push(WasmValue::[<from_ $type>](a.copysign(b)));
                }
            }};
        }
        macro_rules! nearest {
            ($type:ident) => {{
                paste! {
                    let x = stack.pop().unwrap().[<as_ $type>]();
                    let y = if x.is_nan() || x.is_infinite() {
                        x
                    } else {
                        let lower = x.floor();
                        let upper = x.ceil();
                        let dl = x - lower;
                        let du = upper - x;
                        if dl < du {
                            lower
                        } else if dl > du {
                            upper
                        } else {
                            if (lower % 2.0) == 0.0 { lower } else { upper }
                        }
                    };
                    stack.push(WasmValue::[<from_ $type>](y));
                }
            }};
        }
        macro_rules! convert {
            ($src_type:ident -> $dst_type:ident) => {{
                paste! {
                    let v = stack.pop().unwrap().[<as_ $src_type>]();
                    stack.push(WasmValue::[<from_ $dst_type>](v as $dst_type));
                }
            }};
        }
        macro_rules! trunc {
            ($src_type:ident -> $dst_type:ident : $min:expr, $max:expr) => {{
                paste! {
                    let x = stack.pop().unwrap().[<as_ $src_type>]();
                    if !x.is_finite() {
                        if x.is_nan() {
            return Err(Error::trap(INVALID_CONV_TO_INT));
                        } else {
                            return Err(Error::trap(INTEGER_OVERFLOW));
                        }
                    }
                    if x <= $min || x >= $max {
                        return Err(Error::trap(INTEGER_OVERFLOW));
                    }
                    stack.push(WasmValue::[<from_ $dst_type>](x as $dst_type));
                }
            }};
        }
        macro_rules! div_s {
            ($int_type:ident) => {{
                paste! {
                    let b = pop_val!().[<as_ $int_type>]();
                    let a = pop_val!().[<as_ $int_type>]();
                    if b == 0 { return Err(Error::trap(DIVIDE_BY_ZERO)); }
                    if a == $int_type::MIN && b == -1 { return Err(Error::trap(INTEGER_OVERFLOW)); }
                    stack.push(WasmValue::[<from_ $int_type>](a / b));
                }
            }};
        }
        macro_rules! div_u {
            ($uint_type:ident) => {{
                paste! {
                    let b = pop_val!().[<as_ $uint_type>]();
                    let a = pop_val!().[<as_ $uint_type>]();
                    if b == 0 { return Err(Error::trap(DIVIDE_BY_ZERO)); }
                    stack.push(WasmValue::[<from_ $uint_type>](a / b));
                }
            }};
        }
        macro_rules! rem_s {
            ($int_type:ident) => {{
                paste! {
                    let b = pop_val!().[<as_ $int_type>]();
                    let a = pop_val!().[<as_ $int_type>]();
                    if b == 0 { return Err(Error::trap(DIVIDE_BY_ZERO)); }
                    if a == $int_type::MIN && b == -1 {
                        stack.push(WasmValue::[<from_ $int_type>](0));
                    } else {
                        stack.push(WasmValue::[<from_ $int_type>](a % b));
                    }
                }
            }};
        }
        macro_rules! rem_u {
            ($uint_type:ident) => {{
                paste! {
                    let b = pop_val!().[<as_ $uint_type>]();
                    let a = pop_val!().[<as_ $uint_type>]();
                    if b == 0 { return Err(Error::trap(DIVIDE_BY_ZERO)); }
                    stack.push(WasmValue::[<from_ $uint_type>](a % b));
                }
            }};
        }
        macro_rules! load { ($method:ident, $push:expr) => {{
            let _align: u32 = read_leb128(bytes, &mut pc)?;
            let offset: u32 = read_leb128(bytes, &mut pc)?;
            let addr = pop_val!().as_u32();
            let mem = mem.ok_or_else(|| Error::validation(UNKNOWN_MEMORY))?;
            let v = mem.borrow().$method(addr, offset).map_err(Error::trap)?;
            let val = ($push)(v);
            stack.push(val);
        }}}
        macro_rules! store { ($method:ident, $from:expr) => {{
            let _align: u32 = read_leb128(bytes, &mut pc)?;
            let offset: u32 = read_leb128(bytes, &mut pc)?;
            let raw = pop_val!();
            let addr = pop_val!().as_u32();
            let val = ($from)(raw);
            let mem = mem.ok_or_else(|| Error::validation(UNKNOWN_MEMORY))?;
            mem.borrow_mut().$method(addr, offset, val).map_err(Error::trap)?;
        }}}

        loop {
            if pc >= bytes.len() { return Err(Error::malformed(UNEXPECTED_END)); }
            match next_op!() {
                0x00 => return Err(Error::trap(UNREACHABLE)),
                0x01 | 0xbc | 0xbd | 0xbe | 0xbf => {} // nop and reinterprets (no-op on raw bits)
                0x02 => { // block
                    let (body_pc, end_pc, _else_pc, params_len, has_result) =
                        self.module.side_table.lookup(pc).unwrap();
                    pc = body_pc;
                    control.push(ControlFrame {
                        stack_len: stack.len() - (params_len as usize),
                        dest_pc: end_pc,
                        arity: has_result as u32,
                        has_result,
                    });
                }
                0x03 => { // loop
                    let loop_op_pc = pc - 1;
                    let (body_pc, _end_pc, _else_pc, params_len, has_result) =
                        self.module.side_table.lookup(pc).unwrap();
                    pc = body_pc;
                    control.push(ControlFrame {
                        stack_len: stack.len() - (params_len as usize),
                        dest_pc: loop_op_pc,
                        arity: params_len as u32,
                        has_result,
                    });
                }
                0x04 => { // if
                    let (body_pc, end_pc, else_pc, params_len, has_result) =
                        self.module.side_table.lookup(pc).unwrap();
                    let cond = pop_val!().as_u32();
                    control.push(ControlFrame {
                        stack_len: stack.len() - (params_len as usize),
                        dest_pc: end_pc,
                        arity: has_result as u32,
                        has_result,
                    });
                    pc = if cond == 0 { else_pc } else { body_pc };
                }
                0x05 => { // else
                    let _ = Instance::branch(&mut pc, stack, control, 0);
                }
                0x0b => { // end
                    // Check if we're at a function boundary
                    if let Some(&frame_idx) = ctrl_bases.last() {
                        if frame_idx == control.len().saturating_sub(1) {
                            if Instance::branch(&mut pc, stack, control, 0) {
                                ctrl_bases.pop();
                                let _ = func_bases.pop();
                                return Ok(());
                            }
                            ctrl_bases.pop();
                            let _ = func_bases.pop();
                            continue; // Skip the regular block logic
                        }
                    }
                    
                    // Regular block end (not a function boundary)
                    if let Some(target) = control.pop() {
                        if target.has_result {
                            let result = stack[stack.len() - 1];
                            stack.truncate(target.stack_len);
                            stack.push(result);
                        } else {
                            stack.truncate(target.stack_len);
                        }
                    } else {
                        return Ok(()); // No more control frames
                    }
                }
                0x0c => { // br
                    let depth: u32 = read_leb128(bytes, &mut pc)?;
                    if Instance::branch(&mut pc, stack, control, depth) { return Ok(()); }
                }
                0x0d => { // br_if
                    let depth: u32 = read_leb128(bytes, &mut pc)?;
                    let cond = pop_val!().as_u32();
                    if cond != 0 && Instance::branch(&mut pc, stack, control, depth) { return Ok(()); }
                }
                0x0e => { // br_table
                    let v = pop_val!().as_u32();
                    let n_targets: u32 = read_leb128(bytes, &mut pc)?;
                    let mut depth = u32::MAX;
                    for i in 0..n_targets {
                        let t: u32 = read_leb128(bytes, &mut pc)?;
                        if i == v { depth = t; }
                    }
                    let default_t: u32 = read_leb128(bytes, &mut pc)?;
                    if depth == u32::MAX { depth = default_t; }
                    if Instance::branch(&mut pc, stack, control, depth) { return Ok(()); }
                }
                0x0f => { // return
                    if control.is_empty() { return Ok(()); }
                    let base_idx = *ctrl_bases.last().unwrap();
                    let depth = (control.len() - 1).saturating_sub(base_idx) as u32;
                    if Instance::branch(&mut pc, stack, control, depth) {
                        ctrl_bases.pop();
                        let _ = func_bases.pop();
                        return Ok(());
                    }
                    ctrl_bases.pop();
                    let _ = func_bases.pop();
                }
                // Call instructions
                0x10 => { // call
                    // direct calls are fully type-checked at validation time; no
                    // structural type check is required here. we only use the
                    // runtime signature for fast param/result counts to set up frames.
                    let fi: u32 = read_leb128(bytes, &mut pc)?;
                    let f = &self.functions[fi as usize];
                    
                    match f {
                        RuntimeFunction::OwnedWasm { runtime_sig, pc_start, locals_count } => {
                            pc = Self::setup_wasm_function_call(*runtime_sig, *pc_start, *locals_count, stack, control, func_bases, ctrl_bases, pc)?;
                        }
                        RuntimeFunction::ImportedWasm { owner, function_index, runtime_sig } => {
                            if let Some(owner_rc) = owner.upgrade() {
                                let n_params = runtime_sig.n_params() as usize;
                                let params_start = stack.len() - n_params;
                                let mut tmp_stack: Vec<WasmValue> = Vec::with_capacity(n_params);
                                tmp_stack.extend_from_slice(&stack[params_start..(n_params + params_start)]);
                                stack.truncate(params_start);
                                let mut control_nested: Vec<ControlFrame> = Vec::new();
                                let mut ret_pc_nested = 0usize;
                                let mut func_bases_nested: Vec<usize> = Vec::new();
                                let mut ctrl_bases_nested = vec![];
                                owner_rc.call_function_idx(*function_index, &mut ret_pc_nested, &mut tmp_stack, &mut control_nested, &mut func_bases_nested, &mut ctrl_bases_nested)?;
                                for v in tmp_stack { stack.push(v); }
                            } else {
                                return Err(Error::trap(FUNC_NO_IMPL));
                            }
                        }
                        RuntimeFunction::Host { callback, runtime_sig } => {
                            let param_count = runtime_sig.n_params() as usize;
                            let params_start = stack.len() - param_count;
                            if let Some(result) = callback(&stack[params_start..]) {
                                stack.truncate(params_start);
                                stack.push(result);
                            } else {
                                stack.truncate(params_start);
                            }
                        }
                    }
                }
                0x11 => { // call_indirect
                    // Indirect calls must enforce params at runtime
                    // Here we must parse the indices
                    let type_idx: u32 = read_leb128(bytes, &mut pc)?;
                    pc += 1; // Skip the zero flag
                    let elem_idx = match stack.pop() {
                        Some(v) => v.as_u32(),
                        None => return Err(Error::trap(STACK_UNDERFLOW))
                    };
                    let table_rc = match tab {
                        Some(t) => t,
                        None => return Err(Error::trap(UNDEF_ELEM))
                    };
                    let func_ref = {
                        let table_borrow = table_rc.borrow();
                        if elem_idx >= table_borrow.size() {
                            return Err(Error::trap(UNDEF_ELEM));
                        }
                        table_borrow.get(elem_idx).map_err(Error::trap)?
                    };
                    let handle = func_ref.as_u64();
                    if handle == 0 {
                        return Err(Error::trap(UNINITIALIZED_ELEM));
                    }

                    let owner_id: u32 = (handle >> 32) as u32;
                    let low: u32 = (handle & 0xFFFF_FFFF) as u32;
                    if low == 0 {
                        return Err(Error::trap(FUNC_NO_IMPL));
                    }
                    let func_idx = (low - 1) as usize;
                    let expected = RuntimeSignature::from_signature(&self.module.types[type_idx as usize]);

                    if owner_id != self.id {
                        let mut dispatched = false;
                        let mut sig_ok = false;
                        InstanceManager::with(|mgr| {
                            if let Some(owner) = mgr.get_instance(owner_id) {
                                let callee = &owner.functions[func_idx];
                                sig_ok = callee.signature() == expected;
                                if sig_ok {
                                    let n_params = callee.param_count();
                                    let params_start = stack.len() - n_params;
                                    let mut tmp_stack: Vec<WasmValue> = Vec::with_capacity(n_params);
                                    tmp_stack.extend_from_slice(&stack[params_start..(params_start + n_params)]);
                                    stack.truncate(params_start);
                                    let mut control_nested: Vec<ControlFrame> = Vec::new();
                                    let mut ret_pc_nested = 0usize;
                                    let mut func_bases_nested: Vec<usize> = Vec::new();
                                    let mut ctrl_bases_nested = vec![];
                                    match owner.call_function_idx(func_idx, &mut ret_pc_nested, &mut tmp_stack, &mut control_nested, &mut func_bases_nested, &mut ctrl_bases_nested) {
                                        Ok(()) => {
                                            for v in tmp_stack { stack.push(v); }
                                            dispatched = true;
                                        }
                                        Err(_e) => {}
                                    }
                                }
                            }
                        });
                        if !sig_ok {
                            return Err(Error::trap(INDIRECT_CALL_MISMATCH));
                        }
                        if dispatched {
                            continue;
                        } else {
                            return Err(Error::trap(FUNC_NO_IMPL));
                        }
                    }

                    let callee = self.functions[func_idx].clone();
                    if callee.signature() != expected {
                        return Err(Error::trap(INDIRECT_CALL_MISMATCH));
                    }

                    match callee {
                        RuntimeFunction::ImportedWasm { runtime_sig, owner, function_index } => {
                            if let Some(owner_rc) = owner.upgrade() {
                                let n_params = runtime_sig.n_params() as usize;
                                let params_start = stack.len() - n_params;
                                let mut tmp_stack: Vec<WasmValue> = Vec::with_capacity(n_params);
                                tmp_stack.extend_from_slice(&stack[params_start..(params_start + n_params)]);
                                stack.truncate(params_start);
                                let mut control_nested: Vec<ControlFrame> = Vec::new();
                                let mut ret_pc_nested = 0usize;
                                let mut func_bases_nested: Vec<usize> = Vec::new();
                                let mut ctrl_bases_nested = vec![];
                                owner_rc.call_function_idx(function_index, &mut ret_pc_nested, &mut tmp_stack, &mut control_nested, &mut func_bases_nested, &mut ctrl_bases_nested)?;
                                for v in tmp_stack { stack.push(v); }
                            } else {
                                return Err(Error::trap(FUNC_NO_IMPL));
                            }
                        }
                        RuntimeFunction::OwnedWasm { runtime_sig, pc_start, locals_count } => {
                            pc = Self::setup_wasm_function_call(runtime_sig, pc_start, locals_count, stack, control, func_bases, ctrl_bases, pc)?;
                        }
                        RuntimeFunction::Host { callback, runtime_sig } => {
                            let param_count = runtime_sig.n_params() as usize;
                            let params_start = stack.len() - param_count;
                            if let Some(result) = callback(&stack[params_start..]) {
                                stack.truncate(params_start);
                                stack.push(result);
                            } else {
                                stack.truncate(params_start);
                            }
                        }
                    }
                }
                // Parametric instructions
                0x1a => { // drop
                    if stack.pop().is_none() {
                        return Err(Error::trap(STACK_UNDERFLOW));
                    }
                }
                0x1b => { // select
                    let cond = match stack.pop() {
                        Some(v) => v.as_u32(),
                        None => return Err(Error::trap(STACK_UNDERFLOW))
                    };
                    let v2 = match stack.pop() {
                        Some(v) => v,
                        None => return Err(Error::trap(STACK_UNDERFLOW))
                    };
                    let v1 = match stack.pop() {
                        Some(v) => v,
                        None => return Err(Error::trap(STACK_UNDERFLOW))
                    };
                    stack.push(if cond != 0 { v1 } else { v2 });
                }
                // Variable instructions
                0x20 => { // local.get
                    let local: u32 = read_leb128(bytes, &mut pc)?;
                    let base = *func_bases.last().unwrap();
                    let i = base + local as usize;
                    stack.push(stack[i]);
                }
                0x21 => { // local.set
                    let local: u32 = read_leb128(bytes, &mut pc)?;
                    let val = match stack.pop() {
                        Some(v) => v,
                        None => return Err(Error::trap(STACK_UNDERFLOW))
                    };
                    let base = *func_bases.last().unwrap();
                    let i = base + local as usize;
                    stack[i] = val;
                }
                0x22 => { // local.tee
                    let local: u32 = read_leb128(bytes, &mut pc)?;
                    let val = match stack.last() {
                        Some(v) => *v,
                        None => return Err(Error::trap(STACK_UNDERFLOW))
                    };
                    let base = *func_bases.last().unwrap();
                    let i = base + local as usize;
                    stack[i] = val;
                }
                0x23 => { // global.get
                    let gi: u32 = read_leb128(bytes, &mut pc)?;
                    if gi as usize >= self.globals.len() {
                        return Err(Error::trap(UNKNOWN_GLOBAL));
                    }
                    stack.push(self.globals[gi as usize].value.get());
                }
                0x24 => { // global.set
                    let gi: u32 = read_leb128(bytes, &mut pc)?;
                    if gi as usize >= self.globals.len() {
                        return Err(Error::trap(UNKNOWN_GLOBAL));
                    }
                    let val = match stack.pop() {
                        Some(v) => v,
                        None => return Err(Error::trap(STACK_UNDERFLOW))
                    };
                    self.globals[gi as usize].value.set(val);
                }
                // Memory instructions - loads
                0x28 => { load!(load_u32, |v: u32| WasmValue::from_u32(v)); }
                0x29 => { load!(load_u64, |v: u64| WasmValue::from_u64(v)); }
                0x2a => { load!(load_f32, |v: f32| WasmValue::from_f32(v)); }
                0x2b => { load!(load_f64, |v: f64| WasmValue::from_f64(v)); }
                0x2c => { load!(load_i8,  |v: i8| WasmValue::from_i32(v as i32)); }
                0x2d => { load!(load_u8,  |v: u8| WasmValue::from_u32(v as u32)); }
                0x2e => { load!(load_i16, |v: i16| WasmValue::from_i32(v as i32)); }
                0x2f => { load!(load_u16, |v: u16| WasmValue::from_u32(v as u32)); }
                0x30 => { load!(load_i8,  |v: i8| WasmValue::from_i64(v as i64)); }
                0x31 => { load!(load_u8,  |v: u8| WasmValue::from_u64(v as u64)); }
                0x32 => { load!(load_i16, |v: i16| WasmValue::from_i64(v as i64)); }
                0x33 => { load!(load_u16, |v: u16| WasmValue::from_u64(v as u64)); }
                0x34 => { load!(load_i32, |v: i32| WasmValue::from_i64(v as i64)); }
                0x35 => { load!(load_u32, |v: u32| WasmValue::from_u64(v as u64)); }
                // Memory instructions - stores
                0x36 => { store!(store_u32, |w: WasmValue| w.as_u32()); }
                0x37 => { store!(store_u64, |w: WasmValue| w.as_u64()); }
                0x38 => { store!(store_f32, |w: WasmValue| w.as_f32()); }
                0x39 => { store!(store_f64, |w: WasmValue| w.as_f64()); }
                0x3a => { store!(store_u8,  |w: WasmValue| (w.as_u32() & 0xFF) as u8); }
                0x3b => { store!(store_u16, |w: WasmValue| (w.as_u32() & 0xFFFF) as u16); }
                0x3c => { store!(store_u8,  |w: WasmValue| (w.as_u64() & 0xFF) as u8); }
                0x3d => { store!(store_u16, |w: WasmValue| (w.as_u64() & 0xFFFF) as u16); }
                0x3e => { store!(store_u32, |w: WasmValue| (w.as_u64() & 0xFFFF_FFFF) as u32); }
                // Memory instructions - size/grow
                0x3f => { // memory.size
                    pc += 1; // Skip zero flag
                    let mem = mem
        .ok_or(Error::validation(UNKNOWN_MEMORY))?;
                    stack.push(WasmValue::from_u32(mem.borrow().size()));
                }
                0x40 => { // memory.grow
                    pc += 1; // Skip zero flag
                    let delta = match stack.pop() {
                        Some(v) => v.as_u32(),
                        None => return Err(Error::trap(STACK_UNDERFLOW))
                    };
                    let mem = mem
        .ok_or(Error::validation(UNKNOWN_MEMORY))?;
                    let old = mem.borrow_mut().grow(delta);
                    stack.push(WasmValue::from_u32(old));
                }
                // Numeric instructions - constants
                0x41 => { // i32.const
                    stack.push(WasmValue::from_i32(read_sleb128::<i32>(bytes, &mut pc)?));
                }
                0x42 => { // i64.const
                    stack.push(WasmValue::from_i64(read_sleb128::<i64>(bytes, &mut pc)?));
                }
                0x43 => { // f32.const
                    stack.push(WasmValue::from_f32_bits(u32::from_le_bytes(bytes[pc..pc+4].try_into().unwrap())));
                    pc += 4;
                }
                0x44 => { // f64.const
                    stack.push(WasmValue::from_f64_bits(u64::from_le_bytes(bytes[pc..pc+8].try_into().unwrap())));
                    pc += 8;
                }
                // Numeric instructions - i32 comparison
                0x45 => { unary!(u32, |x: u32| (x == 0) as u32); } // i32.eqz
                0x46 => { compare!(u32, ==); } // i32.eq
                0x47 => { compare!(u32, !=); } // i32.ne
                0x48 => { compare!(i32, <); } // i32.lt_s
                0x49 => { compare!(u32, <); } // i32.lt_u
                0x4a => { compare!(i32, >); } // i32.gt_s
                0x4b => { compare!(u32, >); } // i32.gt_u
                0x4c => { compare!(i32, <=); } // i32.le_s
                0x4d => { compare!(u32, <=); } // i32.le_u
                0x4e => { compare!(i32, >=); } // i32.ge_s
                0x4f => { compare!(u32, >=); } // i32.ge_u
                // Numeric instructions - i64 comparison
                0x50 => { // i64.eqz
                    let v = pop_val!().as_u64();
                    stack.push(WasmValue::from_u32((v == 0) as u32));
                }
                0x51 => { compare!(i64, ==); } // i64.eq
                0x52 => { compare!(i64, !=); } // i64.ne
                0x53 => { compare!(i64, <); } // i64.lt_s
                0x54 => { compare!(u64, <); } // i64.lt_u
                0x55 => { compare!(i64, >); } // i64.gt_s
                0x56 => { compare!(u64, >); } // i64.gt_u
                0x57 => { compare!(i64, <=); } // i64.le_s
                0x58 => { compare!(u64, <=); } // i64.le_u
                0x59 => { compare!(i64, >=); } // i64.ge_s
                0x5a => { compare!(u64, >=); } // i64.ge_u
                // Numeric instructions - f32 comparison
                0x5b => { compare!(f32, ==); } // f32.eq
                0x5c => { compare!(f32, !=); } // f32.ne
                0x5d => { compare!(f32, <); } // f32.lt
                0x5e => { compare!(f32, >); } // f32.gt
                0x5f => { compare!(f32, <=); } // f32.le
                0x60 => { compare!(f32, >=); } // f32.ge
                // Numeric instructions - f64 comparison
                0x61 => { compare!(f64, ==); } // f64.eq
                0x62 => { compare!(f64, !=); } // f64.ne
                0x63 => { compare!(f64, <); } // f64.lt
                0x64 => { compare!(f64, >); } // f64.gt
                0x65 => { compare!(f64, <=); } // f64.le
                0x66 => { compare!(f64, >=); } // f64.ge
                // Numeric instructions - i32 operations
                0x67 => { unary!(u32, |x: u32| x.leading_zeros()); } // i32.clz
                0x68 => { unary!(u32, |x: u32| x.trailing_zeros()); } // i32.ctz
                0x69 => { unary!(u32, |x: u32| x.count_ones()); } // i32.popcnt
                0x6a => { binary!(u32, .wrapping_add); } // i32.add
                0x6b => { binary!(u32, .wrapping_sub); } // i32.sub
                0x6c => { binary!(u32, .wrapping_mul); } // i32.mul
                0x6d => { div_s!(i32); } // i32.div_s
                0x6e => { div_u!(u32); } // i32.div_u
                0x6f => { rem_s!(i32); } // i32.rem_s
                0x70 => { rem_u!(u32); } // i32.rem_u
                0x71 => { binary!(u32, &); } // i32.and
                0x72 => { binary!(u32, |); } // i32.or
                0x73 => { binary!(u32, ^); } // i32.xor
                0x74 => { shift!(u32, <<); } // i32.shl
                0x75 => { shr_s!(i32, u32, 32); } // i32.shr_s
                0x76 => { shift!(u32, >>); } // i32.shr_u
                0x77 => { rotate!(u32, left); } // i32.rotl
                0x78 => { rotate!(u32, right); } // i32.rotr
                // Numeric instructions - i64 operations
                0x79 => { unary!(u64, |x: u64| x.leading_zeros() as u64); } // i64.clz
                0x7a => { unary!(u64, |x: u64| x.trailing_zeros() as u64); } // i64.ctz
                0x7b => { unary!(u64, |x: u64| x.count_ones() as u64); } // i64.popcnt
                0x7c => { binary!(u64, .wrapping_add); } // i64.add
                0x7d => { binary!(u64, .wrapping_sub); } // i64.sub
                0x7e => { binary!(u64, .wrapping_mul); } // i64.mul
                0x7f => { div_s!(i64); } // i64.div_s
                0x80 => { div_u!(u64); } // i64.div_u
                0x81 => { rem_s!(i64); } // i64.rem_s
                0x82 => { rem_u!(u64); } // i64.rem_u
                0x83 => { binary!(u64, &); } // i64.and
                0x84 => { binary!(u64, |); } // i64.or
                0x85 => { binary!(u64, ^); } // i64.xor
                0x86 => { shift!(u64, <<); } // i64.shl
                0x87 => { shr_s!(i64, u64, 64); } // i64.shr_s
                0x88 => { shift!(u64, >>); } // i64.shr_u
                0x89 => { rotate!(u64, left); } // i64.rotl
                0x8a => { rotate!(u64, right); } // i64.rotr
                // Numeric instructions - f32 operations
                0x8b => { unary!(f32, |x: f32| x.abs()); } // f32.abs
                0x8c => { unary!(f32, |x: f32| -x); } // f32.neg
                0x8d => { unary!(f32, |x: f32| x.ceil()); } // f32.ceil
                0x8e => { unary!(f32, |x: f32| x.floor()); } // f32.floor
                0x8f => { unary!(f32, |x: f32| x.trunc()); } // f32.trunc
                0x90 => { nearest!(f32); } // f32.nearest
                0x91 => { unary!(f32, |x: f32| x.sqrt()); } // f32.sqrt
                0x92 => { binary!(f32, +); } // f32.add
                0x93 => { binary!(f32, -); } // f32.sub
                0x94 => { binary!(f32, *); } // f32.mul
                0x95 => { binary!(f32, /); } // f32.div
                0x96 => { minmax!(f32, min); } // f32.min
                0x97 => { minmax!(f32, max); } // f32.max
                0x98 => { copysign!(f32); } // f32.copysign
                // Numeric instructions - f64 operations
                0x99 => { unary!(f64, |x: f64| x.abs()); } // f64.abs
                0x9a => { unary!(f64, |x: f64| -x); } // f64.neg
                0x9b => { unary!(f64, |x: f64| x.ceil()); } // f64.ceil
                0x9c => { unary!(f64, |x: f64| x.floor()); } // f64.floor
                0x9d => { unary!(f64, |x: f64| x.trunc()); } // f64.trunc
                0x9e => { nearest!(f64); } // f64.nearest
                0x9f => { unary!(f64, |x: f64| x.sqrt()); } // f64.sqrt
                0xa0 => { binary!(f64, +); } // f64.add
                0xa1 => { binary!(f64, -); } // f64.sub
                0xa2 => { binary!(f64, *); } // f64.mul
                0xa3 => { binary!(f64, /); } // f64.div
                0xa4 => { minmax!(f64, min); } // f64.min
                0xa5 => { minmax!(f64, max); } // f64.max
                0xa6 => { copysign!(f64); } // f64.copysign
                // Conversions and truncations
                0xa7 => { convert!(u64 -> u32); } // i32.wrap_i64
                0xa8 => { trunc!(f32 -> i32 : -2147483777.0, 2147483648.0); } // i32.trunc_f32_s
                0xa9 => { trunc!(f32 -> u32 : -1.0, 4294967296.0); } // i32.trunc_f32_u
                0xaa => { trunc!(f64 -> i32 : -2147483649.0, 2147483648.0); } // i32.trunc_f64_s
                0xab => { trunc!(f64 -> u32 : -1.0, 4294967296.0); } // i32.trunc_f64_u
                0xac => { convert!(i32 -> i64); } // i64.extend_i32_s
                0xad => { convert!(u32 -> u64); } // i64.extend_i32_u
                0xae => { trunc!(f32 -> i64 : -9223373136366404000.0, 9223372036854776000.0); } // i64.trunc_f32_s
                0xaf => { trunc!(f32 -> u64 : -1.0, 18446744073709552000.0); } // i64.trunc_f32_u
                0xb0 => { trunc!(f64 -> i64 : -9223372036854777856.0, 9223372036854776000.0); } // i64.trunc_f64_s
                0xb1 => { trunc!(f64 -> u64 : -1.0, 18446744073709552000.0); } // i64.trunc_f64_u
                // Float conversions from integers
                0xb2 => { convert!(i32 -> f32); } // f32.convert_i32_s
                0xb3 => { convert!(u32 -> f32); } // f32.convert_i32_u
                0xb4 => { convert!(i64 -> f32); } // f32.convert_i64_s
                0xb5 => { convert!(u64 -> f32); } // f32.convert_i64_u
                0xb6 => { convert!(f64 -> f32); } // f32.demote_f64
                0xb7 => { convert!(i32 -> f64); } // f64.convert_i32_s
                0xb8 => { convert!(u32 -> f64); } // f64.convert_i32_u
                0xb9 => { convert!(i64 -> f64); } // f64.convert_i64_s
                0xba => { convert!(u64 -> f64); } // f64.convert_i64_u
                0xbb => { convert!(f32 -> f64); } // f64.promote_f32
                _ => {
                    return Err(Error::malformed(UNKNOWN_INSTRUCTION));
                }
            }
        }
    }

    #[inline]
    fn branch(pc: &mut usize, stack: &mut Vec<WasmValue>, control: &mut Vec<ControlFrame>, depth: u32) -> bool {
        let len = control.len();
        if depth as usize >= len { return true; }
        let keep = len - depth as usize;
        control.truncate(keep);
        let Some(target) = control.pop() else { return true; };
        let result_arity = target.arity as usize;

        if result_arity > 0 {
            let stack_len = stack.len();
            let src_start = stack_len.saturating_sub(result_arity);

            if src_start > target.stack_len {
                stack.copy_within(src_start..stack_len, target.stack_len);
            }
            stack.truncate(target.stack_len + result_arity);
        } else {
            stack.truncate(target.stack_len);
        }

        *pc = target.dest_pc;
        control.is_empty()
    }

    pub fn invoke(&self, func: &RuntimeFunction, args: &[WasmValue]) -> Result<Vec<WasmValue>, Error> {
        let n_params = func.param_count();
        if n_params != args.len() { return Err(Error::trap(INVALID_NUM_ARG)); }

        let mut stack: Vec<WasmValue> = Vec::with_capacity(1024);
        for v in args { stack.push(*v); }
        let mut control: Vec<ControlFrame> = Vec::new();
        let mut func_bases: Vec<usize> = Vec::new();
        let return_pc: usize = 0;

        match func {
            RuntimeFunction::OwnedWasm { runtime_sig, pc_start, locals_count } => {
                let mut ctrl_bases = Vec::new();
                let pc = Self::setup_wasm_function_call(*runtime_sig, *pc_start, *locals_count, &mut stack, &mut control, &mut func_bases, &mut ctrl_bases, return_pc)?;
                self.interpret(pc, &mut stack, &mut control, &mut func_bases, &mut ctrl_bases)?;
            }
            RuntimeFunction::ImportedWasm { owner, function_index, .. } => {
                if let Some(owner_rc) = owner.upgrade() {
                    let mut owned_stack = Vec::with_capacity(64);
                    owned_stack.extend_from_slice(args);
                    let mut control: Vec<ControlFrame> = Vec::new();
                    let mut return_pc: usize = 0;
                    let mut func_bases: Vec<usize> = Vec::new();
                    let mut ctrl_bases = vec![];
                    owner_rc.call_function_idx(*function_index, &mut return_pc, &mut owned_stack, &mut control, &mut func_bases, &mut ctrl_bases)?;
                    return Ok(owned_stack);
                } else {
                    return Err(Error::trap(FUNC_NO_IMPL));
                }
            }
            RuntimeFunction::Host { callback, .. } => {
                if let Some(result) = callback(&stack) {
                    stack.clear();
                    stack.push(result);
                } else {
                    stack.clear();
                }
            }
        }
        Ok(stack)
    }
}