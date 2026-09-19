// Trimmed from keypeek's protocols/mod.rs (Key/EncoderTile/KeyboardLayout/
// KeyboardDefinition) and settings.rs (DisplayLayout) — copied, not linked,
// so keypeek's own build stays untouched.

pub type Row = usize;
pub type Column = usize;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Key {
    pub row: Row,
    pub col: Column,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Rotation angle in degrees, clockwise around the key's center.
    #[serde(default)]
    pub r: f32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EncoderTile {
    pub id: u8,
    pub clockwise: bool,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub r: f32,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct KeyboardLayout {
    pub name: String,
    pub keys: Vec<Key>,
    #[serde(default)]
    pub encoders: Vec<EncoderTile>,
}

impl KeyboardLayout {
    pub fn get_dimensions(&self) -> (f32, f32) {
        let max_x = self
            .keys
            .iter()
            .map(|k| k.x + k.w)
            .chain(self.encoders.iter().map(|e| e.x + e.w))
            .fold(0.0, f32::max);
        let max_y = self
            .keys
            .iter()
            .map(|k| k.y + k.h)
            .chain(self.encoders.iter().map(|e| e.y + e.h))
            .fold(0.0, f32::max);
        (max_x, max_y)
    }

    pub fn encoder_count(&self) -> usize {
        self.encoders.iter().map(|e| e.id as usize + 1).max().unwrap_or(0)
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct KeyboardDefinition {
    pub vid: u16,
    pub pid: u16,
    pub rows: usize,
    pub cols: usize,
    pub layouts: Vec<KeyboardLayout>,
}
