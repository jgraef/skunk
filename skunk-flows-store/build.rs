fn main() {
    println!(
        "cargo::rustc-env=DATABASE_URL=sqlite:{}/schema.db",
        std::env::var("CARGO_MANIFEST_DIR").unwrap(),
    );
}
