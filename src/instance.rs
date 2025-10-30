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
    #[inline (always)] pub fn from_i32(v: i32) -> Self { Self(v as u32 as u64) }
    #[inline (always)] pub fn as_i32(self) -> i32 { self.0 as u32 as i32 }
    #[inline (always)] pub fn from_u32(v: u32) -> Self { Self(v as u64) }
    #[inline (always)] pub fn as_u32(self) -> u32 { self.0 as u32 }
    #[inline (always)] pub fn from_i64(v: i64) -> Self { Self(v as u64) }
    #[inline (always)] pub fn as_i64(self) -> i64 { self.0 as i64 }
    #[inline (always)] pub fn from_u64(v: u64) -> Self { Self(v) }
    #[inline (always)] pub fn as_u64(self) -> u64 { self.0 }
    #[inline (always)] pub fn from_f32_bits(bits: u32) -> Self { Self(bits as u64) }
    #[inline (always)] pub fn as_f32_bits(self) -> u32 { self.0 as u32 }
    #[inline (always)] pub fn from_f64_bits(bits: u64) -> Self { Self(bits) }
    #[inline (always)] pub fn as_f64_bits(self) -> u64 { self.0 }
    #[inline (always)] pub fn from_f32(v: f32) -> Self { Self::from_f32_bits(v.to_bits()) }
    #[inline (always)] pub fn as_f32(self) -> f32 { f32::from_bits(self.as_f32_bits()) }
    #[inline (always)] pub fn from_f64(v: f64) -> Self { Self::from_f64_bits(v.to_bits()) }
    #[inline (always)] pub fn as_f64(self) -> f64 { f64::from_bits(self.as_f64_bits()) }
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
    elements: Vec<FuncRef>,
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
    #[inline(always)]
    pub fn get(&self, idx: u32) -> Result<WasmValue, &'static str> {
        let i = idx as usize;
        if i >= self.elements.len() { return Err(OOB_TABLE_ACCESS); }
        Ok(WasmValue::from_u64(self.elements[i].as_raw()))
    }
    #[inline(always)]
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

type Handler = fn(
    &Instance,
    usize,
    &mut Vec<WasmValue>,
    &mut Vec<ControlFrame>,
    &mut Vec<usize>,
    &mut Vec<usize>,
) -> Result<(), Error>;

#[inline(always)]
fn pop_value(stack: &mut Vec<WasmValue>) -> Result<WasmValue, Error> {
    stack.pop().ok_or_else(|| Error::trap(STACK_UNDERFLOW))
}

#[inline(always)]
fn peek_value(stack: &mut Vec<WasmValue>) -> Result<WasmValue, Error> {
    stack.last().copied().ok_or_else(|| Error::trap(STACK_UNDERFLOW))
}

macro_rules! handler_fn {
    ($name:ident, |$instance:ident, $pc:ident, $stack:ident, $control:ident, $func_bases:ident, $ctrl_bases:ident| $body:block $(;)?) => {
        #[inline(never)]
        #[allow(unused_mut, unused_variables)]
        fn $name(
            $instance: &Instance,
            mut $pc: usize,
            $stack: &mut Vec<WasmValue>,
            $control: &mut Vec<ControlFrame>,
            $func_bases: &mut Vec<usize>,
            $ctrl_bases: &mut Vec<usize>,
        ) -> Result<(), Error> $body
    };
}

macro_rules! next_op {
    ($instance:ident, $pc:ident, $stack:ident, $control:ident, $func_bases:ident, $ctrl_bases:ident) => {{
        let bytes = &$instance.module.bytes;
        let opcode = *bytes.get($pc).ok_or(Error::malformed(UNEXPECTED_END))?;
        let next_pc = $pc + 1;
        become HANDLERS[opcode as usize]( // now it works!
            $instance,
            next_pc,
            $stack,
            $control,
            $func_bases,
            $ctrl_bases,
        )
    }};
}

macro_rules! binary_handler {
    ($name:ident, $ty:ident, method $method:ident) => {
        handler_fn!($name, |instance, pc, stack, control, func_bases, ctrl_bases| {
            paste! {
                let rhs = pop_value(stack)?.[<as_ $ty>]();
                let lhs = pop_value(stack)?.[<as_ $ty>]();
                let result = lhs.$method(rhs);
                stack.push(WasmValue::[<from_ $ty>](result));
            }
            next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
        });
    };
    ($name:ident, $ty:ident, op $op:tt) => {
        handler_fn!($name, |instance, pc, stack, control, func_bases, ctrl_bases| {
            paste! {
                let rhs = pop_value(stack)?.[<as_ $ty>]();
                let lhs = pop_value(stack)?.[<as_ $ty>]();
                let result = lhs $op rhs;
                stack.push(WasmValue::[<from_ $ty>](result));
            }
            next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
        });
    };
}

macro_rules! compare_handler {
    ($name:ident, $ty:ident, $op:tt) => {
        handler_fn!($name, |instance, pc, stack, control, func_bases, ctrl_bases| {
            paste! {
                let rhs = pop_value(stack)?.[<as_ $ty>]();
                let lhs = pop_value(stack)?.[<as_ $ty>]();
                let result = (lhs $op rhs) as u32;
                stack.push(WasmValue::from_u32(result));
            }
            next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
        });
    };
}

macro_rules! unary_handler {
    ($name:ident, $ty:ident, $func:expr) => {
        handler_fn!($name, |instance, pc, stack, control, func_bases, ctrl_bases| {
            paste! {
                let value = pop_value(stack)?.[<as_ $ty>]();
                let result = ($func)(value);
                stack.push(WasmValue::[<from_ $ty>](result));
            }
            next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
        });
    };
}

macro_rules! shift_handler {
    ($name:ident, u32, $op:tt) => {
        handler_fn!($name, |instance, pc, stack, control, func_bases, ctrl_bases| {
            let rhs = pop_value(stack)?.as_u32() % 32;
            let lhs = pop_value(stack)?.as_u32();
            stack.push(WasmValue::from_u32(lhs $op rhs));
            next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
        });
    };
    ($name:ident, u64, $op:tt) => {
        handler_fn!($name, |instance, pc, stack, control, func_bases, ctrl_bases| {
            let rhs = pop_value(stack)?.as_u64() % 64;
            let lhs = pop_value(stack)?.as_u64();
            stack.push(WasmValue::from_u64(lhs $op rhs));
            next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
        });
    };
}

