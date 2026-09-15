pub fn lookup_inner_opcode(hash: u32) -> Option<&'static str> {
    INNER_VM_TABLE
        .binary_search_by_key(&hash, |(h, _, _)| *h)
        .ok()
        .map(|i| INNER_VM_TABLE[i].2)
}
pub fn is_valid_outer_opcode(opcode: u16) -> bool {
    matches!(
        opcode, 256 | 257 | 264 | 265 | 272 | 280 | 281 | 282 | 288 | 289 | 296 | 304 |
        312 | 313 | 314 | 320 | 321 | 322 | 328 | 329 | 330 | 336 | 337 | 338 | 344 | 345
        | 346 | 347 | 348 | 349 | 352 | 353 | 354 | 355 | 356 | 357 | 360 | 368 | 376 |
        384 | 392 | 400 | 401 | 408 | 416 | 417 | 424 | 425 | 432 | 433 | 434 | 435 | 436
        | 437 | 440 | 441 | 448 | 449 | 456 | 464 | 465 | 466 | 472 | 473 | 480 | 488 |
        496 | 504 | 512 | 528 | 529 | 530 | 531 | 532 | 533 | 536 | 537 | 544 | 545 | 552
        | 560 | 562 | 568 | 569 | 576 | 584 | 592 | 600 | 608 | 624 | 625 | 626 | 632 |
        633 | 640 | 641 | 648 | 656 | 657 | 658 | 664 | 665 | 672 | 680 | 688 | 696 | 704
        | 720 | 721 | 722 | 728 | 729 | 736 | 737 | 744 | 752 | 753 | 754 | 760 | 761 |
        768 | 2048 | 2049 | 2050 | 2051 | 2063 | 2064 | 2089 | 2091 | 2092 | 2093 | 2094
        | 2095 | 2096 | 2097 | 2098 | 2099 | 2100 | 2101 | 2102 | 2103 | 2104 | 2105 |
        2106 | 2107 | 2108 | 2109 | 2110 | 2111 | 2112 | 2113 | 2114 | 2115 | 2116 | 2117
        | 2118 | 2119 | 2128
    )
}
pub fn outer_opcode_name(opcode: u16) -> &'static str {
    match opcode {
        2048 => "PUSH_INT",
        2049 => "PUSH_STR",
        2050 => "PUSH_OP",
        2051 => "PUSH_FLOAT",
        2063 => "CALL_GLOBAL",
        2064 => "CALL_LOCAL",
        2089 => "PUSH_BYTES",
        2091 => "RET",
        2092 => "JUMP",
        2093 => "JUMP_IF",
        2094 => "JUMP_IFNOT",
        2095 => "POP",
        2096 => "MARK_JUMP",
        2097 => "JUMP_IF_NE_SYSVAR",
        2098 => "JUMP_IF_LE_SYSVAR",
        2099 => "JUMP_IF_GE_SYSVAR",
        2100 => "INNER_VM0",
        2101 => "INNER_VM1",
        2102 => "SYS_02",
        2103 => "SYS_03",
        2104 => "SYS_04",
        2105 => "SYS_05",
        2106 => "SYS_06",
        2107 => "SYS_07",
        2108 => "SYS_08",
        2109 => "SYS_09",
        2110 => "SYS_0A",
        2111 => "SYS_0B",
        2112 => "TEXT_LINE",
        2113 => "TEXT_RENDER",
        2114 => "SYS_0E",
        2115 => "SYS_0F",
        2116 => "FRAME_RESET",
        2117 => "SET_JUMP",
        2118 => "SYS_12",
        2119 => "SYS_13",
        2128 => "SYS_18",
        401 => "NOT_F",
        256 => "IMUL",
        257 => "FMUL",
        264 => "IDIV",
        265 => "FDIV",
        272 => "IMOD",
        280 => "IADD",
        281 => "FADD",
        282 => "STRCAT",
        288 => "ISUB",
        289 => "FSUB",
        296 => "ISAR",
        304 => "ISHL",
        312 => "ICMP_LE",
        313 => "FCMP_LE",
        314 => "STRCMP_LE",
        320 => "ICMP_LT",
        321 => "FCMP_LT",
        322 => "STRCMP_LT",
        328 => "ICMP_GE",
        329 => "FCMP_GE",
        330 => "STRCMP_GE",
        336 => "ICMP_GT",
        337 => "FCMP_GT",
        338 => "STRCMP_GT",
        344 => "ICMP_EQ",
        345 => "FCMP_EQ",
        346 => "STRCMP_EQ",
        347 => "ICMP_EQ_POP",
        348 => "ICMP_EQ_POP",
        349 => "ICMP_EQ_POP",
        352 => "ICMP_NE",
        353 => "FCMP_NE",
        354 => "STRCMP_NE",
        355 => "ICMP_NE_POP",
        356 => "ICMP_NE_POP",
        357 => "ICMP_NE_POP",
        360 => "IXOR",
        368 => "LAND",
        376 => "LOR",
        384 => "IAND",
        392 => "IOR",
        400 => "BOOL_NOT",
        408 => "BIT_NOT",
        416 => "NEG_INT",
        417 => "NEG_FLOAT",
        432 => "SET",
        433 => "SETF",
        440 => "MUL",
        441 => "MULF",
        448 => "DIV",
        449 => "DIVF",
        456 => "MOD",
        464 => "ADD",
        465 => "ADDF",
        466 => "CONCAT",
        472 => "SUB",
        473 => "SUBF",
        480 => "SHL",
        488 => "SHR",
        496 => "AND",
        504 => "XOR",
        512 => "OR",
        528 => "SET_T1",
        529 => "SETF_T1",
        536 => "MUL_T1",
        537 => "MULF_T1",
        544 => "DIV_T1",
        545 => "DIVF_T1",
        552 => "MOD_T1",
        560 => "ADD_T1",
        _ => {
            if !is_valid_outer_opcode(opcode) {
                return "INVALID";
            }
            let base = opcode & !7;
            let v = opcode & 7;
            if (256..560).contains(&base) {
                match v {
                    0 => "ARITH_I",
                    1 => "ARITH_F",
                    _ => "ARITH_V",
                }
            } else if (0x1B0..=0x260).contains(&opcode) {
                "IMM_ARITH"
            } else if (0x270..=0x300).contains(&opcode) {
                "EXT_STORE"
            } else {
                "OP"
            }
        }
    }
}
pub fn opcode_size(opcode: u16) -> usize {
    match opcode {
        2048 => 6,
        2049 => 0,
        2050 => 10,
        2051 => 6,
        2063 => 12,
        2064 => 12,
        2089 => 0,
        2091 => 2,
        2092 => 6,
        2093 => 6,
        2094 => 6,
        2095 => 2,
        2096 => 6,
        2097 => 6,
        2098 => 6,
        2099 => 6,
        2100 => 8,
        2101 => 8,
        2102 => 0,
        2103 => 10,
        2104 | 2105 | 2107 | 2108 | 2109 | 2115 | 2117 | 2119 => 6,
        2106 => 4,
        2110 | 2111 | 2113 | 2116 | 2118 => 2,
        2114 => 0,
        2128 => 0,
        2112 => 0,
        400 | 401 | 408 | 416 | 417 | 424 | 425 => 2,
        _ if (0x1B0..=0x260).contains(&opcode) => 10,
        _ if (0x270..=0x300).contains(&opcode) => 10,
        _ if (256..=399).contains(&opcode) => 2,
        _ if is_valid_outer_opcode(opcode) => 2,
        _ => 0,
    }
}
pub fn instruction_size(code: &[u8], ip: usize) -> Option<usize> {
    if ip.checked_add(2)? > code.len() {
        return None;
    }
    let opcode = u16::from_le_bytes([code[ip], code[ip + 1]]);
    let read_count = || {
        if ip.checked_add(4)? > code.len() {
            None
        } else {
            Some(u16::from_le_bytes([code[ip + 2], code[ip + 3]]) as usize)
        }
    };
    let len = match opcode {
        0x801 | 0x829 | 0x836 | 0x840 | 0x842 => 4usize.checked_add(read_count()?)?,
        0x850 => 4usize.checked_add(4usize.checked_mul(read_count()?)?)?,
        _ => opcode_size(opcode),
    };
    if len == 0 || ip.checked_add(len)? > code.len() { None } else { Some(len) }
}
static INNER_VM_DIAGNOSTIC_NAMES: &[(u32, &str, &str)] = &[
    (0x008ACBC0, "(inline)", "INPUT_WAIT_MASK"),
    (0x00D696CC, "sub_4031E0", "VM_STR_COUNT_CSV"),
    (0x01B4517C, "sprite_xmodify_define", "SPRITE_XMODIFY_DEFINE"),
    (0x02026AC6, "sub_443BB0", "OP_02026AC6"),
    (0x053FAC99, "sub_422C90", "OP_053FAC99"),
    (0x05D6ED69, "sub_448630", "MUSIC_AUX_STOP"),
    (0x05EA6E4D, "sub_420AB0", "TEXT_FIRST_LINE_STRING"),
    (0x06906970, "sub_4023D0", "OP_06906970"),
    (0x0692314A, "sub_40F260", "OP_0692314A"),
    (0x06B3C8AC, "sub_41EB00", "VM_TEXT_SET_FONT"),
    (0x078A756E, "sub_40FDE0", "REGISTER_HOST_FUNC"),
    (0x098399F2, "sub_41A340", "VM_FONT_CACHE_LOAD"),
    (0x09C43040, "sub_425750", "OP_09C43040"),
    (0x0BEF00DB, "sub_44B870", "SOUND_CH_WAIT"),
    (0x0C070535, "sub_448020", "MUSIC_STOP"),
    (0x0C93FCB4, "sub_443C80", "OP_0C93FCB4"),
    (0x0D389C2F, "sub_404180", "OP_0D389C2F"),
    (0x0DC634C4, "sub_447860", "MUSIC_PLAY_DUAL"),
    (0x109CA5DB, "sub_402580", "OP_109CA5DB"),
    (0x111BD910, "sub_443B70", "OP_111BD910"),
    (0x1204D7E8, "(inline)", "INPUT_WAIT_FOUR_RELEASE"),
    (0x1295BBDA, "sub_40FE70", "REGISTER_HOST_FUNC_HEAD"),
    (0x134D5585, "sub_402090", "PUSH_NESTED_DEFAULT"),
    (0x1576CCEC, "sub_4017F0", "OP_1576CCEC"),
    (0x15EEDEAA, "sub_448150", "MUSIC_FADEOUT"),
    (0x160176BE, "sub_403730", "OP_160176BE"),
    (0x163C0878, "sub_43E620", "OP_163C0878"),
    (0x171AEE22, "sub_424730", "SET_VIEWPORT_OFFSET"),
    (0x19FC7CC7, "sub_44B100", "VOICE_CH_FADE"),
    (0x1BB47604, "sub_42B840", "SAVE_MARK_READY"),
    (0x1D93FD7E, "sub_44AD10", "OP_1D93FD7E"),
    (0x1E24E1B6, "sub_4103F0", "OP_1E24E1B6"),
    (0x1F57B724, "sub_401F40", "OP_1F57B724"),
    (0x201D3D29, "sub_448EF0", "SOUND_NODE_A_SET_VOLUME"),
    (0x2075C300, "sub_43CD10", "VM_GET_KEY_STATE"),
    (0x221C2CC2, "sub_425480", "MOVIE_CLEANUP_PRESENT"),
    (0x22C23FA2, "sub_444050", "OP_22C23FA2"),
    (0x2399D761, "sub_401B10", "SET_FRONTBUFFER"),
    (0x23D5D11D, "sub_43F9F0", "VM_TILE_PATH_CHECK"),
    (0x2539D212, "sub_402720", "VM_FILE_READLINE_INT"),
    (0x25401A1F, "sub_402450", "OP_25401A1F"),
    (0x25D92502, "sub_420970", "TEXT_RECORD_STRING"),
    (0x2626FE02, "sub_448050", "MUSIC_PAUSE"),
    (0x265E07D7, "sub_4500B0", "OP_265E07D7"),
    (0x266A3C79, "sub_403400", "VM_STR_TOUPPER"),
    (0x26DC6566, "sub_44A530", "SOUND_NODE_D_PLAY_DUAL"),
    (0x29098EC0, "sub_4257B0", "OP_29098EC0"),
    (0x29F1AC40, "sub_40FD60", "OP_29F1AC40"),
    (0x2B10081B, "sub_44B810", "SOUND_CH_GET_STAT"),
    (0x2CB1BC41, "sub_41F310", "TEXT_POSITION_BACK"),
    (0x2E861C51, "sub_449860", "SOUND_NODE_B_FADEOUT"),
    (0x30636D6E, "sub_423560", "VM_GRP_EXTCOPY"),
    (0x30FA2A29, "sub_41F520", "OP_30FA2A29"),
    (0x32D0236C, "sub_4485A0", "OP_32D0236C"),
    (0x3513EE87, "sub_443FF0", "OP_3513EE87"),
    (0x35A89417, "sub_41AA40", "VM_FONT_RENDER"),
    (0x3670B283, "sub_426890", "CURSOR_INSIDE_CLIENT"),
    (0x3704C919, "(inline)", "CTX_TRANSITION_ALL"),
    (0x39AE7891, "sub_44A100", "SOUND_NODE_C_FADEOUT"),
    (0x3B1D5537, "vmh_sprite_xmodify_set", "SPRITE_XMODIFY_SET"),
    (0x3C84073C, "(inline)", "WIN_GETKEYSTATE_SHIFT"),
    (0x3D053317, "sub_401D60", "OP_3D053317"),
    (0x3E11F2C5, "sub_401B00", "OP_3E11F2C5"),
    (0x3F34D519, "vm_sprite_set_smooth_animation", "SPRITE_SMOOTH_ANIMATION"),
    (0x3FE05B39, "sub_403230", "VM_STR_GET_CSV_FIELD"),
    (0x40258A56, "sub_44E8B0", "SPRITE_MOVE_WAIT"),
    (0x403960F8, "sub_41EA30", "VM_TEXT_CONFIGURE"),
    (0x4133D7A9, "sub_420A70", "OP_4133D7A9"),
    (0x42D7C922, "sub_403C00", "FILE_DROP_ACCEPT"),
    (0x42F6FDDF, "(inline)", "SCHEDULER_DELTA_NORMALIZE"),
    (0x4311CFA6, "sub_449990", "SOUND_NODE_C_PLAY_DUAL"),
    (0x44BC7555, "(inline)", "INPUT_SUPPRESSION_CLEAR"),
    (0x45815F6D, "sub_402500", "VM_STR_ICOMPARE"),
    (0x45ACD4A7, "sub_42ECA0", "OP_45ACD4A7"),
    (0x4604B9AF, "sub_44B300", "VOICE_CH_GET_FILENAME"),
    (0x461EA516, "(inline)", "CTX_SET_FINALIZER"),
    (0x4662E95D, "(inline)", "WAIT_FRAME"),
    (0x4694366A, "(inline)", "VM_CRC32_LOWER"),
    (0x46B18379, "sub_401CA0", "OP_46B18379"),
    (0x46B28FEF, "sub_448720", "OP_46B28FEF"),
    (0x4A02D664, "sub_44F270", "OP_4A02D664"),
    (0x4AFC6E20, "sub_403B50", "OP_4AFC6E20"),
    (0x4B306C5C, "sub_411A10", "OP_4B306C5C"),
    (0x4C310B4B, "sub_422360", "VM_GRP_POINT_GET"),
    (0x4C99B0EA, "sub_42B140", "OP_4C99B0EA"),
    (0x4D849AA6, "sub_42B950", "SAVE_DIALOG_RESTORE"),
    (0x4E1EB9D0, "sub_43CD60", "OP_4E1EB9D0"),
    (0x4EA2D1FC, "sub_4463D0", "OP_4EA2D1FC"),
    (0x4EF903F3, "(inline)", "CTX_TERMINATE"),
    (0x4F62152A, "sub_4034E0", "VM_STR_WILDCARD_COMPARE"),
    (0x4F6367B3, "sub_448100", "MUSIC_FADE_VOLUME"),
    (0x4FA8483F, "sub_424780", "OP_4FA8483F"),
    (0x507F22C6, "sub_4209D0", "TEXT_RECORD_VALUE_A"),
    (0x50846AD3, "sub_442350", "OP_50846AD3"),
    (0x52819912, "sprite_animate_define_aligned", "SPRITE_ANIMATE_DEFINE_ALIGNED"),
    (0x539B07BC, "sub_423BA0", "OP_539B07BC"),
    (0x55232561, "sub_4029F0", "VM_FILE_READLINE"),
    (0x5548EF5E, "sub_44F690", "OP_5548EF5E"),
    (0x5785A054, "sub_449030", "SOUND_NODE_A_WAIT"),
    (0x57AD7635, "sub_44A170", "SOUND_NODE_C_WAIT"),
    (0x57EFA275, "sub_422180", "OP_57EFA275"),
    (0x57F6310D, "sub_41A6D0", "VM_FONT_LOCATE"),
    (0x59180BBB, "sub_41ED50", "TEXT_ESCAPE_PROCESS"),
    (0x5A981E41, "sub_44B020", "VOICE_CH_STOP"),
    (0x5B87A41D, "sub_41F5A0", "OP_5B87A41D"),
    (0x5BF8F175, "sub_4490A0", "AUDIO_NODE_A_FILENAME"),
    (0x5C4020E6, "sub_4490D0", "AUDIO_NODE_A_FLAG"),
    (0x5C9761C5, "sub_43F070", "OP_5C9761C5"),
    (0x5C9ED743, "sub_41F540", "OP_5C9ED743"),
    (0x5F7FFD3B, "sub_401860", "MSGBOX_YESNO_DEFAULT_NO"),
    (0x5FDCCCEE, "sub_41EAA0", "VM_TEXT_SET_COLORS"),
    (0x60085FA4, "(inline)", "INPUT_SUPPRESSION_SET"),
    (0x6008BEBB, "sub_402CE0", "OP_6008BEBB"),
    (0x619DE833, "sub_403AC0", "OP_619DE833"),
    (0x62442EC6, "sub_442010", "OP_62442EC6"),
    (0x63B8CB23, "sub_402650", "OP_63B8CB23"),
    (0x649CFC79, "sub_4473D0", "OP_649CFC79"),
    (0x6501CC30, "sub_443CB0", "OP_6501CC30"),
    (0x65D3B77C, "sub_4019E0", "OP_65D3B77C"),
    (0x661AFB43, "sprite_rotate", "SPRITE_ROTATE"),
    (0x6621F84D, "sub_41F340", "TEXT_POSITION_PREVIOUS"),
    (0x67444F84, "sub_401790", "OP_67444F84"),
    (0x68EF2987, "sprite_is_moving", "SPRITE_IS_MOVING"),
    (0x69073588, "sub_44F050", "OP_69073588"),
    (0x6B4A7873, "sprite_get_pos_x", "SPRITE_GET_POS_X"),
    (0x6CFC884A, "sub_401D20", "OP_6CFC884A"),
    (0x6E080825, "sub_41F4F0", "OP_6E080825"),
    (0x6E83677A, "sub_4035E0", "OP_6E83677A"),
    (0x6EAEC2A0, "sub_443D50", "OP_6EAEC2A0"),
    (0x6F384A3D, "vmh_sprite_ymodify_define", "SPRITE_YMODIFY_DEFINE"),
    (0x6F5689E4, "sub_4255D0", "OP_6F5689E4"),
    (0x6F769889, "sub_402550", "OP_6F769889"),
    (0x708B0256, "sub_449410", "SOUND_NODE_B_PLAY"),
    (0x718EF651, "sub_4102E0", "SCENE_JUMP_LOAD"),
    (0x721FA5B7, "sub_44DB60", "OP_721FA5B7"),
    (0x735C88F1, "sub_444020", "OP_735C88F1"),
    (0x76EE6C90, "sub_40FD20", "CALL_BY_NAME"),
    (0x77527EF5, "sub_44A150", "AUDIO_NODE_STATUS"),
    (0x777AA894, "sub_449010", "OP_777AA894"),
    (0x78A31C03, "sub_425730", "OP_78A31C03"),
    (0x7B0E970D, "sub_44DFE0", "SPRITE_CLIP"),
    (0x7B657BAB, "sub_44B790", "SOUND_CH_FADEOUT"),
    (0x7C3B36C5, "sub_42B7C0", "OP_7C3B36C5"),
    (0x7D8B2C41, "sub_41A320", "OP_7D8B2C41"),
    (0x7F918815, "sub_448F40", "OP_7F918815"),
    (0x7FCF3C82, "sub_4020C0", "OP_7FCF3C82"),
    (0x801D7AF9, "sub_448660", "MUSIC_AUX_SET_VOLUME"),
    (0x824D124F, "sub_420A00", "TEXT_RECORD_VALUE_B"),
    (0x82FD765B, "sub_420AD0", "VM_TEXT_GET_FONT_SIZE"),
    (0x838DE99B, "sub_449FE0", "SOUND_NODE_C_STOP"),
    (0x8424CF12, "sub_44B3B0", "SOUND_CH_PLAY"),
    (0x884949E0, "sub_44A850", "SOUND_NODE_D_PLAY"),
    (0x88B4B26C, "sub_41F290", "TEXT_POSITION_COUNT"),
    (0x8CFC4573, "sub_420AC0", "TEXT_ACCUMULATED_STRING"),
    (0x8E8167F2, "sub_44ADD0", "VOICE_CH_PLAY"),
    (0x8EB881EF, "sub_44B250", "VOICE_CH_WAIT"),
    (0x8F905B3F, "sub_44B170", "VOICE_CH_FADEOUT"),
    (0x90816C6E, "sub_41A7B0", "FONTOUT_SET_COLORS"),
    (0x90D5298A, "sub_448530", "VOICE_AUX_PLAY"),
    (0x925B75D2, "vm_audio_get_flag", "AUDIO_NODE_D_FLAG"),
    (0x926C13E5, "sub_448FC0", "SOUND_NODE_A_FADEOUT"),
    (0x95E3A441, "sub_44AD80", "AUDIO_NODE_D_FILENAME"),
    (0x95FF28F2, "sub_4439C0", "FONT_SELECT_FACE"),
    (0x99A5DE25, "sub_42B530", "PICTURE_HASH_REGISTERED"),
    (0x9AC11F47, "vm_sprite_make_mosaic", "GRP_MAKE_MOSAIC"),
    (0x9CABFDF3, "sub_44B720", "SOUND_CH_FADE"),
    (0x9FB53FBE, "(inline)", "CTX_IS_REGISTERED"),
    (0xA589DBD1, "sub_447E90", "MUSIC_SNAPSHOT"),
    (0xA62AA5EB, "(inline)", "NATIVE_MODE_PENDING"),
    (0xA65B03AE, "sub_44B6D0", "SOUND_CH_SET_PAN"),
    (0xA93C9856, "sub_40FF00", "REMOVE_HOST_FUNC"),
    (0xAC796720, "sub_424620", "GRP_MODIFY_COPY"),
    (0xAE47892F, "sub_44B1F0", "VOICE_CH_GET_STAT"),
    (0xB0C8F550, "sub_402530", "VM_STR_LEN"),
    (0xB0CE081A, "sub_420960", "TEXT_RECORD_COUNT"),
    (0xB5A1E3C9, "sub_41EBF0", "VM_TEXT_RENDER_ALT"),
    (0xB7E3E81C, "sub_4022E0", "SET_CLICK_SUPPRESSION_MASK"),
    (0xBA383010, "vmh_sprite_ymodify_set", "SPRITE_YMODIFY_SET"),
    (0xBBF35806, "sub_4209A0", "TEXT_RECORD_VOICE"),
    (0xC29B30E3, "sub_44EA40", "SPRITE_ANIMATE_DEFINE"),
    (0xC32E286F, "(inline)", "DEBUG_SOURCE_PATH"),
    (0xC438BA98, "sub_42B650", "SAVE_FINISH_COMMAND"),
    (0xC9B362D0, "sub_44AB80", "SOUND_NODE_D_STOP"),
    (0xCB665AC0, "sub_44B080", "VOICE_CH_SET_VOL"),
    (0xD011FF32, "sub_448070", "MUSIC_RESUME"),
    (0xD1F672C7, "sub_4487F0", "BGM_AUX_WAIT_ONE_TICK"),
    (0xD4590BD3, "(inline)", "SET_CURSOR_POS"),
    (0xD6333B3C, "sub_4026F0", "VM_FILE_CLOSE"),
    (0xDACA10AF, "sub_403C50", "OPEN_HTTP"),
    (0xDD2056F6, "(inline)", "READMARK_HAS_SCRIPT"),
    (0xDE1E2ED0, "sub_4490F0", "SOUND_NODE_B_PLAY_DUAL"),
    (0xDF644F85, "sub_4018D0", "VM_MSGBOX_ERROR_CONDITIONAL"),
    (0xDFCF9F75, "sub_44B5F0", "SOUND_CH_STOP"),
    (0xDFDBA8E4, "sub_44EB80", "SPRITE_ANIMATE_ADD"),
    (0xE0AA2F52, "sub_449970", "AUDIO_NODE_B_FLAG"),
    (0xE186D19A, "sub_41A850", "FONTOUT_SET_STYLE"),
    (0xE3605CFA, "sub_4254B0", "MOVIE_APPLY_CONFIG"),
    (0xE4E3B40D, "sub_402680", "VM_FILE_OPEN"),
    (0xE712FEC1, "sub_449940", "AUDIO_NODE_B_FILENAME"),
    (0xE8A6A59F, "sub_402F50", "VM_FILE_READLINE_RAW"),
    (0xE9E5564D, "sub_44ABB0", "SOUND_NODE_D_SET_VOLUME"),
    (0xEBB5886C, "sub_43EA70", "MAHJONG_SORT_HAND"),
    (0xED84E320, "sub_449CB0", "SOUND_NODE_C_PLAY"),
    (0xF03A9A01, "sub_44A1E0", "AUDIO_NODE_C_FILENAME"),
    (0xF1097A07, "sub_4487C0", "VOICE_AUX_GET_STAT"),
    (0xF1F8A206, "sub_41EBD0", "VM_TEXT_SET_POSITION"),
    (0xF3679C34, "sub_4011E0", "DEBUG_LOG_STACK"),
    (0xF7824B92, "sub_44A210", "AUDIO_NODE_C_FLAG"),
    (0xF86FC950, "sub_447F00", "MUSIC_RESTORE"),
    (0xF8D8925B, "(inline)", "MUSIC_GET_STATUS"),
    (0xF9705332, "sub_4239F0", "VM_PAGE_SET_ANTIDATA"),
    (0xF9D7B692, "sub_44B650", "SOUND_CH_SET_VOL"),
];
pub static INNER_VM_TABLE: std::sync::LazyLock<Vec<(u32, &'static str, &'static str)>> = std::sync::LazyLock::new(||
{
    let mut table = Vec::with_capacity(373);
    for &hash in crate::exec::handlers::INNER_ROUTE_GROUPS
        .iter()
        .flat_map(|group| group.iter())
    {
        if matches!(hash, 0x102B6437 | 0xEDEFB0E0) {
            continue;
        }
        match INNER_VM_DIAGNOSTIC_NAMES
            .binary_search_by_key(&hash, |(known, _, _)| *known)
        {
            Ok(index) => table.push(INNER_VM_DIAGNOSTIC_NAMES[index]),
            Err(_) => {
                let name = Box::leak(format!("OP_{hash:08X}").into_boxed_str());
                table.push((hash, "(routed)", name));
            }
        }
    }
    table.sort_unstable_by_key(|(hash, _, _)| *hash);
    table.dedup_by_key(|(hash, _, _)| *hash);
    debug_assert_eq!(table.len(), 375);
    table
});

