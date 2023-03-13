#[cfg(feature = "network")]
fn main() {
    let mut config = prost_build::Config::new();
    config.type_attribute(".", "#[derive(PartialOrd)]");
    config
        .out_dir("src/network/pb")
        .compile_protos(&["src/network/abi.proto"], &["src/network"])
        .unwrap();
}

#[cfg(not(feature = "network"))]
fn main() {}
