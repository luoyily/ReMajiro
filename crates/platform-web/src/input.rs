pub fn keyboard_code_to_vk(code: &str) -> Option<u8> {
    Some(
        match code {
            "Backspace" => 0x08,
            "Tab" => 0x09,
            "Enter" | "NumpadEnter" => 0x0D,
            "ShiftLeft" | "ShiftRight" => 0x10,
            "ControlLeft" | "ControlRight" => 0x11,
            "AltLeft" | "AltRight" => 0x12,
            "Pause" => 0x13,
            "CapsLock" => 0x14,
            "Escape" => 0x1B,
            "Space" => 0x20,
            "PageUp" => 0x21,
            "PageDown" => 0x22,
            "End" => 0x23,
            "Home" => 0x24,
            "ArrowLeft" => 0x25,
            "ArrowUp" => 0x26,
            "ArrowRight" => 0x27,
            "ArrowDown" => 0x28,
            "PrintScreen" => 0x2C,
            "Insert" => 0x2D,
            "Delete" => 0x2E,
            "Help" => 0x2F,
            "MetaLeft" => 0x5B,
            "MetaRight" => 0x5C,
            "ContextMenu" => 0x5D,
            "NumpadMultiply" => 0x6A,
            "NumpadAdd" => 0x6B,
            "NumpadComma" => 0x6C,
            "NumpadSubtract" => 0x6D,
            "NumpadDecimal" => 0x6E,
            "NumpadDivide" => 0x6F,
            "NumLock" => 0x90,
            "ScrollLock" => 0x91,
            "NumpadEqual" => 0x92,
            "Semicolon" => 0xBA,
            "Equal" => 0xBB,
            "Comma" => 0xBC,
            "Minus" => 0xBD,
            "Period" => 0xBE,
            "Slash" => 0xBF,
            "Backquote" => 0xC0,
            "BracketLeft" => 0xDB,
            "Backslash" => 0xDC,
            "BracketRight" => 0xDD,
            "Quote" => 0xDE,
            "NumpadBackspace" => 0x08,
            "NumpadClear" => 0x0C,
            code if is_digit_code(code) => {
                0x30 + (code.as_bytes()[code.len() - 1] - b'0')
            }
            code if is_letter_code(code) => {
                0x41 + (code.as_bytes()[code.len() - 1] - b'A')
            }
            code if is_numpad_digit_code(code) => {
                0x60 + (code.as_bytes()[code.len() - 1] - b'0')
            }
            code if function_key_number(code).is_some() => {
                0x6F + function_key_number(code).unwrap()
            }
            _ => return None,
        },
    )
}
pub fn wheel_delta_to_vk(delta_y: f64) -> Option<u8> {
    match (-delta_y).partial_cmp(&0.0) {
        Some(std::cmp::Ordering::Greater) => Some(0x21),
        Some(std::cmp::Ordering::Less) => Some(0x22),
        _ => None,
    }
}
fn is_digit_code(code: &str) -> bool {
    code.len() == 6 && code.starts_with("Digit") && code.as_bytes()[5].is_ascii_digit()
}
fn is_letter_code(code: &str) -> bool {
    code.len() == 4 && code.starts_with("Key") && code.as_bytes()[3].is_ascii_uppercase()
}
fn is_numpad_digit_code(code: &str) -> bool {
    code.len() == 7 && code.starts_with("Numpad") && code.as_bytes()[6].is_ascii_digit()
}
fn function_key_number(code: &str) -> Option<u8> {
    let number = code.strip_prefix('F')?.parse::<u8>().ok()?;
    (1..=24).contains(&number).then_some(number)
}