macro_rules! rotate_handler {
    ($name:ident, u32, left) => {
        handler_fn!($name, |instance, pc, stack, control, func_bases, ctrl_bases| {
            let rhs = pop_value(stack)?.as_u32();
            let lhs = pop_value(stack)?.as_u32();
            stack.push(WasmValue::from_u32(lhs.rotate_left(rhs % 32)));
            next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
        });
    };
    ($name:ident, u32, right) => {
        handler_fn!($name, |instance, pc, stack, control, func_bases, ctrl_bases| {
            let rhs = pop_value(stack)?.as_u32();
            let lhs = pop_value(stack)?.as_u32();
            stack.push(WasmValue::from_u32(lhs.rotate_right(rhs % 32)));
            next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
        });
    };
    ($name:ident, u64, left) => {
        handler_fn!($name, |instance, pc, stack, control, func_bases, ctrl_bases| {
            let rhs = pop_value(stack)?.as_u64();
            let lhs = pop_value(stack)?.as_u64();
            stack.push(WasmValue::from_u64(lhs.rotate_left((rhs % 64) as u32)));
            next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
        });
    };
    ($name:ident, u64, right) => {
        handler_fn!($name, |instance, pc, stack, control, func_bases, ctrl_bases| {
            let rhs = pop_value(stack)?.as_u64();
            let lhs = pop_value(stack)?.as_u64();
            stack.push(WasmValue::from_u64(lhs.rotate_right((rhs % 64) as u32)));
            next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
        });
    };
}

macro_rules! minmax_handler {
    ($name:ident, $ty:ident, min) => { minmax_handler!(@impl $name, $ty, min, true); };
    ($name:ident, $ty:ident, max) => { minmax_handler!(@impl $name, $ty, max, false); };
    (@impl $name:ident, $ty:ident, $method:ident, $prefer_negative:literal) => {
        handler_fn!($name, |instance, pc, stack, control, func_bases, ctrl_bases| {
            paste! {
                let rhs = pop_value(stack)?.[<as_ $ty>]();
                let lhs = pop_value(stack)?.[<as_ $ty>]();
                let result = if lhs.is_nan() {
                    lhs
                } else if rhs.is_nan() {
                    rhs
                } else if lhs == rhs && lhs == 0.0 {
                    const SIGN_BIT_SHIFT: usize = core::mem::size_of::<$ty>() * 8 - 1;
                    let lhs_bits = lhs.to_bits();
                    let rhs_bits = rhs.to_bits();
                    let lhs_sign = (lhs_bits >> SIGN_BIT_SHIFT) & 1;
                    let rhs_sign = (rhs_bits >> SIGN_BIT_SHIFT) & 1;
                    if lhs_sign == rhs_sign {
                        lhs
                    } else if $prefer_negative {
                        if lhs_sign == 1 { lhs } else { rhs }
                    } else {
                        if lhs_sign == 0 { lhs } else { rhs }
                    }
                } else {
                    lhs.$method(rhs)
                };
                stack.push(WasmValue::[<from_ $ty>](result));
            }
            next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
        });
    };
}

macro_rules! shr_s_handler {
    ($name:ident, $int_ty:ident, $uint_ty:ident, $bits:expr) => {
        handler_fn!($name, |instance, pc, stack, control, func_bases, ctrl_bases| {
            paste! {
                let shift = pop_value(stack)?.[<as_ $uint_ty>]() % $bits;
                let value = pop_value(stack)?.[<as_ $int_ty>]();
                stack.push(WasmValue::[<from_ $int_ty>](value >> shift));
            }
            next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
        });
    };
}

macro_rules! copysign_handler {
    ($name:ident, $ty:ident) => {
        handler_fn!($name, |instance, pc, stack, control, func_bases, ctrl_bases| {
            paste! {
                let rhs = pop_value(stack)?.[<as_ $ty>]();
                let lhs = pop_value(stack)?.[<as_ $ty>]();
                let result = lhs.copysign(rhs);
                stack.push(WasmValue::[<from_ $ty>](result));
            }
            next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
        });
    };
}

macro_rules! nearest_handler {
    ($name:ident, $ty:ident) => {
        handler_fn!($name, |instance, pc, stack, control, func_bases, ctrl_bases| {
            paste! {
                let value = pop_value(stack)?.[<as_ $ty>]();
                let result = if value.is_nan() || value.is_infinite() {
                    value
                } else {
                    let lower = value.floor();
                    let upper = value.ceil();
                    let dl = value - lower;
                    let du = upper - value;
                    if dl < du {
                        lower
                    } else if dl > du {
                        upper
                    } else {
                        if (lower % 2.0) == 0.0 { lower } else { upper }
                    }
                };
                stack.push(WasmValue::[<from_ $ty>](result));
            }
            next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
        });
    };
}

macro_rules! convert_handler {
    ($name:ident, $src:ident, $dst:ident) => {
        handler_fn!($name, |instance, pc, stack, control, func_bases, ctrl_bases| {
            paste! {
                let value = pop_value(stack)?.[<as_ $src>]();
                stack.push(WasmValue::[<from_ $dst>](value as $dst));
            }
            next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
        });
    };
}

macro_rules! trunc_handler {
    ($name:ident, $src:ident -> $dst:ident : $min:expr, $max:expr) => {
        handler_fn!($name, |instance, pc, stack, control, func_bases, ctrl_bases| {
            paste! {
                let value = pop_value(stack)?.[<as_ $src>]();
                if !value.is_finite() {
                    if value.is_nan() {
                        return Err(Error::trap(INVALID_CONV_TO_INT));
                    } else {
                        return Err(Error::trap(INTEGER_OVERFLOW));
                    }
                }
                if value <= $min || value >= $max {
                    return Err(Error::trap(INTEGER_OVERFLOW));
                }
                stack.push(WasmValue::[<from_ $dst>](value as $dst));
            }
            next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
        });
    };
}

