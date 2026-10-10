use crate::ipc::sf;
use crate::result::*;
use crate::util;

#[cfg(feature = "services")]
use crate::service;

use nx_derive::{Request, Response};

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum Age {
    Young,
    Normal,
    Old,
    #[default]
    All,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum Gender {
    Male,
    Female,
    #[default]
    All,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum FaceColor {
    Black,
    White,
    Asian,
    #[default]
    All,
}

define_bit_set! {
    SourceFlag (u32) {
        Database = bit!(0),
        Default = bit!(1)
    }
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u32)]
pub enum SpecialKeyCode {
    #[default]
    Normal = 0,
    Special = 0xA523B78F,
}

pub type CreateId = util::Uuid;

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum HairType {
    #[default]
    NormalLong,
    NormalShort,
    NormalMedium,
    NormalExtraLong,
    NormalLongBottom,
    NormalTwoPeaks,
    PartingLong,
    FrontLock,
    PartingShort,
    PartingExtraLongCurved,
    PartingExtraLong,
    PartingMiddleLong,
    PartingSquared,
    PartingLongBottom,
    PeaksTop,
    PeaksSquared,
    PartingPeaks,
    PeaksLongBottom,
    Peaks,
    PeaksRounded,
    PeaksSide,
    PeaksMedium,
    PeaksLong,
    PeaksRoundedLong,
    PartingFrontPeaks,
    PartingLongFront,
    PartingLongRounded,
    PartingFrontPeaksLong,
    PartingExtraLongRounded,
    LongRounded,
    NormalUnknown1,
    NormalUnknown2,
    NormalUnknown3,
    NormalUnknown4,
    NormalUnknown5,
    NormalUnknown6,
    DreadLocks,
    PlatedMats,
    Caps,
    Afro,
    PlatedMatsLong,
    Beanie,
    Short,
    ShortTopLongSide,
    ShortUnknown1,
    ShortUnknown2,
    MilitaryParting,
    Military,
    ShortUnknown3,
    ShortUnknown4,
    ShortUnknown5,
    ShortUnknown6,
    NoneTop,
    None,
    LongUnknown1,
    LongUnknown2,
    LongUnknown3,
    LongUnknown4,
    LongUnknown5,
    LongUnknown6,
    LongUnknown7,
    LongUnknown8,
    LongUnknown9,
    LongUnknown10,
    LongUnknown11,
    LongUnknown12,
    LongUnknown13,
    LongUnknown14,
    LongUnknown15,
    LongUnknown16,
    LongUnknown17,
    LongUnknown18,
    LongUnknown19,
    LongUnknown20,
    LongUnknown21,
    LongUnknown22,
    LongUnknown23,
    LongUnknown24,
    LongUnknown25,
    LongUnknown26,
    LongUnknown27,
    LongUnknown28,
    LongUnknown29,
    LongUnknown30,
    LongUnknown31,
    LongUnknown32,
    LongUnknown33,
    LongUnknown34,
    LongUnknown35,
    LongUnknown36,
    LongUnknown37,
    LongUnknown38,
    LongUnknown39,
    LongUnknown40,
    LongUnknown41,
    LongUnknown42,
    LongUnknown43,
    LongUnknown44,
    LongUnknown45,
    LongUnknown46,
    LongUnknown47,
    LongUnknown48,
    LongUnknown49,
    LongUnknown50,
    LongUnknown51,
    LongUnknown52,
    LongUnknown53,
    LongUnknown54,
    LongUnknown55,
    LongUnknown56,
    LongUnknown57,
    LongUnknown58,
    LongUnknown59,
    LongUnknown60,
    LongUnknown61,
    LongUnknown62,
    LongUnknown63,
    LongUnknown64,
    LongUnknown65,
    LongUnknown66,
    TwoMediumFrontStrandsOneLongBackPonyTail,
    TwoFrontStrandsLongBackPonyTail,
    PartingFrontTwoLongBackPonyTails,
    TwoFrontStrandsOneLongBackPonyTail,
    LongBackPonyTail,
    LongFrontTwoLongBackPonyTails,
    StrandsTwoShortSidedPonyTails,
    TwoMediumSidedPonyTails,
    ShortFrontTwoBackPonyTails,
    TwoShortSidedPonyTails,
    TwoLongSidedPonyTails,
    LongFrontTwoBackPonyTails,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum MoleType {
    #[default]
    None,
    OneDot,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum HairFlip {
    #[default]
    Left,
    Right,
}

pub type CommonColor = u8;

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum EyeType {
    #[default]
    Normal,
    NormalLash,
    WhiteLash,
    WhiteNoBottom,
    OvalAngledWhite,
    AngryWhite,
    DotLashType1,
    Line,
    DotLine,
    OvalWhite,
    RoundedWhite,
    NormalShadow,
    CircleWhite,
    Circle,
    CircleWhiteStroke,
    NormalOvalNoBottom,
    NormalOvalLarge,
    NormalRoundedNoBottom,
    SmallLash,
    Small,
    TwoSmall,
    NormalLongLash,
    WhiteTwoLashes,
    WhiteThreeLashes,
    DotAngry,
    DotAngled,
    Oval,
    SmallWhite,
    WhiteAngledNoBottom,
    WhiteAngledNoLeft,
    SmallWhiteTwoLashes,
    LeafWhiteLash,
    WhiteLargeNoBottom,
    Dot,
    DotLashType2,
    DotThreeLashes,
    WhiteOvalTop,
    WhiteOvalBottom,
    WhiteOvalBottomFlat,
    WhiteOvalTwoLashes,
    WhiteOvalThreeLashes,
    WhiteOvalNoBottomTwoLashes,
    DotWhite,
    WhiteOvalTopFlat,
    WhiteThinLeaf,
    StarThreeLashes,
    LineTwoLashes,
    CrowsFeet,
    WhiteNoBottomFlat,
    WhiteNoBottomRounded,
    WhiteSmallBottomLine,
    WhiteNoBottomLash,
    WhiteNoPartialBottomLash,
    WhiteOvalBottomLine,
    WhiteNoBottomLashTopLine,
    WhiteNoPartialBottomTwoLashes,
    NormalTopLine,
    WhiteOvalLash,
    RoundTired,
    WhiteLarge,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum MouthType {
    #[default]
    Neutral,
    NeutralLips,
    Smile,
    SmileStroke,
    SmileTeeth,
    LipsSmall,
    LipsLarge,
    Wave,
    WaveAngrySmall,
    NeutralStrokeLarge,
    TeethSurprised,
    LipsExtraLarge,
    LipsUp,
    NeutralDown,
    Surprised,
    TeethMiddle,
    NeutralStroke,
    LipsExtraSmall,
    Malicious,
    LipsDual,
    NeutralComma,
    NeutralUp,
    TeethLarge,
    WaveAngry,
    LipsSexy,
    SmileInverted,
    LipsSexyOutline,
    SmileRounded,
    LipsTeeth,
    NeutralOpen,
    TeethRounded,
    WaveAngrySmallInverted,
    NeutralCommaInverted,
    TeethFull,
    SmileDownLine,
    Kiss,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum FontRegion {
    #[default]
    Standard,
    China,
    Korea,
    Taiwan,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum FacelineType {
    #[default]
    Sharp,
    Rounded,
    SharpRounded,
    SharpRoundedSmall,
    Large,
    LargeRounded,
    SharpSmall,
    Flat,
    Bump,
    Angular,
    FlatRounded,
    AngularSmall,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum FacelineColor {
    #[default]
    Beige,
    WarmBeige,
    Natural,
    Honey,
    Chestnut,
    Porcelain,
    Ivory,
    WarmIvory,
    Almond,
    Espresso,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum FacelineWrinkle {
    #[default]
    None,
    TearTroughs,
    FacialPain,
    Cheeks,
    Folds,
    UnderTheEyes,
    SplitChin,
    Chin,
    BrowDroop,
    MouthFrown,
    CrowsFeet,
    FoldsCrowsFrown,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum FacelineMake {
    #[default]
    None,
    CheekPorcelain,
    CheekNatural,
    EyeShadowBlue,
    CheekBlushPorcelain,
    CheekBlushNatural,
    CheekPorcelainEyeShadowBlue,
    CheekPorcelainEyeShadowNatural,
    CheekBlushPorcelainEyeShadowEspresso,
    Freckles,
    LionsManeBeard,
    StubbleBeard,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum EyebrowType {
    #[default]
    FlatAngledLarge,
    LowArchRoundedThin,
    SoftAngledLarge,
    MediumArchRoundedThin,
    RoundedMedium,
    LowArchMedium,
    RoundedThin,
    UpThin,
    MediumArchRoundedMedium,
    RoundedLarge,
    UpLarge,
    FlatAngledLargeInverted,
    MediumArchFlat,
    AngledThin,
    HorizontalLarge,
    HighArchFlat,
    Flat,
    MediumArchLarge,
    LowArchThin,
    RoundedThinInverted,
    HighArchLarge,
    Hairy,
    Dotted,
    None,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum NoseType {
    #[default]
    Normal,
    Rounded,
    Dot,
    Arrow,
    Roman,
    Triangle,
    Button,
    RoundedInverted,
    Potato,
    Grecian,
    Snub,
    Aquiline,
    ArrowLeft,
    RoundedLarge,
    Hooked,
    Fat,
    Droopy,
    ArrowLarge,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum BeardType {
    #[default]
    None,
    Goatee,
    GoateeLong,
    LionsManeLong,
    LionsMane,
    Full,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum MustacheType {
    #[default]
    None,
    Walrus,
    Pencil,
    Horseshoe,
    Normal,
    Toothbrush,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum GlassType {
    #[default]
    None,
    Oval,
    Wayfarer,
    Rectangle,
    TopRimless,
    Rounded,
    Oversized,
    CatEye,
    Square,
    BottomRimless,
    SemiOpaqueRounded,
    SemiOpaqueCatEye,
    SemiOpaqueOval,
    SemiOpaqueRectangle,
    SemiOpaqueAviator,
    OpaqueRounded,
    OpaqueCatEye,
    OpaqueOval,
    OpaqueRectangle,
    OpaqueAviator,
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct CharInfo {
    pub id: CreateId,
    pub name: util::ArrayWideString<11>,
    pub font_region: FontRegion,
    pub favorite_color: u8,
    pub gender: Gender,
    pub height: u8,
    pub build: u8,
    pub type_val: u8,
    pub region_move: u8,
    pub faceline_type: FacelineType,
    pub faceline_color: FacelineColor,
    pub faceline_wrinkle: FacelineWrinkle,
    pub faceline_make: FacelineMake,
    pub hair_type: HairType,
    pub hair_color: CommonColor,
    pub hair_flip: HairFlip,
    pub eye_type: EyeType,
    pub eye_color: CommonColor,
    pub eye_scale: u8,
    pub eye_aspect: u8,
    pub eye_rotate: u8,
    pub eye_x: u8,
    pub eye_y: u8,
    pub eyebrow_type: EyebrowType,
    pub eyebrow_color: CommonColor,
    pub eyebrow_scale: u8,
    pub eyebrow_aspect: u8,
    pub eyebrow_rotate: u8,
    pub eyebrow_x: u8,
    pub eyebrow_y: u8,
    pub nose_type: NoseType,
    pub nose_scale: u8,
    pub nose_y: u8,
    pub mouth_type: MouthType,
    pub mouth_color: CommonColor,
    pub mouth_scale: u8,
    pub mouth_aspect: u8,
    pub mouth_y: u8,
    pub beard_color: CommonColor,
    pub beard_type: BeardType,
    pub mustache_type: MustacheType,
    pub mustache_scale: u8,
    pub mustache_y: u8,
    pub glass_type: GlassType,
    pub glass_color: CommonColor,
    pub glass_scale: u8,
    pub glass_y: u8,
    pub mole_type: MoleType,
    pub mole_scale: u8,
    pub mole_x: u8,
    pub mole_y: u8,
    pub reserved: u8,
}
const_assert!(core::mem::size_of::<CharInfo>() == 0x58);

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum CoreDataElement {
    HairType,
    Height,
    MoleType,
    Build,
    HairFlip,
    HairColor,
    Type,
    EyeColor,
    Gender,
    EyebrowColor,
    MouthColor,
    BeardColor,
    GlassColor,
    EyeType,
    RegionMove,
    MouthType,
    FontRegion,
    EyeY,
    GlassScale,
    EyebrowType,
    MustacheType,
    NoseType,
    BeardType,
    NoseY,
    MouthAspect,
    MouthY,
    EyebrowAspect,
    MustacheY,
    EyeRotate,
    GlassY,
    EyeAspect,
    MoleX,
    EyeScale,
    MoleY,
    GlassType,
    FavoriteColor,
    FacelineType,
    FacelineColor,
    FacelineWrinkle,
    FacelineMake,
    EyeX,
    EyebrowScale,
    EyebrowRotate,
    EyebrowX,
    EyebrowY,
    NoseScale,
    MouthScale,
    MustacheScale,
    MoleScale,
}

#[derive(Request, Response, Copy, Clone)]
#[repr(C)]
pub struct CoreDataElementInfo {
    pub byte_offset: u32,
    pub bit_offset: u32,
    pub bit_width: u32,
    pub min_value: u32,
    pub max_value: u32,
    pub unk: u32,
}

pub const fn get_element_info(elm: CoreDataElement) -> CoreDataElementInfo {
    match elm {
        CoreDataElement::HairType => CoreDataElementInfo {
            byte_offset: 0x0,
            bit_offset: 0,
            bit_width: 8,
            min_value: 0,
            max_value: 0x83,
            unk: 1,
        },
        CoreDataElement::Height => CoreDataElementInfo {
            byte_offset: 0x1,
            bit_offset: 0,
            bit_width: 7,
            min_value: 0,
            max_value: 0x7F,
            unk: 0,
        },
        CoreDataElement::MoleType => CoreDataElementInfo {
            byte_offset: 0x1,
            bit_offset: 7,
            bit_width: 1,
            min_value: 0,
            max_value: 0x1,
            unk: 0,
        },
        CoreDataElement::Build => CoreDataElementInfo {
            byte_offset: 0x2,
            bit_offset: 0,
            bit_width: 7,
            min_value: 0,
            max_value: 0x7F,
            unk: 0,
        },
        CoreDataElement::HairFlip => CoreDataElementInfo {
            byte_offset: 0x2,
            bit_offset: 7,
            bit_width: 1,
            min_value: 0,
            max_value: 0x1,
            unk: 0,
        },
        CoreDataElement::HairColor => CoreDataElementInfo {
            byte_offset: 0x3,
            bit_offset: 0,
            bit_width: 7,
            min_value: 0,
            max_value: 0x63,
            unk: 1,
        },
        CoreDataElement::Type => CoreDataElementInfo {
            byte_offset: 0x3,
            bit_offset: 7,
            bit_width: 1,
            min_value: 0,
            max_value: 0x1,
            unk: 0,
        },
        CoreDataElement::EyeColor => CoreDataElementInfo {
            byte_offset: 0x4,
            bit_offset: 0,
            bit_width: 7,
            min_value: 0,
            max_value: 0x63,
            unk: 1,
        },
        CoreDataElement::Gender => CoreDataElementInfo {
            byte_offset: 0x4,
            bit_offset: 7,
            bit_width: 1,
            min_value: 0,
            max_value: 0x1,
            unk: 0,
        },
        CoreDataElement::EyebrowColor => CoreDataElementInfo {
            byte_offset: 0x5,
            bit_offset: 0,
            bit_width: 7,
            min_value: 0,
            max_value: 0x63,
            unk: 1,
        },
        CoreDataElement::MouthColor => CoreDataElementInfo {
            byte_offset: 0x6,
            bit_offset: 0,
            bit_width: 7,
            min_value: 0,
            max_value: 0x63,
            unk: 1,
        },
        CoreDataElement::BeardColor => CoreDataElementInfo {
            byte_offset: 0x7,
            bit_offset: 0,
            bit_width: 7,
            min_value: 0,
            max_value: 0x63,
            unk: 1,
        },
        CoreDataElement::GlassColor => CoreDataElementInfo {
            byte_offset: 0x8,
            bit_offset: 0,
            bit_width: 7,
            min_value: 0,
            max_value: 0x63,
            unk: 1,
        },
        CoreDataElement::EyeType => CoreDataElementInfo {
            byte_offset: 0x9,
            bit_offset: 0,
            bit_width: 6,
            min_value: 0,
            max_value: 0x3B,
            unk: 1,
        },
        CoreDataElement::RegionMove => CoreDataElementInfo {
            byte_offset: 0x9,
            bit_offset: 6,
            bit_width: 2,
            min_value: 0,
            max_value: 0x3,
            unk: 0,
        },
        CoreDataElement::MouthType => CoreDataElementInfo {
            byte_offset: 0xA,
            bit_offset: 0,
            bit_width: 6,
            min_value: 0,
            max_value: 0x23,
            unk: 1,
        },
        CoreDataElement::FontRegion => CoreDataElementInfo {
            byte_offset: 0xA,
            bit_offset: 6,
            bit_width: 2,
            min_value: 0,
            max_value: 0x3,
            unk: 0,
        },
        CoreDataElement::EyeY => CoreDataElementInfo {
            byte_offset: 0xB,
            bit_offset: 0,
            bit_width: 5,
            min_value: 0,
            max_value: 0x12,
            unk: 1,
        },
        CoreDataElement::GlassScale => CoreDataElementInfo {
            byte_offset: 0xB,
            bit_offset: 5,
            bit_width: 3,
            min_value: 0,
            max_value: 0x7,
            unk: 0,
        },
        CoreDataElement::EyebrowType => CoreDataElementInfo {
            byte_offset: 0xC,
            bit_offset: 0,
            bit_width: 5,
            min_value: 0,
            max_value: 0x17,
            unk: 1,
        },
        CoreDataElement::MustacheType => CoreDataElementInfo {
            byte_offset: 0xC,
            bit_offset: 5,
            bit_width: 3,
            min_value: 0,
            max_value: 0x5,
            unk: 1,
        },
        CoreDataElement::NoseType => CoreDataElementInfo {
            byte_offset: 0xD,
            bit_offset: 0,
            bit_width: 5,
            min_value: 0,
            max_value: 0x11,
            unk: 1,
        },
        CoreDataElement::BeardType => CoreDataElementInfo {
            byte_offset: 0xD,
            bit_offset: 5,
            bit_width: 3,
            min_value: 0,
            max_value: 0x5,
            unk: 1,
        },
        CoreDataElement::NoseY => CoreDataElementInfo {
            byte_offset: 0xE,
            bit_offset: 0,
            bit_width: 5,
            min_value: 0,
            max_value: 0x12,
            unk: 1,
        },
        CoreDataElement::MouthAspect => CoreDataElementInfo {
            byte_offset: 0xE,
            bit_offset: 5,
            bit_width: 3,
            min_value: 0,
            max_value: 0x6,
            unk: 1,
        },
        CoreDataElement::MouthY => CoreDataElementInfo {
            byte_offset: 0xF,
            bit_offset: 0,
            bit_width: 5,
            min_value: 0,
            max_value: 0x12,
            unk: 1,
        },
        CoreDataElement::EyebrowAspect => CoreDataElementInfo {
            byte_offset: 0xF,
            bit_offset: 5,
            bit_width: 3,
            min_value: 0,
            max_value: 0x6,
            unk: 1,
        },
        CoreDataElement::MustacheY => CoreDataElementInfo {
            byte_offset: 0x10,
            bit_offset: 0,
            bit_width: 5,
            min_value: 0,
            max_value: 0x10,
            unk: 1,
        },
        CoreDataElement::EyeRotate => CoreDataElementInfo {
            byte_offset: 0x10,
            bit_offset: 5,
            bit_width: 3,
            min_value: 0,
            max_value: 0x7,
            unk: 0,
        },
        CoreDataElement::GlassY => CoreDataElementInfo {
            byte_offset: 0x11,
            bit_offset: 0,
            bit_width: 5,
            min_value: 0,
            max_value: 0x14,
            unk: 1,
        },
        CoreDataElement::EyeAspect => CoreDataElementInfo {
            byte_offset: 0x11,
            bit_offset: 5,
            bit_width: 3,
            min_value: 0,
            max_value: 0x6,
            unk: 1,
        },
        CoreDataElement::MoleX => CoreDataElementInfo {
            byte_offset: 0x12,
            bit_offset: 0,
            bit_width: 5,
            min_value: 0,
            max_value: 0x10,
            unk: 1,
        },
        CoreDataElement::EyeScale => CoreDataElementInfo {
            byte_offset: 0x12,
            bit_offset: 5,
            bit_width: 3,
            min_value: 0,
            max_value: 0x7,
            unk: 0,
        },
        CoreDataElement::MoleY => CoreDataElementInfo {
            byte_offset: 0x13,
            bit_offset: 0,
            bit_width: 5,
            min_value: 0,
            max_value: 0x1E,
            unk: 1,
        },
        CoreDataElement::GlassType => CoreDataElementInfo {
            byte_offset: 0x14,
            bit_offset: 0,
            bit_width: 5,
            min_value: 0,
            max_value: 0x13,
            unk: 1,
        },
        CoreDataElement::FavoriteColor => CoreDataElementInfo {
            byte_offset: 0x15,
            bit_offset: 0,
            bit_width: 4,
            min_value: 0,
            max_value: 0xB,
            unk: 1,
        },
        CoreDataElement::FacelineType => CoreDataElementInfo {
            byte_offset: 0x15,
            bit_offset: 4,
            bit_width: 4,
            min_value: 0,
            max_value: 0xB,
            unk: 1,
        },
        CoreDataElement::FacelineColor => CoreDataElementInfo {
            byte_offset: 0x16,
            bit_offset: 0,
            bit_width: 4,
            min_value: 0,
            max_value: 0x9,
            unk: 1,
        },
        CoreDataElement::FacelineWrinkle => CoreDataElementInfo {
            byte_offset: 0x16,
            bit_offset: 4,
            bit_width: 4,
            min_value: 0,
            max_value: 0xB,
            unk: 1,
        },
        CoreDataElement::FacelineMake => CoreDataElementInfo {
            byte_offset: 0x17,
            bit_offset: 0,
            bit_width: 4,
            min_value: 0,
            max_value: 0xB,
            unk: 1,
        },

        CoreDataElement::EyeX => CoreDataElementInfo {
            byte_offset: 0x17,
            bit_offset: 4,
            bit_width: 4,
            min_value: 0,
            max_value: 0xC,
            unk: 1,
        },
        CoreDataElement::EyebrowScale => CoreDataElementInfo {
            byte_offset: 0x18,
            bit_offset: 0,
            bit_width: 4,
            min_value: 0,
            max_value: 0x8,
            unk: 1,
        },
        CoreDataElement::EyebrowRotate => CoreDataElementInfo {
            byte_offset: 0x18,
            bit_offset: 4,
            bit_width: 4,
            min_value: 0,
            max_value: 0xB,
            unk: 1,
        },
        CoreDataElement::EyebrowX => CoreDataElementInfo {
            byte_offset: 0x19,
            bit_offset: 0,
            bit_width: 4,
            min_value: 0,
            max_value: 0xC,
            unk: 1,
        },
        CoreDataElement::EyebrowY => CoreDataElementInfo {
            byte_offset: 0x19,
            bit_offset: 4,
            bit_width: 4,
            min_value: 0x3,
            max_value: 0x12,
            unk: 0,
        },
        CoreDataElement::NoseScale => CoreDataElementInfo {
            byte_offset: 0x1A,
            bit_offset: 0,
            bit_width: 4,
            min_value: 0,
            max_value: 0x8,
            unk: 1,
        },
        CoreDataElement::MouthScale => CoreDataElementInfo {
            byte_offset: 0x1A,
            bit_offset: 4,
            bit_width: 4,
            min_value: 0,
            max_value: 0x8,
            unk: 1,
        },
        CoreDataElement::MustacheScale => CoreDataElementInfo {
            byte_offset: 0x1B,
            bit_offset: 0,
            bit_width: 4,
            min_value: 0,
            max_value: 0x8,
            unk: 1,
        },
        CoreDataElement::MoleScale => CoreDataElementInfo {
            byte_offset: 0x1B,
            bit_offset: 4,
            bit_width: 4,
            min_value: 0,
            max_value: 0x8,
            unk: 1,
        },
    }
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct CoreData {
    pub data: [u8; 0x1C],
    pub name: util::ArrayWideString<10>,
}
const_assert!(core::mem::size_of::<CoreData>() == 0x30);

impl CoreData {
    #[inline]
    fn get_value(&self, info: CoreDataElementInfo) -> u32 {
        ((self.data[info.byte_offset as usize] as u32 >> info.bit_offset)
            & !(u32::MAX << info.bit_width))
            + info.min_value
    }

    #[inline]
    fn set_value(&mut self, info: CoreDataElementInfo, val: u32) {
        let new_val = (self.data[info.byte_offset as usize] as u32
            & !(!(u32::MAX << info.bit_width) << info.bit_offset)
            | (((val - info.min_value) & !(u32::MAX << info.bit_width)) << info.bit_offset))
            as u8;
        self.data[info.byte_offset as usize] = new_val;
    }

    // TODO: order these?

    #[inline]
    pub fn get_hair_type(&self) -> HairType {
        unsafe {
            core::mem::transmute(self.get_value(get_element_info(CoreDataElement::HairType)) as u8)
        }
    }

    #[inline]
    pub fn set_hair_type(&mut self, hair_type: HairType) {
        self.set_value(
            get_element_info(CoreDataElement::HairType),
            hair_type as u32,
        )
    }

    #[inline]
    pub fn get_height(&self) -> u8 {
        self.get_value(get_element_info(CoreDataElement::Height)) as u8
    }

    #[inline]
    pub fn set_height(&mut self, height: u8) {
        self.set_value(get_element_info(CoreDataElement::Height), height as u32)
    }

    #[inline]
    pub fn get_mole_type(&self) -> MoleType {
        unsafe {
            core::mem::transmute(self.get_value(get_element_info(CoreDataElement::MoleType)) as u8)
        }
    }

    #[inline]
    pub fn set_mole_type(&mut self, mole_type: MoleType) {
        self.set_value(
            get_element_info(CoreDataElement::MoleType),
            mole_type as u32,
        )
    }

    #[inline]
    pub fn get_build(&self) -> u8 {
        self.get_value(get_element_info(CoreDataElement::Build)) as u8
    }

    #[inline]
    pub fn set_build(&mut self, build: u8) {
        self.set_value(get_element_info(CoreDataElement::Build), build as u32)
    }

    #[inline]
    pub fn get_hair_flip(&self) -> HairFlip {
        unsafe {
            core::mem::transmute(self.get_value(get_element_info(CoreDataElement::HairFlip)) as u8)
        }
    }

    #[inline]
    pub fn set_hair_flip(&mut self, hair_flip: HairFlip) {
        self.set_value(
            get_element_info(CoreDataElement::HairFlip),
            hair_flip as u32,
        )
    }

    #[inline]
    pub fn get_hair_color(&self) -> CommonColor {
        self.get_value(get_element_info(CoreDataElement::HairColor)) as u8
    }

    #[inline]
    pub fn set_hair_color(&mut self, color: CommonColor) {
        self.set_value(get_element_info(CoreDataElement::HairColor), color as u32)
    }

    #[inline]
    pub fn get_type(&self) -> u8 {
        self.get_value(get_element_info(CoreDataElement::Type)) as u8
    }

    #[inline]
    pub fn set_type(&mut self, type_val: u8) {
        self.set_value(get_element_info(CoreDataElement::Type), type_val as u32)
    }

    #[inline]
    pub fn get_eye_color(&self) -> u8 {
        self.get_value(get_element_info(CoreDataElement::EyeColor)) as u8
    }

    #[inline]
    pub fn set_eye_color(&mut self, color: u8) {
        self.set_value(get_element_info(CoreDataElement::EyeColor), color as u32)
    }

    #[inline]
    pub fn get_gender(&self) -> Gender {
        unsafe {
            core::mem::transmute(self.get_value(get_element_info(CoreDataElement::Gender)) as u8)
        }
    }

    #[inline]
    pub fn set_gender(&mut self, gender: Gender) {
        self.set_value(get_element_info(CoreDataElement::Gender), gender as u32)
    }

    #[inline]
    pub fn get_eyebrow_color(&self) -> u8 {
        self.get_value(get_element_info(CoreDataElement::EyebrowColor)) as u8
    }

    #[inline]
    pub fn set_eyebrow_color(&mut self, color: u8) {
        self.set_value(
            get_element_info(CoreDataElement::EyebrowColor),
            color as u32,
        )
    }

    #[inline]
    pub fn get_mouth_color(&self) -> u8 {
        self.get_value(get_element_info(CoreDataElement::MouthColor)) as u8
    }

    #[inline]
    pub fn set_mouth_color(&mut self, color: u8) {
        self.set_value(get_element_info(CoreDataElement::MouthColor), color as u32)
    }

    #[inline]
    pub fn get_beard_color(&self) -> u8 {
        self.get_value(get_element_info(CoreDataElement::BeardColor)) as u8
    }

    #[inline]
    pub fn set_beard_color(&mut self, color: u8) {
        self.set_value(get_element_info(CoreDataElement::BeardColor), color as u32)
    }

    #[inline]
    pub fn get_glass_color(&self) -> u8 {
        self.get_value(get_element_info(CoreDataElement::GlassColor)) as u8
    }

    #[inline]
    pub fn set_glass_color(&mut self, color: u8) {
        self.set_value(get_element_info(CoreDataElement::GlassColor), color as u32)
    }

    #[inline]
    pub fn get_eye_type(&self) -> EyeType {
        unsafe {
            core::mem::transmute(self.get_value(get_element_info(CoreDataElement::EyeType)) as u8)
        }
    }

    #[inline]
    pub fn set_eye_type(&mut self, eye_type: EyeType) {
        self.set_value(get_element_info(CoreDataElement::EyeType), eye_type as u32)
    }

    #[inline]
    pub fn get_region_move(&self) -> u8 {
        self.get_value(get_element_info(CoreDataElement::RegionMove)) as u8
    }

    #[inline]
    pub fn set_region_move(&mut self, region_move: u8) {
        self.set_value(
            get_element_info(CoreDataElement::RegionMove),
            region_move as u32,
        )
    }

    #[inline]
    pub fn get_mouth_type(&self) -> MouthType {
        unsafe {
            core::mem::transmute(self.get_value(get_element_info(CoreDataElement::MouthType)) as u8)
        }
    }

    #[inline]
    pub fn set_mouth_type(&mut self, mouth_type: MouthType) {
        self.set_value(
            get_element_info(CoreDataElement::MouthType),
            mouth_type as u32,
        )
    }

    #[inline]
    pub fn get_font_region(&self) -> FontRegion {
        unsafe {
            core::mem::transmute(
                self.get_value(get_element_info(CoreDataElement::FontRegion)) as u8,
            )
        }
    }

    #[inline]
    pub fn set_font_region(&mut self, font_region: FontRegion) {
        self.set_value(
            get_element_info(CoreDataElement::FontRegion),
            font_region as u32,
        )
    }

    #[inline]
    pub fn get_eye_y(&self) -> u8 {
        self.get_value(get_element_info(CoreDataElement::EyeY)) as u8
    }

    #[inline]
    pub fn set_eye_y(&mut self, y: u8) {
        self.set_value(get_element_info(CoreDataElement::EyeY), y as u32)
    }

    #[inline]
    pub fn get_glass_scale(&self) -> u8 {
        self.get_value(get_element_info(CoreDataElement::GlassScale)) as u8
    }

    #[inline]
    pub fn set_glass_scale(&mut self, scale: u8) {
        self.set_value(get_element_info(CoreDataElement::GlassScale), scale as u32)
    }

    #[inline]
    pub fn get_eyebrow_type(&self) -> EyebrowType {
        unsafe {
            core::mem::transmute(
                self.get_value(get_element_info(CoreDataElement::EyebrowType)) as u8,
            )
        }
    }

    #[inline]
    pub fn set_eyebrow_type(&mut self, eyebrow_type: EyebrowType) {
        self.set_value(
            get_element_info(CoreDataElement::EyebrowType),
            eyebrow_type as u32,
        )
    }

    #[inline]
    pub fn get_mustache_type(&self) -> MustacheType {
        unsafe {
            core::mem::transmute(
                self.get_value(get_element_info(CoreDataElement::MustacheType)) as u8,
            )
        }
    }

    #[inline]
    pub fn set_mustache_type(&mut self, mustache_type: MustacheType) {
        self.set_value(
            get_element_info(CoreDataElement::MustacheType),
            mustache_type as u32,
        )
    }

    #[inline]
    pub fn get_nose_type(&self) -> NoseType {
        unsafe {
            core::mem::transmute(self.get_value(get_element_info(CoreDataElement::NoseType)) as u8)
        }
    }

    #[inline]
    pub fn set_nose_type(&mut self, nose_type: NoseType) {
        self.set_value(
            get_element_info(CoreDataElement::NoseType),
            nose_type as u32,
        )
    }

    #[inline]
    pub fn get_beard_type(&self) -> BeardType {
        unsafe {
            core::mem::transmute(self.get_value(get_element_info(CoreDataElement::BeardType)) as u8)
        }
    }

    #[inline]
    pub fn set_beard_type(&mut self, beard_type: BeardType) {
        self.set_value(
            get_element_info(CoreDataElement::BeardType),
            beard_type as u32,
        )
    }

    #[inline]
    pub fn get_nose_y(&self) -> u8 {
        self.get_value(get_element_info(CoreDataElement::NoseY)) as u8
    }

    #[inline]
    pub fn set_nose_y(&mut self, y: u8) {
        self.set_value(get_element_info(CoreDataElement::NoseY), y as u32)
    }

    #[inline]
    pub fn get_mouth_aspect(&self) -> u8 {
        self.get_value(get_element_info(CoreDataElement::MouthAspect)) as u8
    }

    #[inline]
    pub fn set_mouth_aspect(&mut self, aspect: u8) {
        self.set_value(
            get_element_info(CoreDataElement::MouthAspect),
            aspect as u32,
        )
    }

    #[inline]
    pub fn get_mouth_y(&self) -> u8 {
        self.get_value(get_element_info(CoreDataElement::MouthY)) as u8
    }

    #[inline]
    pub fn set_mouth_y(&mut self, y: u8) {
        self.set_value(get_element_info(CoreDataElement::MouthY), y as u32)
    }

    #[inline]
    pub fn get_eyebrow_aspect(&self) -> u8 {
        self.get_value(get_element_info(CoreDataElement::EyebrowAspect)) as u8
    }

    #[inline]
    pub fn set_eyebrow_aspect(&mut self, aspect: u8) {
        self.set_value(
            get_element_info(CoreDataElement::EyebrowAspect),
            aspect as u32,
        )
    }

    #[inline]
    pub fn get_mustache_y(&self) -> u8 {
        self.get_value(get_element_info(CoreDataElement::MustacheY)) as u8
    }

    #[inline]
    pub fn set_mustache_y(&mut self, y: u8) {
        self.set_value(get_element_info(CoreDataElement::MustacheY), y as u32)
    }

    #[inline]
    pub fn get_eye_rotate(&self) -> u8 {
        self.get_value(get_element_info(CoreDataElement::EyeRotate)) as u8
    }

    #[inline]
    pub fn set_eye_rotate(&mut self, rotate: u8) {
        self.set_value(get_element_info(CoreDataElement::EyeRotate), rotate as u32)
    }

    #[inline]
    pub fn get_glass_y(&self) -> u8 {
        self.get_value(get_element_info(CoreDataElement::GlassY)) as u8
    }

    #[inline]
    pub fn set_glass_y(&mut self, y: u8) {
        self.set_value(get_element_info(CoreDataElement::GlassY), y as u32)
    }

    #[inline]
    pub fn get_eye_aspect(&self) -> u8 {
        self.get_value(get_element_info(CoreDataElement::EyeAspect)) as u8
    }

    #[inline]
    pub fn set_eye_aspect(&mut self, aspect: u8) {
        self.set_value(get_element_info(CoreDataElement::EyeAspect), aspect as u32)
    }

    #[inline]
    pub fn get_mole_x(&self) -> u8 {
        self.get_value(get_element_info(CoreDataElement::MoleX)) as u8
    }

    #[inline]
    pub fn set_mole_x(&mut self, x: u8) {
        self.set_value(get_element_info(CoreDataElement::MoleX), x as u32)
    }

    #[inline]
    pub fn get_eye_scale(&self) -> u8 {
        self.get_value(get_element_info(CoreDataElement::EyeScale)) as u8
    }

    #[inline]
    pub fn set_eye_scale(&mut self, scale: u8) {
        self.set_value(get_element_info(CoreDataElement::EyeScale), scale as u32)
    }

    #[inline]
    pub fn get_mole_y(&self) -> u8 {
        self.get_value(get_element_info(CoreDataElement::MoleY)) as u8
    }

    #[inline]
    pub fn set_mole_y(&mut self, y: u8) {
        self.set_value(get_element_info(CoreDataElement::MoleY), y as u32)
    }

    #[inline]
    pub fn get_glass_type(&self) -> GlassType {
        unsafe {
            core::mem::transmute(self.get_value(get_element_info(CoreDataElement::GlassType)) as u8)
        }
    }

    #[inline]
    pub fn set_glass_type(&mut self, glass_type: GlassType) {
        self.set_value(
            get_element_info(CoreDataElement::GlassType),
            glass_type as u32,
        )
    }

    #[inline]
    pub fn get_favorite_color(&self) -> u8 {
        self.get_value(get_element_info(CoreDataElement::FavoriteColor)) as u8
    }

    #[inline]
    pub fn set_favorite_color(&mut self, favorite_color: u8) {
        self.set_value(
            get_element_info(CoreDataElement::FavoriteColor),
            favorite_color as u32,
        )
    }

    #[inline]
    pub fn get_faceline_type(&self) -> FacelineType {
        unsafe {
            core::mem::transmute(
                self.get_value(get_element_info(CoreDataElement::FacelineType)) as u8,
            )
        }
    }

    #[inline]
    pub fn set_faceline_type(&mut self, faceline_type: FacelineType) {
        self.set_value(
            get_element_info(CoreDataElement::FacelineType),
            faceline_type as u32,
        )
    }

    #[inline]
    pub fn get_faceline_color(&self) -> FacelineColor {
        unsafe {
            core::mem::transmute(
                self.get_value(get_element_info(CoreDataElement::FacelineColor)) as u8,
            )
        }
    }

    #[inline]
    pub fn set_faceline_color(&mut self, faceline_color: FacelineColor) {
        self.set_value(
            get_element_info(CoreDataElement::FacelineColor),
            faceline_color as u32,
        )
    }

    #[inline]
    pub fn get_faceline_wrinkle(&self) -> FacelineWrinkle {
        unsafe {
            core::mem::transmute(
                self.get_value(get_element_info(CoreDataElement::FacelineWrinkle)) as u8,
            )
        }
    }

    #[inline]
    pub fn set_faceline_wrinkle(&mut self, faceline_wrinkle: FacelineWrinkle) {
        self.set_value(
            get_element_info(CoreDataElement::FacelineWrinkle),
            faceline_wrinkle as u32,
        )
    }

    #[inline]
    pub fn get_faceline_make(&self) -> FacelineMake {
        unsafe {
            core::mem::transmute(
                self.get_value(get_element_info(CoreDataElement::FacelineMake)) as u8,
            )
        }
    }

    #[inline]
    pub fn set_faceline_make(&mut self, faceline_make: FacelineMake) {
        self.set_value(
            get_element_info(CoreDataElement::FacelineMake),
            faceline_make as u32,
        )
    }

    #[inline]
    pub fn get_eye_x(&self) -> u8 {
        self.get_value(get_element_info(CoreDataElement::EyeX)) as u8
    }

    #[inline]
    pub fn set_eye_x(&mut self, x: u8) {
        self.set_value(get_element_info(CoreDataElement::EyeX), x as u32)
    }

    #[inline]
    pub fn get_eyebrow_scale(&self) -> u8 {
        self.get_value(get_element_info(CoreDataElement::EyebrowScale)) as u8
    }

    #[inline]
    pub fn set_eyebrow_scale(&mut self, scale: u8) {
        self.set_value(
            get_element_info(CoreDataElement::EyebrowScale),
            scale as u32,
        )
    }

    #[inline]
    pub fn get_eyebrow_rotate(&self) -> u8 {
        self.get_value(get_element_info(CoreDataElement::EyebrowRotate)) as u8
    }

    #[inline]
    pub fn set_eyebrow_rotate(&mut self, rotate: u8) {
        self.set_value(
            get_element_info(CoreDataElement::EyebrowRotate),
            rotate as u32,
        )
    }

    #[inline]
    pub fn get_eyebrow_x(&self) -> u8 {
        self.get_value(get_element_info(CoreDataElement::EyebrowX)) as u8
    }

    #[inline]
    pub fn set_eyebrow_x(&mut self, x: u8) {
        self.set_value(get_element_info(CoreDataElement::EyebrowX), x as u32)
    }

    #[inline]
    pub fn get_eyebrow_y(&self) -> u8 {
        self.get_value(get_element_info(CoreDataElement::EyebrowY)) as u8
    }

    #[inline]
    pub fn set_eyebrow_y(&mut self, y: u8) {
        self.set_value(get_element_info(CoreDataElement::EyebrowY), y as u32)
    }

    #[inline]
    pub fn get_nose_scale(&self) -> u8 {
        self.get_value(get_element_info(CoreDataElement::NoseScale)) as u8
    }

    #[inline]
    pub fn set_nose_scale(&mut self, scale: u8) {
        self.set_value(get_element_info(CoreDataElement::NoseScale), scale as u32)
    }

    #[inline]
    pub fn get_mouth_scale(&self) -> u8 {
        self.get_value(get_element_info(CoreDataElement::MouthScale)) as u8
    }

    #[inline]
    pub fn set_mouth_scale(&mut self, scale: u8) {
        self.set_value(get_element_info(CoreDataElement::MouthScale), scale as u32)
    }

    #[inline]
    pub fn get_mustache_scale(&self) -> u8 {
        self.get_value(get_element_info(CoreDataElement::MustacheScale)) as u8
    }

    #[inline]
    pub fn set_mustache_scale(&mut self, scale: u8) {
        self.set_value(
            get_element_info(CoreDataElement::MustacheScale),
            scale as u32,
        )
    }

    #[inline]
    pub fn get_mole_scale(&self) -> u8 {
        self.get_value(get_element_info(CoreDataElement::MoleScale)) as u8
    }

    #[inline]
    pub fn set_mole_scale(&mut self, scale: u8) {
        self.set_value(get_element_info(CoreDataElement::MoleScale), scale as u32)
    }

    pub fn from_charinfo(char_info: CharInfo) -> Result<Self> {
        let mut core_data = Self::default();

        core_data.name.set_string(char_info.name.get_string()?);
        core_data.set_font_region(char_info.font_region);
        core_data.set_favorite_color(char_info.favorite_color);
        core_data.set_gender(char_info.gender);
        core_data.set_height(char_info.height);
        core_data.set_build(char_info.build);
        core_data.set_type(char_info.type_val);
        core_data.set_region_move(char_info.region_move);
        core_data.set_faceline_type(char_info.faceline_type);
        core_data.set_faceline_color(char_info.faceline_color);
        core_data.set_faceline_make(char_info.faceline_make);
        core_data.set_hair_type(char_info.hair_type);
        core_data.set_hair_color(char_info.hair_color);
        core_data.set_hair_flip(char_info.hair_flip);
        core_data.set_eye_type(char_info.eye_type);
        core_data.set_eye_color(char_info.eye_color);
        core_data.set_eye_scale(char_info.eye_scale);
        core_data.set_eye_aspect(char_info.eye_aspect);
        core_data.set_eye_rotate(char_info.eye_rotate);
        core_data.set_eye_x(char_info.eye_x);
        core_data.set_eye_y(char_info.eye_y);
        core_data.set_eyebrow_type(char_info.eyebrow_type);
        core_data.set_eyebrow_color(char_info.eyebrow_color);
        core_data.set_eyebrow_scale(char_info.eyebrow_scale);
        core_data.set_eyebrow_aspect(char_info.eyebrow_aspect);
        core_data.set_eyebrow_rotate(char_info.eyebrow_rotate);
        core_data.set_eyebrow_x(char_info.eyebrow_x);
        core_data.set_eyebrow_y(char_info.eyebrow_y);
        core_data.set_nose_type(char_info.nose_type);
        core_data.set_nose_scale(char_info.nose_scale);
        core_data.set_nose_y(char_info.nose_y);
        core_data.set_mouth_type(char_info.mouth_type);
        core_data.set_mouth_color(char_info.mouth_color);
        core_data.set_mouth_scale(char_info.mouth_scale);
        core_data.set_mouth_aspect(char_info.mouth_aspect);
        core_data.set_mouth_y(char_info.mouth_y);
        core_data.set_beard_type(char_info.beard_type);
        core_data.set_beard_color(char_info.beard_color);
        core_data.set_mustache_type(char_info.mustache_type);
        core_data.set_mustache_scale(char_info.mustache_scale);
        core_data.set_mustache_y(char_info.mustache_y);
        core_data.set_glass_type(char_info.glass_type);
        core_data.set_glass_color(char_info.glass_color);
        core_data.set_glass_scale(char_info.glass_scale);
        core_data.set_glass_y(char_info.glass_y);
        core_data.set_mole_type(char_info.mole_type);
        core_data.set_mole_scale(char_info.mole_scale);
        core_data.set_mole_x(char_info.mole_x);
        core_data.set_mole_y(char_info.mole_y);

        Ok(core_data)
    }

    pub fn to_charinfo(&self) -> Result<CharInfo> {
        let mut charinfo: CharInfo = Default::default();

        charinfo.name.set_string(self.name.get_string()?);
        charinfo.font_region = self.get_font_region();
        charinfo.favorite_color = self.get_favorite_color();
        charinfo.gender = self.get_gender();
        charinfo.height = self.get_height();
        charinfo.build = self.get_build();
        charinfo.type_val = self.get_type();
        charinfo.region_move = self.get_region_move();
        charinfo.faceline_type = self.get_faceline_type();
        charinfo.faceline_color = self.get_faceline_color();
        charinfo.faceline_make = self.get_faceline_make();
        charinfo.hair_type = self.get_hair_type();
        charinfo.hair_color = self.get_hair_color();
        charinfo.hair_flip = self.get_hair_flip();
        charinfo.eye_type = self.get_eye_type();
        charinfo.eye_color = self.get_eye_color();
        charinfo.eye_scale = self.get_eye_scale();
        charinfo.eye_aspect = self.get_eye_aspect();
        charinfo.eye_rotate = self.get_eye_rotate();
        charinfo.eye_x = self.get_eye_x();
        charinfo.eye_y = self.get_eye_y();
        charinfo.eyebrow_type = self.get_eyebrow_type();
        charinfo.eyebrow_color = self.get_eyebrow_color();
        charinfo.eyebrow_scale = self.get_eyebrow_scale();
        charinfo.eyebrow_aspect = self.get_eyebrow_aspect();
        charinfo.eyebrow_rotate = self.get_eyebrow_rotate();
        charinfo.eyebrow_x = self.get_eyebrow_x();
        charinfo.eyebrow_y = self.get_eyebrow_y();
        charinfo.nose_type = self.get_nose_type();
        charinfo.nose_scale = self.get_nose_scale();
        charinfo.nose_y = self.get_nose_y();
        charinfo.mouth_type = self.get_mouth_type();
        charinfo.mouth_color = self.get_mouth_color();
        charinfo.mouth_scale = self.get_mouth_scale();
        charinfo.mouth_aspect = self.get_mouth_aspect();
        charinfo.mouth_y = self.get_mouth_y();
        charinfo.beard_type = self.get_beard_type();
        charinfo.beard_color = self.get_beard_color();
        charinfo.mustache_type = self.get_mustache_type();
        charinfo.mustache_scale = self.get_mustache_scale();
        charinfo.mustache_y = self.get_mustache_y();
        charinfo.glass_type = self.get_glass_type();
        charinfo.glass_color = self.get_glass_color();
        charinfo.glass_scale = self.get_glass_scale();
        charinfo.glass_y = self.get_glass_y();
        charinfo.mole_type = self.get_mole_type();
        charinfo.mole_scale = self.get_mouth_scale();
        charinfo.mole_x = self.get_mole_x();
        charinfo.mole_y = self.get_mole_y();

        Ok(charinfo)
    }
}

pub fn compute_crc16(buffer: &[u8], base_crc: u16, reverse_endianess: bool) -> u16 {
    let crc = buffer
        .iter()
        .copied()
        .map(u16::from)
        .fold(base_crc, |mut crc, val| {
            for _ in 0..8 {
                let apply_shift = (crc & 0x8000) != 0;
                crc <<= 1;

                if apply_shift {
                    crc ^= 0x1021;
                }
            }
            crc ^ val
        });

    if reverse_endianess {
        crc.swap_bytes()
    } else {
        crc
    }
}

#[cfg(feature = "services")]
#[inline]
pub fn get_device_id() -> Result<CreateId> {
    use crate::service::set::{ISystemSettingsClient, SystemSettingsService};

    service::new_service_object::<SystemSettingsService>()?.get_mii_author_id()
}

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct StoreData {
    pub core_data: CoreData,
    pub id: CreateId,
    pub data_crc: u16,
    pub device_crc: u16,
}
const_assert!(core::mem::size_of::<StoreData>() == 0x44);

/// Simple wrapper for interpretting structs as byte slices.
/// This flags in the linter, but is actually used
#[allow(unused_macros)]
macro_rules! struct_to_slice {
    ($val:expr) => {
        unsafe {
            core::slice::from_raw_parts($val as *const _ as *const u8, core::mem::size_of_val($val))
        }
    };
}

impl StoreData {
    #[cfg(feature = "services")]
    pub fn from_charinfo(char_info: CharInfo) -> Result<Self> {
        let mut store_data = Self {
            core_data: CoreData::from_charinfo(char_info)?,
            id: char_info.id,
            data_crc: 0,
            device_crc: 0,
        };

        store_data.data_crc = compute_crc16(
            &struct_to_slice!(&store_data)[..core::mem::offset_of!(StoreData, device_crc)],
            0,
            true,
        );

        let device_id = get_device_id()?;
        let base_device_crc = compute_crc16(struct_to_slice!(&device_id), 0, false);
        store_data.device_crc = compute_crc16(struct_to_slice!(&store_data), base_device_crc, true);

        Ok(store_data)
    }

    #[cfg(feature = "services")]
    pub fn is_valid(&self) -> bool {
        let new_data_crc = compute_crc16(
            &struct_to_slice!(self)[..core::mem::offset_of!(StoreData, device_crc)],
            0,
            false,
        );

        if let Ok(device_id) = get_device_id() {
            let base_new_device_crc = compute_crc16(struct_to_slice!(&device_id), 0, false);
            let new_device_crc = compute_crc16(struct_to_slice!(self), base_new_device_crc, false);

            (new_data_crc == 0) && (new_device_crc == 0)
        } else {
            false
        }
    }

    #[inline]
    pub fn to_charinfo(&self) -> Result<CharInfo> {
        let mut charinfo = self.core_data.to_charinfo()?;
        charinfo.id = self.id;
        Ok(charinfo)
    }
}

// TODO: fill this from emuiibo code

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct Ver3StoreData {
    pub data: [u8; 0x5C],
}
const_assert!(core::mem::size_of::<Ver3StoreData>() == 0x5C);

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct NfpStoreDataExtension {
    pub data: [u8; 0x8],
}
const_assert!(core::mem::size_of::<NfpStoreDataExtension>() == 0x8);

#[nx_derive::ipc_trait]
#[default_client]
pub trait MiiDatabase {
    #[ipc_rid(0)]
    fn is_updated(&self, flag: SourceFlag) -> bool;
    #[ipc_rid(1)]
    fn is_full(&self) -> bool;
    #[ipc_rid(2)]
    fn get_count(&self, flag: SourceFlag) -> u32;
    #[ipc_rid(4)]
    fn get_one(&self, flag: SourceFlag, out_char_infos: sf::OutMapAliasBuffer<'_, CharInfo>)
    -> u32;
    #[ipc_rid(6)]
    fn build_random(
        &self,
        age: sf::EnumAsPrimitiveType<Age, u32>,
        gender: sf::EnumAsPrimitiveType<Gender, u32>,
        race: sf::EnumAsPrimitiveType<FaceColor, u32>,
    ) -> CharInfo;
}

#[nx_derive::ipc_trait]
pub trait Static {
    #[ipc_rid(0)]
    #[return_session]
    fn get_database_service(&self, key_code: SpecialKeyCode) -> MiiDatabase;
}
