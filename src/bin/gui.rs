// New (not from keypeek): a small one-shot "pick device, pick options,
// export" window, built fresh with plain eframe rather than adapting
// keypeek's settings GUI — that one drives a persistent live-overlay
// connection (background reconnect, ZMK/BLE transports, always-on-top
// platform-specific windowing) which is a different problem from ours: scan
// once, read once, write a PDF, done. Only egui-file-dialog (for the save
// path) is shared tooling, used the same way keypeek uses it.

use egui_file_dialog::FileDialog;
use qmk_via_api::scan::{scan_keyboards, KeyboardDeviceInfo};
use std::path::PathBuf;
use keyprint::{os_layout, pdf, vial::VialProtocol};

fn main() -> eframe::Result<()> {
    // macOS must snapshot the active layout on the main thread before the
    // UI thread starts label resolution; a no-op on other platforms.
    os_layout::init();

    let options = eframe::NativeOptions {
        // Fixed size rather than just an initial hint: tiling WMs (Hyprland
        // et al.) generally auto-float a window that declares it can't be
        // resized, instead of stretching it to fill a tile.
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([700.0, 560.0])
            .with_min_inner_size([700.0, 560.0])
            .with_max_inner_size([700.0, 560.0])
            .with_resizable(false),
        ..Default::default()
    };
    eframe::run_native(
        "KeyPrint",
        options,
        Box::new(|cc| {
            // egui's default text size reads small at normal desktop scale —
            // bump it rather than relying on OS font-scaling settings, which
            // egui doesn't pick up on its own.
            cc.egui_ctx.set_zoom_factor(1.4);
            Ok(Box::new(App::new()))
        }),
    )
}

struct App {
    devices: Vec<KeyboardDeviceInfo>,
    selected_device: Option<usize>,
    portrait: bool,
    layers_per_page: usize,
    output_path: PathBuf,
    output_path_is_custom: bool,
    file_dialog: FileDialog,
    status: Option<Result<String, String>>,
}

impl App {
    fn new() -> Self {
        let devices = scan_keyboards().unwrap_or_default();

        let mut app = Self {
            devices,
            selected_device: None,
            portrait: false,
            layers_per_page: 1,
            output_path: PathBuf::from("keyboard.pdf"),
            output_path_is_custom: false,
            file_dialog: FileDialog::new(),
            status: None,
        };
        if !app.devices.is_empty() {
            app.select_device(0);
        }
        app
    }

    fn select_device(&mut self, index: usize) {
        self.selected_device = Some(index);
        if !self.output_path_is_custom {
            if let Some(dev) = self.devices.get(index) {
                let name = dev.product.as_deref().unwrap_or("keyboard").replace(' ', "_");
                self.output_path = PathBuf::from(format!("{name}.pdf"));
            }
        }
    }

    fn rescan(&mut self) {
        self.devices = scan_keyboards().unwrap_or_default();
        self.selected_device = None;
        if !self.devices.is_empty() {
            self.select_device(0);
        }
    }

    fn export(&mut self) {
        let Some(dev) = self.selected_device.and_then(|i| self.devices.get(i)) else {
            self.status = Some(Err("No device selected".to_string()));
            return;
        };

        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let protocol = VialProtocol::connect(dev.vendor_id, dev.product_id)?;
            let def = protocol.definition();
            let layout = &def.layouts[0];
            let layer_count = protocol.get_layer_count()?;
            let all_keys = protocol.read_all_keys(layer_count, def.rows, def.cols);
            let product_name = dev.product.as_deref().unwrap_or("keyboard");
            pdf::export(
                layout,
                &all_keys,
                product_name,
                self.output_path.to_string_lossy().as_ref(),
                self.portrait,
                self.layers_per_page,
            )?;
            Ok(())
        })();

        self.status = Some(match result {
            Ok(()) => Ok(format!("Wrote {}", self.output_path.display())),
            Err(e) => Err(e.to_string()),
        });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.file_dialog.update(ui.ctx());
        if let Some(path) = self.file_dialog.take_picked() {
            self.output_path = path;
            self.output_path_is_custom = true;
        }

        egui::CentralPanel::default().show(ui, |ui| {
            // Scoped to this panel only — the file dialog is drawn in its
            // own Area/Window off the shared context style, so this must
            // not go through `ctx`/`all_styles_mut` or it'd cramp/space out
            // the dialog's own rows too.
            ui.style_mut().spacing.item_spacing.y = 14.0;

            ui.heading("KeyPrint");
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.label("Keyboard:");
                egui::ComboBox::from_id_salt("device")
                    .selected_text(
                        self.selected_device
                            .and_then(|i| self.devices.get(i))
                            .map(|d| d.product.clone().unwrap_or_default())
                            .unwrap_or_else(|| "(none found)".to_string()),
                    )
                    .show_ui(ui, |ui| {
                        for i in 0..self.devices.len() {
                            let label = self.devices[i].product.clone().unwrap_or_default();
                            if ui
                                .selectable_label(self.selected_device == Some(i), label)
                                .clicked()
                            {
                                self.select_device(i);
                            }
                        }
                    });
                if ui.button("Rescan").clicked() {
                    self.rescan();
                }
            });

            ui.checkbox(&mut self.portrait, "Portrait");

            ui.horizontal(|ui| {
                ui.label("Layers per page:");
                ui.add(egui::DragValue::new(&mut self.layers_per_page).range(1..=8));
            });

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label("Output:");
                ui.monospace(self.output_path.display().to_string());
            });
            if ui.button("Choose output file...").clicked() {
                self.file_dialog
                    .config_mut()
                    .default_file_name
                    .clone_from(&self.output_path.to_string_lossy().to_string());
                self.file_dialog.save_file();
            }

            ui.add_space(12.0);
            if ui
                .add_enabled(self.selected_device.is_some(), egui::Button::new("Export PDF"))
                .clicked()
            {
                self.export();
            }

            if let Some(status) = &self.status {
                ui.add_space(8.0);
                match status {
                    Ok(msg) => {
                        ui.colored_label(egui::Color32::from_rgb(60, 160, 60), msg);
                    }
                    Err(msg) => {
                        ui.colored_label(egui::Color32::from_rgb(200, 60, 60), msg);
                    }
                }
            }
        });
    }
}
