fn main() {
    for key in ["DMA", "METRICS", "SCROLL", "POINTER", "REPEAT"] {
        let cfg = format!("liuli_{}", key.to_ascii_lowercase());
        println!("cargo::rustc-check-cfg=cfg({cfg})");
        if std::env::var(format!("DEP_LIULI_PROFILE_{key}")).unwrap() == "1" {
            println!("cargo::rustc-cfg={cfg}");
        }
    }
}
