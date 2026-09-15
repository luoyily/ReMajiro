use crate::exec::VmError;
use crate::value::{Value, ValueData};
use super::helpers::InnerArgs;
use super::inner::InnerOutcome;
pub(super) const HASHES: &[u32] = &[0x23D5D11D, 0xEBB5886C];
impl crate::exec::Vm {
    pub(super) fn handle_inner_gameplay(
        &mut self,
        hash: u32,
        count: usize,
        args: &[Value],
    ) -> Option<Result<InnerOutcome, VmError>> {
        match hash {
            0x23D5D11D => {
                let player = InnerArgs::new(args, count).int_or(0, 0);
                let paths = mahjong_winning_path_count(self, player);
                self.stack.push(Value::int(paths));
                Some(Ok(InnerOutcome::Normal))
            }
            0xEBB5886C => {
                sort_mahjong_hand(self, InnerArgs::new(args, count).int_or(0, 0));
                self.stack.push(Value::int(0));
                Some(Ok(InnerOutcome::Normal))
            }
            _ => None,
        }
    }
}
fn global_array_cells<'a>(vm: &'a crate::exec::Vm, name: &[u8]) -> Option<&'a [Value]> {
    let hash = crate::formats_crc32(name);
    vm.global_keys
        .get(&hash)
        .and_then(|slot| vm.globals.get(*slot))
        .and_then(|value| value.data.as_deref())
        .and_then(|data| match data {
            ValueData::IntArray { cells, .. } => Some(cells.as_slice()),
            _ => None,
        })
}
fn mahjong_winning_path_count(vm: &crate::exec::Vm, player: i32) -> i32 {
    let (hand_name, excluded_name, base) = if player == 3 {
        (
            b"@M_my_pre_te#@GLOBAL".as_slice(),
            b"@M_my_naki_flag2#@GLOBAL".as_slice(),
            0_usize,
        )
    } else {
        let Ok(player) = usize::try_from(player) else {
            return 0;
        };
        if player >= 3 {
            return 0;
        }
        (
            b"@M_com_pre_te#@GLOBAL".as_slice(),
            b"@M_com_naki_flag2#@GLOBAL".as_slice(),
            player * 14,
        )
    };
    let Some(hand) = global_array_cells(vm, hand_name) else {
        return 0;
    };
    let excluded = global_array_cells(vm, excluded_name);
    let mut counts = [0_i32; 40];
    for offset in 0..14 {
        if excluded
            .and_then(|flags| flags.get(base + offset))
            .and_then(Value::as_int)
            .unwrap_or(0) != 0
        {
            continue;
        }
        let tile = hand.get(base + offset).and_then(Value::as_int).unwrap_or(-1);
        if (0..400).contains(&tile) {
            counts[(tile / 10) as usize] += 1;
        }
    }
    let mut paths = 0;
    for tile_type in 0..40 {
        if !native_tile_type_is_valid(tile_type) || counts[tile_type] == 4 {
            continue;
        }
        counts[tile_type] += 1;
        if native_hand_is_complete(&counts) {
            paths += 1;
        }
        counts[tile_type] -= 1;
    }
    paths
}
fn native_tile_type_is_valid(tile_type: usize) -> bool {
    matches!(tile_type, 1..= 9 | 11..= 19 | 21..= 29 | 31..= 37)
}
fn native_hand_is_complete(counts: &[i32; 40]) -> bool {
    const ORPHANS: [usize; 13] = [1, 9, 11, 19, 21, 29, 31, 32, 33, 34, 35, 36, 37];
    let orphan_total: i32 = ORPHANS.iter().map(|&tile| counts[tile]).sum();
    if orphan_total == 14 && ORPHANS.iter().all(|&tile| counts[tile] != 0) {
        return true;
    }
    let pairs: Vec<usize> = (1..40).filter(|&tile| counts[tile] >= 2).collect();
    if pairs.len() >= 7 {
        return true;
    }
    for pair in pairs {
        let mut remaining = *counts;
        remaining[pair] -= 2;
        let mut valid = true;
        for tile in 1..30 {
            let amount = remaining[tile];
            match amount {
                1 | 2 => {
                    remaining[tile] -= amount;
                    remaining[tile + 1] -= amount;
                    remaining[tile + 2] -= amount;
                    if remaining[tile] < 0 || remaining[tile + 1] < 0
                        || remaining[tile + 2] < 0
                    {
                        valid = false;
                        break;
                    }
                }
                3 => remaining[tile] -= 3,
                4 => {
                    remaining[tile] -= 4;
                    remaining[tile + 1] -= 1;
                    remaining[tile + 2] -= 1;
                    if remaining[tile + 1] < 0 || remaining[tile + 2] < 0 {
                        valid = false;
                        break;
                    }
                }
                _ => {}
            }
        }
        if valid && (31..38).all(|tile| matches!(remaining[tile], 0 | 3)) {
            return true;
        }
    }
    false
}
fn sort_mahjong_hand(vm: &mut crate::exec::Vm, player: i32) {
    let (flags_name, hand_name, base) = if player == 3 {
        (
            b"@M_my_naki_flag#@GLOBAL".as_slice(),
            b"@M_my_pre_te#@GLOBAL".as_slice(),
            0_usize,
        )
    } else {
        let Ok(player) = usize::try_from(player) else {
            return;
        };
        if player >= 3 {
            return;
        }
        (
            b"@M_com_naki_flag#@GLOBAL".as_slice(),
            b"@M_com_pre_te#@GLOBAL".as_slice(),
            player * 14,
        )
    };
    let flags_hash = crate::formats_crc32(flags_name);
    let hand_hash = crate::formats_crc32(hand_name);
    let reserved = vm
        .global_keys
        .get(&flags_hash)
        .and_then(|slot| vm.globals.get(*slot))
        .and_then(|value| value.data.as_deref())
        .and_then(|data| match data {
            ValueData::IntArray { cells, .. } => Some(cells),
            _ => None,
        })
        .map(|flags| {
            let mut offset = 0_usize;
            while offset < 14
                && flags.get(base + offset).and_then(Value::as_int).unwrap_or(0) != 0
            {
                offset += 3;
            }
            offset.min(14)
        })
        .unwrap_or(0);
    let Some(slot) = vm.global_keys.get(&hand_hash).copied() else {
        return;
    };
    let Some(hand) = vm.globals.get_mut(slot) else {
        return;
    };
    let Some(data) = hand.data.as_mut() else {
        return;
    };
    let ValueData::IntArray { cells, .. } = std::rc::Rc::make_mut(data) else {
        return;
    };
    let start = base.saturating_add(reserved);
    let end = base.saturating_add(14).min(cells.len());
    if start < end {
        cells[start..end]
            .sort_by_key(|value| value.as_int().unwrap_or(value.bits as i32));
    }
}

