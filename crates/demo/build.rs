fn main() {
    // Compiles `proto/hello.proto` into `$OUT_DIR/hello.rs` (client + server).
    //
    // This uses the `tonic-prost-build` / `tonic-build` toolchain. Consumers may
    // alternatively generate the same code with `buf`; the resulting
    // `GreeterServer<T>` plugs into `add_grpc_service` identically.
    tonic_prost_build::compile_protos("proto/hello.proto")
        .expect("failed to compile proto/hello.proto");
}
