//! Candidate A measurement: embedded WASM compiler (`wasmtime`, Cranelift JIT).

fn main() {
    let wat = r#"(module
      (import "host" "log" (func $log (param i32)))
      (func (export "run") (param i32) (result i32)
        local.get 0
        call $log
        local.get 0
        i32.const 1
        i32.add))"#;

    let t0 = std::time::Instant::now();
    let mut config = wasmtime::Config::new();
    config.cranelift_opt_level(wasmtime::OptLevel::Speed);
    let engine = wasmtime::Engine::new(&config).expect("engine");
    let engine_init = t0.elapsed();

    let bytes = wat::parse_str(wat).expect("wat");
    let t1 = std::time::Instant::now();
    let module = wasmtime::Module::new(&engine, bytes.as_slice()).expect("module");
    let compile = t1.elapsed();

    let t2 = std::time::Instant::now();
    let mut store = wasmtime::Store::new(&engine, ());
    let mut linker = wasmtime::Linker::new(&engine);
    linker
        .func_wrap("host", "log", |_: i32| {})
        .expect("link log");
    let instance = linker.instantiate(&mut store, &module).expect("instantiate");
    let run = instance
        .get_typed_func::<i32, i32>(&mut store, "run")
        .expect("run");
    let instantiate = t2.elapsed();

    let t3 = std::time::Instant::now();
    let mut acc = 0;
    for i in 0..1000 {
        acc = run.call(&mut store, i).expect("call");
    }
    let calls = t3.elapsed();

    println!(
        r#"{{"engine":"wasmtime","engine_init_us":{},"compile_us":{},"instantiate_us":{},"calls_1000_us":{},"call_avg_us":{:.3},"acc":{}}}"#,
        engine_init.as_micros(),
        compile.as_micros(),
        instantiate.as_micros(),
        calls.as_micros(),
        calls.as_micros() as f64 / 1000.0,
        acc
    );
}
