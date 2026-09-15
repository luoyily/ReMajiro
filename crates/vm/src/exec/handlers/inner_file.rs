use crate::host::Host;
use crate::value::Value;
use super::inner::InnerOutcome;
pub(super) const HASHES: &[u32] = &[
    0x2539D212, 0x55232561, 0x6008BEBB, 0xD3C06820, 0xD6333B3C, 0xDCA7DA63, 0xE4E3B40D,
    0xE8A6A59F,
];
impl crate::exec::Vm {
    pub(super) fn handle_inner_file<H: Host>(
        &mut self,
        hash: u32,
        count: usize,
        _has_retval: bool,
        host: &mut H,
        args: &[Value],
    ) -> Option<Result<InnerOutcome, crate::exec::VmError>> {
        match hash {
            0xD3C06820 => {
                let name = top_str(args, count, 0);
                let exists = host.file_exists(name);
                eprintln!(
                    "[FILE] exists {:?} → {}", String::from_utf8_lossy(name), exists
                );
                self.stack.push(Value::int(if exists { 1 } else { 0 }));
                Some(Ok(InnerOutcome::Normal))
            }
            0xE4E3B40D => {
                let name = top_str(args, count, 0);
                let handle = host.file_open(name);
                self.stack.push(Value::int(handle as i32));
                Some(Ok(InnerOutcome::Normal))
            }
            0x55232561 => {
                let handle = top_int(args, count, 0).max(0) as u32;
                let token = host.file_readline(handle).unwrap_or_default();
                self.stack.push(Value::string(token));
                Some(Ok(InnerOutcome::Normal))
            }
            0x2539D212 => {
                let handle = top_int(args, count, 0).max(0) as u32;
                let token = host.file_readline(handle).unwrap_or_default();
                let n = atoi_like(&token);
                self.stack.push(Value::int(n));
                Some(Ok(InnerOutcome::Normal))
            }
            0xE8A6A59F => {
                let handle = top_int(args, count, 0).max(0) as u32;
                let line = host.file_readline_raw(handle).unwrap_or_default();
                self.stack.push(Value::string(line));
                Some(Ok(InnerOutcome::Normal))
            }
            0x6008BEBB => {
                let handle = top_int(args, count, 0).max(0) as u32;
                let line = host.file_readline_raw(handle).unwrap_or_default();
                self.stack.push(Value::string(line));
                Some(Ok(InnerOutcome::Normal))
            }
            0xDCA7DA63 => {
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            0xD6333B3C => {
                let handle = top_int(args, count, 0).max(0) as u32;
                host.file_close(handle);
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            _ => None,
        }
    }
}
fn from_top(args: &[Value], count: usize, n: usize) -> Option<&Value> {
    let real = count.min(args.len());
    if n >= real {
        return None;
    }
    args.get(real - 1 - n)
}
fn top_int(args: &[Value], count: usize, n: usize) -> i32 {
    from_top(args, count, n).and_then(|v| v.as_int()).unwrap_or(0)
}
fn top_str(args: &[Value], count: usize, n: usize) -> &[u8] {
    from_top(args, count, n).and_then(|v| v.as_str_bytes()).unwrap_or(&[])
}
fn atoi_like(s: &[u8]) -> i32 {
    let mut i = 0;
    while i < s.len() && s[i].is_ascii_whitespace() {
        i += 1;
    }
    let mut sign: i32 = 1;
    if i < s.len() && (s[i] == b'+' || s[i] == b'-') {
        if s[i] == b'-' {
            sign = -1;
        }
        i += 1;
    }
    let mut acc: i64 = 0;
    while i < s.len() && s[i].is_ascii_digit() {
        acc = acc * 10 + (s[i] - b'0') as i64;
        if acc > i32::MAX as i64 && sign > 0 {
            return i32::MAX;
        }
        if acc > (i32::MAX as i64) + 1 && sign < 0 {
            return i32::MIN;
        }
        i += 1;
    }
    (sign as i64 * acc) as i32
}

