use formats::save::SavLayout;
#[derive(Debug, Clone, Copy)]
pub struct GameplayFlags {
    pub title_extra_shown_flag: u32,
    pub ending_unlock_flags: &'static [u32],
}
#[derive(Debug, Clone, Copy)]
pub struct GameProfile {
    pub bin_name: &'static str,
    pub title: &'static str,
    pub internal_w: u32,
    pub internal_h: u32,
    pub sav: SavLayout,
    pub arc_priority: Option<&'static [&'static str]>,
    pub preload_scripts: &'static [&'static str],
    pub entry_script: &'static str,
    pub entry_label: &'static str,
    pub hot_reset_hash: u32,
    pub default_rct_key_hash: u32,
    pub gameplay_flags: Option<GameplayFlags>,
}
impl GameProfile {
    pub const OWARUSEKAI: GameProfile = GameProfile {
        bin_name: "owarusekai",
        title: "The end of the world, and happy birthday",
        internal_w: crate::gpu::INTERNAL_W,
        internal_h: crate::gpu::INTERNAL_H,
        sav: SavLayout::OWARUSEKAI,
        arc_priority: None,
        preload_scripts: &["pic", "yazlib"],
        entry_script: "start",
        entry_label: "$init@GLOBAL",
        hot_reset_hash: 0x3857_9896,
        default_rct_key_hash: 0x9CAC_E44B,
        gameplay_flags: Some(GameplayFlags {
            title_extra_shown_flag: 0xF675_D029,
            ending_unlock_flags: &[
                0x3999_6E89, 0x926D_4255, 0x748D_613E, 0x425C_1C5C, 0xE0CA_AF5B,
            ],
        }),
    };
    pub const RURI: GameProfile = GameProfile {
        bin_name: "ruri",
        title: "ルリのかさね ～いもうと物語り～",
        internal_w: 1280,
        internal_h: 720,
        sav: SavLayout::RURI,
        arc_priority: Some(
            &["update", "fastdata", "scenario", "data", "slowdata", "stream", "voice"],
        ),
        preload_scripts: &["pic", "yazlib"],
        entry_script: "start",
        entry_label: "$init@GLOBAL",
        hot_reset_hash: 0x3857_9896,
        default_rct_key_hash: 0x9CAC_E44B,
        gameplay_flags: None,
    };
    pub const PARADISE: GameProfile = GameProfile {
        bin_name: "paradise",
        title: "終わる世界と双子座のパラダイス",
        internal_w: crate::gpu::INTERNAL_W,
        internal_h: crate::gpu::INTERNAL_H,
        sav: SavLayout::FD,
        arc_priority: Some(
            &["update", "fastdata", "scenario", "data", "slowdata", "stream", "voice"],
        ),
        preload_scripts: &["pic", "yazlib"],
        entry_script: "start",
        entry_label: "$init@GLOBAL",
        hot_reset_hash: 0x3857_9896,
        default_rct_key_hash: 0x9CAC_E44B,
        gameplay_flags: None,
    };
}
