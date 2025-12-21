use std::path::PathBuf;

fn main() {
    let mut generator = micropb_gen::Generator::new();
    generator.use_container_heapless();
    generator.configure(".", micropb_gen::Config::new().max_len(12));
    // Compile example.proto into a Rust module
    generator
        .compile_protos(&["proto/readings.proto"], std::env::var("OUT_DIR").unwrap() + "/generated_proto.rs")
        .unwrap();

    println!("cargo:rerun-if-env-changed=SOLAR_BACKEND_BASE_URL");
    println!("cargo:rerun-if-env-changed=SOLAR_BACKEND_TOKEN");

    let url = std::env::var("SOLAR_BACKEND_BASE_URL").expect("SOLAR_BACKEND_BASE_URL not set");
    let token = std::env::var("SOLAR_BACKEND_TOKEN").expect("SOLAR_BACKEND_TOKEN not set");

    let out_dir_path = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let out_file_path = out_dir_path.join("consts.rs");

    std::fs::write(
        out_file_path,
        format!(
            "
            // generated form env vars
            pub const SOLAR_BACKEND_BASE_URL: &str = \"{url}\";
            pub(crate) const SOLAR_BACKEND_TOKEN: &str = \"{token}\";"
        ),
    )
    .unwrap();
}

/*

pub const SOLAR_BACKEND_BASE_URL: &str = env!("SOLAR_BACKEND_BASE_URL");

const SOLAR_BACKEND_TOKEN: &str = env!("SOLAR_BACKEND_TOKEN");

*/
