use qmk_via_api::scan::scan_keyboards;
use keyprint::{os_layout, pdf, vial::VialProtocol};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    os_layout::init();

    let mut arg_index: Option<usize> = None;
    let mut portrait = false;
    let mut layers_per_page: usize = 1;
    for arg in std::env::args().skip(1) {
        if let Some(n) = arg.strip_prefix("--layers-per-page=") {
            layers_per_page = n.parse().unwrap_or(1);
        } else if arg == "--portrait" {
            portrait = true;
        } else if let Ok(n) = arg.parse::<usize>() {
            arg_index = Some(n);
        }
    }

    let devices = scan_keyboards()?;
    if devices.is_empty() {
        println!("No VIA/Vial devices found");
        return Ok(());
    }

    println!("Found devices:");
    for (i, dev) in devices.iter().enumerate() {
        println!(
            "  [{i}] {} {} (vid={:04x} pid={:04x})",
            dev.manufacturer.as_deref().unwrap_or("?"),
            dev.product.as_deref().unwrap_or("?"),
            dev.vendor_id,
            dev.product_id
        );
    }

    for (i, dev) in devices.iter().enumerate() {
        if let Some(want) = arg_index {
            if want != i {
                continue;
            }
        }

        println!("\nConnecting to [{i}] {}...", dev.product.as_deref().unwrap_or("?"));
        let protocol = match VialProtocol::connect(dev.vendor_id, dev.product_id) {
            Ok(p) => p,
            Err(e) => {
                println!("  skip (not a Vial device or connect failed: {e})");
                continue;
            }
        };

        let def = protocol.definition();
        // kle_parser always emits a single "default" layout.
        let layout = &def.layouts[0];
        let layer_count = protocol.get_layer_count()?;
        println!("  layers: {layer_count}, rows: {}, cols: {}", def.rows, def.cols);

        let all_keys = protocol.read_all_keys(layer_count, def.rows, def.cols);

        let product_name = dev.product.as_deref().unwrap_or("keyboard");
        let out_path = format!("{}.pdf", product_name.replace(' ', "_"));
        pdf::export(layout, &all_keys, product_name, &out_path, portrait, layers_per_page)?;
        println!("  wrote {out_path}");
    }

    Ok(())
}
