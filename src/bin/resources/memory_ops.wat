;; Memory operations example
(module
  ;; Define a memory with 1 initial page
  (memory (export "memory") 1)
  
  ;; Store i32 at given offset
  (func $store_i32 (export "store_i32") (param $offset i32) (param $value i32)
    local.get $offset
    local.get $value
    i32.store)
  
  ;; Load i32 from given offset
  (func $load_i32 (export "load_i32") (param $offset i32) (result i32)
    local.get $offset
    i32.load)
  
  ;; Store byte at given offset
  (func $store_byte (export "store_byte") (param $offset i32) (param $value i32)
    local.get $offset
    local.get $value
    i32.store8)
  
  ;; Load byte from given offset
  (func $load_byte (export "load_byte") (param $offset i32) (result i32)
    local.get $offset
    i32.load8_u)
  
  ;; Fill memory region with a value
  (func $memset (export "memset") (param $offset i32) (param $value i32) (param $length i32)
    (local $end i32)
    
    ;; Calculate end offset
    local.get $offset
    local.get $length
    i32.add
    local.set $end
    
    ;; Fill loop
    (block $done
      (loop $loop
        ;; Check if we've reached the end
        local.get $offset
        local.get $end
        i32.ge_u
        br_if $done
        
        ;; Store byte
        local.get $offset
        local.get $value
        i32.store8
        
        ;; Increment offset
        local.get $offset
        i32.const 1
        i32.add
        local.set $offset
        
        ;; Continue
        br $loop)))
  
  ;; Copy memory region
  (func $memcpy (export "memcpy") (param $dest i32) (param $src i32) (param $length i32)
    (local $i i32)
    
    ;; Initialize counter
    i32.const 0
    local.set $i
    
    ;; Copy loop
    (block $done
      (loop $loop
        ;; Check if we've copied everything
        local.get $i
        local.get $length
        i32.ge_u
        br_if $done
        
        ;; Copy byte: dest[i] = src[i]
        local.get $dest
        local.get $i
        i32.add
        local.get $src
        local.get $i
        i32.add
        i32.load8_u
        i32.store8
        
        ;; Increment counter
        local.get $i
        i32.const 1
        i32.add
        local.set $i
        
        ;; Continue
        br $loop)))
)