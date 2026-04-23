/// Unicode character helpers for drawing, animation, and symbols.
///
/// Provides ergonomic access to characters from the following Unicode blocks:
/// - Braille Patterns (U+2800–U+28FF)
/// - Box Drawing (U+2500–U+257F)
/// - Block Elements (U+2580–U+259F)
/// - I Ching Trigrams (U+2630–U+2637) and Hexagrams (U+4DC0–U+4DFF)
/// - Symbols for Legacy Computing (U+1FB00–U+1FBFF)
/// - Symbols for Legacy Computing Supplement (U+1CC00–U+1CEBF)
///
/// # 8-dot cell rendering
///
/// Both Braille patterns and block octant characters represent a 2×4 grid of
/// filled/empty positions.  Use [`OctantDots`] to describe which positions are
/// filled, then choose the visual style via [`OctantStyle`]:
///
/// | Style                   | Characters  | Unicode range         |
/// |-------------------------|-------------|-----------------------|
/// | [`OctantStyle::Braille`] | ⠀–⣿        | U+2800–U+28FF         |
/// | [`OctantStyle::Full`]   | 🬀–🳎        | U+1CD00–U+1CDFE       |
/// | [`OctantStyle::Separated`] | (Unicode 16 supplement) | U+1CE00+ |
///
/// ```ignore
/// use flyline::unicode_helpers::{OctantDots, OctantStyle, octant};
/// // Braille "⠉" (top row filled)
/// let ch = octant(OctantDots::TOP_LEFT | OctantDots::TOP_RIGHT, OctantStyle::Braille);
/// assert_eq!(ch, Some('⠉'));
/// ```

// ─────────────────────────────────────────────────────────────────────────────
// Braille Patterns  U+2800–U+28FF
// ─────────────────────────────────────────────────────────────────────────────

/// The blank Braille Pattern character (U+2800), representing "no dots raised".
///
/// Useful as a sentinel value when overlaying Braille on text.
pub const BRAILLE_BLANK: char = '\u{2800}';

/// Bitflag representing which dots are raised in an 8-dot Braille cell,
/// using the traditional braille dot numbering (column-major layout).
///
/// Standard Braille dot layout:
/// ```text
/// 1  4
/// 2  5
/// 3  6
/// 7  8
/// ```
/// Dots 1–6 form the traditional 6-dot Braille cell; dots 7 and 8 extend it
/// for 8-dot Braille (computer Braille).
///
/// To get a Braille character from a [`BrailleDots`] value, convert it to
/// [`OctantDots`] via [`OctantDots::from_braille`] and call
/// [`octant`] with [`OctantStyle::Braille`].
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct BrailleDots(pub u8);

impl BrailleDots {
    pub const EMPTY: Self = BrailleDots(0);
    pub const DOT_1: Self = BrailleDots(1 << 0); // top-left
    pub const DOT_2: Self = BrailleDots(1 << 1); // mid-left
    pub const DOT_3: Self = BrailleDots(1 << 2); // lower-left
    pub const DOT_4: Self = BrailleDots(1 << 3); // top-right
    pub const DOT_5: Self = BrailleDots(1 << 4); // mid-right
    pub const DOT_6: Self = BrailleDots(1 << 5); // lower-right
    pub const DOT_7: Self = BrailleDots(1 << 6); // bottom-left (8-dot)
    pub const DOT_8: Self = BrailleDots(1 << 7); // bottom-right (8-dot)
}