macro_rules! div_s_handler {
    ($name:ident, $ty:ident) => {
        handler_fn!($name, |instance, pc, stack, control, func_bases, ctrl_bases| {
            paste! {
                let rhs = pop_value(stack)?.[<as_ $ty>]();
                let lhs = pop_value(stack)?.[<as_ $ty>]();
                if rhs == 0 {
                    return Err(Error::trap(DIVIDE_BY_ZERO));
                }
                if lhs == <$ty>::MIN && rhs == -1 {
                    return Err(Error::trap(INTEGER_OVERFLOW));
                }
                stack.push(WasmValue::[<from_ $ty>](lhs / rhs));
            }
            next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
        });
    };
}

macro_rules! div_u_handler {
    ($name:ident, $ty:ident) => {
        handler_fn!($name, |instance, pc, stack, control, func_bases, ctrl_bases| {
            paste! {
                let rhs = pop_value(stack)?.[<as_ $ty>]();
                let lhs = pop_value(stack)?.[<as_ $ty>]();
                if rhs == 0 {
                    return Err(Error::trap(DIVIDE_BY_ZERO));
                }
                stack.push(WasmValue::[<from_ $ty>](lhs / rhs));
            }
            next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
        });
    };
}

macro_rules! rem_s_handler {
    ($name:ident, $ty:ident) => {
        handler_fn!($name, |instance, pc, stack, control, func_bases, ctrl_bases| {
            paste! {
                let rhs = pop_value(stack)?.[<as_ $ty>]();
                let lhs = pop_value(stack)?.[<as_ $ty>]();
                if rhs == 0 {
                    return Err(Error::trap(DIVIDE_BY_ZERO));
                }
                if lhs == <$ty>::MIN && rhs == -1 {
                    stack.push(WasmValue::[<from_ $ty>](0 as $ty));
                } else {
                    stack.push(WasmValue::[<from_ $ty>](lhs % rhs));
                }
            }
            next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
        });
    };
}

macro_rules! rem_u_handler {
    ($name:ident, $ty:ident) => {
        handler_fn!($name, |instance, pc, stack, control, func_bases, ctrl_bases| {
            paste! {
                let rhs = pop_value(stack)?.[<as_ $ty>]();
                let lhs = pop_value(stack)?.[<as_ $ty>]();
                if rhs == 0 {
                    return Err(Error::trap(DIVIDE_BY_ZERO));
                }
                stack.push(WasmValue::[<from_ $ty>](lhs % rhs));
            }
            next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
        });
    };
}

macro_rules! load_handler {
    ($name:ident, $method:ident, $convert:expr) => {
        handler_fn!($name, |instance, pc, stack, control, func_bases, ctrl_bases| {
            let bytes = &instance.module.bytes;
            let _alignment: u32 = read_leb128(bytes, &mut pc)?;
            let offset: u32 = read_leb128(bytes, &mut pc)?;
            let addr = pop_value(stack)?.as_u32();
            let memory = instance.memory.as_ref().ok_or_else(|| Error::validation(UNKNOWN_MEMORY))?;
            let value = memory.borrow().$method(addr, offset).map_err(Error::trap)?;
            stack.push(($convert)(value));
            next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
        });
    };
}

macro_rules! store_handler {
    ($name:ident, $method:ident, $extract:expr) => {
        handler_fn!($name, |instance, pc, stack, control, func_bases, ctrl_bases| {
            let bytes = &instance.module.bytes;
            let _alignment: u32 = read_leb128(bytes, &mut pc)?;
            let offset: u32 = read_leb128(bytes, &mut pc)?;
            let raw = pop_value(stack)?;
            let addr = pop_value(stack)?.as_u32();
            let value = ($extract)(raw);
            let memory = instance.memory.as_ref().ok_or_else(|| Error::validation(UNKNOWN_MEMORY))?;
            memory.borrow_mut().$method(addr, offset, value).map_err(Error::trap)?;
            next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
        });
    };
}

handler_fn!(op_unreachable, |_instance, _pc, _stack, _control, _func_bases, _ctrl_bases| {
    Err(Error::trap(UNREACHABLE))
});

handler_fn!(op_nop, |instance, pc, stack, control, func_bases, ctrl_bases| {
    next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
});

handler_fn!(op_block, |instance, pc, stack, control, func_bases, ctrl_bases| {
    let (mut body_pc, end_pc, _else_pc, params_len, has_result) =
        instance.module.side_table.lookup(pc).unwrap();
    control.push(ControlFrame {
        stack_len: stack.len() - params_len as usize,
        dest_pc: end_pc,
        arity: has_result as u32,
        has_result,
    });
    next_op!(instance, body_pc, stack, control, func_bases, ctrl_bases)
});

handler_fn!(op_loop, |instance, pc, stack, control, func_bases, ctrl_bases| {
    let loop_op_pc = pc.saturating_sub(1);
    let (mut body_pc, _end_pc, _else_pc, params_len, has_result) =
        instance.module.side_table.lookup(pc).unwrap();
    control.push(ControlFrame {
        stack_len: stack.len() - params_len as usize,
        dest_pc: loop_op_pc,
        arity: params_len as u32,
        has_result,
    });
    next_op!(instance, body_pc, stack, control, func_bases, ctrl_bases)
});

handler_fn!(op_if, |instance, pc, stack, control, func_bases, ctrl_bases| {
    let (body_pc, end_pc, else_pc, params_len, has_result) =
        instance.module.side_table.lookup(pc).unwrap();
    let cond = pop_value(stack)?.as_u32();
    control.push(ControlFrame {
        stack_len: stack.len() - params_len as usize,
        dest_pc: end_pc,
        arity: has_result as u32,
        has_result,
    });
    let next = if cond == 0 { else_pc } else { body_pc };
    next_op!(instance, next, stack, control, func_bases, ctrl_bases)
});

handler_fn!(op_else, |instance, pc, stack, control, func_bases, ctrl_bases| {
    let mut next_pc = pc;
    if Instance::branch(&mut next_pc, stack, control, 0) {
        return Ok(());
    }
    next_op!(instance, next_pc, stack, control, func_bases, ctrl_bases)
});

