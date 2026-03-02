use crate::error::*;
use crate::leb128::{read_leb128, read_sleb128};
use crate::module::ExternType;
use crate::opcodes::*;
use crate::signature::{RuntimeSignature, Signature, ValType};
use crate::wasm_memory::WasmMemory;
use crate::Module;
use paste::paste;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

#[derive(Copy, Clone, Default)]
pub struct WasmValue(pub u64);

#[rustfmt::skip]
impl WasmValue {
    #[inline(always)] pub fn from_i32(v: i32) -> Self { Self(v as u32 as u64) }
    #[inline(always)] pub fn as_i32(self) -> i32 { self.0 as u32 as i32 }
    #[inline(always)] pub fn from_u32(v: u32) -> Self { Self(v as u64) }
    #[inline(always)] pub fn as_u32(self) -> u32 { self.0 as u32 }
    #[inline(always)] pub fn from_i64(v: i64) -> Self { Self(v as u64) }
    #[inline(always)] pub fn as_i64(self) -> i64 { self.0 as i64 }
    #[inline(always)] pub fn from_u64(v: u64) -> Self { Self(v) }
    #[inline(always)] pub fn as_u64(self) -> u64 { self.0 }
    #[inline(always)] pub fn from_f32_bits(bits: u32) -> Self { Self(bits as u64) }
    #[inline(always)] pub fn as_f32_bits(self) -> u32 { self.0 as u32 }
    #[inline(always)] pub fn from_f64_bits(bits: u64) -> Self { Self(bits) }
    #[inline(always)] pub fn as_f64_bits(self) -> u64 { self.0 }
    #[inline(always)] pub fn from_f32(v: f32) -> Self { Self::from_f32_bits(v.to_bits()) }
    #[inline(always)] pub fn as_f32(self) -> f32 { f32::from_bits(self.as_f32_bits()) }
    #[inline(always)] pub fn from_f64(v: f64) -> Self { Self::from_f64_bits(v.to_bits()) }
    #[inline(always)] pub fn as_f64(self) -> f64 { f64::from_bits(self.as_f64_bits()) }
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
        Self { handle: ((owner_id as u64) << 32) | ((func_idx as u64) + 1) }
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

    fn as_raw(&self) -> u64 {
        self.handle
    }
    fn owner_id(&self) -> u32 {
        (self.handle >> 32) as u32
    }
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
    fn default() -> Self {
        Self::NULL
    }
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
    elements: Vec<FuncRef>,
    current: u32,
    maximum: u32,
}

#[rustfmt::skip]
impl WasmTable {
    pub fn new(initial: u32, maximum: u32) -> Self { Self { elements: vec![FuncRef::default(); initial as usize], current: initial, maximum } }
    pub fn size(&self) -> u32 { self.current }
    pub fn max(&self) -> u32 { self.maximum }
}

impl WasmTable {
    pub fn grow(&mut self, delta: u32, value: WasmValue) -> u32 {
        if delta == 0 {
            return self.current;
        }
        if delta > self.maximum.saturating_sub(self.current) {
            return u32::MAX;
        }
        let new_current = self.current + delta;
        let func_ref = FuncRef::from_raw(value.as_u64());
        self.elements.resize(new_current as usize, func_ref);
        let old = self.current;
        self.current = new_current;
        old
    }
    #[inline(always)]
    pub fn get(&self, idx: u32) -> Result<WasmValue, &'static str> {
        let i = idx as usize;
        if i >= self.elements.len() {
            return Err(OOB_TABLE_ACCESS);
        }
        Ok(WasmValue::from_u64(self.elements[i].as_raw()))
    }
    #[inline(always)]
    pub fn set(&mut self, idx: u32, value: WasmValue) -> Result<(), &'static str> {
        let i = idx as usize;
        if i >= self.elements.len() {
            return Err(OOB_TABLE_ACCESS);
        }
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
    },
}

impl RuntimeFunction {
    #[inline(always)]
    pub fn signature(&self) -> RuntimeSignature {
        match self {
            RuntimeFunction::OwnedWasm { runtime_sig, .. } => *runtime_sig,
            RuntimeFunction::ImportedWasm { runtime_sig, .. } => *runtime_sig,
            RuntimeFunction::Host { runtime_sig, .. } => *runtime_sig,
        }
    }

    #[inline(always)]
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

#[repr(C)]
struct ControlFrame {
    stack_len: u32,
    dest_pc: u32,
    arity: u32,
    has_result: u32,
}

#[derive(Copy, Clone)]
struct CallFrame {
    stack_base: usize,
    ctrl_index: usize,
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
    /// Register or re-register an instance, used for testing when wrapping in a new Rc
    pub fn register_external_instance(inst: &Rc<Instance>) {
        // This updates the registry entry even if the instance was already registered
        InstanceManager::with(|mgr| mgr.register_instance(inst));
    }

