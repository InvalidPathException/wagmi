use std::collections::HashMap;
use std::hint::black_box;
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use wagmi::{ExportValue, Imports, Instance, Module, RuntimeFunction, ValType, WasmValue};

fn clock_ms_i64() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Clock may have gone backwards")
        .as_millis() as i64
}

fn setup_instance() -> (Instance, wagmi::RuntimeFunction) {
    let coremark_bytes = include_bytes!("coremark-minimal.wasm");
    let module = Module::compile(coremark_bytes.to_vec()).expect("compile coremark");
    let module = Rc::new(module);

    let clock_fn = RuntimeFunction::new_host(vec![], Some(ValType::I64), move |_args| {
        Some(WasmValue::from_i64(clock_ms_i64()))
    });

    let mut imports: Imports = Imports::new();
    let mut env_mod: HashMap<String, ExportValue> = HashMap::new();
    env_mod.insert("clock_ms".to_string(), ExportValue::Function(clock_fn));
    imports.insert("env".to_string(), env_mod);

    let instance = Instance::instantiate(module, &imports).expect("instantiate coremark");
    let run_fn = match instance.exports.get("run") {
        Some(ExportValue::Function(f)) => f.clone(),
        _ => panic!("Export 'run' not found or not a function"),
    };
    (instance, run_fn)
}

fn bench_coremark(c: &mut Criterion) {
    let (instance, run_fn) = setup_instance();
    let t0 = Instant::now();
    let results_once = instance.invoke(&run_fn, &[]).expect("invoke run once");
    let elapsed_once = t0.elapsed();
    let score_once = results_once[0].as_f32();
    println!("coremark single-run: elapsed={:.6}s score={}", elapsed_once.as_secs_f64(), score_once);

    let mut group = c.benchmark_group("coremark_minimal");
    let mut scores: Vec<f32> = Vec::new();
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(600));
    group.warm_up_time(Duration::from_secs(1));
    group.throughput(Throughput::Elements(1));
    group.bench_function("run", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let start = Instant::now();
                let results = instance.invoke(&run_fn, &[]).expect("invoke run");
                black_box(&results);
                total += start.elapsed();
                let score = results[0].as_f32();
                scores.push(score);
            }
            total
        });
    });
    group.finish();
    if !scores.is_empty() {
        let sum: f64 = scores.iter().map(|&s| s as f64).sum();
        let avg = sum / scores.len() as f64;
        println!("coremark scores ({} samples): {:?} avg = {:.6}", scores.len(), scores, avg);
    }
}

criterion_group!(benches, bench_coremark);
criterion_main!(benches);


