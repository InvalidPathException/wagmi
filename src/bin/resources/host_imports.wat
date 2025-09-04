(module
  (import "host" "print" (func $print (param i32)))
  (import "host" "random" (func $random (result i32)))
  (import "host" "add" (func $host_add (param i32 i32) (result i32)))
  (import "host" "mul" (func $host_mul (param i32 i32) (result i32)))
  (import "host" "counter_inc" (func $counter_inc (result i32)))
  (import "host" "counter_get" (func $counter_get (result i32)))
  
  (func $main (export "main") (result i32)
    i32.const 42
    i32.const 8
    call $host_add
    call $print
    i32.const 0)
  
  (func $sequence (export "sequence") (result i32)
    (local i32)
    call $random
    local.tee 0
    call $print
    
    local.get 0
    i32.const 100
    call $host_add
    call $print
    
    call $random
    i32.const 2
    call $host_mul
    call $random
    i32.const 3
    call $host_mul
    call $host_add)
  
  (func $helper_double (param i32) (result i32)
    local.get 0
    i32.const 2
    call $host_mul)
  
  (func $helper_triple (param i32) (result i32)
    local.get 0
    i32.const 3
    call $host_mul)
  
  (func $nested_calls (export "nested_calls") (result i32)
    (local i32)
    call $random
    call $helper_double
    call $helper_triple
    local.tee 0
    call $print
    local.get 0)
  
  (func $stateful (export "stateful") (result i32)
    (local i32)
    call $counter_inc
    call $print
    call $counter_inc
    call $print
    call $counter_inc
    call $print
    
    call $counter_get
    call $random
    call $host_add
    local.tee 0
    call $print
    local.get 0)
)