impl std::ops::BitOr for BrailleDots {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        BrailleDots(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for BrailleDots {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl std::ops::BitAnd for BrailleDots {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        BrailleDots(self.0 & rhs.0)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// I Ching – Trigrams (U+2630–U+2637) and Hexagrams (U+4DC0–U+4DFF)
// ─────────────────────────────────────────────────────────────────────────────

/// ☰ TRIGRAM FOR HEAVEN – all three lines solid (yang), bits: 111
pub const TRIGRAM_HEAVEN: char = '☰';
/// ☱ TRIGRAM FOR LAKE – top line broken, bits: 011
pub const TRIGRAM_LAKE: char = '☱';
/// ☲ TRIGRAM FOR FIRE – middle line broken, bits: 101
pub const TRIGRAM_FIRE: char = '☲';
/// ☳ TRIGRAM FOR THUNDER – bottom line solid only, bits: 001
pub const TRIGRAM_THUNDER: char = '☳';
/// ☴ TRIGRAM FOR WIND – bottom line broken, bits: 110
pub const TRIGRAM_WIND: char = '☴';
/// ☵ TRIGRAM FOR WATER – middle line solid only, bits: 010
pub const TRIGRAM_WATER: char = '☵';
/// ☶ TRIGRAM FOR MOUNTAIN – top line solid only, bits: 100
pub const TRIGRAM_MOUNTAIN: char = '☶';
/// ☷ TRIGRAM FOR EARTH – all three lines broken (yin), bits: 000
pub const TRIGRAM_EARTH: char = '☷';

/// Bitflag selecting which rows of a hexagram are solid (yang, unbroken).
///
/// Bit 0 ([`HexagramRows::BOTTOM`]) is the bottom-most line; bit 5
/// ([`HexagramRows::TOP`]) is the top-most line. A set bit means the line is
/// solid (yang); a clear bit means the line is broken (yin).
///
/// # Example
/// ```
/// use flyline::unicode_helpers::{HexagramRows, yijing_hexagram};
/// // All yang → ䷀ HEXAGRAM FOR THE CREATIVE
/// let ch = yijing_hexagram(HexagramRows::ALL);
/// assert_eq!(ch, '䷀');
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct HexagramRows(pub u8);

impl HexagramRows {
    pub const NONE: Self = HexagramRows(0);
    pub const BOTTOM: Self = HexagramRows(1 << 0);
    pub const ROW_2: Self = HexagramRows(1 << 1);
    pub const ROW_3: Self = HexagramRows(1 << 2);
    pub const ROW_4: Self = HexagramRows(1 << 3);
    pub const ROW_5: Self = HexagramRows(1 << 4);
    pub const TOP: Self = HexagramRows(1 << 5);
    pub const ALL: Self = HexagramRows(0b0011_1111);
}

impl std::ops::BitOr for HexagramRows {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        HexagramRows(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for HexagramRows {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl std::ops::BitAnd for HexagramRows {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        HexagramRows(self.0 & rhs.0)
    }
}

// King Wen ordinal (0-based) for each 6-bit yang-row pattern.
//
// The pattern value encodes the hexagram as (upper_trigram << 3) | lower_trigram,
// with each trigram represented as a 3-bit number (bit 0 = bottom line, solid=1).
//
// Trigram values: Heaven=7, Lake=3, Fire=5, Thunder=1, Wind=6, Water=2, Mountain=4, Earth=0.
const HEXAGRAM_KING_WEN: [u8; 64] = [
    1,  // 0b000000 =  0: Kun           Earth/Earth
    23, // 0b000001 =  1: Fu            Earth/Thunder
    6,  // 0b000010 =  2: Shi           Earth/Water
    18, // 0b000011 =  3: Lin           Earth/Lake
    14, // 0b000100 =  4: Qian (Modest) Earth/Mountain
    35, // 0b000101 =  5: Ming Yi       Earth/Fire
    45, // 0b000110 =  6: Sheng         Earth/Wind
    10, // 0b000111 =  7: Tai           Earth/Heaven
    15, // 0b001000 =  8: Yu            Thunder/Earth
    50, // 0b001001 =  9: Zhen          Thunder/Thunder
    39, // 0b001010 = 10: Jie (Deliver) Thunder/Water
    53, // 0b001011 = 11: Gui Mei       Thunder/Lake
    61, // 0b001100 = 12: Xiao Guo      Thunder/Mountain
    54, // 0b001101 = 13: Feng          Thunder/Fire
    31, // 0b001110 = 14: Heng          Thunder/Wind
    33, // 0b001111 = 15: Da Zhuang     Thunder/Heaven
    7,  // 0b010000 = 16: Bi (Hold)     Water/Earth
    2,  // 0b010001 = 17: Zhun          Water/Thunder
    28, // 0b010010 = 18: Kan           Water/Water
    59, // 0b010011 = 19: Jie (Limit)   Water/Lake
    38, // 0b010100 = 20: Jian (Obst)   Water/Mountain
    62, // 0b010101 = 21: Ji Ji         Water/Fire
    47, // 0b010110 = 22: Jing          Water/Wind
    4,  // 0b010111 = 23: Xu            Water/Heaven
    44, // 0b011000 = 24: Cui           Lake/Earth
    16, // 0b011001 = 25: Sui           Lake/Thunder
    46, // 0b011010 = 26: Kun (Oppress) Lake/Water
    57, // 0b011011 = 27: Dui           Lake/Lake
    30, // 0b011100 = 28: Xian          Lake/Mountain
    48, // 0b011101 = 29: Ge            Lake/Fire
    27, // 0b011110 = 30: Da Guo        Lake/Wind
    42, // 0b011111 = 31: Guai          Lake/Heaven
    22, // 0b100000 = 32: Bo            Mountain/Earth
    26, // 0b100001 = 33: Yi (Nourish)  Mountain/Thunder
    3,  // 0b100010 = 34: Meng          Mountain/Water
    40, // 0b100011 = 35: Sun (Decr)    Mountain/Lake
    51, // 0b100100 = 36: Gen           Mountain/Mountain
    21, // 0b100101 = 37: Bi (Grace)    Mountain/Fire
    17, // 0b100110 = 38: Gu            Mountain/Wind
    25, // 0b100111 = 39: Da Chu        Mountain/Heaven
    34, // 0b101000 = 40: Jin           Fire/Earth
    20, // 0b101001 = 41: Shi He        Fire/Thunder
    63, // 0b101010 = 42: Wei Ji        Fire/Water
    37, // 0b101011 = 43: Kui           Fire/Lake
    55, // 0b101100 = 44: Lü (Wander)   Fire/Mountain
    29, // 0b101101 = 45: Li (Cling)    Fire/Fire
    49, // 0b101110 = 46: Ding          Fire/Wind
    13, // 0b101111 = 47: Da You        Fire/Heaven
    19, // 0b110000 = 48: Guan          Wind/Earth
    41, // 0b110001 = 49: Yi (Increase) Wind/Thunder
    58, // 0b110010 = 50: Huan          Wind/Water
    60, // 0b110011 = 51: Zhong Fu      Wind/Lake
    52, // 0b110100 = 52: Jian (Develop)Wind/Mountain
    36, // 0b110101 = 53: Jia Ren       Wind/Fire
    56, // 0b110110 = 54: Xun (Gentle)  Wind/Wind
    8,  // 0b110111 = 55: Xiao Chu      Wind/Heaven
    11, // 0b111000 = 56: Pi            Heaven/Earth
    24, // 0b111001 = 57: Wu Wang       Heaven/Thunder
    5,  // 0b111010 = 58: Song          Heaven/Water
    9,  // 0b111011 = 59: Lü (Conduct)  Heaven/Lake
    32, // 0b111100 = 60: Dun           Heaven/Mountain
    12, // 0b111101 = 61: Tong Ren      Heaven/Fire
    43, // 0b111110 = 62: Gou           Heaven/Wind
    0,  // 0b111111 = 63: Qian          Heaven/Heaven
];

/// Returns the I Ching hexagram character for the given combination of yang rows.
///
/// The 64 hexagrams are at U+4DC0–U+4DFF in King Wen sequence.
/// `rows_visible` selects which of the six lines are solid (yang); clear bits are yin (broken).
/// Only the lower 6 bits of `rows_visible` are significant.
pub fn yijing_hexagram(rows_visible: HexagramRows) -> char {
    let pattern = (rows_visible.0 & 0x3F) as usize;
    let ordinal = HEXAGRAM_KING_WEN[pattern] as u32;
    // All 64 codepoints U+4DC0–U+4DFF are assigned hexagram characters.
    char::from_u32(0x4DC0 + ordinal).expect("hexagram codepoints are all valid")
}

// ─────────────────────────────────────────────────────────────────────────────
// Box Drawing  U+2500–U+257F
// ─────────────────────────────────────────────────────────────────────────────

/// Bitflag indicating which sides of a cell a pipe character connects to.
///
/// Multiple directions can be combined with `|`.
///
/// # Example
/// ```
/// use flyline::unicode_helpers::{Directions, PipeStyle, pipe};
/// assert_eq!(pipe(Directions::LEFT | Directions::RIGHT, PipeStyle::Single), Some('─'));
/// assert_eq!(pipe(Directions::TOP | Directions::BOTTOM, PipeStyle::Double), Some('║'));
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct Directions(pub u8);

impl Directions {
    pub const NONE: Self = Directions(0);
    pub const TOP: Self = Directions(1 << 0);
    pub const RIGHT: Self = Directions(1 << 1);
    pub const BOTTOM: Self = Directions(1 << 2);
    pub const LEFT: Self = Directions(1 << 3);
    pub const ALL: Self = Directions(0b1111);
}

impl std::ops::BitOr for Directions {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Directions(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for Directions {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl std::ops::BitAnd for Directions {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        Directions(self.0 & rhs.0)
    }
}

/// Line style for the [`pipe`] helper.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PipeStyle {
    /// Thin single-line box drawing (─ │ ┌ …).
    Single,
    /// Double-line box drawing (═ ║ ╔ …).
    Double,
}

// Lookup tables indexed by the 4-bit direction mask (TOP=bit0, RIGHT=bit1, BOTTOM=bit2, LEFT=bit3).
// None entries mean no standard box-drawing character exists for that combination.
#[rustfmt::skip]
const PIPE_SINGLE: [Option<char>; 16] = [
    None,        // 0b0000 none
    Some('╵'),   // 0b0001 T
    Some('╶'),   // 0b0010 R
    Some('└'),   // 0b0011 T+R
    Some('╷'),   // 0b0100 B
    Some('│'),   // 0b0101 T+B
    Some('┌'),   // 0b0110 R+B
    Some('├'),   // 0b0111 T+R+B
    Some('╴'),   // 0b1000 L
    Some('┘'),   // 0b1001 T+L
    Some('─'),   // 0b1010 R+L
    Some('┴'),   // 0b1011 T+R+L
    Some('┐'),   // 0b1100 B+L
    Some('┤'),   // 0b1101 T+B+L
    Some('┬'),   // 0b1110 R+B+L
    Some('┼'),   // 0b1111 T+R+B+L
];

#[rustfmt::skip]
const PIPE_DOUBLE: [Option<char>; 16] = [
    None,        // 0b0000 none
    None,        // 0b0001 T only – no double stub
    None,        // 0b0010 R only – no double stub
    Some('╚'),   // 0b0011 T+R
    None,        // 0b0100 B only – no double stub
    Some('║'),   // 0b0101 T+B
    Some('╔'),   // 0b0110 R+B
    Some('╠'),   // 0b0111 T+R+B
    None,        // 0b1000 L only – no double stub
    Some('╝'),   // 0b1001 T+L
    Some('═'),   // 0b1010 R+L
    Some('╩'),   // 0b1011 T+R+L
    Some('╗'),   // 0b1100 B+L
    Some('╣'),   // 0b1101 T+B+L
    Some('╦'),   // 0b1110 R+B+L
    Some('╬'),   // 0b1111 T+R+B+L
];

/// Returns the box-drawing character that connects the given sides in the given style,
/// or `None` if no standard character exists for that combination.
pub fn pipe(connections: Directions, style: PipeStyle) -> Option<char> {
    let idx = (connections.0 & 0x0F) as usize;
    match style {
        PipeStyle::Single => PIPE_SINGLE[idx],
        PipeStyle::Double => PIPE_DOUBLE[idx],
    }
}

// ── Single light lines ────────────────────────────────────────────────────────
pub const BOX_HORIZONTAL: char = '─'; // U+2500
pub const BOX_VERTICAL: char = '│'; // U+2502
// stubs
pub const BOX_LEFT_STUB: char = '╴'; // U+2574
pub const BOX_TOP_STUB: char = '╵'; // U+2575
pub const BOX_RIGHT_STUB: char = '╶'; // U+2576
pub const BOX_BOTTOM_STUB: char = '╷'; // U+2577
// corners
pub const BOX_DOWN_RIGHT: char = '┌'; // U+250C
pub const BOX_DOWN_LEFT: char = '┐'; // U+2510
pub const BOX_UP_RIGHT: char = '└'; // U+2514
pub const BOX_UP_LEFT: char = '┘'; // U+2518
// T-junctions
pub const BOX_VERT_RIGHT: char = '├'; // U+251C
pub const BOX_VERT_LEFT: char = '┤'; // U+2524
pub const BOX_DOWN_HORIZ: char = '┬'; // U+252C
pub const BOX_UP_HORIZ: char = '┴'; // U+2534
pub const BOX_CROSS: char = '┼'; // U+253C

// ── Single light dashed lines ─────────────────────────────────────────────────
pub const BOX_HORIZ_2DASH: char = '╌'; // U+254C
pub const BOX_VERT_2DASH: char = '╎'; // U+254E
pub const BOX_HORIZ_4DASH: char = '┄'; // U+2504
pub const BOX_VERT_4DASH: char = '┆'; // U+2506
pub const BOX_HORIZ_3DASH: char = '┈'; // U+2508
pub const BOX_VERT_3DASH: char = '┊'; // U+250A

// ── Heavy/thick single lines ──────────────────────────────────────────────────
pub const BOX_HEAVY_HORIZONTAL: char = '━'; // U+2501
pub const BOX_HEAVY_VERTICAL: char = '┃'; // U+2503
// heavy corners
pub const BOX_HEAVY_DOWN_RIGHT: char = '┏'; // U+250F
pub const BOX_HEAVY_DOWN_LEFT: char = '┓'; // U+2513
pub const BOX_HEAVY_UP_RIGHT: char = '┗'; // U+2517
pub const BOX_HEAVY_UP_LEFT: char = '┛'; // U+251B
// heavy T-junctions
pub const BOX_HEAVY_VERT_RIGHT: char = '┣'; // U+2523
pub const BOX_HEAVY_VERT_LEFT: char = '┫'; // U+252B
pub const BOX_HEAVY_DOWN_HORIZ: char = '┳'; // U+2533
pub const BOX_HEAVY_UP_HORIZ: char = '┻'; // U+253B
pub const BOX_HEAVY_CROSS: char = '╋'; // U+254B

// ── Heavy dashed lines ────────────────────────────────────────────────────────
pub const BOX_HEAVY_HORIZ_4DASH: char = '┅'; // U+2505
pub const BOX_HEAVY_VERT_4DASH: char = '┇'; // U+2507
pub const BOX_HEAVY_HORIZ_3DASH: char = '┉'; // U+2509
pub const BOX_HEAVY_VERT_3DASH: char = '┋'; // U+250B
pub const BOX_HEAVY_HORIZ_2DASH: char = '╍'; // U+254D
pub const BOX_HEAVY_VERT_2DASH: char = '╏'; // U+254F

// ── Light/heavy mixed lines ───────────────────────────────────────────────────
// horizontal heavy, vertical light
pub const BOX_LIGHT_DOWN_HEAVY_RIGHT: char = '┍'; // U+250D
pub const BOX_HEAVY_DOWN_LIGHT_RIGHT: char = '┎'; // U+250E
pub const BOX_LIGHT_DOWN_HEAVY_LEFT: char = '┑'; // U+2511
pub const BOX_HEAVY_DOWN_LIGHT_LEFT: char = '┒'; // U+2512
pub const BOX_LIGHT_UP_HEAVY_RIGHT: char = '┕'; // U+2515
pub const BOX_HEAVY_UP_LIGHT_RIGHT: char = '┖'; // U+2516
pub const BOX_LIGHT_UP_HEAVY_LEFT: char = '┙'; // U+2519
pub const BOX_HEAVY_UP_LIGHT_LEFT: char = '┚'; // U+251A
pub const BOX_HEAVY_VERT_LIGHT_RIGHT: char = '┝'; // U+251D
pub const BOX_LIGHT_VERT_HEAVY_RIGHT: char = '┠'; // U+2520
pub const BOX_HEAVY_VERT_LIGHT_LEFT: char = '┥'; // U+2525
pub const BOX_LIGHT_VERT_HEAVY_LEFT: char = '┨'; // U+2528
pub const BOX_LIGHT_DOWN_HEAVY_HORIZ: char = '┯'; // U+252F
pub const BOX_HEAVY_DOWN_LIGHT_HORIZ: char = '┰'; // U+2530
pub const BOX_LIGHT_UP_HEAVY_HORIZ: char = '┷'; // U+2537
pub const BOX_HEAVY_UP_LIGHT_HORIZ: char = '┸'; // U+2538
pub const BOX_LIGHT_VERT_HEAVY_HORIZ: char = '┿'; // U+253F
pub const BOX_HEAVY_VERT_LIGHT_HORIZ: char = '╂'; // U+2542

// ── Double lines ──────────────────────────────────────────────────────────────
pub const BOX_DOUBLE_HORIZONTAL: char = '═'; // U+2550
pub const BOX_DOUBLE_VERTICAL: char = '║'; // U+2551
pub const BOX_DOUBLE_DOWN_RIGHT: char = '╔'; // U+2554
pub const BOX_DOUBLE_DOWN_LEFT: char = '╗'; // U+2557
pub const BOX_DOUBLE_UP_RIGHT: char = '╚'; // U+255A
pub const BOX_DOUBLE_UP_LEFT: char = '╝'; // U+255D
pub const BOX_DOUBLE_VERT_RIGHT: char = '╠'; // U+2560
pub const BOX_DOUBLE_VERT_LEFT: char = '╣'; // U+2563
pub const BOX_DOUBLE_DOWN_HORIZ: char = '╦'; // U+2566
pub const BOX_DOUBLE_UP_HORIZ: char = '╩'; // U+2569
pub const BOX_DOUBLE_CROSS: char = '╬'; // U+256C

// ── Single/double mixed ───────────────────────────────────────────────────────
// (horizontal double, vertical single)
pub const BOX_DOWN_SINGLE_RIGHT_DOUBLE: char = '╒'; // U+2552
pub const BOX_DOWN_DOUBLE_RIGHT_SINGLE: char = '╓'; // U+2553
pub const BOX_DOWN_SINGLE_LEFT_DOUBLE: char = '╕'; // U+2555
pub const BOX_DOWN_DOUBLE_LEFT_SINGLE: char = '╖'; // U+2556
pub const BOX_UP_SINGLE_RIGHT_DOUBLE: char = '╘'; // U+2558
pub const BOX_UP_DOUBLE_RIGHT_SINGLE: char = '╙'; // U+2559
pub const BOX_UP_SINGLE_LEFT_DOUBLE: char = '╛'; // U+255B
pub const BOX_UP_DOUBLE_LEFT_SINGLE: char = '╜'; // U+255C
pub const BOX_VERT_SINGLE_RIGHT_DOUBLE: char = '╞'; // U+255E
pub const BOX_VERT_DOUBLE_RIGHT_SINGLE: char = '╟'; // U+255F
pub const BOX_VERT_SINGLE_LEFT_DOUBLE: char = '╡'; // U+2561
pub const BOX_VERT_DOUBLE_LEFT_SINGLE: char = '╢'; // U+2562
pub const BOX_DOWN_SINGLE_HORIZ_DOUBLE: char = '╤'; // U+2564
pub const BOX_DOWN_DOUBLE_HORIZ_SINGLE: char = '╥'; // U+2565
pub const BOX_UP_SINGLE_HORIZ_DOUBLE: char = '╧'; // U+2567
pub const BOX_UP_DOUBLE_HORIZ_SINGLE: char = '╨'; // U+2568
pub const BOX_VERT_SINGLE_HORIZ_DOUBLE: char = '╪'; // U+256A
pub const BOX_VERT_DOUBLE_HORIZ_SINGLE: char = '╫'; // U+256B

// ── Arc/rounded corners ───────────────────────────────────────────────────────
pub const BOX_ARC_DOWN_RIGHT: char = '╭'; // U+256D
pub const BOX_ARC_DOWN_LEFT: char = '╮'; // U+256E
pub const BOX_ARC_UP_LEFT: char = '╯'; // U+256F
pub const BOX_ARC_UP_RIGHT: char = '╰'; // U+2570

// ── Diagonal lines ────────────────────────────────────────────────────────────
pub const BOX_DIAGONAL_FORWARD: char = '╱'; // U+2571
pub const BOX_DIAGONAL_BACKWARD: char = '╲'; // U+2572
pub const BOX_DIAGONAL_CROSS: char = '╳'; // U+2573

// ─────────────────────────────────────────────────────────────────────────────
// Block Elements  U+2580–U+259F
// ─────────────────────────────────────────────────────────────────────────────

// ── Horizontal fraction blocks (filling from top or bottom) ───────────────────
pub const UPPER_HALF_BLOCK: char = '▀'; // U+2580
pub const LOWER_ONE_EIGHTH_BLOCK: char = '▁'; // U+2581
pub const LOWER_ONE_QUARTER_BLOCK: char = '▂'; // U+2582
pub const LOWER_THREE_EIGHTHS_BLOCK: char = '▃'; // U+2583
pub const LOWER_HALF_BLOCK: char = '▄'; // U+2584
pub const LOWER_FIVE_EIGHTHS_BLOCK: char = '▅'; // U+2585
pub const LOWER_THREE_QUARTERS_BLOCK: char = '▆'; // U+2586
pub const LOWER_SEVEN_EIGHTHS_BLOCK: char = '▇'; // U+2587
pub const FULL_BLOCK: char = '█'; // U+2588

// ── Vertical fraction blocks (filling from left) ──────────────────────────────
pub const LEFT_SEVEN_EIGHTHS_BLOCK: char = '▉'; // U+2589
pub const LEFT_THREE_QUARTERS_BLOCK: char = '▊'; // U+258A
pub const LEFT_FIVE_EIGHTHS_BLOCK: char = '▋'; // U+258B
pub const LEFT_HALF_BLOCK: char = '▌'; // U+258C
pub const LEFT_THREE_EIGHTHS_BLOCK: char = '▍'; // U+258D
pub const LEFT_ONE_QUARTER_BLOCK: char = '▎'; // U+258E
pub const LEFT_ONE_EIGHTH_BLOCK: char = '▏'; // U+258F
pub const RIGHT_HALF_BLOCK: char = '▐'; // U+2590

// ── Shading ───────────────────────────────────────────────────────────────────
pub const LIGHT_SHADE: char = '░'; // U+2591
pub const MEDIUM_SHADE: char = '▒'; // U+2592
pub const DARK_SHADE: char = '▓'; // U+2593

// ── Single-eighth edge blocks ─────────────────────────────────────────────────
pub const UPPER_ONE_EIGHTH_BLOCK: char = '▔'; // U+2594
pub const RIGHT_ONE_EIGHTH_BLOCK: char = '▕'; // U+2595

// ── Quadrant blocks ───────────────────────────────────────────────────────────
pub const LOWER_LEFT_QUADRANT_BLOCK: char = '▖'; // U+2596
pub const LOWER_RIGHT_QUADRANT_BLOCK: char = '▗'; // U+2597
pub const UPPER_LEFT_QUADRANT_BLOCK: char = '▘'; // U+2598
/// Three-quarter block: upper-left + lower-left + lower-right
pub const UPPER_LEFT_LOWER_LEFT_LOWER_RIGHT_BLOCK: char = '▙'; // U+2599
/// Diagonal: upper-left + lower-right
pub const UPPER_LEFT_LOWER_RIGHT_BLOCK: char = '▚'; // U+259A
/// Three-quarter block: upper-left + upper-right + lower-left
pub const UPPER_LEFT_UPPER_RIGHT_LOWER_LEFT_BLOCK: char = '▛'; // U+259B
/// Three-quarter block: upper-left + upper-right + lower-right
pub const UPPER_LEFT_UPPER_RIGHT_LOWER_RIGHT_BLOCK: char = '▜'; // U+259C
pub const UPPER_RIGHT_QUADRANT_BLOCK: char = '▝'; // U+259D
/// Diagonal: upper-right + lower-left
pub const UPPER_RIGHT_LOWER_LEFT_BLOCK: char = '▞'; // U+259E
/// Three-quarter block: upper-right + lower-left + lower-right
pub const UPPER_RIGHT_LOWER_LEFT_LOWER_RIGHT_BLOCK: char = '▟'; // U+259F

// ─────────────────────────────────────────────────────────────────────────────
// Quadrant helper  (uses Block Elements + half-blocks above)
// ─────────────────────────────────────────────────────────────────────────────

/// Bitflag selecting which of the four quadrant positions are filled.
///
/// Each cell is a 2×2 grid:
/// ```text
/// UPPER_LEFT   UPPER_RIGHT
/// LOWER_LEFT   LOWER_RIGHT
/// ```
///
/// # Example
/// ```ignore
/// use flyline::unicode_helpers::{Quadrant, QuadrantStyle, quadrant};
/// assert_eq!(quadrant(Quadrant::UPPER_LEFT | Quadrant::UPPER_RIGHT, QuadrantStyle::Full), Some('▀'));
/// assert_eq!(quadrant(Quadrant::ALL, QuadrantStyle::Full), Some('█'));
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct Quadrant(pub u8);

impl Quadrant {
    pub const NONE: Self = Quadrant(0);
    pub const UPPER_LEFT: Self = Quadrant(1 << 0);
    pub const UPPER_RIGHT: Self = Quadrant(1 << 1);
    pub const LOWER_LEFT: Self = Quadrant(1 << 2);
    pub const LOWER_RIGHT: Self = Quadrant(1 << 3);
    pub const ALL: Self = Quadrant(0b0000_1111);

    /// Construct from a 2-column × 2-row boolean grid where `grid[col][row]`
    /// is `true` if that position is filled.
    ///
    /// - `grid[0]` = left column (rows 0–1 from top to bottom)
    /// - `grid[1]` = right column (rows 0–1 from top to bottom)
    pub fn from_grid(grid: [[bool; 2]; 2]) -> Self {
        Quadrant(
            (grid[0][0] as u8)           // row 0, col 0 → UPPER_LEFT  (bit 0)
            | ((grid[1][0] as u8) << 1)  // row 0, col 1 → UPPER_RIGHT (bit 1)
            | ((grid[0][1] as u8) << 2)  // row 1, col 0 → LOWER_LEFT  (bit 2)
            | ((grid[1][1] as u8) << 3), // row 1, col 1 → LOWER_RIGHT (bit 3)
        )
    }
}

impl std::ops::BitOr for Quadrant {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Quadrant(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for Quadrant {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl std::ops::BitAnd for Quadrant {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        Quadrant(self.0 & rhs.0)
    }
}

/// Visual rendering style for [`quadrant`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum QuadrantStyle {
    /// Characters from Block Elements (U+2580–U+259F) and half-blocks.
    Full,
    /// Separated quadrant block characters from Symbols for Legacy Computing
    /// Supplement (U+1CC00–U+1CEBF, Unicode 16.0).
    Separated,
}

// Lookup table indexed by the 4-bit pattern (bit 0=UL, 1=UR, 2=LL, 3=LR).
// None means no character is assigned for that combination.
#[rustfmt::skip]
const QUADRANT_FULL: [Option<char>; 16] = [
    None,        // 0b0000: empty
    Some('▘'),   // 0b0001: UL
    Some('▝'),   // 0b0010: UR
    Some('▀'),   // 0b0011: UL+UR  (upper half block)
    Some('▖'),   // 0b0100: LL
    Some('▌'),   // 0b0101: UL+LL  (left half block)
    Some('▞'),   // 0b0110: UR+LL
    Some('▛'),   // 0b0111: UL+UR+LL
    Some('▗'),   // 0b1000: LR
    Some('▚'),   // 0b1001: UL+LR
    Some('▐'),   // 0b1010: UR+LR  (right half block)
    Some('▜'),   // 0b1011: UL+UR+LR
    Some('▄'),   // 0b1100: LL+LR  (lower half block)
    Some('▙'),   // 0b1101: UL+LL+LR
    Some('▟'),   // 0b1110: UR+LL+LR
    Some('█'),   // 0b1111: all    (full block)
];

/// Returns the quadrant block character for the given filled positions and style.
///
/// Returns `None` if all positions are clear (empty cell).  For
/// [`QuadrantStyle::Full`], all 15 non-empty combinations have a character;
/// for [`QuadrantStyle::Separated`], the characters come from the Symbols for
/// Legacy Computing Supplement block (Unicode 16.0), which is ordered by the
/// same 4-bit pattern (offset 0 = UL-only, …, offset 14 = all four).
pub fn quadrant(q: Quadrant, style: QuadrantStyle) -> Option<char> {
    let idx = (q.0 & 0x0F) as usize;
    match style {
        QuadrantStyle::Full => QUADRANT_FULL[idx],
        QuadrantStyle::Separated => {
            if idx == 0 {
                return None;
            }
            // Separated quadrant blocks are at U+1CC21–U+1CC2F in the
            // Symbols for Legacy Computing Supplement (Unicode 16.0).
            // The offset is the same 4-bit pattern minus 1.
            char::from_u32(0x1CC21 + (idx as u32 - 1))
        }
    }
}

/// Convert a 2D boolean grid into lines of quadrant characters.
///
/// The input `grid` is row-major: `grid[row][col]`.  Each output character
/// covers a 2-column × 2-row cell.  Empty cells at the end of each line are
/// stripped, so lines may differ in length.
///
/// # Example
/// ```
/// use flyline::unicode_helpers::{QuadrantStyle, quadrant_from_grid};
/// // A 2-row × 4-col grid → one line of 2 quadrant chars.
/// let grid: Vec<Vec<bool>> = vec![
///     vec![true,  false, false, true],
///     vec![false, true,  true,  false],
/// ];
/// let lines = quadrant_from_grid(&grid, QuadrantStyle::Full);
/// assert_eq!(lines.len(), 1);
/// ```
pub fn quadrant_from_grid(grid: &[impl AsRef<[bool]>], style: QuadrantStyle) -> Vec<String> {
    from_grid_inner::<2>(grid, ' ', |cell| {
        quadrant(Quadrant::from_grid(cell), style).unwrap_or(' ')
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Symbols for Legacy Computing  U+1FB00–U+1FBFF
// ─────────────────────────────────────────────────────────────────────────────

// ── Sextant block elements ────────────────────────────────────────────────────

/// Bitflag selecting which of the six sextant positions are filled.
///
/// Each cell is a 2×3 grid:
/// ```text
/// TOP_LEFT    TOP_RIGHT
/// MID_LEFT    MID_RIGHT
/// BOT_LEFT    BOT_RIGHT
/// ```
///
/// # Example
/// ```
/// use flyline::unicode_helpers::{Sextant, SextantStyle, sextant};
/// let ch = sextant(Sextant::TOP_LEFT | Sextant::MID_RIGHT, SextantStyle::Full);
/// assert!(ch.is_some());
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct Sextant(pub u8);

impl Sextant {
    pub const NONE: Self = Sextant(0);
    pub const TOP_LEFT: Self = Sextant(1 << 0); // position 1
    pub const TOP_RIGHT: Self = Sextant(1 << 1); // position 2
    pub const MID_LEFT: Self = Sextant(1 << 2); // position 3
    pub const MID_RIGHT: Self = Sextant(1 << 3); // position 4
    pub const BOT_LEFT: Self = Sextant(1 << 4); // position 5
    pub const BOT_RIGHT: Self = Sextant(1 << 5); // position 6
    pub const ALL: Self = Sextant(0b0011_1111);

    /// Construct from a 2-column × 3-row boolean grid where `grid[col][row]`
    /// is `true` if that position is filled.
    ///
    /// - `grid[0]` = left column (rows 0–2 from top to bottom)
    /// - `grid[1]` = right column (rows 0–2 from top to bottom)
    pub fn from_grid(grid: [[bool; 3]; 2]) -> Self {
        Sextant(
            (grid[0][0] as u8)           // row 0, col 0 → TOP_LEFT  (bit 0)
            | ((grid[1][0] as u8) << 1)  // row 0, col 1 → TOP_RIGHT (bit 1)
            | ((grid[0][1] as u8) << 2)  // row 1, col 0 → MID_LEFT  (bit 2)
            | ((grid[1][1] as u8) << 3)  // row 1, col 1 → MID_RIGHT (bit 3)
            | ((grid[0][2] as u8) << 4)  // row 2, col 0 → BOT_LEFT  (bit 4)
            | ((grid[1][2] as u8) << 5), // row 2, col 1 → BOT_RIGHT (bit 5)
        )
    }
}

impl std::ops::BitOr for Sextant {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Sextant(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for Sextant {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl std::ops::BitAnd for Sextant {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        Sextant(self.0 & rhs.0)
    }
}

/// Visual rendering style for [`sextant`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SextantStyle {
    /// Filled/touching sextant blocks from U+1FB00–U+1FB3E.
    Full,
    /// Separated sextant blocks from U+1CE51–U+1CE8F (Unicode 16.0,
    /// Symbols for Legacy Computing Supplement).
    Separated,
}

/// Returns the block sextant character for the given filled positions and style.
///
/// Returns `None` if all positions are clear (empty cell), or if the requested
/// codepoint is not assigned in the target Unicode block.
///
/// Full sextants are at U+1FB00–U+1FB3E (Symbols for Legacy Computing).
/// Separated sextants are at U+1CE51–U+1CE8F (Symbols for Legacy Computing Supplement).
///
/// The bit pattern encodes positions in reading order:
/// bit 0 = top-left, bit 1 = top-right, bit 2 = mid-left, bit 3 = mid-right,
/// bit 4 = bot-left, bit 5 = bot-right.
pub fn sextant(sextants_visible: Sextant, style: SextantStyle) -> Option<char> {
    let bits = sextants_visible.0 & 0x3F;
    if bits == 0 {
        return None;
    }
    let offset = bits as u32 - 1;
    let base = match style {
        SextantStyle::Full => 0x1FB00,
        SextantStyle::Separated => 0x1CE51,
    };
    char::from_u32(base + offset)
}

/// Convert a 2D boolean grid into lines of sextant characters.
///
/// The input `grid` is row-major: `grid[row][col]`.  Each output character
/// covers a 2-column × 3-row cell.  Empty cells at the end of each line are
/// stripped, so lines may differ in length.
///
/// # Example
/// ```
/// use flyline::unicode_helpers::{SextantStyle, sextant_from_grid};
/// // A 3-row × 2-col grid → one line of 1 sextant char.
/// let grid: Vec<Vec<bool>> = vec![
///     vec![true, false],
///     vec![false, true],
///     vec![true, true],
/// ];
/// let lines = sextant_from_grid(&grid, SextantStyle::Full);
/// assert_eq!(lines.len(), 1);
/// ```
pub fn sextant_from_grid(grid: &[impl AsRef<[bool]>], style: SextantStyle) -> Vec<String> {
    from_grid_inner::<3>(grid, ' ', |cell| {
        sextant(Sextant::from_grid(cell), style).unwrap_or(' ')
    })
}

// ── Octant block elements ─────────────────────────────────────────────────────

/// Bitflag selecting which of the eight octant positions are filled.
///
/// Each cell is a 2×4 grid (positions in reading order, left-to-right then
/// top-to-bottom):
/// ```text
/// TOP_LEFT         TOP_RIGHT
/// UPPER_MID_LEFT   UPPER_MID_RIGHT
/// LOWER_MID_LEFT   LOWER_MID_RIGHT
/// BOT_LEFT         BOT_RIGHT
/// ```
///
/// Use [`octant`] to render the dots as a Braille pattern, a filled block
/// octant character, or a separated block octant character.
///
/// # Example
/// ```ignore
/// use flyline::unicode_helpers::{OctantDots, OctantStyle, octant};
/// // Braille "⠉" (top-left + top-right)
/// assert_eq!(octant(OctantDots::TOP_LEFT | OctantDots::TOP_RIGHT, OctantStyle::Braille), Some('⠉'));
/// // Full block octant (U+1CD00 = top-left only)
/// assert_eq!(octant(OctantDots::TOP_LEFT, OctantStyle::Full), char::from_u32(0x1CD00));
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct OctantDots(pub u8);

impl OctantDots {
    pub const NONE: Self = OctantDots(0);
    pub const TOP_LEFT: Self = OctantDots(1 << 0); // position 1
    pub const TOP_RIGHT: Self = OctantDots(1 << 1); // position 2
    pub const UPPER_MID_LEFT: Self = OctantDots(1 << 2); // position 3
    pub const UPPER_MID_RIGHT: Self = OctantDots(1 << 3); // position 4
    pub const LOWER_MID_LEFT: Self = OctantDots(1 << 4); // position 5
    pub const LOWER_MID_RIGHT: Self = OctantDots(1 << 5); // position 6
    pub const BOT_LEFT: Self = OctantDots(1 << 6); // position 7
    pub const BOT_RIGHT: Self = OctantDots(1 << 7); // position 8
    pub const ALL: Self = OctantDots(0xFF);

    /// Construct from a 2-column × 4-row boolean grid where `grid[col][row]`
    /// is `true` if that position is filled.
    ///
    /// - `grid[0]` = left column (rows 0–3 from top to bottom)
    /// - `grid[1]` = right column (rows 0–3 from top to bottom)
    ///
    /// This mirrors the column layout used by the snake animation and braille displays.
    ///
    /// # Example
    /// ```ignore
    /// use flyline::unicode_helpers::{OctantDots, OctantStyle, octant};
    /// // Left column fully filled, right column empty
    /// let dots = OctantDots::from_grid([[true; 4], [false; 4]]);
    /// assert_eq!(dots, OctantDots::TOP_LEFT | OctantDots::UPPER_MID_LEFT
    ///                | OctantDots::LOWER_MID_LEFT | OctantDots::BOT_LEFT);
    /// ```
    pub fn from_grid(grid: [[bool; 4]; 2]) -> Self {
        OctantDots(
            (grid[0][0] as u8)           // row 0, col 0 → TOP_LEFT        (bit 0)
            | ((grid[1][0] as u8) << 1)  // row 0, col 1 → TOP_RIGHT       (bit 1)
            | ((grid[0][1] as u8) << 2)  // row 1, col 0 → UPPER_MID_LEFT  (bit 2)
            | ((grid[1][1] as u8) << 3)  // row 1, col 1 → UPPER_MID_RIGHT (bit 3)
            | ((grid[0][2] as u8) << 4)  // row 2, col 0 → LOWER_MID_LEFT  (bit 4)
            | ((grid[1][2] as u8) << 5)  // row 2, col 1 → LOWER_MID_RIGHT (bit 5)
            | ((grid[0][3] as u8) << 6)  // row 3, col 0 → BOT_LEFT        (bit 6)
            | ((grid[1][3] as u8) << 7), // row 3, col 1 → BOT_RIGHT       (bit 7)
        )
    }

    /// Construct from a [`BrailleDots`] value, converting from the traditional
    /// column-major braille dot numbering to the reading-order grid used by
    /// [`OctantDots`].
    ///
    /// Equivalent to calling `octant(OctantDots::from_braille(bd), OctantStyle::Braille)`.
    pub fn from_braille(bd: BrailleDots) -> Self {
        // Braille dot layout (column-major): DOT_1=bit0, DOT_2=bit1, DOT_3=bit2,
        // DOT_4=bit3, DOT_5=bit4, DOT_6=bit5, DOT_7=bit6, DOT_8=bit7.
        // OctantDots layout (reading order): TOP_LEFT=bit0, TOP_RIGHT=bit1,
        // UPPER_MID_LEFT=bit2, UPPER_MID_RIGHT=bit3, LOWER_MID_LEFT=bit4,
        // LOWER_MID_RIGHT=bit5, BOT_LEFT=bit6, BOT_RIGHT=bit7.
        //
        // Mapping: Braille DOT_1(bit0)→OctantDots bit0, DOT_2(bit1)→bit2,
        //          DOT_3(bit2)→bit4, DOT_4(bit3)→bit1, DOT_5(bit4)→bit3,
        //          DOT_6(bit5)→bit5, DOT_7(bit6)→bit6, DOT_8(bit7)→bit7.
        let b = bd.0;
        OctantDots(
            (b & 0x01)           // braille bit0 → octant bit0
            | ((b & 0x02) << 1)  // braille bit1 → octant bit2
            | ((b & 0x04) << 2)  // braille bit2 → octant bit4
            | ((b & 0x08) >> 2)  // braille bit3 → octant bit1
            | ((b & 0x10) >> 1)  // braille bit4 → octant bit3
            | (b & 0x20)         // braille bit5 → octant bit5
            | (b & 0xC0), // braille bits6,7 → octant bits6,7
        )
    }
}

impl std::ops::BitOr for OctantDots {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        OctantDots(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for OctantDots {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl std::ops::BitAnd for OctantDots {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        OctantDots(self.0 & rhs.0)
    }
}

/// Visual rendering style for [`octant`].
///
/// All three styles describe the same 2×4 cell grid; only the character set
/// (and thus visual appearance) differs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OctantStyle {
    /// Braille Patterns block (U+2800–U+28FF).
    ///
    /// The Braille encoding reorders the bits to column-major dot order (dots
    /// 1–8).  Because every 8-bit value maps to a valid Braille character,
    /// this variant always returns `Some`, including `Some(BRAILLE_BLANK)` for
    /// [`OctantDots::NONE`].
    Braille,
    /// Filled block octant characters (U+1CD00–U+1CDFE, Symbols for Legacy
    /// Computing Supplement, Unicode 16.0).
    Full,
    /// Separated block octant characters.
    ///
    /// The exact codepoint range in the Symbols for Legacy Computing
    /// Supplement (Unicode 16.0) is not fully standardised for all patterns;
    /// returns `None` if the requested pattern has no assigned codepoint.
    Separated,
}

/// Returns the character that renders the given octant dots in the requested style.
///
/// For [`OctantStyle::Braille`] the function always returns `Some` (returning
/// [`BRAILLE_BLANK`] when `dots` is [`OctantDots::NONE`]).  For the other
/// styles, `None` is returned when `dots` is `NONE` or when no codepoint is
/// assigned.
///
/// # Braille bit remapping
///
/// The [`OctantDots`] type uses reading order (row-major), while the Unicode
/// Braille block uses column-major dot numbering.  [`octant`] performs the
/// remapping automatically so callers always think in terms of grid positions.
pub fn octant(dots: OctantDots, style: OctantStyle) -> Option<char> {
    match style {
        OctantStyle::Braille => {
            // Remap OctantDots (reading order) bits to Braille (column-major) bits.
            // OctantDots: bit0=TL, bit1=TR, bit2=UML, bit3=UMR, bit4=LML, bit5=LMR, bit6=BL, bit7=BR
            // Braille:    bit0=D1(TL), bit1=D2(UML), bit2=D3(LML), bit3=D4(TR),
            //             bit4=D5(UMR), bit5=D6(LMR), bit6=D7(BL), bit7=D8(BR)
            let o = dots.0;
            let b = (o & 0x01)           // octant bit0 (TL)  → braille bit0 (D1)
                | ((o & 0x02) << 2)      // octant bit1 (TR)  → braille bit3 (D4)
                | ((o & 0x04) >> 1)      // octant bit2 (UML) → braille bit1 (D2)
                | ((o & 0x08) << 1)      // octant bit3 (UMR) → braille bit4 (D5)
                | ((o & 0x10) >> 2)      // octant bit4 (LML) → braille bit2 (D3)
                | (o & 0x20)             // octant bit5 (LMR) → braille bit5 (D6)
                | (o & 0xC0); // octant bits6,7 (BL,BR) → braille bits6,7 (D7,D8)
            // Braille block (U+2800–U+28FF) is fully defined for all 256 values.
            Some(
                char::from_u32(0x2800 + b as u32)
                    .expect("Braille block U+2800–U+28FF is fully defined for all 256 patterns"),
            )
        }
        OctantStyle::Full => {
            if dots.0 == 0 {
                return None;
            }
            // Full octant chars: U+1CD00 + (pattern - 1), reading-order encoding.
            let offset = dots.0 as u32 - 1;
            char::from_u32(0x1CD00 + offset)
        }
        OctantStyle::Separated => {
            if dots.0 == 0 {
                return None;
            }
            // Separated octant chars are in the Symbols for Legacy Computing
            // Supplement (Unicode 16.0).  Not all patterns have assigned codepoints.
            let offset = dots.0 as u32 - 1;
            char::from_u32(0x1CE00 + offset)
        }
    }
}

/// Convert a 2D boolean grid into lines of octant characters.
///
/// The input `grid` is row-major: `grid[row][col]`.  Each output character
/// covers a 2-column × 4-row cell.  Empty cells at the end of each line are
/// stripped, so lines may differ in length.  When `style` is
/// [`OctantStyle::Braille`], the "empty" character is [`BRAILLE_BLANK`]
/// (U+2800); for other styles it is a space.
///
/// A grid with 4 or fewer rows always produces a single-element [`Vec`].
///
/// # Example
/// ```
/// use flyline::unicode_helpers::{OctantStyle, octant_from_grid};
/// // A 4-row × 4-col grid → one line of 2 Braille characters.
/// let grid: Vec<Vec<bool>> = vec![
///     vec![true,  false, false, false],
///     vec![false, false, false, false],
///     vec![false, false, false, true ],
///     vec![false, false, false, false],
/// ];
/// let lines = octant_from_grid(&grid, OctantStyle::Braille);
/// assert_eq!(lines.len(), 1);
/// assert_eq!(lines[0].chars().count(), 2);
/// ```
pub fn octant_from_grid(grid: &[impl AsRef<[bool]>], style: OctantStyle) -> Vec<String> {
    // For Braille every pattern maps to a valid character (including BRAILLE_BLANK
    // for the empty pattern), so the empty sentinel is BRAILLE_BLANK.
    // For Full/Separated, octant() returns None for the empty pattern, so we use ' '.
    let empty_char = match style {
        OctantStyle::Braille => BRAILLE_BLANK,
        _ => ' ',
    };
    from_grid_inner::<4>(grid, empty_char, |cell| {
        octant(OctantDots::from_grid(cell), style).unwrap_or(empty_char)
    })
}

/// Shared implementation for all `*_from_grid` functions.
///
/// `R` is the number of rows consumed per output character.  Each character
/// covers a fixed 2-column × R-row cell.  `render_cell` maps a `[[bool; R]; 2]`
/// col-major cell (left column first) to the output character; it should
/// return `empty_char` for a completely empty cell.  Trailing `empty_char`
/// entries are stripped from each output line.
fn from_grid_inner<const R: usize>(
    grid: &[impl AsRef<[bool]>],
    empty_char: char,
    mut render_cell: impl FnMut([[bool; R]; 2]) -> char,
) -> Vec<String> {
    if grid.is_empty() {
        return vec![];
    }
    let num_rows = grid.len();
    let num_cols = grid.iter().map(|r| r.as_ref().len()).max().unwrap_or(0);
    if num_cols == 0 {
        return vec![];
    }

    let char_cols = num_cols.div_ceil(2);
    let num_lines = num_rows.div_ceil(R);

    let mut result = Vec::with_capacity(num_lines);
    for line_idx in 0..num_lines {
        let row_start = line_idx * R;
        let mut chars: Vec<char> = Vec::with_capacity(char_cols);

        for char_col in 0..char_cols {
            let col_start = char_col * 2;

            let mut cell = [[false; R]; 2];
            for r in 0..R {
                let row = row_start + r;
                if row < num_rows {
                    let row_data = grid[row].as_ref();
                    for c in 0..2 {
                        let col = col_start + c;
                        if col < row_data.len() {
                            cell[c][r] = row_data[col];
                        }
                    }
                }
            }

            chars.push(render_cell(cell));
        }

        while chars.last() == Some(&empty_char) {
            chars.pop();
        }
        result.push(chars.into_iter().collect());
    }
    result
}

// ── Additional Symbols for Legacy Computing constants ─────────────────────────

// Segmented digit characters (7-segment LCD style) U+1FBF0–U+1FBF9
pub const SEGMENTED_DIGIT_ZERO: char = '\u{1FBF0}';
pub const SEGMENTED_DIGIT_ONE: char = '\u{1FBF1}';
pub const SEGMENTED_DIGIT_TWO: char = '\u{1FBF2}';
pub const SEGMENTED_DIGIT_THREE: char = '\u{1FBF3}';
pub const SEGMENTED_DIGIT_FOUR: char = '\u{1FBF4}';
pub const SEGMENTED_DIGIT_FIVE: char = '\u{1FBF5}';
pub const SEGMENTED_DIGIT_SIX: char = '\u{1FBF6}';
pub const SEGMENTED_DIGIT_SEVEN: char = '\u{1FBF7}';
pub const SEGMENTED_DIGIT_EIGHT: char = '\u{1FBF8}';
pub const SEGMENTED_DIGIT_NINE: char = '\u{1FBF9}';

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    // ── Braille via octant(…, OctantStyle::Braille) ───────────────────────────

    #[test]
    fn test_octant_braille_blank() {
        // Empty dots → BRAILLE_BLANK (U+2800)
        assert_eq!(
            octant(OctantDots::NONE, OctantStyle::Braille),
            Some(BRAILLE_BLANK)
        );
    }

    #[test]
    fn test_octant_braille_all_dots() {
        // All 8 positions → '⣿' (U+28FF)
        assert_eq!(octant(OctantDots::ALL, OctantStyle::Braille), Some('⣿'));
    }

    #[test]
    fn test_octant_braille_top_row() {
        // TOP_LEFT + TOP_RIGHT → braille DOT_1 + DOT_4 = 0x09 → U+2809 = '⠉'
        assert_eq!(
            octant(
                OctantDots::TOP_LEFT | OctantDots::TOP_RIGHT,
                OctantStyle::Braille
            ),
            Some('⠉')
        );
    }

    #[test]
    fn test_octant_braille_or() {
        // TOP_LEFT + UPPER_MID_LEFT → DOT_1 + DOT_2 = 0x03 → U+2803 = '⠃'
        assert_eq!(
            octant(
                OctantDots::TOP_LEFT | OctantDots::UPPER_MID_LEFT,
                OctantStyle::Braille
            ),
            Some('⠃')
        );
    }

    #[test]
    fn test_octant_braille_most_dots() {
        // All positions except UPPER_MID_RIGHT = all braille dots except DOT_5
        // → 0xEF → U+28EF = '⣯'
        let dots = OctantDots::TOP_LEFT
            | OctantDots::UPPER_MID_LEFT
            | OctantDots::LOWER_MID_LEFT
            | OctantDots::TOP_RIGHT
            | OctantDots::LOWER_MID_RIGHT
            | OctantDots::BOT_LEFT
            | OctantDots::BOT_RIGHT;
        assert_eq!(octant(dots, OctantStyle::Braille), Some('⣯'));
    }

    #[test]
    fn test_octant_braille_single_top_left() {
        // TOP_LEFT only → DOT_1 = 0x01 → U+2801 = '⠁'
        assert_eq!(
            octant(OctantDots::TOP_LEFT, OctantStyle::Braille),
            Some('⠁')
        );
    }

    #[test]
    fn test_octant_braille_single_top_right() {
        // TOP_RIGHT only → DOT_4 = 0x08 → U+2808 = '⠈'
        assert_eq!(
            octant(OctantDots::TOP_RIGHT, OctantStyle::Braille),
            Some('⠈')
        );
    }

    // ── OctantDots::from_grid ─────────────────────────────────────────────────

    #[test]
    fn test_from_grid_left_col_only() {
        // Left column all filled, right column empty
        let dots = OctantDots::from_grid([[true; 4], [false; 4]]);
        assert_eq!(
            dots,
            OctantDots::TOP_LEFT
                | OctantDots::UPPER_MID_LEFT
                | OctantDots::LOWER_MID_LEFT
                | OctantDots::BOT_LEFT
        );
    }

    #[test]
    fn test_from_grid_top_row() {
        // Top row filled (both columns at row 0)
        let dots =
            OctantDots::from_grid([[true, false, false, false], [true, false, false, false]]);
        assert_eq!(dots, OctantDots::TOP_LEFT | OctantDots::TOP_RIGHT);
    }

    #[test]
    fn test_from_grid_braille_consistency() {
        // from_grid followed by OctantStyle::Braille should give the same result
        // as building OctantDots manually and calling octant().
        let col_pair = [[true, false, true, false], [true, false, false, true]];
        let via_grid = octant(OctantDots::from_grid(col_pair), OctantStyle::Braille);
        let manual = octant(
            OctantDots::TOP_LEFT
                | OctantDots::LOWER_MID_LEFT
                | OctantDots::TOP_RIGHT
                | OctantDots::BOT_RIGHT,
            OctantStyle::Braille,
        );
        assert_eq!(via_grid, manual);
    }

    // ── OctantDots::from_braille ──────────────────────────────────────────────

    #[test]
    fn test_from_braille_roundtrip() {
        // Converting BrailleDots → OctantDots → octant(Braille) should produce
        // the same character as using the raw braille bit pattern directly.
        let bd = BrailleDots::DOT_1 | BrailleDots::DOT_4;
        let via_convert = octant(OctantDots::from_braille(bd), OctantStyle::Braille);
        // DOT_1 + DOT_4 → '⠉'
        assert_eq!(via_convert, Some('⠉'));
    }

    // ── Full octant (U+1CD00) ─────────────────────────────────────────────────

    #[test]
    fn test_octant_full_none_returns_none() {
        assert_eq!(octant(OctantDots::NONE, OctantStyle::Full), None);
    }

    #[test]
    fn test_octant_full_top_left_only() {
        // BLOCK OCTANT-1 = U+1CD00
        assert_eq!(
            octant(OctantDots::TOP_LEFT, OctantStyle::Full),
            char::from_u32(0x1CD00)
        );
    }

    #[test]
    fn test_octant_full_two_dots() {
        let dots = OctantDots::TOP_LEFT | OctantDots::BOT_RIGHT;
        // pattern = bit0 | bit7 = 0x81 = 129, offset = 128 → U+1CD80
        assert_eq!(octant(dots, OctantStyle::Full), char::from_u32(0x1CD80));
    }

    // ── Quadrant ─────────────────────────────────────────────────────────────

    #[test]
    fn test_quadrant_none_returns_none() {
        assert_eq!(quadrant(Quadrant::NONE, QuadrantStyle::Full), None);
    }

    #[test]
    fn test_quadrant_full_all() {
        assert_eq!(quadrant(Quadrant::ALL, QuadrantStyle::Full), Some('█'));
    }

    #[test]
    fn test_quadrant_full_upper_half() {
        assert_eq!(
            quadrant(
                Quadrant::UPPER_LEFT | Quadrant::UPPER_RIGHT,
                QuadrantStyle::Full
            ),
            Some('▀')
        );
    }

    #[test]
    fn test_quadrant_full_lower_half() {
        assert_eq!(
            quadrant(
                Quadrant::LOWER_LEFT | Quadrant::LOWER_RIGHT,
                QuadrantStyle::Full
            ),
            Some('▄')
        );
    }

    #[test]
    fn test_quadrant_full_left_half() {
        assert_eq!(
            quadrant(
                Quadrant::UPPER_LEFT | Quadrant::LOWER_LEFT,
                QuadrantStyle::Full
            ),
            Some('▌')
        );
    }

    #[test]
    fn test_quadrant_full_right_half() {
        assert_eq!(
            quadrant(
                Quadrant::UPPER_RIGHT | Quadrant::LOWER_RIGHT,
                QuadrantStyle::Full
            ),
            Some('▐')
        );
    }

    #[test]
    fn test_quadrant_full_single_corners() {
        assert_eq!(
            quadrant(Quadrant::UPPER_LEFT, QuadrantStyle::Full),
            Some('▘')
        );
        assert_eq!(
            quadrant(Quadrant::LOWER_RIGHT, QuadrantStyle::Full),
            Some('▗')
        );
    }

    // ── I Ching hexagrams ─────────────────────────────────────────────────────

    #[test]
    fn test_hexagram_creative_all_yang() {
        // All six lines solid (yang) → Hexagram 1 (Qian, The Creative) = U+4DC0
        assert_eq!(yijing_hexagram(HexagramRows::ALL), '䷀');
    }

    #[test]
    fn test_hexagram_receptive_all_yin() {
        // All six lines broken (yin) → Hexagram 2 (Kun, The Receptive) = U+4DC1
        assert_eq!(yijing_hexagram(HexagramRows::NONE), '䷁');
    }

    #[test]
    fn test_hexagram_wei_ji() {
        // Fire over Water: upper=Fire(5)=101, lower=Water(2)=010 → pattern (5<<3)|2 = 42
        // → Hexagram 64 (Wei Ji, Before Completion) = U+4DFF
        let pattern = HexagramRows(42);
        assert_eq!(yijing_hexagram(pattern), '䷿');
    }

    // ── Pipe / box drawing ────────────────────────────────────────────────────

    #[test]
    fn test_pipe_single_horizontal() {
        assert_eq!(
            pipe(Directions::LEFT | Directions::RIGHT, PipeStyle::Single),
            Some('─')
        );
    }

    #[test]
    fn test_pipe_single_vertical() {
        assert_eq!(
            pipe(Directions::TOP | Directions::BOTTOM, PipeStyle::Single),
            Some('│')
        );
    }

    #[test]
    fn test_pipe_single_cross() {
        assert_eq!(pipe(Directions::ALL, PipeStyle::Single), Some('┼'));
    }

    #[test]
    fn test_pipe_single_corners() {
        assert_eq!(
            pipe(Directions::RIGHT | Directions::BOTTOM, PipeStyle::Single),
            Some('┌')
        );
        assert_eq!(
            pipe(Directions::TOP | Directions::RIGHT, PipeStyle::Single),
            Some('└')
        );
    }

    #[test]
    fn test_pipe_double_horizontal() {
        assert_eq!(
            pipe(Directions::LEFT | Directions::RIGHT, PipeStyle::Double),
            Some('═')
        );
    }

    #[test]
    fn test_pipe_double_cross() {
        assert_eq!(pipe(Directions::ALL, PipeStyle::Double), Some('╬'));
    }

    #[test]
    fn test_pipe_double_stub_is_none() {
        // No double-line stub characters exist
        assert_eq!(pipe(Directions::TOP, PipeStyle::Double), None);
    }

    #[test]
    fn test_pipe_none_direction() {
        assert_eq!(pipe(Directions::NONE, PipeStyle::Single), None);
    }

    // ── Sextant ───────────────────────────────────────────────────────────────

    #[test]
    fn test_sextant_none_returns_none() {
        assert_eq!(sextant(Sextant::NONE, SextantStyle::Full), None);
    }

    #[test]
    fn test_sextant_top_left_only() {
        // BLOCK SEXTANT-1 = U+1FB00
        assert_eq!(
            sextant(Sextant::TOP_LEFT, SextantStyle::Full),
            char::from_u32(0x1FB00)
        );
    }

    #[test]
    fn test_sextant_top_right_only() {
        // BLOCK SEXTANT-2 = U+1FB01
        assert_eq!(
            sextant(Sextant::TOP_RIGHT, SextantStyle::Full),
            char::from_u32(0x1FB01)
        );
    }

    #[test]
    fn test_sextant_top_row() {
        // BLOCK SEXTANT-12 = U+1FB02 (top-left + top-right = bits 0+1 = pattern 3)
        assert_eq!(
            sextant(Sextant::TOP_LEFT | Sextant::TOP_RIGHT, SextantStyle::Full),
            char::from_u32(0x1FB02)
        );
    }

    #[test]
    fn test_sextant_separated_top_left() {
        // SEPARATED BLOCK SEXTANT-1 = U+1CE51
        assert_eq!(
            sextant(Sextant::TOP_LEFT, SextantStyle::Separated),
            char::from_u32(0x1CE51)
        );
    }

    // ── Quadrant::from_grid ───────────────────────────────────────────────────

    #[test]
    fn test_quadrant_from_grid_upper_left() {
        let q = Quadrant::from_grid([[true, false], [false, false]]);
        assert_eq!(q, Quadrant::UPPER_LEFT);
    }

    #[test]
    fn test_quadrant_from_grid_all() {
        let q = Quadrant::from_grid([[true, true], [true, true]]);
        assert_eq!(q, Quadrant::ALL);
        assert_eq!(quadrant(q, QuadrantStyle::Full), Some('█'));
    }

    #[test]
    fn test_quadrant_from_grid_lower_right() {
        let q = Quadrant::from_grid([[false, false], [false, true]]);
        assert_eq!(q, Quadrant::LOWER_RIGHT);
    }

    // ── Sextant::from_grid ────────────────────────────────────────────────────

    #[test]
    fn test_sextant_from_grid_top_left() {
        let s = Sextant::from_grid([[true, false, false], [false, false, false]]);
        assert_eq!(s, Sextant::TOP_LEFT);
    }

    #[test]
    fn test_sextant_from_grid_all() {
        let s = Sextant::from_grid([[true, true, true], [true, true, true]]);
        assert_eq!(s, Sextant::ALL);
    }

    #[test]
    fn test_sextant_from_grid_mid_right() {
        let s = Sextant::from_grid([[false, false, false], [false, true, false]]);
        assert_eq!(s, Sextant::MID_RIGHT);
    }

    // ── octant_from_grid ──────────────────────────────────────────────────────

    #[test]
    fn test_octant_from_grid_empty() {
        let grid: Vec<Vec<bool>> = vec![];
        assert!(octant_from_grid(&grid, OctantStyle::Braille).is_empty());
    }

    #[test]
    fn test_octant_from_grid_single_cell_4_rows() {
        // A 4-row × 2-col grid → 1 output line, 1 Braille char (left col filled).
        let grid: Vec<Vec<bool>> = vec![
            vec![true, false],
            vec![true, false],
            vec![true, false],
            vec![true, false],
        ];
        let lines = octant_from_grid(&grid, OctantStyle::Braille);
        assert_eq!(lines.len(), 1);
        // Left column all filled → TOP_LEFT|UPPER_MID_LEFT|LOWER_MID_LEFT|BOT_LEFT
        let expected = octant(
            OctantDots::TOP_LEFT
                | OctantDots::UPPER_MID_LEFT
                | OctantDots::LOWER_MID_LEFT
                | OctantDots::BOT_LEFT,
            OctantStyle::Braille,
        )
        .unwrap()
        .to_string();
        assert_eq!(lines[0], expected);
    }

    #[test]
    fn test_octant_from_grid_trailing_stripped_braille() {
        // A 4-row × 4-col grid with only the first 2 cols filled → 1 char, trailing blank stripped.
        let grid: Vec<Vec<bool>> = vec![
            vec![true, false, false, false],
            vec![false, false, false, false],
            vec![false, false, false, false],
            vec![false, false, false, false],
        ];
        let lines = octant_from_grid(&grid, OctantStyle::Braille);
        assert_eq!(lines.len(), 1);
        // Only first cell (cols 0-1) has a dot; second cell (cols 2-3) is blank → stripped.
        assert_eq!(lines[0].chars().count(), 1);
    }

    #[test]
    fn test_octant_from_grid_two_lines() {
        // An 8-row × 2-col grid → 2 output lines.
        let mut grid: Vec<Vec<bool>> = vec![vec![false, false]; 8];
        grid[0][0] = true; // top of line 1
        grid[4][1] = true; // top of line 2
        let lines = octant_from_grid(&grid, OctantStyle::Braille);
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_octant_from_grid_four_rows_is_single_line() {
        // Exactly 4 rows → always 1 output line.
        let grid: Vec<Vec<bool>> = vec![
            vec![true, true],
            vec![true, true],
            vec![true, true],
            vec![true, true],
        ];
        let lines = octant_from_grid(&grid, OctantStyle::Braille);
        assert_eq!(lines.len(), 1);
    }

    // ── sextant_from_grid ─────────────────────────────────────────────────────

    #[test]
    fn test_sextant_from_grid_empty() {
        let grid: Vec<Vec<bool>> = vec![];
        assert!(sextant_from_grid(&grid, SextantStyle::Full).is_empty());
    }

    #[test]
    fn test_sextant_from_grid_three_rows_single_line() {
        // 3-row × 2-col grid → 1 output line.
        let grid: Vec<Vec<bool>> = vec![vec![true, false], vec![false, true], vec![true, false]];
        let lines = sextant_from_grid(&grid, SextantStyle::Full);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_sextant_from_grid_four_rows_two_lines() {
        // 4-row grid → 2 lines; second line only has top-third data (row 3).
        let mut grid: Vec<Vec<bool>> = vec![vec![false, false]; 4];
        grid[3][0] = true; // row 3, col 0 → top of sextant on line 2
        let lines = sextant_from_grid(&grid, SextantStyle::Full);
        assert_eq!(lines.len(), 2);
        // Line 2 has a filled top-left sextant position.
        assert!(!lines[1].is_empty());
    }

    #[test]
    fn test_sextant_from_grid_trailing_stripped() {
        // A 3-row × 4-col grid with only first 2 cols filled → 1 char on output.
        let grid: Vec<Vec<bool>> = vec![
            vec![true, false, false, false],
            vec![false, false, false, false],
            vec![false, false, false, false],
        ];
        let lines = sextant_from_grid(&grid, SextantStyle::Full);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].chars().count(), 1);
    }

    // ── quadrant_from_grid ────────────────────────────────────────────────────

    #[test]
    fn test_quadrant_from_grid_fn_empty() {
        let grid: Vec<Vec<bool>> = vec![];
        assert!(quadrant_from_grid(&grid, QuadrantStyle::Full).is_empty());
    }

    #[test]
    fn test_quadrant_from_grid_fn_two_rows_single_line() {
        // 2-row × 4-col grid → 1 output line of 2 chars.
        let grid: Vec<Vec<bool>> = vec![
            vec![true, false, false, true],
            vec![false, true, true, false],
        ];
        let lines = quadrant_from_grid(&grid, QuadrantStyle::Full);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].chars().count(), 2);
    }

    #[test]
    fn test_quadrant_from_grid_fn_trailing_stripped() {
        // 2-row × 4-col grid with only first 2 cols filled → 1 char output.
        let grid: Vec<Vec<bool>> = vec![
            vec![true, false, false, false],
            vec![false, false, false, false],
        ];
        let lines = quadrant_from_grid(&grid, QuadrantStyle::Full);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].chars().count(), 1);
    }

    #[test]
    fn test_quadrant_from_grid_fn_four_rows_two_lines() {
        // 4-row grid → 2 output lines.
        let mut grid: Vec<Vec<bool>> = vec![vec![false, false]; 4];
        grid[0][0] = true;
        grid[2][0] = true;
        let lines = quadrant_from_grid(&grid, QuadrantStyle::Full);
        assert_eq!(lines.len(), 2);
    }
}
