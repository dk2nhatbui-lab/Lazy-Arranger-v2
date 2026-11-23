fn main() {
    for path in std::env::args().skip(1) {
        match plugin_host::scan_vst3(std::path::Path::new(&path)) {
            Ok(classes) => {
                println!("{path}: {} class(es)", classes.len());
                for class in classes {
                    println!(
                        "  {} | {} | {} | {}",
                        class.name, class.vendor, class.version, class.subcategories
                    );
                }
            }
            Err(error) => println!("{path}: {error}"),
        }
    }
}
