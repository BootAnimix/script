fn main() {
    let state = bootanimix_gui_lib::ManagedGuiState::default();
    let snap = state
        .0
        .lock()
        .unwrap()
        .snapshot(None)
        .expect("initial studio state snapshot");
    println!(
        "BootAnimix 3.0 Studio backend ready (project='{}', parts={}, frames={})",
        snap.manifest.name,
        snap.manifest.parts.len(),
        snap.validation.performance.total_frames
    );
}
