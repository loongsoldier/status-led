//! Build script: pre-compute CIE L* reference table for test validation.
//! The compact 32-byte runtime tables are hardcoded in src/pwm.rs;
//! this full 256-byte table is used only in `#[cfg(test)]` to verify
//! that the compact interpolation matches the exact CIE L* formula.

use std::fmt::Write;

fn main() {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let dest = std::path::Path::new(&out_dir).join("gamma_tables.rs");

    let mut code = String::new();
    let _ = writeln!(
        code,
        "// Auto-generated CIE L* reference table (test-only)."
    );

    let cie_lstar = cie_lstar_table();
    write_table(&mut code, "CIE_LSTAR", &cie_lstar);

    std::fs::write(&dest, code).unwrap();
    println!("cargo:rerun-if-changed=build.rs");
}

fn cie_lstar_table() -> [u8; 256] {
    let mut table = [0u8; 256];
    for (raw, out) in table.iter_mut().enumerate() {
        let t = raw as f64 / 255.0;
        let l_star = if t <= 0.008856 {
            903.3 * t
        } else {
            116.0 * t.powf(1.0 / 3.0) - 16.0
        };
        let val = (l_star * 2.55).round() as i32;
        *out = val.clamp(0, 255) as u8;
    }
    table
}

fn write_table(code: &mut String, name: &str, table: &[u8; 256]) {
    let _ = writeln!(code, "#[rustfmt::skip]");
    let _ = writeln!(code, "#[allow(dead_code)]");
    let _ = writeln!(code, "pub(crate) const {name}: [u8; 256] = [");
    for row in table.chunks(16) {
        let line = row
            .iter()
            .map(|v| format!("{v:3}"))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(code, "    {line},");
    }
    let _ = writeln!(code, "];\n");
}
