#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

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
use keyprint::{os_layout, pdf, types::KeyboardLayout, vial::VialProtocol};

fn main() -> eframe::Result<()> {
    // macOS must snapshot the active layout on the main thread before the
    // UI thread starts label resolution; a no-op on other platforms.
    os_layout::init();

    let options = eframe::NativeOptions {
        // Fixed size rather than just an initial hint: tiling WMs (Hyprland
        // et al.) generally auto-float a window that declares it can't be
        // resized, instead of stretching it to fill a tile.
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 560.0])
            .with_min_inner_size([1000.0, 560.0])
            .with_max_inner_size([1000.0, 560.0])
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

fn color32_to_rgb(c: egui::Color32) -> (f32, f32, f32) {
    (c.r() as f32 / 255.0, c.g() as f32 / 255.0, c.b() as f32 / 255.0)
}

struct App {
    devices: Vec<KeyboardDeviceInfo>,
    selected_device: Option<usize>,
    portrait: bool,
    center_vertically: bool,
    layers_per_page: usize,
    show_shifted: bool,
    show_altgr: bool,
    shift_color: egui::Color32,
    altgr_color: egui::Color32,
    output_path: PathBuf,
    output_path_is_custom: bool,
    file_dialog: FileDialog,
    status: Option<Result<String, String>>,
    /// Layout + layer count for the page-1 preview, read once on selection
    /// rather than on every frame (a fresh HID roundtrip per repaint would
    /// be wasteful and can hitch the UI).
    preview_layout: Option<(KeyboardLayout, usize)>,
}

impl App {
    fn new() -> Self {
        let devices = scan_keyboards().unwrap_or_default();

        let mut app = Self {
            devices,
            selected_device: None,
            portrait: false,
            center_vertically: false,
            layers_per_page: 1,
            show_shifted: false,
            show_altgr: false,
            shift_color: egui::Color32::from_rgb(0, 0, 200),
            altgr_color: egui::Color32::from_rgb(200, 0, 0),
            output_path: PathBuf::from("keyboard.pdf"),
            output_path_is_custom: false,
            file_dialog: FileDialog::new(),
            status: None,
            preview_layout: None,
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

        self.preview_layout = self.devices.get(index).and_then(|dev| {
            let protocol = VialProtocol::connect(dev.vendor_id, dev.product_id).ok()?;
            let layer_count = protocol.get_layer_count().ok()?;
            Some((protocol.definition().layouts[0].clone(), layer_count))
        });
    }

    fn rescan(&mut self) {
        self.devices = scan_keyboards().unwrap_or_default();
        self.selected_device = None;
        self.preview_layout = None;
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
            let all_encoders = protocol.read_all_encoders(layer_count, layout.encoder_count());
            let product_name = dev.product.as_deref().unwrap_or("keyboard");
            pdf::export(
                layout,
                &all_keys,
                &all_encoders,
                product_name,
                self.output_path.to_string_lossy().as_ref(),
                self.portrait,
                self.layers_per_page,
                self.center_vertically,
                pdf::LegendOptions {
                    show_shifted: self.show_shifted,
                    show_altgr: self.show_altgr,
                    shift_color: color32_to_rgb(self.shift_color),
                    altgr_color: color32_to_rgb(self.altgr_color),
                },
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
            // Explicit rects rather than a plain horizontal split: the
            // controls column is only as tall as its widgets, and a
            // horizontal layout sizes its cross axis (height) to that
            // shorter child, starving the preview of the full window height.
            let full_rect = ui.max_rect();
            let controls_rect = egui::Rect::from_min_size(full_rect.min, egui::vec2(320.0, full_rect.height()));
            let preview_rect = egui::Rect::from_min_max(
                egui::pos2(controls_rect.max.x + 12.0, full_rect.min.y),
                full_rect.max,
            );

            ui.new_child(egui::UiBuilder::new().max_rect(controls_rect)).scope(|ui| {
                self.controls_ui(ui);
            });
            ui.painter().vline(controls_rect.max.x + 6.0, full_rect.y_range(), ui.visuals().widgets.noninteractive.bg_stroke);
            ui.new_child(egui::UiBuilder::new().max_rect(preview_rect)).scope(|ui| {
                self.preview_ui(ui);
            });
        });
    }
}

