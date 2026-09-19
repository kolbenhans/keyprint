// Trimmed from keypeek's protocols/vial.rs — dropped the KeyboardProtocol
// trait indirection and live-subscription bits (RawHidSubscription/Reopener)
// since a one-shot PDF export doesn't need them. Wire protocol is identical.

use crate::keycodes::keycode_label::get_layout_key;
use crate::kle_parser;
use crate::layout_key::LayoutKey;
use crate::types::KeyboardDefinition;
use qmk_via_api::api::{KeyboardApi, MatrixInfo};
use std::error::Error;

const VIAL_PREFIX: u8 = 0xFE;

#[repr(u8)]
enum VialCommand {
    KeyboardId = 0x00,
    Size = 0x01,
    Def = 0x02,
    GetEncoder = 0x03,
}

pub struct VialProtocol {
    api: KeyboardApi,
    definition: KeyboardDefinition,
}

impl VialProtocol {
    pub fn connect(vid: u16, pid: u16) -> Result<Self, Box<dyn Error>> {
        let api = KeyboardApi::new(vid, pid, 0xff60, None)
            .map_err(|e| format!("Failed to connect to device ({vid:04x}:{pid:04x}): {e}"))?;

        let (protocol_version, _uid) = Self::get_keyboard_id(&api)?;
        if protocol_version == 0 {
            return Err("Device does not support VIAL protocol".into());
        }

        let definition = Self::fetch_definition(&api, vid, pid)?;
        Ok(Self { api, definition })
    }

    pub fn definition(&self) -> &KeyboardDefinition {
        &self.definition
    }

    pub fn get_layer_count(&self) -> Result<usize, Box<dyn Error>> {
        let count = self
            .api
            .get_layer_count()
            .map_err(|e| format!("Failed to get layer count: {e}"))?;
        Ok(count as usize)
    }

    pub fn read_all_keys(
        &self,
        layers: usize,
        rows: usize,
        cols: usize,
    ) -> Vec<Vec<Vec<Option<LayoutKey>>>> {
        let mut keys = vec![vec![vec![None; cols]; rows]; layers];
        let matrix_info = MatrixInfo {
            rows: rows as u8,
            cols: cols as u8,
        };

        for (layer, layer_keys) in keys.iter_mut().enumerate().take(layers) {
            // HID reads occasionally hiccup transiently, especially several
            // layers into a read-everything pass — retry a few times before
            // giving up and leaving the layer blank.
            let mut result = self.api.read_raw_matrix(matrix_info, layer as u8);
            for attempt in 1..3 {
                if result.is_ok() {
                    break;
                }
                eprintln!(
                    "  warning: layer {layer} raw matrix read failed (attempt {attempt}): {:?}",
                    result.as_ref().err()
                );
                std::thread::sleep(std::time::Duration::from_millis(50));
                result = self.api.read_raw_matrix(matrix_info, layer as u8);
            }

            if let Ok(raw_matrix) = result {
                for (i, &keycode) in raw_matrix.iter().enumerate() {
                    let row = i / cols;
                    let col = i % cols;
                    layer_keys[row][col] = get_layout_key(keycode);
                }
            } else {
                eprintln!(
                    "  error: layer {layer} raw matrix read failed after retries: {:?}",
                    result.err()
                );
            }
        }

        keys
    }

    pub fn read_all_encoders(
        &self,
        layers: usize,
        encoder_count: usize,
    ) -> Vec<Vec<(Option<LayoutKey>, Option<LayoutKey>)>> {
        (0..layers)
            .map(|layer| {
                (0..encoder_count)
                    .map(|id| {
                        let Ok(response) =
                            Self::vial_command(&self.api, VialCommand::GetEncoder, &[layer as u8, id as u8])
                        else {
                            return (None, None);
                        };
                        let ccw = u16::from_be_bytes([response[0], response[1]]);
                        let cw = u16::from_be_bytes([response[2], response[3]]);
                        (get_layout_key(ccw), get_layout_key(cw))
                    })
                    .collect()
            })
            .collect()
    }

    fn vial_command(
        api: &KeyboardApi,
        cmd: VialCommand,
        data: &[u8],
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        let mut msg = vec![0u8; 32];
        msg[0] = VIAL_PREFIX;
        msg[1] = cmd as u8;

        let copy_len = data.len().min(30);
        msg[2..2 + copy_len].copy_from_slice(&data[..copy_len]);

        api.hid_send(msg)
            .map_err(|e| format!("VIAL write error: {e}"))?;

        api.hid_read()
            .map_err(|e| format!("VIAL read error: {e}").into())
    }

    fn vial_get_def_block(api: &KeyboardApi, block: u32) -> Result<Vec<u8>, Box<dyn Error>> {
        let mut msg = vec![0u8; 32];
        msg[0] = VIAL_PREFIX;
        msg[1] = VialCommand::Def as u8;
        msg[2..6].copy_from_slice(&block.to_le_bytes());

        api.hid_send(msg)
            .map_err(|e| format!("VIAL write error: {e}"))?;

        api.hid_read()
            .map_err(|e| format!("VIAL read error: {e}").into())
    }

    fn get_keyboard_id(api: &KeyboardApi) -> Result<(u32, [u8; 8]), Box<dyn Error>> {
        let response = Self::vial_command(api, VialCommand::KeyboardId, &[])?;

        let protocol_version =
            u32::from_le_bytes([response[0], response[1], response[2], response[3]]);

        let mut uid = [0u8; 8];
        uid.copy_from_slice(&response[4..12]);

        Ok((protocol_version, uid))
    }

    fn get_definition_size(api: &KeyboardApi) -> Result<u32, Box<dyn Error>> {
        let response = Self::vial_command(api, VialCommand::Size, &[])?;
        let size = u32::from_le_bytes([response[0], response[1], response[2], response[3]]);
        Ok(size)
    }

    fn fetch_definition(
        api: &KeyboardApi,
        vid: u16,
        pid: u16,
    ) -> Result<KeyboardDefinition, Box<dyn Error>> {
        let size = Self::get_definition_size(api)? as usize;
        if size == 0 {
            return Err("VIAL definition size is 0".into());
        }

        let mut compressed = Vec::with_capacity(size);
        let mut block: u32 = 0;

        while compressed.len() < size {
            let response = Self::vial_get_def_block(api, block)?;
            let remaining = size - compressed.len();
            let chunk_size = remaining.min(32);
            compressed.extend_from_slice(&response[..chunk_size]);
            block += 1;
        }

        let mut decompressed = Vec::new();
        {
            let mut cursor = std::io::Cursor::new(&compressed);
            lzma_rs::xz_decompress(&mut cursor, &mut decompressed)
                .map_err(|e| format!("Failed to decompress VIAL definition: {e}"))?;
        }

        let json_str = String::from_utf8(decompressed)
            .map_err(|e| format!("VIAL definition is not valid UTF-8: {e}"))?;
        let json: serde_json::Value = serde_json::from_str(&json_str)
            .map_err(|e| format!("Failed to parse VIAL definition JSON: {e}"))?;

        kle_parser::parse_vial_definition(&json, vid, pid)
    }
}
