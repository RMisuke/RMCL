//! Baseline: same output shape, no WASM engine — used to attribute binary size.

fn main() {
    println!(
        r#"{{"engine":"baseline","engine_init_us":0,"compile_us":0,"instantiate_us":0,"calls_1000_us":0,"call_avg_us":0.0,"acc":0}}"#
    );
}
