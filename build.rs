// `sqlx::migrate!()` embeds the files in ./migrations into the binary at
// compile time. Cargo doesn't know that, so without this line, adding a new
// migration file wouldn't trigger a rebuild and the new migration would
// silently never run.
fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
