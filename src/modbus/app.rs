
pub struct ModbusTool {
    // TODO
}

impl ModbusTool {
    pub fn new() -> Self {
        ModbusTool {

        }
    }
    
    pub fn views(&mut self, ctx: &egui::Context, _ui: &mut egui::Ui) {
        ctx.set_visuals(egui::Visuals::light());
    }
}