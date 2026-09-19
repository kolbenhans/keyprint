use crate::layout_key::LayoutKey;
use crate::os_layout;

pub struct ResolvedLabel {
    pub full: String,
    pub short: Option<String>,
}

/// HID usages 0x04..=0x1D are A-Z.
fn is_letter(usage: u16) -> bool {
    (0x04..=0x1D).contains(&usage)
}

/// Ported from keypeek's `apply_os_overrides`: only letters/`shifted` keys trust the OS query.
fn os_override_allowed(kc: u16) -> bool {
    is_letter(kc)
        || crate::keycodes::keycode_label::get_layout_key(kc).is_some_and(|k| k.shifted.is_some())
}

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
        .filter(|&kc| is_letter(kc) || key.shifted.is_some())
        .and_then(|kc| os_layout::base_char(kc).map(|s| if is_letter(kc) { s.to_uppercase() } else { s }))
        .unwrap_or_else(|| {
            if combo_keycode.is_some() {
                raw_base_label()
            } else {
                key.tap.full.clone()
            }
        });

    match resolve_combo(key) {
        // Resolved combo is always a single char — no shorter fallback needed.
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

/// Second/third character for the optional shift/AltGr keycap corners.
/// `None` for keys that already resolve to a single combo character (S(KC_x)
/// etc.) — those show their result as the main label, a corner would just
/// repeat it.
fn is_combo_key(key: &LayoutKey) -> bool {
    key.shift_base.is_some() || key.altgr_base.is_some() || key.shift_altgr_base.is_some()
}

pub fn resolve_shifted_corner(key: &LayoutKey) -> Option<String> {
    // A letter's shifted form is just its own uppercase — no info a corner
    // adds, so plain A-Z keys get no shift corner at all.
    if is_combo_key(key) || key.base_keycode.is_some_and(is_letter) {
        return None;
    }
    key.base_keycode
        .and_then(os_layout::shifted_char)
        .or_else(|| key.shifted.clone())
}

pub fn resolve_altgr_corner(key: &LayoutKey) -> Option<String> {
    if is_combo_key(key) {
        return None;
    }
    key.base_keycode.and_then(os_layout::ralt_char)
}

fn resolve_combo(key: &LayoutKey) -> Option<String> {
    let shift_altgr_resolved = key
        .shift_altgr_base
        .filter(|&kc| os_override_allowed(kc))
        .and_then(os_layout::ralt_shifted_char);
    let altgr_resolved = key
        .altgr_base
        .filter(|&kc| os_override_allowed(kc))
        .and_then(os_layout::ralt_char);
    let shift_resolved = key.shift_base.and_then(|kc| {
        let os_result = os_override_allowed(kc).then(|| os_layout::shifted_char(kc)).flatten();
        os_result.or_else(|| crate::keycodes::keycode_label::get_layout_key(kc).and_then(|k| k.shifted))
    });
    shift_altgr_resolved.or(altgr_resolved).or(shift_resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout_key::LayoutKey;

    /// 0xFFFF is out of range for every platform's HID->keycode table, so
    /// `os_layout` always returns `None` for it regardless of the live
    /// session — keeping this test deterministic.
    #[test]
    fn shifted_corner_falls_back_to_static_table() {
        let key = LayoutKey {
            base_keycode: Some(0xFFFF),
            shifted: Some("!".to_string()),
            ..Default::default()
        };
        assert_eq!(resolve_shifted_corner(&key), Some("!".to_string()));
    }

    #[test]
    fn shifted_corner_is_none_for_letters() {
        // HID usage 0x04 = KC_A.
        let key = LayoutKey {
            base_keycode: Some(0x04),
            ..Default::default()
        };
        assert_eq!(resolve_shifted_corner(&key), None);
    }

    #[test]
    fn corners_are_none_for_explicit_combo_keys() {
        let key = LayoutKey {
            base_keycode: Some(0xFFFF),
            shifted: Some("!".to_string()),
            shift_base: Some(0x1e),
            ..Default::default()
        };
        assert_eq!(resolve_shifted_corner(&key), None);
        assert_eq!(resolve_altgr_corner(&key), None);
    }

    /// S(A(KC_E)) -> Shift+AltGr+E should resolve via the OS layout (e.g. Polish "Ę"). Needs a live session.
    #[test]
    #[ignore]
    fn shift_altgr_combo_resolves_via_os_layout() {
        // mod_mask=LSFT|LALT|RIGHT_FLAG=0x16, shifted into bits 8-12, | KC_E(0x08).
        let shift_altgr_e: u16 = (0x16 << 8) | 0x08;
        let key = crate::keycodes::keycode_label::get_layout_key(shift_altgr_e)
            .expect("mod-combo keycode should resolve to a LayoutKey");
        assert_eq!(key.shift_altgr_base, Some(0x08));

        let resolved = resolve_label(&key);
        assert_eq!(resolved.full, "Ę");
    }
}