handler_fn!(op_end, |instance, pc, stack, control, func_bases, ctrl_bases| {
    if let Some(&frame_idx) = ctrl_bases.last() {
        if frame_idx == control.len().saturating_sub(1) {
            let mut pc_mut = pc;
            if Instance::branch(&mut pc_mut, stack, control, 0) {
                ctrl_bases.pop();
                let _ = func_bases.pop();
                return Ok(());
            }
            ctrl_bases.pop();
            let _ = func_bases.pop();
            next_op!(instance, pc_mut, stack, control, func_bases, ctrl_bases)
        }
    }

    if let Some(target) = control.pop() {
        if target.has_result {
            let result = stack[stack.len() - 1];
            stack.truncate(target.stack_len);
            stack.push(result);
        } else {
            stack.truncate(target.stack_len);
        }
    } else {
        return Ok(());
    }

    next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
});

handler_fn!(op_br, |instance, pc, stack, control, func_bases, ctrl_bases| {
    let bytes = &instance.module.bytes;
    let mut pc = pc;
    let depth: u32 = read_leb128(bytes, &mut pc)?;
    if Instance::branch(&mut pc, stack, control, depth) {
        return Ok(());
    }
    next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
});

handler_fn!(op_br_if, |instance, pc, stack, control, func_bases, ctrl_bases| {
    let bytes = &instance.module.bytes;
    let mut pc = pc;
    let depth: u32 = read_leb128(bytes, &mut pc)?;
    let cond = pop_value(stack)?.as_u32();
    if cond != 0 && Instance::branch(&mut pc, stack, control, depth) {
        return Ok(());
    }
    next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
});

handler_fn!(op_br_table, |instance, pc, stack, control, func_bases, ctrl_bases| {
    let bytes = &instance.module.bytes;
    let mut pc = pc;
    let value = pop_value(stack)?.as_u32();
    let n_targets: u32 = read_leb128(bytes, &mut pc)?;
    let mut depth = u32::MAX;
    for i in 0..n_targets {
        let target: u32 = read_leb128(bytes, &mut pc)?;
        if i == value {
            depth = target;
        }
    }
    let default_target: u32 = read_leb128(bytes, &mut pc)?;
    if depth == u32::MAX {
        depth = default_target;
    }
    if Instance::branch(&mut pc, stack, control, depth) {
        return Ok(());
    }
    next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
});

handler_fn!(op_return, |instance, pc, stack, control, func_bases, ctrl_bases| {
    let mut next_pc = pc;
    if control.is_empty() {
        return Ok(());
    }
    let base_idx = *ctrl_bases.last().unwrap();
    let depth = (control.len() - 1).saturating_sub(base_idx) as u32;
    if Instance::branch(&mut next_pc, stack, control, depth) {
        ctrl_bases.pop();
        let _ = func_bases.pop();
        return Ok(());
    }
    ctrl_bases.pop();
    let _ = func_bases.pop();
    next_op!(instance, next_pc, stack, control, func_bases, ctrl_bases)
});

handler_fn!(op_call, |instance, pc, stack, control, func_bases, ctrl_bases| {
    let bytes = &instance.module.bytes;
    let mut pc = pc;
    let func_index: u32 = read_leb128(bytes, &mut pc)?;
    let func = &instance.functions[func_index as usize];

    match func {
        RuntimeFunction::OwnedWasm { runtime_sig, pc_start, locals_count } => {
            let new_pc = Instance::setup_wasm_function_call(
                *runtime_sig, *pc_start, *locals_count,
                stack, control, func_bases, ctrl_bases, pc,
            )?;
            next_op!(instance, new_pc, stack, control, func_bases, ctrl_bases)
        }
        RuntimeFunction::ImportedWasm { owner, function_index, runtime_sig } => {
            if let Some(owner_rc) = owner.upgrade() {
                let n_params = runtime_sig.n_params() as usize;
                let params_start = stack.len() - n_params;
                let mut tmp_stack: Vec<WasmValue> = Vec::with_capacity(n_params);
                tmp_stack.extend_from_slice(&stack[params_start..(params_start + n_params)]);
                stack.truncate(params_start);
                let mut control_nested: Vec<ControlFrame> = Vec::new();
                let mut ret_pc_nested = 0usize;
                let mut func_bases_nested: Vec<usize> = Vec::new();
                let mut ctrl_bases_nested: Vec<usize> = Vec::new();
                owner_rc.call_function_idx(
                    *function_index, &mut ret_pc_nested, &mut tmp_stack,
                    &mut control_nested, &mut func_bases_nested, &mut ctrl_bases_nested,
                )?;
                for value in tmp_stack { stack.push(value); }
            } else {
                return Err(Error::trap(FUNC_NO_IMPL));
            }
            next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
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
            next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
        }
    }
});

handler_fn!(op_call_indirect, |instance, pc, stack, control, func_bases, ctrl_bases| {
    let bytes = &instance.module.bytes;
    let mut pc = pc;
    let type_idx: u32 = read_leb128(bytes, &mut pc)?;
    pc += 1; // skip zero flag
    let elem_idx = pop_value(stack)?.as_u32();

    let table = instance.table.as_ref().ok_or_else(|| Error::trap(UNDEF_ELEM))?;
    let func_ref = {
        let table_ref = table.borrow();
        if elem_idx >= table_ref.size() {
            return Err(Error::trap(UNDEF_ELEM));
        }
        table_ref.get(elem_idx).map_err(Error::trap)?
    };

    let handle = func_ref.as_u64();
    if handle == 0 { return Err(Error::trap(UNINITIALIZED_ELEM)); }

    let owner_id = (handle >> 32) as u32;
    let low = (handle & 0xFFFF_FFFF) as u32;
    if low == 0 { return Err(Error::trap(FUNC_NO_IMPL)); }
    let func_idx = (low - 1) as usize;
    let expected = RuntimeSignature::from_signature(&instance.module.types[type_idx as usize]);

    if owner_id != instance.id {
        let mut dispatched = false;
        let mut sig_ok = false;
        InstanceManager::with(|mgr| {
            if let Some(owner_instance) = mgr.get_instance(owner_id) {
                let callee = &owner_instance.functions[func_idx];
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
                    let mut ctrl_bases_nested: Vec<usize> = Vec::new();
                    if owner_instance.call_function_idx(
                        func_idx, &mut ret_pc_nested, &mut tmp_stack,
                        &mut control_nested, &mut func_bases_nested, &mut ctrl_bases_nested,
                    ).is_ok() {
                        for value in tmp_stack { stack.push(value); }
                        dispatched = true;
                    }
                }
            }
        });

        if !sig_ok { return Err(Error::trap(INDIRECT_CALL_MISMATCH)); }
        if dispatched {
            next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
        } else {
            return Err(Error::trap(FUNC_NO_IMPL));
        }
    }

    let callee = instance.functions[func_idx].clone();
    if callee.signature() != expected { return Err(Error::trap(INDIRECT_CALL_MISMATCH)); }

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
                let mut ctrl_bases_nested: Vec<usize> = Vec::new();
                owner_rc.call_function_idx(
                    function_index, &mut ret_pc_nested, &mut tmp_stack,
                    &mut control_nested, &mut func_bases_nested, &mut ctrl_bases_nested,
                )?;
                for value in tmp_stack { stack.push(value); }
            } else {
                return Err(Error::trap(FUNC_NO_IMPL));
            }
        }
        RuntimeFunction::OwnedWasm { runtime_sig, pc_start, locals_count } => {
            let new_pc = Instance::setup_wasm_function_call(
                runtime_sig, pc_start, locals_count,
                stack, control, func_bases, ctrl_bases, pc,
            )?;
            next_op!(instance, new_pc, stack, control, func_bases, ctrl_bases)
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

    next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
});