    fn resolve_import<'a>(
        imports: &'a Imports,
        import_ref: &crate::module::ImportRef,
    ) -> Result<&'a ExportValue, Error> {
        imports
            .get(&import_ref.module)
            .and_then(|m| m.get(&import_ref.field))
            .ok_or(Error::link(UNKNOWN_IMPORT))
    }

    pub fn instantiate(module: Rc<Module>, imports: &Imports) -> Result<Self, Error> {
        // Build the instance inside a Rc so we can register a Weak handle
        // for cross-instance func_ref dispatch even if instantiation ultimately fails.
        let mut inst_rc = Rc::new(Instance { module: module.clone(), ..Default::default() });
        {
            // Configure the instance while we hold the only strong Rc
            let inst = Rc::get_mut(&mut inst_rc).expect("sole owner expected");
            inst.id = InstanceManager::with(|mgr| mgr.allocate_id());

            // Memory
            if let Some(memory) = &module.memory {
                if let Some(import_ref) = &memory.import {
                    let imported = Self::resolve_import(imports, import_ref)?;
                    match imported {
                        ExportValue::Memory(mem) => {
                            let m = mem.borrow();
                            if m.size() < memory.min || m.max() > memory.max {
                                return Err(Error::link(INCOMPATIBLE_IMPORT));
                            }
                            drop(m);
                            inst.memory = Some(mem.clone());
                        }
                        _ => return Err(Error::link(INCOMPATIBLE_IMPORT)),
                    }
                } else {
                    inst.memory =
                        Some(Rc::new(RefCell::new(WasmMemory::new(memory.min, memory.max))));
                }
            }

            // Tables
            if let Some(table) = &module.table {
                if let Some(import_ref) = &table.import {
                    let imported = Self::resolve_import(imports, import_ref)?;
                    match imported {
                        ExportValue::Table(tab) => {
                            let tb = tab.borrow();
                            if tb.size() < table.min || tb.max() > table.max {
                                return Err(Error::link(INCOMPATIBLE_IMPORT));
                            }
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
                if let Some(import_ref) = &function.import {
                    let imported = Self::resolve_import(imports, import_ref)?;
                    let runtime_sig = RuntimeSignature::from_signature(&function.ty);
                    match imported {
                        ExportValue::Function(f) => {
                            if f.signature() != runtime_sig {
                                return Err(Error::link(INCOMPATIBLE_IMPORT));
                            }
                            inst.functions.push(f.clone());
                        }
                        _ => return Err(Error::link(INCOMPATIBLE_IMPORT)),
                    }
                } else {
                    let locals_count =
                        function.locals.len().saturating_sub(function.ty.params.len());
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
                if let Some(import_ref) = &g.import {
                    let imported = Self::resolve_import(imports, import_ref)?;
                    match imported {
                        ExportValue::Global(gl) => {
                            let gb = gl.as_ref();
                            if gb.ty != g.ty || gb.mutable != g.is_mutable {
                                return Err(Error::link(INCOMPATIBLE_IMPORT));
                            }
                            inst.globals.push(gl.clone());
                        }
                        _ => return Err(Error::link(INCOMPATIBLE_IMPORT)),
                    }
                } else {
                    // evaluate constant initializer
                    let mut cpc = g.initializer_offset;
                    let val = Instance::eval_const(&module, &mut cpc, &inst.globals)?;
                    inst.globals.push(Rc::new(WasmGlobal {
                        ty: g.ty,
                        mutable: g.is_mutable,
                        value: Cell::new(val),
                    }));
                }
            }

            // Collect element segments (validate bounds, defer writes)
            let mut collected_elements: Vec<(u32, Vec<u32>)> = Vec::new();
            if module.element_count > 0 {
                if inst.table.is_none() {
                    return Err(Error::link(UNKNOWN_TABLE));
                }
                let bytes = &module.bytes;
                let mut it = module.element_start;
                collected_elements.reserve(module.element_count as usize);
                for _ in 0..module.element_count {
                    let flags: u32 = read_leb128(bytes, &mut it)?;
                    if flags != 0 {
                        return Err(Error::malformed(INVALID_VALUE_TYPE));
                    }
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
                    collected_elements.push((offset, indices));
                }
            }

            // Validate data segments (bounds check, defer writes)
            let mut pending_data: Vec<(u32, usize, usize)> = Vec::new();
            if let Some(mem) = &inst.memory {
                for seg in &module.data_segments {
                    let mut ip = seg.initializer_offset;
                    let offset = Instance::eval_const(&module, &mut ip, &inst.globals)?.as_u32();
                    let data_len = seg.data_range.end - seg.data_range.start;
                    let m = mem.borrow();
                    let end = (offset as usize).saturating_add(data_len);
                    if end > (m.size() as usize) * (WasmMemory::PAGE_SIZE as usize) {
                        return Err(Error::link(DATA_SEG_DNF));
                    }
                    drop(m);
                    pending_data.push((offset, seg.data_range.start, seg.data_range.end));
                }
            }

            // Apply element segments now that data segments have been validated
            if !collected_elements.is_empty() {
                let table_rc = inst.table.as_ref().ok_or(Error::link(UNKNOWN_TABLE))?.clone();
                for (offset, indices) in &collected_elements {
                    for (j, idx) in indices.iter().enumerate() {
                        let func_idx = *idx as usize;
                        let (owner_id, owner_func_idx) = match &inst.functions[func_idx] {
                            RuntimeFunction::ImportedWasm { owner, function_index, .. } => {
                                if let Some(owner_rc) = owner.upgrade() {
                                    (owner_rc.id, *function_index as u32)
                                } else {
                                    (inst.id, func_idx as u32)
                                }
                            }
                            RuntimeFunction::OwnedWasm { .. } | RuntimeFunction::Host { .. } => {
                                (inst.id, func_idx as u32)
                            }
                        };
                        let func_ref = FuncRef::new(owner_id, owner_func_idx);
                        let func_ref_value = WasmValue::from_u64(func_ref.as_raw());
                        if table_rc.borrow_mut().set(*offset + (j as u32), func_ref_value).is_err()
                        {
                            return Err(Error::link(ELEM_SEG_DNF));
                        }
                    }
                }
            }

            // Apply data segments (writes), after elements
            if let Some(mem) = &inst.memory {
                let mut m = mem.borrow_mut();
                for &(offset, start, end) in &pending_data {
                    m.write_bytes(offset, &module.bytes[start..end]).map_err(Error::trap)?;
                }
            }

            // Exports
            for (name, ex) in &module.exports {
                match ex.extern_type {
                    ExternType::Func => {
                        inst.exports.insert(
                            name.clone(),
                            ExportValue::Function(inst.functions[ex.idx as usize].clone()),
                        );
                    }
                    ExternType::Table => {
                        if let Some(table) = &inst.table {
                            inst.exports.insert(name.clone(), ExportValue::Table(table.clone()));
                        }
                    }
                    ExternType::Mem => {
                        if let Some(mem) = &inst.memory {
                            inst.exports.insert(name.clone(), ExportValue::Memory(mem.clone()));
                        }
                    }
                    ExternType::Global => {
                        inst.exports.insert(
                            name.clone(),
                            ExportValue::Global(inst.globals[ex.idx as usize].clone()),
                        );
                    }
                }
            }
        }

        // Register a weak reference before potential start execution so that
        // even if start traps, func_refs already stored in tables can resolve
        // the owning instance via the registry
        InstanceManager::with(|mgr| mgr.register_instance(&inst_rc));

        // Start
        if let Some(start_idx) = module.start {
            let fi = start_idx as usize;
            let function = &inst_rc.functions[fi];
            if function.signature().n_params() != 0 || function.signature().has_result() {
                return Err(Error::validation(START_FUNC));
            }
            let mut stack: Vec<WasmValue> = Vec::with_capacity(64);
            let mut return_pc = 0usize;
            let mut control: Vec<ControlFrame> = Vec::with_capacity(16);
            let mut call_frames: Vec<CallFrame> = Vec::with_capacity(8);
            match inst_rc.call_function_idx(
                fi,
                &mut return_pc,
                &mut stack,
                &mut control,
                &mut call_frames,
            ) {
                Ok(()) => {}
                Err(Error::Trap(msg)) => {
                    // If there are live func_ref references to this instance,
                    // keep it alive as a zombie until all references are dropped
                    InstanceManager::with(|mgr| mgr.add_zombie(inst_rc));
                    return Err(Error::uninstantiable(msg));
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }

        // Success: unwrap Rc to return by value
        match Rc::try_unwrap(inst_rc) {
            Ok(inst) => Ok(inst),
            Err(_) => unreachable!("unexpected extra strong refs while instantiating"),
        }
    }

    #[rustfmt::skip]
    fn eval_const(
        module: &Module,
        pc: &mut usize,
        globals: &[Rc<WasmGlobal>]
    ) -> Result<WasmValue, Error> {
        let bytes: &[u8] = &module.bytes;
        let mut stack: Vec<WasmValue> = Vec::with_capacity(4);
        loop {
            let op = bytes[*pc]; *pc += 1;
            match op {
                I32_CONST => { let v: i32 = read_sleb128(bytes, pc)?; stack.push(WasmValue::from_i32(v)); }
                I64_CONST => { let v: i64 = read_sleb128(bytes, pc)?; stack.push(WasmValue::from_i64(v)); }
                F32_CONST => { let bits = u32::from_le_bytes(bytes[*pc..*pc+4].try_into().unwrap()); *pc += 4; stack.push(WasmValue::from_f32_bits(bits)); }
                F64_CONST => { let bits = u64::from_le_bytes(bytes[*pc..*pc+8].try_into().unwrap()); *pc += 8; stack.push(WasmValue::from_f64_bits(bits)); }
                GLOBAL_GET => { let gi: u32 = read_leb128(bytes, pc)?; let g = gi as usize; if g >= globals.len() { return Err(Error::validation(UNKNOWN_GLOBAL)); }stack.push(globals[g].value.get()); }
                I32_ADD => { let b = stack.pop().unwrap().as_u32(); let a = stack.pop().unwrap().as_u32(); stack.push(WasmValue::from_u32(a.wrapping_add(b))); }
                I32_SUB => { let b = stack.pop().unwrap().as_u32(); let a = stack.pop().unwrap().as_u32(); stack.push(WasmValue::from_u32(a.wrapping_sub(b))); }
                I32_MUL => { let b = stack.pop().unwrap().as_u32(); let a = stack.pop().unwrap().as_u32(); stack.push(WasmValue::from_u32(a.wrapping_mul(b))); }
                I64_ADD => { let b = stack.pop().unwrap().as_u64(); let a = stack.pop().unwrap().as_u64(); stack.push(WasmValue::from_u64(a.wrapping_add(b))); }
                I64_SUB => { let b = stack.pop().unwrap().as_u64(); let a = stack.pop().unwrap().as_u64(); stack.push(WasmValue::from_u64(a.wrapping_sub(b))); }
                I64_MUL => { let b = stack.pop().unwrap().as_u64(); let a = stack.pop().unwrap().as_u64(); stack.push(WasmValue::from_u64(a.wrapping_mul(b))); }
                END => break,
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
        call_frames: &mut Vec<CallFrame>,
        return_dest: usize,
    ) -> Result<usize, Error> {
        let n_params = runtime_sig.n_params() as usize;
        let has_result = runtime_sig.has_result();
        let locals_start = stack.len() - n_params;

        // Allocate space for local variables
        stack.resize(stack.len() + locals_count, WasmValue::default());

        // Push return target
        control.push(ControlFrame {
            stack_len: locals_start as u32,
            dest_pc: return_dest as u32,
            arity: if has_result { 1 } else { 0 },
            has_result: has_result as u32,
        });

        const MAX_CONTROL_DEPTH: usize = 1000;
        if control.len() > MAX_CONTROL_DEPTH {
            return Err(Error::trap(STACK_EXHAUSTED));
        }

        // Track function frame
        call_frames.push(CallFrame { stack_base: locals_start, ctrl_index: control.len() - 1 });

        // Return the function's start PC
        Ok(pc_start)
    }

    /// Dispatch a host function call, handling params and optional result.
    #[inline(always)]
    fn call_host(
        callback: &dyn Fn(&[WasmValue]) -> Option<WasmValue>,
        runtime_sig: RuntimeSignature,
        stack: &mut Vec<WasmValue>,
    ) {
        let param_count = runtime_sig.n_params() as usize;
        let params_start = stack.len() - param_count;
        if let Some(result) = callback(&stack[params_start..]) {
            stack.truncate(params_start);
            stack.push(result);
        } else {
            stack.truncate(params_start);
        }
    }

    /// Dispatch a cross-instance call by copying params to a temporary stack.
    fn call_remote(
        owner: &Instance,
        function_index: usize,
        n_params: usize,
        stack: &mut Vec<WasmValue>,
    ) -> Result<(), Error> {
        let params_start = stack.len() - n_params;
        let mut tmp_stack: Vec<WasmValue> = Vec::with_capacity(n_params);
        tmp_stack.extend_from_slice(&stack[params_start..(params_start + n_params)]);
        stack.truncate(params_start);
        let mut control_nested: Vec<ControlFrame> = Vec::with_capacity(16);
        let mut ret_pc_nested = 0usize;
        let mut call_frames_nested: Vec<CallFrame> = Vec::with_capacity(8);
        owner.call_function_idx(
            function_index,
            &mut ret_pc_nested,
            &mut tmp_stack,
            &mut control_nested,
            &mut call_frames_nested,
        )?;
        stack.extend(tmp_stack);
        Ok(())
    }

    #[inline(always)]
    fn call_function_idx(
        &self,
        idx: usize,
        return_pc: &mut usize,
        stack: &mut Vec<WasmValue>,
        control: &mut Vec<ControlFrame>,
        call_frames: &mut Vec<CallFrame>,
    ) -> Result<(), Error> {
        const MAX_CALL_DEPTH: usize = 1000;
        if call_frames.len() >= MAX_CALL_DEPTH {
            return Err(Error::trap(STACK_EXHAUSTED));
        }
        let fi = &self.functions[idx];
        match fi {
            RuntimeFunction::OwnedWasm { runtime_sig, pc_start, locals_count } => {
                let pc = Self::setup_wasm_function_call(
                    *runtime_sig,
                    *pc_start,
                    *locals_count,
                    stack,
                    control,
                    call_frames,
                    *return_pc,
                )?;
                self.interpret(pc, stack, control, call_frames)?;
            }
            RuntimeFunction::ImportedWasm { owner, function_index, .. } => {
                if let Some(owner_rc) = owner.upgrade() {
                    owner_rc.call_function_idx(
                        *function_index,
                        return_pc,
                        stack,
                        control,
                        call_frames,
                    )?;
                } else {
                    return Err(Error::trap(FUNC_NO_IMPL));
                }
            }
            RuntimeFunction::Host { callback, runtime_sig } => {
                Self::call_host(callback.as_ref(), *runtime_sig, stack);
            }
        }
        Ok(())
    }

    #[rustfmt::skip]
    fn interpret(
        &self,
        mut pc: usize,
        stack: &mut Vec<WasmValue>,
        control: &mut Vec<ControlFrame>,
        call_frames: &mut Vec<CallFrame>,
    ) -> Result<(), Error> {
        let bytes: &[u8] = &self.module.bytes;
        let mem = self.memory.as_ref();
        let tab = self.table.as_ref();
        let mut current_base = call_frames.last().unwrap().stack_base;

        macro_rules! next_op { () => {{ let byte = unsafe { *bytes.get_unchecked(pc) }; pc += 1; byte }} }
        macro_rules! pop_val { () => {{
            match stack.pop() { Some(v) => v, None => return Err(Error::trap(STACK_UNDERFLOW)) }
        }} }
        macro_rules! overwrite {
            ($val:expr) => {{
                let len = stack.len();
                *unsafe { stack.get_unchecked_mut(len - 1) } = $val;
            }}
        }
        macro_rules! peek_one {
            ($type:ident) => {{
                paste! {
                    let len = stack.len();
                    if len < 1 { return Err(Error::trap(STACK_UNDERFLOW)); }
                    unsafe { stack.get_unchecked(len - 1) }.[<as_ $type>]()
                }
            }}
        }
        macro_rules! peek_two {
            ($type:ident) => {{
                paste! {
                    let len = stack.len();
                    if len < 2 { return Err(Error::trap(STACK_UNDERFLOW)); }
                    let a = unsafe { stack.get_unchecked(len - 2) }.[<as_ $type>]();
                    let b = unsafe { stack.get_unchecked(len - 1) }.[<as_ $type>]();
                    unsafe { stack.set_len(len - 1); }
                    (a, b)
                }
            }}
        }
        macro_rules! binary {
            ($type:ident, $op:tt) => {{
                paste! {
                    let (a, b) = peek_two!($type);
                    overwrite!(WasmValue::[<from_ $type>](a $op b));
                }
            }};
            ($type:ident, .$method:ident) => {{
                paste! {
                    let (a, b) = peek_two!($type);
                    overwrite!(WasmValue::[<from_ $type>](a.$method(b)));
                }
            }};
        }
        macro_rules! compare {
            ($type:ident, $op:tt) => {{
                paste! {
                    let (a, b) = peek_two!($type);
                    overwrite!(WasmValue::from_u32((a $op b) as u32));
                }
            }};
        }
        macro_rules! shift {
            (u32, $op:tt) => {{
                let (a, b) = peek_two!(u32);
                overwrite!(WasmValue::from_u32(a $op (b % 32)));
            }};
            (u64, $op:tt) => {{
                let (a, b) = peek_two!(u64);
                overwrite!(WasmValue::from_u64(a $op (b % 64)));
            }};
        }
        macro_rules! rotate {
            (u32, $dir:ident) => {{
                let (a, b) = peek_two!(u32);
                paste! {
                    overwrite!(WasmValue::from_u32(a.[<rotate_ $dir>](b % 32)));
                }
            }};
            (u64, $dir:ident) => {{
                let (a, b) = peek_two!(u64);
                paste! {
                    overwrite!(WasmValue::from_u64(a.[<rotate_ $dir>]((b % 64) as u32)));
                }
            }};
        }
        macro_rules! unary {
            ($type:ident, $f:expr) => {{
                paste! {
                    let a = peek_one!($type);
                    overwrite!(WasmValue::[<from_ $type>]($f(a)));
                }
            }};
        }
        macro_rules! minmax {
            ($type:ident, min) => {{ minmax!(@impl $type, min, true) }};
            ($type:ident, max) => {{ minmax!(@impl $type, max, false) }};
            (@impl $type:ident, $op:ident, $want_negative:literal) => {{
                paste! {
                    let (a, b) = peek_two!($type);

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

                    overwrite!(WasmValue::[<from_ $type>](result));
                }
            }};
        }
        macro_rules! shr_s {
            ($int_type:ident, $uint_type:ident, $bits:literal) => {{
                paste! {
                    let (a, b) = peek_two!($int_type);
                    overwrite!(WasmValue::[<from_ $int_type>](a >> (b as $uint_type % $bits)));
                }
            }};
        }
        macro_rules! copysign {
            ($type:ident) => {{
                paste! {
                    let (a, b) = peek_two!($type);
                    overwrite!(WasmValue::[<from_ $type>](a.copysign(b)));
                }
            }};
        }
        macro_rules! nearest {
            ($type:ident) => {{
                paste! {
                    let x = peek_one!($type);
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
                    overwrite!(WasmValue::[<from_ $type>](y));
                }
            }};
        }
        macro_rules! convert {
            ($src_type:ident -> $dst_type:ident) => {{
                paste! {
                    let v = peek_one!($src_type);
                    overwrite!(WasmValue::[<from_ $dst_type>](v as $dst_type));
                }
            }};
        }
        macro_rules! trunc {
            ($src_type:ident -> $dst_type:ident : $min:expr, $max:expr) => {{
                paste! {
                    let x = peek_one!($src_type);
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
                    overwrite!(WasmValue::[<from_ $dst_type>](x as $dst_type));
                }
            }};
        }
        macro_rules! div_s {
            ($int_type:ident) => {{
                paste! {
                    let (a, b) = peek_two!($int_type);
                    if b == 0 { return Err(Error::trap(DIVIDE_BY_ZERO)); }
                    if a == $int_type::MIN && b == -1 { return Err(Error::trap(INTEGER_OVERFLOW)); }
                    overwrite!(WasmValue::[<from_ $int_type>](a / b));
                }
            }};
        }
        macro_rules! div_u {
            ($uint_type:ident) => {{
                paste! {
                    let (a, b) = peek_two!($uint_type);
                    if b == 0 { return Err(Error::trap(DIVIDE_BY_ZERO)); }
                    overwrite!(WasmValue::[<from_ $uint_type>](a / b));
                }
            }};
        }
        macro_rules! rem_s {
            ($int_type:ident) => {{
                paste! {
                    let (a, b) = peek_two!($int_type);
                    if b == 0 { return Err(Error::trap(DIVIDE_BY_ZERO)); }
                    let result = if a == $int_type::MIN && b == -1 { 0 } else { a % b };
                    overwrite!(WasmValue::[<from_ $int_type>](result));
                }
            }};
        }
        macro_rules! rem_u {
            ($uint_type:ident) => {{
                paste! {
                    let (a, b) = peek_two!($uint_type);
                    if b == 0 { return Err(Error::trap(DIVIDE_BY_ZERO)); }
                    overwrite!(WasmValue::[<from_ $uint_type>](a % b));
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
            match next_op!() {
                OP_UNREACHABLE => return Err(Error::trap(UNREACHABLE)),
                // nop and reinterprets (no-op on raw bits)
                NOP | I32_REINTERPRET_F32 | I64_REINTERPRET_F64 | F32_REINTERPRET_I32 | F64_REINTERPRET_I64 => {}
                BLOCK => {
                    let (body_pc, end_pc, _else_pc, params_len, has_result) =
                        self.module.side_table.lookup(pc).unwrap();
                    pc = body_pc;
                    control.push(ControlFrame {
                        stack_len: (stack.len() - (params_len as usize)) as u32,
                        dest_pc: end_pc as u32,
                        arity: has_result as u32,
                        has_result: has_result as u32,
                    });
                }
                LOOP => {
                    let loop_op_pc = pc - 1;
                    let (body_pc, _end_pc, _else_pc, params_len, has_result) =
                        self.module.side_table.lookup(pc).unwrap();
                    pc = body_pc;
                    control.push(ControlFrame {
                        stack_len: (stack.len() - (params_len as usize)) as u32,
                        dest_pc: loop_op_pc as u32,
                        arity: params_len as u32,
                        has_result: has_result as u32,
                    });
                }
                IF => {
                    let (body_pc, end_pc, else_pc, params_len, has_result) =
                        self.module.side_table.lookup(pc).unwrap();
                    let cond = pop_val!().as_u32();
                    control.push(ControlFrame {
                        stack_len: (stack.len() - (params_len as usize)) as u32,
                        dest_pc: end_pc as u32,
                        arity: has_result as u32,
                        has_result: has_result as u32,
                    });
                    pc = if cond == 0 { else_pc } else { body_pc };
                }
                ELSE => {
                    let _ = Instance::branch(&mut pc, stack, control, 0);
                }
                END => {
                    // Check if we're at a function boundary
                    if let Some(frame) = call_frames.last() {
                        if frame.ctrl_index == control.len().saturating_sub(1) {
                            if Instance::branch(&mut pc, stack, control, 0) {
                                call_frames.pop();
                                return Ok(());
                            }
                            call_frames.pop();
                            current_base = call_frames.last().unwrap().stack_base;
                            continue; // Skip the regular block logic
                        }
                    }

                    // Regular block end (not a function boundary)
                    if let Some(target) = control.pop() {
                        let sl = target.stack_len as usize;
                        if target.has_result != 0 {
                            let result = stack[stack.len() - 1];
                            stack.truncate(sl + 1);
                            stack[sl] = result;
                        } else {
                            stack.truncate(sl);
                        }
                    } else {
                        return Ok(()); // No more control frames
                    }
                }
                BR => {
                    let depth: u32 = read_leb128(bytes, &mut pc)?;
                    if Instance::branch(&mut pc, stack, control, depth) { return Ok(()); }
                }
                BR_IF => {
                    let depth: u32 = read_leb128(bytes, &mut pc)?;
                    let cond = pop_val!().as_u32();
                    if cond != 0 && Instance::branch(&mut pc, stack, control, depth) { return Ok(()); }
                }
                BR_TABLE => {
                    let v = pop_val!().as_u32();
                    let depth = self.module.side_table.lookup_br_table(pc, v).unwrap();
                    if Instance::branch(&mut pc, stack, control, depth) { return Ok(()); }
                }
                RETURN => {
                    if control.is_empty() { return Ok(()); }
                    let base_idx = call_frames.last().unwrap().ctrl_index;
                    let depth = (control.len() - 1).saturating_sub(base_idx) as u32;
                    if Instance::branch(&mut pc, stack, control, depth) {
                        call_frames.pop();
                        return Ok(());
                    }
                    call_frames.pop();
                    current_base = call_frames.last().unwrap().stack_base;
                }
                // Call instructions
                CALL => {
                    // direct calls are fully type-checked at validation time; no
                    // structural type check is required here. we only use the
                    // runtime signature for fast param/result counts to set up frames.
                    let fi: u32 = read_leb128(bytes, &mut pc)?;
                    let f = &self.functions[fi as usize];

                    match f {
                        RuntimeFunction::OwnedWasm { runtime_sig, pc_start, locals_count } => {
                            pc = Self::setup_wasm_function_call(*runtime_sig, *pc_start, *locals_count, stack, control, call_frames, pc)?;
                            current_base = call_frames.last().unwrap().stack_base;
                        }
                        RuntimeFunction::ImportedWasm { owner, function_index, runtime_sig } => {
                            let owner_rc = owner.upgrade().ok_or(Error::trap(FUNC_NO_IMPL))?;
                            Self::call_remote(&owner_rc, *function_index, runtime_sig.n_params() as usize, stack)?;
                        }
                        RuntimeFunction::Host { callback, runtime_sig } => {
                            Self::call_host(callback.as_ref(), *runtime_sig, stack);
                        }
                    }
                }
                CALL_INDIRECT => {
                    // Indirect calls must enforce params at runtime
                    // Here we must parse the indices
                    let type_idx: u32 = read_leb128(bytes, &mut pc)?;
                    pc += 1; // Skip the zero flag
                    let elem_idx = pop_val!().as_u32();
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
                        let mut result: Option<Result<(), Error>> = None;
                        let mut sig_ok = false;
                        InstanceManager::with(|mgr| {
                            if let Some(owner) = mgr.get_instance(owner_id) {
                                let callee = &owner.functions[func_idx];
                                sig_ok = callee.signature() == expected;
                                if sig_ok {
                                    result = Some(Self::call_remote(&owner, func_idx, callee.param_count(), stack));
                                }
                            }
                        });
                        if !sig_ok {
                            return Err(Error::trap(INDIRECT_CALL_MISMATCH));
                        }
                        match result {
                            Some(Ok(())) => continue,
                            Some(Err(e)) => return Err(e),
                            None => return Err(Error::trap(FUNC_NO_IMPL)),
                        }
                    }

                    let callee = &self.functions[func_idx];
                    if callee.signature() != expected {
                        return Err(Error::trap(INDIRECT_CALL_MISMATCH));
                    }

                    match callee {
                        RuntimeFunction::ImportedWasm { runtime_sig, owner, function_index } => {
                            let owner_rc = owner.upgrade().ok_or(Error::trap(FUNC_NO_IMPL))?;
                            Self::call_remote(&owner_rc, *function_index, runtime_sig.n_params() as usize, stack)?;
                        }
                        RuntimeFunction::OwnedWasm { runtime_sig, pc_start, locals_count } => {
                            pc = Self::setup_wasm_function_call(*runtime_sig, *pc_start, *locals_count, stack, control, call_frames, pc)?;
                            current_base = call_frames.last().unwrap().stack_base;
                        }
                        RuntimeFunction::Host { callback, runtime_sig } => {
                            Self::call_host(callback.as_ref(), *runtime_sig, stack);
                        }
                    }
                }
                DROP => {
                    pop_val!();
                }
                SELECT => {
                    let cond = pop_val!().as_u32();
                    let v2 = pop_val!();
                    let v1 = pop_val!();
                    stack.push(if cond != 0 { v1 } else { v2 });
                }
                LOCAL_GET => {
                    let local: u32 = read_leb128(bytes, &mut pc)?;
                    let i = current_base + local as usize;
                    stack.push(stack[i]);
                }
                LOCAL_SET => {
                    let local: u32 = read_leb128(bytes, &mut pc)?;
                    let val = pop_val!();
                    let i = current_base + local as usize;
                    stack[i] = val;
                }
                LOCAL_TEE => {
                    let local: u32 = read_leb128(bytes, &mut pc)?;
                    let val = match stack.last() {
                        Some(v) => *v,
                        None => return Err(Error::trap(STACK_UNDERFLOW))
                    };
                    let i = current_base + local as usize;
                    stack[i] = val;
                }
                GLOBAL_GET => {
                    let gi: u32 = read_leb128(bytes, &mut pc)?;
                    if gi as usize >= self.globals.len() {
                        return Err(Error::trap(UNKNOWN_GLOBAL));
                    }
                    stack.push(self.globals[gi as usize].value.get());
                }
                GLOBAL_SET => {
                    let gi: u32 = read_leb128(bytes, &mut pc)?;
                    let val = pop_val!();
                    self.globals[gi as usize].value.set(val);
                }
                I32_LOAD => { load!(load_u32, |v: u32| WasmValue::from_u32(v)); }
                I64_LOAD => { load!(load_u64, |v: u64| WasmValue::from_u64(v)); }
                F32_LOAD => { load!(load_f32, |v: f32| WasmValue::from_f32(v)); }
                F64_LOAD => { load!(load_f64, |v: f64| WasmValue::from_f64(v)); }
                I32_LOAD8_S => { load!(load_i8,  |v: i8| WasmValue::from_i32(v as i32)); }
                I32_LOAD8_U => { load!(load_u8,  |v: u8| WasmValue::from_u32(v as u32)); }
                I32_LOAD16_S => { load!(load_i16, |v: i16| WasmValue::from_i32(v as i32)); }
                I32_LOAD16_U => { load!(load_u16, |v: u16| WasmValue::from_u32(v as u32)); }
                I64_LOAD8_S => { load!(load_i8,  |v: i8| WasmValue::from_i64(v as i64)); }
                I64_LOAD8_U => { load!(load_u8,  |v: u8| WasmValue::from_u64(v as u64)); }
                I64_LOAD16_S => { load!(load_i16, |v: i16| WasmValue::from_i64(v as i64)); }
                I64_LOAD16_U => { load!(load_u16, |v: u16| WasmValue::from_u64(v as u64)); }
                I64_LOAD32_S => { load!(load_i32, |v: i32| WasmValue::from_i64(v as i64)); }
                I64_LOAD32_U => { load!(load_u32, |v: u32| WasmValue::from_u64(v as u64)); }
                I32_STORE => { store!(store_u32, |w: WasmValue| w.as_u32()); }
                I64_STORE => { store!(store_u64, |w: WasmValue| w.as_u64()); }
                F32_STORE => { store!(store_f32, |w: WasmValue| w.as_f32()); }
                F64_STORE => { store!(store_f64, |w: WasmValue| w.as_f64()); }
                I32_STORE8 => { store!(store_u8,  |w: WasmValue| (w.as_u32() & 0xFF) as u8); }
                I32_STORE16 => { store!(store_u16, |w: WasmValue| (w.as_u32() & 0xFFFF) as u16); }
                I64_STORE8 => { store!(store_u8,  |w: WasmValue| (w.as_u64() & 0xFF) as u8); }
                I64_STORE16 => { store!(store_u16, |w: WasmValue| (w.as_u64() & 0xFFFF) as u16); }
                I64_STORE32 => { store!(store_u32, |w: WasmValue| (w.as_u64() & 0xFFFF_FFFF) as u32); }
                MEMORY_SIZE => {
                    pc += 1; // Skip zero flag
                    let mem = mem.ok_or(Error::validation(UNKNOWN_MEMORY))?;
                    stack.push(WasmValue::from_u32(mem.borrow().size()));
                }
                MEMORY_GROW => {
                    pc += 1; // Skip zero flag
                    let delta = pop_val!().as_u32();
                    let mem = mem.ok_or(Error::validation(UNKNOWN_MEMORY))?;
                    let old = mem.borrow_mut().grow(delta);
                    stack.push(WasmValue::from_u32(old));
                }
                I32_CONST => {
                    stack.push(WasmValue::from_i32(read_sleb128::<i32>(bytes, &mut pc)?));
                }
                I64_CONST => {
                    stack.push(WasmValue::from_i64(read_sleb128::<i64>(bytes, &mut pc)?));
                }
                F32_CONST => {
                    let bits = unsafe { (bytes.as_ptr().add(pc) as *const u32).read_unaligned() };
                    stack.push(WasmValue::from_f32_bits(u32::from_le(bits)));
                    pc += 4;
                }
                F64_CONST => {
                    let bits = unsafe { (bytes.as_ptr().add(pc) as *const u64).read_unaligned() };
                    stack.push(WasmValue::from_f64_bits(u64::from_le(bits)));
                    pc += 8;
                }
                I32_EQZ => { unary!(u32, |x: u32| (x == 0) as u32); }
                I32_EQ => { compare!(u32, ==); }
                I32_NE => { compare!(u32, !=); }
                I32_LT_S => { compare!(i32, <); }
                I32_LT_U => { compare!(u32, <); }
                I32_GT_S => { compare!(i32, >); }
                I32_GT_U => { compare!(u32, >); }
                I32_LE_S => { compare!(i32, <=); }
                I32_LE_U => { compare!(u32, <=); }
                I32_GE_S => { compare!(i32, >=); }
                I32_GE_U => { compare!(u32, >=); }
                I64_EQZ => {
                    let v = peek_one!(u64);
                    overwrite!(WasmValue::from_u32((v == 0) as u32));
                }
                I64_EQ => { compare!(i64, ==); }
                I64_NE => { compare!(i64, !=); }
                I64_LT_S => { compare!(i64, <); }
                I64_LT_U => { compare!(u64, <); }
                I64_GT_S => { compare!(i64, >); }
                I64_GT_U => { compare!(u64, >); }
                I64_LE_S => { compare!(i64, <=); }
                I64_LE_U => { compare!(u64, <=); }
                I64_GE_S => { compare!(i64, >=); }
                I64_GE_U => { compare!(u64, >=); }
                F32_EQ => { compare!(f32, ==); }
                F32_NE => { compare!(f32, !=); }
                F32_LT => { compare!(f32, <); }
                F32_GT => { compare!(f32, >); }
                F32_LE => { compare!(f32, <=); }
                F32_GE => { compare!(f32, >=); }
                F64_EQ => { compare!(f64, ==); }
                F64_NE => { compare!(f64, !=); }
                F64_LT => { compare!(f64, <); }
                F64_GT => { compare!(f64, >); }
                F64_LE => { compare!(f64, <=); }
                F64_GE => { compare!(f64, >=); }
                I32_CLZ => { unary!(u32, |x: u32| x.leading_zeros()); }
                I32_CTZ => { unary!(u32, |x: u32| x.trailing_zeros()); }
                I32_POPCNT => { unary!(u32, |x: u32| x.count_ones()); }
                I32_ADD => { binary!(u32, .wrapping_add); }
                I32_SUB => { binary!(u32, .wrapping_sub); }
                I32_MUL => { binary!(u32, .wrapping_mul); }
                I32_DIV_S => { div_s!(i32); }
                I32_DIV_U => { div_u!(u32); }
                I32_REM_S => { rem_s!(i32); }
                I32_REM_U => { rem_u!(u32); }
                I32_AND => { binary!(u32, &); }
                I32_OR => { binary!(u32, |); }
                I32_XOR => { binary!(u32, ^); }
                I32_SHL => { shift!(u32, <<); }
                I32_SHR_S => { shr_s!(i32, u32, 32); }
                I32_SHR_U => { shift!(u32, >>); }
                I32_ROTL => { rotate!(u32, left); }
                I32_ROTR => { rotate!(u32, right); }
                I64_CLZ => { unary!(u64, |x: u64| x.leading_zeros() as u64); }
                I64_CTZ => { unary!(u64, |x: u64| x.trailing_zeros() as u64); }
                I64_POPCNT => { unary!(u64, |x: u64| x.count_ones() as u64); }
                I64_ADD => { binary!(u64, .wrapping_add); }
                I64_SUB => { binary!(u64, .wrapping_sub); }
                I64_MUL => { binary!(u64, .wrapping_mul); }
                I64_DIV_S => { div_s!(i64); }
                I64_DIV_U => { div_u!(u64); }
                I64_REM_S => { rem_s!(i64); }
                I64_REM_U => { rem_u!(u64); }
                I64_AND => { binary!(u64, &); }
                I64_OR => { binary!(u64, |); }
                I64_XOR => { binary!(u64, ^); }
                I64_SHL => { shift!(u64, <<); }
                I64_SHR_S => { shr_s!(i64, u64, 64); }
                I64_SHR_U => { shift!(u64, >>); }
                I64_ROTL => { rotate!(u64, left); }
                I64_ROTR => { rotate!(u64, right); }
                F32_ABS => { unary!(f32, |x: f32| x.abs()); }
                F32_NEG => { unary!(f32, |x: f32| -x); }
                F32_CEIL => { unary!(f32, |x: f32| x.ceil()); }
                F32_FLOOR => { unary!(f32, |x: f32| x.floor()); }
                F32_TRUNC => { unary!(f32, |x: f32| x.trunc()); }
                F32_NEAREST => { nearest!(f32); }
                F32_SQRT => { unary!(f32, |x: f32| x.sqrt()); }
                F32_ADD => { binary!(f32, +); }
                F32_SUB => { binary!(f32, -); }
                F32_MUL => { binary!(f32, *); }
                F32_DIV => { binary!(f32, /); }
                F32_MIN => { minmax!(f32, min); }
                F32_MAX => { minmax!(f32, max); }
                F32_COPYSIGN => { copysign!(f32); }
                F64_ABS => { unary!(f64, |x: f64| x.abs()); }
                F64_NEG => { unary!(f64, |x: f64| -x); }
                F64_CEIL => { unary!(f64, |x: f64| x.ceil()); }
                F64_FLOOR => { unary!(f64, |x: f64| x.floor()); }
                F64_TRUNC => { unary!(f64, |x: f64| x.trunc()); }
                F64_NEAREST => { nearest!(f64); }
                F64_SQRT => { unary!(f64, |x: f64| x.sqrt()); }
                F64_ADD => { binary!(f64, +); }
                F64_SUB => { binary!(f64, -); }
                F64_MUL => { binary!(f64, *); }
                F64_DIV => { binary!(f64, /); }
                F64_MIN => { minmax!(f64, min); }
                F64_MAX => { minmax!(f64, max); }
                F64_COPYSIGN => { copysign!(f64); }
                I32_WRAP_I64 => { convert!(u64 -> u32); }
                I32_TRUNC_F32_S => { trunc!(f32 -> i32 : -2147483777.0, 2147483648.0); }
                I32_TRUNC_F32_U => { trunc!(f32 -> u32 : -1.0, 4294967296.0); }
                I32_TRUNC_F64_S => { trunc!(f64 -> i32 : -2147483649.0, 2147483648.0); }
                I32_TRUNC_F64_U => { trunc!(f64 -> u32 : -1.0, 4294967296.0); }
                I64_EXTEND_I32_S => { convert!(i32 -> i64); }
                I64_EXTEND_I32_U => { convert!(u32 -> u64); }
                I64_TRUNC_F32_S => { trunc!(f32 -> i64 : -9223373136366404000.0, 9223372036854776000.0); }
                I64_TRUNC_F32_U => { trunc!(f32 -> u64 : -1.0, 18446744073709552000.0); }
                I64_TRUNC_F64_S => { trunc!(f64 -> i64 : -9223372036854777856.0, 9223372036854776000.0); }
                I64_TRUNC_F64_U => { trunc!(f64 -> u64 : -1.0, 18446744073709552000.0); }
                F32_CONVERT_I32_S => { convert!(i32 -> f32); }
                F32_CONVERT_I32_U => { convert!(u32 -> f32); }
                F32_CONVERT_I64_S => { convert!(i64 -> f32); }
                F32_CONVERT_I64_U => { convert!(u64 -> f32); }
                F32_DEMOTE_F64 => { convert!(f64 -> f32); }
                F64_CONVERT_I32_S => { convert!(i32 -> f64); }
                F64_CONVERT_I32_U => { convert!(u32 -> f64); }
                F64_CONVERT_I64_S => { convert!(i64 -> f64); }
                F64_CONVERT_I64_U => { convert!(u64 -> f64); }
                F64_PROMOTE_F32 => { convert!(f32 -> f64); }
                _ => {
                    return Err(Error::malformed(UNKNOWN_INSTRUCTION));
                }
            }
        }
    }

    #[inline(always)]
    fn branch(
        pc: &mut usize,
        stack: &mut Vec<WasmValue>,
        control: &mut Vec<ControlFrame>,
        depth: u32,
    ) -> bool {
        let len = control.len();
        if depth as usize >= len {
            return true;
        }
        let keep = len - depth as usize;
        control.truncate(keep);
        let Some(target) = control.pop() else {
            return true;
        };
        let result_arity = target.arity as usize;
        let sl = target.stack_len as usize;

        if result_arity > 0 {
            let stack_len = stack.len();
            let src_start = stack_len.saturating_sub(result_arity);

            if src_start > sl {
                stack.copy_within(src_start..stack_len, sl);
            }
            stack.truncate(sl + result_arity);
        } else {
            stack.truncate(sl);
        }

        *pc = target.dest_pc as usize;
        control.is_empty()
    }

    pub fn invoke(
        &self,
        func: &RuntimeFunction,
        args: &[WasmValue],
    ) -> Result<Vec<WasmValue>, Error> {
        let n_params = func.param_count();
        if n_params != args.len() {
            return Err(Error::trap(INVALID_NUM_ARG));
        }

        let mut stack: Vec<WasmValue> = Vec::with_capacity(1024);
        stack.extend_from_slice(args);
        let mut control: Vec<ControlFrame> = Vec::with_capacity(64);
        let mut call_frames: Vec<CallFrame> = Vec::with_capacity(16);
        let return_pc: usize = 0;

        match func {
            RuntimeFunction::OwnedWasm { runtime_sig, pc_start, locals_count } => {
                let pc = Self::setup_wasm_function_call(
                    *runtime_sig,
                    *pc_start,
                    *locals_count,
                    &mut stack,
                    &mut control,
                    &mut call_frames,
                    return_pc,
                )?;
                self.interpret(pc, &mut stack, &mut control, &mut call_frames)?;
            }
            RuntimeFunction::ImportedWasm { owner, function_index, .. } => {
                if let Some(owner_rc) = owner.upgrade() {
                    let mut owned_stack: Vec<WasmValue> = Vec::with_capacity(64);
                    owned_stack.extend_from_slice(args);
                    let mut control: Vec<ControlFrame> = Vec::with_capacity(16);
                    let mut return_pc: usize = 0;
                    let mut call_frames: Vec<CallFrame> = Vec::with_capacity(8);
                    owner_rc.call_function_idx(
                        *function_index,
                        &mut return_pc,
                        &mut owned_stack,
                        &mut control,
                        &mut call_frames,
                    )?;
                    return Ok(owned_stack);
                } else {
                    return Err(Error::trap(FUNC_NO_IMPL));
                }
            }
            RuntimeFunction::Host { callback, runtime_sig, .. } => {
                Self::call_host(callback.as_ref(), *runtime_sig, &mut stack);
            }
        }
        Ok(stack)
    }
}
