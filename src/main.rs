#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod app;
mod diff;
mod git;
mod graph;
mod settings;
mod tasks;
mod theme;
mod timing;
mod ui;
mod ui_branches;
mod ui_dialogs;
mod ui_diff;
mod ui_history;
mod ui_inspector;
mod ui_workbook;
#[cfg(windows)]
mod window_chrome;
#[cfg(windows)]
mod workbook_worker;

use eframe::egui;
use std::path::PathBuf;

fn main() -> eframe::Result<()> {
    let started = std::time::Instant::now();
    let mut paths = Vec::new();
    let mut import = false;
    let mut arguments = std::env::args_os().skip(1).peekable();
    #[cfg(windows)]
    if arguments
        .peek()
        .is_some_and(|argument| argument == workbook_worker::ARGUMENT)
    {
        std::process::exit(i32::from(workbook_worker::run().is_err()));
    }
    while let Some(argument) = arguments.next() {
        if argument == "--repo" {
            if let Some(path) = arguments.next() {
                paths.push(PathBuf::from(path));
            }
        } else if argument == "--import-smartgit" {
            import = true;
        } else if !argument.to_string_lossy().starts_with('-') {
            paths.push(PathBuf::from(argument));
        }
    }
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/refdesk.png"))
        .expect("embedded Refdesk icon must be a valid PNG");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_icon(icon)
            .with_inner_size(timing::capture_viewport().unwrap_or([1480.0, 850.0]))
            .with_position([56.0, 62.0])
            .with_min_inner_size([930.0, 610.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    if let Err(error) = eframe::run_native(
        "Refdesk · Git",
        options,
        Box::new(move |context| {
            ui::install_style(&context.egui_ctx);
            Ok(Box::new(app::Desk::new(context, paths, import, started)))
        }),
    ) {
        let _ = rfd::MessageDialog::new()
            .set_title("Refdesk 无法启动")
            .set_description(error.to_string())
            .show();
        return Err(error);
    }
    Ok(())
}
