// Trimmed from keypeek's overlay_window/ui_overlay.rs (generate_tap_galleys /
// resolve_combo) — reproduces "Single" legend mode for a static print instead
// of a live overlay: no shift/altgr-held branches (nothing is being held),
// no Dual-mode stacking, no icon-font symbol handling (dropped upstream in
// layout_key.rs/keycodes/basic.rs already).
//
// Translation now comes from keypeek's os_layout module (the OS's *active*
// keyboard layout, queried live) instead of a static per-language JSON
// table — same seam fields (base_keycode/shift_base/altgr_base/
// shift_altgr_base) the keycodes/*.rs builders already attach to a
// LayoutKey, just resolved against a different backend.

use crate::layout_key::LayoutKey;
use crate::os_layout;

/// Main label text plus an optional shorter fallback for when `full` doesn't
/// fit the keycap box.
pub struct ResolvedLabel {
    pub full: String,
    pub short: Option<String>,
}

/// Final single-line label for a key under the OS's active keyboard layout,
/// mirroring keypeek's "Single" legend mode: a key programmed as e.g.
/// `S(KC_8)` resolves straight to "(" on a German layout, a plain `KC_Y`
/// resolves to "Z" (physical position swap), anything the OS query can't
/// answer (unsupported platform, no session) falls back to the raw US label.
pub fn resolve_label(key: &LayoutKey) -> ResolvedLabel {
    let combo_keycode = key.shift_altgr_base.or(key.shift_base).or(key.altgr_base);

    let raw_base_label = || {
        combo_keycode
            .and_then(|kc| crate::keycodes::keycode_label::get_layout_key(kc))
            .map(|k| k.tap.full)
            .unwrap_or_else(|| key.tap.full.clone())
    };

    let tap_full = key
        .base_keycode
        .and_then(os_layout::base_char)
        .unwrap_or_else(|| {
            if combo_keycode.is_some() {
                raw_base_label()
            } else {
                key.tap.full.clone()
            }
        });

    match resolve_combo(key) {
        // A resolved combo (e.g. "(" for S(KC_8)) is always a single
        // character — no shorter fallback needed.
        Some(ch) => ResolvedLabel {
            full: ch,
            short: None,
        },
        None => ResolvedLabel {
            full: tap_full,
            short: key.tap.short.clone(),
        },
    }
}

fn resolve_combo(key: &LayoutKey) -> Option<String> {
    let shift_altgr_resolved = key.shift_altgr_base.and_then(os_layout::ralt_shifted_char);
    let altgr_resolved = key.altgr_base.and_then(os_layout::ralt_char);
    let shift_resolved = key.shift_base.and_then(|kc| {
        os_layout::shifted_char(kc)
            .or_else(|| crate::keycodes::keycode_label::get_layout_key(kc).and_then(|k| k.shifted))
    });
    shift_altgr_resolved.or(altgr_resolved).or(shift_resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// S(A(KC_E)) -> Shift+AltGr+E should resolve to the layout's Shift+AltGr
    /// combo (e.g. Polish "Ę"), not fall through to plain "E" or get misfiled
    /// as lone-shift/lone-altgr. Needs a live session on a layout that
    /// defines a Shift+AltGr combo on E, so it's not part of the normal
    /// `cargo test` run — see keypeek's os_layout tests for the same pattern.
    #[test]
    #[ignore]
    fn shift_altgr_combo_resolves_via_os_layout() {
        // (mod_mask=LSFT|LALT|RIGHT_FLAG=0x16) << 8 | KC_E(0x08). QK_MODS's
        // bits 8-12 *are* the mod flags (LCTL=bit8) — no separate marker.
        let shift_altgr_e: u16 = (0x16 << 8) | 0x08;
        let key = crate::keycodes::keycode_label::get_layout_key(shift_altgr_e)
            .expect("mod-combo keycode should resolve to a LayoutKey");
        assert_eq!(key.shift_altgr_base, Some(0x08));

        let resolved = resolve_label(&key);
        assert_eq!(resolved.full, "Ę");
    }
}
