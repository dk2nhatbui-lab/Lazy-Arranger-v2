fn main() {
    for path in std::env::args().skip(1) {
        match plugin_host::scan_vst3(std::path::Path::new(&path)) {
            Ok(classes) => {
                println!("{path}: {} class(es)", classes.len());
                for class in &classes {
                    println!(
                        "  {} | {} | {} | {}",
                        class.name, class.vendor, class.version, class.subcategories
                    );
                }
                if let Ok(effect) = plugin_host::Vst3Effect::new(
                    std::path::Path::new(&path),
                    &classes[0].cid,
                    48_000.0,
                    256,
                ) {
                    for (id, title, default) in effect.parameters() {
                        println!("  param {id} {title:?} default={default}");
                    }
                }
            }
            Err(error) => println!("{path}: {error}"),
        }
    }
}
