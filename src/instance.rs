use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use crate::error::*;
use crate::Module;
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

pub struct Instance {
    pub id: u32,
    pub module: Rc<Module>,
    pub memory: Option<Rc<RefCell<WasmMemory>>>,
    pub tables: Vec<Rc<RefCell<WasmTable>>>,
    pub globals: Vec<Rc<RefCell<WasmGlobal>>>,
    pub functions: Vec<FunctionInfo>,
    pub exports: Exports,
}