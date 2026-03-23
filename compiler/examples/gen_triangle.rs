use quantalang::codegen::backend::spirv::SpirvBackend;

fn main() {
    let mut backend = SpirvBackend::new();

    let vert = backend.generate_triangle_vertex_shader();
    std::fs::write("demos/hardcoded_vert.spv", &vert).expect("write vert");
    eprintln!("Wrote demos/hardcoded_vert.spv ({} bytes)", vert.len());

    let frag = backend.generate_triangle_fragment_shader();
    std::fs::write("demos/hardcoded_frag.spv", &frag).expect("write frag");
    eprintln!("Wrote demos/hardcoded_frag.spv ({} bytes)", frag.len());
}
