fn main() {
    let mock = std::env::args().any(|a| a == "--mock");
    if mock {
        eprintln!("troweld --mock is not implemented yet (Phase 1)");
    } else {
        eprintln!("troweld is not implemented yet (Phase 5)");
    }
    std::process::exit(2);
}
