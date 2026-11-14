fn main() {
    println!("cargo:rerun-if-changed=omniprint-manifest.rc");
    println!("cargo:rerun-if-changed=omniprint.manifest");
    embed_resource::compile("omniprint-manifest.rc", embed_resource::NONE);
}
