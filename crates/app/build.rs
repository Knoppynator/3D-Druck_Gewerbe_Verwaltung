//! Bettet das Programmsymbol (`assets/druckplaner.ico`) und die Versionsangaben in die Windows-`.exe` ein.

fn main() {
    println!("cargo:rerun-if-changed=assets/druckplaner.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/druckplaner.ico");
    res.set("FileDescription", "Druckplaner – Verwaltung für 3D-Druck-Serienfertigung");
    res.set("ProductName", "Druckplaner");
    // Ohne Windows-SDK (rc.exe) bleibt die exe ohne eigenes Symbol, der Build scheitert daran nicht
    if let Err(e) = res.compile() {
        println!("cargo:warning=Programmsymbol nicht eingebettet: {e}");
    }
}
