fn main() {
    let mut generator = micropb_gen::Generator::new();
    generator.use_container_heapless();
    generator.configure(".", micropb_gen::Config::new().max_len(12));
    // Compile example.proto into a Rust module
    generator
        .compile_protos(&["proto/readings.proto"], std::env::var("OUT_DIR").unwrap() + "/generated_proto.rs")
        .unwrap();
}
