fn main() {
    println!("cargo:rerun-if-changed=assets/refdesk.rc");
    println!("cargo:rerun-if-changed=assets/refdesk.ico");
    embed_resource::compile("assets/refdesk.rc", embed_resource::NONE)
        .manifest_optional()
        .expect("failed to compile the Refdesk icon resource");
}