handler_fn!(op_drop, |instance, pc, stack, control, func_bases, ctrl_bases| {
    let _ = pop_value(stack)?;
    next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
});

handler_fn!(op_select, |instance, pc, stack, control, func_bases, ctrl_bases| {
    let cond = pop_value(stack)?.as_u32();
    let v2 = pop_value(stack)?;
    let v1 = pop_value(stack)?;
    stack.push(if cond != 0 { v1 } else { v2 });
    next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
});

handler_fn!(op_local_get, |instance, pc, stack, control, func_bases, ctrl_bases| {
    let bytes = &instance.module.bytes;
    let mut pc = pc;
    let local_idx: u32 = read_leb128(bytes, &mut pc)?;
    let base = *func_bases.last().unwrap();
    let index = base + local_idx as usize;
    stack.push(stack[index]);
    next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
});

handler_fn!(op_local_set, |instance, pc, stack, control, func_bases, ctrl_bases| {
    let bytes = &instance.module.bytes;
    let mut pc = pc;
    let local_idx: u32 = read_leb128(bytes, &mut pc)?;
    let value = pop_value(stack)?;
    let base = *func_bases.last().unwrap();
    let index = base + local_idx as usize;
    stack[index] = value;
    next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
});

handler_fn!(op_local_tee, |instance, pc, stack, control, func_bases, ctrl_bases| {
    let bytes = &instance.module.bytes;
    let mut pc = pc;
    let local_idx: u32 = read_leb128(bytes, &mut pc)?;
    let value = peek_value(stack)?;
    let base = *func_bases.last().unwrap();
    let index = base + local_idx as usize;
    stack[index] = value;
    next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
});

handler_fn!(op_global_get, |instance, pc, stack, control, func_bases, ctrl_bases| {
    let bytes = &instance.module.bytes;
    let mut pc = pc;
    let global_idx: u32 = read_leb128(bytes, &mut pc)?;
    if global_idx as usize >= instance.globals.len() {
        return Err(Error::trap(UNKNOWN_GLOBAL));
    }
    stack.push(instance.globals[global_idx as usize].value.get());
    next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
});

handler_fn!(op_global_set, |instance, pc, stack, control, func_bases, ctrl_bases| {
    let bytes = &instance.module.bytes;
    let mut pc = pc;
    let global_idx: u32 = read_leb128(bytes, &mut pc)?;
    if global_idx as usize >= instance.globals.len() {
        return Err(Error::trap(UNKNOWN_GLOBAL));
    }
    let value = pop_value(stack)?;
    instance.globals[global_idx as usize].value.set(value);
    next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
});

handler_fn!(op_memory_size, |instance, pc, stack, control, func_bases, ctrl_bases| {
    let mut pc = pc + 1; // skip zero flag
    let memory = instance
        .memory
        .as_ref()
        .ok_or_else(|| Error::validation(UNKNOWN_MEMORY))?;
    stack.push(WasmValue::from_u32(memory.borrow().size()));
    next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
});

handler_fn!(op_memory_grow, |instance, pc, stack, control, func_bases, ctrl_bases| {
    let mut pc = pc + 1; // skip zero flag
    let delta = pop_value(stack)?.as_u32();
    let memory = instance
        .memory
        .as_ref()
        .ok_or_else(|| Error::validation(UNKNOWN_MEMORY))?;
    let old = memory.borrow_mut().grow(delta);
    stack.push(WasmValue::from_u32(old));
    next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
});

handler_fn!(op_i32_const, |instance, pc, stack, control, func_bases, ctrl_bases| {
    let bytes = &instance.module.bytes;
    let mut pc = pc;
    let value: i32 = read_sleb128(bytes, &mut pc)?;
    stack.push(WasmValue::from_i32(value));
    next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
});

handler_fn!(op_i64_const, |instance, pc, stack, control, func_bases, ctrl_bases| {
    let bytes = &instance.module.bytes;
    let mut pc = pc;
    let value: i64 = read_sleb128(bytes, &mut pc)?;
    stack.push(WasmValue::from_i64(value));
    next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
});

handler_fn!(op_f32_const, |instance, pc, stack, control, func_bases, ctrl_bases| {
    let bytes = &instance.module.bytes;
    let mut pc = pc;
    let bits = u32::from_le_bytes(bytes[pc..pc + 4].try_into().unwrap());
    pc += 4;
    stack.push(WasmValue::from_f32_bits(bits));
    next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
});

handler_fn!(op_f64_const, |instance, pc, stack, control, func_bases, ctrl_bases| {
    let bytes = &instance.module.bytes;
    let mut pc = pc;
    let bits = u64::from_le_bytes(bytes[pc..pc + 8].try_into().unwrap());
    pc += 8;
    stack.push(WasmValue::from_f64_bits(bits));
    next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
});

handler_fn!(op_i64_eqz, |instance, pc, stack, control, func_bases, ctrl_bases| {
    let value = pop_value(stack)?.as_u64();
    stack.push(WasmValue::from_u32((value == 0) as u32));
    next_op!(instance, pc, stack, control, func_bases, ctrl_bases)
});

handler_fn!(op_unknown, |_instance, _pc, _stack, _control, _func_bases, _ctrl_bases| {
    Err(Error::malformed(UNKNOWN_INSTRUCTION))
});