impl App {
    fn controls_ui(&mut self, ui: &mut egui::Ui) {
            // Scoped to this column only — the file dialog is drawn in its
            // own Area/Window off the shared context style, so this must
            // not go through `ctx`/`all_styles_mut` or it'd cramp/space out
            // the dialog's own rows too.
            ui.style_mut().spacing.item_spacing.y = 14.0;
            ui.set_width(320.0);

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
            ui.checkbox(&mut self.center_vertically, "Center vertically on page");

            ui.horizontal(|ui| {
                ui.label("Layers per page:");
                ui.add(egui::DragValue::new(&mut self.layers_per_page).range(1..=8));
            });

            ui.horizontal(|ui| {
                ui.checkbox(&mut self.show_shifted, "Show shifted char");
                ui.add_enabled_ui(self.show_shifted, |ui| {
                    egui::color_picker::color_edit_button_srgba(ui, &mut self.shift_color, egui::color_picker::Alpha::Opaque);
                });
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.show_altgr, "Show RAlt char");
                ui.add_enabled_ui(self.show_altgr, |ui| {
                    egui::color_picker::color_edit_button_srgba(ui, &mut self.altgr_color, egui::color_picker::Alpha::Opaque);
                });
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
    }

    /// Shape-only page-1 preview: box outlines matching the real PDF's
    /// layout exactly, no labels (too small to read at this scale anyway).
    fn preview_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Preview — Page 1");
        ui.add_space(8.0);

        let Some((layout, layer_count)) = &self.preview_layout else {
            ui.centered_and_justified(|ui| ui.label("No device selected"));
            return;
        };

        let metrics = pdf::page_metrics(layout, self.layers_per_page, self.portrait, self.center_vertically);
        let avail = ui.available_size();
        let scale = (avail.x / metrics.page_w).min(avail.y / metrics.page_h);
        let (page_w_px, page_h_px) = (metrics.page_w * scale, metrics.page_h * scale);

        let (rect, _) = ui.allocate_exact_size(avail, egui::Sense::hover());
        let origin = rect.center() - egui::vec2(page_w_px, page_h_px) * 0.5;
        let painter = ui.painter_at(rect);

        // Drop shadow sells the "sheet of paper" look; flat white alone just reads as a panel.
        painter.rect_filled(
            egui::Rect::from_min_size(origin + egui::vec2(4.0, 4.0), egui::vec2(page_w_px, page_h_px)),
            0.0,
            egui::Color32::from_black_alpha(60),
        );
        painter.rect_filled(
            egui::Rect::from_min_size(origin, egui::vec2(page_w_px, page_h_px)),
            0.0,
            egui::Color32::WHITE,
        );

        // mm-space (origin bottom-left, y-up) -> screen pixels.
        let to_screen = |x: f32, y: f32| {
            let (x, y) = pdf::transform_point(x, y, &metrics);
            origin + egui::vec2(x * scale, (metrics.page_h - y) * scale)
        };

        // Mirrors draw_key()/draw_encoder()'s box geometry in pdf.rs exactly.
        let tile = |painter: &egui::Painter, x: f32, y: f32, w: f32, h: f32, block_canvas_h: f32| {
            let x0 = pdf::MARGIN_MM + x * pdf::UNIT_MM + pdf::GAP_MM * 0.5;
            let w = w * pdf::UNIT_MM - pdf::GAP_MM;
            let h = h * pdf::UNIT_MM - pdf::GAP_MM;
            let top_y_mm = block_canvas_h - pdf::MARGIN_MM - pdf::HEADER_MM - y * pdf::UNIT_MM - pdf::GAP_MM * 0.5;
            let y0 = top_y_mm - h;
            let p0 = to_screen(x0, y0 + h);
            let p1 = to_screen(x0 + w, y0);
            painter.rect(
                egui::Rect::from_two_pos(p0, p1),
                1.0,
                egui::Color32::from_gray(235),
                egui::Stroke::new(1.0, egui::Color32::from_gray(140)),
                egui::StrokeKind::Inside,
            );
        };

        let blocks = self.layers_per_page.max(1).min((*layer_count).max(1));
        for block in 0..blocks {
            let block_canvas_h = metrics.content_h - (block as f32) * metrics.block_h;
            for key in &layout.keys {
                tile(&painter, key.x, key.y, key.w, key.h, block_canvas_h);
            }
            for enc in &layout.encoders {
                tile(&painter, enc.x, enc.y, enc.w, enc.h, block_canvas_h);
            }
        }
    }
}
