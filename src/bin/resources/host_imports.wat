;; Module that imports host functions
(module
  ;; Import host functions
  (import "host" "print" (func $print (param i32)))
  (import "host" "random" (func $random (result i32)))
  (import "host" "add" (func $host_add (param i32 i32) (result i32)))
  
  ;; Main function that uses host imports
  (func $main (export "main") (result i32)
    ;; Call host.add(42, 8)
    i32.const 42
    i32.const 8
    call $host_add
    
    ;; Print the result
    call $print
    
    ;; Return 0
    i32.const 0)
  
  ;; Generate and print a random number
  (func $print_random (export "print_random") (result i32)
    ;; Get random number
    call $random
    
    ;; Print it
    call $print
    
    ;; Return 0
    i32.const 0)
  
  ;; Calculate using random numbers
  (func $random_calc (export "random_calculation") (result i32)
    ;; Get two random numbers and add them
    call $random
    call $random
    call $host_add
    
    ;; Print result
    call $print
    
    ;; Return the result
    call $random
    call $random
    call $host_add)
)