use std::{
    path::PathBuf,
    process::Command,
};

fn main() {
    let crate_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    let migrations_dir = crate_dir.join("migrations");
    let db_file = out_dir.join("schema.db");
    let db_url = format!("sqlite://{}", db_file.display());

    println!("cargo::rerun-if-changed={}", migrations_dir.display());

    if db_file.exists() {
        std::fs::remove_file(&db_file).unwrap();
    }

    std::fs::write(&db_file, b"").unwrap();

    let exit_status = Command::new("sqlx")
        .arg("migrate")
        .arg("run")
        .arg("--source")
        .arg(&migrations_dir)
        .arg("--database-url")
        .arg(&db_url)
        .spawn()
        .unwrap()
        .wait()
        .unwrap();

    if !exit_status.success() {
        panic!("sqlx failed: {exit_status}");
    }

    println!("cargo::rustc-env=DATABASE_URL={}", db_url,);
}