binary_handler!(op_i32_add, u32, method wrapping_add);
binary_handler!(op_i32_sub, u32, method wrapping_sub);
binary_handler!(op_i32_mul, u32, method wrapping_mul);
binary_handler!(op_i32_and, u32, op &);
binary_handler!(op_i32_or,  u32, op |);
binary_handler!(op_i32_xor, u32, op ^);
binary_handler!(op_i64_add, u64, method wrapping_add);
binary_handler!(op_i64_sub, u64, method wrapping_sub);
binary_handler!(op_i64_mul, u64, method wrapping_mul);
binary_handler!(op_i64_and, u64, op &);
binary_handler!(op_i64_or,  u64, op |);
binary_handler!(op_i64_xor, u64, op ^);
binary_handler!(op_f32_add, f32, op +);
binary_handler!(op_f32_sub, f32, op -);
binary_handler!(op_f32_mul, f32, op *);
binary_handler!(op_f32_div, f32, op /);
binary_handler!(op_f64_add, f64, op +);
binary_handler!(op_f64_sub, f64, op -);
binary_handler!(op_f64_mul, f64, op *);
binary_handler!(op_f64_div, f64, op /);
unary_handler!(op_i32_eqz,   u32, |x: u32| (x == 0) as u32);
unary_handler!(op_i32_clz,   u32, |x: u32| x.leading_zeros());
unary_handler!(op_i32_ctz,   u32, |x: u32| x.trailing_zeros());
unary_handler!(op_i32_popcnt,u32, |x: u32| x.count_ones());
unary_handler!(op_i64_clz,   u64, |x: u64| x.leading_zeros() as u64);
unary_handler!(op_i64_ctz,   u64, |x: u64| x.trailing_zeros() as u64);
unary_handler!(op_i64_popcnt,u64, |x: u64| x.count_ones() as u64);
unary_handler!(op_f32_abs,   f32, |x: f32| x.abs());
unary_handler!(op_f32_neg,   f32, |x: f32| -x);
unary_handler!(op_f32_ceil,  f32, |x: f32| x.ceil());
unary_handler!(op_f32_floor, f32, |x: f32| x.floor());
unary_handler!(op_f32_trunc, f32, |x: f32| x.trunc());
unary_handler!(op_f32_sqrt,  f32, |x: f32| x.sqrt());
unary_handler!(op_f64_abs,   f64, |x: f64| x.abs());
unary_handler!(op_f64_neg,   f64, |x: f64| -x);
unary_handler!(op_f64_ceil,  f64, |x: f64| x.ceil());
unary_handler!(op_f64_floor, f64, |x: f64| x.floor());
unary_handler!(op_f64_trunc, f64, |x: f64| x.trunc());
unary_handler!(op_f64_sqrt,  f64, |x: f64| x.sqrt());
compare_handler!(op_i32_eq,  u32, ==);
compare_handler!(op_i32_ne,  u32, !=);
compare_handler!(op_i32_lt_s,i32, < );
compare_handler!(op_i32_lt_u,u32, < );
compare_handler!(op_i32_gt_s,i32, > );
compare_handler!(op_i32_gt_u,u32, > );
compare_handler!(op_i32_le_s,i32, <=);
compare_handler!(op_i32_le_u,u32, <=);
compare_handler!(op_i32_ge_s,i32, >=);
compare_handler!(op_i32_ge_u,u32, >=);
compare_handler!(op_i64_eq,  i64, ==);
compare_handler!(op_i64_ne,  i64, !=);
compare_handler!(op_i64_lt_s,i64, < );
compare_handler!(op_i64_lt_u,u64, < );
compare_handler!(op_i64_gt_s,i64, > );
compare_handler!(op_i64_gt_u,u64, > );
compare_handler!(op_i64_le_s,i64, <=);
compare_handler!(op_i64_le_u,u64, <=);
compare_handler!(op_i64_ge_s,i64, >=);
compare_handler!(op_i64_ge_u,u64, >=);
compare_handler!(op_f32_eq,  f32, ==);
compare_handler!(op_f32_ne,  f32, !=);
compare_handler!(op_f32_lt,  f32, < );
compare_handler!(op_f32_gt,  f32, > );
compare_handler!(op_f32_le,  f32, <=);
compare_handler!(op_f32_ge,  f32, >=);
compare_handler!(op_f64_eq,  f64, ==);
compare_handler!(op_f64_ne,  f64, !=);
compare_handler!(op_f64_lt,  f64, < );
compare_handler!(op_f64_gt,  f64, > );
compare_handler!(op_f64_le,  f64, <=);
compare_handler!(op_f64_ge,  f64, >=);
shift_handler!(op_i32_shl,   u32, <<);
shift_handler!(op_i32_shr_u, u32, >>);
shift_handler!(op_i64_shl,   u64, <<);
shift_handler!(op_i64_shr_u, u64, >>);
shr_s_handler!(op_i32_shr_s, i32, u32, 32);
shr_s_handler!(op_i64_shr_s, i64, u64, 64);
rotate_handler!(op_i32_rotl, u32, left);
rotate_handler!(op_i32_rotr, u32, right);
rotate_handler!(op_i64_rotl, u64, left);
rotate_handler!(op_i64_rotr, u64, right);
minmax_handler!(op_f32_min, f32, min);
minmax_handler!(op_f32_max, f32, max);
minmax_handler!(op_f64_min, f64, min);
minmax_handler!(op_f64_max, f64, max);
copysign_handler!(op_f32_copysign, f32);
copysign_handler!(op_f64_copysign, f64);
nearest_handler!(op_f32_nearest, f32);
nearest_handler!(op_f64_nearest, f64);
convert_handler!(op_i32_wrap_i64,     u64, u32);
convert_handler!(op_i64_extend_i32_s, i32, i64);
convert_handler!(op_i64_extend_i32_u, u32, u64);
convert_handler!(op_f32_convert_i32_s, i32, f32);
convert_handler!(op_f32_convert_i32_u, u32, f32);
convert_handler!(op_f32_convert_i64_s, i64, f32);
convert_handler!(op_f32_convert_i64_u, u64, f32);
convert_handler!(op_f32_demote_f64,    f64, f32);
convert_handler!(op_f64_convert_i32_s, i32, f64);
convert_handler!(op_f64_convert_i32_u, u32, f64);
convert_handler!(op_f64_convert_i64_s, i64, f64);
convert_handler!(op_f64_convert_i64_u, u64, f64);
convert_handler!(op_f64_promote_f32,   f32, f64);
trunc_handler!(op_i32_trunc_f32_s, f32 -> i32 : -2147483777.0,          2147483648.0);
trunc_handler!(op_i32_trunc_f32_u, f32 -> u32 : -1.0,                    4294967296.0);
trunc_handler!(op_i32_trunc_f64_s, f64 -> i32 : -2147483649.0,           2147483648.0);
trunc_handler!(op_i32_trunc_f64_u, f64 -> u32 : -1.0,                    4294967296.0);
trunc_handler!(op_i64_trunc_f32_s, f32 -> i64 : -9223373136366404000.0,  9223372036854776000.0);
trunc_handler!(op_i64_trunc_f32_u, f32 -> u64 : -1.0,                    18446744073709552000.0);
trunc_handler!(op_i64_trunc_f64_s, f64 -> i64 : -9223372036854777856.0,  9223372036854776000.0);
trunc_handler!(op_i64_trunc_f64_u, f64 -> u64 : -1.0,                    18446744073709552000.0);
load_handler!(op_i32_load,      load_u32, |v: u32| WasmValue::from_u32(v));
load_handler!(op_i64_load,      load_u64, |v: u64| WasmValue::from_u64(v));
load_handler!(op_f32_load,      load_f32, |v: f32| WasmValue::from_f32(v));
load_handler!(op_f64_load,      load_f64, |v: f64| WasmValue::from_f64(v));
load_handler!(op_i32_load8_s,   load_i8,  |v: i8|  WasmValue::from_i32(v as i32));
load_handler!(op_i32_load8_u,   load_u8,  |v: u8|  WasmValue::from_u32(v as u32));
load_handler!(op_i32_load16_s,  load_i16, |v: i16| WasmValue::from_i32(v as i32));
load_handler!(op_i32_load16_u,  load_u16, |v: u16| WasmValue::from_u32(v as u32));
load_handler!(op_i64_load8_s,   load_i8,  |v: i8|  WasmValue::from_i64(v as i64));
load_handler!(op_i64_load8_u,   load_u8,  |v: u8|  WasmValue::from_u64(v as u64));
load_handler!(op_i64_load16_s,  load_i16, |v: i16| WasmValue::from_i64(v as i64));
load_handler!(op_i64_load16_u,  load_u16, |v: u16| WasmValue::from_u64(v as u64));
load_handler!(op_i64_load32_s,  load_i32, |v: i32| WasmValue::from_i64(v as i64));
load_handler!(op_i64_load32_u,  load_u32, |v: u32| WasmValue::from_u64(v as u64));
store_handler!(op_i32_store,   store_u32, |w: WasmValue| w.as_u32());
store_handler!(op_i64_store,   store_u64, |w: WasmValue| w.as_u64());
store_handler!(op_f32_store,   store_f32, |w: WasmValue| w.as_f32());
store_handler!(op_f64_store,   store_f64, |w: WasmValue| w.as_f64());
store_handler!(op_i32_store8,  store_u8,  |w: WasmValue| (w.as_u32() & 0xFF) as u8);
store_handler!(op_i32_store16, store_u16, |w: WasmValue| (w.as_u32() & 0xFFFF) as u16);
store_handler!(op_i64_store8,  store_u8,  |w: WasmValue| (w.as_u64() & 0xFF) as u8);
store_handler!(op_i64_store16, store_u16, |w: WasmValue| (w.as_u64() & 0xFFFF) as u16);
store_handler!(op_i64_store32, store_u32, |w: WasmValue| (w.as_u64() & 0xFFFF_FFFF) as u32);
rem_u_handler!(op_i32_rem_u, u32);
rem_s_handler!(op_i32_rem_s, i32);
rem_u_handler!(op_i64_rem_u, u64);
rem_s_handler!(op_i64_rem_s, i64);
div_u_handler!(op_i32_div_u, u32);
div_s_handler!(op_i32_div_s, i32);
div_u_handler!(op_i64_div_u, u64);
div_s_handler!(op_i64_div_s, i64);


