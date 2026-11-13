//! DexNotas in a desktop window, to iterate without a device:
//! `cargo run -p dexnotas --features desktop -- [file…]`.

fn main() {
    let launch = dexnotas::Launch { open: std::env::args().skip(1).map(Into::into).collect(), persistence_path: None };
    let options_launch = launch.clone();
    dexnotas::run(move |renderer| dexnotas::native_options(renderer, &options_launch), launch);
}