const fn build_handlers_table() -> [Handler; 256] {
    let mut table: [Handler; 256] = [op_unknown as Handler; 256];

    table[0x00] = op_unreachable;
    table[0x01] = op_nop;
    table[0xbc] = op_nop;
    table[0xbd] = op_nop;
    table[0xbe] = op_nop;
    table[0xbf] = op_nop;
    table[0x02] = op_block;
    table[0x03] = op_loop;
    table[0x04] = op_if;
    table[0x05] = op_else;
    table[0x0b] = op_end;
    table[0x0c] = op_br;
    table[0x0d] = op_br_if;
    table[0x0e] = op_br_table;
    table[0x0f] = op_return;
    table[0x10] = op_call;
    table[0x11] = op_call_indirect;
    table[0x1a] = op_drop;
    table[0x1b] = op_select;
    table[0x20] = op_local_get;
    table[0x21] = op_local_set;
    table[0x22] = op_local_tee;
    table[0x23] = op_global_get;
    table[0x24] = op_global_set;
    table[0x28] = op_i32_load;
    table[0x29] = op_i64_load;
    table[0x2a] = op_f32_load;
    table[0x2b] = op_f64_load;
    table[0x2c] = op_i32_load8_s;
    table[0x2d] = op_i32_load8_u;
    table[0x2e] = op_i32_load16_s;
    table[0x2f] = op_i32_load16_u;
    table[0x30] = op_i64_load8_s;
    table[0x31] = op_i64_load8_u;
    table[0x32] = op_i64_load16_s;
    table[0x33] = op_i64_load16_u;
    table[0x34] = op_i64_load32_s;
    table[0x35] = op_i64_load32_u;
    table[0x36] = op_i32_store;
    table[0x37] = op_i64_store;
    table[0x38] = op_f32_store;
    table[0x39] = op_f64_store;
    table[0x3a] = op_i32_store8;
    table[0x3b] = op_i32_store16;
    table[0x3c] = op_i64_store8;
    table[0x3d] = op_i64_store16;
    table[0x3e] = op_i64_store32;
    table[0x3f] = op_memory_size;
    table[0x40] = op_memory_grow;
    table[0x41] = op_i32_const;
    table[0x42] = op_i64_const;
    table[0x43] = op_f32_const;
    table[0x44] = op_f64_const;
    table[0x45] = op_i32_eqz;
    table[0x46] = op_i32_eq;
    table[0x47] = op_i32_ne;
    table[0x48] = op_i32_lt_s;
    table[0x49] = op_i32_lt_u;
    table[0x4a] = op_i32_gt_s;
    table[0x4b] = op_i32_gt_u;
    table[0x4c] = op_i32_le_s;
    table[0x4d] = op_i32_le_u;
    table[0x4e] = op_i32_ge_s;
    table[0x4f] = op_i32_ge_u;
    table[0x50] = op_i64_eqz;
    table[0x51] = op_i64_eq;
    table[0x52] = op_i64_ne;
    table[0x53] = op_i64_lt_s;
    table[0x54] = op_i64_lt_u;
    table[0x55] = op_i64_gt_s;
    table[0x56] = op_i64_gt_u;
    table[0x57] = op_i64_le_s;
    table[0x58] = op_i64_le_u;
    table[0x59] = op_i64_ge_s;
    table[0x5a] = op_i64_ge_u;
    table[0x5b] = op_f32_eq;
    table[0x5c] = op_f32_ne;
    table[0x5d] = op_f32_lt;
    table[0x5e] = op_f32_gt;
    table[0x5f] = op_f32_le;
    table[0x60] = op_f32_ge;
    table[0x61] = op_f64_eq;
    table[0x62] = op_f64_ne;
    table[0x63] = op_f64_lt;
    table[0x64] = op_f64_gt;
    table[0x65] = op_f64_le;
    table[0x66] = op_f64_ge;
    table[0x67] = op_i32_clz;
    table[0x68] = op_i32_ctz;
    table[0x69] = op_i32_popcnt;
    table[0x6a] = op_i32_add;
    table[0x6b] = op_i32_sub;
    table[0x6c] = op_i32_mul;
    table[0x6d] = op_i32_div_s;
    table[0x6e] = op_i32_div_u;
    table[0x6f] = op_i32_rem_s;
    table[0x70] = op_i32_rem_u;
    table[0x71] = op_i32_and;
    table[0x72] = op_i32_or;
    table[0x73] = op_i32_xor;
    table[0x74] = op_i32_shl;
    table[0x75] = op_i32_shr_s;
    table[0x76] = op_i32_shr_u;
    table[0x77] = op_i32_rotl;
    table[0x78] = op_i32_rotr;
    table[0x79] = op_i64_clz;
    table[0x7a] = op_i64_ctz;
    table[0x7b] = op_i64_popcnt;
    table[0x7c] = op_i64_add;
    table[0x7d] = op_i64_sub;
    table[0x7e] = op_i64_mul;
    table[0x7f] = op_i64_div_s;
    table[0x80] = op_i64_div_u;
    table[0x81] = op_i64_rem_s;
    table[0x82] = op_i64_rem_u;
    table[0x83] = op_i64_and;
    table[0x84] = op_i64_or;
    table[0x85] = op_i64_xor;
    table[0x86] = op_i64_shl;
    table[0x87] = op_i64_shr_s;
    table[0x88] = op_i64_shr_u;
    table[0x89] = op_i64_rotl;
    table[0x8a] = op_i64_rotr;
    table[0x8b] = op_f32_abs;
    table[0x8c] = op_f32_neg;
    table[0x8d] = op_f32_ceil;
    table[0x8e] = op_f32_floor;
    table[0x8f] = op_f32_trunc;
    table[0x90] = op_f32_nearest;
    table[0x91] = op_f32_sqrt;
    table[0x92] = op_f32_add;
    table[0x93] = op_f32_sub;
    table[0x94] = op_f32_mul;
    table[0x95] = op_f32_div;
    table[0x96] = op_f32_min;
    table[0x97] = op_f32_max;
    table[0x98] = op_f32_copysign;
    table[0x99] = op_f64_abs;
    table[0x9a] = op_f64_neg;
    table[0x9b] = op_f64_ceil;
    table[0x9c] = op_f64_floor;
    table[0x9d] = op_f64_trunc;
    table[0x9e] = op_f64_nearest;
    table[0x9f] = op_f64_sqrt;
    table[0xa0] = op_f64_add;
    table[0xa1] = op_f64_sub;
    table[0xa2] = op_f64_mul;
    table[0xa3] = op_f64_div;
    table[0xa4] = op_f64_min;
    table[0xa5] = op_f64_max;
    table[0xa6] = op_f64_copysign;
    table[0xa7] = op_i32_wrap_i64;
    table[0xa8] = op_i32_trunc_f32_s;
    table[0xa9] = op_i32_trunc_f32_u;
    table[0xaa] = op_i32_trunc_f64_s;
    table[0xab] = op_i32_trunc_f64_u;
    table[0xac] = op_i64_extend_i32_s;
    table[0xad] = op_i64_extend_i32_u;
    table[0xae] = op_i64_trunc_f32_s;
    table[0xaf] = op_i64_trunc_f32_u;
    table[0xb0] = op_i64_trunc_f64_s;
    table[0xb1] = op_i64_trunc_f64_u;
    table[0xb2] = op_f32_convert_i32_s;
    table[0xb3] = op_f32_convert_i32_u;
    table[0xb4] = op_f32_convert_i64_s;
    table[0xb5] = op_f32_convert_i64_u;
    table[0xb6] = op_f32_demote_f64;
    table[0xb7] = op_f64_convert_i32_s;
    table[0xb8] = op_f64_convert_i32_u;
    table[0xb9] = op_f64_convert_i64_s;
    table[0xba] = op_f64_convert_i64_u;
    table[0xbb] = op_f64_promote_f32;

    table
}

const HANDLERS: [Handler; 256] = build_handlers_table();

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
        pc: usize,
        stack: &mut Vec<WasmValue>,
        control: &mut Vec<ControlFrame>,
        func_bases: &mut Vec<usize>,
        ctrl_bases: &mut Vec<usize>
    ) -> Result<(), Error> {
        return op_nop(self, pc, stack, control, func_bases, ctrl_bases)
    }

    #[inline(always)]
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